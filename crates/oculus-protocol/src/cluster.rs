use serde::{Deserialize, Serialize};
use specta::Type;

use crate::OculusError;

/// sha256(rutaCanónica + "\0" + contexto) en hex, 16 caracteres. Estable entre reinicios.
pub type ClusterId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum SourceOrigin {
    /// `~/.kube/config` cuando no hay `KUBECONFIG`.
    Default,
    /// Una de las rutas de `KUBECONFIG`.
    Env,
    /// Añadido a mano desde la app.
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KubeconfigSource {
    pub id: String,
    pub path: String,
    pub origin: SourceOrigin,
    /// Fichero que falta o no se puede leer. Sus contextos no aparecen, pero la app sigue.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ClusterPrefs {
    pub alias: Option<String>,
    /// Color en hex (#rrggbb) para distinguir el clúster en toda la UI.
    pub color: Option<String>,
    /// Oculta todas las acciones que modifican el clúster.
    pub read_only: bool,
    /// Las acciones destructivas exigen escribir el nombre del recurso.
    pub protected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum ConnectStep {
    Auth,
    Version,
    Discovery,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "state", rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum ClusterStatus {
    Disconnected,
    Connecting { step: ConnectStep },
    Connected { server_version: String, degraded: bool },
    Error { error: OculusError },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ClusterSummary {
    pub id: ClusterId,
    pub context_name: String,
    pub source_id: String,
    /// URL del API server; solo se muestra.
    pub server: Option<String>,
    pub default_namespace: Option<String>,
    pub prefs: ClusterPrefs,
    pub status: ClusterStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Workloads,
    Config,
    Network,
    Storage,
    Access,
    Cluster,
    Custom,
}

/// Un tipo de recurso descubierto en el clúster. Las columnas llegan en la etapa 2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResourceKind {
    /// Vacío para el grupo core.
    pub group: String,
    pub version: String,
    pub kind: String,
    pub plural: String,
    pub namespaced: bool,
    pub verbs: Vec<String>,
    pub category: Category,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;

    #[test]
    fn el_estado_va_etiquetado_y_en_camel_case() {
        let connected = ClusterStatus::Connected { server_version: "v1.32.0".into(), degraded: false };
        assert_eq!(
            serde_json::to_value(&connected).unwrap(),
            serde_json::json!({ "state": "connected", "serverVersion": "v1.32.0", "degraded": false })
        );

        let err = ClusterStatus::Error { error: OculusError::new(ErrorCode::Unreachable, "timeout") };
        assert_eq!(serde_json::to_value(&err).unwrap()["state"], "error");
        assert_eq!(serde_json::to_value(&err).unwrap()["error"]["code"], "Unreachable");
    }
}
