use serde::{Deserialize, Serialize};
use specta::Type;

/// Un objeto concreto dentro de un clúster.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ObjectRef {
    /// Vacío para el grupo core.
    pub group: String,
    pub version: String,
    pub plural: String,
    pub namespace: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EventInfo {
    #[serde(rename = "type")]
    pub event_type: String,
    pub reason: String,
    pub message: String,
    pub count: u32,
    /// ISO 8601.
    pub last_seen: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Relation {
    /// Este objeto pertenece a `target` (ownerReferences).
    Owner,
    /// `target` cumple el selector de este objeto (pods de un Deployment o Service).
    Selects,
    /// Nodo en el que corre el pod.
    Node,
    /// ConfigMap, Secret o PVC montado por el pod.
    Mounts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RelatedRef {
    pub relation: Relation,
    pub kind: String,
    /// `None` si el tipo no aparece en el discovery (no se puede enlazar).
    pub target: Option<ObjectRef>,
    pub name: String,
}

/// Lo que necesita el panel de detalle. El YAML lo genera la UI a partir del JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDetail {
    pub api_version: String,
    pub kind: String,
    pub uid: String,
    pub resource_version: String,
    /// El objeto en JSON, sin `managedFields`. En un Secret, los valores van vacíos.
    pub object_json: String,
    pub events: Vec<EventInfo>,
    pub related: Vec<RelatedRef>,
    /// Se puede editar el YAML (verbo update, clúster no en solo lectura, no es un Secret).
    pub editable: bool,
    /// Claves de un Secret; sus valores solo llegan con `secret_reveal`.
    pub secret_keys: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateResult {
    pub resource_version: String,
    pub object_json: String,
}
