import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// APIs que usan cmdk y Radix y que jsdom no implementa.
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
};
Element.prototype.scrollIntoView ??= () => {};

// jsdom no maqueta: todo mide 0 px y la tabla virtual (que mide con
// clientWidth/clientHeight) no pintaría filas. Se simula un contenedor de 1200×800.
Object.defineProperty(HTMLElement.prototype, "clientHeight", {
  configurable: true,
  get: () => 800,
});
Object.defineProperty(HTMLElement.prototype, "clientWidth", {
  configurable: true,
  get: () => 1200,
});

afterEach(() => cleanup());
