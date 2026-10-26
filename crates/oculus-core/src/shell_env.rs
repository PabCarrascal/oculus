//! Entorno de la shell de login del usuario.
//!
//! Una app lanzada desde el Finder no hereda el `PATH` ni el `KUBECONFIG` de la
//! terminal, así que `aws`, `gke-gcloud-auth-plugin` o `kubelogin` no se
//! encontrarían (riesgo 1 en docs/06-riesgos.md). Se pregunta una vez a la shell
//! de login y se guarda en memoria; nunca se escribe ni se registra en logs,
//! porque puede contener secretos.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

const START: &str = "__OCULUS_ENV_START__";
const END: &str = "__OCULUS_ENV_END__";
const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Default)]
pub struct ShellEnv {
    vars: HashMap<String, String>,
    /// Viene de la shell de login (y no del proceso como último recurso).
    pub from_login_shell: bool,
}

impl ShellEnv {
    pub fn from_process() -> Self {
        Self { vars: std::env::vars().collect(), from_login_shell: false }
    }

    #[cfg(test)]
    pub fn from_vars(vars: &[(&str, &str)]) -> Self {
        Self {
            vars: vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            from_login_shell: true,
        }
    }

    /// En Windows no hay shell de login que consultar: se usa el entorno del proceso.
    pub async fn capture() -> Self {
        #[cfg(unix)]
        match capture_login_shell().await {
            Ok(vars) => return Self { vars, from_login_shell: true },
            Err(err) => tracing::warn!(%err, "no se pudo leer el entorno de la shell de login"),
        }
        Self::from_process()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.vars.get(key).map(String::as_str)
    }

    pub fn vars(&self) -> impl Iterator<Item = (&str, &str)> {
        self.vars.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

#[cfg(unix)]
async fn capture_login_shell() -> Result<HashMap<String, String>, String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_owned());
    // Los marcadores separan el entorno de lo que impriman los ficheros rc.
    let script = format!("printf '%s' '{START}'; env -0; printf '%s' '{END}'");
    let child = tokio::process::Command::new(&shell)
        .args(["-ilc", &script])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(TIMEOUT, child)
        .await
        .map_err(|_| format!("{shell} tardó más de {}s", TIMEOUT.as_secs()))?
        .map_err(|err| format!("{shell}: {err}"))?;
    parse_env_dump(&output.stdout).ok_or_else(|| "salida sin marcadores".to_owned())
}

/// Extrae `CLAVE=valor` separados por NUL entre los marcadores.
pub fn parse_env_dump(stdout: &[u8]) -> Option<HashMap<String, String>> {
    let text = String::from_utf8_lossy(stdout);
    let start = text.find(START)? + START.len();
    let end = text[start..].find(END)? + start;
    let vars = text[start..end]
        .split('\0')
        .filter_map(|pair| pair.split_once('='))
        .filter(|(k, _)| !k.is_empty())
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    Some(vars)
}

/// Orden de búsqueda de binarios: rutas extra de los ajustes, luego el `PATH`
/// de la shell de login y por último el del proceso. Sin duplicados.
pub fn effective_path(extra: &[String], shell: &ShellEnv) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    let shell_path = shell.get("PATH").map(OsString::from);
    let process_path = std::env::var_os("PATH");
    extra
        .iter()
        .map(PathBuf::from)
        .chain(shell_path.iter().flat_map(std::env::split_paths))
        .chain(process_path.iter().flat_map(std::env::split_paths))
        .filter(|p| !p.as_os_str().is_empty())
        .filter(|p| seen.insert(p.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignora_lo_que_imprimen_los_ficheros_rc() {
        let out = format!("Bienvenido!\n{START}PATH=/opt/bin:/usr/bin\0AWS_PROFILE=dev\0X=a=b\0{END}adiós");
        let vars = parse_env_dump(out.as_bytes()).unwrap();
        assert_eq!(vars["PATH"], "/opt/bin:/usr/bin");
        assert_eq!(vars["AWS_PROFILE"], "dev");
        assert_eq!(vars["X"], "a=b", "solo el primer = separa clave y valor");
    }

    #[test]
    fn sin_marcadores_no_hay_entorno() {
        assert!(parse_env_dump(b"PATH=/usr/bin").is_none());
    }

    #[test]
    fn las_rutas_extra_van_primero_y_sin_duplicados() {
        let sep = if cfg!(windows) { ";" } else { ":" };
        let shell = ShellEnv::from_vars(&[("PATH", &format!("/opt/homebrew/bin{sep}/usr/bin"))]);
        let path = effective_path(&["/mis/bin".into(), "/usr/bin".into()], &shell);
        assert_eq!(&path[..3], &[PathBuf::from("/mis/bin"), "/usr/bin".into(), "/opt/homebrew/bin".into()]);
        assert_eq!(path.iter().filter(|p| *p == &PathBuf::from("/usr/bin")).count(), 1);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn captura_el_entorno_de_la_shell_de_login() {
        let env = ShellEnv::capture().await;
        assert!(env.get("PATH").is_some_and(|p| !p.is_empty()));
    }
}
