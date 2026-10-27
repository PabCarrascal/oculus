// Backend simulado para abrir la UI en un navegador sin Tauri (`pnpm dev:web`).
// Solo se carga en desarrollo; la build de producción lo elimina.
import type {
  ClusterStatus,
  ClusterSummary,
  ColumnDef,
  KubeconfigSource,
  ResourceEvent,
  ResourceKind,
  Row,
  Settings,
} from "@oculus/bindings";
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";

const prefs = { alias: null, color: null, readOnly: false, protected: false };
const sources: KubeconfigSource[] = [
  { id: "s1", path: "~/.kube/config", origin: "default", error: null },
  { id: "s2", path: "~/work/clientes.yaml", origin: "manual", error: null },
  {
    id: "s3",
    path: "~/.kube/roto.yaml",
    origin: "manual",
    error: "did not find expected node content at line 3",
  },
];
const clusters: ClusterSummary[] = [
  {
    id: "kind",
    contextName: "kind-oculus-it",
    sourceId: "s1",
    server: "https://127.0.0.1:52341",
    defaultNamespace: null,
    prefs,
    status: { state: "connected", serverVersion: "v1.37.0", degraded: false },
  },
  {
    id: "prod",
    contextName: "arn:aws:eks:eu-west-1:123456789012:cluster/prod",
    sourceId: "s1",
    server: "https://A1B2C3.gr7.eu-west-1.eks.amazonaws.com",
    defaultNamespace: "payments",
    prefs: { alias: "prod-eu", color: "#c8323b", readOnly: false, protected: true },
    status: {
      state: "error",
      error: {
        code: "Unauthorized",
        message: "No se encontró el exec plugin «aws»",
        details: {
          status: null,
          hint: "Instálalo o añade su carpeta en Ajustes › Rutas extra para binarios.",
          causes: [],
        },
      },
    },
  },
  {
    id: "staging",
    contextName: "gke_acme_europe-west1_staging",
    sourceId: "s2",
    server: "https://34.90.1.2",
    defaultNamespace: null,
    prefs: { alias: "staging", color: "#1f8a4c", readOnly: true, protected: false },
    status: { state: "disconnected" },
  },
];
const kinds: ResourceKind[] = [
  ["", "Pod", "workloads"],
  ["apps", "Deployment", "workloads"],
  ["apps", "StatefulSet", "workloads"],
  ["batch", "Job", "workloads"],
  ["", "ConfigMap", "config"],
  ["", "Secret", "config"],
  ["", "Service", "network"],
  ["networking.k8s.io", "Ingress", "network"],
  ["", "PersistentVolumeClaim", "storage"],
  ["rbac.authorization.k8s.io", "Role", "access"],
  ["", "Namespace", "cluster"],
  ["", "Node", "cluster"],
  ...Array.from(
    { length: 12 },
    (_, i) => [`g${i + 1}.oculus.test`, `Widget${i + 1}`, "custom"] as const,
  ),
].map(([group, kind, category]) => ({
  group,
  version: "v1",
  kind,
  plural: `${kind.toLowerCase()}s`,
  namespaced: kind !== "Namespace" && kind !== "Node",
  verbs: ["get", "list", "watch"],
  category: category as ResourceKind["category"],
}));

let settings: Settings = {
  theme: "system",
  locale: "es",
  shell: null,
  extraPath: [],
  telemetry: { crashReports: false },
  watchLingerSeconds: 30,
};

function setStatus(id: string, status: ClusterStatus, delay: number) {
  setTimeout(() => {
    update(id, { status });
    void emit("cluster-status-changed", { clusterId: id, status });
  }, delay);
}

// Se sustituye el objeto en lugar de modificarlo: la UI nunca debe compartir
// referencias con el "backend", igual que con el IPC real.
function update(id: string, patch: Partial<ClusterSummary>) {
  const i = clusters.findIndex((c) => c.id === id);
  if (i >= 0) clusters[i] = { ...clusters[i], ...patch } as ClusterSummary;
}

type ChannelLike = { onmessage: (event: ResourceEvent) => void };

// ?rows=10000&churn=500 en la URL: tamaño de la lista de pods y cambios por segundo.
const params = new URLSearchParams(location.search);
const ROWS = Number(params.get("rows") ?? 300);
const CHURN = Number(params.get("churn") ?? 2);
const PHASES = [
  "Running",
  "Running",
  "Running",
  "Pending",
  "CrashLoopBackOff",
  "Completed",
] as const;
const subscriptions = new Map<string, () => void>();

function syntheticPod(i: number, generation = 0): Row {
  const phase = PHASES[(i + generation) % PHASES.length] as string;
  const tone =
    phase === "Running"
      ? "ok"
      : phase === "Pending"
        ? "progress"
        : phase === "Completed"
          ? "neutral"
          : "error";
  return {
    uid: `pod-${i}`,
    name: `app-${i % 97}-${i.toString(36).padStart(5, "0")}`,
    namespace: `team-${i % 12}`,
    createdAt: new Date(Date.now() - ((i * 7919) % 864_000) * 1000).toISOString(),
    status: { tone, text: phase },
    cells: {
      ready: tone === "ok" ? "1/1" : "0/1",
      restarts: (i * 31 + generation) % 7,
      node: `node-${i % 40}`,
      labels: { labels: { app: `app-${i % 97}`, tier: i % 2 ? "web" : "worker" } },
    },
  };
}

