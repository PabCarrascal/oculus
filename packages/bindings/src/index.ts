// Fachada sobre el fichero generado. specta separa la forma serializada de la
// deserializada cuando hay `#[serde(default)]`; aquí se exporta la que ve la UI:
// lo que recibe del backend (Serialize) y lo que le envía (Deserialize).

export type {
  AppInfo,
  ErrorCode,
  Locale,
  OculusError_Serialize as OculusError,
  Settings_Serialize as Settings,
  SettingsPatch_Deserialize as SettingsPatch,
  StartupReport,
  Theme,
} from "./bindings";
export { commands } from "./bindings";
