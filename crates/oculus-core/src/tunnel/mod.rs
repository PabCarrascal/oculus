//! Túneles SSH por clúster (D13): el `ssh` del sistema con `-D` hace de proxy
//! SOCKS5 en `127.0.0.1` y el cliente de kube-rs sale por él. Un túnel por host,
//! compartido por los clústeres que lo usan; si `ssh` termina, se relanza en el
//! mismo puerto para que el `Client` siga valiendo.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use oculus_protocol::{ErrorCode, OculusError, SshHostInfo, TunnelState};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::cluster::errors;
use crate::shell_env::ShellEnv;

const MAX_HOST: usize = 255;
/// Líneas de la salida de error de `ssh` que se guardan para explicar un fallo.
const STDERR_LINES: usize = 20;
const PORT_ATTEMPTS: usize = 3;

#[derive(Debug, Clone, Copy)]
pub struct TunnelTimeouts {
    /// Hasta que el `-D` acepta conexiones (incluye autenticarse en el bastión).
    pub ready: Duration,
    /// Tope de la espera entre relanzamientos.
    pub backoff_max: Duration,
}

impl Default for TunnelTimeouts {
    fn default() -> Self {
        Self { ready: Duration::from_secs(15), backoff_max: Duration::from_secs(30) }
    }
}

/// Alias de `~/.ssh/config` o `usuario@host[:puerto]`. Además de esto, el host
/// va siempre detrás de `--`, así que nunca se interpreta como una opción.
pub fn validate_host(host: &str) -> Result<String, OculusError> {
    let host = host.trim();
    let hint = "Usa un alias de ~/.ssh/config o usuario@host[:puerto].";
    if host.is_empty() {
        return Err(OculusError::invalid("Falta el host del túnel SSH").with_hint(hint));
    }
    if host.chars().count() > MAX_HOST {
        return Err(OculusError::invalid(format!(
            "El host del túnel SSH no puede pasar de {MAX_HOST} caracteres"
        )));
    }
    if host.starts_with('-') || host.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(OculusError::invalid(format!("Host SSH no válido: {host}")).with_hint(hint));
    }
    Ok(host.to_owned())
}

/// Lo que se le pasa a `ssh` como destino. `ssh` no entiende `host:puerto` (lo
/// tomaría por un nombre de host), pero sí la forma `ssh://usuario@host:puerto`.
pub fn destination(host: &str) -> String {
    if host.starts_with("ssh://") {
        return host.to_owned();
    }
    let rest = host.rsplit_once('@').map_or(host, |(_, r)| r);
    let has_port = match rest.rsplit_once(':') {
        Some((name, port)) => {
            let bracketed = name.starts_with('[') && name.ends_with(']') && name.len() > 2;
            !port.is_empty()
                && port.bytes().all(|b| b.is_ascii_digit())
                && (bracketed || (!name.is_empty() && !name.contains(':')))
        }
        None => false,
    };
    if has_port { format!("ssh://{host}") } else { host.to_owned() }
}

/// Cómo lanzar `ssh`: el binario ya resuelto y el entorno de la shell de login.
#[derive(Debug, Clone)]
pub struct SshLauncher {
    pub program: PathBuf,
    pub env: Vec<(String, String)>,
    /// Opciones antes del `--`. Los tests pasan `-F` para no leer el `~/.ssh` del usuario.
    pub extra_args: Vec<String>,
}

impl SshLauncher {
    /// Busca `ssh` con el `PATH` efectivo, como los exec plugins (riesgo 1).
    pub fn resolve(path: &[PathBuf], shell: &ShellEnv) -> Result<Self, OculusError> {
        let joined = std::env::join_paths(path).map_err(|e| OculusError::internal(e.to_string()))?;
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let found =
            which::which_in("ssh", Some(&joined), cwd).ok().or_else(windows_openssh).ok_or_else(|| {
                OculusError::new(ErrorCode::NotFound, "No se encontró «ssh» en el PATH").with_hint(
                "Instala el cliente de OpenSSH o añade su carpeta en Ajustes › Rutas extra para binarios.",
            )
            })?;
        let mut env = vec![("PATH".to_owned(), joined.to_string_lossy().into_owned())];
        // El agente que ve el usuario en su terminal (1Password, Secretive…) puede
        // no ser el del proceso, que hereda el de launchd.
        if let Some(sock) = shell.get("SSH_AUTH_SOCK") {
            env.push(("SSH_AUTH_SOCK".to_owned(), sock.to_owned()));
        }
        Ok(Self { program: found, env, extra_args: Vec::new() })
    }

