import { commands, type ObjectRef, type OculusError, type ResourceDetail } from "@oculus/bindings";
import { Button } from "@oculus/ui";
import { Suspense, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useDarkMode } from "../../lib/darkMode";
import { isOculusError, unwrap } from "../../lib/ipc";
import { modKey } from "../../lib/platform";
import { useSchema } from "../../stores/detail";
import { ErrorNotice } from "../clusters/ErrorNotice";
import { ErrorBoundary } from "./ErrorBoundary";
import { LazyYamlDiff, LazyYamlEditor } from "./editor/lazy";
import { locate, type Marker, parseYaml, toYaml } from "./yamlPaths";

interface Props {
  clusterId: string;
  target: ObjectRef;
  detail: ResourceDetail;
  /** El objeto cambió en el clúster desde que se cargó (aviso de la tabla en vivo). */
  changedRemotely: boolean;
  onDirtyChange: (dirty: boolean) => void;
  onApplied: () => void;
}

type Phase = "idle" | "validating" | "applying" | "validated" | "applied";

/** Una versión del objeto tal como la dio el clúster: base de los diffs. */
interface Version {
  text: string;
  json: string;
  resourceVersion: string;
}

const versionOf = (objectJson: string, resourceVersion: string): Version => ({
  text: toYaml(objectJson),
  json: objectJson,
  resourceVersion,
});

