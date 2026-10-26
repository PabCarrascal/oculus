import type { Row } from "@oculus/bindings";
import { describe, expect, it } from "vitest";
import { ResourceStore } from "./resources";

const row = (uid: string, text = "Running"): Row => ({
  uid,
  name: uid,
  namespace: "n",
  createdAt: null,
  status: { tone: "ok", text },
  cells: {},
});

describe("ResourceStore", () => {
  it("aplica snapshot, deltas y estado sin perder filas", () => {
    const store = new ResourceStore();
    let notified = 0;
    store.subscribe(() => notified++);

    expect(store.getSnapshot().loaded).toBe(false);
    store.apply({ type: "snapshot", columns: [], rows: [row("a"), row("b")] });
    expect(store.getSnapshot().rows.map((r) => r.uid)).toEqual(["a", "b"]);
    expect(store.getSnapshot().loaded).toBe(true);

    store.apply({ type: "delta", upserts: [row("a", "Failed"), row("c")], deletes: ["b"] });
    const rows = store.getSnapshot().rows;
    expect(rows.map((r) => r.uid).sort()).toEqual(["a", "c"]);
    expect(rows.find((r) => r.uid === "a")?.status?.text).toBe("Failed");

    const before = store.getSnapshot().rows;
    store.apply({ type: "status", state: "relisting", error: null });
    expect(store.getSnapshot().state).toBe("relisting");
    expect(store.getSnapshot().rows).toBe(before);
    expect(notified).toBe(3);
  });

  it("la instantánea es estable entre cambios (useSyncExternalStore)", () => {
    const store = new ResourceStore();
    expect(store.getSnapshot()).toBe(store.getSnapshot());
  });
});
