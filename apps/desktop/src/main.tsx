import { commands } from "@oculus/bindings";
import { initI18n } from "@oculus/i18n";
import { QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { queryClient } from "./lib/queryClient";
import { reportErrors } from "./lib/reportErrors";
import { applyTheme } from "./lib/theme";
import { router } from "./router";
import "./styles.css";

// En un navegador sin Tauri (pnpm dev:web, o la build de medida con --mode perf)
// se usa un backend simulado. En la build normal esta rama desaparece.
if (
  (import.meta.env.DEV || import.meta.env.MODE === "perf") &&
  !("__TAURI_INTERNALS__" in window)
) {
  (await import("./dev/browserMock")).installBrowserMock();
}

// Ajustes antes de pintar: así el primer frame ya sale con el tema y el idioma
// correctos, sin destellos ni textos que cambian.
if ("__TAURI_INTERNALS__" in window) reportErrors();

// Una recarga del webview no avisa al backend: se cierran las suscripciones de la carga anterior.
await commands.resourcesReset();
const settings = await commands.settingsGet();
queryClient.setQueryData(["settings"], settings);
applyTheme(settings.theme);
await initI18n(settings.locale);

const root = document.getElementById("root");
if (!root) throw new Error("falta #root en index.html");

createRoot(root).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  </StrictMode>,
);
