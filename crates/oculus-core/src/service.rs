//! Fachada que usa la capa Tauri: une fuentes de kubeconfig, preferencias,
//! entorno de la shell y sesiones.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kube::config::KubeConfigOptions;
use kube::{Client, Config};
use oculus_protocol::{
    ClusterPrefs, ClusterSummary, ErrorCode, KindRef, KubeconfigSource, OculusError, ResourceKind, Scope,
    SourceOrigin,
};
use oculus_watch::{Subscriber, SubscriptionId};
use tokio::sync::OnceCell;

use crate::cluster::{self, ClusterManager, StatusSink, Timeouts, errors, exec};
use crate::kubeconfig::{Catalog, SourceSpec, discover_sources};
use crate::shell_env::{ShellEnv, effective_path};
use crate::workspace::WorkspaceStore;

pub struct ClusterService {
    workspace: WorkspaceStore,
    manager: ClusterManager,
    shell: Arc<OnceCell<ShellEnv>>,
    home: Option<PathBuf>,
    /// En tests se fija `KUBECONFIG` para no leer el entorno real del usuario.
    fixed_kubeconfig_env: Option<Option<OsString>>,
}

impl ClusterService {
    pub fn new(workspace_path: impl Into<PathBuf>, sink: StatusSink) -> Self {
        Self {
            workspace: WorkspaceStore::load(workspace_path),
            manager: ClusterManager::new(sink, Timeouts::default()),
            shell: Arc::default(),
            home: dirs::home_dir(),
            fixed_kubeconfig_env: None,
        }
    }

    #[doc(hidden)]
    pub fn with_fixed_env(
        workspace_path: impl Into<PathBuf>,
        sink: StatusSink,
        home: Option<PathBuf>,
        kubeconfig_env: Option<OsString>,
        shell: ShellEnv,
    ) -> Self {
        Self {
            workspace: WorkspaceStore::load(workspace_path),
            manager: ClusterManager::new(sink, Timeouts::default()),
            shell: Arc::new(OnceCell::new_with(Some(shell))),
            home,
            fixed_kubeconfig_env: Some(kubeconfig_env),
        }
    }

    /// Lee el entorno de la shell de login. Se lanza al arrancar en segundo
    /// plano; `connect` lo espera si aún no ha terminado.
    pub async fn init_shell_env(&self) -> bool {
        self.shell.get_or_init(ShellEnv::capture).await.from_login_shell
    }

    fn kubeconfig_env(&self) -> Option<OsString> {
        if let Some(fixed) = &self.fixed_kubeconfig_env {
            return fixed.clone();
        }
        // Desde una terminal el proceso ya lo trae; desde el Finder, solo la shell de login.
        std::env::var_os("KUBECONFIG").or_else(|| self.shell.get()?.get("KUBECONFIG").map(OsString::from))
    }

    pub fn source_specs(&self) -> Vec<SourceSpec> {
        discover_sources(
            self.kubeconfig_env().as_deref(),
            self.home.as_deref(),
            &self.workspace.manual_sources(),
        )
    }

    /// Rutas a vigilar para detectar cambios en los kubeconfig.
    pub fn watch_paths(&self) -> Vec<PathBuf> {
        self.source_specs().into_iter().map(|s| s.path).collect()
    }

    pub fn catalog(&self) -> Catalog {
        Catalog::load(&self.source_specs())
    }

    pub fn list_sources(&self) -> Vec<KubeconfigSource> {
        self.catalog().sources
    }

    pub fn list_clusters(&self) -> Vec<ClusterSummary> {
        self.catalog()
            .contexts
            .into_iter()
            .map(|ctx| ClusterSummary {
                prefs: self.workspace.prefs(&ctx.id),
                status: self.manager.status(&ctx.id),
                id: ctx.id,
                context_name: ctx.context_name,
                source_id: ctx.source_id,
                server: ctx.server,
                default_namespace: ctx.default_namespace,
            })
            .collect()
    }

