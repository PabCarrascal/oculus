# 03 · Contrato backend ↔ frontend

Los tipos se definen en Rust (`crates/oculus-protocol`) y `tauri-specta` los genera en TypeScript (`packages/bindings`). Aquí aparecen en TypeScript porque es como los consume la UI. Rust es la fuente de verdad.

Hay tres mecanismos:
- **Comandos** (`invoke`): petición → respuesta. Devuelven `Result<T, OculusError>`.
- **Channels:** un stream por operación (suscripción a recursos, logs, exec, terminal). Lo crea la UI, se pasa como argumento a un comando y se cierra con el comando `*_close` correspondiente.
- **Eventos globales:** difusión de cosas poco frecuentes que interesan a toda la app.

## Tipos base

```ts
/** sha256(rutaCanónicaDelKubeconfig + "\0" + nombreDelContexto), en hex, 16 caracteres. Estable entre reinicios. */
type ClusterId = string;
type SubscriptionId = string; // uuid
type StreamId = string; // uuid; logs, exec, terminal y port-forward

interface ResourceKind {
  group: string; // "" para el core
  version: string;
  kind: string;
  plural: string;
  namespaced: boolean;
  verbs: string[]; // del discovery
  category: Category; // "workloads" | "config" | "network" | "storage" | "access" | "cluster" | "custom"
  columns: ColumnDef[]; // etapa 2: definidas en Rust o desde additionalPrinterColumns
}

interface ColumnDef {
  id: string;
  label: string; // clave i18n para tipos nativos; texto literal para CRDs
  type: "string" | "number" | "date" | "status" | "labels";
  defaultVisible: boolean;
}

interface ObjectRef {
  clusterId: ClusterId;
  group: string;
  version: string;
  plural: string;
  namespace?: string;
  name: string;
}

interface Row {
  uid: string;
  name: string;
  namespace?: string;
  createdAt: string; // ISO 8601; la "edad" se calcula en la UI
  status: { tone: "ok" | "warning" | "error" | "progress" | "neutral"; text: string };
  cells: Record<string, CellValue>; // clave = ColumnDef.id
}

type CellValue = string | number | null | { labels: Record<string, string> };
```

## Errores

Un solo tipo para todo el contrato. La UI decide cómo mostrarlo según el `code`.

```ts
interface OculusError {
  code:
    | "Unauthorized" // 401, token caducado, fallo del exec plugin
    | "Forbidden" // 403 de RBAC
    | "NotFound"
    | "Conflict" // 409, resourceVersion obsoleta al aplicar
    | "Invalid" // 422, validación o dry-run fallido
    | "Unreachable" // red, DNS, TLS, timeout
    | "Cancelled"
    | "Internal";
  message: string; // ya traducido o listo para traducir
  details?: {
    status?: number;
    causes?: { field?: string; message: string }[]; // de Status.details.causes de Kubernetes
    hint?: string; // por ejemplo "No se encontró 'aws' en el PATH"
  };
}
```

## Kubeconfig y clústeres

```ts
kubeconfig_list_sources(): KubeconfigSource[]
kubeconfig_pick_source(): KubeconfigSource | null  // abre el selector nativo desde Rust; null si se cancela
kubeconfig_remove_source(sourceId: string): void

interface KubeconfigSource { id: string; path: string; origin: "default" | "env" | "manual"; error?: string }

clusters_list(): ClusterSummary[]
cluster_connect(clusterId: ClusterId): void           // vuelve rápido; el progreso llega por evento
cluster_disconnect(clusterId: ClusterId): void
cluster_set_prefs(clusterId: ClusterId, prefs: ClusterPrefs): void

interface ClusterSummary {
  id: ClusterId;
  contextName: string;
  sourceId: string;
  server: string;         // URL del API server (solo se muestra)
  defaultNamespace?: string;
  prefs: ClusterPrefs;
  status: ClusterStatus;
}

interface ClusterPrefs {
  alias?: string;
  color?: string;
  readOnly: boolean;      // oculta todas las acciones que modifican el clúster
  protected: boolean;     // las acciones destructivas exigen escribir el nombre del recurso
}

type ClusterStatus =
  | { state: "disconnected" }
  | { state: "connecting"; step: "auth" | "version" | "discovery" }
  | { state: "connected"; serverVersion: string; degraded: boolean }  // degraded = algún watcher en error
  | { state: "error"; error: OculusError };  // sin reintento automático: el usuario pulsa Reintentar

discovery_get(clusterId: ClusterId): ResourceKind[]
namespaces_list(clusterId: ClusterId): string[]   // los que se pueden listar; si listar todos da 403, los de las preferencias
```

