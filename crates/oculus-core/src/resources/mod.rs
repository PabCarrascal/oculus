//! Detalle y edición de un objeto (etapa 3): lectura con eventos y relacionados,
//! reemplazo con validación estricta y control de concurrencia, Secrets y esquemas.

mod related;

use json_patch::PatchOperation;
use k8s_openapi::api::core::v1::{Event, Pod, Secret};
use kube::api::{Api, ApiResource, DynamicObject, GroupVersionKind, ListParams, Patch, PatchParams};
use kube::core::PartialObjectMeta;
use oculus_protocol::{
    ErrorCode, EventInfo, ObjectRef, OculusError, ResourceDetail, ResourceKind, UpdateResult,
};
use serde_json::Value;

use crate::cluster::{ClusterSession, errors};

pub use related::static_related;

const MAX_EVENTS: usize = 50;
const MAX_RELATED_PODS: u32 = 50;
const FIELD_MANAGER: &str = "oculus";

impl ClusterSession {
    fn kind_for(&self, r: &ObjectRef) -> Result<&ResourceKind, OculusError> {
        self.kinds
            .iter()
            .find(|k| k.group == r.group && k.version == r.version && k.plural == r.plural)
            .ok_or_else(|| OculusError::new(ErrorCode::NotFound, format!("Tipo desconocido: {}", r.plural)))
    }

    fn api_for(&self, rk: &ResourceKind, r: &ObjectRef) -> Api<DynamicObject> {
        let gvk = GroupVersionKind::gvk(&rk.group, &rk.version, &rk.kind);
        let ar = ApiResource::from_gvk_with_plural(&gvk, &rk.plural);
        match (&r.namespace, rk.namespaced) {
            (Some(ns), true) => Api::namespaced_with(self.client.clone(), ns, &ar),
            _ => Api::all_with(self.client.clone(), &ar),
        }
    }

    pub async fn detail(&self, r: &ObjectRef, read_only: bool) -> Result<ResourceDetail, OculusError> {
        let rk = self.kind_for(r)?;
        let mut obj = self.api_for(rk, r).get(&r.name).await.map_err(|e| errors::from_kube(&e))?;
        obj.metadata.managed_fields = None;
        let is_secret = is_secret(rk);
        let secret_keys = is_secret.then(|| mask_secret(&mut obj));

        let uid = obj.metadata.uid.clone().unwrap_or_default();
        let (mut related, selector) = static_related(&obj, &self.kinds);
        if let (Some(selector), Some(ns)) = (selector, &r.namespace) {
            related.extend(self.selected_pods(ns, &selector).await);
        }
        let events = self.events_for(&uid, r.namespace.as_deref()).await;

        Ok(ResourceDetail {
            api_version: if rk.group.is_empty() {
                rk.version.clone()
            } else {
                format!("{}/{}", rk.group, rk.version)
            },
            kind: rk.kind.clone(),
            resource_version: obj.metadata.resource_version.clone().unwrap_or_default(),
            object_json: serde_json::to_string(&obj).map_err(|e| OculusError::internal(e.to_string()))?,
            uid,
            events,
            related,
            editable: !read_only && !is_secret && rk.verbs.iter().any(|v| v == "update"),
            secret_keys,
        })
    }

    /// Aplica los cambios del editor como un JSON Patch calculado entre el
    /// objeto tal como se cargó (`base_json`) y el editado. Cada campo tocado
    /// lleva antes una operación `test` con su valor original: si otro lo cambió
    /// entre medias, el parche entero se rechaza (conflicto). Los campos que el
    /// usuario no tocó no se envían, así que los cambios de estado del
    /// controlador no interfieren. `force` omite los `test`: sobrescribe solo lo
    /// que el usuario cambió.
    pub async fn update(
        &self,
        r: &ObjectRef,
        base_json: &str,
        edited_json: &str,
        dry_run: bool,
        force: bool,
    ) -> Result<UpdateResult, OculusError> {
        let rk = self.kind_for(r)?;
        if is_secret(rk) {
            return Err(OculusError::invalid("Los Secrets todavía no se pueden editar desde Oculus"));
        }
        let parse = |text: &str| -> Result<Value, OculusError> {
            serde_json::from_str(text)
                .map_err(|e| OculusError::invalid(format!("El documento no es válido: {e}")))
        };
        let base = parse(base_json)?;
        let mut edited = parse(edited_json)?;
        check_identity(&mut edited, rk, r)?;
        let ops = build_patch(&base, &edited, force)?;
        let has_tests = ops.iter().any(|op| matches!(op, PatchOperation::Test(_)));

        let mut pp = PatchParams::default().validation_strict();
        pp.field_manager = Some(FIELD_MANAGER.to_owned());
        if dry_run {
            pp = pp.dry_run();
        }
        let started = std::time::Instant::now();
        let result = self
            .api_for(rk, r)
            .patch(&r.name, &pp, &Patch::<()>::Json(json_patch::Patch(ops)))
            .await
            .map_err(|e| patch_error(&e, has_tests));
        // Del error, solo el código y las rutas: los mensajes pueden llevar valores.
        let (code, fields) = match &result {
            Ok(_) => (None, Vec::new()),
            Err(e) => (
                Some(e.code),
                e.details.iter().flat_map(|d| &d.causes).filter_map(|c| c.field.clone()).collect(),
            ),
        };
        tracing::info!(
            kind = %rk.kind,
            dry_run,
            force,
            ok = result.is_ok(),
            code = ?code,
            fields = ?fields,
            ms = started.elapsed().as_millis() as u64,
            "actualización"
        );
        let mut obj = result?;
        obj.metadata.managed_fields = None;
        Ok(UpdateResult {
            resource_version: obj.metadata.resource_version.clone().unwrap_or_default(),
            object_json: serde_json::to_string(&obj).map_err(|e| OculusError::internal(e.to_string()))?,
        })
    }

