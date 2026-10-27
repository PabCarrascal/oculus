//! Registro de comandos y eventos para tauri-specta. Es el único sitio que toca
//! su API (sigue en RC; riesgo 8 de docs/06-riesgos.md).

use tauri_specta::{Builder, collect_commands, collect_events};

use crate::{commands, events};

pub fn builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::app::app_info,
            commands::app::startup_report,
            commands::app::ui_log,
            commands::settings::settings_get,
            commands::settings::settings_set,
            commands::clusters::kubeconfig_list_sources,
            commands::clusters::kubeconfig_pick_source,
            commands::clusters::kubeconfig_remove_source,
            commands::clusters::clusters_list,
            commands::clusters::cluster_connect,
            commands::clusters::cluster_disconnect,
            commands::clusters::cluster_set_prefs,
            commands::clusters::ssh_tunnel_check,
            commands::clusters::discovery_get,
            commands::clusters::namespaces_list,
            commands::resources::resources_subscribe,
            commands::resources::resources_unsubscribe,
            commands::resources::resources_reset,
            commands::detail::resource_get,
            commands::detail::resource_update,
            commands::detail::secret_reveal,
            commands::detail::openapi_schema,
        ])
        .events(collect_events![events::ClusterStatusChanged, events::KubeconfigChanged])
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
