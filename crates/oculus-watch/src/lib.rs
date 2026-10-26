//! Watchers compartidos y proyección de objetos a filas (D4 y D5 en docs/01-arquitectura.md).

pub mod columns;
pub mod manager;
pub mod source;

pub use manager::{ErrorMapper, Subscriber, SubscriptionId, WatchManager, WatchOptions};
pub use source::{KindInfo, KubeSource, Source};
