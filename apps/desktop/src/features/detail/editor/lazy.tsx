import { lazy } from "react";

// Monaco solo se descarga al abrir la pestaña YAML (D6).
export const LazyYamlEditor = lazy(() =>
  import("./YamlEditor").then((m) => ({ default: m.YamlEditor })),
);
export const LazyYamlDiff = lazy(() =>
  import("./YamlEditor").then((m) => ({ default: m.YamlDiff })),
);