> **Implementado en la etapa 2** (`commands/resources.rs`), con estos cambios respecto a lo de abajo:
> - `kind` es un `KindRef { group, version, plural }` y `scope` es `{ kind: "all" } | { kind: "namespaces", names }`.
> - Las **columnas llegan en el `snapshot`** (`{ type: "snapshot", columns, rows }`), no en `ResourceKind`: para los CRDs hay que leer su definición y eso solo tiene sentido al abrir la vista.
> - `resources_unsubscribe(clusterId, subscriptionId)`.
> - **`resources_reset()`**: la UI lo llama al arrancar. Una recarga del webview no avisa al backend y los Channels de la carga anterior ya no tienen receptor (Tauri no da error al enviarles), así que sin esto cada recarga dejaría watchers vivos.
> - `status` lleva `error: OculusError | null`; no hay `resourceVersion` en el snapshot.
> - **Limitación conocida:** con varios namespaces, un 403 en uno solo detiene toda la vista, aunque los demás sí se puedan leer, y la pista "elige namespaces" aparece aunque ya se hayan elegido. Se resolverá con el filtrado por RBAC de la etapa 4.

## Suscripción a recursos (ciclo de vida)

```ts
resources_subscribe(
  clusterId: ClusterId,
  kind: { group: string; version: string; plural: string },
  scope: { allNamespaces: true } | { namespaces: string[] },
  channel: Channel<ResourceEvent>
): SubscriptionId

resources_unsubscribe(sub: SubscriptionId): void

type ResourceEvent =
  | { type: "snapshot"; rows: Row[]; resourceVersion: string }
  | { type: "delta"; upserts: Row[]; deletes: string[] /* uids */ }
  | { type: "status"; state: "initializing" | "ready" | "relisting" | "backoff" | "error"; error?: OculusError };
```

1. La UI crea un `Channel` y llama a `resources_subscribe`.
2. Llega `status: initializing` y después un `snapshot` completo.
3. Llegan `delta` agrupados cada ~80 ms (solo si hay cambios).
4. Si el watch se rompe (por ejemplo, un 410), llega `status: relisting` y, después, un `snapshot` nuevo que **sustituye** al anterior.
5. Al cambiar de vista, `resources_unsubscribe`. El backend mantiene el watcher 30 s por si se vuelve.

Si dos vistas piden lo mismo, comparten watcher en el backend: cada una recibe su propio snapshot inicial y, a partir de ahí, los mismos deltas.