    /// Un solo valor de un Secret, bajo petición explícita. Nunca se registra.
    pub async fn reveal_secret(&self, r: &ObjectRef, key: &str) -> Result<String, OculusError> {
        let ns = r
            .namespace
            .as_deref()
            .ok_or_else(|| OculusError::invalid("Un Secret siempre tiene namespace"))?;
        let api: Api<Secret> = Api::namespaced(self.client.clone(), ns);
        let secret = api.get(&r.name).await.map_err(|e| errors::from_kube(&e))?;
        tracing::info!(secret = %r.name, key, "valor de Secret revelado");
        let bytes = secret.data.and_then(|d| d.get(key).cloned()).ok_or_else(|| {
            OculusError::new(ErrorCode::NotFound, format!("El Secret no tiene la clave «{key}»"))
        })?;
        Ok(String::from_utf8(bytes.0.clone())
            .unwrap_or_else(|_| format!("<binario, {} bytes>", bytes.0.len())))
    }

    async fn events_for(&self, uid: &str, namespace: Option<&str>) -> Vec<EventInfo> {
        if uid.is_empty() {
            return Vec::new();
        }
        let api: Api<Event> = match namespace {
            Some(ns) => Api::namespaced(self.client.clone(), ns),
            None => Api::all(self.client.clone()),
        };
        let params = ListParams::default().fields(&format!("involvedObject.uid={uid}"));
        let Ok(list) = api.list(&params).await else { return Vec::new() };
        let mut events: Vec<EventInfo> = list.items.into_iter().map(event_info).collect();
        events.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
        events.truncate(MAX_EVENTS);
        events
    }

    async fn selected_pods(&self, namespace: &str, selector: &str) -> Vec<oculus_protocol::RelatedRef> {
        let api: Api<PartialObjectMeta<Pod>> = Api::namespaced(self.client.clone(), namespace);
        let params = ListParams::default().labels(selector).limit(MAX_RELATED_PODS);
        let Ok(list) = api.list(&params).await else { return Vec::new() };
        list.items
            .into_iter()
            .filter_map(|p| p.metadata.name)
            .map(|name| oculus_protocol::RelatedRef {
                relation: oculus_protocol::Relation::Selects,
                kind: "Pod".into(),
                target: Some(ObjectRef {
                    group: String::new(),
                    version: "v1".into(),
                    plural: "pods".into(),
                    namespace: Some(namespace.to_owned()),
                    name: name.clone(),
                }),
                name,
            })
            .collect()
    }
}

fn is_secret(rk: &ResourceKind) -> bool {
    rk.group.is_empty() && rk.kind == "Secret"
}

/// Vacía los valores de `data`/`stringData` y devuelve las claves.
pub fn mask_secret(obj: &mut DynamicObject) -> Vec<String> {
    let mut keys = std::collections::BTreeSet::new();
    for field in ["data", "stringData"] {
        if let Some(map) = obj.data.get_mut(field).and_then(Value::as_object_mut) {
            for (k, v) in map.iter_mut() {
                keys.insert(k.clone());
                *v = Value::String(String::new());
            }
        }
    }
    // La anotación de kubectl apply guarda una copia completa del objeto, valores incluidos.
    if let Some(annotations) = obj.metadata.annotations.as_mut() {
        annotations.remove("kubectl.kubernetes.io/last-applied-configuration");
    }
    keys.into_iter().collect()
}

