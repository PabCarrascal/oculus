import { describe, expect, it } from "vitest";
import { fieldToPath, locate, parseYaml, toYaml } from "./yamlPaths";

const doc = `apiVersion: apps/v1
kind: Deployment
metadata:
  name: web
spec:
  replicass: 3
  template:
    spec:
      containers:
        - name: app
          image: nginx
        - name: side
          image: busybox
`;

describe("YAML del editor", () => {
  it("ida y vuelta JSON → YAML → JSON conserva el objeto", () => {
    const obj = {
      apiVersion: "v1",
      kind: "ConfigMap",
      metadata: { name: "c" },
      data: { "a.txt": "x\ny\n", n: "1" },
    };
    const back = parseYaml(toYaml(JSON.stringify(obj)));
    expect(back.markers).toEqual([]);
    expect(JSON.parse(back.json ?? "null")).toEqual(obj);
  });

  it("un error de sintaxis da su línea y no deja enviar", () => {
    const r = parseYaml("metadata:\n  name: web\n   bad: [\n");
    expect(r.json).toBeNull();
    expect(r.markers[0]?.line).toBeGreaterThanOrEqual(2);
  });

  it("traduce rutas de Kubernetes con índices", () => {
    expect(fieldToPath("spec.template.spec.containers[1].image")).toEqual([
      "spec",
      "template",
      "spec",
      "containers",
      1,
      "image",
    ]);
    expect(fieldToPath("metadata.labels")).toEqual(["metadata", "labels"]);
  });

  it("marca la línea del campo que rechaza el API server", () => {
    expect(locate(doc, "spec.replicass", "campo desconocido").line).toBe(6);
    expect(locate(doc, "spec.template.spec.containers[1].image", "x").line).toBe(13);
    // Un campo que no existe en el documento: el padre más profundo que sí existe.
    expect(locate(doc, "spec.template.spec.containers[0].resources.limits", "x").line).toBe(10);
    expect(locate(doc, "no.existe", "x").line).toBe(1);
  });
});
