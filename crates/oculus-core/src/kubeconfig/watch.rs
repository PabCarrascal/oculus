//! Vigila los ficheros kubeconfig y avisa (con antirrebote) cuando cambian.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use notify::{EventKind, RecursiveMode, Watcher};

use super::canonical;

const DEBOUNCE: Duration = Duration::from_millis(300);

/// Se vigila el directorio padre de cada fichero, no el fichero: muchos editores
/// (y `aws eks update-kubeconfig`) lo reemplazan con un rename y un watch sobre
/// el fichero original se perdería.
pub struct KubeconfigWatcher {
    _watcher: notify::RecommendedWatcher,
}

impl KubeconfigWatcher {
    pub fn start(paths: &[PathBuf], on_change: impl Fn() + Send + 'static) -> notify::Result<Self> {
        let targets: HashSet<PathBuf> = paths.iter().map(|p| canonical(p)).collect();
        let dirs: HashSet<PathBuf> = targets.iter().filter_map(|p| p.parent().map(PathBuf::from)).collect();

        let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
        let mut watcher = notify::recommended_watcher(tx)?;
        for dir in dirs.iter().filter(|d| d.is_dir()) {
            if let Err(err) = watcher.watch(dir, RecursiveMode::NonRecursive) {
                tracing::warn!(%err, dir = %dir.display(), "no se puede vigilar el directorio");
            }
        }

        std::thread::Builder::new().name("kubeconfig-watch".into()).spawn(move || {
            let relevant = |event: &notify::Result<notify::Event>| match event {
                Ok(e) => {
                    !matches!(e.kind, EventKind::Access(_))
                        && e.paths.iter().any(|p| targets.contains(&canonical(p)) || targets.contains(p))
                }
                Err(_) => false,
            };
            // Cuando se cierra el canal (se soltó el watcher) termina el hilo.
            while let Ok(event) = rx.recv() {
                if !relevant(&event) {
                    continue;
                }
                // Agrupa la ráfaga de eventos de una sola escritura en un aviso.
                loop {
                    match rx.recv_timeout(DEBOUNCE) {
                        Ok(_) => continue,
                        Err(mpsc::RecvTimeoutError::Timeout) => break,
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                }
                on_change();
            }
        })?;

        Ok(Self { _watcher: watcher })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    use super::*;

    fn wait_until(cond: impl Fn() -> bool, max: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < max {
            if cond() {
                return true;
            }
            std::thread::park_timeout(Duration::from_millis(50));
        }
        cond()
    }

    #[test]
    fn avisa_una_vez_por_rafaga_e_ignora_otros_ficheros() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config");
        std::fs::write(&config, "a").unwrap();

        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        let _w = KubeconfigWatcher::start(std::slice::from_ref(&config), move || {
            counter.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();

        std::fs::write(dir.path().join("otro.txt"), "x").unwrap();
        // Reemplazo con rename, como hacen los editores.
        for i in 0..3 {
            let tmp = dir.path().join("config.tmp");
            std::fs::write(&tmp, format!("v{i}")).unwrap();
            std::fs::rename(&tmp, &config).unwrap();
        }

        assert!(wait_until(|| hits.load(Ordering::SeqCst) >= 1, Duration::from_secs(5)), "no llegó el aviso");
        std::thread::park_timeout(DEBOUNCE * 3);
        assert_eq!(hits.load(Ordering::SeqCst), 1, "la ráfaga debe dar un solo aviso");
    }
}
