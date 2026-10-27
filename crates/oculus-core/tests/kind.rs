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

/// Detalle y edición contra el clúster de `setup.sh` (namespace demo).
#[tokio::test]
#[ignore = "necesita el clúster de tests/fixtures/kind/setup.sh: OCULUS_IT_KUBECONFIG"]
async fn detalle_edicion_y_secret_contra_un_cluster_real() {
    use oculus_protocol::{ObjectRef, Relation};

    let kubeconfig = std::env::var_os("OCULUS_IT_KUBECONFIG").expect("define OCULUS_IT_KUBECONFIG");
    let (_dir, svc, _log) = service(kubeconfig);
    let id = svc.list_clusters()[0].id.clone();
    svc.connect(&id, vec![]).unwrap();
    let status = wait_final(&svc, &id, Duration::from_secs(60)).await;
    assert!(matches!(status, ClusterStatus::Connected { .. }), "{status:?}");

    let web = ObjectRef {
        group: "apps".into(),
        version: "v1".into(),
        plural: "deployments".into(),
        namespace: Some("demo".into()),
        name: "web".into(),
    };
    let started = Instant::now();
    let d = svc.detail(&id, &web).await.unwrap();
    println!(
        "detalle de web en {} ms: {} eventos, {} relacionados",
        started.elapsed().as_millis(),
        d.events.len(),
        d.related.len()
    );
    assert!(d.editable);
    assert!(!d.object_json.contains("managedFields"));
    assert!(d.related.iter().any(|r| r.relation == Relation::Selects && r.kind == "Pod"), "{:?}", d.related);

    // Dry-run de un cambio válido: no escribe. Siempre un valor distinto del
    // actual, para que el test no dependa de ejecuciones previas.
    let base: serde_json::Value = serde_json::from_str(&d.object_json).unwrap();
    let original = base["spec"]["replicas"].as_i64().unwrap_or(3);
    let mut edited = base.clone();
    edited["spec"]["replicas"] = serde_json::json!(original + 1);
    let (b, e) = (base.to_string(), edited.to_string());
    let ok = svc.update(&id, &web, &b, &e, true, false).await.unwrap();
    assert!(ok.object_json.contains(&format!("\"replicas\":{}", original + 1)));
    assert_eq!(
        svc.detail(&id, &web).await.unwrap().resource_version,
        d.resource_version,
        "un dry-run no escribe"
    );

    // Campo desconocido: error con la ruta exacta para marcar la línea.
    let mut bad = edited.clone();
    bad["spec"]["replicass"] = serde_json::json!(3);
    let err = svc.update(&id, &web, &b, &bad.to_string(), true, false).await.unwrap_err();
    let fields: Vec<_> =
        err.details.as_ref().unwrap().causes.iter().filter_map(|c| c.field.clone()).collect();
    println!("campo desconocido → {:?} {:?}", err.code, fields);
    assert!(fields.iter().any(|f| f == "spec.replicass"), "{fields:?}");

    // Valor inválido (422): causa con ruta.
    let mut invalid = base.clone();
    invalid["spec"]["replicas"] = serde_json::json!(-1);
    let err = svc.update(&id, &web, &b, &invalid.to_string(), true, false).await.unwrap_err();
    let fields: Vec<_> =
        err.details.as_ref().unwrap().causes.iter().filter_map(|c| c.field.clone()).collect();
    println!("réplicas negativas → {:?} {:?}", err.code, fields);
    assert!(fields.iter().any(|f| f == "spec.replicas"), "{fields:?}");

    // Aplicar de verdad y, en pleno escalado (el controlador reescribe el
    // estado sin parar), restaurar desde la misma base sin releer: con PUT y
    // resourceVersion esto daba 409; con el parche por campos debe funcionar.
    svc.update(&id, &web, &b, &e, false, false).await.unwrap();
    let restored = svc.update(&id, &web, &e, &b, false, false).await;
    println!(
        "restaurar durante el escalado sin releer → {:?}",
        restored.as_ref().map(|_| "ok").map_err(|e| e.code)
    );
    restored.unwrap();

    // Conflicto real: alguien cambia las réplicas por fuera; mi edición, basada
    // en el valor anterior, debe rechazarse y "sobrescribir" debe aplicarla.
    let mut theirs = base.clone();
    theirs["spec"]["replicas"] = serde_json::json!(original + 2);
    svc.update(&id, &web, &b, &theirs.to_string(), false, true).await.unwrap();
    let err = svc.update(&id, &web, &b, &e, false, false).await.unwrap_err();
    println!("edición sobre un campo que otro cambió → {:?}", err.code);
    assert_eq!(err.code, oculus_protocol::ErrorCode::Conflict);
    svc.update(&id, &web, &b, &e, false, true).await.unwrap();
    let now: serde_json::Value =
        serde_json::from_str(&svc.detail(&id, &web).await.unwrap().object_json).unwrap();
    assert_eq!(now["spec"]["replicas"].as_i64(), Some(original + 1), "sobrescribir aplica mi valor");
    // Se deja como estaba.
    svc.update(&id, &web, &e, &b, false, true).await.unwrap();

    // Secret: sin valores en el detalle, uno a uno bajo petición.
    let secret = ObjectRef {
        group: String::new(),
        version: "v1".into(),
        plural: "secrets".into(),
        namespace: Some("demo".into()),
        name: "credenciales".into(),
    };
    let s = svc.detail(&id, &secret).await.unwrap();
    assert!(!s.editable);
    assert_eq!(s.secret_keys.as_deref(), Some(&["token".to_owned()][..]));
    assert!(!s.object_json.contains("bm8tZXMtcmVhbA"), "el valor en base64 no sale");
    assert_eq!(svc.reveal_secret(&id, &secret, "token").await.unwrap(), "no-es-real");

    // Esquemas: nativo y de CRD.
    let started = Instant::now();
    let schema = svc.schema(&id, "apps", "v1", "Deployment").await.unwrap().expect("esquema de Deployment");
    println!(
        "esquema apps/v1 Deployment: {} KB en {} ms",
        schema.len() / 1024,
        started.elapsed().as_millis()
    );
    let doc: serde_json::Value = serde_json::from_str(&schema).unwrap();
    assert!(doc.pointer("/properties/spec").is_some(), "el tipo va en la raíz");
    assert!(doc.pointer("/components/schemas/io.k8s.api.apps.v1.DeploymentSpec").is_some());
    let started = Instant::now();
    svc.schema(&id, "apps", "v1", "Deployment").await.unwrap();
    println!("  (segunda vez, de caché: {} ms)", started.elapsed().as_millis());
    assert!(svc.schema(&id, "demo.oculus.test", "v1", "Gadget").await.unwrap().is_some());
}