/// Comprueba que el documento editado es el mismo objeto (y completa el
/// namespace si se borró, para que el diff no lo quite).
pub fn check_identity(value: &mut Value, rk: &ResourceKind, r: &ObjectRef) -> Result<(), OculusError> {
    let api_version =
        if rk.group.is_empty() { rk.version.clone() } else { format!("{}/{}", rk.group, rk.version) };
    let mismatch = |what: &str, got: &str, want: &str| {
        OculusError::invalid(format!("{what} no coincide: «{got}» en lugar de «{want}»")).with_hint(
            "Para crear o mover objetos usa la acción de aplicar YAML; el editor solo modifica este objeto.",
        )
    };
    let get = |v: &Value, path: &[&str]| {
        path.iter().try_fold(v, |v, k| v.get(k)).and_then(Value::as_str).map(str::to_owned)
    };
    if get(value, &["apiVersion"]).as_deref() != Some(api_version.as_str()) {
        return Err(mismatch("apiVersion", &get(value, &["apiVersion"]).unwrap_or_default(), &api_version));
    }
    if get(value, &["kind"]).as_deref() != Some(rk.kind.as_str()) {
        return Err(mismatch("kind", &get(value, &["kind"]).unwrap_or_default(), &rk.kind));
    }
    if get(value, &["metadata", "name"]).as_deref() != Some(r.name.as_str()) {
        return Err(mismatch("El nombre", &get(value, &["metadata", "name"]).unwrap_or_default(), &r.name));
    }
    let ns = get(value, &["metadata", "namespace"]);
    if let (Some(got), Some(want)) = (&ns, &r.namespace)
        && got != want
    {
        return Err(mismatch("El namespace", got, want));
    }
    let metadata = value
        .get_mut("metadata")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| OculusError::invalid("Falta metadata"))?;
    if let (None, Some(want)) = (&ns, &r.namespace) {
        metadata.insert("namespace".into(), Value::String(want.clone()));
    }
    Ok(())
}

/// Campos que el editor muestra pero no se escriben: los gestiona el servidor.
fn ignored(path: &str) -> bool {
    const IGNORED: [&str; 7] = [
        "/status",
        "/metadata/resourceVersion",
        "/metadata/managedFields",
        "/metadata/generation",
        "/metadata/uid",
        "/metadata/creationTimestamp",
        "/metadata/selfLink",
    ];
    IGNORED.iter().any(|p| path == *p || path.strip_prefix(p).is_some_and(|rest| rest.starts_with('/')))
}

/// Diff base → editado como JSON Patch. Sin `force`, cada operación que
/// sustituye o quita un valor va precedida de un `test` con el valor que tiene
/// ese campo *en ese punto de la secuencia* (las operaciones sobre arrays
/// desplazan índices, así que no basta con leerlo de la base sin aplicar).
pub fn build_patch(base: &Value, edited: &Value, force: bool) -> Result<Vec<PatchOperation>, OculusError> {
    let ops: Vec<PatchOperation> =
        json_patch::diff(base, edited).0.into_iter().filter(|op| !ignored(op.path().as_str())).collect();
    if ops.is_empty() {
        return Err(OculusError::invalid("No hay cambios que aplicar")
            .with_hint("Los cambios en status y en los metadatos del sistema no se pueden escribir."));
    }
    if force {
        return Ok(ops);
    }
    let mut doc = base.clone();
    let mut out = Vec::with_capacity(ops.len() * 2);
    for op in ops {
        let tested = match &op {
            PatchOperation::Replace(o) => Some(o.path.clone()),
            PatchOperation::Remove(o) => Some(o.path.clone()),
            PatchOperation::Move(o) => Some(o.from.clone()),
            _ => None,
        };
        if let Some(path) = tested
            && let Some(value) = doc.pointer(path.as_str()).cloned()
        {
            out.push(PatchOperation::Test(json_patch::TestOperation { path, value }));
        }
        json_patch::patch(&mut doc, std::slice::from_ref(&op))
            .map_err(|e| OculusError::internal(format!("diff inconsistente: {e}")))?;
        out.push(op);
    }
    Ok(out)
}

/// Un `test` que falla vuelve como 422 genérico sin causas: eso es un conflicto.
fn patch_error(err: &kube::Error, has_tests: bool) -> OculusError {
    const TEST_FAILED: &str = "the server rejected our request due to an error in our request";
    if let kube::Error::Api(status) = err
        && has_tests
        && status.code == 422
        && status.message == TEST_FAILED
        && status.details.as_ref().is_none_or(|d| d.causes.is_empty())
    {
        return OculusError::new(
            ErrorCode::Conflict,
            "Alguien cambió en el clúster los mismos campos que has editado",
        )
        .with_hint("Mira las diferencias y decide si recargar o sobrescribir solo tus cambios.");
    }
    errors::from_kube(err)
}

