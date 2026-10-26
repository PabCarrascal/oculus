use std::sync::OnceLock;
use std::time::Instant;

use oculus_core::settings::SettingsStore;

pub struct AppState {
    pub started: Instant,
    /// Solo cuenta la primera pintura; las recargas del webview no la pisan.
    pub first_paint_ms: OnceLock<f64>,
    pub settings: SettingsStore,
}

impl AppState {
    pub fn new(started: Instant, settings: SettingsStore) -> Self {
        Self { started, first_paint_ms: OnceLock::new(), settings }
    }
}
