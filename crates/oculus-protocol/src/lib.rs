//! Tipos que cruzan la frontera IPC. Solo datos: sin kube-rs ni tokio, para que
//! compile rápido y los bindings TS se generen sin arrastrar el núcleo.

mod app;
mod cluster;
mod detail;
mod error;
mod resources;
mod settings;

pub use app::{AppInfo, StartupReport};
pub use cluster::{
    Category, ClusterId, ClusterPrefs, ClusterStatus, ClusterSummary, ConnectStep, KubeconfigSource,
    ResourceKind, SourceOrigin, SshHostInfo, SshTunnelPrefs, TunnelState, TunnelStatus,
};
pub use detail::{EventInfo, ObjectRef, RelatedRef, Relation, ResourceDetail, UpdateResult};
pub use error::{ErrorCode, ErrorDetails, FieldCause, OculusError};
pub use resources::{
    CellValue, ColumnDef, ColumnType, KindRef, ResourceEvent, Row, RowStatus, Scope, Tone, WatchState,
};
pub use settings::{Locale, Settings, SettingsPatch, TelemetrySettings, Theme};
