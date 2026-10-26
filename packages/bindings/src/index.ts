// Fachada sobre el fichero generado. specta separa la forma serializada de la
// deserializada cuando hay `#[serde(default)]` u opcionales; aquí se exporta la
// que ve la UI: lo que recibe del backend (Serialize) y lo que le envía (Deserialize).

export type {
  AppInfo,
  Category,
  ClusterPrefs_Serialize as ClusterPrefs,
  ClusterStatus_Serialize as ClusterStatus,
  ClusterStatusChanged_Serialize as ClusterStatusChanged,
  ClusterSummary_Serialize as ClusterSummary,
  ConnectStep,
  ErrorCode,
  KubeconfigSource_Serialize as KubeconfigSource,
  Locale,
  OculusError_Serialize as OculusError,
  ResourceKind,
  Settings_Serialize as Settings,
  SettingsPatch_Deserialize as SettingsPatch,
  SourceOrigin,
  StartupReport,
  Theme,
} from "./bindings";
export { commands, events } from "./bindings";
