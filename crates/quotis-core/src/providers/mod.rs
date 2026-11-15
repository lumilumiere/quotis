//! Provider adapters. To add a provider, implement `Provider` in a new file and add it to
//! `registry()`. Nothing else, including the UI, needs to change.

use crate::model::{Collected, ProviderError};
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod apikey;
mod claude;
mod codex;
mod copilot;
mod cursor;
mod opencode;
mod tokenlog;

/// Static facts about a provider, shown in Settings and `quotis-cli providers`.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct Info {
    pub id: &'static str,
    pub name: &'static str,
    pub vendor: &'static str,
    /// Short colour hint for the UI (CSS colour).
    pub color: &'static str,
    /// What the user needs to do, and what they get.
    pub hint: &'static str,
    /// Host contacted by `collect_online`, if any.
    pub endpoint: Option<&'static str>,
    /// Whether that endpoint is publicly documented by the vendor.
    pub documented: bool,
    /// Environment variable holding the API key, for providers that have no local login.
    pub key_env: Option<&'static str>,
    pub default_enabled: bool,
}

pub trait Provider: Send + Sync {
    fn info(&self) -> &'static Info;

    /// The tool's data folder when the user has not set one.
    fn default_dir(&self) -> PathBuf {
        PathBuf::new()
    }

    /// Cheap check that the tool is installed or signed in. Must not touch the network.
    fn detect(&self, dir: &Path) -> bool;

    /// Folder to watch for changes that should trigger `collect_local`.
    fn watch_dir(&self, _dir: &Path) -> Option<PathBuf> {
        None
    }

    /// Reads local files only. `None` means this provider has no local source.
    fn collect_local(&self, _dir: &Path, _now: i64) -> Option<Result<Collected, ProviderError>> {
        None
    }

    /// One bounded network request to the provider. `None` means no online source.
    fn collect_online(&self, _dir: &Path, _now: i64) -> Option<Result<Collected, ProviderError>> {
        None
    }

    fn has_online(&self) -> bool {
        self.info().endpoint.is_some()
    }
}

pub fn registry() -> Vec<Arc<dyn Provider>> {
    vec![
        Arc::new(claude::Claude),
        Arc::new(codex::Codex),
        Arc::new(copilot::Copilot),
        Arc::new(cursor::Cursor),
        Arc::new(tokenlog::TokenLog::gemini()),
        Arc::new(tokenlog::TokenLog::qwen()),
        Arc::new(opencode::OpenCode),
        Arc::new(apikey::OpenRouter),
        Arc::new(apikey::DeepSeek),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_safe() {
        let reg = registry();
        let mut ids: Vec<_> = reg.iter().map(|p| p.info().id).collect();
        assert!(ids.iter().all(|id| id.bytes().all(|b| b.is_ascii_lowercase())));
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), reg.len());
    }

    #[test]
    fn undocumented_endpoints_are_flagged_and_key_providers_are_opt_in() {
        for p in registry() {
            let i = p.info();
            if i.key_env.is_some() {
                assert!(!i.default_enabled && i.documented, "{}: API-key providers are opt-in and documented", i.id);
            }
            assert_eq!(p.has_online(), i.endpoint.is_some());
        }
    }

    #[test]
    fn nothing_is_detected_in_an_empty_folder() {
        let dir = crate::fsx::temp_dir("empty");
        // On macOS the Claude login can come from the real Keychain, so that adapter is skipped there.
        let skip = |id: &str| cfg!(target_os = "macos") && id == "claude";
        for p in registry().into_iter().filter(|p| p.info().key_env.is_none() && !skip(p.info().id)) {
            assert!(!p.detect(&dir), "{} detected in an empty folder", p.info().id);
            if let Some(Err(e)) = p.collect_online(&dir, 0) {
                assert_eq!(e.kind, crate::model::ErrorKind::NotSignedIn, "{}", p.info().id);
            } else if p.has_online() {
                panic!("{} went online without a login", p.info().id);
            }
        }
    }
}
