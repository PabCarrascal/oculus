//! Fuentes de kubeconfig y catálogo de contextos.
//!
//! Oculus nunca copia el contenido de un kubeconfig: guarda rutas y relee los
//! ficheros cada vez que hace falta (D9 en docs/01-arquitectura.md).

pub mod watch;

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use kube::config::Kubeconfig;
use oculus_protocol::{ClusterId, KubeconfigSource, SourceOrigin};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpec {
    pub path: PathBuf,
    pub origin: SourceOrigin,
}

/// Reglas de kubectl: si hay `KUBECONFIG` se usan sus rutas (en orden, sin las
/// vacías); si no, `~/.kube/config`. Los ficheros manuales van detrás. Una ruta
/// que aparece dos veces solo cuenta la primera.
pub fn discover_sources(
    env_value: Option<&OsStr>,
    home: Option<&Path>,
    manual: &[PathBuf],
) -> Vec<SourceSpec> {
    let mut specs = Vec::new();
    let env_paths: Vec<PathBuf> = env_value
        .map(|v| std::env::split_paths(v).filter(|p| !p.as_os_str().is_empty()).collect())
        .unwrap_or_default();

    if env_paths.is_empty() {
        if let Some(home) = home {
            specs.push(SourceSpec { path: home.join(".kube").join("config"), origin: SourceOrigin::Default });
        }
    } else {
        specs.extend(env_paths.into_iter().map(|path| SourceSpec { path, origin: SourceOrigin::Env }));
    }
    specs.extend(manual.iter().cloned().map(|path| SourceSpec { path, origin: SourceOrigin::Manual }));

    let mut seen = HashSet::new();
    specs.retain(|s| seen.insert(canonical(&s.path)));
    specs
}

/// Ruta absoluta y sin enlaces si el fichero existe; si no, la ruta tal cual.
pub fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn short_hash(parts: &[&[u8]], bytes: usize) -> String {
    let mut hasher = Sha256::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            hasher.update([0u8]);
        }
        hasher.update(part);
    }
    hex::encode(&hasher.finalize()[..bytes])
}

pub fn source_id(path: &Path) -> String {
    short_hash(&[canonical(path).as_os_str().as_encoded_bytes()], 6)
}

