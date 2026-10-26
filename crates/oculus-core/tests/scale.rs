//! Medidas de escala de la etapa 2 contra un clúster KWOK con miles de pods.
//! Ignorado por defecto:
//!
//!   tests/fixtures/kwok/setup.sh /tmp/oculus-perf.kubeconfig 10000
//!   OCULUS_SCALE_KUBECONFIG=/tmp/oculus-perf.kubeconfig \
//!     cargo test -p oculus-core --release --test scale -- --ignored --nocapture
//!
//! OCULUS_SCALE_CHURN_SECS (60 por defecto) y OCULUS_SCALE_CHURN_RATE (500/s).

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use k8s_openapi::api::core::v1::Pod;
use kube::api::{Api, Patch, PatchParams};
use kube::config::{KubeConfigOptions, Kubeconfig};
use oculus_core::cluster::StatusSink;
use oculus_core::service::ClusterService;
use oculus_core::shell_env::ShellEnv;
use oculus_protocol::{CellValue, ClusterStatus, KindRef, ResourceEvent, Row, Scope};
use oculus_watch::Subscriber;

#[derive(Default)]
struct Seen {
    rows: HashMap<String, Row>,
    snapshot_at: Option<Instant>,
    deltas: usize,
    upserts: usize,
}

fn rss_mb() -> f64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps");
    String::from_utf8_lossy(&out.stdout).trim().parse::<f64>().unwrap_or(0.0) / 1024.0
}

fn label(row: &Row, key: &str) -> Option<String> {
    match row.cells.get("labels")? {
        CellValue::Labels { labels } => labels.get(key).cloned(),
        _ => None,
    }
}

