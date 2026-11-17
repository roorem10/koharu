//! Booharu's MCP server: it listens on the port `[booharu-mcp]` sets, read at
//! launch, so Booharu serves beside a Koharu on Koharu's own, and how its
//! start went is kept for Booharu's settings to show.

use std::sync::OnceLock;

use anyhow::{Context as _, Result};
use booharu_core::settings::McpSettings;
use koharu_agent::McpServer;
use serde::Serialize;

/// How the server's start at launch went, once it was tried.
static STATUS: OnceLock<McpStatus> = OnceLock::new();

/// How the MCP server's start went.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum McpStatus {
    /// It listens on 127.0.0.1 at `port`.
    Listening { port: u16 },
    /// It doesn't listen, for `error`: the setting is bad, which fails
    /// closed, or the port is taken.
    Failed { error: String },
}

/// Starts the server on the port `[booharu-mcp]` sets, and remembers how
/// that went. The app runs on without it when it fails.
pub(crate) async fn start() -> Result<McpServer> {
    let (server, status) = listen(koharu_config::get().map_err(Into::into)).await;
    let _ = STATUS.set(status);
    server
}

/// How the server's start went, or `None` before it was tried.
#[expect(dead_code, reason = "Booharu's settings show it from a later stage")]
pub(crate) fn status() -> Option<&'static McpStatus> {
    STATUS.get()
}

/// The server on the port `settings` sets, or why there's none, and the
/// status that tells it.
async fn listen(settings: Result<McpSettings>) -> (Result<McpServer>, McpStatus) {
    let server = match settings {
        Ok(McpSettings { port }) => McpServer::start(port)
            .await
            .with_context(|| format!("127.0.0.1:{port}")),
        Err(error) => Err(error),
    };
    let status = match &server {
        Ok(server) => McpStatus::Listening {
            port: server.address().port(),
        },
        Err(error) => McpStatus::Failed {
            error: format!("{error:#}"),
        },
    };
    (server, status)
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, TcpListener};

    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn the_server_listens_on_the_settings_port_unless_it_is_taken() {
        let taken = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a free port");
        let port = taken.local_addr().expect("its address").port();

        let (server, status) = listen(Ok(McpSettings { port })).await;
        assert!(server.is_err());
        let McpStatus::Failed { error } = status else {
            panic!("{status:?}");
        };
        assert!(error.contains(&format!("127.0.0.1:{port}: ")), "{error}");

        drop(taken);
        let (server, status) = listen(Ok(McpSettings { port })).await;
        let server = server.expect("a server on the freed port");
        assert_eq!(server.address().port(), port);
        assert_eq!(status, McpStatus::Listening { port });
    }

    #[tokio::test]
    async fn a_bad_stored_port_starts_no_server() {
        let settings = serde_json::from_value::<McpSettings>(json!({"port": 80}));
        let (server, status) = listen(settings.map_err(Into::into)).await;
        assert!(server.is_err());
        let McpStatus::Failed { error } = status else {
            panic!("{status:?}");
        };
        assert!(error.contains("port 80 isn't"), "{error}");
    }
}
