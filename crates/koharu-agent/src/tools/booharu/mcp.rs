//! Booharu's part of the MCP server: an outside app's calls in flight, which
//! closing the project they went to drops, as closing it stops the in-app
//! agent's, so none keeps a closed project's session, and so its file's lock,
//! as a long one such as booharu_get_qc_findings on every page would until it
//! ended.

use std::{future::Future, sync::Arc};

use parking_lot::Mutex;
use rmcp::model::{CallToolResult, ContentBlock};
use tokio_util::sync::CancellationToken;

/// What a call the project's closing dropped answers: an edit it was
/// applying on a blocking thread may still land.
const CLOSED: &str =
    "the project closed before the call ended; a change it made may have been saved";

/// The MCP server's calls in flight, which the server and each MCP session's
/// handler share.
#[derive(Clone, Default)]
pub(crate) struct McpCalls(Arc<Mutex<CancellationToken>>);

impl McpCalls {
    /// Drops every call in flight, as the project they went to closes or
    /// another opens in its place; the calls made from then on go on.
    pub(crate) fn close(&self) {
        std::mem::take(&mut *self.0.lock()).cancel();
    }

    /// `call`'s answer, or, once the project closes first, an error saying
    /// so, `call` dropped with what it held of the project. What drops it is
    /// taken before `call` is first polled, which is when it reads the
    /// project open, so a project closed after that read drops it.
    pub(crate) async fn until_closed(
        &self,
        call: impl Future<Output = CallToolResult>,
    ) -> CallToolResult {
        let closed = self.0.lock().clone();
        tokio::select! {
            biased;
            answer = call => answer,
            () = closed.cancelled() => CallToolResult::error(vec![ContentBlock::text(CLOSED)]),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use booharu::context::read::ReadSummary;
    use booharu_core::settings::BooharuPreferences;
    use futures::future::BoxFuture;
    use koharu_pipeline::{Pipeline, PipelineConfig};
    use koharu_renderer::{Renderer, RendererConfig};
    use koharu_scene::Session;
    use serde_json::{Map, Value, json};
    use tokio::sync::mpsc;

    use super::*;
    use crate::{McpServer, Tools, tools::booharu::BooharuHost};

    /// The app, as far as a read of the chapter that never ends by itself:
    /// it holds the session it reads and tells `started` as it starts.
    struct Reading {
        started: mpsc::UnboundedSender<()>,
    }

    impl BooharuHost for Reading {
        fn preferences(&self) -> anyhow::Result<BooharuPreferences> {
            anyhow::bail!("nothing here reads settings")
        }

        fn save_section(
            &self,
            _: &str,
            _: Map<String, Value>,
        ) -> anyhow::Result<BooharuPreferences> {
            anyhow::bail!("nothing here saves settings")
        }

        fn font_families(&self) -> anyhow::Result<Vec<String>> {
            Ok(Vec::new())
        }

        fn read_chapter(
            &self,
            session: Arc<Mutex<Session>>,
            _: Option<String>,
        ) -> BoxFuture<'static, anyhow::Result<ReadSummary>> {
            let started = self.started.clone();
            Box::pin(async move {
                let _reads = session;
                let _ = started.send(());
                std::future::pending().await
            })
        }
    }

    /// An outside app's call, as it reaches Booharu's MCP server: its own
    /// requests, in the order an MCP client sends them.
    #[tokio::test]
    async fn closing_the_project_drops_an_outside_apps_call_and_lets_the_project_go() {
        let dir = tempfile::tempdir().expect("a folder");
        let session = Session::create(dir.path().join("Ch.1.khr")).expect("a project");
        let (started, mut reading) = mpsc::unbounded_channel();
        let tools = Tools::new(
            Arc::new(Mutex::new(session)),
            Pipeline::new(PipelineConfig::default()),
            Renderer::new(RendererConfig::default()),
        )
        .with_booharu(Arc::new(Reading { started }));
        let held = Arc::downgrade(&tools.session);
        let server = McpServer::start(0).await.expect("a server");
        server.open(Some(tools));
        let client = reqwest::Client::new();
        let url = format!("http://{}/mcp", server.address());
        let post = |session: Option<&str>, message: Value| {
            let request = client
                .post(&url)
                .header("Accept", "application/json, text/event-stream")
                .json(&message);
            match session {
                Some(session) => request.header("Mcp-Session-Id", session),
                None => request,
            }
            .send()
        };
        let initialize = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"},
        }});
        let started = post(None, initialize).await.expect("a session");
        let session = started.headers()["mcp-session-id"]
            .to_str()
            .expect("its id")
            .to_owned();
        let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        post(Some(&session), initialized)
            .await
            .expect("the session starts");

        let read = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
            "name": "booharu_read_chapter",
            "arguments": {},
        }});
        // The answer comes as a server-sent event.
        let answer = async {
            let answer = post(Some(&session), read).await.expect("an answer");
            answer.text().await.expect("events")
        };
        let close = async {
            reading.recv().await.expect("the read starts");
            server.open(None);
        };
        let both = async { tokio::join!(answer, close) };
        let (events, ()) = tokio::time::timeout(Duration::from_secs(30), both)
            .await
            .expect("the call answers once its project closes");
        let result = events
            .lines()
            .find_map(|line| {
                let message = serde_json::from_str::<Value>(line.strip_prefix("data:")?).ok()?;
                (message["id"] == json!(2)).then(|| message["result"].clone())
            })
            .unwrap_or_else(|| panic!("no answer in {events}"));
        assert_eq!(result["isError"], json!(true), "{result}");
        assert_eq!(result["content"][0]["text"], json!(CLOSED), "{result}");
        assert!(
            held.upgrade().is_none(),
            "the dropped call holds the session"
        );
    }
}
