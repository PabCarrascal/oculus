//! Vigilancias compartidas: una por (tipo, ámbito), con recuento de
//! suscriptores, linger y envío de cambios por lotes (D5).
//!
//! Solo se guardan filas proyectadas, nunca los objetos: 10.000 pods como
//! `serde_json::Value` no caben en el presupuesto de memoria de la etapa 2.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use futures::StreamExt;
use kube::api::DynamicObject;
use kube::runtime::watcher::{self, Event};
use oculus_protocol::{ColumnDef, ErrorCode, OculusError, ResourceEvent, Row, Scope, WatchState};

use crate::columns::Projector;
use crate::source::{KindInfo, Source};

/// Recibe los eventos de una suscripción. Devuelve `false` si el receptor ya
/// no existe (por ejemplo, el webview se recargó): cuenta como baja.
pub type Subscriber = Arc<dyn Fn(&ResourceEvent) -> bool + Send + Sync>;
pub type SubscriptionId = String;
/// Traduce errores de kube-rs sin filtrar secretos (lo pone `oculus-core`).
pub type ErrorMapper = fn(&kube::Error) -> OculusError;

#[derive(Debug, Clone, Copy)]
pub struct WatchOptions {
    /// Cada cuánto se envían los cambios acumulados.
    pub flush_every: Duration,
}

