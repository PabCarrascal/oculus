use std::sync::Mutex as StdMutex;
use std::time::Instant;

use http::{Request, Response};
use kube::client::Body;
use oculus_protocol::Category;

use super::*;

type Handle = tower_test::mock::Handle<Request<Body>, Response<Body>>;

#[derive(Clone, Copy)]
enum Fake {
    Healthy,
    NamespacesForbidden,
    /// Nunca responde: para probar timeouts.
    Silent,
}

/// API server mínimo: /version y el discovery clásico con unos pocos tipos.
fn fake_client(fake: Fake) -> Client {
    let (service, handle) = tower_test::mock::pair::<Request<Body>, Response<Body>>();
    tokio::spawn(serve(handle, fake));
    Client::new(service, "default")
}

async fn serve(mut handle: Handle, fake: Fake) {
    while let Some((request, send)) = handle.next_request().await {
        if matches!(fake, Fake::Silent) {
            // Se queda con la petición sin responder.
            std::mem::forget(send);
            continue;
        }
        let aggregated = request
            .headers()
            .get(http::header::ACCEPT)
            .is_some_and(|v| v.to_str().unwrap_or("").contains("apidiscovery"));
        let (status, body) = match (request.uri().path(), aggregated) {
            (_, true) => (
                404,
                serde_json::json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure", "message": "not found", "reason": "NotFound", "code": 404 }),
            ),
            ("/version", _) => (
                200,
                serde_json::json!({
                    "major": "1", "minor": "32", "gitVersion": "v1.32.0", "gitCommit": "", "gitTreeState": "",
                    "buildDate": "", "goVersion": "", "compiler": "", "platform": "linux/arm64"
                }),
            ),
            ("/api", _) => (
                200,
                serde_json::json!({ "kind": "APIVersions", "versions": ["v1"], "serverAddressByClientCIDRs": [] }),
            ),
            ("/apis", _) => {
                (200, serde_json::json!({ "kind": "APIGroupList", "apiVersion": "v1", "groups": [] }))
            }
            ("/api/v1", _) => (
                200,
                serde_json::json!({
                    "kind": "APIResourceList", "groupVersion": "v1",
                    "resources": [
                        { "name": "pods", "singularName": "pod", "namespaced": true, "kind": "Pod", "verbs": ["get", "list", "watch"] },
                        { "name": "namespaces", "singularName": "namespace", "namespaced": false, "kind": "Namespace", "verbs": ["get", "list"] }
                    ]
                }),
            ),
            ("/api/v1/namespaces", _) if matches!(fake, Fake::NamespacesForbidden) => (
                403,
                serde_json::json!({
                    "kind": "Status", "apiVersion": "v1", "status": "Failure", "message": "forbidden", "reason": "Forbidden", "code": 403
                }),
            ),
            ("/api/v1/namespaces", _) => (
                200,
                serde_json::json!({
                    "kind": "PartialObjectMetadataList", "apiVersion": "meta.k8s.io/v1", "metadata": {},
                    "items": [
                        { "apiVersion": "meta.k8s.io/v1", "kind": "PartialObjectMetadata", "metadata": { "name": "kube-system" } },
                        { "apiVersion": "meta.k8s.io/v1", "kind": "PartialObjectMetadata", "metadata": { "name": "default" } }
                    ]
                }),
            ),
            (path, _) => panic!("petición inesperada: {path}"),
        };
        let response =
            Response::builder().status(status).body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap();
        send.send_response(response);
    }
}

fn recording_manager(timeouts: Timeouts) -> (ClusterManager, Arc<StdMutex<Vec<ClusterStatus>>>) {
    let log = Arc::new(StdMutex::new(Vec::new()));
    let sink_log = log.clone();
    let sink: StatusSink = Arc::new(move |_, status| sink_log.lock().unwrap().push(status.clone()));
    (ClusterManager::new(sink, timeouts), log)
}

