use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use oculus_core::kubeconfig::watch::KubeconfigWatcher;
use oculus_core::service::ClusterService;
use oculus_core::settings::SettingsStore;
use tauri::AppHandle;
use tauri_specta::Event;

use crate::events::KubeconfigChanged;

pub struct AppState {
    pub started: Instant,
    /// Solo cuenta la primera pintura; las recargas del webview no la pisan.
    pub first_paint_ms: OnceLock<f64>,
    pub settings: SettingsStore,
    pub clusters: Arc<ClusterService>,
    watcher: Mutex<Option<KubeconfigWatcher>>,
    /// Suscripciones abiertas por el webview: (clúster, id).
    subscriptions: Mutex<Vec<(String, String)>>,
}

impl AppState {
    pub fn new(started: Instant, settings: SettingsStore, clusters: Arc<ClusterService>) -> Self {
        Self {
            started,
            first_paint_ms: OnceLock::new(),
            settings,
            clusters,
            watcher: Mutex::new(None),
            subscriptions: Mutex::new(Vec::new()),
        }
    }

    /// Vigila las fuentes actuales. Se llama al arrancar y cada vez que cambia
    /// la lista (fichero añadido o quitado, `KUBECONFIG` de la shell leído).
    pub fn restart_watcher(&self, app: &AppHandle) {
        let app = app.clone();
        let paths = self.clusters.watch_paths();
        let watcher = KubeconfigWatcher::start(&paths, move || {
            tracing::info!("kubeconfig modificado");
            let _ = KubeconfigChanged.emit(&app);
        });
        match watcher {
            Ok(w) => *self.watcher.lock().expect("lock del watcher") = Some(w),
            Err(err) => tracing::warn!(%err, "no se pueden vigilar los kubeconfig"),
        }
    }

    pub fn track_subscription(&self, cluster_id: &str, id: &str) {
        self.subs().push((cluster_id.to_owned(), id.to_owned()));
    }

    pub fn untrack_subscription(&self, id: &str) {
        self.subs().retain(|(_, s)| s != id);
    }

    pub fn take_subscriptions(&self) -> Vec<(String, String)> {
        std::mem::take(&mut *self.subs())
    }

    fn subs(&self) -> std::sync::MutexGuard<'_, Vec<(String, String)>> {
        self.subscriptions.lock().expect("lock de suscripciones")
    }
}
