// Todo Monaco vive detrás de este módulo, que solo se importa con import():
// pesa varios MB y no debe tocar el arranque (D6).
//
// Monaco se queda en 0.52.x: desde 0.53 `createWebWorker` recibe el worker ya
// creado y monaco-yaml (vía monaco-worker-manager 2.0.1, sin cambios desde 2022)
// sigue usando la API anterior, así que su validador no arrancaría.
import "monaco-editor/esm/vs/basic-languages/yaml/yaml.contribution.js";
import * as monaco from "monaco-editor/esm/vs/editor/editor.api.js";
// "&inline": el worker va dentro del bundle y se crea desde un blob:. WebKit no
// deja crear workers desde el esquema propio de Tauri (tauri://).
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker.js?worker&inline";
import { configureMonacoYaml, type MonacoYaml } from "monaco-yaml";
import YamlWorker from "./yaml.worker.ts?worker&inline";

self.MonacoEnvironment = {
  getWorker(_moduleId: string, label: string) {
    return label === "yaml" ? new YamlWorker() : new EditorWorker();
  },
};

let yaml: MonacoYaml | undefined;

/** Un esquema por modelo: el del tipo de recurso que se está editando. */
export function setSchema(modelUri: string, schema: object | null) {
  // El id no puede llevar escapes (%3A…): el servicio de YAML normaliza la URI al
  // buscar el contenido y no lo encontraría ("Unable to load schema … No content").
  const id = `oculus://schema/${modelUri.replace(/^[a-z]+:\/\//, "")}`;
  const schemas = schema ? [{ uri: id, fileMatch: [modelUri], schema }] : [];
  if (yaml) {
    yaml.update({ enableSchemaRequest: false, validate: true, schemas });
  } else {
    yaml = configureMonacoYaml(monaco, { enableSchemaRequest: false, validate: true, schemas });
  }
}

export { monaco };
