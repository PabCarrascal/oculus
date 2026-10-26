import { Button } from "@oculus/ui";
import { ChevronDown } from "lucide-react";
import { Popover } from "radix-ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";

interface Props {
  available: string[];
  selected: string[];
  onChange: (namespaces: string[]) => void;
}

/** Vacío = todos los namespaces. */
export function NamespacePicker({ available, selected, onChange }: Props) {
  const { t } = useTranslation();
  const [filter, setFilter] = useState("");
  const set = new Set(selected);
  const visible = available.filter((ns) => ns.includes(filter.trim()));
  const summary =
    selected.length === 0
      ? t("explorer.allNamespaces")
      : selected.length <= 2
        ? selected.join(", ")
        : t("explorer.namespacesCount", { count: selected.length });

  return (
    <Popover.Root>
      <Popover.Trigger asChild>
        <Button className="max-w-64">
          <span className="truncate">{summary}</span>
          <ChevronDown size={14} aria-hidden />
        </Button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="start"
          sideOffset={4}
          className="z-50 w-64 rounded-lg border border-border bg-surface p-2 shadow-xl"
        >
          <input
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder={t("explorer.filterNamespaces")}
            aria-label={t("explorer.filterNamespaces")}
            className="mb-2 h-7 w-full rounded border border-border bg-bg px-2 text-sm outline-none focus-visible:border-focus"
          />
          <label className="flex items-center gap-2 rounded px-1 py-1 text-sm hover:bg-surface-2">
            <input type="checkbox" checked={selected.length === 0} onChange={() => onChange([])} />
            {t("explorer.allNamespaces")}
          </label>
          <div className="my-1 h-px bg-border" />
          <div className="max-h-64 overflow-y-auto">
            {visible.map((ns) => (
              <label
                key={ns}
                className="flex items-center gap-2 rounded px-1 py-1 font-mono text-xs hover:bg-surface-2"
              >
                <input
                  type="checkbox"
                  checked={set.has(ns)}
                  onChange={() =>
                    onChange(
                      set.has(ns) ? selected.filter((s) => s !== ns) : [...selected, ns].sort(),
                    )
                  }
                />
                {ns}
              </label>
            ))}
          </div>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
