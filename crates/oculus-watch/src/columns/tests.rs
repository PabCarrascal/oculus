use kube::api::DynamicObject;
use oculus_protocol::{CellValue, ColumnType, Tone};
use serde_json::{Value, json};

use super::*;

fn obj(v: Value) -> DynamicObject {
    serde_json::from_value(v).unwrap()
}

fn pod(status: Value) -> DynamicObject {
    obj(json!({
        "apiVersion": "v1", "kind": "Pod",
        "metadata": { "name": "web-1", "namespace": "prod", "uid": "u1",
                      "creationTimestamp": "2026-10-26T10:00:00Z", "labels": { "app": "web" } },
        "spec": { "nodeName": "n1", "containers": [{ "name": "a" }, { "name": "b" }] },
        "status": status
    }))
}

fn text(row: &Row, id: &str) -> Option<String> {
    match row.cells.get(id)? {
        CellValue::Text(s) => Some(s.clone()),
        CellValue::Number(n) => Some(n.to_string()),
        CellValue::Labels { .. } => Some("labels".into()),
    }
}

fn status_of(row: &Row) -> (Tone, String) {
    let s = row.status.clone().expect("fila con estado");
    (s.tone, s.text)
}

fn pods() -> Projector {
    Projector::native("", "Pod", true)
}

#[test]
fn un_pod_sano_va_en_verde_con_ready_y_reinicios() {
    let row = pods().project(&pod(json!({
        "phase": "Running", "podIP": "10.0.0.5",
        "containerStatuses": [
            { "name": "a", "ready": true, "restartCount": 1, "state": { "running": {} } },
            { "name": "b", "ready": true, "restartCount": 2, "state": { "running": {} } }
        ]
    })));
    assert_eq!(row.uid, "u1");
    assert_eq!(row.namespace.as_deref(), Some("prod"));
    assert_eq!(row.created_at.as_deref(), Some("2026-10-26T10:00:00Z"));
    assert_eq!(status_of(&row), (Tone::Ok, "Running".into()));
    assert_eq!(text(&row, "ready").as_deref(), Some("2/2"));
    assert_eq!(text(&row, "restarts").as_deref(), Some("3"));
    assert_eq!(text(&row, "node").as_deref(), Some("n1"));
    assert_eq!(text(&row, "labels").as_deref(), Some("labels"));
}

#[test]
fn estados_de_pod_como_kubectl() {
    let cases = [
        (
            json!({ "phase": "Running", "containerStatuses": [
                { "ready": false, "restartCount": 17, "state": { "waiting": { "reason": "CrashLoopBackOff" } } },
                { "ready": true, "state": { "running": {} } } ] }),
            Tone::Error,
            "CrashLoopBackOff",
        ),
        (
            json!({ "phase": "Pending", "containerStatuses": [
                { "ready": false, "state": { "waiting": { "reason": "ContainerCreating" } } } ] }),
            Tone::Progress,
            "ContainerCreating",
        ),
        (
            json!({ "phase": "Running", "containerStatuses": [
                { "ready": true, "state": { "running": {} } },
                { "ready": false, "state": { "running": {} } } ] }),
            Tone::Warning,
            "Running",
        ),
        (
            json!({ "phase": "Failed", "containerStatuses": [
                { "ready": false, "state": { "terminated": { "reason": "OOMKilled", "exitCode": 137 } } } ] }),
            Tone::Error,
            "OOMKilled",
        ),
        (
            json!({ "phase": "Succeeded", "containerStatuses": [
                { "ready": false, "state": { "terminated": { "reason": "Completed", "exitCode": 0 } } } ] }),
            Tone::Neutral,
            "Completed",
        ),
        (json!({ "phase": "Failed", "reason": "Evicted" }), Tone::Error, "Evicted"),
    ];
    for (status, tone, text) in cases {
        let row = pods().project(&pod(status));
        assert_eq!(status_of(&row), (tone, text.to_owned()), "caso {text}");
    }
}

