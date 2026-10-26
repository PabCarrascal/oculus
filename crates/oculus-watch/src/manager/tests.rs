use std::sync::atomic::{AtomicBool, Ordering};

use futures::FutureExt;
use futures::future::BoxFuture;
use kube::core::Status;
use oculus_protocol::KindRef;
use serde_json::json;
use tokio::sync::mpsc;

use super::*;
use crate::source::{EventStream, Opened};

type Item = (usize, Result<Event<DynamicObject>, watcher::Error>);

/// Fuente controlada por el test: cada `open` crea un canal por el que el test empuja eventos.
#[derive(Default)]
struct FakeSource {
    senders: Mutex<Vec<mpsc::UnboundedSender<Item>>>,
    streams_per_open: Mutex<Vec<usize>>,
}

impl Source for FakeSource {
    fn open(&self, _info: &KindInfo, scope: &Scope) -> BoxFuture<'static, Opened> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.senders.lock().unwrap().push(tx);
        let streams = match scope {
            Scope::Namespaces { names } => names.len(),
            Scope::All => 1,
        };
        self.streams_per_open.lock().unwrap().push(streams);
        let events: EventStream = tokio_stream_from(rx);
        async move { Opened { projector: Projector::native("", "Pod", true), streams, events } }.boxed()
    }
}

fn tokio_stream_from(mut rx: mpsc::UnboundedReceiver<Item>) -> EventStream {
    futures::stream::poll_fn(move |cx| rx.poll_recv(cx)).boxed()
}

impl FakeSource {
    fn opens(&self) -> usize {
        self.senders.lock().unwrap().len()
    }
    fn send(&self, open: usize, item: Item) {
        self.senders.lock().unwrap()[open].send(item).unwrap();
    }
}

#[derive(Clone, Default)]
struct Recorder {
    events: Arc<Mutex<Vec<ResourceEvent>>>,
    dead: Arc<AtomicBool>,
}

impl Recorder {
    fn subscriber(&self) -> Subscriber {
        let events = self.events.clone();
        let dead = self.dead.clone();
        Arc::new(move |e| {
            if dead.load(Ordering::SeqCst) {
                return false;
            }
            events.lock().unwrap().push(e.clone());
            true
        })
    }
    fn take(&self) -> Vec<ResourceEvent> {
        std::mem::take(&mut *self.events.lock().unwrap())
    }
}

fn map_error(e: &kube::Error) -> OculusError {
    match e {
        kube::Error::Api(s) if s.code == 403 => OculusError::new(ErrorCode::Forbidden, s.message.clone()),
        other => OculusError::new(ErrorCode::Unreachable, other.to_string()),
    }
}

fn info() -> KindInfo {
    KindInfo {
        kind: KindRef { group: String::new(), version: "v1".into(), plural: "pods".into() },
        kind_name: "Pod".into(),
        namespaced: true,
        verbs: vec!["list".into(), "watch".into()],
    }
}

fn manager(flush_ms: u64) -> (WatchManager, Arc<FakeSource>) {
    let source = Arc::new(FakeSource::default());
    let m = WatchManager::new(
        source.clone(),
        WatchOptions { flush_every: Duration::from_millis(flush_ms) },
        map_error,
    );
    (m, source)
}

fn pod(name: &str, ns: &str, phase: &str) -> DynamicObject {
    serde_json::from_value(json!({
        "apiVersion": "v1", "kind": "Pod",
        "metadata": { "name": name, "namespace": ns, "uid": format!("uid-{name}") },
        "spec": { "containers": [{ "name": "c" }] },
        "status": { "phase": phase }
    }))
    .unwrap()
}

/// Deja correr las tareas en segundo plano (con el reloj de tokio, pausado o no).
async fn settle() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    tokio::time::sleep(Duration::from_millis(30)).await;
}

