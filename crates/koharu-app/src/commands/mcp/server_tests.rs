use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use koharu_agent::Tool as AgentTool;
use serde_json::Value;
use tokio::sync::Notify;

use super::*;

#[test]
fn bearer_token_is_required_and_compared_in_full() {
    let mut headers = HeaderMap::new();
    assert!(!authorized(&headers, "secret"));
    for supplied in [
        "Bearer wrong",
        "Basic secret",
        "Bearer secret ",
        "Bearer secre",
    ] {
        headers.insert(header::AUTHORIZATION, supplied.parse().unwrap());
        assert!(!authorized(&headers, "secret"));
    }
    headers.insert(header::AUTHORIZATION, "Bearer secret".parse().unwrap());
    assert!(authorized(&headers, "secret"));
    assert!(!authorized(&headers, ""));
    headers.append(header::AUTHORIZATION, "Bearer secret".parse().unwrap());
    assert!(!authorized(&headers, "secret"));
}

#[test]
fn origins_must_be_null_or_local_without_credentials() {
    let mut headers = HeaderMap::new();
    assert!(allowed_origin(&headers));
    for origin in [
        "null",
        "http://localhost:3000",
        "https://localhost",
        "http://127.0.0.1:47821",
        "http://[::1]:47821",
    ] {
        headers.insert(header::ORIGIN, origin.parse().unwrap());
        assert!(allowed_origin(&headers), "{origin}");
    }
    for origin in [
        "https://example.com",
        "http://localhost.example.com",
        "http://localhost@evil.com",
        "http://user@localhost",
        "file://localhost",
        "http://localhost/path",
        "http://localhost?x",
        "http://localhost#x",
        "garbage",
    ] {
        headers.insert(header::ORIGIN, origin.parse().unwrap());
        assert!(!allowed_origin(&headers), "{origin}");
    }
    headers.insert(header::ORIGIN, "null".parse().unwrap());
    headers.append(header::ORIGIN, "null".parse().unwrap());
    assert!(!allowed_origin(&headers));
}

#[test]
fn invocation_keeps_json_and_image_content_separate() {
    let invocation = Invocation::read(json!({ "page": 1 }))
        .unwrap()
        .with_image("page", "data:image/webp;base64,aW1hZ2U=");
    let result = serde_json::to_value(invocation_result(Ok(invocation))).unwrap();
    assert_eq!(result["isError"], false);
    assert_eq!(result["content"][0]["text"], "{\n  \"page\": 1\n}");
    assert_eq!(
        result["content"][1],
        json!({ "type": "image", "mimeType": "image/webp", "data": "aW1hZ2U=" })
    );
    let error =
        serde_json::to_value(invocation_result(Err(anyhow!("no project is open")))).unwrap();
    assert_eq!(error["isError"], true);
    assert_eq!(error["content"][0]["text"], "no project is open");
    assert!(error.get("error").is_none());
    let invalid = Invocation::read(json!({}))
        .unwrap()
        .with_image("bad", "data:image/webp;base64,%%%");
    assert_eq!(
        serde_json::to_value(invocation_result(Ok(invalid))).unwrap()["isError"],
        true
    );
}

#[derive(Default)]
struct FakeHost {
    started: Notify,
    controls: SyncMutex<Vec<Control>>,
    edits: AtomicUsize,
}

#[async_trait]
impl Host for FakeHost {
    async fn context(&self) -> Result<Value> {
        Ok(json!({ "project": "test" }))
    }

    fn tools(&self) -> Vec<AgentTool> {
        [
            "inspect_project",
            "view_page",
            "set_translation",
            "run_pipeline",
            "fails",
        ]
        .into_iter()
        .map(|name| {
            AgentTool::new(
                name,
                format!("Description of {name}"),
                json!({ "type": "object", "properties": {} }),
            )
        })
        .collect()
    }