fn event_info(e: Event) -> EventInfo {
    let last_seen = e
        .series
        .as_ref()
        .and_then(|s| s.last_observed_time.as_ref().map(|t| t.0.to_string()))
        .or_else(|| e.last_timestamp.as_ref().map(|t| t.0.to_string()))
        .or_else(|| e.event_time.as_ref().map(|t| t.0.to_string()))
        .or_else(|| e.metadata.creation_timestamp.as_ref().map(|t| t.0.to_string()));
    let count = e.series.as_ref().and_then(|s| s.count).or(e.count).unwrap_or(1).max(1) as u32;
    EventInfo {
        event_type: e.type_.unwrap_or_else(|| "Normal".into()),
        reason: e.reason.unwrap_or_default(),
        message: e.message.unwrap_or_default(),
        count,
        last_seen,
        source: e.source.and_then(|s| s.component).or(e.reporting_component),
    }
}

/// Esquema JSON del tipo para el editor, sacado de `/openapi/v3`. Se cachea por
/// grupo/versión en la sesión: el de `apps/v1` pesa unos 700 KB.
pub async fn schema_for(
    session: &ClusterSession,
    group: &str,
    version: &str,
    kind: &str,
) -> Result<Option<String>, OculusError> {
    let key = if group.is_empty() { format!("api/{version}") } else { format!("apis/{group}/{version}") };
    let started = std::time::Instant::now();
    let schemas = {
        let cached = session.schemas.lock().await.get(&key).cloned();
        match cached {
            Some(s) => s,
            None => {
                let fetched = std::sync::Arc::new(fetch_schemas(session, &key).await?);
                session.schemas.lock().await.insert(key.clone(), fetched.clone());
                fetched
            }
        }
    };
    let Some(name) = schema_name(&schemas, group, version, kind) else { return Ok(None) };
    let doc = schema_document(&schemas, &name);
    tracing::info!(%key, kind, ms = started.elapsed().as_millis() as u64, "esquema OpenAPI");
    Ok(Some(doc.to_string()))
}

/// El esquema del tipo como raíz, con todos los del grupo en `components` para
/// que resuelvan sus `#/components/schemas/…`. Una raíz con solo `$ref` no vale:
/// monaco-yaml convierte `$ref` con hermanos en `allOf` y la raíz se queda sin
/// `components`, así que no resolvería nada.
pub fn schema_document(schemas: &Value, name: &str) -> Value {
    let mut doc = schemas.get(name).cloned().unwrap_or_else(|| Value::Object(Default::default()));
    if let Some(root) = doc.as_object_mut() {
        root.insert("components".into(), serde_json::json!({ "schemas": schemas }));
    }
    doc
}

async fn fetch_schemas(session: &ClusterSession, key: &str) -> Result<Value, OculusError> {
    let get = |url: String| {
        let client = session.client.clone();
        async move {
            let req =
                http::Request::get(url).body(Vec::new()).map_err(|e| OculusError::internal(e.to_string()))?;
            client.request_text(req).await.map_err(|e| errors::from_kube(&e))
        }
    };
    let index: Value = serde_json::from_str(&get("/openapi/v3".into()).await?)
        .map_err(|e| OculusError::internal(e.to_string()))?;
    let Some(url) = index
        .pointer(&format!("/paths/{}/serverRelativeURL", key.replace('/', "~1")))
        .and_then(Value::as_str)
    else {
        return Ok(Value::Object(Default::default()));
    };
    let mut doc: Value = serde_json::from_str(&get(url.to_owned()).await?)
        .map_err(|e| OculusError::internal(e.to_string()))?;
    Ok(doc.pointer_mut("/components/schemas").map(Value::take).unwrap_or_default())
}

/// El esquema cuyo `x-kubernetes-group-version-kind` coincide con el tipo.
pub fn schema_name(schemas: &Value, group: &str, version: &str, kind: &str) -> Option<String> {
    schemas.as_object()?.iter().find_map(|(name, s)| {
        let gvks = s.get("x-kubernetes-group-version-kind")?.as_array()?;
        gvks.iter()
            .any(|g| {
                g.get("group").and_then(Value::as_str) == Some(group)
                    && g.get("version").and_then(Value::as_str) == Some(version)
                    && g.get("kind").and_then(Value::as_str) == Some(kind)
            })
            .then(|| name.clone())
    })
}

#[cfg(test)]
mod tests;
