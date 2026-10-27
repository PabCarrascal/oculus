import type { ObjectRef, RelatedRef } from "@oculus/bindings";
import { Button, StatusDot } from "@oculus/ui";
import { useQueryClient } from "@tanstack/react-query";
import { X } from "lucide-react";
import { Tabs } from "radix-ui";
import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { formatAge, useNow } from "../../lib/time";
import { detailKey, useDetail } from "../../stores/detail";
import { ErrorNotice } from "../clusters/ErrorNotice";
import { Section, Summary } from "./Summary";

// La pestaña YAML (con la librería yaml y, dentro, Monaco) se descarga al abrirla.
const YamlTab = lazy(() => import("./YamlTab").then((m) => ({ default: m.YamlTab })));

interface Props {
  clusterId: string;
  target: ObjectRef;
  /** Cambia cuando la tabla en vivo recibe una versión nueva de la fila. */
  rowVersion: unknown;
  onClose: () => void;
  onNavigate: (target: ObjectRef) => void;
}

export function DetailPanel({ clusterId, target, rowVersion, onClose, onNavigate }: Props) {
  const { t } = useTranslation();
  const client = useQueryClient();
  const detail = useDetail(clusterId, target);
  const [tab, setTab] = useState("summary");
  const [dirty, setDirty] = useState(false);
  const [changedRemotely, setChangedRemotely] = useState(false);
  const firstRow = useRef(rowVersion);

  // Si la fila cambia en la tabla: sin edición pendiente se recarga; con ella, solo se avisa.
  useEffect(() => {
    if (rowVersion === firstRow.current) return;
    firstRow.current = rowVersion;
    if (dirty) setChangedRemotely(true);
    else void client.invalidateQueries({ queryKey: detailKey(clusterId, target) });
  }, [rowVersion, dirty, client, clusterId, target]);

  const onApplied = useCallback(() => {
    setChangedRemotely(false);
    void client.invalidateQueries({ queryKey: detailKey(clusterId, target) });
  }, [client, clusterId, target]);

  return (
    <aside
      aria-label={target.name}
      className="flex h-full w-[min(560px,55%)] shrink-0 flex-col border-l border-border bg-bg"
    >
      <header className="flex items-start gap-2 border-b border-border px-4 py-3">
        <div className="min-w-0 flex-1">
          <p className="text-xs text-muted">{detail.data?.kind ?? target.plural}</p>
          <h2 className="truncate font-semibold" title={target.name}>
            {target.name}
          </h2>
        </div>
        <Button variant="ghost" className="px-2" aria-label={t("detail.close")} onClick={onClose}>
          <X size={16} aria-hidden />
        </Button>
      </header>

      {detail.isError && (
        <div className="p-4">
          <ErrorNotice error={detail.error} />
        </div>
      )}
      {detail.data && (
        <Tabs.Root value={tab} onValueChange={setTab} className="flex min-h-0 flex-1 flex-col">
          <Tabs.List className="flex gap-1 border-b border-border px-3" aria-label={target.name}>
            {[
              ["summary", t("detail.summary")],
              ["yaml", "YAML"],
              ["events", `${t("detail.events")} (${detail.data.events.length})`],
              ["related", `${t("detail.related")} (${detail.data.related.length})`],
            ].map(([value, label]) => (
              <Tabs.Trigger
                key={value}
                value={value as string}
                className="border-b-2 border-transparent px-2 py-2 text-sm whitespace-nowrap text-muted hover:text-text focus-visible:outline-2 focus-visible:outline-focus data-[state=active]:border-accent data-[state=active]:text-text"
              >
                {label}
              </Tabs.Trigger>
            ))}
          </Tabs.List>
          <Tabs.Content value="summary" className="min-h-0 flex-1 overflow-y-auto">
            <Summary clusterId={clusterId} target={target} detail={detail.data} />
          </Tabs.Content>
          {/* forceMount: el editor conserva lo escrito al cambiar de pestaña. */}
          <Tabs.Content value="yaml" forceMount hidden={tab !== "yaml"} className="min-h-0 flex-1">
            {(tab === "yaml" || dirty) && (
              <Suspense
                fallback={<p className="p-3 text-sm text-muted">{t("detail.loadingEditor")}</p>}
              >
                <YamlTab
                  key={`${clusterId}/${target.namespace}/${target.name}`}
                  clusterId={clusterId}
                  target={target}
                  detail={detail.data}
                  changedRemotely={changedRemotely}
                  onDirtyChange={setDirty}
                  onApplied={onApplied}
                />
              </Suspense>
            )}
          </Tabs.Content>
          <Tabs.Content value="events" className="min-h-0 flex-1 overflow-y-auto">
            <Events events={detail.data.events} />
          </Tabs.Content>
          <Tabs.Content value="related" className="min-h-0 flex-1 overflow-y-auto">
            <Related related={detail.data.related} onNavigate={onNavigate} />
          </Tabs.Content>
        </Tabs.Root>
      )}
    </aside>
  );
}

function Events({ events }: { events: import("@oculus/bindings").EventInfo[] }) {
  const { t } = useTranslation();
  const now = useNow();
  if (events.length === 0) return <p className="p-4 text-sm text-muted">{t("detail.noEvents")}</p>;
  return (
    <ul className="divide-y divide-border">
      {events.map((e) => (
        <li key={`${e.reason}-${e.lastSeen}-${e.message}`} className="px-4 py-2.5 text-sm">
          <div className="flex items-center gap-2">
            <StatusDot tone={e.type === "Warning" ? "warning" : "neutral"} />
            <span className="font-medium">{e.reason}</span>
            {e.count > 1 && <span className="text-xs text-muted">×{e.count}</span>}
            <span className="ml-auto text-xs text-muted" title={e.lastSeen ?? ""}>
              {formatAge(e.lastSeen, now)}
            </span>
          </div>
          <p className="mt-0.5 text-xs break-words text-muted">{e.message}</p>
        </li>
      ))}
    </ul>
  );
}

function Related({
  related,
  onNavigate,
}: {
  related: RelatedRef[];
  onNavigate: (t: ObjectRef) => void;
}) {
  const { t } = useTranslation();
  if (related.length === 0)
    return <p className="p-4 text-sm text-muted">{t("detail.noRelated")}</p>;
  const groups = (["owner", "selects", "node", "mounts"] as const)
    .map((rel) => [rel, related.filter((r) => r.relation === rel)] as const)
    .filter(([, items]) => items.length > 0);
  return (
    <div className="space-y-5 p-4">
      {groups.map(([rel, items]) => (
        <Section key={rel} title={t(`detail.relation.${rel}`)}>
          <ul className="space-y-1 text-sm">
            {items.map((r) => (
              <li key={`${r.kind}/${r.name}`} className="flex items-center gap-2">
                <span className="w-36 shrink-0 text-xs text-muted">{r.kind}</span>
                {r.target ? (
                  <button
                    type="button"
                    onClick={() => r.target && onNavigate(r.target)}
                    className="truncate text-left text-accent hover:underline focus-visible:outline-2 focus-visible:outline-focus"
                  >
                    {r.name}
                  </button>
                ) : (
                  <span className="truncate">{r.name}</span>
                )}
              </li>
            ))}
          </ul>
        </Section>
      ))}
    </div>
  );
}
