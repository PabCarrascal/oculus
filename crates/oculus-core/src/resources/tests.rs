use std::sync::{Arc, Mutex};

use http::{Request, Response};
use kube::Client;
use kube::client::Body;
use oculus_protocol::{Category, Relation};
use oculus_watch::{KubeSource, WatchManager, WatchOptions};
use serde_json::json;

use super::*;

fn kind(group: &str, kind: &str, plural: &str, namespaced: bool) -> ResourceKind {
    ResourceKind {
        group: group.into(),
        version: "v1".into(),
        kind: kind.into(),
        plural: plural.into(),
        namespaced,
        verbs: vec!["get".into(), "list".into(), "watch".into(), "update".into()],
        category: Category::Workloads,
    }
}

fn kinds() -> Vec<ResourceKind> {
    vec![
        kind("", "Pod", "pods", true),
        kind("", "Node", "nodes", false),
        kind("", "ConfigMap", "configmaps", true),
        kind("", "Secret", "secrets", true),
        kind("apps", "Deployment", "deployments", true),
        kind("apps", "ReplicaSet", "replicasets", true),
    ]
}

fn dep_ref() -> ObjectRef {
    ObjectRef {
        group: "apps".into(),
        version: "v1".into(),
        plural: "deployments".into(),
        namespace: Some("demo".into()),
        name: "web".into(),
    }
}

fn obj(v: Value) -> DynamicObject {
    serde_json::from_value(v).unwrap()
}

#[test]
fn un_secret_se_lee_sin_valores_ni_copia_en_anotaciones() {
    let mut s = obj(json!({
        "apiVersion": "v1", "kind": "Secret",
        "metadata": { "name": "s", "namespace": "n",
                      "annotations": { "kubectl.kubernetes.io/last-applied-configuration": "{\"data\":{\"token\":\"c2VjcmV0bw==\"}}", "otra": "x" } },
        "data": { "token": "c2VjcmV0bw==", "ca.crt": "Y2E=" },
        "stringData": { "extra": "valor" }
    }));
    assert_eq!(mask_secret(&mut s), ["ca.crt", "extra", "token"]);
    let text = serde_json::to_string(&s).unwrap();
    assert!(!text.contains("c2VjcmV0bw"), "ningún valor sale: {text}");
    assert!(!text.contains("valor"));
    assert!(text.contains("\"otra\":\"x\""), "las demás anotaciones se conservan");
}

#[test]
fn la_identidad_del_objeto_no_se_puede_cambiar_desde_el_editor() {
    let rk = kind("apps", "Deployment", "deployments", true);
    let mut ok = json!({ "apiVersion": "apps/v1", "kind": "Deployment", "metadata": { "name": "web" } });
    check_identity(&mut ok, &rk, &dep_ref()).unwrap();
    assert_eq!(ok["metadata"]["namespace"], "demo", "se completa si se borró");

    for (field, value, needle) in [
        ("name", json!("otro"), "nombre"),
        ("kind", json!("StatefulSet"), "kind"),
        ("apiVersion", json!("apps/v2"), "apiVersion"),
        ("namespace", json!("prod"), "namespace"),
    ] {
        let mut bad = json!({ "apiVersion": "apps/v1", "kind": "Deployment", "metadata": { "name": "web" } });
        match field {
            "name" | "namespace" => bad["metadata"][field] = value,
            _ => bad[field] = value,
        }
        let err = check_identity(&mut bad, &rk, &dep_ref()).unwrap_err();
        assert_eq!(err.code, ErrorCode::Invalid);
        assert!(err.message.contains(needle), "{needle}: {}", err.message);
    }
}

fn two_containers() -> Value {
    json!({
        "apiVersion": "v1", "kind": "Pod",
        "metadata": { "name": "p", "resourceVersion": "5", "generation": 1 },
        "spec": { "containers": [ { "name": "a", "image": "a:1" }, { "name": "b", "image": "b:1" } ] },
        "status": { "phase": "Running" }
    })
}

#[test]
fn el_parche_lleva_tests_con_el_valor_de_cada_paso() {
    let base = two_containers();
    // Se borra el primer contenedor y se cambia la imagen del segundo.
    let mut edited = base.clone();
    edited["spec"]["containers"] = json!([{ "name": "b", "image": "b:2" }]);
    let ops = build_patch(&base, &edited, false).unwrap();

    // Aplicado sobre la base, deja exactamente lo editado.
    let mut doc = base.clone();
    json_patch::patch(&mut doc, &ops).unwrap();
    assert_eq!(doc["spec"], edited["spec"]);
    // Cada operación que sustituye o quita va precedida de su test.
    for (i, op) in ops.iter().enumerate() {
        if matches!(op, PatchOperation::Replace(_) | PatchOperation::Remove(_)) {
            assert!(matches!(ops[i - 1], PatchOperation::Test(_)), "falta el test antes de {op}");
        }
    }

    // Si otro cambió entre medias un campo que el usuario tocó, el parche no aplica.
    let mut theirs = base.clone();
    theirs["spec"]["containers"][1]["image"] = json!("b:9");
    assert!(json_patch::patch(&mut theirs.clone(), &ops).is_err());
    // Con force (sobrescribir) sí, y solo con los cambios del usuario.
    let forced = build_patch(&base, &edited, true).unwrap();
    assert!(!forced.iter().any(|op| matches!(op, PatchOperation::Test(_))));
}

