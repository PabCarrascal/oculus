//! Capa Tauri: arranca la app y expone el núcleo por IPC. Sin lógica de negocio.

mod bindings;
mod commands;
mod events;
mod state;

use std::sync::Arc;
use std::time::Instant;

use oculus_core::cluster::StatusSink;
use oculus_core::service::ClusterService;
use oculus_core::settings::SettingsStore;
use tauri::Manager;
use tauri_specta::Event;
use tracing_subscriber::EnvFilter;

use crate::events::{ClusterStatusChanged, KubeconfigChanged};
use crate::state::AppState;

pub fn run(started: Instant) {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_env("OCULUS_LOG").unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let builder = bindings::builder();

    #[cfg(debug_assertions)]
    bindings::export(&builder);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let config_dir = app.path().app_config_dir()?;
            tracing::info!(dir = %config_dir.display(), "directorio de configuración");

            let handle = app.handle().clone();
            let sink: StatusSink = Arc::new(move |id, status| {
                let event = ClusterStatusChanged { cluster_id: id.clone(), status: status.clone() };
                let _ = event.emit(&handle);
            });
            let clusters = Arc::new(ClusterService::new(config_dir.join("workspace.json"), sink));
            let settings = SettingsStore::load(config_dir.join("settings.json"));
            app.manage(AppState::new(started, settings, clusters.clone()));
            app.state::<AppState>().restart_watcher(app.handle());

            // El entorno de la shell de login tarda unos cientos de ms: no se
            // espera al arrancar. Al tenerlo puede haber fuentes nuevas (KUBECONFIG).
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let from_shell = clusters.init_shell_env().await;
                tracing::info!(from_shell, "entorno de la shell de login listo");
                handle.state::<AppState>().restart_watcher(&handle);
                let _ = KubeconfigChanged.emit(&handle);
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("no se pudo arrancar Oculus")
        .run(|app, event| {
            // Cierra las sesiones y, con ellas, los túneles SSH (si no, quedarían
            // procesos `ssh` vivos tras salir).
            if let tauri::RunEvent::Exit = event {
                app.state::<AppState>().clusters.shutdown();
            }
        });
}
