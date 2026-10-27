use super::*;

#[test]
fn el_host_se_valida_antes_de_llegar_a_ssh() {
    assert_eq!(validate_host("  bastion  ").unwrap(), "bastion");
    assert_eq!(validate_host("ops@10.0.0.1:2222").unwrap(), "ops@10.0.0.1:2222");
    for bad in ["", "   ", "-oProxyCommand=evil", "dos palabras", "host\nmás", &"x".repeat(256)] {
        assert_eq!(validate_host(bad).unwrap_err().code, ErrorCode::Invalid, "{bad:?}");
    }
}

#[test]
fn host_con_puerto_pasa_a_ssh_como_url() {
    assert_eq!(destination("bastion"), "bastion");
    assert_eq!(destination("ops@bastion"), "ops@bastion");
    assert_eq!(destination("ops@10.0.0.1:2222"), "ssh://ops@10.0.0.1:2222");
    assert_eq!(destination("bastion:22"), "ssh://bastion:22");
    assert_eq!(destination("[::1]:2222"), "ssh://[::1]:2222");
    assert_eq!(destination("ssh://ops@bastion:2222"), "ssh://ops@bastion:2222");
    for kept in ["::1", "bastion:", "bastion:ssh", "fe80::1:22"] {
        assert_eq!(destination(kept), kept, "{kept:?}");
    }
    let args = tunnel_args(40123, "ops@10.0.0.1:2222", &[]);
    assert_eq!(&args[args.len() - 2..], ["--", "ssh://ops@10.0.0.1:2222"]);
}

#[test]
fn la_linea_de_ordenes_lleva_el_host_tras_el_separador() {
    let args = tunnel_args(40123, "bastion", &["-F".into(), "/tmp/cfg".into()]);
    assert_eq!(&args[..3], ["-N", "-D", "127.0.0.1:40123"]);
    for opt in ["BatchMode=yes", "ExitOnForwardFailure=yes", "ControlMaster=no", "ControlPath=none"] {
        assert!(args.windows(2).any(|w| w == ["-o", opt]), "falta {opt}");
    }
    assert_eq!(&args[args.len() - 4..], ["-F", "/tmp/cfg", "--", "bastion"]);
}

#[test]
fn la_salida_de_ssh_se_traduce_a_errores_con_pista() {
    let lines = |s: &[&str]| s.iter().map(|l| l.to_string()).collect::<Vec<_>>();

    let e = classify("bastion", &lines(&["ops@bastion: Permission denied (publickey)."]));
    assert_eq!(e.code, ErrorCode::Unauthorized);
    assert!(e.details.unwrap().hint.unwrap().contains("ssh-add"));

    let e = classify("bastion", &lines(&["Host key verification failed."]));
    assert_eq!(e.code, ErrorCode::Unauthorized);
    assert!(e.details.unwrap().hint.unwrap().contains("ssh bastion"));

    let e = classify("bastion", &lines(&["ssh: Could not resolve hostname bastion: nodename nor servname"]));
    assert_eq!(e.code, ErrorCode::Unreachable);
    assert!(e.details.unwrap().hint.unwrap().contains("Could not resolve hostname"));

    let busy = classify_start("bastion", &lines(&["bind [127.0.0.1]:40123: Address already in use"]));
    assert!(busy.port_busy);
    assert!(!classify_start("bastion", &lines(&["Connection refused"])).port_busy);
}

#[test]
fn ssh_g_da_el_destino_real_del_alias() {
    let info = parse_ssh_g(
        "user ops\nhostname 10.0.0.7\nport 2222\nproxyjump jump.example\nidentityfile ~/.ssh/id\n",
    )
    .unwrap();
    assert_eq!(
        info,
        SshHostInfo {
            hostname: "10.0.0.7".into(),
            user: "ops".into(),
            port: 2222,
            proxy_jump: Some("jump.example".into())
        }
    );
    assert_eq!(parse_ssh_g("user a\nhostname b\nport 22\n").unwrap().proxy_jump, None);
    assert_eq!(parse_ssh_g("user a\n"), None);
}

/// Un `ssh` falso: un script de shell con lo que haga falta.
#[cfg(unix)]
fn fake_ssh(dir: &Path, script: &str) -> SshLauncher {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("ssh");
    std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    SshLauncher { program: path, env: Vec::new(), extra_args: Vec::new() }
}