export function YamlTab({
  clusterId,
  target,
  detail,
  changedRemotely,
  onDirtyChange,
  onApplied,
}: Props) {
  const { t } = useTranslation();
  const dark = useDarkMode();
  const [base, setBase] = useState(() => versionOf(detail.objectJson, detail.resourceVersion));
  const [text, setText] = useState(base.text);
  const [markers, setMarkers] = useState<Marker[]>([]);
  const [phase, setPhase] = useState<Phase>("idle");
  const [error, setError] = useState<OculusError | null>(null);
  const [latest, setLatest] = useState<Version | null>(null);
  const [showDiff, setShowDiff] = useState(false);
  const schema = useSchema(clusterId, target.group, target.version, detail.kind);
  const dirty = text !== base.text;
  const busy = phase === "validating" || phase === "applying";
  const uri = useMemo(
    () =>
      `oculus://${clusterId}/${target.group || "core"}/${detail.kind}/${target.namespace ?? "_"}/${target.name}.yaml`,
    [clusterId, target, detail.kind],
  );

  useEffect(() => onDirtyChange(dirty), [dirty, onDirtyChange]);

  // Sin cambios locales, el editor sigue al objeto del clúster.
  // biome-ignore lint/correctness/useExhaustiveDependencies: solo cuando llega otra versión del objeto.
  useEffect(() => {
    if (dirty || detail.resourceVersion === base.resourceVersion) return;
    const next = versionOf(detail.objectJson, detail.resourceVersion);
    setBase(next);
    setText(next.text);
  }, [detail.resourceVersion]);

  const onChange = (value: string) => {
    setText(value);
    setMarkers(parseYaml(value).markers);
    if (phase === "validated" || phase === "applied") setPhase("idle");
  };

  const fetchLatest = async () => {
    const fresh = await unwrap(commands.resourceGet(clusterId, target));
    const l = versionOf(fresh.objectJson, fresh.resourceVersion);
    setLatest(l);
    return l;
  };

  /** Envía solo lo que cambió respecto a `base`; `force` ignora cambios ajenos en esos campos. */
  const submit = async (dryRun: boolean, force = false) => {
    const parsed = parseYaml(text);
    setMarkers(parsed.markers);
    if (!parsed.json) return;
    setError(null);
    setPhase(dryRun ? "validating" : "applying");
    try {
      const result = await unwrap(
        commands.resourceUpdate(clusterId, target, base.json, parsed.json, dryRun, force),
      );
      setMarkers([]);
      if (dryRun) {
        setPhase("validated");
      } else {
        const next = versionOf(result.objectJson, result.resourceVersion);
        setBase(next);
        setText(next.text);
        setLatest(null);
        setShowDiff(false);
        setPhase("applied");
        onApplied();
      }
    } catch (e) {
      setPhase("idle");
      const err = isOculusError(e)
        ? e
        : ({ code: "Internal", message: String(e), details: null } as OculusError);
      setError(err);
      if (err.code === "Conflict") {
        await fetchLatest().catch(() => undefined);
      }
      const causes = err.details?.causes ?? [];
      if (causes.length > 0) {
        setMarkers(causes.map((c) => locate(text, c.field ?? "", c.message)));
      }
    }
  };

  const reload = (l: Version) => {
    setBase(l);
    setText(l.text);
    setLatest(null);
    setShowDiff(false);
    setError(null);
    setMarkers([]);
  };

  const compare = async () => {
    if (!latest) await fetchLatest().catch((e) => setError(e as OculusError));
    setShowDiff((v) => !v);
  };

  const conflict = error?.code === "Conflict";

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex flex-wrap items-center gap-2 border-b border-border px-3 py-2">
        {!detail.editable && <span className="text-xs text-muted">{t("detail.readOnlyYaml")}</span>}
        {detail.editable && (
          <span className="text-xs text-muted">
            {phase === "validated" && t("detail.validated")}
            {phase === "applied" && t("detail.applied")}
            {dirty && phase === "idle" && t("detail.unsaved")}
          </span>
        )}
        <div className="flex-1" />
        {detail.editable && (
          <>
            <Button variant="ghost" disabled={!dirty || busy} onClick={() => reload(base)}>
              {t("detail.discard")}
            </Button>
            <Button
              disabled={!dirty || busy || markers.length > 0}
              onClick={() => void submit(true)}
            >
              {phase === "validating" ? t("detail.validating") : t("detail.validate")}
            </Button>
            <Button
              variant="primary"
              disabled={!dirty || busy || markers.length > 0}
              onClick={() => void submit(false)}
              title={`${modKey}S`}
            >
              {phase === "applying" ? t("detail.applying") : t("detail.apply")}
            </Button>
          </>
        )}
      </div>

      {(conflict || (changedRemotely && dirty)) && (
        <div
          role="status"
          className="flex flex-wrap items-center gap-2 border-b border-warning/40 bg-warning/10 px-3 py-2 text-sm"
        >
          <span className="flex-1">{t("detail.changedRemotely")}</span>
          <Button variant="ghost" onClick={() => void compare()}>
            {showDiff ? t("detail.hideDiff") : t("detail.showDiff")}
          </Button>
          {latest && (
            <>
              <Button variant="ghost" onClick={() => reload(latest)}>
                {t("detail.reload")}
              </Button>
              {conflict && (
                <Button onClick={() => void submit(false, true)}>{t("detail.overwrite")}</Button>
              )}
            </>
          )}
        </div>
      )}
      {error && !conflict && (
        <div className="px-3 pt-2">
          <ErrorNotice error={error} />
        </div>
      )}

      <div className="min-h-0 flex-1">
        <ErrorBoundary retryLabel={t("clusters.retry")}>
          <Suspense
            fallback={<p className="p-3 text-sm text-muted">{t("detail.loadingEditor")}</p>}
          >
            {showDiff && latest ? (
              <LazyYamlDiff original={latest.text} modified={text} dark={dark} />
            ) : (
              <LazyYamlEditor
                uri={uri}
                value={text}
                onChange={onChange}
                onSave={() => {
                  if (detail.editable && dirty && !busy) void submit(false);
                }}
                markers={markers}
                readOnly={!detail.editable || busy}
                schema={schema.data ?? null}
                dark={dark}
              />
            )}
          </Suspense>
        </ErrorBoundary>
      </div>
    </div>
  );
}
