//! User settings, shared by the desktop app and the CLI. Stored as `settings.json`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const APP_ID: &str = "io.github.lumilumiere.quotis";
const BACKUP: &str = "settings.json.bak";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderCfg {
    pub enabled: bool,
    pub dir: String,
}

impl Default for ProviderCfg {
    fn default() -> Self {
        Self { enabled: true, dir: String::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Per-provider overrides, keyed by provider id. A provider with no entry uses its default.
    pub providers: BTreeMap<String, ProviderCfg>,
    #[serde(alias = "claude_refresh_min")]
    pub refresh_min: u64,
    pub always_on_top: bool,
    pub show_used: bool,
    /// "island", "notch" or "panel".
    pub mode: String,
    /// "auto", "dark" or "light".
    pub theme: String,
    /// Translucent glass styling instead of the matte default.
    #[serde(alias = "blur")]
    pub glass: bool,
    /// Background opacity in percent, used by the glass style.
    #[serde(alias = "glass_opacity")]
    pub opacity: u8,
    pub hover_expand: bool,
    /// Seconds before an expanded island collapses by itself. 0 keeps it open.
    pub auto_collapse_secs: u64,
    /// Monitor name to show on. Empty means the primary monitor.
    pub display: String,
    /// Provider id pinned to the collapsed pill. Empty picks the tightest quota.
    pub pill_provider: String,
    /// Remaining-quota percentage at or below which the pill shows a warning. 0 turns alerts off.
    pub warn_below: u8,
    pub autostart: bool,
    /// Where the island was last dragged to: top-centre, physical pixels.
    pub position: Option<(i32, i32)>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            providers: BTreeMap::new(),
            refresh_min: 5,
            always_on_top: true,
            show_used: false,
            mode: "island".into(),
            theme: "auto".into(),
            glass: false,
            opacity: 80,
            hover_expand: false,
            auto_collapse_secs: 0,
            display: String::new(),
            pill_provider: String::new(),
            warn_below: 20,
            autostart: false,
            position: None,
        }
    }
}

impl Settings {
    /// Parses settings JSON, accepting the flat 0.x layout (`{"claude": {...}, ...}`) too.
    /// Unknown or malformed input yields the defaults rather than an error.
    pub fn parse(text: &str) -> Self {
        Self::try_parse(text).unwrap_or_default()
    }

    /// Like `parse`, but `None` when the text is not usable settings at all.
    pub fn try_parse(text: &str) -> Option<Self> {
        let mut v = serde_json::from_str::<Value>(text).ok()?;
        let obj = v.as_object_mut()?;
        let legacy = !obj.contains_key("providers");
        if legacy {
            let mut providers = serde_json::Map::new();
            for id in ["claude", "codex", "gemini", "qwen", "opencode", "copilot", "cursor"] {
                if let Some(cfg) = obj.remove(id) {
                    providers.insert(id.into(), cfg);
                }
            }
            obj.insert("providers".into(), Value::Object(providers));
            // 0.x defaulted to the glass look; "blur" meant a screen-sampled backdrop that no longer exists.
            obj.remove("blur");
        }
        let mut s: Settings = serde_json::from_value(v).ok()?;
        s.sanitize();
        Some(s)
    }

