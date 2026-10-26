//! Tipos que cruzan la frontera IPC. Solo datos: sin kube-rs ni tokio, para que
//! compile rápido y los bindings TS se generen sin arrastrar el núcleo.

mod app;
mod error;
mod settings;

pub use app::{AppInfo, StartupReport};
pub use error::{ErrorCode, ErrorDetails, OculusError};
pub use settings::{Locale, Settings, SettingsPatch, TelemetrySettings, Theme};
