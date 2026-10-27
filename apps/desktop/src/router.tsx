import { createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import { AppShell } from "./features/shell/AppShell";
import { ClusterPage } from "./routes/ClusterPage";
import { ExplorerPage, type ExplorerSearch } from "./routes/ExplorerPage";
import { HomePage } from "./routes/HomePage";
import { SettingsPage } from "./routes/SettingsPage";

const rootRoute = createRootRoute({ component: AppShell });

const homeRoute = createRoute({ getParentRoute: () => rootRoute, path: "/", component: HomePage });
const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  component: SettingsPage,
});

const clusterRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/clusters/$clusterId",
  component: ClusterPage,
});

const explorerRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/clusters/$clusterId/explore",
  component: ExplorerPage,
  validateSearch: (search: Record<string, unknown>): ExplorerSearch => ({
    kind: typeof search.kind === "string" ? search.kind : undefined,
    ns: typeof search.ns === "string" ? search.ns : undefined,
    q: typeof search.q === "string" ? search.q : undefined,
    sel: typeof search.sel === "string" ? search.sel : undefined,
  }),
});

export const routeTree = rootRoute.addChildren([
  homeRoute,
  settingsRoute,
  clusterRoute,
  explorerRoute,
]);

export const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
