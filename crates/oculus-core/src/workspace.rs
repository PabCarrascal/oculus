//! Estado del usuario sobre sus clústeres: ficheros añadidos a mano y
//! preferencias por clúster. Solo rutas y preferencias, nunca credenciales.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use oculus_protocol::{ClusterId, ClusterPrefs, OculusError, SshTunnelPrefs};
use serde::{Deserialize, Serialize};

use crate::kubeconfig::canonical;
use crate::persist;

const MAX_ALIAS: usize = 64;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct WorkspaceFile {
    manual_sources: Vec<PathBuf>,
    cluster_prefs: BTreeMap<ClusterId, ClusterPrefs>,
}

pub struct WorkspaceStore {
    path: PathBuf,
    current: RwLock<WorkspaceFile>,
}

impl WorkspaceStore {
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let current = persist::read_json_or(&path, WorkspaceFile::default);
        Self { path, current: RwLock::new(current) }
    }

    pub fn manual_sources(&self) -> Vec<PathBuf> {
        self.read().manual_sources.clone()
    }

    pub fn add_manual_source(&self, path: &Path) -> Result<(), OculusError> {
        if !path.is_file() {
            return Err(OculusError::invalid(format!("{} no es un fichero", path.display())));
        }
        let path = canonical(path);
        self.update(|w| {
            if !w.manual_sources.contains(&path) {
                w.manual_sources.push(path);
            }
            Ok(())
        })
    }

    pub fn remove_manual_source(&self, path: &Path) -> Result<(), OculusError> {
        let path = canonical(path);
        self.update(|w| {
            w.manual_sources.retain(|p| canonical(p) != path);
            Ok(())
        })
    }

    pub fn prefs(&self, id: &str) -> ClusterPrefs {
        self.read().cluster_prefs.get(id).cloned().unwrap_or_default()
    }

    pub fn set_prefs(&self, id: &str, prefs: ClusterPrefs) -> Result<ClusterPrefs, OculusError> {
        let prefs = normalize(prefs)?;
        let stored = prefs.clone();
        self.update(move |w| {
            if stored == ClusterPrefs::default() {
                w.cluster_prefs.remove(id);
            } else {
                w.cluster_prefs.insert(id.to_owned(), stored);
            }
            Ok(())
        })?;
        Ok(prefs)
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, WorkspaceFile> {
        self.current.read().expect("lock de workspace envenenado")
    }

    fn update(
        &self,
        f: impl FnOnce(&mut WorkspaceFile) -> Result<(), OculusError>,
    ) -> Result<(), OculusError> {
        let mut current = self.current.write().expect("lock de workspace envenenado");
        let mut next = current.clone();
        f(&mut next)?;
        persist::write_json_atomic(&self.path, &next)?;
        *current = next;
        Ok(())
    }
}

fn normalize(mut prefs: ClusterPrefs) -> Result<ClusterPrefs, OculusError> {
    prefs.alias = prefs.alias.map(|a| a.trim().to_owned()).filter(|a| !a.is_empty());
    if prefs.alias.as_ref().is_some_and(|a| a.chars().count() > MAX_ALIAS) {
        return Err(OculusError::invalid(format!("El alias no puede pasar de {MAX_ALIAS} caracteres")));
    }
    if let Some(color) = &prefs.color {
        let valid =
            color.len() == 7 && color.starts_with('#') && color[1..].chars().all(|c| c.is_ascii_hexdigit());
        if !valid {
            return Err(
                OculusError::invalid(format!("Color no válido: {color}")).with_hint("Usa el formato #rrggbb")
            );
        }
        prefs.color = Some(color.to_ascii_lowercase());
    }
    // Un host vacío equivale a quitar el túnel.
    prefs.ssh_tunnel = match prefs.ssh_tunnel {
        Some(t) if !t.host.trim().is_empty() => {
            Some(SshTunnelPrefs { host: crate::tunnel::validate_host(&t.host)? })
        }
        _ => None,
    };
    Ok(prefs)
}

#[cfg(test)]
mod tests {
    use oculus_protocol::ErrorCode;

    use super::*;

    fn store() -> (tempfile::TempDir, WorkspaceStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = WorkspaceStore::load(dir.path().join("workspace.json"));
        (dir, store)
    }

    #[test]
    fn las_preferencias_se_normalizan_y_persisten() {
        let (dir, store) = store();
        let prefs = ClusterPrefs {
            alias: Some("  prod EU ".into()),
            color: Some("#FF8800".into()),
            read_only: true,
            protected: true,
            ssh_tunnel: Some(SshTunnelPrefs { host: " ops@bastion:2222 ".into() }),
        };
        store.set_prefs("c1", prefs).unwrap();

        let reloaded = WorkspaceStore::load(dir.path().join("workspace.json"));
        let p = reloaded.prefs("c1");
        assert_eq!(p.alias.as_deref(), Some("prod EU"));
        assert_eq!(p.color.as_deref(), Some("#ff8800"));
        assert!(p.read_only && p.protected);
        assert_eq!(p.ssh_tunnel.unwrap().host, "ops@bastion:2222");
    }

    #[test]
    fn rechaza_colores_y_alias_invalidos() {
        let (_dir, store) = store();
        let bad_color = ClusterPrefs { color: Some("rojo".into()), ..Default::default() };
        assert_eq!(store.set_prefs("c1", bad_color).unwrap_err().code, ErrorCode::Invalid);
        let long = ClusterPrefs { alias: Some("x".repeat(65)), ..Default::default() };
        assert_eq!(store.set_prefs("c1", long).unwrap_err().code, ErrorCode::Invalid);
        let tunnel = |host: &str| ClusterPrefs {
            ssh_tunnel: Some(SshTunnelPrefs { host: host.into() }),
            ..Default::default()
        };
        assert_eq!(store.set_prefs("c1", tunnel("-oProxyCommand=x")).unwrap_err().code, ErrorCode::Invalid);
        assert_eq!(store.set_prefs("c1", tunnel("  ")).unwrap().ssh_tunnel, None);
    }

    #[test]
    fn volver_a_los_valores_por_defecto_borra_la_entrada() {
        let (dir, store) = store();
        store.set_prefs("c1", ClusterPrefs { read_only: true, ..Default::default() }).unwrap();
        store.set_prefs("c1", ClusterPrefs::default()).unwrap();
        let text = std::fs::read_to_string(dir.path().join("workspace.json")).unwrap();
        assert!(!text.contains("c1"));
    }

    #[test]
    fn las_fuentes_manuales_no_se_duplican_y_se_pueden_quitar() {
        let (dir, store) = store();
        let file = dir.path().join("k.yaml");
        std::fs::write(&file, "apiVersion: v1").unwrap();

        store.add_manual_source(&file).unwrap();
        store.add_manual_source(&file).unwrap();
        assert_eq!(store.manual_sources().len(), 1);

        store.remove_manual_source(&file).unwrap();
        assert!(store.manual_sources().is_empty());
        assert_eq!(store.add_manual_source(&dir.path().join("no")).unwrap_err().code, ErrorCode::Invalid);
    }
}