async fn wait_for(
    manager: &ClusterManager,
    id: &str,
    done: impl Fn(&ClusterStatus) -> bool,
) -> ClusterStatus {
    let start = Instant::now();
    loop {
        let status = manager.status(id);
        if done(&status) {
            return status;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "estado final no alcanzado: {status:?}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn is_final(s: &ClusterStatus) -> bool {
    matches!(s, ClusterStatus::Connected { .. } | ClusterStatus::Error { .. } | ClusterStatus::Disconnected)
}

#[tokio::test]
async fn conecta_pasando_por_todos_los_pasos() {
    let (manager, log) = recording_manager(Timeouts::default());
    manager.connect("c1".into(), "default".into(), async { Ok(fake_client(Fake::Healthy)) });

    let status = wait_for(&manager, "c1", is_final).await;
    assert_eq!(
        status,
        ClusterStatus::Connected { server_version: "v1.32.0".into(), degraded: false, tunnel: None }
    );
    assert_eq!(
        *log.lock().unwrap(),
        vec![
            ClusterStatus::Connecting { step: ConnectStep::Auth },
            ClusterStatus::Connecting { step: ConnectStep::Version },
            ClusterStatus::Connecting { step: ConnectStep::Discovery },
            status,
        ]
    );

    let session = manager.session("c1").unwrap();
    let pod = session.kinds.iter().find(|k| k.kind == "Pod").unwrap();
    assert!(pod.namespaced);
    assert_eq!(pod.category, Category::Workloads);
    assert_eq!(session.kinds.iter().find(|k| k.kind == "Namespace").unwrap().category, Category::Cluster);
    assert_eq!(session.namespaces().await.unwrap(), vec!["default", "kube-system"]);
}

#[tokio::test]
async fn conectar_dos_veces_no_lanza_una_segunda_conexion() {
    let (manager, log) = recording_manager(Timeouts::default());
    manager.connect("c1".into(), "default".into(), async { Ok(fake_client(Fake::Healthy)) });
    manager.connect("c1".into(), "default".into(), async { panic!("no debe ejecutarse") });
    wait_for(&manager, "c1", is_final).await;
    let auth_steps = log
        .lock()
        .unwrap()
        .iter()
        .filter(|s| **s == ClusterStatus::Connecting { step: ConnectStep::Auth })
        .count();
    assert_eq!(auth_steps, 1);
}

#[tokio::test]
async fn desconectar_a_medias_cancela_y_no_deja_rastro() {
    let (manager, log) = recording_manager(Timeouts::default());
    manager.connect("c1".into(), "default".into(), std::future::pending());
    manager.disconnect("c1");
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(manager.status("c1"), ClusterStatus::Disconnected);
    assert!(manager.session("c1").is_none());
    assert_eq!(log.lock().unwrap().last(), Some(&ClusterStatus::Disconnected));
}

#[tokio::test]
async fn un_error_al_preparar_queda_en_el_estado() {
    let (manager, _) = recording_manager(Timeouts::default());
    manager.connect("c1".into(), "default".into(), async {
        Err(OculusError::new(ErrorCode::Unauthorized, "No se encontró el exec plugin «aws»"))
    });
    let ClusterStatus::Error { error } = wait_for(&manager, "c1", is_final).await else { panic!() };
    assert_eq!(error.code, ErrorCode::Unauthorized);

    // Tras un error se puede reintentar.
    manager.connect("c1".into(), "default".into(), async { Ok(fake_client(Fake::Healthy)) });
    let status = wait_for(&manager, "c1", |s| matches!(s, ClusterStatus::Connected { .. })).await;
    assert!(matches!(status, ClusterStatus::Connected { .. }));
}

#[tokio::test]
async fn un_servidor_mudo_da_unreachable_al_vencer_el_timeout() {
    let timeouts = Timeouts { version: Duration::from_millis(200), ..Timeouts::default() };
    let (manager, _) = recording_manager(timeouts);
    let start = Instant::now();
    manager.connect("c1".into(), "default".into(), async { Ok(fake_client(Fake::Silent)) });

    let ClusterStatus::Error { error } = wait_for(&manager, "c1", is_final).await else { panic!() };
    assert_eq!(error.code, ErrorCode::Unreachable);
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn un_puerto_cerrado_falla_rapido_con_unreachable() {
    let (manager, _) = recording_manager(Timeouts::default());
    let start = Instant::now();
    manager.connect("c1".into(), "default".into(), async {
        let config = configure(Config::new("https://127.0.0.1:1".parse().unwrap()), &Timeouts::default());
        Client::try_from(config).map_err(|e| errors::from_kube(&e))
    });

    let ClusterStatus::Error { error } = wait_for(&manager, "c1", is_final).await else { panic!() };
    assert_eq!(error.code, ErrorCode::Unreachable, "{error:?}");
    assert!(error.details.and_then(|d| d.hint).is_some());
    assert!(start.elapsed() < Duration::from_secs(10), "el hito pide fallar en menos de 10 s");
}

#[tokio::test]
async fn sin_permiso_para_listar_namespaces_usa_el_del_contexto() {
    let (manager, _) = recording_manager(Timeouts::default());
    manager.connect("c1".into(), "equipo-a".into(), async { Ok(fake_client(Fake::NamespacesForbidden)) });
    wait_for(&manager, "c1", is_final).await;
    assert_eq!(manager.session("c1").unwrap().namespaces().await.unwrap(), vec!["equipo-a"]);
}
