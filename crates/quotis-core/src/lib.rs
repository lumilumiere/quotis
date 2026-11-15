//! Quotis monitoring core: provider adapters, the normalized usage model, scheduling and the
//! snapshot cache. No UI and no Tauri: the desktop app and the CLI are both thin shells over this.

pub mod cache;
pub mod engine;
pub mod fsx;
pub mod layout;
pub mod model;
pub mod net;
pub mod providers;
pub mod settings;
pub mod time;

pub use engine::{Engine, Job, Kind, Outcome, ProviderStatus};
pub use model::*;
pub use settings::{data_dir, ProviderCfg, Settings};
