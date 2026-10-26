import type { ClusterPrefs, ClusterSummary } from "@oculus/bindings";
import { Button, cn, Dialog, Field, Input, Switch } from "@oculus/ui";
import { Check } from "lucide-react";
import { ToggleGroup } from "radix-ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useSetPrefs } from "../../stores/clusters";
import { ErrorNotice } from "./ErrorNotice";
import { displayName, PRESET_COLORS } from "./naming";

interface Props {
  cluster: ClusterSummary;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function ClusterPrefsDialog({ cluster, open, onOpenChange }: Props) {
  const { t } = useTranslation();
  const [prefs, setPrefs] = useState<ClusterPrefs>(cluster.prefs);
  const save = useSetPrefs();
  const patch = (p: Partial<ClusterPrefs>) => setPrefs((prev) => ({ ...prev, ...p }));

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (next) setPrefs(cluster.prefs);
        onOpenChange(next);
      }}
      title={t("clusters.prefsTitle", { name: displayName(cluster) })}
      description={cluster.contextName}
      footer={
        <>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            {t("clusters.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={save.isPending}
            onClick={() =>
              save.mutate({ id: cluster.id, prefs }, { onSuccess: () => onOpenChange(false) })
            }
          >
            {t("clusters.save")}
          </Button>
        </>
      }
    >
      {save.isError && <ErrorNotice error={save.error} />}
      <div className="divide-y divide-border">
        <Field label={t("clusters.alias")} hint={t("clusters.aliasHint")} htmlFor="alias">
          <Input
            id="alias"
            className="w-48"
            value={prefs.alias ?? ""}
            placeholder={cluster.contextName}
            onChange={(e) => patch({ alias: e.target.value || null })}
          />
        </Field>
        <Field label={t("clusters.color")}>
          <ToggleGroup.Root
            type="single"
            aria-label={t("clusters.color")}
            value={prefs.color ?? NO_COLOR}
            // Radix emite "" al pulsar la opción activa: se ignora para que siempre haya una.
            onValueChange={(v) => v && patch({ color: v === NO_COLOR ? null : v })}
            className="flex flex-wrap gap-1.5"
          >
            <ColorSwatch value={NO_COLOR} label={t("clusters.noColor")} selected={!prefs.color} />
            {PRESET_COLORS.map((color) => (
              <ColorSwatch
                key={color}
                value={color}
                label={color}
                selected={prefs.color === color}
              />
            ))}
          </ToggleGroup.Root>
        </Field>
        <Field label={t("clusters.readOnly")} hint={t("clusters.readOnlyHint")} htmlFor="read-only">
          <Switch
            id="read-only"
            checked={prefs.readOnly}
            onCheckedChange={(readOnly) => patch({ readOnly })}
          />
        </Field>
        <Field
          label={t("clusters.protected")}
          hint={t("clusters.protectedHint")}
          htmlFor="protected"
        >
          <Switch
            id="protected"
            checked={prefs.protected}
            onCheckedChange={(p) => patch({ protected: p })}
          />
        </Field>
      </div>
    </Dialog>
  );
}

const NO_COLOR = "none";

function ColorSwatch({
  value,
  label,
  selected,
}: {
  value: string;
  label: string;
  selected: boolean;
}) {
  const color = value === NO_COLOR ? undefined : value;
  return (
    <ToggleGroup.Item
      value={value}
      aria-label={label}
      title={label}
      style={color ? { backgroundColor: color } : undefined}
      className={cn(
        "flex size-6 items-center justify-center rounded-full border border-border",
        "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus",
        !color && "bg-surface-2",
      )}
    >
      {selected && <Check size={12} className={color ? "text-white" : "text-muted"} aria-hidden />}
    </ToggleGroup.Item>
  );
}