async fn initial_list(source: &FakeSource, open: usize, stream: usize, pods: Vec<DynamicObject>) {
    source.send(open, (stream, Ok(Event::Init)));
    for p in pods {
        source.send(open, (stream, Ok(Event::InitApply(p))));
    }
    source.send(open, (stream, Ok(Event::InitDone)));
    settle().await;
}

fn names(event: &ResourceEvent) -> Vec<String> {
    let mut v: Vec<String> = match event {
        ResourceEvent::Snapshot { rows, .. } => rows.iter().map(|r| r.name.clone()).collect(),
        ResourceEvent::Delta { upserts, .. } => upserts.iter().map(|r| r.name.clone()).collect(),
        ResourceEvent::Status { .. } => vec![],
    };
    v.sort();
    v
}

fn state_of(event: &ResourceEvent) -> Option<WatchState> {
    match event {
        ResourceEvent::Status { state, .. } => Some(*state),
        _ => None,
    }
}

#[tokio::test]
async fn el_snapshot_llega_al_terminar_el_listado_inicial() {
    let (m, source) = manager(20);
    let rec = Recorder::default();
    m.subscribe(&info(), Scope::All, Duration::from_secs(30), rec.subscriber()).unwrap();
    settle().await;
    assert_eq!(state_of(&rec.take()[0]), Some(WatchState::Initializing));

    initial_list(&source, 0, 0, vec![pod("a", "n", "Running"), pod("b", "n", "Pending")]).await;
    let events = rec.take();
    let ResourceEvent::Snapshot { columns, rows } = &events[0] else { panic!("{events:?}") };
    assert_eq!(rows.len(), 2);
    assert!(columns.iter().any(|c| c.id == "restarts"), "columnas del proyector de pods");
    assert_eq!(state_of(&events[1]), Some(WatchState::Ready));
    assert_eq!(events.len(), 2, "no hay deltas duplicando el snapshot");
}

#[tokio::test]
async fn los_cambios_llegan_agrupados_y_sin_duplicados() {
    let (m, source) = manager(40);
    let rec = Recorder::default();
    m.subscribe(&info(), Scope::All, Duration::from_secs(30), rec.subscriber()).unwrap();
    settle().await;
    initial_list(&source, 0, 0, vec![pod("a", "n", "Pending"), pod("b", "n", "Running")]).await;
    rec.take();

    // Tres cambios del mismo pod, uno sin cambios reales, un alta y una baja.
    source.send(0, (0, Ok(Event::Apply(pod("a", "n", "Running")))));
    source.send(0, (0, Ok(Event::Apply(pod("a", "n", "Failed")))));
    source.send(0, (0, Ok(Event::Apply(pod("a", "n", "Succeeded")))));
    source.send(0, (0, Ok(Event::Apply(pod("b", "n", "Running")))));
    source.send(0, (0, Ok(Event::Apply(pod("c", "n", "Running")))));
    source.send(0, (0, Ok(Event::Delete(pod("b", "n", "Running")))));
    tokio::time::sleep(Duration::from_millis(120)).await;

    let events = rec.take();
    assert_eq!(events.len(), 1, "un solo lote: {events:?}");
    let ResourceEvent::Delta { upserts, deletes } = &events[0] else { panic!() };
    assert_eq!(names(&events[0]), ["a", "c"]);
    assert_eq!(upserts.iter().find(|r| r.name == "a").unwrap().status.as_ref().unwrap().text, "Succeeded");
    assert_eq!(deletes, &["uid-b".to_owned()]);
}

