//! Traducción de errores de kube-rs al `OculusError` del contrato, con pistas
//! concretas para lo que el usuario puede arreglar.

use std::error::Error as _;

use kube::client::AuthError;
use oculus_protocol::{ErrorCode, ErrorDetails, FieldCause, OculusError};

const MAX_DETAIL: usize = 300;

pub fn from_kube(err: &kube::Error) -> OculusError {
    // Va primero: un fallo de credenciales puede llegar envuelto en otros errores
    // y su texto original no se puede mostrar (ver `auth_error`).
    if let Some(auth) = find_auth(err) {
        return auth_error(auth);
    }
    match err {
        kube::Error::Api(status) => api_error(status),
        kube::Error::InferKubeconfig(_) => OculusError::new(ErrorCode::Invalid, chain(err)),
        kube::Error::RustlsTls(_) => tls(chain(err)),
        kube::Error::HyperError(_) | kube::Error::Service(_) => network(chain(err)),
        _ => OculusError::internal(chain(err)),
    }
}

fn api_error(status: &kube::core::Status) -> OculusError {
    let code = match status.code {
        // 400: la validación estricta (campos desconocidos o duplicados) llega así.
        400 | 422 => ErrorCode::Invalid,
        401 => ErrorCode::Unauthorized,
        403 => ErrorCode::Forbidden,
        404 => ErrorCode::NotFound,
        409 => ErrorCode::Conflict,
        _ => ErrorCode::Internal,
    };
    let mut causes: Vec<FieldCause> = status
        .details
        .as_ref()
        .map(|d| {
            d.causes
                .iter()
                .map(|c| FieldCause {
                    field: Some(c.field.clone()).filter(|f| !f.is_empty()),
                    message: if c.message.is_empty() { c.reason.clone() } else { c.message.clone() },
                })
                .collect()
        })
        .unwrap_or_default();
    // La validación estricta llega como 400 (PUT) o como 422 con una causa en el
    // campo "patch" (PATCH), y en este caso el mensaje incluye el objeto entero.
    // Se sacan las rutas reales y se descarta el volcado.
    let mut message = status.message.clone();
    if let Some(i) = message.find("strict decoding error:") {
        causes = strict_causes(&message[i..]);
        message = message[i..].to_owned();
    }
    let message = truncate(&message, MAX_MESSAGE);
    for cause in &mut causes {
        cause.message = truncate(&cause.message, MAX_MESSAGE);
    }
    let hint = match code {
        ErrorCode::Unauthorized => Some(
            "Las credenciales han caducado o no son válidas. Vuelve a iniciar sesión con la herramienta del proveedor (aws sso login, az login…).",
        ),
        ErrorCode::Conflict => Some(
            "El objeto cambió en el clúster mientras lo editabas. Mira las diferencias y decide si recargar o sobrescribir.",
        ),
        _ => None,
    };
    OculusError {
        code,
        message,
        details: Some(ErrorDetails { status: Some(status.code), hint: hint.map(str::to_owned), causes }),
    }
}

const MAX_MESSAGE: usize = 500;

/// Los mensajes del API server pueden traer el objeto entero: se recortan.
fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    text.chars().take(max).collect::<String>() + "…"
}

/// `strict decoding error: unknown field "spec.replicass", duplicate field "spec.x"`
fn strict_causes(message: &str) -> Vec<FieldCause> {
    let mut causes = Vec::new();
    for (kind, label) in
        [("unknown field \"", "campo desconocido"), ("duplicate field \"", "campo duplicado")]
    {
        let mut rest = message;
        while let Some(i) = rest.find(kind) {
            let after = &rest[i + kind.len()..];
            let Some(end) = after.find('"') else { break };
            let field = after[..end].to_owned();
            causes.push(FieldCause { message: format!("{label}: {field}"), field: Some(field) });
            rest = &after[end..];
        }
    }
    causes
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
    fn la_validacion_estricta_y_los_422_traen_causas_por_campo() {
        let strict = Status::failure(
            r#"Deployment in version "v1" cannot be handled as a Deployment: strict decoding error: unknown field "spec.replicass", duplicate field "metadata.name""#,
            "BadRequest",
        )
        .with_code(400);
        let e = from_kube(&kube::Error::Api(Box::new(strict)));
        assert_eq!(e.code, ErrorCode::Invalid);
        let fields: Vec<_> = e.details.unwrap().causes.into_iter().map(|c| c.field.unwrap()).collect();
        assert_eq!(fields, ["spec.replicass", "metadata.name"]);

        let mut invalid = Status::failure("Deployment.apps \"web\" is invalid", "Invalid").with_code(422);
        invalid.details = Some(kube::core::response::StatusDetails {
            name: "web".into(),
            group: "apps".into(),
            kind: "Deployment".into(),
            uid: String::new(),
            retry_after_seconds: 0,
            causes: vec![kube::core::response::StatusCause {
                reason: "FieldValueInvalid".into(),
                message: "Invalid value: -1: must be greater than or equal to 0".into(),
                field: "spec.replicas".into(),
            }],
        });
        let e = from_kube(&kube::Error::Api(Box::new(invalid)));
        let cause = &e.details.unwrap().causes[0];
        assert_eq!(cause.field.as_deref(), Some("spec.replicas"));
        assert!(cause.message.contains("greater than or equal"));
    }

    #[test]
    fn el_campo_desconocido_de_un_patch_no_vuelca_el_objeto() {
        let huge = format!(
            r#" "" is invalid: patch: Invalid value: "{{\"spec\":{{{}}}}}": strict decoding error: unknown field "spec.replicass""#,
            "\"x\":1,".repeat(2000)
        );
        let mut status = Status::failure(&huge, "Invalid").with_code(422);
        status.details = Some(kube::core::response::StatusDetails {
            name: String::new(),
            group: String::new(),
            kind: String::new(),
            uid: String::new(),
            retry_after_seconds: 0,
            causes: vec![kube::core::response::StatusCause {
                reason: "FieldValueInvalid".into(),
                message: huge.clone(),
                field: "patch".into(),
            }],
        });
        let e = from_kube(&kube::Error::Api(Box::new(status)));
        assert_eq!(e.code, ErrorCode::Invalid);
        assert_eq!(e.message, r#"strict decoding error: unknown field "spec.replicass""#);
        let causes = e.details.unwrap().causes;
        assert_eq!(causes.len(), 1);
        assert_eq!(causes[0].field.as_deref(), Some("spec.replicass"));
    }

    #[test]
    fn un_409_explica_el_conflicto() {
        let e = from_kube(&api(409));
        assert_eq!(e.code, ErrorCode::Conflict);
        assert!(e.details.unwrap().hint.unwrap().contains("diferencias"));
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