impl Default for WatchOptions {
    fn default() -> Self {
        Self { flush_every: Duration::from_millis(80) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct WatchKey {
    kind: oculus_protocol::KindRef,
    scope: Scope,
}

#[derive(Default)]
struct Inner {
    watches: HashMap<WatchKey, Arc<Shared>>,
    subs: HashMap<SubscriptionId, WatchKey>,
    next_id: u64,
    closed: bool,
}

struct Shared {
    key: WatchKey,
    state: Mutex<State>,
    task: Mutex<Option<tokio::task::AbortHandle>>,
}

#[derive(Default)]
struct StreamState {
    /// Filas de un listado en curso (inicial o relist).
    buffer: Option<HashMap<String, Row>>,
    listed: bool,
}

struct State {
    /// "group/version/plural" para los logs.
    label: String,
    opened_at: std::time::Instant,
    columns: Vec<ColumnDef>,
    /// uid → (stream, fila)
    rows: HashMap<String, (usize, Row)>,
    streams: Vec<StreamState>,
    /// Ya se envió el primer snapshot.
    initialized: bool,
    status: (WatchState, Option<OculusError>),
    pending_up: HashMap<String, Row>,
    pending_del: HashSet<String>,
    subscribers: HashMap<SubscriptionId, Subscriber>,
    linger: Duration,
    linger_pending: bool,
    /// Cambia con cada alta: un linger programado antes ya no vale.
    linger_gen: u64,
}

#[derive(Clone)]
pub struct WatchManager {
    inner: Arc<Mutex<Inner>>,
    source: Arc<dyn Source>,
    options: WatchOptions,
    map_error: ErrorMapper,
}

impl WatchManager {
    pub fn new(source: Arc<dyn Source>, options: WatchOptions, map_error: ErrorMapper) -> Self {
        Self { inner: Arc::default(), source, options, map_error }
    }

    /// Da de alta un suscriptor. Recibe el estado actual y, si la vigilancia ya
    /// tiene datos, un snapshot; después, los mismos deltas que los demás.
    pub fn subscribe(
        &self,
        info: &KindInfo,
        scope: Scope,
        linger: Duration,
        subscriber: Subscriber,
    ) -> Result<SubscriptionId, OculusError> {
        if !(info.verbs.iter().any(|v| v == "list") && info.verbs.iter().any(|v| v == "watch")) {
            return Err(OculusError::invalid(format!("{} no admite list y watch", info.kind_name)));
        }
        let key = WatchKey { kind: info.kind.clone(), scope: normalize(scope, info.namespaced) };

        let (id, shared) = {
            let mut inner = self.lock();
            if inner.closed {
                return Err(OculusError::invalid("El clúster no está conectado"));
            }
            inner.next_id += 1;
            let id = format!("w{}", inner.next_id);
            let shared = match inner.watches.get(&key) {
                Some(s) => s.clone(),
                None => {
                    let s = self.spawn(info, key.clone());
                    inner.watches.insert(key.clone(), s.clone());
                    s
                }
            };
            tracing::debug!(subscription = %id, kind = ?key.kind, scope = ?key.scope, "alta");
            inner.subs.insert(id.clone(), key);
            (id, shared)
        };

        // Bajo el lock: así ningún delta puede adelantar al snapshot de este suscriptor.
        let mut st = shared.lock();
        st.linger_gen += 1;
        st.linger_pending = false;
        st.linger = linger;
        let (state, error) = st.status.clone();
        let mut alive = subscriber(&ResourceEvent::Status { state, error });
        if alive && st.initialized {
            alive = subscriber(&snapshot(&st));
        }
        if alive {
            st.subscribers.insert(id.clone(), subscriber);
        } else {
            self.release_if_unused(&shared, &mut st);
        }
        Ok(id)
    }

    pub fn unsubscribe(&self, id: &str) {
        let shared = {
            let mut inner = self.lock();
            let Some(key) = inner.subs.remove(id) else { return };
            inner.watches.get(&key).cloned()
        };
        if let Some(shared) = shared {
            let mut st = shared.lock();
            st.subscribers.remove(id);
            self.release_if_unused(&shared, &mut st);
        }
    }

    /// Número de vigilancias vivas (para tests y diagnóstico).
    pub fn active_watches(&self) -> usize {
        self.lock().watches.len()
    }

    /// Cierra todo: la sesión se desconecta. Los suscriptores reciben un error final.
    pub fn shutdown(&self) {
        let watches: Vec<Arc<Shared>> = {
            let mut inner = self.lock();
            inner.closed = true;
            inner.subs.clear();
            inner.watches.drain().map(|(_, s)| s).collect()
        };
        let event = ResourceEvent::Status {
            state: WatchState::Error,
            error: Some(OculusError::new(ErrorCode::Unreachable, "El clúster se ha desconectado")),
        };
        for shared in watches {
            if let Some(task) = shared.task.lock().expect("lock de tarea").take() {
                task.abort();
            }
            let mut st = shared.lock();
            for sub in st.subscribers.values() {
                sub(&event);
            }
            st.subscribers.clear();
        }
    }

    fn spawn(&self, info: &KindInfo, key: WatchKey) -> Arc<Shared> {
        let shared = Arc::new(Shared {
            key: key.clone(),
            state: Mutex::new(State {
                label: format!("{}/{}/{}", key.kind.group, key.kind.version, key.kind.plural),
                opened_at: std::time::Instant::now(),
                columns: Vec::new(),
                rows: HashMap::new(),
                streams: Vec::new(),
                initialized: false,
                status: (WatchState::Initializing, None),
                pending_up: HashMap::new(),
                pending_del: HashSet::new(),
                subscribers: HashMap::new(),
                linger: Duration::ZERO,
                linger_pending: false,
                linger_gen: 0,
            }),
            task: Mutex::new(None),
        });
        let open = self.source.open(info, &key.scope);
        let task_shared = Arc::downgrade(&shared);
        let this = self.clone();
        let handle = tokio::spawn(async move {
            let opened = open.await;
            let Some(shared) = task_shared.upgrade() else { return };
            {
                let mut st = shared.lock();
                st.columns = opened.projector.columns().to_vec();
                st.streams = (0..opened.streams).map(|_| StreamState::default()).collect();
            }
            this.run(&shared, &opened.projector, opened.events).await;
        });
        *shared.task.lock().expect("lock de tarea") = Some(handle.abort_handle());
        shared
    }

    async fn run(&self, shared: &Arc<Shared>, projector: &Projector, mut events: crate::source::EventStream) {
        let mut tick = tokio::time::interval(self.options.flush_every);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                item = events.next() => {
                    let Some((stream, result)) = item else { break };
                    let mut st = shared.lock();
                    match result {
                        Ok(event) => apply(&mut st, stream, event, projector),
                        Err(err) => {
                            let fatal = is_fatal(&err);
                            let error = self.watch_error(&err);
                            let state = if fatal { WatchState::Error } else { WatchState::Backoff };
                            set_status(&mut st, state, Some(error));
                            if fatal {
                                tracing::warn!(kind = %st.label, "watch detenido por un error definitivo");
                                self.release_if_unused(shared, &mut st);
                                return;
                            }
                        }
                    }
                    self.release_if_unused(shared, &mut st);
                }
                _ = tick.tick() => {
                    let mut st = shared.lock();
                    flush(&mut st);
                    self.release_if_unused(shared, &mut st);
                }
            }
        }
    }

    fn watch_error(&self, err: &watcher::Error) -> OculusError {
        let mut error = match err {
            watcher::Error::InitialListFailed(e)
            | watcher::Error::WatchStartFailed(e)
            | watcher::Error::WatchFailed(e) => (self.map_error)(e),
            watcher::Error::WatchError(status) => (self.map_error)(&kube::Error::Api(status.clone())),
            watcher::Error::NoResourceVersion => OculusError::invalid("El recurso no admite watch"),
        };
        if error.code == ErrorCode::Forbidden {
            error = error.with_hint("No tienes permiso para listar este recurso en todo el clúster. Elige namespaces concretos en el filtro.");
        }
        error
    }

    /// Si ya no queda ningún suscriptor (bajas o receptores muertos), programa
    /// una sola parada diferida.
    fn release_if_unused(&self, shared: &Arc<Shared>, st: &mut State) {
        if !st.subscribers.is_empty() || st.linger_pending {
            return;
        }
        st.linger_pending = true;
        st.linger_gen += 1;
        let generation = st.linger_gen;
        let linger = st.linger;
        let inner = Arc::downgrade(&self.inner);
        let weak = Arc::downgrade(shared);
        tokio::spawn(async move {
            tokio::time::sleep(linger).await;
            expire(inner, weak, generation);
        });
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("lock de vigilancias")
    }
}

impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("lock de vigilancia")
    }
}

