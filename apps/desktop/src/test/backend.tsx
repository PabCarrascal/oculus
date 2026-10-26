import type { ClusterSummary, KubeconfigSource, ResourceKind, Settings } from "@oculus/bindings";
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

export interface BackendState {
  settings: Settings;
  sources: KubeconfigSource[];
  clusters: ClusterSummary[];
  kinds: ResourceKind[];
  namespaces: string[];
}

/** Simula el backend: guarda estado y registra cada llamada IPC. */
export function mockBackend(initial: Partial<BackendState> = {}) {
  const state: BackendState = {
    settings: { ...baseSettings },
    sources: [],
    clusters: [],
    kinds: [],
    namespaces: [],
    ...initial,
  };
  const calls: { cmd: string; args: unknown }[] = [];
  mockIPC(
    (cmd, args) => {
      calls.push({ cmd, args });
      // Copias, como el IPC real: la UI no debe compartir referencias con el backend.
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
    }
    return null;
  }
  return { state, calls };
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
