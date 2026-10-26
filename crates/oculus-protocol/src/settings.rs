use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum Locale {
    Es,
    #[default]
    En,
}

impl Locale {
    /// Elige idioma a partir de una etiqueta BCP 47 ("es-ES", "en_US"…).
    pub fn from_tag(tag: &str) -> Self {
        if tag.to_ascii_lowercase().starts_with("es") { Self::Es } else { Self::En }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct TelemetrySettings {
    /// Desactivado por defecto: nada sale de la máquina sin consentimiento.
    pub crash_reports: bool,
}

/// Ajustes persistidos. `#[serde(default)]` permite leer ficheros de versiones
/// anteriores a las que les falten campos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: Theme,
    pub locale: Locale,
    pub shell: Option<String>,
    /// Rutas extra para encontrar exec plugins (aws, gcloud, kubelogin…).
    pub extra_path: Vec<String>,
    pub telemetry: TelemetrySettings,
    pub watch_linger_seconds: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            locale: Locale::default(),
            shell: None,
            extra_path: Vec::new(),
            telemetry: TelemetrySettings::default(),
            watch_linger_seconds: 30,
        }
    }
}

/// Cambio parcial: solo se aplican los campos presentes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsPatch {
    #[specta(optional)]
    pub theme: Option<Theme>,
    #[specta(optional)]
    pub locale: Option<Locale>,
    /// Ausente: no se toca. `null`: vuelve a la shell del sistema.
    #[serde(deserialize_with = "present_or_null")]
    #[specta(type = Option<String>, optional)]
    pub shell: Option<Option<String>>,
    #[specta(optional)]
    pub extra_path: Option<Vec<String>>,
    #[specta(optional)]
    pub telemetry: Option<TelemetrySettings>,
    #[specta(optional)]
    pub watch_linger_seconds: Option<u32>,
}

/// Sin esto serde convierte `null` en "campo ausente" y no se podría borrar.
fn present_or_null<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detecta_el_idioma_por_etiqueta() {
        assert_eq!(Locale::from_tag("es-ES"), Locale::Es);
        assert_eq!(Locale::from_tag("ES_mx"), Locale::Es);
        assert_eq!(Locale::from_tag("en-GB"), Locale::En);
        assert_eq!(Locale::from_tag("fr-FR"), Locale::En);
    }

    #[test]
    fn rellena_campos_que_faltan_con_valores_por_defecto() {
        let s: Settings = serde_json::from_str(r#"{ "theme": "dark" }"#).unwrap();
        assert_eq!(s.theme, Theme::Dark);
        assert_eq!(s.watch_linger_seconds, 30);
        assert!(!s.telemetry.crash_reports);
    }

    #[test]
    fn distingue_campo_ausente_de_null_en_el_parche() {
        let ausente: SettingsPatch = serde_json::from_str("{}").unwrap();
        let nulo: SettingsPatch = serde_json::from_str(r#"{ "shell": null }"#).unwrap();
        assert_eq!(ausente.shell, None);
        assert_eq!(nulo.shell, Some(None));
    }
}