    pub fn add_source(&self, path: &Path) -> Result<KubeconfigSource, OculusError> {
        self.workspace.add_manual_source(path)?;
        let id = crate::kubeconfig::source_id(path);
        self.list_sources()
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| OculusError::internal("la fuente añadida no aparece en el catálogo"))
    }

    pub fn remove_source(&self, source_id: &str) -> Result<(), OculusError> {
        let catalog = self.catalog();
        let source = catalog
            .sources
            .iter()
            .find(|s| s.id == source_id)
            .ok_or_else(|| OculusError::new(ErrorCode::NotFound, "Fuente no encontrada"))?;
        if source.origin != SourceOrigin::Manual {
            return Err(OculusError::invalid("Solo se pueden quitar los ficheros añadidos desde Oculus")
                .with_hint("Los de KUBECONFIG o ~/.kube/config se gestionan fuera de la app."));
        }
        for ctx in catalog.contexts.iter().filter(|c| c.source_id == source_id) {
            self.manager.disconnect(&ctx.id);
        }
        self.workspace.remove_manual_source(Path::new(&source.path))
    }

    pub fn set_prefs(&self, id: &str, prefs: ClusterPrefs) -> Result<ClusterPrefs, OculusError> {
        self.workspace.set_prefs(id, prefs)
    }

    /// Vuelve enseguida; el progreso llega por el `StatusSink`. El kubeconfig se
    /// relee ahora, así que una rotación de credenciales se aplica al reconectar.
    pub fn connect(&self, id: &str, extra_path: Vec<String>) -> Result<(), OculusError> {
        let catalog = self.catalog();
        let entry = catalog.context(id).ok_or_else(|| {
            OculusError::new(ErrorCode::NotFound, "El contexto ya no existe en ningún kubeconfig")
        })?;
        let default_namespace = entry.default_namespace.clone().unwrap_or_else(|| "default".to_owned());
        let (kubeconfig, context) = catalog.kubeconfig_for(id).expect("el contexto existe en el catálogo");

        let shell = self.shell.clone();
        let timeouts = self.manager.timeouts();
        let prepare = async move {
            let env = shell.get_or_init(ShellEnv::capture).await;
            let path = effective_path(&extra_path, env);
            let mut kubeconfig = kubeconfig;
            exec::prepare_exec(&mut kubeconfig, &context, &path, env)?;
            let options = KubeConfigOptions { context: Some(context), ..Default::default() };
            let config = Config::from_custom_kubeconfig(kubeconfig, &options).await.map_err(|e| {
                OculusError::invalid(format!("kubeconfig no válido: {}", errors::scrub(&e.to_string())))
            })?;
            Client::try_from(cluster::configure(config, &timeouts)).map_err(|e| errors::from_kube(&e))
        };
        self.manager.connect(id.to_owned(), default_namespace, prepare);
        Ok(())
    }

    pub fn disconnect(&self, id: &str) {
        self.manager.disconnect(id);
    }

    pub fn discovery(&self, id: &str) -> Result<Vec<ResourceKind>, OculusError> {
        Ok(self.manager.require_session(id)?.kinds.clone())
    }

    pub async fn namespaces(&self, id: &str) -> Result<Vec<String>, OculusError> {
        self.manager.require_session(id)?.namespaces().await
    }

    pub fn subscribe(
        &self,
        id: &str,
        kind: &KindRef,
        scope: Scope,
        linger: std::time::Duration,
        subscriber: Subscriber,
    ) -> Result<SubscriptionId, OculusError> {
        self.manager.require_session(id)?.subscribe(kind, scope, linger, subscriber)
    }

    /// Si el clúster ya se desconectó no hay nada que dar de baja.
    pub fn unsubscribe(&self, id: &str, subscription: &str) {
        if let Some(session) = self.manager.session(id) {
            session.watches.unsubscribe(subscription);
        }
    }
}

#[cfg(test)]
mod tests {
    use oculus_protocol::ClusterStatus;

    use super::*;

    const KUBECONFIG: &str = r#"
apiVersion: v1
kind: Config
clusters: [{ name: c, cluster: { server: "https://127.0.0.1:1" } }]
users:
- name: u
  user:
    exec: { apiVersion: client.authentication.k8s.io/v1beta1, command: plugin-que-no-existe }
contexts: [{ name: ctx, context: { cluster: c, user: u, namespace: equipo } }]
"#;

    fn service(dir: &Path, kubeconfig_env: Option<OsString>) -> ClusterService {
        let sink: StatusSink = Arc::new(|_, _| {});
        ClusterService::with_fixed_env(
            dir.join("workspace.json"),
            sink,
            Some(dir.join("home")),
            kubeconfig_env,
            ShellEnv::default(),
        )
    }

