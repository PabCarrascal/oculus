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

### Túnel SSH por clúster (D13)

> **Implementado** (`crates/oculus-core/src/tunnel`), con estos añadidos al diseño:
> - `ClusterSummary.hasProxyUrl`: el kubeconfig ya define `proxy-url` para ese clúster. Solo el booleano; la URL no se envía, porque puede llevar credenciales.
> - `ssh` recibe el `PATH` efectivo y el `SSH_AUTH_SOCK` de la shell de login: el agente que ve el usuario en su terminal (1Password, Secretive…) no tiene por qué ser el que hereda la app de launchd.
> - "Listo" es un saludo SOCKS5 completo, no solo que el puerto acepte conexiones, para no dar por bueno otro proceso que ocupe el puerto.
> - Los pids van en `tunnels.json`, junto a `workspace.json`. La limpieza de huérfanos al arrancar solo existe en macOS y Linux (usa `ps`). En Windows queda pendiente un Job Object; mientras tanto, un `ssh` huérfano cae solo por `ServerAlive` cuando el bastión deja de responder.

Cambios en el contrato:

```ts
interface ClusterPrefs {
  // …lo de arriba
  sshTunnel?: { host: string };  // alias de ~/.ssh/config o usuario@host[:puerto]
}

type ConnectStep = "tunnel" | "auth" | "version" | "discovery";  // "tunnel" solo si hay túnel

// En el estado "connected", junto a serverVersion y degraded:
tunnel: null | { host: string; state: "up" | "reconnecting" };

// Para el diálogo de preferencias: lo que ssh entiende de ese host, sin conectar (`ssh -G`).
ssh_tunnel_check(host: string): { hostname: string; user: string; port: number; proxyJump?: string }
```

Cómo funciona:

- **Validación del host.** No vacío, sin espacios ni caracteres de control, sin `-` inicial y como mucho 255 caracteres. Además va siempre detrás de `--`: comprobado que así `-oProxyCommand=…` se rechaza como nombre de host en vez de tomarse como opción. `ssh` no entiende `host:puerto` (lo toma por un nombre de host), así que `usuario@host:puerto` se le pasa como `ssh://usuario@host:puerto`. Los alias y los hosts sin puerto van tal cual.
- **Binario.** `ssh` se busca en el `PATH` de la shell de login más `extraPath`, como los exec plugins (riesgo 1). En Windows, también en `System32\OpenSSH`, y se lanza sin ventana de consola.
- **Puerto.** `-D` no admite el puerto 0 (comprobado), así que Oculus pide un puerto libre al sistema, lo suelta y se lo pasa a `ssh`. Si otro proceso lo ocupa entre medias, `ExitOnForwardFailure` hace que `ssh` termine y se reintenta con otro puerto (tres veces como mucho).
- **Línea de órdenes:** `ssh -N -D 127.0.0.1:PUERTO -o BatchMode=yes -o ExitOnForwardFailure=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 -o ControlMaster=no -o ControlPath=none -- HOST`.
  - `ControlMaster=no` y `ControlPath=none`: si el usuario usa multiplexación, el `-D` acabaría en un proceso maestro ajeno que Oculus no puede vigilar ni parar.
  - La entrada y la salida estándar van a `null`. De la salida de error se guardan las últimas 20 líneas en memoria, para clasificar el fallo.
- **Listo.** `ssh` solo abre el `-D` después de autenticarse. El túnel está listo cuando `127.0.0.1:PUERTO` acepta conexiones (se comprueba cada 100 ms). Falla si `ssh` termina antes o si pasan 15 s.
- **Cliente.** `proxy_url = socks5://127.0.0.1:PUERTO`. kube-rs solo acepta el esquema `socks5`, no `socks5h`, pero ya resuelve los nombres en el bastión. Si el kubeconfig trae su propio `proxy-url`, **gana el túnel**: se registra un aviso y el diálogo de preferencias lo indica. El `connect_timeout` de 5 s cubre el saludo SOCKS y TLS, porque kube-rs lo aplica sobre el conector completo.
- **Un túnel por host, compartido.** Si varios clústeres usan el mismo bastión, comparten un único `ssh` con un contador de referencias, para no autenticar (ni pedir que se toque una llave física) una vez por clúster. Cada sesión guarda su parte del túnel. El `ssh` se cierra cuando lo suelta el último clúster que lo usa: al desconectar, o si la conexión falla después de levantar el túnel.
- **Si el túnel se cae** (`ssh` termina con la sesión abierta), la sesión sigue en `connected` con `tunnel.state = "reconnecting"` y `degraded = true`. `ssh` se relanza **en el mismo puerto** con espera creciente (1, 2, 4… hasta 30 s). Como el puerto no cambia, el `Client` sigue valiendo y los watchers se recuperan solos con su propio reintento. Se abandona, y el clúster pasa a `error`, si el relanzamiento falla al autenticar o si el puerto ya no está libre; al reconectar se elige otro.
- **Al salir de la app**, los procesos `ssh` se matan uno a uno por su pid, de forma síncrona. No basta con soltar los túneles: eso solo programa su cierre en el runtime, y el proceso puede terminar antes de que se ejecute. Por si la app se cuelga, los pid se anotan en el directorio de datos. Al arrancar se matan los que sigan vivos, pero solo tras comprobar que su línea de órdenes es un `ssh -D 127.0.0.1:…` (un pid se puede reutilizar).
- **Errores** a partir de la salida de error de `ssh`:

  | Salida de `ssh` | `code` | Pista |
  | --- | --- | --- |
  | `Permission denied` | `Unauthorized` | Carga la clave en el agente (`ssh-add`). Los bastiones con segundo factor interactivo aún no están soportados. |
  | `Host key verification failed`, `REMOTE HOST IDENTIFICATION HAS CHANGED` | `Unauthorized` | Conéctate una vez desde la terminal (`ssh HOST`) para revisar y aceptar la huella. Oculus nunca la acepta por su cuenta. |
  | `Could not resolve hostname`, `Connection refused`, `timed out`, `No route to host` | `Unreachable` | La última línea de `ssh`. |
  | `administratively prohibited` (al abrir el canal) | `Unreachable` | El bastión no permite reenvío de puertos (`AllowTcpForwarding`). |
  | `ssh` no encontrado | `NotFound` | No se encontró `ssh` en el `PATH`. |

  La última línea de `ssh` va en la pista (truncada y pasada por `scrub`). Solo contiene host, usuario y puerto; `ssh` no escribe secretos ahí.
- **Trazas:** túnel arriba o abajo, con host, puerto, tiempo y código de salida. Nunca el entorno.
- **Lo que no pasa por el túnel:** los exec plugins (`aws`, `gke-gcloud-auth-plugin`, `kubelogin`), que hablan directamente con su nube.
- **Terminal (etapa 5, D8):** `kubectl` en la terminal integrada no pasaría por el túnel. Hay dos vías, a decidir en la etapa 5:
  - poner `HTTPS_PROXY=socks5://127.0.0.1:PUERTO` en el entorno del PTY: Go lo respeta, pero también afecta a `curl` y a las CLIs de nube, así que hace falta `NO_PROXY`;
  - una entrada de clúster con `proxy-url` en el kubeconfig mínimo: copia `server` y la CA, que no son credenciales, pero rompe la regla de D8 de copiar solo nombres.

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
