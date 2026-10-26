//! Prepara el exec plugin de un contexto antes de conectar: resuelve el binario
//! con el `PATH` efectivo y le pasa el entorno de la shell de login.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use kube::config::{ExecInteractiveMode, Kubeconfig};
use oculus_protocol::{ErrorCode, OculusError};

use crate::shell_env::ShellEnv;

pub fn prepare_exec(
    config: &mut Kubeconfig,
    context: &str,
    path: &[PathBuf],
    shell: &ShellEnv,
) -> Result<(), OculusError> {
    let Some(user) = config
        .contexts
        .iter()
        .find(|c| c.name == context)
        .and_then(|c| c.context.as_ref())
        .and_then(|c| c.user.clone())
    else {
        return Ok(());
    };
    let Some(exec) = config
        .auth_infos
        .iter_mut()
        .find(|u| u.name == user)
        .and_then(|u| u.auth_info.as_mut())
        .and_then(|a| a.exec.as_mut())
    else {
        return Ok(());
    };
    let Some(command) = exec.command.clone() else { return Ok(()) };

    let joined = std::env::join_paths(path).map_err(|err| OculusError::internal(err.to_string()))?;
    let resolved = resolve(&command, &joined).ok_or_else(|| {
        let mut hint = format!(
            "Instálalo o añade su carpeta en Ajustes › Rutas extra para binarios. Se buscó en: {}",
            path.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
        );
        if let Some(install) = &exec.install_hint {
            hint = format!("{} {hint}", install.trim());
        }
        OculusError::new(ErrorCode::Unauthorized, format!("No se encontró el exec plugin «{command}»"))
            .with_hint(hint)
    })?;
    exec.command = Some(resolved.display().to_string());

    // Una app de escritorio no tiene terminal: como kubectl sin tty, el plugin no
    // es interactivo salvo que el kubeconfig lo exija. Así además se captura su
    // stderr y se puede mostrar la causa de un fallo.
    if exec.interactive_mode != Some(ExecInteractiveMode::Always) {
        exec.interactive_mode = Some(ExecInteractiveMode::Never);
    }

    // Variables de la shell de login que el proceso no tiene (AWS_PROFILE,
    // AZURE_CONFIG_DIR…), sin pisar las que el kubeconfig fija explícitamente.
    let env = exec.env.get_or_insert_with(Vec::new);
    let explicit: std::collections::HashSet<String> =
        env.iter().filter_map(|e| e.get("name").cloned()).collect();
    let mut add = |name: &str, value: &str| {
        if !explicit.contains(name) {
            env.push(HashMap::from([("name".into(), name.into()), ("value".into(), value.into())]));
        }
    };
    add("PATH", &joined.to_string_lossy());
    for (name, value) in shell.vars() {
        if name != "PATH" && std::env::var_os(name).is_none() {
            add(name, value);
        }
    }
    Ok(())
}

fn resolve(command: &str, joined_path: &std::ffi::OsStr) -> Option<PathBuf> {
    let candidate = Path::new(command);
    if candidate.is_absolute() {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }
    let cwd = std::env::current_dir().ok()?;
    which::which_in(command, Some(joined_path), cwd).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kubeconfig(command: &str) -> Kubeconfig {
        Kubeconfig::from_yaml(&format!(
            r#"
apiVersion: v1
kind: Config
clusters: [{{ name: c, cluster: {{ server: "https://x" }} }}]
users:
- name: u
  user:
    exec:
      apiVersion: client.authentication.k8s.io/v1beta1
      command: {command}
      installHint: "brew install fake-auth."
      env: [{{ name: AWS_PROFILE, value: from-kubeconfig }}]
contexts: [{{ name: ctx, context: {{ cluster: c, user: u }} }}]
"#
        ))
        .unwrap()
    }

    fn exec_env(config: &Kubeconfig) -> HashMap<String, String> {
        config.auth_infos[0]
            .auth_info
            .as_ref()
            .unwrap()
            .exec
            .as_ref()
            .unwrap()
            .env
            .as_ref()
            .unwrap()
            .iter()
            .map(|e| (e["name"].clone(), e["value"].clone()))
            .collect()
    }

    #[cfg(unix)]
    fn fake_binary(dir: &Path, name: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn resuelve_el_binario_en_las_rutas_extra_y_pasa_el_entorno() {
        let dir = tempfile::tempdir().unwrap();
        let bin = fake_binary(dir.path(), "fake-auth");
        let mut config = kubeconfig("fake-auth");
        let shell = ShellEnv::from_vars(&[("AWS_PROFILE", "from-shell"), ("OCULUS_TEST_ONLY_IN_SHELL", "1")]);

        prepare_exec(&mut config, "ctx", &[dir.path().to_path_buf()], &shell).unwrap();

        let exec = config.auth_infos[0].auth_info.as_ref().unwrap().exec.as_ref().unwrap();
        assert_eq!(exec.command.as_deref(), Some(bin.to_str().unwrap()));
        assert_eq!(exec.interactive_mode, Some(ExecInteractiveMode::Never));
        let env = exec_env(&config);
        assert_eq!(env["AWS_PROFILE"], "from-kubeconfig", "el kubeconfig manda sobre la shell");
        assert_eq!(env["OCULUS_TEST_ONLY_IN_SHELL"], "1");
        assert!(env["PATH"].starts_with(dir.path().to_str().unwrap()));
    }

    #[test]
    fn sin_binario_da_un_error_con_pista() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = kubeconfig("no-existe-este-plugin");
        let err =
            prepare_exec(&mut config, "ctx", &[dir.path().to_path_buf()], &ShellEnv::default()).unwrap_err();

        assert_eq!(err.code, ErrorCode::Unauthorized);
        assert!(err.message.contains("no-existe-este-plugin"));
        let hint = err.details.unwrap().hint.unwrap();
        assert!(hint.starts_with("brew install fake-auth."), "incluye el installHint del kubeconfig: {hint}");
        assert!(hint.contains("Ajustes"));
    }

    #[test]
    fn un_contexto_sin_exec_no_se_toca() {
        let mut config = Kubeconfig::from_yaml(
            "apiVersion: v1\nkind: Config\nusers: [{ name: u, user: { token: t } }]\ncontexts: [{ name: ctx, context: { cluster: c, user: u } }]\n",
        )
        .unwrap();
        let before = serde_json::to_value(&config).unwrap();
        prepare_exec(&mut config, "ctx", &[], &ShellEnv::default()).unwrap();
        assert_eq!(serde_json::to_value(&config).unwrap(), before);
    }
}
