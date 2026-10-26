//! Traducción de errores de kube-rs al `OculusError` del contrato, con pistas
//! concretas para lo que el usuario puede arreglar.

use std::error::Error as _;

use kube::client::AuthError;
use oculus_protocol::{ErrorCode, ErrorDetails, OculusError};

const MAX_DETAIL: usize = 300;

pub fn from_kube(err: &kube::Error) -> OculusError {
    // Va primero: un fallo de credenciales puede llegar envuelto en otros errores
    // y su texto original no se puede mostrar (ver `auth_error`).
    if let Some(auth) = find_auth(err) {
        return auth_error(auth);
    }
    match err {
        kube::Error::Api(status) => {
            let code = match status.code {
                401 => ErrorCode::Unauthorized,
                403 => ErrorCode::Forbidden,
                404 => ErrorCode::NotFound,
                409 => ErrorCode::Conflict,
                422 => ErrorCode::Invalid,
                _ => ErrorCode::Internal,
            };
            let mut e = OculusError::new(code, status.message.clone());
            e.details = Some(ErrorDetails { status: Some(status.code), hint: None });
            if code == ErrorCode::Unauthorized {
                e = e.with_hint("Las credenciales han caducado o no son válidas. Vuelve a iniciar sesión con la herramienta del proveedor (aws sso login, az login…).");
            }
            e
        }
        kube::Error::InferKubeconfig(_) => OculusError::new(ErrorCode::Invalid, chain(err)),
        kube::Error::RustlsTls(_) => tls(chain(err)),
        kube::Error::HyperError(_) | kube::Error::Service(_) => network(chain(err)),
        _ => OculusError::internal(chain(err)),
    }
}

pub fn timeout(what: &str, secs: u64) -> OculusError {
    OculusError::new(ErrorCode::Unreachable, format!("{what}: sin respuesta en {secs} s")).with_hint(
        "El API server no responde. Comprueba la VPN o la red, y que la URL del clúster es accesible desde este equipo.",
    )
}

fn find_auth(err: &kube::Error) -> Option<&AuthError> {
    if let kube::Error::Auth(auth) = err {
        return Some(auth);
    }
    let mut source = err.source();
    while let Some(s) = source {
        if let Some(auth) = s.downcast_ref::<AuthError>() {
            return Some(auth);
        }
        source = s.source();
    }
    None
}

/// Nunca usa el texto de kube-rs para `AuthExecRun`: incluye `{cmd:?}`, que
/// vuelca todas las variables de entorno del plugin (el entorno completo de la
/// shell de login, con sus tokens), y la salida entera del plugin.
fn auth_error(err: &AuthError) -> OculusError {
    let message = match err {
        AuthError::AuthExecStart(io) => format!("No se pudo ejecutar el exec plugin: {io}"),
        AuthError::AuthExecRun { status, out, .. } => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            match stderr.lines().map(str::trim).find(|l| !l.is_empty()) {
                Some(line) => format!("El exec plugin falló ({status}): {}", scrub(line)),
                None => format!("El exec plugin falló ({status})"),
            }
        }
        AuthError::AuthExecParse(_) => "El exec plugin devolvió una respuesta que no se entiende".to_owned(),
        other => scrub(&other.to_string()),
    };
    OculusError::new(ErrorCode::Unauthorized, message).with_hint(
        "Falló el exec plugin o el proveedor de credenciales del kubeconfig. Inicia sesión con la herramienta del proveedor (aws sso login, az login…) o prueba el mismo contexto con kubectl en una terminal.",
    )
}