    async fn invoke(&self, call: ToolCall, control: &Control) -> Result<Invocation> {
        match call.name.as_str() {
            "run_pipeline" => {
                self.controls.lock().push(control.clone());
                self.started.notify_one();
                control.cancelled().await;
                Err(anyhow!("pipeline processing was cancelled"))
            }
            "set_translation" => {
                self.edits.fetch_add(1, Ordering::SeqCst);
                Invocation::changed(json!({ "changed": true }))
            }
            "fails" => Err(anyhow!("no project is open")),
            "view_page" => Ok(Invocation::read(json!({ "page": 1 }))?
                .with_image("page", "data:image/webp;base64,aW1hZ2U=")),
            _ => Invocation::read(
                json!({ "arguments": serde_json::from_str::<Value>(&call.arguments)? }),
            ),
        }
    }
}

fn bridge(host: Arc<dyn Host>, edits: bool) -> Arc<Bridge> {
    Bridge::new(
        host,
        "test-token".into(),
        edits,
        "test-version".into(),
        Arc::new(Mutex::new(())),
    )
}

#[test]
fn read_only_list_preserves_the_host_schemas_and_descriptions() {
    let host = Arc::new(FakeHost::default());
    let bridge = bridge(host.clone(), false);
    let tools = json!({ "tools": bridge.tools().unwrap() });
    let names: Vec<_> = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, READ_TOOLS);
    assert_eq!(tools["tools"][0]["inputSchema"], host.tools()[0].parameters);
    assert_eq!(
        tools["tools"][0]["description"],
        host.tools()[0].description
    );
}

struct Server {
    endpoint: String,
    client: reqwest::Client,
    stop: Control,
    task: Option<tokio::task::JoinHandle<Result<()>>>,
}

impl Server {
    async fn start(bridge: Arc<Bridge>) -> Self {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
        let stop = Control::default();
        Self {
            endpoint,
            client: reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
            task: Some(tokio::spawn(bridge.serve(
                listener,
                stop.clone(),
                Control::default(),
            ))),
            stop,
        }
    }

    fn post(&self, session: Option<&str>, message: Value) -> reqwest::RequestBuilder {
        let request = self
            .client
            .post(&self.endpoint)
            .bearer_auth("test-token")
            .header("Accept", "application/json, text/event-stream")
            .json(&message);
        match session {
            Some(session) => request.header("Mcp-Session-Id", session),
            None => request,
        }
    }

