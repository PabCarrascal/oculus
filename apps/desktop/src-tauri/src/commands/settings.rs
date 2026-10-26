use oculus_protocol::{OculusError, Settings, SettingsPatch};
use tauri::State;

use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub fn settings_get(state: State<'_, AppState>) -> Settings {
    state.settings.get()
}

/// Asíncrono para que la escritura en disco no bloquee el hilo principal.
#[tauri::command]
#[specta::specta]
pub async fn settings_set(state: State<'_, AppState>, patch: SettingsPatch) -> Result<Settings, OculusError> {
    state.settings.apply(patch)
}
