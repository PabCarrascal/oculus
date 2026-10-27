//! Relaciones que se ven en el propio objeto, sin preguntar al clúster: dueños,
//! nodo y volúmenes de un pod, y el selector de pods de un workload o Service.

use kube::api::DynamicObject;
use oculus_protocol::{ObjectRef, RelatedRef, Relation, ResourceKind};
use serde_json::Value;

/// Devuelve las relaciones directas y, si el objeto selecciona pods, el selector
/// de labels para listarlos (`app=web,tier=front`).
pub fn static_related(obj: &DynamicObject, kinds: &[ResourceKind]) -> (Vec<RelatedRef>, Option<String>) {
    let namespace = obj.metadata.namespace.clone();
    let kind = obj.types.as_ref().map(|t| t.kind.as_str()).unwrap_or_default();
    let mut related = Vec::new();

    for owner in obj.metadata.owner_references.iter().flatten() {
        let group = owner.api_version.rsplit_once('/').map(|(g, _)| g).unwrap_or("");
        related.push(link(kinds, Relation::Owner, group, &owner.kind, namespace.clone(), &owner.name));
    }

    let data = &obj.data;
    let selector = match kind {
        "Deployment" | "StatefulSet" | "DaemonSet" | "ReplicaSet" | "Job" => {
            match_labels(data.pointer("/spec/selector/matchLabels"))
        }
        "Service" => match_labels(data.pointer("/spec/selector")),
        _ => None,
    };

    if kind == "Pod" {
        if let Some(node) = data.pointer("/spec/nodeName").and_then(Value::as_str) {
            related.push(link(kinds, Relation::Node, "", "Node", None, node));
        }
        for volume in data.pointer("/spec/volumes").and_then(Value::as_array).into_iter().flatten() {
            let mounted = [
                ("ConfigMap", "/configMap/name"),
                ("Secret", "/secret/secretName"),
                ("PersistentVolumeClaim", "/persistentVolumeClaim/claimName"),
            ];
            for (k, pointer) in mounted {
                if let Some(name) = volume.pointer(pointer).and_then(Value::as_str) {
                    related.push(link(kinds, Relation::Mounts, "", k, namespace.clone(), name));
                }
            }
        }
    }
    (related, selector)
}

fn match_labels(v: Option<&Value>) -> Option<String> {
    let map = v?.as_object()?;
    if map.is_empty() {
        return None;
    }
    Some(map.iter().filter_map(|(k, v)| Some(format!("{k}={}", v.as_str()?))).collect::<Vec<_>>().join(","))
}

fn link(
    kinds: &[ResourceKind],
    relation: Relation,
    group: &str,
    kind: &str,
    namespace: Option<String>,
    name: &str,
) -> RelatedRef {
    let target = kinds.iter().find(|k| k.group == group && k.kind == kind).map(|k| ObjectRef {
        group: k.group.clone(),
        version: k.version.clone(),
        plural: k.plural.clone(),
        namespace: if k.namespaced { namespace } else { None },
        name: name.to_owned(),
    });
    RelatedRef { relation, kind: kind.to_owned(), target, name: name.to_owned() }
}
