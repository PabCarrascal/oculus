use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub os: String,
    pub arch: String,
}

/// Medida de arranque en frío: desde que arranca el proceso hasta que la UI
/// avisa de su primera pintura con contenido (hito de la etapa 0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StartupReport {
    pub process_to_first_paint_ms: f64,
}