#[tokio::test]
async fn dos_vistas_comparten_un_watcher_y_la_segunda_recibe_snapshot() {
    let (m, source) = manager(20);
    let first = Recorder::default();
    m.subscribe(&info(), Scope::All, Duration::from_secs(30), first.subscriber()).unwrap();
    settle().await;
    initial_list(&source, 0, 0, vec![pod("a", "n", "Running")]).await;

    let second = Recorder::default();
    m.subscribe(&info(), Scope::All, Duration::from_secs(30), second.subscriber()).unwrap();
    settle().await;
    assert_eq!(source.opens(), 1, "mismo tipo y ámbito: un solo watcher");
    let events = second.take();
    assert_eq!(state_of(&events[0]), Some(WatchState::Ready));
    assert_eq!(names(&events[1]), ["a"]);

    // Otro ámbito es otra vigilancia.
    let other = Recorder::default();
    m.subscribe(
        &info(),
        Scope::Namespaces { names: vec!["n".into()] },
        Duration::from_secs(30),
        other.subscriber(),
    )
    .unwrap();
    settle().await;
    assert_eq!(source.opens(), 2);
}

#[tokio::test(start_paused = true)]
async fn sin_suscriptores_el_watcher_se_para_tras_el_linger() {
    let (m, source) = manager(80);
    let rec = Recorder::default();
    let id = m.subscribe(&info(), Scope::All, Duration::from_secs(30), rec.subscriber()).unwrap();
    settle().await;
    initial_list(&source, 0, 0, vec![pod("a", "n", "Running")]).await;

    m.unsubscribe(&id);
    tokio::time::sleep(Duration::from_secs(29)).await;
    assert_eq!(m.active_watches(), 1, "sigue vivo durante el linger");
    tokio::time::sleep(Duration::from_secs(2)).await;
    settle().await;
    assert_eq!(m.active_watches(), 0, "parado a los 30 s ± 1");

    // Volver dentro del linger reutiliza el watcher y no lo para.
    let id = m.subscribe(&info(), Scope::All, Duration::from_secs(30), rec.subscriber()).unwrap();
    settle().await;
    m.unsubscribe(&id);
    tokio::time::sleep(Duration::from_secs(10)).await;
    m.subscribe(&info(), Scope::All, Duration::from_secs(30), rec.subscriber()).unwrap();
    tokio::time::sleep(Duration::from_secs(60)).await;
    settle().await;
    assert_eq!(m.active_watches(), 1);
    assert_eq!(source.opens(), 2);
}

#[tokio::test(start_paused = true)]
async fn un_receptor_muerto_cuenta_como_baja() {
    let (m, source) = manager(80);
    let rec = Recorder::default();
    m.subscribe(&info(), Scope::All, Duration::from_secs(30), rec.subscriber()).unwrap();
    settle().await;
    initial_list(&source, 0, 0, vec![pod("a", "n", "Running")]).await;

    // El webview se recarga sin llamar a unsubscribe.
    rec.dead.store(true, Ordering::SeqCst);
    source.send(0, (0, Ok(Event::Apply(pod("a", "n", "Failed")))));
    tokio::time::sleep(Duration::from_millis(200)).await;
    tokio::time::sleep(Duration::from_secs(31)).await;
    settle().await;
    assert_eq!(m.active_watches(), 0);
}

#[tokio::test]
async fn un_relist_manda_solo_las_diferencias() {
    let (m, source) = manager(20);
    let rec = Recorder::default();
    m.subscribe(&info(), Scope::All, Duration::from_secs(30), rec.subscriber()).unwrap();
    settle().await;
    initial_list(&source, 0, 0, vec![pod("a", "n", "Running"), pod("b", "n", "Running")]).await;
    rec.take();

    // Tras un 410 el watcher vuelve a listar: "b" ya no existe y "c" es nuevo.
    initial_list(&source, 0, 0, vec![pod("a", "n", "Running"), pod("c", "n", "Pending")]).await;
    tokio::time::sleep(Duration::from_millis(60)).await;
    let events = rec.take();
    let states: Vec<_> = events.iter().filter_map(state_of).collect();
    assert_eq!(states, [WatchState::Relisting, WatchState::Ready]);
    let delta = events.iter().find(|e| matches!(e, ResourceEvent::Delta { .. })).expect("delta");
    let ResourceEvent::Delta { deletes, .. } = delta else { unreachable!() };
    assert_eq!(names(delta), ["c"]);
    assert_eq!(deletes, &["uid-b".to_owned()]);
    assert!(!events.iter().any(|e| matches!(e, ResourceEvent::Snapshot { .. })), "sin snapshot: no parpadea");
}

