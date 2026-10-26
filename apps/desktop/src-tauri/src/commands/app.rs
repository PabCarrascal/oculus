use oculus_protocol::{AppInfo, StartupReport};
use tauri::{AppHandle, State};

use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub fn app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
    }
}

/// La UI lo llama tras su primera pintura con contenido.
#[tauri::command]
#[specta::specta]
pub fn startup_report(state: State<'_, AppState>) -> StartupReport {
    let ms = *state.first_paint_ms.get_or_init(|| {
        let ms = state.started.elapsed().as_secs_f64() * 1000.0;
        tracing::info!(ms = format!("{ms:.0}"), "primera pintura");
        ms
    });
    StartupReport { process_to_first_paint_ms: ms }
}
