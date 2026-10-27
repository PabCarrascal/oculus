// Se carga con React.lazy: importar este fichero arrastra Monaco entero.
import { useEffect, useRef } from "react";
import type { Marker } from "../yamlPaths";
import { monaco, setSchema } from "./monaco";

const OPTIONS = {
  automaticLayout: true,
  minimap: { enabled: false },
  fontSize: 12,
  tabSize: 2,
  scrollBeyondLastLine: false,
  renderWhitespace: "boundary",
  wordWrap: "off",
} as const;

export interface YamlEditorProps {
  uri: string;
  value: string;
  onChange: (value: string) => void;
  onSave: () => void;
  markers: Marker[];
  readOnly: boolean;
  schema: object | null;
  dark: boolean;
}

export function YamlEditor({
  uri,
  value,
  onChange,
  onSave,
  markers,
  readOnly,
  schema,
  dark,
}: YamlEditorProps) {
  const host = useRef<HTMLDivElement>(null);
  const editor = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  const model = useRef<monaco.editor.ITextModel | null>(null);
  // Los callbacks cambian en cada render; el editor llama siempre al último.
  const callbacks = useRef({ onChange, onSave });
  callbacks.current = { onChange, onSave };

  // biome-ignore lint/correctness/useExhaustiveDependencies: el editor se crea una vez por modelo; el resto se sincroniza abajo.
  useEffect(() => {
    if (!host.current) return;
    const m = monaco.editor.createModel(value, "yaml", monaco.Uri.parse(uri));
    const e = monaco.editor.create(host.current, {
      ...OPTIONS,
      model: m,
      readOnly,
      theme: dark ? "vs-dark" : "vs",
    });
    e.onDidChangeModelContent(() => callbacks.current.onChange(m.getValue()));
    e.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => callbacks.current.onSave());
    editor.current = e;
    model.current = m;
    performance.mark("oculus:yaml-editor-ready");
    return () => {
      e.dispose();
      m.dispose();
    };
  }, [uri]);

  useEffect(() => {
    if (model.current && model.current.getValue() !== value) model.current.setValue(value);
  }, [value]);

  useEffect(() => {
    if (!model.current) return;
    monaco.editor.setModelMarkers(
      model.current,
      "oculus",
      markers.map((m) => ({
        startLineNumber: m.line,
        startColumn: m.column,
        endLineNumber: m.endLine,
        endColumn: Math.max(m.endColumn, m.column + 1),
        message: m.message,
        severity: monaco.MarkerSeverity.Error,
      })),
    );
  }, [markers]);

  useEffect(() => setSchema(uri, schema), [uri, schema]);
  useEffect(() => editor.current?.updateOptions({ readOnly }), [readOnly]);
  useEffect(() => monaco.editor.setTheme(dark ? "vs-dark" : "vs"), [dark]);

  return <div ref={host} className="h-full min-h-0 w-full" data-testid="yaml-editor" />;
}

export interface YamlDiffProps {
  original: string;
  modified: string;
  dark: boolean;
}

/** Diferencias de solo lectura: a la izquierda el clúster, a la derecha lo editado. */
export function YamlDiff({ original, modified, dark }: YamlDiffProps) {
  const host = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!host.current) return;
    const left = monaco.editor.createModel(original, "yaml");
    const right = monaco.editor.createModel(modified, "yaml");
    const diff = monaco.editor.createDiffEditor(host.current, {
      ...OPTIONS,
      readOnly: true,
      originalEditable: false,
      renderSideBySide: true,
      theme: dark ? "vs-dark" : "vs",
    });
    diff.setModel({ original: left, modified: right });
    return () => {
      diff.dispose();
      left.dispose();
      right.dispose();
    };
  }, [original, modified, dark]);
  return <div ref={host} className="h-full min-h-0 w-full" data-testid="yaml-diff" />;
}