#[test]
fn status_y_metadatos_del_sistema_no_se_envian() {
    let base = two_containers();
    let mut edited = base.clone();
    edited["status"]["phase"] = json!("Failed");
    edited["metadata"]["resourceVersion"] = json!("99");
    edited["metadata"]["generation"] = json!(7);
    assert_eq!(build_patch(&base, &edited, false).unwrap_err().message, "No hay cambios que aplicar");

    edited["metadata"]["labels"] = json!({ "app": "x" });
    let ops = build_patch(&base, &edited, false).unwrap();
    let paths: Vec<_> = ops.iter().map(|op| op.path().to_string()).collect();
    assert_eq!(paths, ["/metadata/labels"]);
}

#[test]
fn relacionados_de_un_pod_y_selector_de_un_deployment() {
    let pod = obj(json!({
        "apiVersion": "v1", "kind": "Pod",
        "metadata": { "name": "web-1", "namespace": "demo",
                      "ownerReferences": [
                          { "apiVersion": "apps/v1", "kind": "ReplicaSet", "name": "web-86d", "uid": "u" },
                          { "apiVersion": "x.io/v1", "kind": "Raro", "name": "r", "uid": "u2" } ] },
        "spec": { "nodeName": "n1", "volumes": [
            { "name": "a", "configMap": { "name": "conf" } },
            { "name": "b", "secret": { "secretName": "cred" } },
            { "name": "c", "emptyDir": {} } ] }
    }));
    let (related, selector) = static_related(&pod, &kinds());
    assert!(selector.is_none());
    let summary: Vec<_> = related
        .iter()
        .map(|r| {
            (r.relation, r.kind.as_str(), r.name.as_str(), r.target.as_ref().map(|t| t.namespace.clone()))
        })
        .collect();
    assert_eq!(
        summary,
        [
            (Relation::Owner, "ReplicaSet", "web-86d", Some(Some("demo".into()))),
            (Relation::Owner, "Raro", "r", None),
            (Relation::Node, "Node", "n1", Some(None)),
            (Relation::Mounts, "ConfigMap", "conf", Some(Some("demo".into()))),
            (Relation::Mounts, "Secret", "cred", Some(Some("demo".into()))),
        ]
    );

    let dep = obj(json!({
        "apiVersion": "apps/v1", "kind": "Deployment", "metadata": { "name": "web", "namespace": "demo" },
        "spec": { "selector": { "matchLabels": { "app": "web", "tier": "front" } } }
    }));
    assert_eq!(static_related(&dep, &kinds()).1.as_deref(), Some("app=web,tier=front"));
}

#[test]
fn el_esquema_se_encuentra_por_su_gvk() {
    let schemas = json!({
        "io.k8s.api.apps.v1.DeploymentSpec": { "type": "object" },
        "io.k8s.api.apps.v1.Deployment": { "x-kubernetes-group-version-kind": [{ "group": "apps", "version": "v1", "kind": "Deployment" }] },
        "io.k8s.api.core.v1.Pod": { "x-kubernetes-group-version-kind": [{ "group": "", "version": "v1", "kind": "Pod" }] }
    });
    assert_eq!(
        schema_name(&schemas, "apps", "v1", "Deployment").as_deref(),
        Some("io.k8s.api.apps.v1.Deployment")
    );
    assert_eq!(schema_name(&schemas, "", "v1", "Pod").as_deref(), Some("io.k8s.api.core.v1.Pod"));
    assert_eq!(schema_name(&schemas, "apps", "v1", "Nope"), None);
}

#[test]
fn el_documento_del_esquema_tiene_el_tipo_en_la_raiz() {
    let schemas = json!({
        "io.k8s.api.apps.v1.DeploymentSpec": { "type": "object" },
        "io.k8s.api.apps.v1.Deployment": {
            "type": "object",
            "properties": { "spec": { "allOf": [{ "$ref": "#/components/schemas/io.k8s.api.apps.v1.DeploymentSpec" }] } }
        }
    });
    let doc = schema_document(&schemas, "io.k8s.api.apps.v1.Deployment");
    assert!(doc.get("$ref").is_none());
    assert_eq!(doc["type"], "object");
    assert!(doc.pointer("/properties/spec/allOf/0/$ref").is_some());
    assert_eq!(doc["components"]["schemas"], schemas);
}

