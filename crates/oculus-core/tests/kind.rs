//! Integración contra un clúster real. Ignorados por defecto:
//!
//!   OCULUS_IT_KUBECONFIG=/ruta/kind.kubeconfig cargo test -p oculus-core --test kind -- --ignored --nocapture
//!
//! `tests/fixtures/kind/setup.sh` crea el clúster y los 100 CRDs del hito.

use std::ffi::OsString;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use oculus_core::cluster::StatusSink;
use oculus_core::service::ClusterService;
use oculus_core::shell_env::ShellEnv;
use oculus_protocol::{Category, ClusterStatus, ConnectStep, ErrorCode};

type Log = Arc<Mutex<Vec<(Instant, ClusterStatus)>>>;

fn service(kubeconfig: OsString) -> (tempfile::TempDir, ClusterService, Log) {
    let dir = tempfile::tempdir().unwrap();
    let log: Log = Arc::default();
    let sink_log = log.clone();
    let sink: StatusSink = Arc::new(move |_, s| sink_log.lock().unwrap().push((Instant::now(), s.clone())));
    let svc = ClusterService::with_fixed_env(
        dir.path().join("workspace.json"),
        sink,
        None,
        Some(kubeconfig),
        ShellEnv::from_process(),
    );
    (dir, svc, log)
}

async fn wait_final(svc: &ClusterService, id: &str, max: Duration) -> ClusterStatus {
    let start = Instant::now();
    loop {
        let status = svc.list_clusters().into_iter().find(|c| c.id == id).unwrap().status;
        if matches!(status, ClusterStatus::Connected { .. } | ClusterStatus::Error { .. })
            || start.elapsed() > max
        {
            return status;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn since(log: &Log, from: impl Fn(&ClusterStatus) -> bool) -> Duration {
    let log = log.lock().unwrap();
    let start = log.iter().find(|(_, s)| from(s)).expect("paso inicial").0;
    log.last().unwrap().0 - start
}

#[tokio::test]
#[ignore = "necesita un clúster: OCULUS_IT_KUBECONFIG"]
async fn conecta_a_un_cluster_real_y_descubre_sus_recursos() {
    let kubeconfig = std::env::var_os("OCULUS_IT_KUBECONFIG").expect("define OCULUS_IT_KUBECONFIG");
    let (_dir, svc, log) = service(kubeconfig);
    let id = svc.list_clusters()[0].id.clone();

    svc.connect(&id, vec![]).unwrap();
    let status = wait_final(&svc, &id, Duration::from_secs(60)).await;
    let ClusterStatus::Connected { server_version, .. } = &status else { panic!("{status:?}") };

    let total = since(&log, |s| matches!(s, ClusterStatus::Connecting { step: ConnectStep::Auth }));
    let discovery = since(&log, |s| matches!(s, ClusterStatus::Connecting { step: ConnectStep::Discovery }));
    let kinds = svc.discovery(&id).unwrap();
    let custom = kinds.iter().filter(|k| k.category == Category::Custom).count();
    let namespaces = svc.namespaces(&id).await.unwrap();
    println!(
        "servidor {server_version}: {} tipos ({custom} de CRDs), {} namespaces | conexión {} ms, discovery {} ms",
        kinds.len(),
        namespaces.len(),
        total.as_millis(),
        discovery.as_millis()
    );

    assert!(kinds.iter().any(|k| k.kind == "Pod" && k.category == Category::Workloads));
    assert!(kinds.iter().any(|k| k.kind == "Deployment" && k.group == "apps"));
    assert!(namespaces.contains(&"kube-system".to_owned()));
    if custom >= 100 {
        assert!(discovery < Duration::from_millis(1500), "hito: discovery < 1,5 s con 100 CRDs");
    }
}

#[tokio::test]
#[ignore = "red real: tarda hasta el timeout de conexión"]
async fn un_servidor_inalcanzable_falla_en_menos_de_10_segundos() {
    let dir = tempfile::tempdir().unwrap();
    let kc = dir.path().join("blackhole.yaml");
    // 10.255.255.1 no responde: el SYN se pierde y solo salva el timeout.
    std::fs::write(
        &kc,
        r#"
apiVersion: v1
kind: Config
clusters: [{ name: c, cluster: { server: "https://10.255.255.1:6443" } }]
users: [{ name: u, user: { token: t } }]
contexts: [{ name: blackhole, context: { cluster: c, user: u } }]
"#,
    )
    .unwrap();
    let (_d, svc, _) = service(kc.into_os_string());
    let id = svc.list_clusters()[0].id.clone();

    let start = Instant::now();
    svc.connect(&id, vec![]).unwrap();
    let status = wait_final(&svc, &id, Duration::from_secs(15)).await;
    let elapsed = start.elapsed();
    println!("inalcanzable: {status:?} en {} ms", elapsed.as_millis());

    let ClusterStatus::Error { error } = status else { panic!("debería fallar") };
    assert_eq!(error.code, ErrorCode::Unreachable);
    assert!(elapsed < Duration::from_secs(10), "hito: error claro en < 10 s");
}