#[test]
fn los_init_containers_mandan_mientras_arrancan() {
    let mut p = pod(json!({ "phase": "Pending", "initContainerStatuses": [
        { "state": { "terminated": { "exitCode": 0 } } },
        { "state": { "running": {} } } ] }));
    p.data["spec"]["initContainers"] = json!([{ "name": "i1" }, { "name": "i2" }]);
    assert_eq!(status_of(&pods().project(&p)), (Tone::Progress, "Init:1/2".into()));

    let mut failed = pod(json!({ "phase": "Pending", "initContainerStatuses": [
        { "state": { "waiting": { "reason": "CrashLoopBackOff" } } } ] }));
    failed.data["spec"]["initContainers"] = json!([{ "name": "i1" }]);
    assert_eq!(status_of(&pods().project(&failed)), (Tone::Error, "Init:CrashLoopBackOff".into()));
}

#[test]
fn un_objeto_borrandose_sale_como_terminating() {
    let mut p = pod(json!({ "phase": "Running" }));
    p.metadata.deletion_timestamp = p.metadata.creation_timestamp.clone();
    assert_eq!(status_of(&pods().project(&p)), (Tone::Warning, "Terminating".into()));
}

#[test]
fn deployments_segun_sus_replicas() {
    let dep = |spec: Value, status: Value| {
        Projector::native("apps", "Deployment", true).project(&obj(json!({
            "apiVersion": "apps/v1", "kind": "Deployment", "metadata": { "name": "d", "namespace": "n", "uid": "d1" },
            "spec": spec, "status": status
        })))
    };
    let ok = dep(
        json!({ "replicas": 3 }),
        json!({ "readyReplicas": 3, "updatedReplicas": 3, "availableReplicas": 3 }),
    );
    assert_eq!(status_of(&ok), (Tone::Ok, "Available".into()));
    assert_eq!(text(&ok, "ready").as_deref(), Some("3/3"));

    let rolling = dep(json!({ "replicas": 3 }), json!({ "readyReplicas": 2, "updatedReplicas": 1 }));
    assert_eq!(status_of(&rolling).0, Tone::Progress);
    let down = dep(json!({ "replicas": 0 }), json!({}));
    assert_eq!(status_of(&down), (Tone::Neutral, "Scaled down".into()));
    let broken = dep(json!({ "replicas": 2 }), json!({}));
    assert_eq!(status_of(&broken), (Tone::Error, "Unavailable".into()));
}

#[test]
fn servicios_nodos_y_eventos() {
    let svc = Projector::native("", "Service", true).project(&obj(json!({
        "apiVersion": "v1", "kind": "Service", "metadata": { "name": "s", "namespace": "n", "uid": "s1" },
        "spec": { "type": "LoadBalancer", "clusterIP": "10.96.0.10",
                  "ports": [{ "port": 80, "nodePort": 30080, "protocol": "TCP" }, { "port": 53, "protocol": "UDP" }] },
        "status": { "loadBalancer": { "ingress": [{ "ip": "34.1.2.3" }] } }
    })));
    assert_eq!(text(&svc, "ports").as_deref(), Some("80:30080/TCP,53/UDP"));
    assert_eq!(text(&svc, "externalIP").as_deref(), Some("34.1.2.3"));

    let node = Projector::native("", "Node", false).project(&obj(json!({
        "apiVersion": "v1", "kind": "Node",
        "metadata": { "name": "n1", "uid": "n1", "labels": { "node-role.kubernetes.io/control-plane": "" } },
        "spec": { "unschedulable": true },
        "status": { "conditions": [{ "type": "Ready", "status": "True" }],
                    "nodeInfo": { "kubeletVersion": "v1.32.0" },
                    "addresses": [{ "type": "InternalIP", "address": "172.18.0.2" }] }
    })));
    assert_eq!(status_of(&node), (Tone::Warning, "Ready,SchedulingDisabled".into()));
    assert_eq!(text(&node, "roles").as_deref(), Some("control-plane"));
    assert_eq!(text(&node, "internalIP").as_deref(), Some("172.18.0.2"));
    assert!(node.namespace.is_none());

    let ev = Projector::native("events.k8s.io", "Event", true).project(&obj(json!({
        "apiVersion": "events.k8s.io/v1", "kind": "Event", "metadata": { "name": "e", "namespace": "n", "uid": "e1" },
        "type": "Warning", "reason": "BackOff", "note": "Back-off restarting", "deprecatedCount": 17,
        "regarding": { "kind": "Pod", "name": "web-1" }
    })));
    assert_eq!(text(&ev, "object").as_deref(), Some("pod/web-1"));
    assert_eq!(text(&ev, "message").as_deref(), Some("Back-off restarting"));
    assert_eq!(text(&ev, "count").as_deref(), Some("17"));
    assert_eq!(status_of(&ev).0, Tone::Warning);
}

