use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow, ensure};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use koharu_agent::{Control, Host};
use koharu_config::Config;
use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::{net::TcpListener, sync::Mutex, task::JoinHandle};

use super::server;

#[derive(Clone, Deserialize, Serialize, Type)]
#[serde(default)]
pub(crate) struct McpSettings {
    pub enabled: bool,
    pub port: u16,
    pub token: String,
    pub allow_edits: bool,
}

impl Default for McpSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 47821,
            token: String::new(),
            allow_edits: true,
        }
    }
}

// Tokens are server-owned, so a stale preferences form cannot undo a rotation.
#[derive(Deserialize, Type)]
pub(crate) struct McpOptions {
    pub enabled: bool,
    pub port: u16,
    pub allow_edits: bool,
}

#[derive(Serialize, Type)]
pub(crate) struct McpStatus {
    pub settings: McpSettings,
    pub running: bool,
    pub endpoint: String,
    pub error: Option<String>,
}

pub(crate) struct McpState {
    config: Config<McpSettings>,
    host: Arc<dyn Host>,
    version: String,
    server: Mutex<ServerState>,
    gate: Arc<Mutex<()>>,
    application_stop: Control,
}

#[derive(Default)]
struct ServerState {
    running: Option<Running>,
    error: Option<String>,
}

struct Running {
    stop: Control,
    task: JoinHandle<Result<()>>,
}

impl McpState {
    pub(crate) fn new(host: Arc<dyn Host>, version: String) -> Result<Self> {
        Self::with_config(koharu_config::load::<McpSettings>("mcp")?, host, version)
    }

    fn with_config(
        config: Config<McpSettings>,
        host: Arc<dyn Host>,
        version: String,
    ) -> Result<Self> {
        if config.read()?.token.is_empty() {
            let mut settings = config.write()?;
            settings.token = generate_token()?;
            settings.save()?;
        }
        Ok(Self {
            config,
            host,
            version,
            server: Mutex::new(ServerState::default()),
            gate: Arc::new(Mutex::new(())),
            application_stop: Control::default(),
        })
    }

    pub(crate) fn cancel(&self) {
        self.application_stop.cancel();
    }

    pub(crate) async fn start(&self) {
        let mut server = self.server.lock().await;
        if server.running.is_some() || server.error.is_some() {
            return;
        }
        // Read inside this lock so a settings change cannot race startup.
        match self.config.read().map(|settings| settings.clone()) {
            Ok(settings) => self.restart(&mut server, &settings).await,
            Err(error) => server.error = Some(format!("{error:#}")),
        }
    }

    async fn restart(&self, server: &mut ServerState, settings: &McpSettings) {
        if let Some(running) = server.running.take() {
            running.stop.cancel();
            let _ = running.task.await;
        }
        server.error = None;
        if !settings.enabled || self.application_stop.is_cancelled() {
            return;
        }
        if settings.port == 0 {
            server.error = Some("MCP port must be between 1 and 65535".into());
            return;
        }
        let listener = match TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, settings.port)).await
        {
            Ok(listener) => listener,
            Err(error) => {
                server.error = Some(format!(
                    "Could not listen on 127.0.0.1:{}: {error}",
                    settings.port
                ));
                return;
            }
        };
        let bridge = server::Bridge::new(
            self.host.clone(),
            settings.token.clone(),
            settings.allow_edits,
            self.version.clone(),
            self.gate.clone(),
        );
        let stop = Control::default();
        server.running = Some(Running {
            task: tokio::spawn(bridge.serve(listener, stop.clone(), self.application_stop.clone())),
            stop,
        });
    }

    pub(super) async fn status(&self) -> Result<McpStatus> {
        let mut server = self.server.lock().await;
        if server
            .running
            .as_ref()
            .is_some_and(|running| running.task.is_finished())
        {
            let running = server.running.take().expect("finished server is present");
            server.error = match running.task.await {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(format!("MCP server stopped: {error:#}")),
                Err(_) => Some("MCP server task stopped unexpectedly".into()),
            };
        }
        let settings = self.config.read()?.clone();
        Ok(Self::describe(&server, settings))
    }

    fn describe(server: &ServerState, settings: McpSettings) -> McpStatus {
        McpStatus {
            endpoint: format!("http://127.0.0.1:{}/mcp", settings.port),
            settings,
            running: server.running.is_some(),
            error: server.error.clone(),
        }
    }

    pub(super) async fn update(
        &self,
        options: Option<McpOptions>,
        rotate: bool,
    ) -> Result<McpStatus> {
        let mut server = self.server.lock().await;
        let previous = self.config.read()?.clone();
        let mut settings = previous.clone();
        if let Some(options) = options {
            ensure!(options.port > 0, "MCP port must be between 1 and 65535");
            settings.enabled = options.enabled;
            settings.port = options.port;
            settings.allow_edits = options.allow_edits;
        }
        if rotate {
            settings.token = generate_token()?;
        }
        // End the synchronous configuration guard before awaiting the restart.
        {
            let mut write = self.config.write()?;
            *write = settings.clone();
            if let Err(error) = write.save() {
                *self.config.write()? = previous;
                return Err(error).context("Could not save MCP settings");
            }
        }
        self.restart(&mut server, &settings).await;
        Ok(Self::describe(&server, settings))
    }
}

