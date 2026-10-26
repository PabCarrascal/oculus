// Backend simulado para abrir la UI en un navegador sin Tauri (`pnpm dev:web`).
// Solo se carga en desarrollo; la build de producción lo elimina.
import type {
  ClusterStatus,
  ClusterSummary,
  KubeconfigSource,
  ResourceKind,
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

export function installBrowserMock() {
  mockIPC((cmd, args) => structuredClone(handle(cmd, args)), { shouldMockEvents: true });
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
      return ["default", "kube-node-lease", "kube-public", "kube-system", "local-path-storage"];
  }
  return null;
}