#[test]
fn las_columnas_enmarcan_las_del_tipo() {
    let ids: Vec<_> = pods().columns().iter().map(|c| c.id.clone()).collect();
    assert_eq!(ids, ["name", "namespace", "ready", "status", "restarts", "node", "ip", "age", "labels"]);
    assert!(!pods().columns().iter().find(|c| c.id == "ip").unwrap().default_visible);

    let cluster_scoped: Vec<_> =
        Projector::native("", "Namespace", false).columns().iter().map(|c| c.id.clone()).collect();
    assert!(!cluster_scoped.contains(&"namespace".to_owned()));

    let generic: Vec<_> = Projector::native("rbac.authorization.k8s.io", "Role", true)
        .columns()
        .iter()
        .map(|c| c.id.clone())
        .collect();
    assert_eq!(generic, ["name", "namespace", "age", "labels"]);
}

#[test]
fn columnas_de_crd_con_jsonpath() {
    let printer = vec![
        PrinterColumn {
            name: "Ready".into(),
            column_type: ColumnType::String,
            path: JsonPath::parse(r#".status.conditions[?(@.type=="Ready")].status"#).unwrap(),
            priority: 0,
        },
        PrinterColumn {
            name: "Replicas".into(),
            column_type: ColumnType::Number,
            path: JsonPath::parse(".spec.replicas").unwrap(),
            priority: 0,
        },
        PrinterColumn {
            name: "Detalle".into(),
            column_type: ColumnType::String,
            path: JsonPath::parse(".spec.extra").unwrap(),
            priority: 1,
        },
        PrinterColumn {
            name: "Age".into(),
            column_type: ColumnType::Date,
            path: JsonPath::parse(".metadata.creationTimestamp").unwrap(),
            priority: 0,
        },
    ];
    let p = Projector::printer(true, printer);
    let cols: Vec<_> = p
        .columns()
        .iter()
        .map(|c| (c.id.as_str(), c.label.as_str(), c.translate, c.default_visible))
        .collect();
    assert_eq!(
        cols,
        [
            ("name", "columns.name", true, true),
            ("namespace", "columns.namespace", true, true),
            ("p0", "Ready", false, true),
            ("p1", "Replicas", false, true),
            ("p2", "Detalle", false, false),
            ("age", "columns.age", true, true),
            ("labels", "columns.labels", true, false),
        ]
    );

    let row = p.project(&obj(json!({
        "apiVersion": "x.io/v1", "kind": "Widget", "metadata": { "name": "w", "namespace": "n", "uid": "w1" },
        "spec": { "replicas": "3", "extra": { "a": [1, 2] } },
        "status": { "conditions": [{ "type": "Ready", "status": "True" }] }
    })));
    assert_eq!(row.cells.get("p0"), Some(&CellValue::Text("True".into())));
    assert_eq!(row.cells.get("p1"), Some(&CellValue::Number(3.0)), "un número en texto se convierte");
    assert_eq!(row.cells.get("p2"), Some(&CellValue::Text(r#"{"a":[1,2]}"#.into())));
}