#[tokio::test]
async fn un_403_inicial_para_el_watcher_con_pista_y_un_fallo_de_red_reintenta() {
    let (m, source) = manager(20);
    let rec = Recorder::default();
    m.subscribe(&info(), Scope::All, Duration::from_secs(30), rec.subscriber()).unwrap();
    settle().await;
    let forbidden = Status::failure("pods is forbidden", "Forbidden").with_code(403);
    source.send(0, (0, Err(watcher::Error::InitialListFailed(kube::Error::Api(Box::new(forbidden))))));
    settle().await;
    let events = rec.take();
    let Some(ResourceEvent::Status { state: WatchState::Error, error: Some(e) }) = events.last() else {
        panic!("{events:?}")
    };
    assert_eq!(e.code, ErrorCode::Forbidden);
    assert!(e.details.as_ref().unwrap().hint.as_ref().unwrap().contains("namespaces"));

    // Un error de red no es definitivo: backoff y, con el siguiente evento, Ready.
    let rec = Recorder::default();
    m.subscribe(
        &info(),
        Scope::Namespaces { names: vec!["n".into()] },
        Duration::from_secs(30),
        rec.subscriber(),
    )
    .unwrap();
    settle().await;
    initial_list(&source, 1, 0, vec![pod("a", "n", "Running")]).await;
    let net = kube::Error::Service("connection reset".into());
    source.send(1, (0, Err(watcher::Error::WatchFailed(net))));
    settle().await;
    source.send(1, (0, Ok(Event::Apply(pod("a", "n", "Failed")))));
    settle().await;
    let states: Vec<_> = rec.take().iter().filter_map(state_of).collect();
    assert_eq!(states, [WatchState::Initializing, WatchState::Ready, WatchState::Backoff, WatchState::Ready]);
}

#[tokio::test]
async fn con_varios_namespaces_el_snapshot_espera_a_todos() {
    let (m, source) = manager(20);
    let rec = Recorder::default();
    let scope = Scope::Namespaces { names: vec!["b".into(), "a".into(), "a".into()] };
    m.subscribe(&info(), scope, Duration::from_secs(30), rec.subscriber()).unwrap();
    settle().await;
    assert_eq!(source.streams_per_open.lock().unwrap()[0], 2, "nombres ordenados y sin repetir");
    rec.take();

    initial_list(&source, 0, 0, vec![pod("x", "a", "Running")]).await;
    assert!(rec.take().is_empty(), "falta el otro namespace");
    initial_list(&source, 0, 1, vec![pod("y", "b", "Running")]).await;
    assert_eq!(names(&rec.take()[0]), ["x", "y"]);
}

#[tokio::test]
async fn al_desconectar_todos_reciben_un_error_y_no_se_admiten_altas() {
    let (m, source) = manager(20);
    let rec = Recorder::default();
    m.subscribe(&info(), Scope::All, Duration::from_secs(30), rec.subscriber()).unwrap();
    settle().await;
    initial_list(&source, 0, 0, vec![pod("a", "n", "Running")]).await;
    rec.take();

    m.shutdown();
    let events = rec.take();
    assert_eq!(state_of(&events[0]), Some(WatchState::Error));
    assert_eq!(m.active_watches(), 0);
    assert!(m.subscribe(&info(), Scope::All, Duration::from_secs(30), rec.subscriber()).is_err());
}

#[tokio::test]
async fn un_tipo_sin_watch_no_se_puede_suscribir() {
    let (m, _) = manager(20);
    let mut i = info();
    i.verbs = vec!["create".into()];
    assert_eq!(
        m.subscribe(&i, Scope::All, Duration::ZERO, Recorder::default().subscriber()).unwrap_err().code,
        ErrorCode::Invalid
    );
}
