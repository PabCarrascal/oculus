//! Proyección de objetos a filas: solo lo que la tabla pinta (D4).

mod jsonpath;
mod native;

use std::collections::BTreeMap;

use kube::api::DynamicObject;
use oculus_protocol::{CellValue, ColumnDef, ColumnType, Row, RowStatus};
use serde_json::Value;

pub use jsonpath::JsonPath;

type NativeFn = fn(&DynamicObject, &Value, &mut Row);

/// Columna de un CRD sacada de `additionalPrinterColumns`.
#[derive(Debug, Clone)]
pub struct PrinterColumn {
    pub name: String,
    pub column_type: ColumnType,
    pub path: JsonPath,
    pub priority: i32,
}

#[derive(Clone)]
enum Kind {
    Native(NativeFn),
    Printer(Vec<(String, PrinterColumn)>),
    Generic,
}

/// Sabe qué columnas tiene un tipo y cómo convertir cada objeto en su fila.
#[derive(Clone)]
pub struct Projector {
    columns: Vec<ColumnDef>,
    kind: Kind,
}

impl Projector {
    pub fn native(group: &str, kind: &str, namespaced: bool) -> Self {
        let (extra, f) = native::lookup(group, kind);
        let kind = match f {
            Some(f) => Kind::Native(f),
            None => Kind::Generic,
        };
        Self { columns: frame(namespaced, extra), kind }
    }

    pub fn printer(namespaced: bool, printer: Vec<PrinterColumn>) -> Self {
        // Name y Age ya están en todas las tablas; muchos CRDs los repiten.
        let printer: Vec<(String, PrinterColumn)> = printer
            .into_iter()
            .filter(|c| {
                let n = c.name.to_ascii_lowercase();
                n != "name" && n != "age" && n != "namespace"
            })
            .enumerate()
            .map(|(i, c)| (format!("p{i}"), c))
            .collect();
        let extra = printer
            .iter()
            .map(|(id, c)| ColumnDef {
                id: id.clone(),
                label: c.name.clone(),
                translate: false,
                column_type: c.column_type,
                default_visible: c.priority == 0,
            })
            .collect();
        Self { columns: frame(namespaced, extra), kind: Kind::Printer(printer) }
    }

    pub fn columns(&self) -> &[ColumnDef] {
        &self.columns
    }

    pub fn project(&self, obj: &DynamicObject) -> Row {
        let meta = &obj.metadata;
        let name = meta.name.clone().unwrap_or_default();
        let mut row = Row {
            uid: meta
                .uid
                .clone()
                .unwrap_or_else(|| format!("{}/{name}", meta.namespace.as_deref().unwrap_or(""))),
            name,
            namespace: meta.namespace.clone(),
            created_at: meta.creation_timestamp.as_ref().map(|t| t.0.to_string()),
            status: None,
            cells: BTreeMap::new(),
        };
        if let Some(labels) = meta.labels.as_ref().filter(|l| !l.is_empty()) {
            row.cells.insert("labels".into(), CellValue::Labels { labels: labels.clone() });
        }
        match &self.kind {
            Kind::Native(f) => f(obj, &obj.data, &mut row),
            Kind::Printer(cols) => {
                for (id, col) in cols {
                    if let Some(v) = col.path.eval(&obj.data).and_then(|v| cell(v, col.column_type)) {
                        row.cells.insert(id.clone(), v);
                    }
                }
            }
            Kind::Generic => {}
        }
        if meta.deletion_timestamp.is_some() {
            row.status = Some(RowStatus { tone: oculus_protocol::Tone::Warning, text: "Terminating".into() });
        }
        row
    }
}

/// Columnas que tienen todas las tablas alrededor de las propias del tipo.
fn frame(namespaced: bool, extra: Vec<ColumnDef>) -> Vec<ColumnDef> {
    let mut cols = vec![col("name", ColumnType::String, true)];
    if namespaced {
        cols.push(col("namespace", ColumnType::String, true));
    }
    cols.extend(extra);
    cols.push(col("age", ColumnType::Date, true));
    cols.push(col("labels", ColumnType::Labels, false));
    cols
}

pub(crate) fn col(id: &str, column_type: ColumnType, default_visible: bool) -> ColumnDef {
    ColumnDef { id: id.into(), label: format!("columns.{id}"), translate: true, column_type, default_visible }
}

fn cell(v: &Value, ty: ColumnType) -> Option<CellValue> {
    match (v, ty) {
        (Value::Null, _) => None,
        (Value::Number(n), _) => n.as_f64().map(CellValue::Number),
        (Value::String(s), ColumnType::Number) => {
            s.parse().ok().map(CellValue::Number).or(Some(CellValue::Text(s.clone())))
        }
        (Value::String(s), _) => Some(CellValue::Text(s.clone())),
        (Value::Bool(b), _) => Some(CellValue::Text(b.to_string())),
        (other, _) => {
            let s = other.to_string();
            // Por caracteres, no bytes: truncar a mitad de un carácter UTF-8 entra en pánico.
            Some(CellValue::Text(if s.chars().count() > 120 {
                s.chars().take(117).collect::<String>() + "…"
            } else {
                s
            }))
        }
    }
}

#[cfg(test)]
mod tests;