fn env_num(name: &str, default: u64) -> u64 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "necesita un clúster KWOK: OCULUS_SCALE_KUBECONFIG"]
async fn diez_mil_pods_snapshot_memoria_y_churn() {
    let path = std::env::var_os("OCULUS_SCALE_KUBECONFIG").expect("define OCULUS_SCALE_KUBECONFIG");
    let churn_secs = env_num("OCULUS_SCALE_CHURN_SECS", 60);
    let churn_rate = env_num("OCULUS_SCALE_CHURN_RATE", 500);

    let dir = tempfile::tempdir().unwrap();
    let sink: StatusSink = Arc::new(|_, _| {});
    let svc = ClusterService::with_fixed_env(
        dir.path().join("workspace.json"),
        sink,
        None,
        Some(OsString::from(&path)),
        ShellEnv::from_process(),
    );
    let id = svc.list_clusters()[0].id.clone();
    svc.connect(&id, vec![]).unwrap();
    let start = Instant::now();
    loop {
        match svc.list_clusters()[0].status.clone() {
            ClusterStatus::Connected { .. } => break,
            ClusterStatus::Error { error } => panic!("{error:?}"),
            _ if start.elapsed() > Duration::from_secs(30) => panic!("no conecta"),
            _ => tokio::time::sleep(Duration::from_millis(20)).await,
        }
    }

    let rss_before = rss_mb();
    let seen = Arc::new(Mutex::new(Seen::default()));
    let sub_seen = seen.clone();
    let subscriber: Subscriber = Arc::new(move |event| {
        let mut s = sub_seen.lock().unwrap();
        match event {
            ResourceEvent::Snapshot { rows, .. } => {
                s.rows = rows.iter().map(|r| (r.uid.clone(), r.clone())).collect();
                s.snapshot_at.get_or_insert_with(Instant::now);
            }
            ResourceEvent::Delta { upserts, deletes } => {
                s.deltas += 1;
                s.upserts += upserts.len();
                for uid in deletes {
                    s.rows.remove(uid);
                }
                for r in upserts {
                    s.rows.insert(r.uid.clone(), r.clone());
                }
            }
            ResourceEvent::Status { .. } => {}
        }
        true
    });

    let pods = KindRef { group: String::new(), version: "v1".into(), plural: "pods".into() };
    let subscribed = Instant::now();
    let sub = svc.subscribe(&id, &pods, Scope::All, Duration::from_secs(1), subscriber).unwrap();
    let snapshot_after = loop {
        if let Some(at) = seen.lock().unwrap().snapshot_at {
            break at - subscribed;
        }
        assert!(subscribed.elapsed() < Duration::from_secs(60), "no llegó el snapshot");
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    let total = seen.lock().unwrap().rows.len();
    let rss_snapshot = rss_mb();
    println!(
        "snapshot: {total} pods en {} ms | RSS {:.0} → {:.0} MB (+{:.0} MB)",
        snapshot_after.as_millis(),
        rss_before,
        rss_snapshot,
        rss_snapshot - rss_before
    );

    // Churn: cambia un label de pods al azar al ritmo pedido.
    let kubeconfig = Kubeconfig::read_from(&path).unwrap();
    let config =
        kube::Config::from_custom_kubeconfig(kubeconfig, &KubeConfigOptions::default()).await.unwrap();
    let client = kube::Client::try_from(config).unwrap();
    let names: Vec<(String, String)> = seen
        .lock()
        .unwrap()
        .rows
        .values()
        .map(|r| (r.namespace.clone().unwrap_or_default(), r.name.clone()))
        .collect();
    let last: Arc<Mutex<BTreeMap<(String, String), u64>>> = Arc::default();
    let limiter = Arc::new(tokio::sync::Semaphore::new(64));
    let mut tick = tokio::time::interval(Duration::from_micros(1_000_000 / churn_rate));
    let churn_start = Instant::now();
    let mut sent = 0u64;
    let errors = Arc::new(std::sync::atomic::AtomicU64::new(0));
    while churn_start.elapsed() < Duration::from_secs(churn_secs) {
        tick.tick().await;
        sent += 1;
        let (ns, name) = names[(sent as usize * 7919) % names.len()].clone();
        let permit = limiter.clone().acquire_owned().await.unwrap();
        let (client, last, errors) = (client.clone(), last.clone(), errors.clone());
        tokio::spawn(async move {
            let api: Api<Pod> = Api::namespaced(client, &ns);
            let patch = serde_json::json!({ "metadata": { "labels": { "churn": sent.to_string() } } });
            match api.patch(&name, &PatchParams::default(), &Patch::Merge(&patch)).await {
                Ok(_) => {
                    let mut l = last.lock().unwrap();
                    let e = l.entry((ns, name)).or_default();
                    *e = (*e).max(sent);
                }
                Err(_) => {
                    errors.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
            drop(permit);
        });
    }
    let _ = limiter.acquire_many(64).await.unwrap();
    let churn_elapsed = churn_start.elapsed();
    tokio::time::sleep(Duration::from_secs(2)).await;

    // En un bloque: el lock se suelta antes del siguiente await.
    let (rss_end, mismatched, rows_end) = {
        let s = seen.lock().unwrap();
        let rss_end = rss_mb();
        let applied: usize = last.lock().unwrap().len();
        let mismatched = last
            .lock()
            .unwrap()
            .iter()
            .filter(|((ns, name), v)| {
                !s.rows.values().any(|r| {
                    &r.name == name
                        && r.namespace.as_deref() == Some(ns)
                        && label(r, "churn") == Some(v.to_string())
                })
            })
            .count();
        println!(
            "churn: {sent} parches en {:.1} s ({:.0}/s, {} errores) → {} deltas, {} filas actualizadas | {} pods distintos tocados, {} desfasados | RSS final {:.0} MB",
            churn_elapsed.as_secs_f64(),
            sent as f64 / churn_elapsed.as_secs_f64(),
            errors.load(std::sync::atomic::Ordering::Relaxed),
            s.deltas,
            s.upserts,
            applied,
            mismatched,
            rss_end
        );
        (rss_end, mismatched, s.rows.len())
    };

    svc.unsubscribe(&id, &sub);
    tokio::time::sleep(Duration::from_secs(2)).await;

    assert!(total >= 1000, "el clúster tiene pocos pods: {total}");
    assert_eq!(rows_end, total, "ningún pod aparece ni desaparece por el churn");
    assert_eq!(mismatched, 0, "la vista termina igual que el clúster");
    assert!(snapshot_after < Duration::from_millis(1500), "hito: snapshot en < 1,5 s");
    assert!(rss_end - rss_before < 150.0, "hito: < 150 MB por clúster");
}
