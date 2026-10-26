use serde::{Deserialize, Serialize};
use specta::Type;

/// Error único de todo el contrato IPC (docs/03-contrato-ipc.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{code:?}: {message}")]
pub struct OculusError {
    pub code: ErrorCode,
    pub message: String,
    pub details: Option<ErrorDetails>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum ErrorCode {
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    Invalid,
    Unreachable,
    Cancelled,
    Internal,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ErrorDetails {
    pub status: Option<u16>,
    pub hint: Option<String>,
}

impl OculusError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), details: None }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Invalid, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.details.get_or_insert_with(ErrorDetails::default).hint = Some(hint.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializa_con_el_formato_del_contrato() {
        let err = OculusError::invalid("valor fuera de rango").with_hint("usa 0–600");
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "code": "Invalid",
                "message": "valor fuera de rango",
                "details": { "status": null, "hint": "usa 0–600" }
            })
        );
    }
}
