//! Sesiones de clúster: una por contexto conectado, con su cliente, su estado y
//! su discovery. Un clúster que falla no afecta a los demás.

pub mod discovery;
pub mod errors;
pub mod exec;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use k8s_openapi::api::core::v1::Namespace;
use kube::api::{Api, ListParams};
use kube::{Client, Config};
use oculus_protocol::{
    ClusterId, ClusterStatus, ConnectStep, ErrorCode, KindRef, OculusError, ResourceKind, Scope, TunnelState,
    TunnelStatus,
};
use oculus_watch::{KindInfo, KubeSource, Subscriber, SubscriptionId, WatchManager, WatchOptions};

use crate::tunnel::{SshLauncher, Tunnel, TunnelManager, TunnelPhase, TunnelTimeouts};

#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    /// Conexión TCP/TLS: así un clúster inalcanzable falla rápido.
    pub connect: Duration,
    /// Primera petición (`/version`), que incluye el exec plugin.
    pub version: Duration,
    pub discovery: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(5),
            version: Duration::from_secs(10),
            discovery: Duration::from_secs(30),
        }
    }
}

/// Aplica los timeouts de Oculus a una configuración de kube-rs.
pub fn configure(mut config: Config, timeouts: &Timeouts) -> Config {
    config.connect_timeout = Some(timeouts.connect);
    config.read_timeout = Some(Duration::from_secs(30));
    config
}

pub struct ClusterSession {
    pub client: Client,
    pub server_version: String,
    pub kinds: Vec<ResourceKind>,
    pub default_namespace: String,
    /// Watchers de esta sesión; se cierran al desconectar.
    pub watches: WatchManager,
    /// Esquemas OpenAPI v3 por "apis/grupo/versión", para el editor.
    pub schemas: tokio::sync::Mutex<HashMap<String, Arc<serde_json::Value>>>,
    /// Túnel SSH por el que sale el cliente, si el clúster lo usa. Al soltar la
    /// sesión se suelta su parte del túnel.
    pub tunnel: Option<Arc<Tunnel>>,
}

/// Túnel que hay que levantar antes de conectar. El lanzador es asíncrono porque
/// necesita el entorno de la shell de login.
pub struct TunnelRequest {
    pub host: String,
    pub launcher: Pin<Box<dyn Future<Output = Result<SshLauncher, OculusError>> + Send>>,
}

impl ClusterSession {
    /// Se suscribe a un tipo descubierto en esta sesión.
    pub fn subscribe(
        &self,
        kind: &KindRef,
        scope: Scope,
        linger: Duration,
        subscriber: Subscriber,
    ) -> Result<SubscriptionId, OculusError> {
        let rk = self
            .kinds
            .iter()
            .find(|k| k.group == kind.group && k.version == kind.version && k.plural == kind.plural)
            .ok_or_else(|| {
                OculusError::new(ErrorCode::NotFound, format!("Tipo desconocido: {}", kind.plural))
            })?;
        let info = KindInfo {
            kind: kind.clone(),
            kind_name: rk.kind.clone(),
            namespaced: rk.namespaced,
            verbs: rk.verbs.clone(),
        };
        self.watches.subscribe(&info, scope, linger, subscriber)
    }

    /// Namespaces visibles. Si el usuario no puede listarlos, el del contexto.
    pub async fn namespaces(&self) -> Result<Vec<String>, OculusError> {
        let api: Api<Namespace> = Api::all(self.client.clone());
        match api.list_metadata(&ListParams::default()).await {
            Ok(list) => {
                let mut names: Vec<String> =
                    list.items.into_iter().filter_map(|ns| ns.metadata.name).collect();
                names.sort();
                Ok(names)
            }
            Err(kube::Error::Api(status)) if status.code == 403 => Ok(vec![self.default_namespace.clone()]),
            Err(err) => Err(errors::from_kube(&err)),
        }
    }
}

pub type StatusSink = Arc<dyn Fn(&ClusterId, &ClusterStatus) + Send + Sync>;

struct Entry {
    status: ClusterStatus,
    session: Option<Arc<ClusterSession>>,
    task: Option<tokio::task::AbortHandle>,
    /// Cada connect/disconnect lo incrementa: una tarea antigua que termine
    /// tarde no puede pisar el estado nuevo.
    generation: u64,
}

impl Default for Entry {
    fn default() -> Self {
        Self { status: ClusterStatus::Disconnected, session: None, task: None, generation: 0 }
    }
}

#[derive(Clone)]
pub struct ClusterManager {
    entries: Arc<Mutex<HashMap<ClusterId, Entry>>>,
    sink: StatusSink,
    timeouts: Timeouts,
    tunnels: TunnelManager,
}

impl ClusterManager {
    pub fn new(sink: StatusSink, timeouts: Timeouts) -> Self {
        let tunnels = TunnelManager::new(None, TunnelTimeouts::default());
        Self { entries: Arc::default(), sink, timeouts, tunnels }
    }

