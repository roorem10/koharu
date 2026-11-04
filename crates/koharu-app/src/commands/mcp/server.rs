use std::{
    convert::Infallible,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use anyhow::{Context as _, Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use bytes::Bytes;
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use hyper::{
    Request, Response, StatusCode,
    body::{Body, Frame, Incoming},
    header::{self, HeaderMap},
    server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use koharu_agent::{Control, Host, Invocation, ToolCall};
use parking_lot::Mutex as SyncMutex;
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
        ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde_json::json;
use tokio::{net::TcpListener, sync::Mutex, task::JoinSet};
use tokio_util::sync::CancellationToken;
use url::Url;

pub(super) const READ_TOOLS: &[&str] = &["inspect_project", "view_page", "get_project_summary"];
type HttpResponse = Response<BoxBody<Bytes, Infallible>>;

pub(super) struct Bridge {
    host: Arc<dyn Host>,
    token: String,
    allow_edits: bool,
    version: String,
    gate: Arc<Mutex<()>>,
    tasks: SyncMutex<JoinSet<()>>,
    stopping: Control,
}

impl Bridge {
    pub(super) fn new(
        host: Arc<dyn Host>,
        token: String,
        allow_edits: bool,
        version: String,
        gate: Arc<Mutex<()>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            host,
            token,
            allow_edits,
            version,
            gate,
            tasks: SyncMutex::new(JoinSet::new()),
            stopping: Control::default(),
        })
    }

    pub(super) async fn serve(
        self: Arc<Self>,
        listener: TcpListener,
        stop: Control,
        application_stop: Control,
    ) -> Result<()> {
        let cancellation = CancellationToken::new();
        let config = StreamableHttpServerConfig::default()
            .with_cancellation_token(cancellation.clone())
            .with_sse_keep_alive(Some(Duration::from_secs(5)))
            .with_max_request_body_bytes(1024 * 1024);
        let bridge = self.clone();
        let service = StreamableHttpService::new(
            move || {
                Ok(McpHandler {
                    bridge: bridge.clone(),
                })
            },
            Arc::new(LocalSessionManager::default()),
            config,
        );
        let mut connections = JoinSet::new();
        let result = loop {
            tokio::select! {
                biased;
                _ = stop.cancelled() => break Ok(()),
                _ = application_stop.cancelled() => break Ok(()),
                Some(_) = connections.join_next(), if !connections.is_empty() => {},
                accepted = listener.accept() => {
                    let (stream, _) = match accepted {
                        Ok(accepted) => accepted,
                        Err(error) => break Err(error.into()),
                    };
                    let bridge = self.clone();
                    let service = service.clone();
                    connections.spawn(async move {
                        let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), service_fn(move |request| {
                            let bridge = bridge.clone();
                            let service = service.clone();
                            async move { Ok::<_, Infallible>(bridge.handle(request, &service).await) }
                        })).await;
                    });
                }
            }
        };
        self.stopping.cancel();
        cancellation.cancel();
        connections.abort_all();
        while connections.join_next().await.is_some() {}
        // Keep the invocation lane until cooperative pipeline cleanup has finished.
        let mut tasks = std::mem::take(&mut *self.tasks.lock());
        while tasks.join_next().await.is_some() {}
        result
    }

    async fn handle(
        self: &Arc<Self>,
        mut request: Request<Incoming>,
        service: &StreamableHttpService<McpHandler, LocalSessionManager>,
    ) -> HttpResponse {
        if !authorized(request.headers(), &self.token) {
            let mut response = response(StatusCode::UNAUTHORIZED, "Unauthorized");
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                header::HeaderValue::from_static("Bearer"),
            );
            return response;
        }
        if !allowed_origin(request.headers()) {
            return response(StatusCode::FORBIDDEN, "Origin is not allowed");
        }
        if request.uri().path() != "/mcp" {
            return response(StatusCode::NOT_FOUND, "Not found");
        }
        if self.stopping.is_cancelled() {
            return response(StatusCode::SERVICE_UNAVAILABLE, "Server is stopping");
        }
        if let Some(authorization) = request.headers_mut().get_mut(header::AUTHORIZATION) {
            authorization.set_sensitive(true);
        }
        let control = Control::default();
        request
            .extensions_mut()
            .insert(HttpControl(control.clone()));
        let mut pending_cancel = CancelOnDrop(Some(control.clone()));
        let mut response = service
            .handle(request)
            .await
            .map(|inner| DisconnectBody { inner, control }.boxed());
        pending_cancel.0.take();
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-store"),
        );
        response
    }

    fn tools(&self) -> Result<Vec<Tool>> {
        let mut tools = self
            .host
            .tools()
            .into_iter()
            .filter(|tool| self.allow_edits || READ_TOOLS.contains(&tool.name.as_str()))
            .map(|tool| {
                let schema = tool
                    .parameters
                    .as_object()
                    .cloned()
                    .context("Built-in tool schema is not an object")?;
                Ok(Tool::new(tool.name, tool.description, schema))
            })
            .collect::<Result<Vec<_>>>()?;
        tools.push(Tool::new(
            "get_project_summary",
            "Read the open project's summary and context.",
            json!({ "type": "object", "properties": {}, "additionalProperties": false })
                .as_object()
                .unwrap()
                .clone(),
        ));
        Ok(tools)
    }

    fn invoke(
        self: &Arc<Self>,
        call: ToolCall,
        control: Control,
    ) -> tokio::sync::oneshot::Receiver<CallToolResult> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let bridge = self.clone();
        let mut tasks = self.tasks.lock();
        while tasks.try_join_next().is_some() {}
        tasks.spawn(async move {
            let invoke = async {
                let _guard = tokio::select! {
                    biased;
                    _ = bridge.stopping.cancelled() => return Err(anyhow!("MCP server is stopping")),
                    _ = control.cancelled() => return Err(anyhow!("Tool call cancelled")),
                    guard = bridge.gate.lock() => guard,
                };
                if control.is_cancelled() || bridge.stopping.is_cancelled() {
                    return Err(anyhow!("Tool call cancelled"));
                }
                if call.name == "get_project_summary" {
                    Invocation::read(bridge.host.context().await?)
                } else {
                    bridge.host.invoke(call, &control).await
                }
            };
            tokio::pin!(invoke);
            let result = tokio::select! {
                biased;
                _ = bridge.stopping.cancelled() => { control.cancel(); invoke.await },
                result = &mut invoke => result,
            };
            let _ = sender.send(invocation_result(result));
        });
        receiver
    }
}

