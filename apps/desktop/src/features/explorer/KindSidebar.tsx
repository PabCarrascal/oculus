import type { Category, KindRef, ResourceKind } from "@oculus/bindings";
import { cn } from "@oculus/ui";
import { ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { sameKind, watchable } from "../../lib/kinds";

const CATEGORIES: Category[] = [
  "workloads",
  "config",
  "network",
  "storage",
  "access",
  "cluster",
  "custom",
];

interface Props {
  kinds: ResourceKind[];
  active: KindRef | null;
  onSelect: (kind: ResourceKind) => void;
}

export function KindSidebar({ kinds, active, onSelect }: Props) {
  const { t } = useTranslation();
  const [collapsed, setCollapsed] = useState<Set<Category>>(() => new Set(["custom"]));
  const usable = kinds.filter(watchable);

  return (
    <nav
      aria-label={t("clusters.resources")}
      className="w-56 shrink-0 overflow-y-auto border-r border-border bg-surface py-2 text-sm"
    >
      {CATEGORIES.map((category) => {
        const items = usable.filter((k) => k.category === category);
        if (items.length === 0) return null;
        const open = !collapsed.has(category);
        return (
          <div key={category} className="mb-1">
            <button
              type="button"
              aria-expanded={open}
              onClick={() => {
                const next = new Set(collapsed);
                if (open) next.add(category);
                else next.delete(category);
                setCollapsed(next);
              }}
              className="flex w-full items-center gap-1 px-3 py-1 text-xs font-semibold uppercase tracking-wide text-muted hover:text-text focus-visible:outline-2 focus-visible:outline-focus"
            >
              {open ? (
                <ChevronDown size={12} aria-hidden />
              ) : (
                <ChevronRight size={12} aria-hidden />
              )}
              {t(`clusters.category.${category}`)}
              <span className="ml-auto font-normal normal-case">{items.length}</span>
            </button>
            {open && (
              <ul>
                {items.map((k) => {
                  const isActive = active != null && sameKind(active, k);
                  return (
                    <li key={`${k.group}/${k.version}/${k.plural}`}>
                      <button
                        type="button"
                        aria-current={isActive ? "page" : undefined}
                        title={k.group ? `${k.group}/${k.version}` : k.version}
                        onClick={() => onSelect(k)}
                        className={cn(
                          "w-full truncate py-1 pr-3 pl-7 text-left focus-visible:outline-2 focus-visible:outline-focus",
                          isActive
                            ? "bg-surface-2 font-medium text-text"
                            : "text-muted hover:bg-surface-2 hover:text-text",
                        )}
                      >
                        {k.kind}
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
          </div>
        );
      })}
    </nav>
  );
}