    pub fn with_tunnels(mut self, tunnels: TunnelManager) -> Self {
        self.tunnels = tunnels;
        self
    }

    pub fn status(&self, id: &str) -> ClusterStatus {
        self.lock().get(id).map(|e| e.status.clone()).unwrap_or(ClusterStatus::Disconnected)
    }

    pub fn session(&self, id: &str) -> Option<Arc<ClusterSession>> {
        self.lock().get(id).and_then(|e| e.session.clone())
    }

    pub fn timeouts(&self) -> Timeouts {
        self.timeouts
    }

    /// Lanza la conexión en segundo plano y vuelve enseguida; el progreso llega
    /// por el `StatusSink`. Si ya está conectando o conectado no hace nada.
    /// `prepare` construye el cliente (kubeconfig, exec plugin…).
    pub fn connect<F>(&self, id: ClusterId, default_namespace: String, prepare: F)
    where
        F: Future<Output = Result<Client, OculusError>> + Send + 'static,
    {
        self.connect_with(id, default_namespace, None, move |_| prepare);
    }

    /// Como `connect`, pero antes levanta el túnel (si hay) y pasa su `proxy_url`
    /// a `prepare`.
    pub fn connect_with<P, F>(
        &self,
        id: ClusterId,
        default_namespace: String,
        tunnel: Option<TunnelRequest>,
        prepare: P,
    ) where
        P: FnOnce(Option<http::Uri>) -> F + Send + 'static,
        F: Future<Output = Result<Client, OculusError>> + Send + 'static,
    {
        let generation = {
            let mut entries = self.lock();
            let entry = entries.entry(id.clone()).or_default();
            if matches!(entry.status, ClusterStatus::Connecting { .. } | ClusterStatus::Connected { .. }) {
                return;
            }
            entry.generation += 1;
            entry.generation
        };
        let step = if tunnel.is_some() { ConnectStep::Tunnel } else { ConnectStep::Auth };
        self.set_status(&id, generation, ClusterStatus::Connecting { step });

        let this = self.clone();
        let task_id = id.clone();
        let handle = tokio::spawn(async move {
            let result = this.run_connect(&task_id, generation, default_namespace, tunnel, prepare).await;
            match result {
                Ok(session) => {
                    let status = ClusterStatus::Connected {
                        server_version: session.server_version.clone(),
                        degraded: false,
                        tunnel: session
                            .tunnel
                            .as_ref()
                            .map(|t| TunnelStatus { host: t.host.clone(), state: TunnelState::Up }),
                    };
                    if let Some(tunnel) = &session.tunnel {
                        this.follow_tunnel(&task_id, generation, session.server_version.clone(), tunnel);
                    }
                    if let Some(entry) = this.lock().get_mut(&task_id).filter(|e| e.generation == generation)
                    {
                        entry.session = Some(Arc::new(session));
                    }
                    this.set_status(&task_id, generation, status);
                }
                Err(error) => {
                    tracing::info!(cluster = %task_id, code = ?error.code, "no se pudo conectar");
                    this.set_status(&task_id, generation, ClusterStatus::Error { error });
                }
            }
        });
        if let Some(entry) = self.lock().get_mut(&id).filter(|e| e.generation == generation) {
            entry.task = Some(handle.abort_handle());
        }
    }

    async fn run_connect<P, F>(
        &self,
        id: &ClusterId,
        generation: u64,
        default_namespace: String,
        tunnel: Option<TunnelRequest>,
        prepare: P,
    ) -> Result<ClusterSession, OculusError>
    where
        P: FnOnce(Option<http::Uri>) -> F,
        F: Future<Output = Result<Client, OculusError>>,
    {
        let tunnel = match tunnel {
            Some(request) => {
                let launcher = request.launcher.await?;
                let tunnel = self.tunnels.acquire(&request.host, &launcher).await?;
                self.set_status(id, generation, ClusterStatus::Connecting { step: ConnectStep::Auth });
                Some(tunnel)
            }
            None => None,
        };
        let client = prepare(tunnel.as_ref().map(|t| t.proxy_url())).await?;
        let explain = |error: OculusError| match &tunnel {
            Some(t) => explain_tunnel_error(error, t),
            None => error,
        };

        self.set_status(id, generation, ClusterStatus::Connecting { step: ConnectStep::Version });
        let secs = self.timeouts.version.as_secs();
        let version = tokio::time::timeout(self.timeouts.version, client.apiserver_version())
            .await
            .map_err(|_| errors::timeout("Conexión con el API server", secs))
            .and_then(|r| r.map_err(|e| errors::from_kube(&e)))
            .map_err(explain)?;

        self.set_status(id, generation, ClusterStatus::Connecting { step: ConnectStep::Discovery });
        let secs = self.timeouts.discovery.as_secs();
        let kinds = tokio::time::timeout(self.timeouts.discovery, discovery::discover(&client))
            .await
            .map_err(|_| errors::timeout("Discovery de recursos", secs))
            .and_then(|r| r.map_err(|e| errors::from_kube(&e)))
            .map_err(explain)?;

        let source = Arc::new(KubeSource { client: client.clone() });
        let watches = WatchManager::new(source, WatchOptions::default(), errors::from_kube);
        Ok(ClusterSession {
            client,
            server_version: version.git_version,
            kinds,
            default_namespace,
            watches,
            schemas: Default::default(),
            tunnel,
        })
    }

