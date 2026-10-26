//! Columnas de los tipos nativos. Los textos de estado imitan a `kubectl get`.

use kube::api::DynamicObject;
use oculus_protocol::{CellValue, ColumnDef, ColumnType, Row, RowStatus, Tone};
use serde_json::Value;

use super::{NativeFn, col};

pub fn lookup(group: &str, kind: &str) -> (Vec<ColumnDef>, Option<NativeFn>) {
    use ColumnType::{Date, Number, String as Str};
    let c = |id: &str, t: ColumnType| col(id, t, true);
    let hidden = |id: &str, t: ColumnType| col(id, t, false);
    let (cols, f): (Vec<ColumnDef>, NativeFn) = match (group, kind) {
        ("", "Pod") => (
            vec![c("ready", Str), c("status", Str), c("restarts", Number), c("node", Str), hidden("ip", Str)],
            pod,
        ),
        ("apps", "Deployment") => (
            vec![c("ready", Str), c("upToDate", Number), c("available", Number), c("status", Str)],
            deployment,
        ),
        ("apps", "StatefulSet") => (vec![c("ready", Str), c("status", Str)], stateful_set),
        ("apps", "DaemonSet") => (
            vec![
                c("desired", Number),
                c("current", Number),
                c("ready", Number),
                c("upToDate", Number),
                c("available", Number),
            ],
            daemon_set,
        ),
        ("apps", "ReplicaSet") => {
            (vec![c("desired", Number), c("current", Number), c("ready", Number)], replica_set)
        }
        ("batch", "Job") => (vec![c("completions", Str), c("status", Str)], job),
        ("batch", "CronJob") => (
            vec![c("schedule", Str), c("suspend", Str), c("active", Number), c("lastSchedule", Date)],
            cron_job,
        ),
        ("", "Service") => {
            (vec![c("type", Str), c("clusterIP", Str), c("externalIP", Str), c("ports", Str)], service)
        }
        ("networking.k8s.io", "Ingress") => {
            (vec![c("class", Str), c("hosts", Str), c("address", Str)], ingress)
        }
        ("", "ConfigMap") => (vec![c("data", Number)], config_map),
        ("", "PersistentVolumeClaim") => (
            vec![
                c("status", Str),
                c("volume", Str),
                c("capacity", Str),
                c("accessModes", Str),
                c("storageClass", Str),
            ],
            pvc,
        ),
        ("", "PersistentVolume") => (
            vec![
                c("capacity", Str),
                c("accessModes", Str),
                c("reclaimPolicy", Str),
                c("status", Str),
                c("claim", Str),
                c("storageClass", Str),
            ],
            pv,
        ),
        ("", "Node") => {
            (vec![c("status", Str), c("roles", Str), c("version", Str), c("internalIP", Str)], node)
        }
        ("", "Namespace") => (vec![c("status", Str)], namespace),
        ("", "Event") | ("events.k8s.io", "Event") => (
            vec![
                c("type", Str),
                c("reason", Str),
                c("object", Str),
                c("message", Str),
                c("count", Number),
                c("lastSeen", Date),
            ],
            event,
        ),
        ("autoscaling", "HorizontalPodAutoscaler") => {
            (vec![c("reference", Str), c("min", Number), c("max", Number), c("replicas", Number)], hpa)
        }
        ("storage.k8s.io", "StorageClass") => {
            (vec![c("provisioner", Str), c("reclaimPolicy", Str), c("default", Str)], storage_class)
        }
        _ => return (Vec::new(), None),
    };
    (cols, Some(f))
}

// --- utilidades de lectura -------------------------------------------------

fn at<'a>(v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(v, |v, k| v.get(k))
}

fn num(v: &Value, path: &[&str]) -> Option<i64> {
    at(v, path)?.as_i64()
}

fn text(v: &Value, path: &[&str]) -> Option<String> {
    at(v, path)?.as_str().map(str::to_owned)
}

fn arr<'a>(v: &'a Value, path: &[&str]) -> impl Iterator<Item = &'a Value> {
    at(v, path).and_then(Value::as_array).into_iter().flatten()
}

fn put_text(row: &mut Row, id: &str, value: Option<String>) {
    if let Some(v) = value.filter(|v| !v.is_empty()) {
        row.cells.insert(id.into(), CellValue::Text(v));
    }
}