/// Fin del linger: si sigue sin suscriptores y nadie se ha dado de alta entre
/// medias, se para el watcher.
fn expire(inner: Weak<Mutex<Inner>>, shared: Weak<Shared>, generation: u64) {
    let (Some(inner), Some(shared)) = (inner.upgrade(), shared.upgrade()) else { return };
    let mut inner = inner.lock().expect("lock de vigilancias");
    {
        let st = shared.lock();
        if !st.subscribers.is_empty() || st.linger_gen != generation {
            return;
        }
    }
    if let Some(task) = shared.task.lock().expect("lock de tarea").take() {
        task.abort();
    }
    let key = shared.key.clone();
    inner.watches.remove(&key);
    inner.subs.retain(|_, k| *k != key);
    tracing::debug!(kind = ?key.kind, "vigilancia parada tras el linger");
}

fn normalize(scope: Scope, namespaced: bool) -> Scope {
    match scope {
        Scope::Namespaces { mut names } if namespaced && !names.is_empty() => {
            names.sort();
            names.dedup();
            Scope::Namespaces { names }
        }
        _ => Scope::All,
    }
}

fn is_fatal(err: &watcher::Error) -> bool {
    let api_code = match err {
        watcher::Error::InitialListFailed(kube::Error::Api(s))
        | watcher::Error::WatchStartFailed(kube::Error::Api(s)) => Some(s.code),
        _ => None,
    };
    // Sin permiso o el tipo ya no existe: reintentar no lo arregla.
    matches!(api_code, Some(401 | 403 | 404)) || matches!(err, watcher::Error::NoResourceVersion)
}

