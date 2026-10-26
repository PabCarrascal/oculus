use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use oculus_protocol::{Locale, OculusError, Settings, SettingsPatch};

const MAX_LINGER_SECONDS: u32 = 600;

/// Ajustes en un JSON dentro del directorio de configuración de la app.
pub struct SettingsStore {
    path: PathBuf,
    current: RwLock<Settings>,
}

impl SettingsStore {
    /// Nunca falla: si el fichero no existe o está corrupto se usan los valores
    /// por defecto, y el fichero dañado se aparta para poder inspeccionarlo.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let current = match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|err| {
                tracing::warn!(%err, path = %path.display(), "ajustes ilegibles; se usan los valores por defecto");
                let _ = fs::rename(&path, path.with_extension("json.corrupt"));
                defaults()
            }),
            Err(_) => defaults(),
        };
        Self { path, current: RwLock::new(current) }
    }

    pub fn get(&self) -> Settings {
        self.current.read().expect("lock de ajustes envenenado").clone()
    }

    pub fn apply(&self, patch: SettingsPatch) -> Result<Settings, OculusError> {
        let mut current = self.current.write().expect("lock de ajustes envenenado");
        let mut next = current.clone();
        merge(&mut next, patch)?;
        write_atomic(&self.path, &next)?;
        *current = next.clone();
        Ok(next)
    }
}

fn defaults() -> Settings {
    let locale = sys_locale::get_locale().map(|tag| Locale::from_tag(&tag)).unwrap_or_default();
    Settings { locale, ..Settings::default() }
}

fn merge(s: &mut Settings, patch: SettingsPatch) -> Result<(), OculusError> {
    if let Some(theme) = patch.theme {
        s.theme = theme;
    }
    if let Some(locale) = patch.locale {
        s.locale = locale;
    }
    if let Some(shell) = patch.shell {
        s.shell = shell.filter(|sh| !sh.trim().is_empty());
    }
    if let Some(paths) = patch.extra_path {
        s.extra_path = paths.into_iter().map(|p| p.trim().to_owned()).filter(|p| !p.is_empty()).collect();
    }
    if let Some(telemetry) = patch.telemetry {
        s.telemetry = telemetry;
    }
    if let Some(linger) = patch.watch_linger_seconds {
        if linger > MAX_LINGER_SECONDS {
            return Err(OculusError::invalid(format!("watchLingerSeconds = {linger}"))
                .with_hint(format!("Debe estar entre 0 y {MAX_LINGER_SECONDS}")));
        }
        s.watch_linger_seconds = linger;
    }
    Ok(())
}

/// Escribe en un temporal y lo renombra: un cierre a medias nunca deja el
/// fichero de ajustes truncado.
fn write_atomic(path: &Path, settings: &Settings) -> Result<(), OculusError> {
    let io_err =
        |err: std::io::Error| OculusError::internal(format!("no se pudieron guardar los ajustes: {err}"));
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(io_err)?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(settings).map_err(|err| OculusError::internal(err.to_string()))?;
    let mut file = fs::File::create(&tmp).map_err(io_err)?;
    file.write_all(&json).map_err(io_err)?;
    file.sync_all().map_err(io_err)?;
    fs::rename(&tmp, path).map_err(io_err)
}

#[cfg(test)]
mod tests {
    use oculus_protocol::{ErrorCode, Theme};

    use super::*;

    fn store() -> (tempfile::TempDir, SettingsStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::load(dir.path().join("settings.json"));
        (dir, store)
    }

    #[test]
    fn sin_fichero_usa_valores_por_defecto() {
        let (_dir, store) = store();
        assert_eq!(store.get().theme, Theme::System);
        assert_eq!(store.get().watch_linger_seconds, 30);
    }

    #[test]
    fn aplica_el_parche_y_lo_persiste() {
        let (dir, store) = store();
        let patch = SettingsPatch { theme: Some(Theme::Dark), ..Default::default() };
        store.apply(patch).unwrap();

        let reloaded = SettingsStore::load(dir.path().join("settings.json"));
        assert_eq!(reloaded.get().theme, Theme::Dark);
    }

    #[test]
    fn rechaza_valores_fuera_de_rango_sin_tocar_el_estado() {
        let (_dir, store) = store();
        let patch = SettingsPatch { watch_linger_seconds: Some(10_000), ..Default::default() };
        let err = store.apply(patch).unwrap_err();
        assert_eq!(err.code, ErrorCode::Invalid);
        assert_eq!(store.get().watch_linger_seconds, 30);
    }

    #[test]
    fn null_en_shell_vuelve_a_la_del_sistema() {
        let (_dir, store) = store();
        store.apply(SettingsPatch { shell: Some(Some("/bin/zsh".into())), ..Default::default() }).unwrap();
        store.apply(SettingsPatch { shell: Some(None), ..Default::default() }).unwrap();
        assert_eq!(store.get().shell, None);
    }

    #[test]
    fn limpia_rutas_vacias() {
        let (_dir, store) = store();
        let paths = vec!["  /opt/homebrew/bin ".into(), "".into(), "   ".into()];
        store.apply(SettingsPatch { extra_path: Some(paths), ..Default::default() }).unwrap();
        assert_eq!(store.get().extra_path, vec!["/opt/homebrew/bin"]);
    }

    #[test]
    fn un_fichero_corrupto_se_aparta_y_no_rompe_el_arranque() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "{ esto no es json").unwrap();

        let store = SettingsStore::load(&path);
        assert_eq!(store.get().theme, Theme::System);
        assert!(path.with_extension("json.corrupt").exists());
    }
}
