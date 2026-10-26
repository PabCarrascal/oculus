use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::OculusError;

/// Identifica un tipo de recurso dentro de un clúster.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KindRef {
    /// Vacío para el grupo core.
    pub group: String,
    pub version: String,
    pub plural: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Scope {
    /// Todo el clúster (o todos los namespaces).
    All,
    /// Solo estos namespaces: un watcher por namespace.
    Namespaces { names: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum ColumnType {
    String,
    Number,
    Date,
    Labels,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ColumnDef {
    pub id: String,
    /// Clave i18n (`columns.<id>`) para los tipos nativos; texto literal para CRDs.
    pub label: String,
    pub translate: bool,
    #[serde(rename = "type")]
    pub column_type: ColumnType,
    pub default_visible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(untagged)]
pub enum CellValue {
    Number(f64),
    Text(String),
    Labels { labels: BTreeMap<String, String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    Ok,
    Warning,
    Error,
    Progress,
    Neutral,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RowStatus {
    pub tone: Tone,
    pub text: String,
}

/// Lo que la UI pinta de un objeto. El objeto completo se pide al abrir el detalle (D4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub uid: String,
    pub name: String,
    pub namespace: Option<String>,
    /// ISO 8601; la edad se calcula en la UI.
    pub created_at: Option<String>,
    pub status: Option<RowStatus>,
    /// Clave = `ColumnDef.id`. Una celda que falta se pinta vacía.
    pub cells: BTreeMap<String, CellValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum WatchState {
    Initializing,
    Ready,
    Relisting,
    Backoff,
    Error,
}

/// Lo que llega por el Channel de una suscripción (docs/03-contrato-ipc.md).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ResourceEvent {
    /// Lista completa: sustituye a lo que hubiera.
    Snapshot {
        columns: Vec<ColumnDef>,
        rows: Vec<Row>,
    },
    Delta {
        upserts: Vec<Row>,
        deletes: Vec<String>,
    },
    Status {
        state: WatchState,
        error: Option<OculusError>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn las_celdas_viajan_sin_etiqueta() {
        let cells = BTreeMap::from([
            ("restarts".to_owned(), CellValue::Number(3.0)),
            ("node".to_owned(), CellValue::Text("n1".into())),
            (
                "labels".to_owned(),
                CellValue::Labels { labels: BTreeMap::from([("app".into(), "web".into())]) },
            ),
        ]);
        assert_eq!(
            serde_json::to_value(&cells).unwrap(),
            serde_json::json!({ "labels": { "labels": { "app": "web" } }, "node": "n1", "restarts": 3.0 })
        );
    }

    #[test]
    fn los_eventos_van_etiquetados_por_tipo() {
        let e = ResourceEvent::Delta { upserts: vec![], deletes: vec!["u1".into()] };
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            serde_json::json!({ "type": "delta", "upserts": [], "deletes": ["u1"] })
        );
        let s = Scope::Namespaces { names: vec!["a".into()] };
        assert_eq!(
            serde_json::to_value(&s).unwrap(),
            serde_json::json!({ "kind": "namespaces", "names": ["a"] })
        );
    }
}
