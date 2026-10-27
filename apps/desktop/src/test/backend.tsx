import type {
  ClusterSummary,
  KubeconfigSource,
  ResourceDetail,
  ResourceEvent,
  ResourceKind,
  Settings,
  UpdateResult,
} from "@oculus/bindings";
import { QueryClientProvider } from "@tanstack/react-query";
import { createMemoryHistory, createRouter, RouterProvider } from "@tanstack/react-router";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, render } from "@testing-library/react";
import { createQueryClient } from "../lib/queryClient";
import { routeTree } from "../router";

export const baseSettings: Settings = {
  theme: "system",
  locale: "es",
  shell: null,
  extraPath: [],
  telemetry: { crashReports: false },
  watchLingerSeconds: 30,
};

type ChannelLike = { onmessage: (event: ResourceEvent) => void };

export interface BackendState {
  settings: Settings;
  sources: KubeconfigSource[];
  clusters: ClusterSummary[];
  kinds: ResourceKind[];
  namespaces: string[];
  /** Channels de las suscripciones abiertas, por id. */
  channels: Map<string, ChannelLike>;
  /** Detalle por nombre de objeto. */
  details: Record<string, ResourceDetail>;
  /** Respuesta de resource_update: el resultado o el error que lanza. */
  update: (args: {
    baseJson: string;
    editedJson: string;
    dryRun: boolean;
    force: boolean;
  }) => UpdateResult;
  secrets: Record<string, string>;
}

/** Simula el backend: guarda estado y registra cada llamada IPC. */
export function mockBackend(initial: Partial<BackendState> = {}) {
  const state: BackendState = {
    settings: { ...baseSettings },
    sources: [],
    clusters: [],
    kinds: [],
    namespaces: [],
    channels: new Map(),
    details: {},
    update: ({ editedJson }) => ({ resourceVersion: "2", objectJson: editedJson }),
    secrets: {},
    ...initial,
  };
  const calls: { cmd: string; args: unknown }[] = [];
  mockIPC(
    (cmd, args) => {
      calls.push({ cmd, args });
      // Copias, como el IPC real: la UI no debe compartir referencias con el backend.
      // El Channel no se copia: es el objeto vivo por el que el "backend" envía.
      if (cmd === "resources_subscribe") {
        const id = `w${state.channels.size + 1}`;
        state.channels.set(id, (args as { channel: ChannelLike }).channel);
        return id;
      }
      return structuredClone(respond(cmd, args));
    },
    { shouldMockEvents: true },
  );
  function respond(cmd: string, args: unknown): unknown {
    const a = args as Record<string, unknown>;
    switch (cmd) {
      case "settings_get":
        return state.settings;
      case "settings_set":
        state.settings = { ...state.settings, ...(a.patch as Partial<Settings>) };
        return state.settings;
      case "app_info":
        return { version: "0.1.0", os: "linux", arch: "x86_64" };
      case "startup_report":
        return { processToFirstPaintMs: 420 };
      case "kubeconfig_list_sources":
        return state.sources;
      case "clusters_list":
        return state.clusters;
      case "cluster_connect":
      case "cluster_disconnect":
        return null;
      case "cluster_set_prefs":
        return a.prefs;
      case "discovery_get":
        return state.kinds;
      case "namespaces_list":
        return state.namespaces;
      case "resources_unsubscribe":
        state.channels.delete(a.subscriptionId as string);
        return null;
      case "resources_reset":
        return null;
      case "resource_get": {
        const d = state.details[(a.target as { name: string }).name];
        if (!d) throw { code: "NotFound", message: "no existe", details: null };
        return d;
      }
      case "resource_update":
        return state.update(a as never);
      case "secret_reveal":
        return state.secrets[a.key as string] ?? "";
      case "openapi_schema":
        return null;
    }
    return null;
  }
  /** Envía un evento por el Channel de una suscripción (la última si no se indica). */
  const push = (event: ResourceEvent, id?: string) => {
    const channel = id ? state.channels.get(id) : [...state.channels.values()].at(-1);
    if (!channel) throw new Error("no hay suscripciones abiertas");
    channel.onmessage(structuredClone(event));
  };
  return { state, calls, push };
}

export function renderApp(path = "/") {
  const client = createQueryClient();
  client.setQueryData(["settings"], baseSettings);
  const router = createRouter({
    routeTree,
    history: createMemoryHistory({ initialEntries: [path] }),
  });
  render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return { router, client };
}

/**
 * Desmonta, espera a que los listeners se den de baja (es asíncrono) y solo
 * entonces quita los mocks de Tauri.
 */
export async function teardown() {
  cleanup();
  await new Promise((resolve) => setTimeout(resolve, 0));
  clearMocks();
}
