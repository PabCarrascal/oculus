use oculus_protocol::{ObjectRef, OculusError, ResourceDetail, UpdateResult};
use tauri::State;

use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub async fn resource_get(
    state: State<'_, AppState>,
    cluster_id: String,
    target: ObjectRef,
) -> Result<ResourceDetail, OculusError> {
    state.clusters.detail(&cluster_id, &target).await
}

/// Aplica los cambios del editor (JSON Patch entre `base_json` y `edited_json`).
/// Con `dry_run` solo valida; con `force` sobrescribe aunque otro haya cambiado
/// los mismos campos.
#[tauri::command]
#[specta::specta]
pub async fn resource_update(
    state: State<'_, AppState>,
    cluster_id: String,
    target: ObjectRef,
    base_json: String,
    edited_json: String,
    dry_run: bool,
    force: bool,
) -> Result<UpdateResult, OculusError> {
    state.clusters.update(&cluster_id, &target, &base_json, &edited_json, dry_run, force).await
}

/// Un valor de un Secret, solo cuando el usuario lo pide.
#[tauri::command]
#[specta::specta]
pub async fn secret_reveal(
    state: State<'_, AppState>,
    cluster_id: String,
    target: ObjectRef,
    key: String,
) -> Result<String, OculusError> {
    state.clusters.reveal_secret(&cluster_id, &target, &key).await
}

/// Esquema JSON del tipo para validar el editor; `None` si el clúster no lo publica.
#[tauri::command]
#[specta::specta]
pub async fn openapi_schema(
    state: State<'_, AppState>,
    cluster_id: String,
    group: String,
    version: String,
    kind: String,
) -> Result<Option<String>, OculusError> {
    state.clusters.schema(&cluster_id, &group, &version, &kind).await
}
