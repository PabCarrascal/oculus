//! Subconjunto de JSONPath de `additionalPrinterColumns`: campos con punto,
//! índices (`[0]`) y filtros de igualdad (`[?(@.type=="Ready")]`). Lo que no se
//! entiende devuelve `None` (celda vacía), nunca un error.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
enum Step {
    Field(String),
    Index(usize),
    /// `[?(@.campo=="valor")]`: primer elemento del array que cumple.
    Filter {
        field: String,
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct JsonPath(Vec<Step>);

impl JsonPath {
    pub fn parse(path: &str) -> Option<Self> {
        let path = path.trim().trim_start_matches('{').trim_end_matches('}');
        let mut rest = path.strip_prefix('.').unwrap_or(path);
        let mut steps = Vec::new();
        while !rest.is_empty() {
            if let Some(inner) = rest.strip_prefix('[') {
                let end = inner.find(']')?;
                let body = &inner[..end];
                steps.push(parse_bracket(body)?);
                rest = &inner[end + 1..];
                rest = rest.strip_prefix('.').unwrap_or(rest);
            } else {
                let end = rest.find(['.', '[']).unwrap_or(rest.len());
                if end == 0 {
                    return None;
                }
                steps.push(Step::Field(rest[..end].to_owned()));
                rest = &rest[end..];
                rest = rest.strip_prefix('.').unwrap_or(rest);
            }
        }
        Some(Self(steps))
    }

    pub fn eval<'a>(&self, root: &'a Value) -> Option<&'a Value> {
        self.0.iter().try_fold(root, |v, step| match step {
            Step::Field(name) => v.get(name),
            Step::Index(i) => v.get(*i),
            Step::Filter { field, value } => v.as_array()?.iter().find(|item| {
                item.get(field).is_some_and(|f| match f {
                    Value::String(s) => s == value,
                    Value::Bool(b) => value == if *b { "true" } else { "false" },
                    Value::Number(n) => n.as_f64().is_some_and(|n| value.parse::<f64>() == Ok(n)),
                    _ => false,
                })
            }),
        })
    }
}

fn parse_bracket(body: &str) -> Option<Step> {
    if let Ok(i) = body.parse::<usize>() {
        return Some(Step::Index(i));
    }
    // ?(@.campo=="valor") o ?(@.campo=='valor')
    let expr = body.strip_prefix("?(")?.strip_suffix(')')?.trim();
    let (lhs, rhs) = expr.split_once("==")?;
    let field = lhs.trim().strip_prefix("@.")?.to_owned();
    let value = rhs.trim().trim_matches(|c| c == '"' || c == '\'').to_owned();
    Some(Step::Filter { field, value })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn eval(path: &str, v: &Value) -> Option<Value> {
        JsonPath::parse(path)?.eval(v).cloned()
    }

    #[test]
    fn campos_indices_y_filtros() {
        let obj = json!({
            "spec": { "replicas": 3, "containers": [{ "image": "nginx:1" }] },
            "status": { "conditions": [
                { "type": "Synced", "status": "False" },
                { "type": "Ready", "status": "True" }
            ]}
        });
        assert_eq!(eval(".spec.replicas", &obj), Some(json!(3)));
        assert_eq!(eval(".spec.containers[0].image", &obj), Some(json!("nginx:1")));
        assert_eq!(eval(r#".status.conditions[?(@.type=="Ready")].status"#, &obj), Some(json!("True")));
        assert_eq!(eval(".status.conditions[?(@.type=='Synced')].status", &obj), Some(json!("False")));
        assert_eq!(eval("{.spec.replicas}", &obj), Some(json!(3)));
    }

    #[test]
    fn lo_que_no_existe_o_no_se_entiende_es_none() {
        let obj = json!({ "spec": {} });
        assert_eq!(eval(".spec.nope", &obj), None);
        assert_eq!(eval(".spec.items[3]", &obj), None);
        assert!(JsonPath::parse(".spec[*].x").is_none());
        assert!(JsonPath::parse(".status..x").is_none());
    }
}