    pub fn sanitize(&mut self) {
        self.refresh_min = self.refresh_min.clamp(1, 60);
        self.opacity = self.opacity.clamp(20, 100);
        self.warn_below = self.warn_below.min(90);
        self.auto_collapse_secs = self.auto_collapse_secs.min(600);
        if !["island", "notch", "panel"].contains(&self.mode.as_str()) {
            self.mode = "island".into();
        }
        if !["auto", "dark", "light"].contains(&self.theme.as_str()) {
            self.theme = "auto".into();
        }
        self.providers.retain(|id, _| id.len() <= 32 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'));
    }

    /// Loads settings. If the file is damaged, the last good copy is used instead, so a
    /// provider the user turned off is not quietly turned back on. Defaults only when
    /// neither exists (first run) or both are unusable.
    pub fn load(dir: &Path) -> Self {
        let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok().and_then(|t| Self::try_parse(&t));
        read("settings.json").or_else(|| read(BACKUP)).unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        // The backup is written first and must succeed: a save is only reported as done when
        // both copies hold it, so a later damaged main file can never fall back to older
        // choices (and quietly turn a provider back on). If the main write then fails, the
        // main file still holds the previous good save and is what loads.
        crate::cache::write_atomic(&dir.join(BACKUP), json.as_bytes())?;
        crate::cache::write_atomic(&dir.join("settings.json"), json.as_bytes())
    }
}

pub fn home() -> PathBuf {
    std::env::home_dir().unwrap_or_default()
}

pub fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

pub fn config_base() -> PathBuf {
    if cfg!(windows) {
        env_dir("APPDATA").unwrap_or_else(|| home().join("AppData/Roaming"))
    } else if cfg!(target_os = "macos") {
        home().join("Library/Application Support")
    } else {
        env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config"))
    }
}

/// Where Quotis keeps `settings.json` and `snapshot.json`: the per-user app config folder.
/// `QUOTIS_DATA_DIR` overrides it (used by tests).
pub fn data_dir() -> PathBuf {
    env_dir("QUOTIS_DATA_DIR").unwrap_or_else(|| config_base().join(APP_ID))
}

/// Resolves a user-typed folder (`~` allowed) or falls back to the provider default.
pub fn resolve_dir(typed: &str, fallback: PathBuf) -> PathBuf {
    let dir = typed.trim();
    // "~" and "~/x" mean the user's home. "~someone" is another account and is not accepted.
    if dir == "~" {
        return home();
    }
    if let Some(rest) = dir.strip_prefix("~/").or_else(|| dir.strip_prefix("~\\")) {
        return home().join(rest.trim_start_matches(['/', '\\']));
    }
    // A relative path would depend on wherever the app happened to be started from.
    if Path::new(dir).is_absolute() {
        return PathBuf::from(dir);
    }
    fallback
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_flat_settings_migrate() {
        let s = Settings::parse(r#"{"codex":{"enabled":false},"gemini":{"dir":"~/g"},"claude_refresh_min":7,"glass_opacity":55,"blur":true,"show_used":true}"#);
        assert!(!s.providers["codex"].enabled);
        assert_eq!(s.providers["gemini"], ProviderCfg { enabled: true, dir: "~/g".into() });
        assert!(!s.providers.contains_key("claude"));
        assert_eq!((s.refresh_min, s.opacity, s.show_used, s.glass), (7, 55, true, false));
        assert_eq!(s.mode, "island");
    }

    #[test]
    fn current_settings_round_trip() {
        let mut s = Settings::default();
        s.providers.insert("openrouter".into(), ProviderCfg { enabled: true, dir: String::new() });
        s.mode = "notch".into();
        s.position = Some((10, -20));
        let back = Settings::parse(&serde_json::to_string(&s).unwrap());
        assert_eq!(back, s);
    }

    #[test]
    fn malformed_and_hostile_input_falls_back_to_safe_values() {
        assert_eq!(Settings::parse("not json"), Settings::default());
        assert_eq!(Settings::parse("[1,2]"), Settings::default());
        assert_eq!(Settings::parse(r#"{"providers": 5}"#), Settings::default());
        let s = Settings::parse(
            r#"{"providers":{"../evil":{"enabled":true},"ok":{}},"refresh_min":0,"opacity":255,"mode":"<script>","theme":"x","warn_below":200,"auto_collapse_secs":99999}"#,
        );
        assert_eq!(s.providers.keys().collect::<Vec<_>>(), ["ok"]);
        assert_eq!((s.refresh_min, s.opacity, s.mode.as_str(), s.theme.as_str(), s.warn_below, s.auto_collapse_secs), (1, 100, "island", "auto", 90, 600));
    }

    #[test]
    fn a_damaged_settings_file_falls_back_to_the_last_good_one() {
        let dir = crate::fsx::temp_dir("settings-bak");
        assert_eq!(Settings::load(&dir), Settings::default(), "first run");
        // Everything on, saved. Then Claude is turned off in the very next save.
        let mut s = Settings::default();
        s.save(&dir).unwrap();
        s.providers.insert("claude".into(), ProviderCfg { enabled: false, dir: String::new() });
        s.refresh_min = 9;
        s.save(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), "{ half written").unwrap();
        let loaded = Settings::load(&dir);
        assert!(!loaded.providers["claude"].enabled, "the latest choice survives: the provider stays off");
        assert_eq!(loaded.refresh_min, 9, "the backup is the latest good save, not the one before");
        // A save whose backup cannot be written fails, and changes neither file.
        let blocked = crate::fsx::temp_dir("settings-bak-blocked");
        Settings::default().save(&blocked).unwrap();
        std::fs::remove_file(blocked.join("settings.json.bak")).unwrap();
        std::fs::create_dir(blocked.join("settings.json.bak")).unwrap();
        let mut off = Settings::default();
        off.providers.insert("claude".into(), ProviderCfg { enabled: false, dir: String::new() });
        assert!(off.save(&blocked).is_err());
        assert_eq!(Settings::load(&blocked), Settings::default(), "the failed save left the old settings in place");
        // Damage to the backup alone changes nothing.
        loaded.save(&dir).unwrap();
        std::fs::write(dir.join("settings.json.bak"), "garbage").unwrap();
        assert_eq!(Settings::load(&dir), loaded);
    }

    #[test]
    fn dirs_resolve() {
        assert_eq!(resolve_dir("~/g", PathBuf::from("x")), home().join("g"));
        assert_eq!(resolve_dir("  ", PathBuf::from("x")), PathBuf::from("x"));
        let abs = if cfg!(windows) { r"C:\abs\dir" } else { "/abs/dir" };
        assert_eq!(resolve_dir(abs, PathBuf::from("x")), PathBuf::from(abs));
        assert_eq!(resolve_dir("~", PathBuf::from("x")), home());
        // Another user's home, and paths relative to wherever the app started, are refused.
        assert_eq!(resolve_dir("~someone/.claude", PathBuf::from("x")), PathBuf::from("x"));
        assert_eq!(resolve_dir("relative/dir", PathBuf::from("x")), PathBuf::from("x"));
        assert_eq!(resolve_dir("../../etc", PathBuf::from("x")), PathBuf::from("x"));
    }
}
