import {
  type ClusterPrefs,
  type ClusterSummary,
  commands,
  type OculusError,
  type SshHostInfo,
} from "@oculus/bindings";
import { Button, cn, Dialog, Field, Input, Switch } from "@oculus/ui";
import { useMutation } from "@tanstack/react-query";
import { Check } from "lucide-react";
import { ToggleGroup } from "radix-ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { unwrap } from "../../lib/ipc";
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
        <SshTunnelField
          host={prefs.sshTunnel?.host ?? ""}
          hasProxyUrl={cluster.hasProxyUrl}
          onChange={(host) => patch({ sshTunnel: host ? { host } : null })}
        />
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

/** Host del bastión, con «Probar» para ver adónde irá ssh sin conectar. */
function SshTunnelField({
  host,
  hasProxyUrl,
  onChange,
}: {
  host: string;
  hasProxyUrl: boolean;
  onChange: (host: string) => void;
}) {
  const { t } = useTranslation();
  const check = useMutation<SshHostInfo, OculusError, string>({
    mutationFn: (h) => unwrap(commands.sshTunnelCheck(h)),
  });
  const trimmed = host.trim();
  return (
    <div>
      <Field
        label={t("clusters.sshTunnel")}
        hint={t("clusters.sshTunnelHint")}
        htmlFor="ssh-tunnel"
      >
        <div className="flex gap-2">
          <Input
            id="ssh-tunnel"
            className="w-40 font-mono text-xs"
            value={host}
            placeholder={t("clusters.sshTunnelNone")}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            onChange={(e) => {
              check.reset();
              onChange(e.target.value);
            }}
          />
          <Button
            variant="ghost"
            disabled={!trimmed || check.isPending}
            onClick={() => check.mutate(trimmed)}
          >
            {check.isPending ? t("clusters.sshTunnelChecking") : t("clusters.sshTunnelCheck")}
          </Button>
        </div>
      </Field>
      {check.data && (
        <p className="-mt-1 pb-3 text-xs text-muted" role="status">
          {t("clusters.sshTunnelResolved", {
            user: check.data.user,
            hostname: check.data.hostname,
            port: check.data.port,
          })}
          {check.data.proxyJump && ` ${t("clusters.sshTunnelVia", { jump: check.data.proxyJump })}`}
        </p>
      )}
      {check.isError && (
        <div className="pb-3">
          <ErrorNotice error={check.error} />
        </div>
      )}
      {trimmed && hasProxyUrl && (
        <p className="-mt-1 pb-3 text-xs text-warning">{t("clusters.sshTunnelProxyUrl")}</p>
      )}
    </div>
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