#[derive(Clone)]
struct McpHandler {
    bridge: Arc<Bridge>,
}

impl ServerHandler for McpHandler {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("booharu", self.bridge.version.clone()))
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = self
            .bridge
            .tools()
            .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if !self.bridge.allow_edits && !READ_TOOLS.contains(&request.name.as_ref()) {
            return Ok(tool_error("Edits are disabled in Booharu's MCP settings").into());
        }
        if request.name != "get_project_summary"
            && !self
                .bridge
                .host
                .tools()
                .iter()
                .any(|tool| tool.name == request.name)
        {
            return Ok(tool_error("Unknown tool").into());
        }
        let control = context
            .extensions
            .get::<http::request::Parts>()
            .and_then(|parts| parts.extensions.get::<HttpControl>())
            .map(|control| control.0.clone())
            .unwrap_or_default();
        let _cancel_on_drop = CancelOnDrop(Some(control.clone()));
        let mut pending = self.bridge.invoke(
            ToolCall {
                call_id: context.id.to_string(),
                name: request.name.to_string(),
                arguments: serde_json::to_string(&request.arguments.unwrap_or_default())
                    .expect("JSON objects serialize"),
            },
            control.clone(),
        );
        let result = tokio::select! {
            result = &mut pending => result,
            _ = context.ct.cancelled() => { control.cancel(); pending.await }
        };
        Ok(result
            .unwrap_or_else(|_| tool_error("Tool invocation stopped unexpectedly"))
            .into())
    }
}

#[derive(Clone)]
struct HttpControl(Control);

struct CancelOnDrop(Option<Control>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(control) = &self.0 {
            control.cancel();
        }
    }
}

// Legacy SDK sessions preserve dropped streams for resumption. Booharu instead
// cancels the call's Control when its HTTP response disappears.
struct DisconnectBody {
    inner: BoxBody<Bytes, Infallible>,
    control: Control,
}
impl Body for DisconnectBody {
    type Data = Bytes;
    type Error = Infallible;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        Pin::new(&mut self.inner).poll_frame(context)
    }
}
impl Drop for DisconnectBody {
    fn drop(&mut self) {
        self.control.cancel();
    }
}

fn authorized(headers: &HeaderMap, token: &str) -> bool {
    if token.is_empty() || headers.get_all(header::AUTHORIZATION).iter().count() != 1 {
        return false;
    }
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let Some((scheme, supplied)) = value.split_once(' ') else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("Bearer") || supplied.len() != token.len() {
        return false;
    }
    supplied
        .bytes()
        .zip(token.bytes())
        .fold(0u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

fn allowed_origin(headers: &HeaderMap) -> bool {
    if headers.get_all(header::ORIGIN).iter().count() > 1 {
        return false;
    }
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    if origin == "null" {
        return true;
    }
    let Ok(url) = Url::parse(origin) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https")
        && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
        && url.username().is_empty()
        && url.password().is_none()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
}

fn invocation_result(result: Result<Invocation>) -> CallToolResult {
    let result = (|| -> Result<_> {
        let invocation = result?;
        let mut content = vec![ContentBlock::text(serde_json::to_string_pretty(
            &invocation.value,
        )?)];
        for image in invocation.images {
            let (metadata, data) = image
                .data_url
                .split_once(',')
                .ok_or_else(|| anyhow!("Invalid tool image data URL"))?;
            let mime = metadata
                .strip_prefix("data:")
                .and_then(|value| value.strip_suffix(";base64"))
                .filter(|mime| mime.starts_with("image/"))
                .ok_or_else(|| anyhow!("Invalid tool image MIME type"))?;
            STANDARD
                .decode(data)
                .map_err(|_| anyhow!("Invalid tool image base64 data"))?;
            content.push(ContentBlock::image(data, mime));
        }
        Ok(CallToolResult::success(content))
    })();
    result.unwrap_or_else(|error| tool_error(&format!("{error:#}")))
}

fn tool_error(message: &str) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message)])
}

fn response(status: StatusCode, text: &str) -> HttpResponse {
    let mut response = Response::new(Full::new(Bytes::copy_from_slice(text.as_bytes())).boxed());
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
