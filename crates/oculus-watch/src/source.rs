//! De dónde salen los eventos: kube-rs en producción, un canal en los tests.

use futures::future::BoxFuture;
use futures::stream::BoxStream;
use futures::{FutureExt, StreamExt};
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::Client;
use kube::api::{Api, ApiResource, DynamicObject, GroupVersionKind};
use kube::core::PartialObjectMeta;
use kube::runtime::WatchStreamExt;
use kube::runtime::watcher::watcher;
use kube::runtime::watcher::{self, Event};
use oculus_protocol::{ColumnType, KindRef, Scope};

use crate::columns::{JsonPath, PrinterColumn, Projector};

/// Un tipo de recurso tal como lo dio el discovery.
#[derive(Debug, Clone)]
pub struct KindInfo {
    pub kind: KindRef,
    pub kind_name: String,
    pub namespaced: bool,
    pub verbs: Vec<String>,
}

/// Eventos de todos los streams de una vigilancia, con el índice del stream
/// (uno por namespace con `Scope::Namespaces`).
pub type EventStream = BoxStream<'static, (usize, Result<Event<DynamicObject>, watcher::Error>)>;

pub struct Opened {
    pub projector: Projector,
    pub streams: usize,
    pub events: EventStream,
}

pub trait Source: Send + Sync + 'static {
    fn open(&self, info: &KindInfo, scope: &Scope) -> BoxFuture<'static, Opened>;
}

pub struct KubeSource {
    pub client: Client,
}

impl Source for KubeSource {
    fn open(&self, info: &KindInfo, scope: &Scope) -> BoxFuture<'static, Opened> {
        let client = self.client.clone();
        let info = info.clone();
        let scope = scope.clone();
        async move {
            let projector = projector_for(&client, &info).await;
            let gvk = GroupVersionKind::gvk(&info.kind.group, &info.kind.version, &info.kind_name);
            let ar = ApiResource::from_gvk_with_plural(&gvk, &info.kind.plural);
            let namespaces: Vec<Option<String>> = match &scope {
                Scope::Namespaces { names } if info.namespaced => names.iter().cloned().map(Some).collect(),
                _ => vec![None],
            };
            // Secrets: solo metadatos. Los valores no pasan nunca por la lista (D5, riesgo 7).
            let metadata_only = info.kind.group.is_empty() && info.kind_name == "Secret";
            let streams = namespaces.len();
            let events = futures::stream::select_all(namespaces.into_iter().enumerate().map(|(i, ns)| {
                // any_semantic: el listado inicial sale de la caché del API server (riesgo 4).
                let config = watcher::Config::default().any_semantic();
                let stream: BoxStream<'static, Result<Event<DynamicObject>, watcher::Error>> =
                    if metadata_only {
                        // Api<PartialObjectMeta<_>> pide solo metadatos al servidor.
                        let api: Api<PartialObjectMeta<DynamicObject>> = match &ns {
                            Some(ns) => Api::namespaced_with(client.clone(), ns, &ar),
                            None => Api::all_with(client.clone(), &ar),
                        };
                        watcher(api, config).map(|r| r.map(|e| e.map_obj(meta_to_dynamic))).boxed()
                    } else {
                        let api: Api<DynamicObject> = match &ns {
                            Some(ns) => Api::namespaced_with(client.clone(), ns, &ar),
                            None => Api::all_with(client.clone(), &ar),
                        };
                        watcher(api, config).boxed()
                    };
                stream.default_backoff().map(move |r| (i, r)).boxed()
            }))
            .boxed();
            Opened { projector, streams, events }
        }
        .boxed()
    }
}

trait MapObj<A> {
    fn map_obj<B>(self, f: impl Fn(A) -> B) -> Event<B>;
}

impl<A> MapObj<A> for Event<A> {
    fn map_obj<B>(self, f: impl Fn(A) -> B) -> Event<B> {
        match self {
            Event::Apply(o) => Event::Apply(f(o)),
            Event::Delete(o) => Event::Delete(f(o)),
            Event::Init => Event::Init,
            Event::InitApply(o) => Event::InitApply(f(o)),
            Event::InitDone => Event::InitDone,
        }
    }
}

fn meta_to_dynamic(m: PartialObjectMeta<DynamicObject>) -> DynamicObject {
    DynamicObject { types: None, metadata: m.metadata, data: serde_json::Value::Object(Default::default()) }
}

fn builtin(group: &str) -> bool {
    group.is_empty()
        || group.ends_with(".k8s.io")
        || ["apps", "batch", "autoscaling", "policy", "extensions"].contains(&group)
}

/// Columnas nativas, o las `additionalPrinterColumns` del CRD si se puede leer.
async fn projector_for(client: &Client, info: &KindInfo) -> Projector {
    let KindRef { group, version, plural } = &info.kind;
    if builtin(group) {
        return Projector::native(group, &info.kind_name, info.namespaced);
    }
    let crds: Api<CustomResourceDefinition> = Api::all(client.clone());
    let printer = match crds.get_opt(&format!("{plural}.{group}")).await {
        Ok(Some(crd)) => crd
            .spec
            .versions
            .into_iter()
            .find(|v| &v.name == version)
            .and_then(|v| v.additional_printer_columns)
            .unwrap_or_default(),
        // Sin permiso para leer CRDs, o es una API agregada: columnas genéricas.
        Ok(None) | Err(_) => return Projector::native(group, &info.kind_name, info.namespaced),
    };
    let columns = printer
        .into_iter()
        .filter_map(|c| {
            Some(PrinterColumn {
                path: JsonPath::parse(&c.json_path)?,
                column_type: match c.type_.as_str() {
                    "integer" | "number" => ColumnType::Number,
                    "date" => ColumnType::Date,
                    _ => ColumnType::String,
                },
                priority: c.priority.unwrap_or(0),
                name: c.name,
            })
        })
        .collect();
    Projector::printer(info.namespaced, columns)
}
