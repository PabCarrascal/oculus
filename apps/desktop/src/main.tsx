import { commands } from "@oculus/bindings";
import { initI18n } from "@oculus/i18n";
import { QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { queryClient } from "./lib/queryClient";
import { applyTheme } from "./lib/theme";
import { router } from "./router";
import "./styles.css";

// Ajustes antes de pintar: así el primer frame ya sale con el tema y el idioma
// correctos, sin destellos ni textos que cambian.
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
