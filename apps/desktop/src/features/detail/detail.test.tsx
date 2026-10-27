import type {
  ClusterSummary,
  ColumnDef,
  ResourceDetail,
  ResourceKind,
  Row,
} from "@oculus/bindings";
import { initI18n } from "@oculus/i18n";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { mockBackend, renderApp, teardown } from "../../test/backend";
import type { YamlEditorProps } from "./editor/YamlEditor";

// Monaco no funciona en jsdom: un textarea que muestra los marcadores en su lugar.
vi.mock("./editor/YamlEditor", () => ({
  YamlEditor: ({ value, onChange, markers, readOnly }: YamlEditorProps) => (
    <div>
      <textarea
        aria-label="yaml"
        value={value}
        readOnly={readOnly}
        onChange={(e) => onChange(e.target.value)}
      />
      <ul aria-label="marcadores">
        {markers.map((m) => (
          <li key={`${m.line}-${m.message}`}>{`${m.line}: ${m.message}`}</li>
        ))}
      </ul>
    </div>
  ),
  YamlDiff: ({ original, modified }: { original: string; modified: string }) => (
    <pre data-testid="diff">{`${original}\n---\n${modified}`}</pre>
  ),
}));

const cluster: ClusterSummary = {
  id: "c1",
  contextName: "kind",
  hasProxyUrl: false,
  sourceId: "s1",
  server: null,
  defaultNamespace: null,
  prefs: { alias: null, color: null, readOnly: false, protected: false, sshTunnel: null },
  status: { state: "connected", serverVersion: "v1.37.0", degraded: false, tunnel: null },
};
const kind = (k: string, plural: string, group = ""): ResourceKind => ({
  group,
  version: "v1",
  kind: k,
  plural,
  namespaced: true,
  verbs: ["get", "list", "watch", "update"],
  category: "workloads",
});
const columns: ColumnDef[] = [
  { id: "name", label: "columns.name", translate: true, type: "string", defaultVisible: true },
];
const row = (name: string): Row => ({
  uid: `u-${name}`,
  name,
  namespace: "demo",
  createdAt: null,
  status: null,
  cells: {},
});

const deployment = {
  apiVersion: "apps/v1",
  kind: "Deployment",
  metadata: { name: "web", namespace: "demo", resourceVersion: "7", labels: { app: "web" } },
  spec: { replicas: 3 },
};
const detail = (over: Partial<ResourceDetail> = {}): ResourceDetail => ({
  apiVersion: "apps/v1",
  kind: "Deployment",
  uid: "u-web",
  resourceVersion: "7",
  objectJson: JSON.stringify(deployment),
  events: [
    {
      type: "Normal",
      reason: "ScalingReplicaSet",
      message: "Scaled up to 3",
      count: 1,
      lastSeen: null,
      source: "deployment-controller",
    },
  ],
  related: [
    {
      relation: "selects",
      kind: "Pod",
      name: "web-1",
      target: { group: "", version: "v1", plural: "pods", namespace: "demo", name: "web-1" },
    },
  ],
  editable: true,
  secretKeys: null,
  ...over,
});

async function openWeb(backend: ReturnType<typeof mockBackend>) {
  renderApp("/clusters/c1/explore?kind=apps/v1/deployments");
  await waitFor(() =>
    expect(backend.calls.some((c) => c.cmd === "resources_subscribe")).toBe(true),
  );
  await act(async () => backend.push({ type: "snapshot", columns, rows: [row("web")] }));
  fireEvent.click(await screen.findByRole("button", { name: "web" }));
  const panel = await screen.findByRole("complementary", { name: "web" });
  // El detalle llega de forma asíncrona: se espera a que haya pestañas.
  await within(panel).findByRole("tab", { name: "Resumen" });
  return panel;
}

const setup = (over: Partial<Parameters<typeof mockBackend>[0]> = {}) =>
  mockBackend({
    sources: [{ id: "s1", path: "/k", origin: "default", error: null }],
    clusters: [cluster],
    kinds: [
      kind("Deployment", "deployments", "apps"),
      kind("Pod", "pods"),
      kind("Secret", "secrets"),
    ],
    details: { web: detail() },
    ...over,
  });

/** Las pestañas de Radix se activan con mousedown, no con click. */
function selectTab(panel: HTMLElement, name: string) {
  const tab = within(panel).getByRole("tab", { name });
  fireEvent.mouseDown(tab, { button: 0 });
  fireEvent.click(tab);
}

async function editYaml(panel: HTMLElement, from: string, to: string) {
  selectTab(panel, "YAML");
  const area = (await within(panel).findByLabelText("yaml")) as HTMLTextAreaElement;
  fireEvent.change(area, { target: { value: area.value.replace(from, to) } });
  return area;
}

beforeAll(async () => {
  await initI18n("es");
});
afterEach(teardown);

