import { describe, expect, it } from "vitest";
import { formatAge } from "./time";

describe("formatAge", () => {
  const now = Date.parse("2026-10-26T12:00:00Z");
  it("usa la unidad mayor, como kubectl", () => {
    expect(formatAge("2026-10-26T11:59:15Z", now)).toBe("45s");
    expect(formatAge("2026-10-26T11:48:00Z", now)).toBe("12m");
    expect(formatAge("2026-10-26T07:00:00Z", now)).toBe("5h");
    expect(formatAge("2026-10-23T12:00:00Z", now)).toBe("3d");
    expect(formatAge("2024-10-01T12:00:00Z", now)).toBe("2y");
  });
  it("sin fecha o con fecha futura no rompe", () => {
    expect(formatAge(null, now)).toBe("");
    expect(formatAge("no-es-fecha", now)).toBe("");
    expect(formatAge("2026-10-26T12:00:30Z", now)).toBe("0s");
  });
});
