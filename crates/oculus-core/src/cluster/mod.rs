//! Sesiones de clúster: una por contexto conectado, con su cliente, su estado y
//! su discovery. Un clúster que falla no afecta a los demás.

pub mod discovery;
pub mod errors;
pub mod exec;

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use k8s_openapi::api::core::v1::Namespace;
use kube::api::{Api, ListParams};
use kube::{Client, Config};
use oculus_protocol::{ClusterId, ClusterStatus, ConnectStep, ErrorCode, OculusError, ResourceKind};

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
}

impl ClusterSession {
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
}

impl ClusterManager {
    pub fn new(sink: StatusSink, timeouts: Timeouts) -> Self {
        Self { entries: Arc::default(), sink, timeouts }
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
        let generation = {
            let mut entries = self.lock();
            let entry = entries.entry(id.clone()).or_default();
            if matches!(entry.status, ClusterStatus::Connecting { .. } | ClusterStatus::Connected { .. }) {
                return;
            }
            entry.generation += 1;
            entry.generation
        };
        self.set_status(&id, generation, ClusterStatus::Connecting { step: ConnectStep::Auth });

        let this = self.clone();
        let task_id = id.clone();
        let handle = tokio::spawn(async move {
            let result = this.run_connect(&task_id, generation, default_namespace, prepare).await;
            match result {
                Ok(session) => {
                    let status = ClusterStatus::Connected {
                        server_version: session.server_version.clone(),
                        degraded: false,
                    };
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

    async fn run_connect<F>(
        &self,
        id: &ClusterId,
        generation: u64,
        default_namespace: String,
        prepare: F,
    ) -> Result<ClusterSession, OculusError>
    where
        F: Future<Output = Result<Client, OculusError>>,
    {
        let client = prepare.await?;

        self.set_status(id, generation, ClusterStatus::Connecting { step: ConnectStep::Version });
        let secs = self.timeouts.version.as_secs();
        let version = tokio::time::timeout(self.timeouts.version, client.apiserver_version())
            .await
            .map_err(|_| errors::timeout("Conexión con el API server", secs))?
            .map_err(|e| errors::from_kube(&e))?;

        self.set_status(id, generation, ClusterStatus::Connecting { step: ConnectStep::Discovery });
        let secs = self.timeouts.discovery.as_secs();
        let kinds = tokio::time::timeout(self.timeouts.discovery, discovery::discover(&client))
            .await
            .map_err(|_| errors::timeout("Discovery de recursos", secs))?
            .map_err(|e| errors::from_kube(&e))?;

        Ok(ClusterSession { client, server_version: version.git_version, kinds, default_namespace })
    }

    pub fn disconnect(&self, id: &str) {
        let generation = {
            let mut entries = self.lock();
            let Some(entry) = entries.get_mut(id) else { return };
            if let Some(task) = entry.task.take() {
                task.abort();
            }
            entry.session = None;
            entry.generation += 1;
            entry.generation
        };
        self.set_status(&id.to_owned(), generation, ClusterStatus::Disconnected);
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

#[cfg(test)]
mod tests;
