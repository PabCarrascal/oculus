import { createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import { AppShell } from "./features/shell/AppShell";
import { ClusterPage } from "./routes/ClusterPage";
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

export const routeTree = rootRoute.addChildren([homeRoute, settingsRoute, clusterRoute]);

export const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
