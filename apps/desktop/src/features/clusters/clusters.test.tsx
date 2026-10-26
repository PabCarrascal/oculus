import type { ClusterStatus, ClusterSummary } from "@oculus/bindings";
import { initI18n } from "@oculus/i18n";
import { emit } from "@tauri-apps/api/event";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { mockBackend, renderApp, teardown } from "../../test/backend";

const source = { id: "s1", path: "/home/u/.kube/config", origin: "default" as const, error: null };

function cluster(over: Partial<ClusterSummary> = {}): ClusterSummary {
  return {
    id: "c1",
    contextName: "kind-oculus-it",
    sourceId: "s1",
    server: "https://127.0.0.1:6443",
    defaultNamespace: null,
    prefs: { alias: null, color: null, readOnly: false, protected: false },
    status: { state: "disconnected" },
    ...over,
  };
}

const status = (clusterId: string, s: ClusterStatus) =>
  act(() => emit("cluster-status-changed", { clusterId, status: s }));

beforeAll(async () => {
  await initI18n("es");
});

afterEach(teardown);

describe("clústeres", () => {
  it("conecta y refleja el estado que llega por eventos", async () => {
    const { calls } = mockBackend({ sources: [source], clusters: [cluster()] });
    renderApp();

    const row = (await screen.findByRole("link", { name: "kind-oculus-it" })).closest(
      "li",
    ) as HTMLElement;
    expect(within(row).getByText("Desconectado")).toBeInTheDocument();
    fireEvent.click(within(row).getByRole("button", { name: "Conectar" }));
    await waitFor(() =>
      expect(calls).toContainEqual({ cmd: "cluster_connect", args: { clusterId: "c1" } }),
    );

    await status("c1", { state: "connecting", step: "discovery" });
    expect(await within(row).findByText("Descubriendo recursos…")).toBeInTheDocument();

    await status("c1", { state: "connected", serverVersion: "v1.37.0", degraded: false });
    expect(await within(row).findByText("Conectado · v1.37.0")).toBeInTheDocument();
    expect(within(row).getByRole("button", { name: "Desconectar" })).toBeInTheDocument();
    // Aparece en la barra lateral y en la barra de estado.
    expect(screen.getByRole("navigation").querySelector('a[href="/clusters/c1"]')).not.toBeNull();
    expect(screen.getByText("1 clúster conectado")).toBeInTheDocument();
  });

  it("muestra el error con su pista y permite reintentar", async () => {
    const error = {
      code: "Unauthorized" as const,
      message: "No se encontró el exec plugin «aws»",
      details: { status: null, hint: "Añade su carpeta en Ajustes" },
    };
    const { calls } = mockBackend({
      sources: [source],
      clusters: [cluster({ status: { state: "error", error } })],
    });
    renderApp();

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("No se encontró el exec plugin «aws»");
    expect(alert).toHaveTextContent("Añade su carpeta en Ajustes");
    fireEvent.click(screen.getByRole("button", { name: "Reintentar" }));
    await waitFor(() => expect(calls.some((c) => c.cmd === "cluster_connect")).toBe(true));
  });

  it("guarda alias y color desde las preferencias", async () => {
    const { calls } = mockBackend({ sources: [source], clusters: [cluster()] });
    renderApp();

    fireEvent.click(await screen.findByRole("button", { name: "Preferencias" }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.change(within(dialog).getByLabelText("Alias"), { target: { value: "pruebas" } });
    fireEvent.click(within(dialog).getByRole("radio", { name: "#1f8a4c" }));
    fireEvent.click(within(dialog).getByRole("switch", { name: "Protegido" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Guardar" }));

    await waitFor(() =>
      expect(calls).toContainEqual({
        cmd: "cluster_set_prefs",
        args: {
          clusterId: "c1",
          prefs: { alias: "pruebas", color: "#1f8a4c", readOnly: false, protected: true },
        },
      }),
    );
    expect(await screen.findByRole("link", { name: "pruebas" })).toBeInTheDocument();
  });

  it("la página del clúster muestra namespaces y tipos por categoría", async () => {
    mockBackend({
      sources: [source],
      clusters: [
        cluster({ status: { state: "connected", serverVersion: "v1.37.0", degraded: false } }),
      ],
      namespaces: ["default", "kube-system"],
      kinds: [
        {
          group: "",
          version: "v1",
          kind: "Pod",
          plural: "pods",
          namespaced: true,
          verbs: ["list"],
          category: "workloads",
        },
        {
          group: "g1.oculus.test",
          version: "v1",
          kind: "Widget1",
          plural: "widgets1",
          namespaced: true,
          verbs: ["list"],
          category: "custom",
        },
      ],
    });
    renderApp("/clusters/c1");

    expect(await screen.findByText("kube-system")).toBeInTheDocument();
    expect(screen.getByText("Workloads")).toBeInTheDocument();
    expect(screen.getByText("Recursos personalizados (CRDs)")).toBeInTheDocument();
    expect(screen.getByText("Widget1")).toBeInTheDocument();
  });

  it("al cambiar un kubeconfig vuelve a pedir la lista", async () => {
    const { state, calls } = mockBackend({ sources: [source], clusters: [cluster()] });
    renderApp();
    await screen.findByRole("link", { name: "kind-oculus-it" });

    state.clusters = [cluster(), cluster({ id: "c2", contextName: "nuevo" })];
    await act(() => emit("kubeconfig-changed", null));
    expect(await screen.findByRole("link", { name: "nuevo" })).toBeInTheDocument();
    expect(calls.filter((c) => c.cmd === "clusters_list").length).toBeGreaterThanOrEqual(2);
  });
});