fn put_num(row: &mut Row, id: &str, value: Option<i64>) {
    if let Some(v) = value {
        row.cells.insert(id.into(), CellValue::Number(v as f64));
    }
}

fn status(row: &mut Row, tone: Tone, text: impl Into<String>) {
    row.status = Some(RowStatus { tone, text: text.into() });
}

fn ratio(ready: i64, total: i64) -> String {
    format!("{ready}/{total}")
}

fn condition<'a>(v: &'a Value, kind: &str) -> Option<&'a Value> {
    arr(v, &["status", "conditions"]).find(|c| c.get("type").and_then(Value::as_str) == Some(kind))
}

fn condition_true(v: &Value, kind: &str) -> bool {
    condition(v, kind).and_then(|c| c.get("status")).and_then(Value::as_str) == Some("True")
}

fn access_modes(modes: impl Iterator<Item = String>) -> String {
    modes
        .map(|m| match m.as_str() {
            "ReadWriteOnce" => "RWO".to_owned(),
            "ReadOnlyMany" => "ROX".to_owned(),
            "ReadWriteMany" => "RWX".to_owned(),
            "ReadWriteOncePod" => "RWOP".to_owned(),
            other => other.to_owned(),
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn lb_ingress(v: &Value) -> String {
    arr(v, &["status", "loadBalancer", "ingress"])
        .filter_map(|i| i.get("ip").or_else(|| i.get("hostname")).and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(",")
}

// --- tipos -----------------------------------------------------------------

/// Como `kubectl get pods`: motivo de espera o terminación del contenedor si
/// lo hay; si no, la fase.
pub(super) fn pod_status(v: &Value) -> String {
    let mut reason =
        text(v, &["status", "reason"]).or_else(|| text(v, &["status", "phase"])).unwrap_or_default();

    let init_total = arr(v, &["spec", "initContainers"]).count();
    for (i, cs) in arr(v, &["status", "initContainerStatuses"]).enumerate() {
        if let Some(t) = cs.pointer("/state/terminated") {
            if t.get("exitCode").and_then(Value::as_i64).unwrap_or(0) != 0 {
                let r = t.get("reason").and_then(Value::as_str).unwrap_or("Error");
                return format!("Init:{r}");
            }
            continue;
        }
        if let Some(r) =
            cs.pointer("/state/waiting/reason").and_then(Value::as_str).filter(|r| *r != "PodInitializing")
        {
            return format!("Init:{r}");
        }
        return format!("Init:{i}/{init_total}");
    }

    let mut running_ready = false;
    for cs in arr(v, &["status", "containerStatuses"]).collect::<Vec<_>>().into_iter().rev() {
        if let Some(r) = cs.pointer("/state/waiting/reason").and_then(Value::as_str) {
            reason = r.to_owned();
        } else if let Some(t) = cs.pointer("/state/terminated") {
            reason = match t.get("reason").and_then(Value::as_str) {
                Some(r) => r.to_owned(),
                None => match t.get("signal").and_then(Value::as_i64) {
                    Some(s) if s != 0 => format!("Signal:{s}"),
                    _ => format!("ExitCode:{}", t.get("exitCode").and_then(Value::as_i64).unwrap_or(0)),
                },
            };
        } else if cs.get("ready").and_then(Value::as_bool) == Some(true)
            && cs.pointer("/state/running").is_some()
        {
            running_ready = true;
        }
    }
    if reason == "Completed" && running_ready {
        reason = "Running".into();
    }
    reason
}

pub(super) fn tone_for(text: &str) -> Tone {
    let t = text.to_ascii_lowercase();
    if ["backoff", "err", "fail", "oomkilled", "evicted", "unknown", "notready", "lost", "signal", "exitcode"]
        .iter()
        .any(|k| t.contains(k))
    {
        Tone::Error
    } else if t.starts_with("init:")
        || ["pending", "creating", "initializing", "terminating", "progressing"].iter().any(|k| t.contains(k))
    {
        Tone::Progress
    } else if ["succeeded", "completed", "complete"].contains(&t.as_str()) {
        Tone::Neutral
    } else if ["running", "ready", "active", "bound", "available"].iter().any(|k| t.starts_with(k)) {
        Tone::Ok
    } else {
        Tone::Neutral
    }
}

fn pod(_: &DynamicObject, v: &Value, row: &mut Row) {
    let statuses: Vec<&Value> = arr(v, &["status", "containerStatuses"]).collect();
    let total = arr(v, &["spec", "containers"]).count() as i64;
    let ready =
        statuses.iter().filter(|c| c.get("ready").and_then(Value::as_bool) == Some(true)).count() as i64;
    let restarts: i64 = statuses.iter().filter_map(|c| c.get("restartCount").and_then(Value::as_i64)).sum();
    put_text(row, "ready", Some(ratio(ready, total)));
    put_num(row, "restarts", Some(restarts));
    put_text(row, "node", text(v, &["spec", "nodeName"]));
    put_text(row, "ip", text(v, &["status", "podIP"]));

    let text = pod_status(v);
    let mut tone = tone_for(&text);
    // Running pero con contenedores sin estar listos: aviso, no verde.
    if tone == Tone::Ok && ready < total {
        tone = Tone::Warning;
    }
    status(row, tone, text);
}

fn deployment(_: &DynamicObject, v: &Value, row: &mut Row) {
    let replicas = num(v, &["spec", "replicas"]).unwrap_or(1);
    let ready = num(v, &["status", "readyReplicas"]).unwrap_or(0);
    let updated = num(v, &["status", "updatedReplicas"]).unwrap_or(0);
    let available = num(v, &["status", "availableReplicas"]).unwrap_or(0);
    put_text(row, "ready", Some(ratio(ready, replicas)));
    put_num(row, "upToDate", Some(updated));
    put_num(row, "available", Some(available));
    workload_status(row, replicas, ready, updated);
}

fn workload_status(row: &mut Row, replicas: i64, ready: i64, updated: i64) {
    match () {
        _ if replicas == 0 => status(row, Tone::Neutral, "Scaled down"),
        _ if ready >= replicas && updated >= replicas => status(row, Tone::Ok, "Available"),
        _ if ready == 0 => status(row, Tone::Error, "Unavailable"),
        _ => status(row, Tone::Progress, "Progressing"),
    }
}

fn stateful_set(_: &DynamicObject, v: &Value, row: &mut Row) {
    let replicas = num(v, &["spec", "replicas"]).unwrap_or(1);
    let ready = num(v, &["status", "readyReplicas"]).unwrap_or(0);
    let updated = num(v, &["status", "updatedReplicas"]).unwrap_or(ready);
    put_text(row, "ready", Some(ratio(ready, replicas)));
    workload_status(row, replicas, ready, updated);
}

fn daemon_set(_: &DynamicObject, v: &Value, row: &mut Row) {
    let desired = num(v, &["status", "desiredNumberScheduled"]).unwrap_or(0);
    let ready = num(v, &["status", "numberReady"]).unwrap_or(0);
    put_num(row, "desired", Some(desired));
    put_num(row, "current", num(v, &["status", "currentNumberScheduled"]));
    put_num(row, "ready", Some(ready));
    put_num(row, "upToDate", num(v, &["status", "updatedNumberScheduled"]));
    put_num(row, "available", num(v, &["status", "numberAvailable"]));
    let updated = num(v, &["status", "updatedNumberScheduled"]).unwrap_or(0);
    workload_status(row, desired, ready, updated);
}

fn replica_set(_: &DynamicObject, v: &Value, row: &mut Row) {
    put_num(row, "desired", num(v, &["spec", "replicas"]));
    put_num(row, "current", num(v, &["status", "replicas"]));
    put_num(row, "ready", Some(num(v, &["status", "readyReplicas"]).unwrap_or(0)));
}

fn job(_: &DynamicObject, v: &Value, row: &mut Row) {
    let completions = num(v, &["spec", "completions"]).unwrap_or(1);
    let succeeded = num(v, &["status", "succeeded"]).unwrap_or(0);
    put_text(row, "completions", Some(ratio(succeeded, completions)));
    if condition_true(v, "Complete") {
        status(row, Tone::Neutral, "Complete");
    } else if condition_true(v, "Failed") {
        status(row, Tone::Error, "Failed");
    } else if at(v, &["spec", "suspend"]).and_then(Value::as_bool) == Some(true) {
        status(row, Tone::Neutral, "Suspended");
    } else {
        status(row, Tone::Progress, "Running");
    }
}

fn cron_job(_: &DynamicObject, v: &Value, row: &mut Row) {
    put_text(row, "schedule", text(v, &["spec", "schedule"]));
    let suspended = at(v, &["spec", "suspend"]).and_then(Value::as_bool).unwrap_or(false);
    put_text(row, "suspend", Some(suspended.to_string()));
    put_num(row, "active", Some(arr(v, &["status", "active"]).count() as i64));
    put_text(row, "lastSchedule", text(v, &["status", "lastScheduleTime"]));
}

fn service(_: &DynamicObject, v: &Value, row: &mut Row) {
    put_text(row, "type", text(v, &["spec", "type"]));
    put_text(row, "clusterIP", text(v, &["spec", "clusterIP"]));
    let mut external = lb_ingress(v);
    if external.is_empty() {
        external = arr(v, &["spec", "externalIPs"]).filter_map(Value::as_str).collect::<Vec<_>>().join(",");
    }
    put_text(row, "externalIP", Some(external));
    let ports = arr(v, &["spec", "ports"])
        .map(|p| {
            let port = p.get("port").and_then(Value::as_i64).unwrap_or(0);
            let proto = p.get("protocol").and_then(Value::as_str).unwrap_or("TCP");
            match p.get("nodePort").and_then(Value::as_i64) {
                Some(np) => format!("{port}:{np}/{proto}"),
                None => format!("{port}/{proto}"),
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    put_text(row, "ports", Some(ports));
}

fn ingress(_: &DynamicObject, v: &Value, row: &mut Row) {
    put_text(row, "class", text(v, &["spec", "ingressClassName"]));
    let hosts =
        arr(v, &["spec", "rules"]).filter_map(|r| r.get("host").and_then(Value::as_str)).collect::<Vec<_>>();
    put_text(row, "hosts", Some(if hosts.is_empty() { "*".to_owned() } else { hosts.join(",") }));
    put_text(row, "address", Some(lb_ingress(v)));
}

fn config_map(_: &DynamicObject, v: &Value, row: &mut Row) {
    let count = |k: &str| v.get(k).and_then(Value::as_object).map_or(0, |o| o.len());
    put_num(row, "data", Some((count("data") + count("binaryData")) as i64));
}

fn pvc(_: &DynamicObject, v: &Value, row: &mut Row) {
    let phase = text(v, &["status", "phase"]).unwrap_or_else(|| "Pending".into());
    put_text(row, "volume", text(v, &["spec", "volumeName"]));
    put_text(row, "capacity", text(v, &["status", "capacity", "storage"]));
    put_text(
        row,
        "accessModes",
        Some(access_modes(arr(v, &["status", "accessModes"]).filter_map(|m| m.as_str().map(str::to_owned)))),
    );
    put_text(row, "storageClass", text(v, &["spec", "storageClassName"]));
    status(row, tone_for(&phase), phase);
}

fn pv(_: &DynamicObject, v: &Value, row: &mut Row) {
    let phase = text(v, &["status", "phase"]).unwrap_or_default();
    put_text(row, "capacity", text(v, &["spec", "capacity", "storage"]));
    put_text(
        row,
        "accessModes",
        Some(access_modes(arr(v, &["spec", "accessModes"]).filter_map(|m| m.as_str().map(str::to_owned)))),
    );
    put_text(row, "reclaimPolicy", text(v, &["spec", "persistentVolumeReclaimPolicy"]));
    let claim = at(v, &["spec", "claimRef"]).map(|c| {
        format!(
            "{}/{}",
            c.get("namespace").and_then(Value::as_str).unwrap_or(""),
            c.get("name").and_then(Value::as_str).unwrap_or("")
        )
    });
    put_text(row, "claim", claim);
    put_text(row, "storageClass", text(v, &["spec", "storageClassName"]));
    let tone = match phase.as_str() {
        "Released" => Tone::Warning,
        p => tone_for(p),
    };
    status(row, tone, phase);
}

fn node(obj: &DynamicObject, v: &Value, row: &mut Row) {
    let ready = condition_true(v, "Ready");
    let unschedulable = at(v, &["spec", "unschedulable"]).and_then(Value::as_bool).unwrap_or(false);
    let mut text_status = if ready { "Ready".to_owned() } else { "NotReady".to_owned() };
    if unschedulable {
        text_status.push_str(",SchedulingDisabled");
    }
    let tone = match (ready, unschedulable) {
        (false, _) => Tone::Error,
        (true, true) => Tone::Warning,
        (true, false) => Tone::Ok,
    };
    status(row, tone, text_status);

    let roles: Vec<&str> = obj
        .metadata
        .labels
        .iter()
        .flatten()
        .filter_map(|(k, _)| k.strip_prefix("node-role.kubernetes.io/"))
        .filter(|r| !r.is_empty())
        .collect();
    put_text(row, "roles", Some(if roles.is_empty() { "<none>".to_owned() } else { roles.join(",") }));
    put_text(row, "version", text(v, &["status", "nodeInfo", "kubeletVersion"]));
    let ip = arr(v, &["status", "addresses"])
        .find(|a| a.get("type").and_then(Value::as_str) == Some("InternalIP"))
        .and_then(|a| a.get("address").and_then(Value::as_str))
        .map(str::to_owned);
    put_text(row, "internalIP", ip);
}

fn namespace(_: &DynamicObject, v: &Value, row: &mut Row) {
    let phase = text(v, &["status", "phase"]).unwrap_or_else(|| "Active".into());
    let tone = if phase == "Terminating" { Tone::Warning } else { Tone::Ok };
    status(row, tone, phase);
}

fn event(_: &DynamicObject, v: &Value, row: &mut Row) {
    let ty = text(v, &["type"]).unwrap_or_else(|| "Normal".into());
    let tone = if ty == "Warning" { Tone::Warning } else { Tone::Neutral };
    put_text(row, "type", Some(ty.clone()));
    put_text(row, "reason", text(v, &["reason"]));
    // core/v1 usa involvedObject/message/count; events.k8s.io, regarding/note/deprecatedCount.
    let target = v.get("involvedObject").or_else(|| v.get("regarding"));
    let object = target.map(|o| {
        format!(
            "{}/{}",
            o.get("kind").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase(),
            o.get("name").and_then(Value::as_str).unwrap_or("")
        )
    });
    put_text(row, "object", object);
    put_text(row, "message", text(v, &["message"]).or_else(|| text(v, &["note"])));
    put_num(row, "count", num(v, &["count"]).or_else(|| num(v, &["deprecatedCount"])).or(Some(1)));
    let last = text(v, &["lastTimestamp"])
        .or_else(|| text(v, &["series", "lastObservedTime"]))
        .or_else(|| text(v, &["eventTime"]))
        .or_else(|| text(v, &["deprecatedLastTimestamp"]));
    put_text(row, "lastSeen", last);
    status(row, tone, ty);
}

fn hpa(_: &DynamicObject, v: &Value, row: &mut Row) {
    let reference = at(v, &["spec", "scaleTargetRef"]).map(|r| {
        format!(
            "{}/{}",
            r.get("kind").and_then(Value::as_str).unwrap_or(""),
            r.get("name").and_then(Value::as_str).unwrap_or("")
        )
    });
    put_text(row, "reference", reference);
    put_num(row, "min", num(v, &["spec", "minReplicas"]).or(Some(1)));
    put_num(row, "max", num(v, &["spec", "maxReplicas"]));
    put_num(row, "replicas", num(v, &["status", "currentReplicas"]));
}

fn storage_class(obj: &DynamicObject, v: &Value, row: &mut Row) {
    put_text(row, "provisioner", text(v, &["provisioner"]));
    put_text(row, "reclaimPolicy", text(v, &["reclaimPolicy"]).or(Some("Delete".into())));
    let default = obj
        .metadata
        .annotations
        .as_ref()
        .and_then(|a| a.get("storageclass.kubernetes.io/is-default-class"))
        .is_some_and(|v| v == "true");
    put_text(row, "default", Some(if default { "true".into() } else { "false".into() }));
}
