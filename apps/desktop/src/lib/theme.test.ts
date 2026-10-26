import { describe, expect, it } from "vitest";
import { applyTheme } from "./theme";

describe("applyTheme", () => {
  it("fija el tema explícito en data-theme", () => {
    const root = document.createElement("html");
    applyTheme("dark", root);
    expect(root.dataset.theme).toBe("dark");
  });

  it("con 'system' quita el atributo para seguir al sistema", () => {
    const root = document.createElement("html");
    root.dataset.theme = "light";
    applyTheme("system", root);
    expect(root.hasAttribute("data-theme")).toBe(false);
  });
});