fn snapshot(st: &State) -> ResourceEvent {
    ResourceEvent::Snapshot {
        columns: st.columns.clone(),
        rows: st.rows.values().map(|(_, r)| r.clone()).collect(),
    }
}

fn send_all(st: &mut State, event: &ResourceEvent) {
    st.subscribers.retain(|_, sub| sub(event));
}

fn set_status(st: &mut State, state: WatchState, error: Option<OculusError>) {
    if st.status.0 == state && st.status.1 == error {
        return;
    }
    st.status = (state, error.clone());
    send_all(st, &ResourceEvent::Status { state, error });
}

fn flush(st: &mut State) {
    if !st.initialized || (st.pending_up.is_empty() && st.pending_del.is_empty()) {
        return;
    }
    let event = ResourceEvent::Delta {
        upserts: st.pending_up.drain().map(|(_, r)| r).collect(),
        deletes: st.pending_del.drain().collect(),
    };
    send_all(st, &event);
}

fn apply(st: &mut State, stream: usize, event: Event<DynamicObject>, projector: &Projector) {
    match event {
        Event::Init => {
            if let Some(s) = st.streams.get_mut(stream) {
                s.buffer = Some(HashMap::new());
            }
            if st.initialized {
                set_status(st, WatchState::Relisting, None);
            }
        }
        Event::InitApply(obj) => {
            let row = projector.project(&obj);
            if let Some(buf) = st.streams.get_mut(stream).and_then(|s| s.buffer.as_mut()) {
                buf.insert(row.uid.clone(), row);
            }
        }
        Event::InitDone => {
            let Some(buffer) = st.streams.get_mut(stream).and_then(|s| s.buffer.take()) else { return };
            st.streams[stream].listed = true;
            // Lo que había de este stream y ya no está, se borra; lo distinto, se actualiza.
            let gone: Vec<String> = st
                .rows
                .iter()
                .filter(|(uid, (s, _))| *s == stream && !buffer.contains_key(*uid))
                .map(|(u, _)| u.clone())
                .collect();
            for uid in gone {
                st.rows.remove(&uid);
                st.pending_up.remove(&uid);
                st.pending_del.insert(uid);
            }
            for (uid, row) in buffer {
                if st.rows.get(&uid).map(|(_, r)| r) != Some(&row) {
                    st.pending_del.remove(&uid);
                    st.pending_up.insert(uid.clone(), row.clone());
                    st.rows.insert(uid, (stream, row));
                }
            }
            if st.streams.iter().all(|s| s.listed && s.buffer.is_none()) {
                if !st.initialized {
                    st.initialized = true;
                    tracing::info!(
                        kind = %st.label,
                        rows = st.rows.len(),
                        ms = st.opened_at.elapsed().as_millis() as u64,
                        "primer snapshot"
                    );
                    st.pending_up.clear();
                    st.pending_del.clear();
                    let snap = snapshot(st);
                    send_all(st, &snap);
                }
                set_status(st, WatchState::Ready, None);
            }
        }
        Event::Apply(obj) => {
            let row = projector.project(&obj);
            if st.rows.get(&row.uid).map(|(_, r)| r) != Some(&row) {
                st.pending_del.remove(&row.uid);
                st.pending_up.insert(row.uid.clone(), row.clone());
                st.rows.insert(row.uid.clone(), (stream, row));
            }
            recover(st);
        }
        Event::Delete(obj) => {
            let row = projector.project(&obj);
            if st.rows.remove(&row.uid).is_some() {
                st.pending_up.remove(&row.uid);
                st.pending_del.insert(row.uid);
            }
            recover(st);
        }
    }
}

/// Tras un backoff, el primer evento bueno devuelve el estado a Ready.
fn recover(st: &mut State) {
    if st.status.0 == WatchState::Backoff && st.initialized && st.streams.iter().all(|s| s.buffer.is_none()) {
        set_status(st, WatchState::Ready, None);
    }
}

#[cfg(test)]
mod tests;