    fn command(&self, args: &[String]) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(args).envs(self.env.iter().map(|(k, v)| (k, v))).stdin(Stdio::null()).stdout(Stdio::null());
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        cmd
    }
}

#[cfg(windows)]
fn windows_openssh() -> Option<PathBuf> {
    let root = std::env::var_os("SystemRoot")?;
    let path = PathBuf::from(root).join("System32").join("OpenSSH").join("ssh.exe");
    path.is_file().then_some(path)
}

#[cfg(not(windows))]
fn windows_openssh() -> Option<PathBuf> {
    None
}

/// `ssh -N -D 127.0.0.1:PUERTO … -- HOST`. Sin diálogo (`BatchMode`) y sin
/// multiplexación: con `ControlMaster`, el `-D` viviría en un proceso ajeno.
pub fn tunnel_args(port: u16, host: &str, extra: &[String]) -> Vec<String> {
    let mut args: Vec<String> = ["-N", "-D", &format!("127.0.0.1:{port}")].map(String::from).to_vec();
    for opt in [
        "BatchMode=yes",
        "ExitOnForwardFailure=yes",
        "ConnectTimeout=10",
        "ServerAliveInterval=15",
        "ServerAliveCountMax=3",
        "ControlMaster=no",
        "ControlPath=none",
    ] {
        args.push("-o".into());
        args.push(opt.into());
    }
    args.extend(extra.iter().cloned());
    args.push("--".into());
    args.push(destination(host));
    args
}

/// Fallo al levantar el túnel. `port_busy`: otro proceso tomó el puerto y vale
/// la pena reintentar con otro.
#[derive(Debug)]
struct StartError {
    error: OculusError,
    port_busy: bool,
}

/// Traduce la salida de error de `ssh` a un `OculusError` con su pista.
pub fn classify(host: &str, lines: &[String]) -> OculusError {
    classify_start(host, lines).error
}

fn classify_start(host: &str, lines: &[String]) -> StartError {
    let has = |needles: &[&str]| lines.iter().any(|l| needles.iter().any(|n| l.contains(n)));
    let last = lines.iter().rev().find(|l| !l.trim().is_empty()).map(|l| {
        let line: String = errors::scrub(l.trim()).chars().take(300).collect();
        format!("ssh: {line}")
    });
    let message = format!("No se pudo abrir el túnel SSH con «{host}»");
    let with_last = |hint: &str| match &last {
        Some(l) => format!("{hint} {l}"),
        None => hint.to_owned(),
    };
    let (code, hint, port_busy) = if has(&["Permission denied"]) {
        (
            ErrorCode::Unauthorized,
            with_last(
                "ssh no pudo autenticarse sin preguntar: carga la clave en el agente (ssh-add). \
                 Los bastiones con segundo factor interactivo aún no están soportados.",
            ),
            false,
        )
    } else if has(&["Host key verification failed", "REMOTE HOST IDENTIFICATION HAS CHANGED"]) {
        (
            ErrorCode::Unauthorized,
            with_last(&format!(
                "Conéctate una vez desde la terminal (ssh {}) para revisar y aceptar la huella del host. \
                 Oculus nunca la acepta por su cuenta.",
                destination(host)
            )),
            false,
        )
    } else if has(&["Address already in use", "cannot listen to port", "Could not request local forwarding"])
    {
        (ErrorCode::Unreachable, with_last("El puerto local del túnel estaba ocupado."), true)
    } else {
        (ErrorCode::Unreachable, last.unwrap_or_else(|| "ssh terminó sin dar explicaciones.".into()), false)
    };
    StartError { error: OculusError::new(code, message).with_hint(hint), port_busy }
}

/// Lo que `ssh -G` entiende de un host, sin conectar.
pub fn parse_ssh_g(output: &str) -> Option<SshHostInfo> {
    let mut values: HashMap<&str, &str> = HashMap::new();
    for line in output.lines() {
        if let Some((key, value)) = line.split_once(' ') {
            values.entry(key).or_insert(value.trim());
        }
    }
    Some(SshHostInfo {
        hostname: values.get("hostname")?.to_string(),
        user: values.get("user")?.to_string(),
        port: values.get("port")?.parse().ok()?,
        proxy_jump: values.get("proxyjump").filter(|v| **v != "none").map(|v| v.to_string()),
    })
}

