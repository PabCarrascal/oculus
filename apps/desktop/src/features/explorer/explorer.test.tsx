import type { ClusterSummary, ColumnDef, ResourceKind, Row } from "@oculus/bindings";
import { initI18n } from "@oculus/i18n";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { mockBackend, renderApp, teardown } from "../../test/backend";

const cluster: ClusterSummary = {
  id: "c1",
  contextName: "kind-oculus-it",
  sourceId: "s1",
  server: null,
  defaultNamespace: null,
  prefs: { alias: null, color: null, readOnly: false, protected: false },
  status: { state: "connected", serverVersion: "v1.37.0", degraded: false },
};

const kind = (k: string, plural: string, group = "", namespaced = true): ResourceKind => ({
  group,
  version: "v1",
  kind: k,
  plural,
  namespaced,
  verbs: ["get", "list", "watch"],
  category: k === "Node" ? "cluster" : "workloads",
});

const columns: ColumnDef[] = [
  { id: "name", label: "columns.name", translate: true, type: "string", defaultVisible: true },
  {
    id: "namespace",
    label: "columns.namespace",
    translate: true,
    type: "string",
    defaultVisible: true,
  },
  { id: "status", label: "columns.status", translate: true, type: "string", defaultVisible: true },
  {
    id: "restarts",
    label: "columns.restarts",
    translate: true,
    type: "number",
    defaultVisible: true,
  },
  { id: "node", label: "columns.node", translate: true, type: "string", defaultVisible: false },
];

const pod = (name: string, text = "Running", restarts = 0): Row => ({
  uid: `uid-${name}`,
  name,
  namespace: "default",
  createdAt: null,
  status: { tone: text === "Running" ? "ok" : "error", text },
  cells: { restarts, node: "n1" },
});

function setup() {
  const backend = mockBackend({
    sources: [{ id: "s1", path: "/k", origin: "default", error: null }],
    clusters: [cluster],
    kinds: [
      kind("Pod", "pods"),
      kind("Node", "nodes", "", false),
      kind("Deployment", "deployments", "apps"),
    ],
    namespaces: ["default", "kube-system"],
  });
  renderApp("/clusters/c1/explore");
  return backend;
}

const subscribeCalls = (calls: { cmd: string; args: unknown }[]) =>
  calls
    .filter((c) => c.cmd === "resources_subscribe")
    .map((c) => {
      const { kind, scope } = c.args as { kind: unknown; scope: unknown };
      return { kind, scope };
    });

beforeAll(async () => {
  await initI18n("es");
  localStorage.clear();
});

afterEach(teardown);

describe("explorador", () => {
  it("se suscribe a pods y pinta snapshot y deltas", async () => {
    const { calls, push } = setup();
    await waitFor(() =>
      expect(subscribeCalls(calls)).toEqual([
        { kind: { group: "", version: "v1", plural: "pods" }, scope: { kind: "all" } },
      ]),
    );
    expect(screen.getByText("Cargando…")).toBeInTheDocument();

    await act(async () => {
      push({ type: "status", state: "initializing", error: null });
      push({
        type: "snapshot",
        columns,
        rows: [pod("web-1"), pod("api-1", "CrashLoopBackOff", 17)],
      });
      push({ type: "status", state: "ready", error: null });
    });
    const table = await screen.findByRole("table");
    expect(within(table).getByText("web-1")).toBeInTheDocument();
    expect(within(table).getByText("CrashLoopBackOff")).toBeInTheDocument();
    // "node" está oculta por defecto.
    expect(within(table).queryByText("n1")).not.toBeInTheDocument();
    expect(screen.getByText(/En vivo · 2 elementos/)).toBeInTheDocument();

    await act(async () => push({ type: "delta", upserts: [pod("db-1")], deletes: ["uid-web-1"] }));
    expect(await within(table).findByText("db-1")).toBeInTheDocument();
    expect(within(table).queryByText("web-1")).not.toBeInTheDocument();
  });

  it("filtra, ordena y muestra columnas ocultas", async () => {
    const { push, calls } = setup();
    await waitFor(() => expect(subscribeCalls(calls)).toHaveLength(1));
    await act(async () =>
      push({
        type: "snapshot",
        columns,
        rows: [pod("pod-10", "Running", 1), pod("pod-2", "Running", 9), pod("db-1")],
      }),
    );
    const table = await screen.findByRole("table");
    const names = () =>
      within(table)
        .getAllByRole("row")
        .slice(1)
        .map((r) => r.querySelectorAll("td")[1]?.textContent);

    expect(names()).toEqual(["db-1", "pod-2", "pod-10"]);
    fireEvent.click(within(table).getByRole("button", { name: "Reinicios" }));
    expect(names()).toEqual(["db-1", "pod-10", "pod-2"]);
    fireEvent.click(within(table).getByRole("button", { name: /Reinicios/ }));
    expect(names()).toEqual(["pod-2", "pod-10", "db-1"]);

    fireEvent.change(screen.getByRole("textbox", { name: /Filtrar…/ }), {
      target: { value: "pod" },
    });
    await waitFor(() => expect(names()).toHaveLength(2));
    expect(screen.getByText("2 / 3")).toBeInTheDocument();
  });

  it("cambiar de tipo da de baja la suscripción anterior", async () => {
    const { calls } = setup();
    await waitFor(() => expect(subscribeCalls(calls)).toHaveLength(1));
    fireEvent.click(await screen.findByRole("button", { name: "Deployment" }));
    await waitFor(() => expect(subscribeCalls(calls)).toHaveLength(2));
    expect(subscribeCalls(calls)[1]?.kind).toEqual({
      group: "apps",
      version: "v1",
      plural: "deployments",
    });
    await waitFor(() =>
      expect(calls).toContainEqual({
        cmd: "resources_unsubscribe",
        args: { clusterId: "c1", subscriptionId: "w1" },
      }),
    );
  });

  it("un error del watch se muestra con su pista", async () => {
    const { push, calls } = setup();
    await waitFor(() => expect(subscribeCalls(calls)).toHaveLength(1));
    await act(async () =>
      push({
        type: "status",
        state: "error",
        error: {
          code: "Forbidden",
          message: "pods is forbidden",
          details: { status: 403, hint: "Elige namespaces concretos en el filtro.", causes: [] },
        },
      }),
    );
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("pods is forbidden");
    expect(alert).toHaveTextContent("Elige namespaces concretos");
  });
});
