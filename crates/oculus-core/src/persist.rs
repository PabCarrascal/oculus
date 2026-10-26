//! Ficheros JSON de la app: escritura atómica y lectura tolerante.

use std::fs;
use std::io::Write;
use std::path::Path;

use oculus_protocol::OculusError;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Si el fichero no existe devuelve el valor por defecto; si está corrupto lo
/// aparta como `.corrupt` para poder inspeccionarlo y también usa el defecto.
pub fn read_json_or<T: DeserializeOwned>(path: &Path, default: impl FnOnce() -> T) -> T {
    let Ok(text) = fs::read_to_string(path) else { return default() };
    serde_json::from_str(&text).unwrap_or_else(|err| {
        tracing::warn!(%err, path = %path.display(), "fichero ilegible; se usan los valores por defecto");
        let _ = fs::rename(path, path.with_extension("json.corrupt"));
        default()
    })
}

/// Escribe en un temporal y lo renombra: un cierre a medias nunca deja el
/// fichero truncado.
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), OculusError> {
    let io_err =
        |err: std::io::Error| OculusError::internal(format!("no se pudo guardar {}: {err}", path.display()));
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(io_err)?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(value).map_err(|err| OculusError::internal(err.to_string()))?;
    let mut file = fs::File::create(&tmp).map_err(io_err)?;
    file.write_all(&json).map_err(io_err)?;
    file.sync_all().map_err(io_err)?;
    fs::rename(&tmp, path).map_err(io_err)
}