/// El hito del túnel SSH (D13). Necesita además el bastión de
/// `tests/fixtures/ssh/setup.sh`:
///
///   OCULUS_IT_SSH_DIR=/ruta cargo test -p oculus-core --test kind tunel -- --ignored --nocapture
#[tokio::test]
#[ignore = "necesita kind y el bastión: OCULUS_IT_SSH_DIR"]
async fn tunel_ssh_hasta_un_cluster_real() {
    use oculus_protocol::{ClusterPrefs, ObjectRef, SshTunnelPrefs, TunnelState, TunnelStatus};

    let dir =
        std::path::PathBuf::from(std::env::var_os("OCULUS_IT_SSH_DIR").expect("define OCULUS_IT_SSH_DIR"));
    let (_tmp, svc, log) = service(dir.join("kind-internal.kubeconfig").into());
    let svc = svc.with_ssh_args(vec!["-F".into(), dir.join("ssh_config").display().to_string()]);
    let id = svc.list_clusters()[0].id.clone();
    let use_host = |host: &str| {
        let prefs =
            ClusterPrefs { ssh_tunnel: Some(SshTunnelPrefs { host: host.into() }), ..Default::default() };
        svc.set_prefs(&id, prefs).unwrap();
    };

    // Sin túnel, el nombre interno del API server no resuelve desde aquí.
    svc.connect(&id, vec![]).unwrap();
    let status = wait_final(&svc, &id, Duration::from_secs(30)).await;
    assert!(matches!(status, ClusterStatus::Error { .. }), "sin túnel no debería llegar: {status:?}");

    // Un usuario sin clave: Unauthorized con su pista, rápido.
    use_host("oculus-it-sinclave");
    let started = Instant::now();
    svc.connect(&id, vec![]).unwrap();
    let ClusterStatus::Error { error } = wait_final(&svc, &id, Duration::from_secs(20)).await else {
        panic!("debía fallar")
    };
    println!("sin clave → {:?} en {} ms", error.code, started.elapsed().as_millis());
    assert_eq!(error.code, ErrorCode::Unauthorized);
    assert!(error.details.unwrap().hint.unwrap().contains("ssh-add"));
    assert!(started.elapsed() < Duration::from_secs(15));

    // Con el bastión: túnel, autenticación, versión y discovery.
    use_host("oculus-it-bastion");
    log.lock().unwrap().clear();
    let started = Instant::now();
    svc.connect(&id, vec![]).unwrap();
    let status = wait_final(&svc, &id, Duration::from_secs(60)).await;
    let ClusterStatus::Connected { tunnel, .. } = &status else { panic!("{status:?}") };
    assert_eq!(tunnel, &Some(TunnelStatus { host: "oculus-it-bastion".into(), state: TunnelState::Up }));
    assert!(
        log.lock()
            .unwrap()
            .iter()
            .any(|(_, s)| *s == ClusterStatus::Connecting { step: ConnectStep::Tunnel })
    );
    println!("conectado por el túnel en {} ms", started.elapsed().as_millis());

    let web = ObjectRef {
        group: "apps".into(),
        version: "v1".into(),
        plural: "deployments".into(),
        namespace: Some("demo".into()),
        name: "web".into(),
    };
    let d = svc.detail(&id, &web).await.unwrap();
    let base: serde_json::Value = serde_json::from_str(&d.object_json).unwrap();
    let mut edited = base.clone();
    edited["spec"]["replicas"] = serde_json::json!(base["spec"]["replicas"].as_i64().unwrap_or(3) + 1);
    svc.update(&id, &web, &base.to_string(), &edited.to_string(), true, false).await.unwrap();

    // Se cae ssh: «reconnecting» y vuelve solo, sin reconectar el clúster.
    log.lock().unwrap().clear();
    // Anclado al principio: solo el ssh del túnel, no otro proceso que mencione el host.
    const OUR_SSH: &str = "^[^ ]*ssh -N -D 127[.]0[.]0[.]1:[0-9]+ .*-- oculus-it-bastion$";
    let killed = std::process::Command::new("pkill").args(["-f", OUR_SSH]).status().unwrap();
    assert!(killed.success(), "no había ningún ssh que matar");
    let started = Instant::now();
    let back_up = |s: &ClusterStatus| {
        matches!(
            s,
            ClusterStatus::Connected { tunnel: Some(TunnelStatus { state: TunnelState::Up, .. }), .. }
        )
    };
    loop {
        let seen_reconnecting = log.lock().unwrap().iter().any(|(_, s)| {
            matches!(
                s,
                ClusterStatus::Connected {
                    degraded: true,
                    tunnel: Some(TunnelStatus { state: TunnelState::Reconnecting, .. }),
                    ..
                }
            )
        });
        if seen_reconnecting && log.lock().unwrap().last().is_some_and(|(_, s)| back_up(s)) {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(20), "el túnel no volvió: {:?}", log.lock().unwrap());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    println!("túnel restablecido en {} ms", started.elapsed().as_millis());
    assert!(
        !log.lock().unwrap().iter().any(|(_, s)| matches!(s, ClusterStatus::Connecting { .. })),
        "no debía reconectar el clúster entero"
    );
    svc.detail(&id, &web).await.unwrap();
    assert!(!svc.namespaces(&id).await.unwrap().is_empty());

    // Al desconectar, ssh desaparece.
    svc.disconnect(&id);
    let started = Instant::now();
    loop {
        let alive = std::process::Command::new("pgrep").args(["-fl", OUR_SSH]).output().unwrap();
        if alive.stdout.is_empty() {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "quedó un ssh vivo: {}",
            String::from_utf8_lossy(&alive.stdout)
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    println!("ssh cerrado {} ms después de desconectar", started.elapsed().as_millis());
}
