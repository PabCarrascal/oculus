//! Eventos globales (docs/03-contrato-ipc.md): difusión de cambios poco frecuentes.

use oculus_protocol::{ClusterId, ClusterStatus};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct ClusterStatusChanged {
    pub cluster_id: ClusterId,
    pub status: ClusterStatus,
}

/// Cambió un fichero kubeconfig, la lista de fuentes o el `KUBECONFIG` de la
/// shell de login: la UI vuelve a pedir fuentes y clústeres.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct KubeconfigChanged;