    #[test]
    fn lista_los_contextos_con_sus_preferencias() {
        let dir = tempfile::tempdir().unwrap();
        let kc = dir.path().join("k.yaml");
        std::fs::write(&kc, KUBECONFIG).unwrap();
        let svc = service(dir.path(), Some(kc.clone().into_os_string()));

        let clusters = svc.list_clusters();
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].default_namespace.as_deref(), Some("equipo"));
        assert_eq!(clusters[0].status, ClusterStatus::Disconnected);

        svc.set_prefs(&clusters[0].id, ClusterPrefs { alias: Some("pruebas".into()), ..Default::default() })
            .unwrap();
        assert_eq!(svc.list_clusters()[0].prefs.alias.as_deref(), Some("pruebas"));
    }

    #[test]
    fn las_fuentes_de_kubeconfig_no_se_pueden_quitar_pero_las_manuales_si() {
        let dir = tempfile::tempdir().unwrap();
        let env_kc = dir.path().join("env.yaml");
        let manual = dir.path().join("manual.yaml");
        std::fs::write(&env_kc, KUBECONFIG).unwrap();
        std::fs::write(&manual, KUBECONFIG.replace("name: ctx", "name: otro")).unwrap();
        let svc = service(dir.path(), Some(env_kc.into_os_string()));

        let added = svc.add_source(&manual).unwrap();
        assert_eq!(added.origin, SourceOrigin::Manual);
        assert_eq!(svc.list_clusters().len(), 2);

        let env_id = &svc.list_sources()[0].id;
        assert_eq!(svc.remove_source(env_id).unwrap_err().code, ErrorCode::Invalid);
        svc.remove_source(&added.id).unwrap();
        assert_eq!(svc.list_clusters().len(), 1);
    }

    #[tokio::test]
    async fn un_exec_plugin_que_falta_se_ve_en_el_estado() {
        let dir = tempfile::tempdir().unwrap();
        let kc = dir.path().join("k.yaml");
        std::fs::write(&kc, KUBECONFIG).unwrap();
        let svc = service(dir.path(), Some(kc.into_os_string()));
        let id = svc.list_clusters()[0].id.clone();

        svc.connect(&id, vec![]).unwrap();
        let start = std::time::Instant::now();
        let status = loop {
            let s = svc.list_clusters()[0].status.clone();
            if matches!(s, ClusterStatus::Error { .. }) || start.elapsed().as_secs() > 5 {
                break s;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        };
        let ClusterStatus::Error { error } = status else { panic!("{status:?}") };
        assert_eq!(error.code, ErrorCode::Unauthorized);
        assert!(error.message.contains("plugin-que-no-existe"));
    }

    #[test]
    fn conectar_un_contexto_que_no_existe_da_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let svc = service(dir.path(), None);
        assert_eq!(svc.connect("nada", vec![]).unwrap_err().code, ErrorCode::NotFound);
    }

    /// Regresión: kube-rs mete `{cmd:?}` (con todas las variables de entorno que
    /// Oculus le pasa al plugin) en el texto del error. Nada de eso puede llegar
    /// al estado que ve la UI.
    #[cfg(unix)]
    #[tokio::test]
    async fn un_exec_plugin_que_falla_no_filtra_el_entorno_de_la_shell() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let plugin = dir.path().join("fake-auth");
        std::fs::write(
            &plugin,
            "#!/bin/sh\necho 'Error: Token has expired and refresh failed' >&2\nexit 1\n",
        )
        .unwrap();
        std::fs::set_permissions(&plugin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let kc = dir.path().join("k.yaml");
        std::fs::write(&kc, KUBECONFIG.replace("plugin-que-no-existe", plugin.to_str().unwrap())).unwrap();

        let sink: StatusSink = Arc::new(|_, _| {});
        let shell = ShellEnv::from_vars(&[("OCULUS_TEST_SECRET", "hunter2-no-debe-salir")]);
        let svc = ClusterService::with_fixed_env(
            dir.path().join("workspace.json"),
            sink,
            None,
            Some(kc.into_os_string()),
            shell,
        );
        let id = svc.list_clusters()[0].id.clone();
        svc.connect(&id, vec![]).unwrap();

        let start = std::time::Instant::now();
        let error = loop {
            if let ClusterStatus::Error { error } = svc.list_clusters()[0].status.clone() {
                break error;
            }
            assert!(start.elapsed().as_secs() < 10, "no llegó el error");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        };

        let everything = format!("{error:?}");
        assert!(!everything.contains("hunter2"), "fuga de secretos: {everything}");
        assert!(!everything.contains("OCULUS_TEST_SECRET"), "fuga del entorno: {everything}");
        assert!(!everything.contains("PATH="), "fuga del entorno: {everything}");
        assert_eq!(error.code, ErrorCode::Unauthorized);
        assert!(
            error.message.contains("Token has expired"),
            "la causa útil sí se muestra: {}",
            error.message
        );
    }
}