pub async fn check_host(launcher: &SshLauncher, host: &str) -> Result<SshHostInfo, OculusError> {
    let host = validate_host(host)?;
    let mut args = vec!["-G".to_owned()];
    args.extend(launcher.extra_args.iter().cloned());
    args.extend(["--".to_owned(), destination(&host)]);
    let mut cmd = launcher.command(&args);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(5), cmd.output())
        .await
        .map_err(|_| errors::timeout("ssh -G", 5))?
        .map_err(|e| OculusError::internal(format!("no se pudo ejecutar ssh: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let line: String =
            errors::scrub(stderr.lines().last().unwrap_or("").trim()).chars().take(300).collect();
        return Err(OculusError::invalid(format!("ssh no entiende el host «{host}»"))
            .with_hint(format!("ssh: {line}")));
    }
    parse_ssh_g(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| OculusError::internal("salida de ssh -G inesperada"))
}

/// Fase del túnel, por un canal `watch` para quien lo use.
#[derive(Debug, Clone, PartialEq)]
pub enum TunnelPhase {
    Up,
    Reconnecting,
    /// Abandonado: no se puede autenticar o el puerto ya no está libre.
    Failed(OculusError),
}

impl TunnelPhase {
    pub fn state(&self) -> Option<TunnelState> {
        match self {
            Self::Up => Some(TunnelState::Up),
            Self::Reconnecting => Some(TunnelState::Reconnecting),
            Self::Failed(_) => None,
        }
    }
}

type Lines = Arc<Mutex<VecDeque<String>>>;
/// El túnel abierto de un host, si lo hay.
type Slot = Arc<tokio::sync::Mutex<Weak<Tunnel>>>;

pub struct Tunnel {
    pub host: String,
    pub port: u16,
    phase: watch::Receiver<TunnelPhase>,
    stderr: Lines,
    supervisor: tokio::task::AbortHandle,
}

impl Tunnel {
    pub fn proxy_url(&self) -> http::Uri {
        format!("socks5://127.0.0.1:{}", self.port).parse().expect("URL de proxy válida")
    }

    pub fn phase(&self) -> watch::Receiver<TunnelPhase> {
        self.phase.clone()
    }

    /// ¿Ha escrito `ssh` algo así desde que arrancó? Para afinar errores del API server.
    pub fn stderr_contains(&self, needle: &str) -> bool {
        self.stderr_line(needle).is_some()
    }

    /// La última línea de `ssh` que contiene eso, limpia y recortada.
    pub fn stderr_line(&self, needle: &str) -> Option<String> {
        let lines = self.stderr.lock().expect("lock de stderr");
        let line = lines.iter().rev().find(|l| l.contains(needle))?;
        Some(errors::scrub(line.trim()).chars().take(300).collect())
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        // Al abortar el supervisor se suelta el `Child` y `kill_on_drop` mata `ssh`.
        self.supervisor.abort();
        tracing::info!(host = %self.host, port = self.port, "túnel SSH cerrado");
    }
}

/// Pids de los `ssh` vivos, en disco, por si la app se cuelga y quedan huérfanos.
#[derive(Clone, Default)]
struct PidRegistry {
    path: Option<PathBuf>,
    pids: Arc<Mutex<HashSet<u32>>>,
}

impl PidRegistry {
    fn add(&self, pid: u32) {
        let mut pids = self.pids.lock().expect("lock de pids");
        pids.insert(pid);
        self.save(&pids);
    }

    fn remove(&self, pid: u32) {
        let mut pids = self.pids.lock().expect("lock de pids");
        pids.remove(&pid);
        self.save(&pids);
    }

    fn save(&self, pids: &HashSet<u32>) {
        if let Some(path) = &self.path {
            let list: Vec<u32> = pids.iter().copied().collect();
            if let Err(err) = crate::persist::write_json_atomic(path, &list) {
                tracing::warn!(error = %err.message, "no se pudo anotar los pids de los túneles");
            }
        }
    }
}

/// Un `ssh` vivo. Al soltarlo muere (`kill_on_drop`) y sale del registro.
struct SshProcess {
    child: Child,
    pid: Option<u32>,
    pids: PidRegistry,
}

impl Drop for SshProcess {
    fn drop(&mut self) {
        if let Some(pid) = self.pid {
            self.pids.remove(pid);
        }
    }
}

/// Mata los `ssh -D 127.0.0.1:…` que dejó una ejecución anterior que no cerró
/// bien. Antes comprueba la línea de órdenes: un pid se puede reutilizar.
pub fn kill_orphans(path: &Path) {
    let pids: Vec<u32> = crate::persist::read_json_or(path, Vec::new);
    for pid in pids {
        if is_our_ssh(pid) {
            tracing::info!(pid, "túnel SSH huérfano de una ejecución anterior; se cierra");
            kill(pid);
        }
    }
    let _ = crate::persist::write_json_atomic(path, &Vec::<u32>::new());
}

#[cfg(unix)]
fn is_our_ssh(pid: u32) -> bool {
    let Ok(out) = std::process::Command::new("ps").args(["-o", "command=", "-p", &pid.to_string()]).output()
    else {
        return false;
    };
    let cmd = String::from_utf8_lossy(&out.stdout);
    cmd.contains("ssh") && cmd.contains("-N") && cmd.contains("-D 127.0.0.1:")
}

#[cfg(unix)]
fn kill(pid: u32) {
    let _ = std::process::Command::new("kill").arg(pid.to_string()).status();
}

// En Windows no hay `ps`: los huérfanos de un cuelgue se quedan hasta que caen
// solos (ServerAlive). Pendiente: un Job Object que los mate con la app.
#[cfg(not(unix))]
fn is_our_ssh(_pid: u32) -> bool {
    false
}

#[cfg(not(unix))]
fn kill(pid: u32) {
    let _ = std::process::Command::new("taskkill").args(["/F", "/PID", &pid.to_string()]).status();
}

#[derive(Clone)]
pub struct TunnelManager {
    /// Un hueco por host. El `Mutex` asíncrono hace que dos clústeres que piden
    /// el mismo host a la vez compartan el mismo arranque.
    slots: Arc<Mutex<HashMap<String, Slot>>>,
    pids: PidRegistry,
    timeouts: TunnelTimeouts,
}

impl TunnelManager {
    /// `pid_file`: dónde anotar los pids. Al crear el gestor se cierran los
    /// huérfanos que haya.
    pub fn new(pid_file: Option<PathBuf>, timeouts: TunnelTimeouts) -> Self {
        if let Some(path) = &pid_file {
            kill_orphans(path);
        }
        Self { slots: Arc::default(), pids: PidRegistry { path: pid_file, pids: Arc::default() }, timeouts }
    }

    /// Mata ya, sin pasar por el runtime, todos los `ssh` vivos. Al salir de la
    /// app: soltar los túneles solo programa su cierre, y el proceso puede
    /// terminar antes de que el runtime lo ejecute.
    pub fn kill_all(&self) {
        let pids: Vec<u32> = self.pids.pids.lock().expect("lock de pids").iter().copied().collect();
        for pid in pids {
            kill(pid);
            self.pids.remove(pid);
        }
    }

    /// El túnel de ese host: el que ya esté abierto o uno nuevo.
    pub async fn acquire(&self, host: &str, launcher: &SshLauncher) -> Result<Arc<Tunnel>, OculusError> {
        let host = validate_host(host)?;
        let slot = self.slots.lock().expect("lock de túneles").entry(host.clone()).or_default().clone();
        let mut current = slot.lock().await;
        if let Some(tunnel) = current.upgrade()
            && !matches!(*tunnel.phase.borrow(), TunnelPhase::Failed(_))
        {
            return Ok(tunnel);
        }

        let stderr: Lines = Arc::default();
        let started = std::time::Instant::now();
        let mut attempt = 0;
        let (process, port) = loop {
            attempt += 1;
            let port = free_port()?;
            match self.start(launcher, &host, port, &stderr).await {
                Ok(process) => break (process, port),
                Err(e) if e.port_busy && attempt < PORT_ATTEMPTS => continue,
                Err(e) => {
                    tracing::info!(%host, code = ?e.error.code, "no se pudo abrir el túnel SSH");
                    return Err(e.error);
                }
            }
        };
        tracing::info!(%host, port, ms = started.elapsed().as_millis() as u64, "túnel SSH abierto");

        let (tx, rx) = watch::channel(TunnelPhase::Up);
        let supervisor = tokio::spawn(self.clone().supervise(
            process,
            launcher.clone(),
            host.clone(),
            port,
            stderr.clone(),
            tx,
        ));
        let tunnel =
            Arc::new(Tunnel { host, port, phase: rx, stderr, supervisor: supervisor.abort_handle() });
        *current = Arc::downgrade(&tunnel);
        Ok(tunnel)
    }

    /// Lanza `ssh` y espera a que el `-D` responda como un proxy SOCKS5.
    async fn start(
        &self,
        launcher: &SshLauncher,
        host: &str,
        port: u16,
        stderr: &Lines,
    ) -> Result<SshProcess, StartError> {
        let mut cmd = launcher.command(&tunnel_args(port, host, &launcher.extra_args));
        cmd.stderr(Stdio::piped()).kill_on_drop(true);
        let mut child = cmd.spawn().map_err(|e| StartError {
            error: OculusError::internal(format!("no se pudo ejecutar ssh: {e}")),
            port_busy: false,
        })?;
        let pid = child.id();
        if let Some(pid) = pid {
            self.pids.add(pid);
        }
        let reader = read_stderr(&mut child, stderr.clone());
        let mut process = SshProcess { child, pid, pids: self.pids.clone() };

        let deadline = tokio::time::Instant::now() + self.timeouts.ready;
        loop {
            if let Ok(Some(_)) = process.child.try_wait() {
                return Err(self.failure(host, reader, stderr).await);
            }
            if socks_ready(port).await {
                return Ok(process);
            }
            if tokio::time::Instant::now() >= deadline {
                drop(process);
                let previous = self.failure(host, reader, stderr).await.error;
                let mut error = OculusError::new(
                    ErrorCode::Unreachable,
                    format!("El túnel SSH con «{host}» no respondió en {} s", self.timeouts.ready.as_secs()),
                );
                error.details = previous.details;
                return Err(StartError { error, port_busy: false });
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn failure(&self, host: &str, reader: Option<JoinHandle<()>>, stderr: &Lines) -> StartError {
        if let Some(reader) = reader {
            let _ = tokio::time::timeout(Duration::from_secs(1), reader).await;
        }
        let lines: Vec<String> = stderr.lock().expect("lock de stderr").iter().cloned().collect();
        classify_start(host, &lines)
    }

    /// Vigila `ssh`. Si termina, lo relanza en el mismo puerto con espera
    /// creciente; se rinde si no puede autenticarse o el puerto ya no está libre.
    async fn supervise(
        self,
        mut process: SshProcess,
        launcher: SshLauncher,
        host: String,
        port: u16,
        stderr: Lines,
        tx: watch::Sender<TunnelPhase>,
    ) {
        loop {
            let status = process.child.wait().await;
            drop(process);
            tracing::warn!(%host, port, status = ?status.ok().and_then(|s| s.code()), "el túnel SSH se cayó; se relanza");
            let _ = tx.send(TunnelPhase::Reconnecting);
            let mut delay = Duration::from_secs(1);
            process = loop {
                tokio::time::sleep(delay).await;
                stderr.lock().expect("lock de stderr").clear();
                match self.start(&launcher, &host, port, &stderr).await {
                    Ok(p) => break p,
                    Err(e) if e.port_busy || e.error.code == ErrorCode::Unauthorized => {
                        tracing::warn!(%host, code = ?e.error.code, "se abandona el túnel SSH");
                        let _ = tx.send(TunnelPhase::Failed(e.error));
                        return;
                    }
                    Err(_) => delay = (delay * 2).min(self.timeouts.backoff_max),
                }
            };
            tracing::info!(%host, port, "túnel SSH restablecido");
            let _ = tx.send(TunnelPhase::Up);
        }
    }
}

fn read_stderr(child: &mut Child, lines: Lines) -> Option<JoinHandle<()>> {
    let stderr = child.stderr.take()?;
    Some(tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let mut lines = lines.lock().expect("lock de stderr");
            if lines.len() == STDERR_LINES {
                lines.pop_front();
            }
            lines.push_back(line);
        }
    }))
}

/// Un puerto libre en `127.0.0.1`. `-D` no admite el 0, así que hay que elegirlo
/// antes; si otro lo toma entre medias, `ExitOnForwardFailure` lo detecta.
fn free_port() -> Result<u16, OculusError> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .map_err(|e| OculusError::internal(format!("no hay puertos locales libres: {e}")))?;
    Ok(listener.local_addr().map_err(|e| OculusError::internal(e.to_string()))?.port())
}

/// ¿Hay un proxy SOCKS5 sin autenticación escuchando? Un saludo completo, para
/// no dar por bueno cualquier proceso que acepte conexiones en el puerto.
async fn socks_ready(port: u16) -> bool {
    let probe = async {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.ok()?;
        stream.write_all(&[5, 1, 0]).await.ok()?;
        let mut reply = [0u8; 2];
        stream.read_exact(&mut reply).await.ok()?;
        (reply == [5, 0]).then_some(())
    };
    matches!(tokio::time::timeout(Duration::from_secs(1), probe).await, Ok(Some(())))
}

#[cfg(test)]
mod tests;