// --- reemplazo contra un API server simulado -------------------------------

type Seen = Arc<Mutex<Vec<(String, String, Value)>>>;

fn session(status: u16, response: Value) -> (ClusterSession, Seen) {
    let (service, mut handle) = tower_test::mock::pair::<Request<Body>, Response<Body>>();
    let seen: Seen = Arc::default();
    let log = seen.clone();
    tokio::spawn(async move {
        while let Some((req, send)) = handle.next_request().await {
            let uri = format!("{} {}", req.method(), req.uri());
            let content_type = req
                .headers()
                .get(http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_owned();
            let bytes = http_body_util::BodyExt::collect(req.into_body()).await.unwrap().to_bytes();
            log.lock().unwrap().push((
                uri,
                content_type,
                serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            ));
            let body = Body::from(serde_json::to_vec(&response).unwrap());
            send.send_response(Response::builder().status(status).body(body).unwrap());
        }
    });
    let client = Client::new(service, "default");
    let watches = WatchManager::new(
        Arc::new(KubeSource { client: client.clone() }),
        WatchOptions::default(),
        errors::from_kube,
    );
    let s = ClusterSession {
        client,
        server_version: "v1.37.0".into(),
        kinds: kinds(),
        default_namespace: "default".into(),
        watches,
        schemas: Default::default(),
        tunnel: None,
    };
    (s, seen)
}

fn base() -> String {
    json!({
        "apiVersion": "apps/v1", "kind": "Deployment",
        "metadata": { "name": "web", "namespace": "demo", "resourceVersion": "7" },
        "spec": { "replicas": 3 }
    })
    .to_string()
}

fn edited() -> String {
    base().replace("\"replicas\":3", "\"replicas\":5")
}

#[tokio::test]
async fn la_actualizacion_envia_un_json_patch_estricto_con_tests() {
    let reply = json!({ "apiVersion": "apps/v1", "kind": "Deployment",
                        "metadata": { "name": "web", "namespace": "demo", "resourceVersion": "8", "managedFields": [{}] } });
    let (s, seen) = session(200, reply);
    let result = s.update(&dep_ref(), &base(), &edited(), true, false).await.unwrap();
    assert_eq!(result.resource_version, "8");
    assert!(!result.object_json.contains("managedFields"));

    let (uri, content_type, body) = seen.lock().unwrap()[0].clone();
    assert!(uri.starts_with("PATCH /apis/apps/v1/namespaces/demo/deployments/web?"), "{uri}");
    for param in ["dryRun=All", "fieldManager=oculus", "fieldValidation=Strict"] {
        assert!(uri.contains(param), "falta {param}: {uri}");
    }
    assert_eq!(content_type, "application/json-patch+json");
    assert_eq!(
        body,
        json!([
            { "op": "test", "path": "/spec/replicas", "value": 3 },
            { "op": "replace", "path": "/spec/replicas", "value": 5 }
        ])
    );
}

#[tokio::test]
async fn un_test_fallido_es_un_conflicto_y_un_campo_desconocido_trae_su_causa() {
    let test_failed = json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure", "reason": "Invalid", "code": 422,
        "message": "the server rejected our request due to an error in our request", "details": {} });
    let (s, _) = session(422, test_failed.clone());
    let e = s.update(&dep_ref(), &base(), &edited(), false, false).await.unwrap_err();
    assert_eq!(e.code, ErrorCode::Conflict);
    // Con force no hay tests: el mismo 422 no se interpreta como conflicto.
    let (s, _) = session(422, test_failed);
    assert_eq!(
        s.update(&dep_ref(), &base(), &edited(), false, true).await.unwrap_err().code,
        ErrorCode::Invalid
    );

    let strict = json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure", "reason": "BadRequest", "code": 400,
        "message": "error decoding patch: strict decoding error: unknown field \"spec.replicass\"" });
    let (s, _) = session(400, strict);
    let e = s.update(&dep_ref(), &base(), &edited(), false, false).await.unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid);
    assert_eq!(e.details.unwrap().causes[0].field.as_deref(), Some("spec.replicass"));
}

#[tokio::test]
async fn un_secret_no_se_puede_editar() {
    let (s, seen) = session(200, json!({}));
    let r = ObjectRef {
        group: String::new(),
        version: "v1".into(),
        plural: "secrets".into(),
        namespace: Some("n".into()),
        name: "s".into(),
    };
    let e = s.update(&r, "{}", "{}", false, false).await.unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid);
    assert!(seen.lock().unwrap().is_empty(), "no llega a hacer la petición");
}
