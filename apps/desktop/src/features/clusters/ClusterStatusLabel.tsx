import type { ClusterStatus } from "@oculus/bindings";
import { StatusDot, type Tone } from "@oculus/ui";
import { useTranslation } from "react-i18next";

export function statusTone(status: ClusterStatus): Tone {
  switch (status.state) {
    case "connected":
      return status.degraded ? "warning" : "ok";
    case "connecting":
      return "progress";
    case "error":
      return "error";
    case "disconnected":
      return "neutral";
  }
}

export function ClusterStatusLabel({ status }: { status: ClusterStatus }) {
  const { t } = useTranslation();
  const text =
    status.state === "connected"
      ? t("clusters.state.connected", { version: status.serverVersion })
      : status.state === "connecting"
        ? t(`clusters.state.${status.step}`)
        : t(`clusters.state.${status.state}`);
  return (
    <span className="inline-flex shrink-0 items-center gap-1.5 whitespace-nowrap text-xs text-muted">
      <StatusDot tone={statusTone(status)} />
      {text}
    </span>
  );
}