    /// Refleja en el estado del clúster lo que le pase al túnel: «reconnecting»
    /// mientras `ssh` se relanza y error si se abandona.
    fn follow_tunnel(&self, id: &ClusterId, generation: u64, server_version: String, tunnel: &Tunnel) {
        let mut phase = tunnel.phase();
        let host = tunnel.host.clone();
        let this = self.clone();
        let id = id.clone();
        tokio::spawn(async move {
            // Solo guarda el receptor: si la sesión suelta el túnel, el canal se cierra.
            while phase.changed().await.is_ok() {
                let current = phase.borrow_and_update().clone();
                let state = match current {
                    TunnelPhase::Failed(error) => {
                        this.fail_session(&id, generation, error);
                        return;
                    }
                    TunnelPhase::Up => TunnelState::Up,
                    TunnelPhase::Reconnecting => TunnelState::Reconnecting,
                };
                let status = ClusterStatus::Connected {
                    server_version: server_version.clone(),
                    degraded: state == TunnelState::Reconnecting,
                    tunnel: Some(TunnelStatus { host: host.clone(), state }),
                };
                this.set_status(&id, generation, status);
            }
        });
    }

    /// La sesión ya no sirve (el túnel se abandonó): se cierra y queda el error.
    fn fail_session(&self, id: &ClusterId, generation: u64, error: OculusError) {
        let session =
            self.lock().get_mut(id).filter(|e| e.generation == generation).and_then(|e| e.session.take());
        if let Some(session) = session {
            session.watches.shutdown();
        }
        self.set_status(id, generation, ClusterStatus::Error { error });
    }

    pub fn disconnect(&self, id: &str) {
        let generation = {
            let mut entries = self.lock();
            let Some(entry) = entries.get_mut(id) else { return };
            if let Some(task) = entry.task.take() {
                task.abort();
            }
            if let Some(session) = entry.session.take() {
                session.watches.shutdown();
            }
            entry.generation += 1;
            entry.generation
        };
        self.set_status(&id.to_owned(), generation, ClusterStatus::Disconnected);
    }

    /// Cierra todas las sesiones y mata los `ssh` de los túneles. Al salir de la app.
    pub fn shutdown(&self) {
        let ids: Vec<ClusterId> = self.lock().keys().cloned().collect();
        for id in ids {
            self.disconnect(&id);
        }
        self.tunnels.kill_all();
    }

    /// Error del paso previo a conectar (por ejemplo, el contexto ya no existe).
    pub fn fail(&self, id: ClusterId, error: OculusError) {
        let generation = {
            let mut entries = self.lock();
            let entry = entries.entry(id.clone()).or_default();
            entry.generation += 1;
            entry.generation
        };
        self.set_status(&id, generation, ClusterStatus::Error { error });
    }

    pub fn require_session(&self, id: &str) -> Result<Arc<ClusterSession>, OculusError> {
        self.session(id).ok_or_else(|| OculusError::new(ErrorCode::Invalid, "El clúster no está conectado"))
    }

    fn set_status(&self, id: &ClusterId, generation: u64, status: ClusterStatus) {
        {
            let mut entries = self.lock();
            let entry = entries.entry(id.clone()).or_default();
            if entry.generation != generation {
                return;
            }
            entry.status = status.clone();
        }
        (self.sink)(id, &status);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<ClusterId, Entry>> {
        self.entries.lock().expect("lock de sesiones envenenado")
    }
}

/// Un fallo de red con túnel suele ser del bastión, no del API server: si `ssh`
/// lo dijo, se cuenta en la pista.
fn explain_tunnel_error(mut error: OculusError, tunnel: &Tunnel) -> OculusError {
    if error.code != ErrorCode::Unreachable {
        return error;
    }
    let hint = if tunnel.stderr_contains("administratively prohibited") {
        "El bastión no permite reenvío de puertos (AllowTcpForwarding).".to_owned()
    } else if let Some(line) = tunnel.stderr_line("open failed") {
        format!("El bastión «{}» no pudo llegar al API server. ssh: {line}", tunnel.host)
    } else {
        return error;
    };
    error = error.with_hint(hint);
    error
}

#[cfg(test)]
mod tests;
