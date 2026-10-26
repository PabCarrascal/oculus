import type { Settings } from "@oculus/bindings";
import { initI18n } from "@oculus/i18n";
import { QueryClientProvider } from "@tanstack/react-query";
import { createMemoryHistory, createRouter, RouterProvider } from "@tanstack/react-router";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { createQueryClient } from "./lib/queryClient";
import { routeTree } from "./router";

const baseSettings: Settings = {
  theme: "system",
  locale: "es",
  shell: null,
  extraPath: [],
  telemetry: { crashReports: false },
  watchLingerSeconds: 30,
};

/** Simula el backend: guarda los ajustes y registra cada llamada IPC. */
function mockBackend() {
  let settings = { ...baseSettings };
  const calls: { cmd: string; args: unknown }[] = [];
  mockIPC((cmd, args) => {
    calls.push({ cmd, args });
    switch (cmd) {
      case "settings_get":
        return settings;
      case "settings_set":
        settings = { ...settings, ...(args as { patch: Partial<Settings> }).patch };
        return settings;
      case "app_info":
        return { version: "0.1.0", os: "linux", arch: "x86_64" };
      case "startup_report":
        return { processToFirstPaintMs: 420 };
    }
  });
  return { calls };
}

function renderApp(path = "/") {
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
  return router;
}

const pressCtrl = (key: string) =>
  act(() => void fireEvent.keyDown(window, { key, ctrlKey: true }));

beforeAll(async () => {
  await initI18n("es");
});

afterEach(() => {
  clearMocks();
  delete document.documentElement.dataset.theme;
});

describe("shell de la app", () => {
  it("arranca en la pantalla de clústeres vacía e informa del arranque", async () => {
    const { calls } = mockBackend();
    renderApp();
    expect(await screen.findByText("Aún no hay clústeres")).toBeInTheDocument();
    await waitFor(() => expect(calls.some((c) => c.cmd === "startup_report")).toBe(true));
  });

  it("Ctrl+, abre los ajustes", async () => {
    mockBackend();
    const router = renderApp();
    await screen.findByText("Aún no hay clústeres");
    await pressCtrl(",");
    await waitFor(() => expect(router.state.location.pathname).toBe("/settings"));
    expect(await screen.findByRole("heading", { name: "Ajustes" })).toBeInTheDocument();
  });

  it("la paleta cambia el tema, lo persiste por IPC y lo aplica", async () => {
    const { calls } = mockBackend();
    renderApp();
    await screen.findByText("Aún no hay clústeres");

    await pressCtrl("k");
    fireEvent.click(await screen.findByText("Tema: Oscuro"));

    await waitFor(() => expect(document.documentElement.dataset.theme).toBe("dark"));
    expect(calls).toContainEqual({ cmd: "settings_set", args: { patch: { theme: "dark" } } });
    expect(screen.queryByPlaceholderText("Escribe un comando o busca…")).not.toBeInTheDocument();
  });

  it("muestra el error del backend con su pista si un ajuste se rechaza", async () => {
    mockIPC((cmd) => {
      if (cmd === "settings_set") {
        throw {
          code: "Invalid",
          message: "valor fuera de rango",
          details: { status: null, hint: "usa 0–600" },
        };
      }
      return null;
    });
    renderApp("/settings");
    fireEvent.click(await screen.findByRole("radio", { name: "Claro" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("valor fuera de rango — usa 0–600");
  });
});
