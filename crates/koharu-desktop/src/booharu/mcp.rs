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
    /// It doesn't listen, for `error`: the settings file can't be read, as
    /// when it isn't TOML, or the port is taken. A stored port that doesn't
    /// read reads as its default, 47821.
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
    async fn a_bad_stored_port_reads_as_booharus_own_and_a_file_that_isnt_toml_starts_no_server() {
        // A port below 1024, edited into the file, reads as Booharu's own,
        // never as itself or Koharu's 7878.
        let stored = koharu_config::booharu_read_section::<McpSettings>("[booharu-mcp]\nport = 80");
        assert_eq!(stored.ok().map(|mcp| mcp.port), Some(47821));
        let unreadable = koharu_config::booharu_read_section::<McpSettings>("[booharu-mcp");
        let (server, status) = listen(unreadable.map_err(Into::into)).await;
        assert!(server.is_err());
        let McpStatus::Failed { error } = status else {
            panic!("{status:?}");
        };
        assert!(error.contains("config.toml is invalid"), "{error}");
    }
}
