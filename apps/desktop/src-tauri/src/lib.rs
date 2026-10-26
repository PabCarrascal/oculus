//! Capa Tauri: arranca la app y expone el núcleo por IPC. Sin lógica de negocio.

mod bindings;
mod commands;
mod state;

use std::time::Instant;

use oculus_core::settings::SettingsStore;
use tauri::Manager;
use tracing_subscriber::EnvFilter;

use crate::state::AppState;

pub fn run(started: Instant) {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_env("OCULUS_LOG").unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let builder = bindings::builder();

    #[cfg(debug_assertions)]
    bindings::export(&builder);

    tauri::Builder::default()
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            tracing::info!(path = %settings_path.display(), "cargando ajustes");
            app.manage(AppState::new(started, SettingsStore::load(settings_path)));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("no se pudo arrancar Oculus");
}