> **Implementado en la etapa 3** (`commands/detail.rs`), con estos cambios respecto a lo de abajo:
> - **`resource_get(clusterId, target)`** devuelve el objeto como **JSON** (`objectJson`, sin `managedFields`); el YAML lo genera la UI con la librería `yaml`, que también sirve para llevar un error a su línea. Incluye `editable` (verbo update, clúster no en solo lectura, no es un Secret) en lugar de `allowedActions`; el filtrado por RBAC es de la etapa 4.
> - **`resource_update(clusterId, target, baseJson, editedJson, dryRun, force)`** en lugar de `resource_apply`. El backend calcula un **JSON Patch** entre el objeto tal como se cargó y el editado, con una operación `test` antes de cada campo que se sustituye o se quita. Si otro cambió esos mismos campos, el parche entero se rechaza y la UI ofrece ver diferencias, recargar o sobrescribir (`force`: el mismo diff sin los `test`, que pisa solo lo que el usuario tocó). Se descartaron:
>   - *PUT con `resourceVersion`*: el controlador de un Deployment reescribe el estado varias veces por segundo durante un despliegue y cada escritura cambia la versión, así que editar daba conflicto aunque nadie hubiera tocado la `spec` (comprobado contra kind).
>   - *Server-side apply*: deja intactos los campos de otros gestores y da conflictos de propiedad con todo lo creado por `kubectl` o por los controladores.
>   - Aplicar YAML nuevo (crear objetos) con server-side apply queda para la etapa 4.
> - Validación estricta (`fieldValidation=Strict`): un campo desconocido es un error. Con PATCH llega como 422 con el objeto entero en el mensaje; el backend extrae la ruta (`spec.replicass`) a `details.causes` y descarta el volcado.
> - **`secret_reveal(clusterId, target, key)`**: un valor cada vez. Los Secrets todavía no se pueden editar desde Oculus (su YAML es de solo lectura, con los valores vacíos).
> - **`openapi_schema(clusterId, group, version, kind)`**: esquema JSON del tipo (de `/openapi/v3`, cacheado por grupo/versión en la sesión) para validar mientras se escribe. El documento tiene el esquema del tipo en la raíz y todos los del grupo en `components.schemas`, para que resuelvan sus `$ref`; una raíz con solo `$ref` no la resuelve `monaco-yaml`.

## Detalle y modificación

```ts
resource_get(ref: ObjectRef): ResourceDetail
interface ResourceDetail {
  object: unknown;          // JSON completo, sin managedFields salvo que se pida
  yaml: string;
  resourceVersion: string;
  events: KubeEvent[];      // eventos cuyo involvedObject es este recurso
  related: RelatedRef[];    // propietarios, hijos y relaciones por selector
  allowedActions: Action[]; // ya filtradas por RBAC y por readOnly
}

interface RelatedRef { ref: ObjectRef; kind: string; relation: "owner" | "owned" | "selects" | "selectedBy" | "mounts" }
type Action = "edit" | "delete" | "scale" | "restart" | "logs" | "exec" | "portForward";

resource_apply(clusterId: ClusterId, yaml: string, opts: { dryRun: boolean; force: boolean }): ApplyResult
interface ApplyResult { applied: ObjectRef[]; warnings: string[] }   // Invalid o Conflict llegan como OculusError

resource_delete(refs: ObjectRef[], opts: { propagation: "Foreground" | "Background" | "Orphan"; gracePeriodSeconds?: number }): DeleteResult
interface DeleteResult { deleted: ObjectRef[]; failed: { ref: ObjectRef; error: OculusError }[] }

workload_scale(ref: ObjectRef, replicas: number): void
workload_restart(ref: ObjectRef): void   // parchea spec.template.metadata.annotations["kubectl.kubernetes.io/restartedAt"]

secret_reveal(ref: ObjectRef, key: string): string   // acción explícita; queda en el log de auditoría local, sin el valor

rbac_check(clusterId: ClusterId, checks: AccessCheck[]): boolean[]
interface AccessCheck { verb: string; group: string; resource: string; namespace?: string; subresource?: string }

openapi_schema(clusterId: ClusterId, kind: { group: string; version: string; kind: string }): unknown  // JSON Schema para el editor
```

## Logs

