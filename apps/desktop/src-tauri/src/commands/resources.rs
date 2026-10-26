use std::sync::Arc;
use std::time::Duration;

use oculus_protocol::{KindRef, OculusError, ResourceEvent, Scope};
use oculus_watch::Subscriber;
use tauri::State;
use tauri::ipc::Channel;

use crate::state::AppState;

/// Abre (o comparte) una vigilancia y devuelve su id. Los eventos llegan por `channel`.
#[tauri::command]
#[specta::specta]
pub async fn resources_subscribe(
    state: State<'_, AppState>,
    cluster_id: String,
    kind: KindRef,
    scope: Scope,
    channel: Channel<ResourceEvent>,
) -> Result<String, OculusError> {
    let linger = Duration::from_secs(u64::from(state.settings.get().watch_linger_seconds));
    // Un envío solo falla si el webview se cerró; las recargas las cubre `resources_reset`.
    let subscriber: Subscriber = Arc::new(move |event| channel.send(event.clone()).is_ok());
    let id = state.clusters.subscribe(&cluster_id, &kind, scope, linger, subscriber)?;
    state.track_subscription(&cluster_id, &id);
    Ok(id)
}

#[tauri::command]
#[specta::specta]
pub async fn resources_unsubscribe(
    state: State<'_, AppState>,
    cluster_id: String,
    subscription_id: String,
) -> Result<(), OculusError> {
    state.untrack_subscription(&subscription_id);
    state.clusters.unsubscribe(&cluster_id, &subscription_id);
    Ok(())
}

/// La UI lo llama al arrancar: da de baja lo que abrió una carga anterior del
/// webview (una recarga no avisa y sus Channels ya no tienen receptor).
#[tauri::command]
#[specta::specta]
pub async fn resources_reset(state: State<'_, AppState>) -> Result<(), OculusError> {
    for (cluster_id, id) in state.take_subscriptions() {
        state.clusters.unsubscribe(&cluster_id, &id);
    }
    Ok(())
}