fn generate_token() -> Result<String> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| anyhow!("Could not generate MCP token: {error}"))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeHost;

    #[async_trait::async_trait]
    impl Host for FakeHost {
        async fn context(&self) -> Result<serde_json::Value> {
            Err(anyhow!("no project is open"))
        }

        fn tools(&self) -> Vec<koharu_agent::Tool> {
            Vec::new()
        }

        async fn invoke(
            &self,
            _: koharu_agent::ToolCall,
            _: &Control,
        ) -> Result<koharu_agent::Invocation> {
            Err(anyhow!("no project is open"))
        }
    }

    #[test]
    fn default_settings_stay_off_and_tokens_contain_32_random_bytes() {
        let defaults: McpSettings = serde_json::from_str("{}").unwrap();
        assert!(!defaults.enabled);
        assert_eq!(defaults.port, 47821);
        assert!(defaults.allow_edits);
        let first = generate_token().unwrap();
        assert_eq!(URL_SAFE_NO_PAD.decode(&first).unwrap().len(), 32);
        assert_eq!(first.len(), 43);
        assert_ne!(first, generate_token().unwrap());
    }

    #[tokio::test]
    async fn live_settings_rotate_tokens_stop_and_report_the_requested_port_conflict() {
        let reserved = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = reserved.local_addr().unwrap().port();
        drop(reserved);
        let config = Config::memory(McpSettings {
            port,
            ..McpSettings::default()
        });
        let state =
            McpState::with_config(config.clone(), Arc::new(FakeHost), "test".into()).unwrap();
        let first_token = config.read().unwrap().token.clone();
        assert_eq!(first_token.len(), 43);
        state.start().await;
        assert!(!state.status().await.unwrap().running);
        let options = |enabled, port| McpOptions {
            enabled,
            port,
            allow_edits: true,
        };
        let running = state
            .update(Some(options(true, port)), false)
            .await
            .unwrap();
        assert!(running.running);
        assert!(running.error.is_none());
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .unwrap();
        let rotated = state.update(None, true).await.unwrap();
        assert!(rotated.running);
        assert_ne!(rotated.settings.token, first_token);
        let old = client
            .post(&rotated.endpoint)
            .bearer_auth(first_token)
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(old.status(), 401);
        let new = client
            .post(&rotated.endpoint)
            .bearer_auth(rotated.settings.token)
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap();
        assert_ne!(new.status(), 401);
        assert!(state.update(Some(options(true, 0)), false).await.is_err());
        assert!(state.status().await.unwrap().running);
        assert_eq!(state.status().await.unwrap().settings.port, port);
        let busy = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let busy_port = busy.local_addr().unwrap().port();
        let conflict = state
            .update(Some(options(true, busy_port)), false)
            .await
            .unwrap();
        assert!(!conflict.running);
        assert!(conflict.error.is_some());
        assert_eq!(conflict.settings.port, busy_port);
        let off = state
            .update(Some(options(false, busy_port)), false)
            .await
            .unwrap();
        assert!(!off.running);
        assert!(off.error.is_none());
        // Startup that happens after a user switch uses the newest settings.
        state.start().await;
        assert!(!state.status().await.unwrap().running);
    }
}