const POD_COLUMNS: ColumnDef[] = [
  ["name", "string", true],
  ["namespace", "string", true],
  ["ready", "string", true],
  ["status", "string", true],
  ["restarts", "number", true],
  ["node", "string", true],
  ["age", "date", true],
  ["labels", "labels", false],
].map(([id, type, visible]) => ({
  id: id as string,
  label: `columns.${id}`,
  translate: true,
  type: type as ColumnDef["type"],
  defaultVisible: visible as boolean,
}));

/** Igual que el backend: snapshot, luego deltas agrupados cada 80 ms. */
function subscribe(channel: ChannelLike): string {
  const id = `w${subscriptions.size + 1}-${Date.now()}`;
  const send = (e: ResourceEvent) => channel.onmessage(structuredClone(e));
  send({ type: "status", state: "initializing", error: null });
  const rows = Array.from({ length: ROWS }, (_, i) => syntheticPod(i));
  let generation = 0;
  let pending = new Map<string, Row>();
  const start = setTimeout(() => {
    send({ type: "snapshot", columns: POD_COLUMNS, rows });
    send({ type: "status", state: "ready", error: null });
  }, 250);
  const churn = setInterval(() => {
    for (let n = 0; n < (CHURN * 80) / 1000; n++) {
      generation++;
      const i = (generation * 7919) % ROWS;
      pending.set(`pod-${i}`, syntheticPod(i, generation));
    }
    if (pending.size > 0) {
      send({ type: "delta", upserts: [...pending.values()], deletes: [] });
      pending = new Map();
    }
  }, 80);
  subscriptions.set(id, () => {
    clearTimeout(start);
    clearInterval(churn);
  });
  return id;
}

export function installBrowserMock() {
  mockIPC(
    (cmd, args) => {
      if (cmd === "resources_subscribe")
        return subscribe((args as { channel: ChannelLike }).channel);
      return structuredClone(handle(cmd, args));
    },
    { shouldMockEvents: true },
  );
}

function handle(cmd: string, args: unknown): unknown {
  const a = (args ?? {}) as Record<string, unknown>;
  switch (cmd) {
    case "settings_get":
      return settings;
    case "settings_set":
      settings = { ...settings, ...(a.patch as Partial<Settings>) };
      return settings;
    case "app_info":
      return { version: "0.1.0-dev", os: "browser", arch: "mock" };
    case "startup_report":
      return { processToFirstPaintMs: performance.now() };
    case "kubeconfig_list_sources":
      return sources;
    case "clusters_list":
      return clusters;
    case "cluster_connect": {
      const id = a.clusterId as string;
      setStatus(id, { state: "connecting", step: "auth" }, 0);
      setStatus(id, { state: "connecting", step: "version" }, 400);
      setStatus(id, { state: "connecting", step: "discovery" }, 800);
      setStatus(id, { state: "connected", serverVersion: "v1.36.2", degraded: false }, 1200);
      return null;
    }
    case "cluster_disconnect":
      setStatus(a.clusterId as string, { state: "disconnected" }, 0);
      return null;
    case "cluster_set_prefs":
      update(a.clusterId as string, { prefs: a.prefs as ClusterSummary["prefs"] });
      return a.prefs;
    case "discovery_get":
      return kinds;
    case "namespaces_list":
      return ["default", ...Array.from({ length: 12 }, (_, i) => `team-${i}`), "kube-system"];
    case "resources_unsubscribe": {
      const stop = subscriptions.get(a.subscriptionId as string);
      stop?.();
      subscriptions.delete(a.subscriptionId as string);
      return null;
    }
    case "resource_get": {
      // Un Pod sintético para desarrollar el detalle sin clúster.
      const t = a.target as { name: string; namespace: string | null };
      const pod = {
        apiVersion: "v1",
        kind: "Pod",
        metadata: {
          name: t.name,
          namespace: t.namespace,
          uid: `uid-${t.name}`,
          resourceVersion: "1",
          labels: { app: t.name.split("-").slice(0, 2).join("-") },
        },
        spec: {
          nodeName: "node-1",
          containers: [{ name: "app", image: "registry.example/app:1.4.2" }],
        },
        status: {
          phase: "Running",
          containerStatuses: [
            { name: "app", ready: true, restartCount: 2, state: { running: {} } },
          ],
        },
      };
      return {
        apiVersion: "v1",
        kind: "Pod",
        uid: `uid-${t.name}`,
        resourceVersion: "1",
        objectJson: JSON.stringify(pod),
        events: [
          {
            type: "Normal",
            reason: "Started",
            message: "Started container app",
            count: 1,
            lastSeen: new Date().toISOString(),
            source: "kubelet",
          },
        ],
        related: [{ relation: "node", kind: "Node", name: "node-1", target: null }],
        editable: true,
        secretKeys: null,
      };
    }
    case "resource_update":
      return { resourceVersion: "2", objectJson: a.editedJson };
    case "openapi_schema":
      // Un esquema mínimo, para ver la validación del editor sin clúster.
      return JSON.stringify({
        type: "object",
        properties: {
          apiVersion: { type: "string" },
          kind: { type: "string" },
          metadata: { type: "object" },
          spec: {
            type: "object",
            properties: {
              nodeName: { type: "string", description: "Nodo en el que se programa el Pod." },
            },
          },
          status: { type: "object" },
        },
      });
    case "resources_reset":
      for (const stop of subscriptions.values()) stop();
      subscriptions.clear();
      return null;
  }
  return null;
}