/// Tapa todo lo que parezca un token (secuencias largas sin espacios) y recorta.
pub fn scrub(text: &str) -> String {
    let is_token_char = |c: char| c.is_ascii_alphanumeric() || "+/=_-.".contains(c);
    let mut out = String::with_capacity(text.len());
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        if run.len() >= 24 {
            out.push_str("***");
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for c in text.chars() {
        if is_token_char(c) {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    if out.chars().count() > MAX_DETAIL {
        out = out.chars().take(MAX_DETAIL).collect::<String>() + "…";
    }
    out
}

fn network(message: String) -> OculusError {
    let lower = message.to_lowercase();
    if lower.contains("certificate") || lower.contains("tls") || lower.contains("handshake") {
        return tls(message);
    }
    if lower.contains("deadline has elapsed") || lower.contains("timed out") {
        return OculusError::new(ErrorCode::Unreachable, "Tiempo agotado al conectar con el API server").with_hint(
            "El servidor no responde. Comprueba la VPN o la red, y que la URL del clúster es accesible desde este equipo.",
        );
    }
    let hint = if lower.contains("dns") || lower.contains("lookup") {
        "No se pudo resolver el nombre del servidor. Si es un clúster privado, puede que necesites la VPN."
    } else if lower.contains("refused") {
        "El servidor rechazó la conexión: el clúster puede estar parado o la URL tener el puerto equivocado."
    } else {
        "Comprueba la VPN o la red, y que la URL del clúster es accesible desde este equipo."
    };
    OculusError::new(ErrorCode::Unreachable, message).with_hint(hint)
}

fn tls(message: String) -> OculusError {
    OculusError::new(ErrorCode::Unreachable, message).with_hint(
        "Falló la verificación TLS. Si hay un proxy con inspección TLS en la red, el certificado del clúster no coincidirá.",
    )
}

/// Mensaje con la cadena de causas: "error del cliente: conexión: dns…".
fn chain(err: &kube::Error) -> String {
    let mut parts = vec![err.to_string()];
    let mut source = err.source();
    while let Some(s) = source {
        let text = s.to_string();
        // kube y hyper repiten la causa dentro de su propio mensaje.
        if !parts.last().is_some_and(|last| last.ends_with(&text)) {
            parts.push(text);
        }
        source = s.source();
    }
    parts.join(": ")
}

#[cfg(test)]
mod tests {
    use kube::core::Status;

    use super::*;

    fn api(code: u16) -> kube::Error {
        kube::Error::Api(Box::new(Status::failure("mensaje del servidor", "Razón").with_code(code)))
    }

    #[test]
    fn traduce_los_codigos_http_del_api_server() {
        assert_eq!(from_kube(&api(401)).code, ErrorCode::Unauthorized);
        assert_eq!(from_kube(&api(403)).code, ErrorCode::Forbidden);
        assert_eq!(from_kube(&api(404)).code, ErrorCode::NotFound);
        assert_eq!(from_kube(&api(409)).code, ErrorCode::Conflict);
        assert_eq!(from_kube(&api(422)).code, ErrorCode::Invalid);
        let e = from_kube(&api(403));
        assert_eq!(e.message, "mensaje del servidor");
        assert_eq!(e.details.unwrap().status, Some(403));
    }

    #[test]
    fn los_fallos_de_red_dan_pistas_segun_la_causa() {
        let dns = network("client error (Connect): dns error: failed to lookup address".into());
        assert_eq!(dns.code, ErrorCode::Unreachable);
        assert!(dns.details.unwrap().hint.unwrap().contains("VPN"));

        let refused = network("tcp connect error: Connection refused (os error 61)".into());
        assert!(refused.details.unwrap().hint.unwrap().contains("rechazó"));

        let cert = network("invalid peer certificate: UnknownIssuer".into());
        assert!(cert.details.unwrap().hint.unwrap().contains("TLS"));

        let slow = network("ServiceError: client error (Connect): deadline has elapsed".into());
        assert_eq!(slow.message, "Tiempo agotado al conectar con el API server");
    }

    #[test]
    fn scrub_tapa_tokens_y_respeta_el_texto_normal() {
        let line = "Error: token eyJhbGciOiJSUzI1NiIsImtpZCI6IjEifQ.payload expired for profile dev";
        assert_eq!(scrub(line), "Error: token *** expired for profile dev");
        assert_eq!(scrub("Token has expired and refresh failed"), "Token has expired and refresh failed");
        assert!(scrub(&"a ".repeat(400)).ends_with('…'));
    }
}
