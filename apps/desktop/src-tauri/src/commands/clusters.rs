use oculus_protocol::{ClusterPrefs, ClusterSummary, KubeconfigSource, OculusError, ResourceKind};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_specta::Event;

use crate::events::KubeconfigChanged;
use crate::state::AppState;

// Asíncronos: leen kubeconfig del disco y no deben bloquear el hilo principal.

#[tauri::command]
#[specta::specta]
pub async fn kubeconfig_list_sources(
    state: State<'_, AppState>,
) -> Result<Vec<KubeconfigSource>, OculusError> {
    Ok(state.clusters.list_sources())
}

/// Abre el selector de ficheros nativo y añade el elegido. `None` si se cancela.
#[tauri::command]
#[specta::specta]
pub async fn kubeconfig_pick_source(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<KubeconfigSource>, OculusError> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut dialog = app.dialog().file().set_title("Añadir kubeconfig");
    if let Some(kube_dir) = dirs_kube() {
        dialog = dialog.set_directory(kube_dir);
    }
    dialog.pick_file(move |path| {
        let _ = tx.send(path);
    });
    let Some(path) = rx.await.ok().flatten() else { return Ok(None) };
    let path = path.into_path().map_err(|e| OculusError::invalid(format!("ruta no válida: {e}")))?;

    let source = state.clusters.add_source(&path)?;
    state.restart_watcher(&app);
    let _ = KubeconfigChanged.emit(&app);
    Ok(Some(source))
}

#[tauri::command]
#[specta::specta]
pub async fn kubeconfig_remove_source(
    app: AppHandle,
    state: State<'_, AppState>,
    source_id: String,
) -> Result<(), OculusError> {
    state.clusters.remove_source(&source_id)?;
    state.restart_watcher(&app);
    let _ = KubeconfigChanged.emit(&app);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn clusters_list(state: State<'_, AppState>) -> Result<Vec<ClusterSummary>, OculusError> {
    Ok(state.clusters.list_clusters())
}

#[tauri::command]
#[specta::specta]
pub async fn cluster_connect(state: State<'_, AppState>, cluster_id: String) -> Result<(), OculusError> {
    let extra_path = state.settings.get().extra_path;
    state.clusters.connect(&cluster_id, extra_path)
}

#[tauri::command]
#[specta::specta]
pub async fn cluster_disconnect(state: State<'_, AppState>, cluster_id: String) -> Result<(), OculusError> {
    state.clusters.disconnect(&cluster_id);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn cluster_set_prefs(
    state: State<'_, AppState>,
    cluster_id: String,
    prefs: ClusterPrefs,
) -> Result<ClusterPrefs, OculusError> {
    state.clusters.set_prefs(&cluster_id, prefs)
}

#[tauri::command]
#[specta::specta]
pub async fn discovery_get(
    state: State<'_, AppState>,
    cluster_id: String,
) -> Result<Vec<ResourceKind>, OculusError> {
    state.clusters.discovery(&cluster_id)
}

#[tauri::command]
#[specta::specta]
pub async fn namespaces_list(
    state: State<'_, AppState>,
    cluster_id: String,
) -> Result<Vec<String>, OculusError> {
    state.clusters.namespaces(&cluster_id).await
}

fn dirs_kube() -> Option<std::path::PathBuf> {
    let dir = std::env::home_dir()?.join(".kube");
    dir.is_dir().then_some(dir)
}