pub fn cluster_id(path: &Path, context: &str) -> ClusterId {
    short_hash(&[canonical(path).as_os_str().as_encoded_bytes(), context.as_bytes()], 8)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextEntry {
    pub id: ClusterId,
    pub source_id: String,
    pub context_name: String,
    pub server: Option<String>,
    pub default_namespace: Option<String>,
    /// El clúster ya define su propio `proxy-url`; un túnel SSH lo sustituiría.
    pub has_proxy_url: bool,
}

struct LoadedFile {
    origin: SourceOrigin,
    config: Kubeconfig,
}

/// Instantánea de todas las fuentes leídas en un momento dado.
pub struct Catalog {
    pub sources: Vec<KubeconfigSource>,
    pub contexts: Vec<ContextEntry>,
    files: HashMap<String, LoadedFile>,
    /// Ficheros de `KUBECONFIG` fusionados como hace kubectl: un contexto de un
    /// fichero puede usar un usuario definido en otro.
    env_merged: Option<Kubeconfig>,
}

impl Catalog {
    pub fn load(specs: &[SourceSpec]) -> Self {
        let mut sources = Vec::new();
        let mut files = HashMap::new();
        let mut order = Vec::new();
        let mut env_merged: Option<Kubeconfig> = None;

        for spec in specs {
            let id = source_id(&spec.path);
            let path = spec.path.display().to_string();
            match read(&spec.path) {
                Ok(config) => {
                    if spec.origin == SourceOrigin::Env {
                        env_merged = Some(match env_merged.take() {
                            None => config.clone(),
                            // merge solo falla si los ficheros piden cosas incompatibles;
                            // en ese caso nos quedamos con lo ya fusionado.
                            Some(acc) => acc.clone().merge(config.clone()).unwrap_or(acc),
                        });
                    }
                    sources.push(KubeconfigSource { id: id.clone(), path, origin: spec.origin, error: None });
                    order.push((id.clone(), spec.path.clone()));
                    files.insert(id, LoadedFile { origin: spec.origin, config });
                }
                Err(error) => {
                    sources.push(KubeconfigSource { id, path, origin: spec.origin, error: Some(error) });
                }
            }
        }

        let mut catalog = Self { sources, contexts: Vec::new(), files, env_merged };
        for (id, path) in order {
            let file = &catalog.files[&id];
            for named in &file.config.contexts {
                let Some(context) = &named.context else { continue };
                let cluster = catalog.resolve_view(&id).and_then(|view| {
                    let cluster_name = context.cluster.as_str();
                    view.clusters.iter().find(|c| c.name == cluster_name)?.cluster.clone()
                });
                let server = cluster.as_ref().and_then(|c| c.server.clone());
                let has_proxy_url = cluster.as_ref().is_some_and(|c| c.proxy_url.is_some());
                catalog.contexts.push(ContextEntry {
                    id: cluster_id(&path, &named.name),
                    source_id: id.clone(),
                    context_name: named.name.clone(),
                    server,
                    default_namespace: context.namespace.clone(),
                    has_proxy_url,
                });
            }
        }
        catalog
    }

    pub fn context(&self, id: &str) -> Option<&ContextEntry> {
        self.contexts.iter().find(|c| c.id == id)
    }

    /// Kubeconfig listo para conectar a ese contexto, sin escribir nada en disco.
    pub fn kubeconfig_for(&self, id: &str) -> Option<(Kubeconfig, String)> {
        let entry = self.context(id)?;
        let mut config = self.resolve_view(&entry.source_id)?;
        config.current_context = Some(entry.context_name.clone());
        Some((config, entry.context_name.clone()))
    }

    /// El propio fichero primero (para que sus entradas ganen) y, si viene de
    /// `KUBECONFIG`, el resto de esos ficheros detrás para resolver referencias.
    fn resolve_view(&self, source_id: &str) -> Option<Kubeconfig> {
        let file = self.files.get(source_id)?;
        match (&file.origin, &self.env_merged) {
            (SourceOrigin::Env, Some(merged)) => {
                Some(file.config.clone().merge(merged.clone()).unwrap_or_else(|_| file.config.clone()))
            }
            _ => Some(file.config.clone()),
        }
    }
}

fn read(path: &Path) -> Result<Kubeconfig, String> {
    if !path.exists() {
        return Err("El fichero no existe".to_owned());
    }
    // El error puede citar la línea que falla, y esa línea puede llevar un token.
    Kubeconfig::read_from(path).map_err(|err| crate::cluster::errors::scrub(&err.to_string()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const CLUSTER_A: &str = r#"
apiVersion: v1
kind: Config
clusters:
- name: a
  cluster: { server: "https://a.example:6443" }
users:
- name: alice
  user: { token: "t" }
contexts:
- name: ctx-a
  context: { cluster: a, user: alice, namespace: team-a }
"#;

    // Contexto cuyo usuario está definido en otro fichero de KUBECONFIG.
    const CLUSTER_B_NO_USER: &str = r#"
apiVersion: v1
kind: Config
clusters:
- name: b
  cluster: { server: "https://b.example:6443" }
contexts:
- name: ctx-b
  context: { cluster: b, user: alice }
"#;

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn sin_kubeconfig_usa_el_fichero_por_defecto() {
        let specs = discover_sources(None, Some(Path::new("/home/u")), &[]);
        assert_eq!(
            specs,
            vec![SourceSpec { path: "/home/u/.kube/config".into(), origin: SourceOrigin::Default }]
        );
    }

    #[test]
    fn ignora_entradas_vacias_y_duplicadas_de_kubeconfig() {
        let sep = if cfg!(windows) { ";" } else { ":" };
        let value = format!("/k/a{sep}{sep}/k/b{sep}/k/a{sep}");
        let specs = discover_sources(Some(OsStr::new(&value)), Some(Path::new("/home/u")), &["/k/b".into()]);
        let paths: Vec<_> = specs.iter().map(|s| (s.path.to_str().unwrap(), s.origin)).collect();
        assert_eq!(paths, vec![("/k/a", SourceOrigin::Env), ("/k/b", SourceOrigin::Env)]);
    }

    #[test]
    fn kubeconfig_solo_con_separadores_cuenta_como_no_definido() {
        let sep = if cfg!(windows) { ";" } else { ":" };
        let specs = discover_sources(Some(OsStr::new(sep)), Some(Path::new("/home/u")), &[]);
        assert_eq!(specs[0].origin, SourceOrigin::Default);
    }

    #[test]
    fn los_ids_son_estables_y_distinguen_contexto_y_fichero() {
        let p = Path::new("/no/existe/config");
        assert_eq!(cluster_id(p, "a"), cluster_id(p, "a"));
        assert_ne!(cluster_id(p, "a"), cluster_id(p, "b"));
        assert_ne!(cluster_id(p, "a"), cluster_id(Path::new("/otro"), "a"));
        assert_eq!(cluster_id(p, "a").len(), 16);
    }

    #[test]
    fn carga_contextos_con_servidor_y_namespace() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "a.yaml", CLUSTER_A);
        let catalog = Catalog::load(&[SourceSpec { path: a, origin: SourceOrigin::Manual }]);

        assert_eq!(catalog.contexts.len(), 1);
        let ctx = &catalog.contexts[0];
        assert_eq!(ctx.context_name, "ctx-a");
        assert_eq!(ctx.server.as_deref(), Some("https://a.example:6443"));
        assert_eq!(ctx.default_namespace.as_deref(), Some("team-a"));
    }

    #[test]
    fn un_fichero_roto_no_impide_leer_los_demas() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "a.yaml", CLUSTER_A);
        let roto = write(dir.path(), "roto.yaml", "esto: [no es");
        let falta = dir.path().join("falta.yaml");
        let catalog = Catalog::load(&[
            SourceSpec { path: roto, origin: SourceOrigin::Manual },
            SourceSpec { path: falta, origin: SourceOrigin::Manual },
            SourceSpec { path: a, origin: SourceOrigin::Manual },
        ]);

        assert!(catalog.sources[0].error.is_some());
        assert_eq!(catalog.sources[1].error.as_deref(), Some("El fichero no existe"));
        assert!(catalog.sources[2].error.is_none());
        assert_eq!(catalog.contexts.len(), 1);
    }

    #[test]
    fn en_kubeconfig_un_contexto_usa_usuarios_de_otro_fichero() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "a.yaml", CLUSTER_A);
        let b = write(dir.path(), "b.yaml", CLUSTER_B_NO_USER);
        let catalog = Catalog::load(&[
            SourceSpec { path: a, origin: SourceOrigin::Env },
            SourceSpec { path: b.clone(), origin: SourceOrigin::Env },
        ]);

        let id = cluster_id(&b, "ctx-b");
        let (config, context) = catalog.kubeconfig_for(&id).expect("contexto de b");
        assert_eq!(context, "ctx-b");
        assert_eq!(config.current_context.as_deref(), Some("ctx-b"));
        assert!(config.auth_infos.iter().any(|u| u.name == "alice"), "el usuario de a.yaml está disponible");
        assert_eq!(catalog.context(&id).unwrap().server.as_deref(), Some("https://b.example:6443"));
    }

    #[test]
    fn un_fichero_manual_no_ve_los_usuarios_de_kubeconfig() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "a.yaml", CLUSTER_A);
        let b = write(dir.path(), "b.yaml", CLUSTER_B_NO_USER);
        let catalog = Catalog::load(&[
            SourceSpec { path: a, origin: SourceOrigin::Env },
            SourceSpec { path: b.clone(), origin: SourceOrigin::Manual },
        ]);

        let (config, _) = catalog.kubeconfig_for(&cluster_id(&b, "ctx-b")).unwrap();
        assert!(config.auth_infos.is_empty());
    }
}
