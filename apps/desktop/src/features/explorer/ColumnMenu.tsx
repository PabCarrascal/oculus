import type { ColumnDef } from "@oculus/bindings";
import { Button } from "@oculus/ui";
import { Columns3 } from "lucide-react";
import { DropdownMenu } from "radix-ui";
import { useTranslation } from "react-i18next";

interface Props {
  columns: ColumnDef[];
  visible: Set<string>;
  onToggle: (columnId: string) => void;
}

export function ColumnMenu({ columns, visible, onToggle }: Props) {
  const { t } = useTranslation();
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <Button
          variant="ghost"
          className="px-2"
          aria-label={t("explorer.columns")}
          title={t("explorer.columns")}
        >
          <Columns3 size={16} aria-hidden />
        </Button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          align="end"
          sideOffset={4}
          className="z-50 min-w-48 rounded-lg border border-border bg-surface p-1 shadow-xl"
        >
          {columns
            .filter((c) => c.id !== "name")
            .map((c) => (
              <DropdownMenu.CheckboxItem
                key={c.id}
                checked={visible.has(c.id)}
                onCheckedChange={() => onToggle(c.id)}
                onSelect={(e) => e.preventDefault()}
                className="flex cursor-default items-center gap-2 rounded px-2 py-1 text-sm outline-none data-[highlighted]:bg-surface-2"
              >
                <span className="w-3 text-accent">{visible.has(c.id) ? "✓" : ""}</span>
                {c.translate ? t(c.label as never) : c.label}
              </DropdownMenu.CheckboxItem>
            ))}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}
