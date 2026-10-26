import { describe, expect, it } from "vitest";
import en from "./en.json";
import es from "./es.json";

function keys(obj: object, prefix = ""): string[] {
  return Object.entries(obj).flatMap(([k, v]) =>
    typeof v === "object" && v !== null ? keys(v, `${prefix}${k}.`) : [`${prefix}${k}`],
  );
}

describe("catálogos", () => {
  it("es y en tienen exactamente las mismas claves", () => {
    expect(keys(en).sort()).toEqual(keys(es).sort());
  });

  it("no hay traducciones vacías", () => {
    for (const catalog of [es, en]) {
      const empty = keys(catalog).filter((k) => {
        const value = k
          .split(".")
          .reduce<unknown>((o, p) => (o as Record<string, unknown>)[p], catalog);
        return typeof value === "string" && value.trim() === "";
      });
      expect(empty).toEqual([]);
    }
  });
});