describe("detalle", () => {
  it("abre el resumen, los eventos y los relacionados", async () => {
    const backend = setup();
    const panel = await openWeb(backend);
    expect(backend.calls).toContainEqual({
      cmd: "resource_get",
      args: {
        clusterId: "c1",
        target: {
          group: "apps",
          version: "v1",
          plural: "deployments",
          namespace: "demo",
          name: "web",
        },
      },
    });
    expect(within(panel).getByText("app=web")).toBeInTheDocument();

    selectTab(panel, "Eventos (1)");
    expect(await within(panel).findByText("Scaled up to 3")).toBeInTheDocument();

    selectTab(panel, "Relacionados (1)");
    fireEvent.click(await within(panel).findByRole("button", { name: "web-1" }));
    await waitFor(() =>
      expect(
        backend.calls.at(-1)?.cmd === "resources_subscribe" ||
          backend.calls.some((c) => c.cmd === "resources_unsubscribe"),
      ).toBe(true),
    );
  });

  it("edita, valida y aplica con la versión leída", async () => {
    const backend = setup();
    const panel = await openWeb(backend);
    await editYaml(panel, "replicas: 3", "replicas: 5");

    fireEvent.click(within(panel).getByRole("button", { name: "Validar" }));
    expect(
      await within(panel).findByText("Validado por el clúster (sin aplicar)"),
    ).toBeInTheDocument();
    fireEvent.click(within(panel).getByRole("button", { name: "Aplicar" }));
    expect(await within(panel).findByText("Aplicado")).toBeInTheDocument();

    const updates = backend.calls
      .filter((c) => c.cmd === "resource_update")
      .map((c) => c.args as Record<string, unknown>);
    expect(updates.map((a) => [a.dryRun, a.force])).toEqual([
      [true, false],
      [false, false],
    ]);
    // Se envían la base (tal como se cargó) y lo editado; el backend calcula el diff.
    expect(JSON.parse(updates[1]?.baseJson as string).spec.replicas).toBe(3);
    expect(JSON.parse(updates[1]?.editedJson as string).spec.replicas).toBe(5);
  });

  it("un campo rechazado se marca en su línea", async () => {
    const backend = setup({
      update: () => {
        throw {
          code: "Invalid",
          message: "strict decoding error",
          details: {
            status: 400,
            hint: null,
            causes: [{ field: "spec.replicass", message: "campo desconocido: spec.replicass" }],
          },
        };
      },
    });
    const panel = await openWeb(backend);
    const area = await editYaml(panel, "replicas: 3", "replicass: 3");
    fireEvent.click(within(panel).getByRole("button", { name: "Validar" }));

    const markers = await within(panel).findByRole("list", { name: "marcadores" });
    await waitFor(() => expect(markers.textContent).toContain("campo desconocido"));
    const line = area.value.split("\n").findIndex((l) => l.includes("replicass")) + 1;
    expect(markers.textContent).toContain(`${line}: campo desconocido`);
  });

  it("un conflicto deja ver diferencias y sobrescribir solo lo mío", async () => {
    let calls = 0;
    const backend = setup({
      update: ({ editedJson, force }) => {
        calls++;
        if (!force) {
          throw {
            code: "Conflict",
            message: "Alguien cambió en el clúster los mismos campos que has editado",
            details: { status: null, hint: null, causes: [] },
          };
        }
        return { resourceVersion: "9", objectJson: editedJson };
      },
    });
    const panel = await openWeb(backend);
    await editYaml(panel, "replicas: 3", "replicas: 5");
    // Mientras tanto, el objeto cambió en el clúster.
    backend.state.details.web = detail({
      resourceVersion: "8",
      objectJson: JSON.stringify({
        ...deployment,
        metadata: { ...deployment.metadata, resourceVersion: "8" },
        spec: { replicas: 4 },
      }),
    });
    fireEvent.click(within(panel).getByRole("button", { name: "Aplicar" }));

    expect(
      await within(panel).findByText("El recurso cambió en el clúster mientras lo editabas."),
    ).toBeInTheDocument();
    fireEvent.click(within(panel).getByRole("button", { name: "Ver diferencias" }));
    expect((await within(panel).findByTestId("diff")).textContent).toMatch(
      /replicas: 4[\s\S]*---[\s\S]*replicas: 5/,
    );

    fireEvent.click(within(panel).getByRole("button", { name: "Sobrescribir" }));
    expect(await within(panel).findByText("Aplicado")).toBeInTheDocument();
    expect(calls).toBe(2);
    // Sobrescribir reenvía mi diff con la misma base y force: lo ajeno no se deshace.
    const last = backend.calls.filter((c) => c.cmd === "resource_update").at(-1)?.args as Record<
      string,
      unknown
    >;
    expect(last.force).toBe(true);
    expect(JSON.parse(last.baseJson as string).spec.replicas).toBe(3);
  });

  it("un Secret oculta sus valores hasta pedirlos y no es editable", async () => {
    const backend = setup({
      details: {
        web: detail({
          kind: "Secret",
          apiVersion: "v1",
          editable: false,
          secretKeys: ["token"],
          events: [],
          related: [],
        }),
      },
      secrets: { token: "no-es-real" },
    });
    const panel = await openWeb(backend);
    expect(within(panel).queryByText("no-es-real")).not.toBeInTheDocument();
    fireEvent.click(within(panel).getByRole("button", { name: "Mostrar el valor de token" }));
    expect(await within(panel).findByText("no-es-real")).toBeInTheDocument();
    expect(backend.calls).toContainEqual(expect.objectContaining({ cmd: "secret_reveal" }));

    selectTab(panel, "YAML");
    expect(await within(panel).findByText("Solo lectura")).toBeInTheDocument();
    expect(within(panel).queryByRole("button", { name: "Aplicar" })).not.toBeInTheDocument();
  });
});
