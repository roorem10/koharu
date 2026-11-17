//! Booharu's part of the app: its commands, which the Booharu block of
//! `commands` lists, and the host Booharu's agent tools reach the app
//! through.

pub(crate) mod settings;

use booharu_core::settings::BooharuPreferences;
use koharu_agent::BooharuHost;

/// The app as Booharu's agent tools reach it, the in-app agent's and the MCP
/// server's alike, through what the commands run.
pub(crate) struct DesktopHost;

impl BooharuHost for DesktopHost {
    fn preferences(&self) -> anyhow::Result<BooharuPreferences> {
        BooharuPreferences::get()
    }

    fn save_preferences(
        &self,
        preferences: BooharuPreferences,
    ) -> anyhow::Result<BooharuPreferences> {
        preferences.save()
    }
}
