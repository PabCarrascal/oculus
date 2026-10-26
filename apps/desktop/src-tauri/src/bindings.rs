//! Registro de comandos y eventos para tauri-specta. Es el único sitio que toca
//! su API (sigue en RC; riesgo 8 de docs/06-riesgos.md).

use tauri_specta::{Builder, collect_commands};

use crate::commands;

pub fn builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new().commands(collect_commands![
        commands::app::app_info,
        commands::app::startup_report,
        commands::settings::settings_get,
        commands::settings::settings_set,
    ])
}

/// Solo en debug y tests: en release no se escribe nada en disco.
#[cfg(any(debug_assertions, test))]
pub fn bindings_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../packages/bindings/src/bindings.ts")
}

#[cfg(any(debug_assertions, test))]
pub fn export(builder: &Builder<tauri::Wry>) {
    builder
        .export(specta_typescript::Typescript::default(), bindings_path())
        .expect("no se pudieron exportar los bindings TS");
}

#[cfg(test)]
mod tests {
    /// `pnpm gen:bindings` ejecuta este test; CI comprueba después que no hay diff.
    #[test]
    fn export_bindings() {
        super::export(&super::builder());
    }
}