#[cfg(unix)]
#[tokio::test]
async fn un_ssh_que_no_se_autentica_da_unauthorized_y_no_deja_pids() {
    let dir = tempfile::tempdir().unwrap();
    let launcher = fake_ssh(dir.path(), "echo 'ops@bastion: Permission denied (publickey).' >&2; exit 255");
    let pid_file = dir.path().join("tunnels.json");
    let manager = TunnelManager::new(Some(pid_file.clone()), TunnelTimeouts::default());
    let err = manager.acquire("bastion", &launcher).await.err().unwrap();
    assert_eq!(err.code, ErrorCode::Unauthorized);
    assert!(err.details.unwrap().hint.unwrap().contains("ssh: ops@bastion: Permission denied"));
    let pids: Vec<u32> = crate::persist::read_json_or(&pid_file, Vec::new);
    assert!(pids.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn un_ssh_que_no_responde_agota_el_tiempo_y_se_mata() {
    let dir = tempfile::tempdir().unwrap();
    let launcher = fake_ssh(dir.path(), "exec sleep 30");
    let timeouts = TunnelTimeouts { ready: Duration::from_millis(400), ..Default::default() };
    let manager = TunnelManager::new(None, timeouts);
    let started = std::time::Instant::now();
    let err = manager.acquire("bastion", &launcher).await.err().unwrap();
    assert_eq!(err.code, ErrorCode::Unreachable);
    assert!(err.message.contains("no respondió"));
    assert!(started.elapsed() < Duration::from_secs(3));
}

/// Un `ssh` falso que sí levanta un proxy SOCKS5: Python responde al saludo.
#[cfg(unix)]
fn fake_socks_ssh(dir: &Path) -> Option<SshLauncher> {
    std::process::Command::new("python3").arg("--version").output().ok()?.status.success().then_some(())?;
    let server = r#"
import socket, sys
port = int(next(a for a in sys.argv if a.startswith("127.0.0.1:")).split(":")[1])
s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("127.0.0.1", port)); s.listen()
while True:
    c, _ = s.accept(); c.recv(3); c.sendall(bytes([5, 0])); c.close()
"#;
    std::fs::write(dir.join("socks.py"), server).unwrap();
    Some(fake_ssh(dir, &format!("exec python3 -I {} \"$@\"", dir.join("socks.py").display())))
}

#[cfg(unix)]
#[tokio::test]
async fn al_salir_kill_all_mata_ssh_sin_esperar_al_runtime() {
    let dir = tempfile::tempdir().unwrap();
    let Some(launcher) = fake_socks_ssh(dir.path()) else {
        eprintln!("sin python3: se omite");
        return;
    };
    let manager = TunnelManager::new(Some(dir.path().join("tunnels.json")), TunnelTimeouts::default());
    let tunnel = manager.acquire("bastion", &launcher).await.unwrap();
    let pid = manager.pids.pids.lock().unwrap().iter().copied().next().unwrap();

    // Lo que hace la app al salir: matar sin soltar antes el túnel ni ceder el runtime.
    manager.kill_all();
    let alive = |pid: u32| {
        std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .unwrap()
            .stdout
    };
    let started = std::time::Instant::now();
    // Muerto o zombi (el runtime aún no lo ha recogido): en ambos casos ya no escucha.
    while !(alive(pid).is_empty() || alive(pid).starts_with(b"Z")) {
        assert!(started.elapsed() < Duration::from_secs(2), "ssh sigue vivo");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(manager.pids.pids.lock().unwrap().is_empty());
    drop(tunnel);
}

#[cfg(unix)]
#[test]
fn los_huerfanos_solo_se_matan_si_son_un_ssh_nuestro() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("tunnels.json");
    // Un proceso que no es ssh con un pid anotado: no se toca.
    let mut other = std::process::Command::new("sleep").arg("30").spawn().unwrap();
    crate::persist::write_json_atomic(&pid_file, &vec![other.id()]).unwrap();
    kill_orphans(&pid_file);
    assert!(other.try_wait().unwrap().is_none(), "mató un proceso ajeno");
    other.kill().unwrap();
    let pids: Vec<u32> = crate::persist::read_json_or(&pid_file, Vec::new);
    assert!(pids.is_empty());
}