```ts
logs_open(
  ref: ObjectRef,             // Pod, o Deployment/StatefulSet/Job: en ese caso se agregan todos sus pods
  opts: { containers: string[] | "all"; follow: boolean; tailLines?: number; sinceSeconds?: number; timestamps: boolean; previous: boolean },
  channel: Channel<LogEvent>
): StreamId

type LogEvent =
  | { type: "lines"; lines: { pod: string; container: string; ts?: string; text: string }[] }
  | { type: "status"; state: "streaming" | "ended" | "reconnecting"; pod?: string; error?: OculusError };

logs_download(streamId: StreamId, path: string): void   // vuelca el búffer a un fichero elegido por el usuario
stream_close(streamId: StreamId): void                    // común a logs, exec y terminal
```

## Exec en pods y terminal local

La entrada es poca y va por comandos. La salida usa un **Channel binario**: bytes crudos, sin JSON.

```ts
exec_open(
  ref: ObjectRef,
  opts: { container: string; command: string[]; tty: boolean; cols: number; rows: number },
  output: Channel<ArrayBuffer>,
  status: Channel<{ state: "open" | "exited"; exitCode?: number; error?: OculusError }>
): StreamId

terminal_open(
  opts: { clusterId?: ClusterId; namespace?: string; cwd?: string; shell?: string; cols: number; rows: number },
  output: Channel<ArrayBuffer>,
  status: Channel<{ state: "open" | "exited"; exitCode?: number }>
): StreamId

stream_write(streamId: StreamId, data: Uint8Array): void   // la UI agrupa las pulsaciones por frame
stream_resize(streamId: StreamId, cols: number, rows: number): void
```

## Port-forward

Son recursos gestionados: siguen vivos aunque se cierre la vista que los creó y aparecen en una lista global.

```ts
portforward_start(
  target: { ref: ObjectRef /* Pod o Service */; remotePort: number },
  localPort?: number          // si no se indica, se elige uno libre
): PortForward

portforward_stop(id: StreamId): void
portforward_list(): PortForward[]

interface PortForward {
  id: StreamId;
  clusterId: ClusterId;
  target: ObjectRef;
  remotePort: number;
  localPort: number;          // siempre en 127.0.0.1
  state: "active" | "reconnecting" | "error";
  connections: number;
  error?: OculusError;
}
```

En un Service se resuelve un pod listo detrás del selector. Si ese pod muere, se busca otro y el estado pasa por `reconnecting`.

## Ajustes

```ts
settings_get(): Settings
settings_set(patch: Partial<Settings>): Settings

interface Settings {
  theme: "system" | "light" | "dark";
  locale: "es" | "en";
  shell?: string;
  extraPath: string[];          // rutas extra para encontrar aws, gcloud, kubelogin…
  telemetry: { crashReports: boolean };   // false por defecto
  watchLingerSeconds: number;   // 30 por defecto
}
```

> **Implementado en la etapa 1** (`commands/clusters.rs`): kubeconfig, clústeres, discovery y namespaces, más los eventos `cluster-status-changed` y `kubeconfig-changed`. Respecto al diseño original:
> - Para añadir un fichero se usa `kubeconfig_pick_source()`, que abre el selector desde Rust, en lugar de `kubeconfig_add_source(path)`. Así el webview no tiene permiso para abrir diálogos ni elegir rutas.
> - Los eventos se llaman como los genera tauri-specta (`cluster-status-changed`), no `cluster://status`.
> - `ResourceKind.columns` llega en la etapa 2, y el estado de error no reintenta solo.

## Eventos globales

| Evento                  | Payload                                | Cuándo                                                                     |
| ----------------------- | -------------------------------------- | -------------------------------------------------------------------------- |
| `cluster-status-changed` | `{ clusterId, status: ClusterStatus }` | Cambia el estado de conexión de un clúster                                 |
| `kubeconfig-changed`     | `null`                                 | Cambia un kubeconfig vigilado, la lista de fuentes o el `KUBECONFIG` de la shell |
| `portforward://changed` | `PortForward[]`                        | Se crea, se para o cambia de estado un port-forward                        |
| `app://error`           | `OculusError`                          | Error no asociado a ninguna vista (por ejemplo, un fallo en segundo plano) |
