import { commands, type ObjectRef, type ResourceDetail } from "@oculus/bindings";
import { Button, StatusDot, type Tone } from "@oculus/ui";
import { Eye, EyeOff } from "lucide-react";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { unwrap } from "../../lib/ipc";
import { formatAge, useNow } from "../../lib/time";

type Json = Record<string, unknown>;
const obj = (v: unknown): Json =>
  v && typeof v === "object" && !Array.isArray(v) ? (v as Json) : {};
const arr = (v: unknown): Json[] => (Array.isArray(v) ? (v as Json[]) : []);
const str = (v: unknown) => (typeof v === "string" || typeof v === "number" ? String(v) : "");

interface Props {
  clusterId: string;
  target: ObjectRef;
  detail: ResourceDetail;
}

export function Summary({ clusterId, target, detail }: Props) {
  const { t } = useTranslation();
  const now = useNow();
  const o = JSON.parse(detail.objectJson) as Json;
  const meta = obj(o.metadata);
  const labels = Object.entries(obj(meta.labels));
  const annotations = Object.keys(obj(meta.annotations));
  const conditions = arr(obj(o.status).conditions);
  const containers = detail.kind === "Pod" ? podContainers(o) : [];

  return (
    <div className="space-y-5 p-4 text-sm">
      <dl className="grid grid-cols-[max-content_1fr] gap-x-6 gap-y-1.5">
        <Item label={t("detail.kind")}>{`${detail.kind} · ${detail.apiVersion}`}</Item>
        {target.namespace && <Item label={t("columns.namespace")}>{target.namespace}</Item>}
        <Item label={t("columns.age")}>
          <span title={str(meta.creationTimestamp)}>
            {formatAge(str(meta.creationTimestamp), now)}
          </span>
        </Item>
        <Item label="UID">
          <span className="font-mono text-xs">{detail.uid}</span>
        </Item>
        {annotations.length > 0 && (
          <Item label={t("detail.annotations")}>{annotations.length}</Item>
        )}
      </dl>

      {labels.length > 0 && (
        <Section title={t("columns.labels")}>
          <ul className="flex flex-wrap gap-1.5">
            {labels.map(([k, v]) => (
              <li
                key={k}
                className="rounded border border-border bg-surface px-1.5 py-0.5 font-mono text-xs"
              >
                {k}={str(v)}
              </li>
            ))}
          </ul>
        </Section>
      )}

      {detail.secretKeys && (
        <Section title={t("detail.secretKeys")}>
          <p className="mb-2 text-xs text-muted">{t("detail.secretHint")}</p>
          <ul className="space-y-1">
            {detail.secretKeys.map((k) => (
              <SecretKey key={k} clusterId={clusterId} target={target} name={k} />
            ))}
          </ul>
        </Section>
      )}

      {containers.length > 0 && (
        <Section title={t("detail.containers")}>
          <ul className="space-y-2">
            {containers.map((c) => (
              <li key={c.name} className="rounded-md border border-border bg-surface px-3 py-2">
                <div className="flex items-center gap-2">
                  <StatusDot tone={c.tone} />
                  <span className="font-medium">{c.name}</span>
                  <span className="text-xs text-muted">{c.state}</span>
                  <span className="ml-auto text-xs text-muted">
                    {t("detail.restarts", { count: c.restarts })}
                  </span>
                </div>
                <p className="mt-0.5 truncate font-mono text-xs text-muted" title={c.image}>
                  {c.image}
                </p>
              </li>
            ))}
          </ul>
        </Section>
      )}

      {conditions.length > 0 && (
        <Section title={t("detail.conditions")}>
          <ul className="space-y-1">
            {conditions.map((c) => (
              <li key={str(c.type)} className="flex items-start gap-2">
                <StatusDot
                  tone={
                    str(c.status) === "True"
                      ? "ok"
                      : str(c.status) === "False"
                        ? "warning"
                        : "neutral"
                  }
                  className="mt-1.5"
                />
                <div className="min-w-0">
                  <span className="font-medium">{str(c.type)}</span>
                  {str(c.reason) && (
                    <span className="ml-2 text-xs text-muted">{str(c.reason)}</span>
                  )}
                  {str(c.message) && <p className="text-xs text-muted">{str(c.message)}</p>}
                </div>
              </li>
            ))}
          </ul>
        </Section>
      )}
    </div>
  );
}

function Item({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <dt className="text-muted">{label}</dt>
      <dd className="min-w-0 truncate">{children}</dd>
    </>
  );
}

export function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section>
      <h3 className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted">{title}</h3>
      {children}
    </section>
  );
}

function SecretKey({
  clusterId,
  target,
  name,
}: {
  clusterId: string;
  target: ObjectRef;
  name: string;
}) {
  const { t } = useTranslation();
  const [value, setValue] = useState<string | null>(null);
  const [error, setError] = useState(false);
  return (
    <li className="flex items-start gap-2">
      <span className="w-40 shrink-0 truncate pt-1.5 font-mono text-xs">{name}</span>
      <div className="min-w-0 flex-1 pt-1.5 font-mono text-xs break-all">
        {value === null ? <span className="text-muted">••••••••</span> : value}
        {error && <span className="text-error">{t("detail.revealError")}</span>}
      </div>
      <Button
        variant="ghost"
        className="h-7 px-2"
        aria-label={
          value === null ? t("detail.reveal", { key: name }) : t("detail.hide", { key: name })
        }
        onClick={async () => {
          if (value !== null) return setValue(null);
          try {
            setValue(await unwrap(commands.secretReveal(clusterId, target, name)));
            setError(false);
          } catch {
            setError(true);
          }
        }}
      >
        {value === null ? <Eye size={14} aria-hidden /> : <EyeOff size={14} aria-hidden />}
      </Button>
    </li>
  );
}

interface ContainerInfo {
  name: string;
  image: string;
  state: string;
  restarts: number;
  tone: Tone;
}

function podContainers(o: Json): ContainerInfo[] {
  const statuses = new Map(arr(obj(o.status).containerStatuses).map((s) => [str(s.name), s]));
  return arr(obj(o.spec).containers).map((c) => {
    const s = statuses.get(str(c.name)) ?? {};
    const state = obj(s.state);
    const [phase, details] = Object.entries(state)[0] ?? ["", {}];
    const reason = str(obj(details).reason);
    const ready = s.ready === true;
    const tone: Tone = ready
      ? "ok"
      : phase === "waiting" && reason.includes("BackOff")
        ? "error"
        : phase === "terminated"
          ? "neutral"
          : "progress";
    return {
      name: str(c.name),
      image: str(c.image),
      state: reason || phase,
      restarts: Number(s.restartCount ?? 0),
      tone,
    };
  });
}