    async fn initialize(&self) -> String {
        let initialized = self.post(None, json!({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": { "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } },
        })).send().await.unwrap();
        let session = initialized.headers()["mcp-session-id"]
            .to_str()
            .unwrap()
            .to_owned();
        let result = rpc(initialized).await;
        assert_eq!(result["result"]["serverInfo"]["name"], "booharu");
        assert_eq!(result["result"]["serverInfo"]["version"], "test-version");
        assert_eq!(
            self.post(
                Some(&session),
                json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })
            )
            .send()
            .await
            .unwrap()
            .status(),
            StatusCode::ACCEPTED
        );
        session
    }

    async fn call(&self, session: &str, id: i32, name: &str) -> reqwest::Response {
        self.post(Some(session), json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": { "page": 1 } } }))
            .send().await.unwrap()
    }

    async fn cancel(&self, session: &str, id: i32) {
        assert_eq!(self.post(Some(session), json!({ "jsonrpc": "2.0", "method": "notifications/cancelled", "params": { "requestId": id } }))
            .send().await.unwrap().status(), StatusCode::ACCEPTED);
    }

    async fn stop(mut self) {
        self.stop.cancel();
        tokio::time::timeout(Duration::from_secs(3), self.task.take().unwrap())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

async fn rpc(response: reqwest::Response) -> Value {
    let status = response.status();
    let content = response.text().await.unwrap();
    let json = content
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .find(|data| !data.trim().is_empty())
        .unwrap_or(&content);
    serde_json::from_str(json)
        .unwrap_or_else(|error| panic!("Invalid MCP response ({status}): {content:?}: {error}"))
}

#[tokio::test]
async fn http_initialization_tools_images_errors_and_session_deletion() {
    let server = Server::start(bridge(Arc::new(FakeHost::default()), true)).await;
    assert_eq!(
        server
            .client
            .post(&server.endpoint)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .client
            .get(&server.endpoint)
            .bearer_auth("wrong")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .post(None, json!({}))
            .header("Origin", "https://example.com")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let session = server.initialize().await;
    let list = server
        .post(
            Some(&session),
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(
        rpc(list).await["result"]["tools"].as_array().unwrap().len(),
        6
    );
    let inspected = rpc(server.call(&session, 2, "inspect_project").await).await;
    let value: Value =
        serde_json::from_str(inspected["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(value["arguments"]["page"], 1);
    assert_eq!(
        rpc(server.call(&session, 3, "view_page").await).await["result"]["content"][1]["mimeType"],
        "image/webp"
    );
    let summary = rpc(server.call(&session, 4, "get_project_summary").await).await;
    assert!(
        summary["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("test")
    );
    let failed = rpc(server.call(&session, 5, "fails").await).await;
    assert_eq!(failed["result"]["isError"], true);
    assert!(failed.get("error").is_none());
    assert!(
        server
            .client
            .delete(&server.endpoint)
            .bearer_auth("test-token")
            .header("Mcp-Session-Id", &session)
            .send()
            .await
            .unwrap()
            .status()
            .is_success()
    );
    assert_eq!(
        server.call(&session, 6, "inspect_project").await.status(),
        StatusCode::NOT_FOUND
    );
    server.stop().await;
}

#[tokio::test]
async fn edits_are_refused_even_when_called_directly() {
    let host = Arc::new(FakeHost::default());
    let server = Server::start(bridge(host.clone(), false)).await;
    let session = server.initialize().await;
    assert_eq!(
        rpc(server.call(&session, 1, "set_translation").await).await["result"]["isError"],
        true
    );
    assert_eq!(host.edits.load(Ordering::SeqCst), 0);
    server.stop().await;
}

#[tokio::test]
async fn queued_calls_cancel_without_invoking_and_ids_are_session_scoped() {
    let host = Arc::new(FakeHost::default());
    let server = Server::start(bridge(host.clone(), true)).await;
    let first = server.initialize().await;
    let second = server.initialize().await;
    let active = server.call(&first, 1, "run_pipeline").await;
    tokio::time::timeout(Duration::from_secs(2), host.started.notified())
        .await
        .unwrap();
    let queued = server.call(&second, 1, "set_translation").await;
    server.cancel(&second, 1).await;
    // MCP clients discard cancelled responses; verify their effects instead.
    queued.text().await.unwrap();
    assert!(!host.controls.lock()[0].is_cancelled());
    assert_eq!(host.edits.load(Ordering::SeqCst), 0);
    server.cancel(&first, 1).await;
    active.text().await.unwrap();
    assert!(host.controls.lock()[0].is_cancelled());
    assert_eq!(
        rpc(server.call(&first, 2, "set_translation").await).await["result"]["isError"],
        false
    );
    assert_eq!(host.edits.load(Ordering::SeqCst), 1);
    server.stop().await;
}

#[tokio::test]
async fn disconnect_cancels_the_control_and_releases_the_call_lane() {
    let host = Arc::new(FakeHost::default());
    let server = Server::start(bridge(host.clone(), true)).await;
    let session = server.initialize().await;
    let response = server.call(&session, 1, "run_pipeline").await;
    tokio::time::timeout(Duration::from_secs(2), host.started.notified())
        .await
        .unwrap();
    let control = host.controls.lock()[0].clone();
    drop(response);
    tokio::time::timeout(Duration::from_secs(8), control.cancelled())
        .await
        .unwrap();
    assert_eq!(
        rpc(server.call(&session, 2, "set_translation").await).await["result"]["isError"],
        false
    );
    server.stop().await;
}
