import { commands } from "@oculus/bindings";

const describe = (value: unknown): string => {
  if (value instanceof Error) return `${value.name}: ${value.message}\n${value.stack ?? ""}`;
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
};

/**
 * Reenvía al log del backend los errores del webview: en la build de release no
 * hay consola, y sin esto un fallo de la UI no deja rastro.
 */
export function reportErrors() {
  const send = (level: "error" | "warn", message: string) => {
    void commands.uiLog(level, message).catch(() => undefined);
  };
  window.addEventListener("error", (e) =>
    send("error", `${e.message} (${e.filename}:${e.lineno})`),
  );
  // La CSP bloquea en silencio: el navegador no lo escribe en la consola de JS.
  document.addEventListener("securitypolicyviolation", (e) =>
    send(
      "warn",
      `CSP bloqueó ${e.violatedDirective}: ${e.blockedURI || "inline"} (${e.sourceFile}:${e.lineNumber})`,
    ),
  );
  window.addEventListener("unhandledrejection", (e) =>
    send("error", `promesa rechazada: ${describe(e.reason)}`),
  );
  for (const level of ["error", "warn"] as const) {
    const original = console[level].bind(console);
    console[level] = (...args: unknown[]) => {
      original(...args);
      send(level, args.map(describe).join(" "));
    };
  }
}
