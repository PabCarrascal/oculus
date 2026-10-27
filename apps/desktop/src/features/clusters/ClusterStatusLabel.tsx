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
  let text: string;
  if (status.state === "connected") {
    const tunnel = status.tunnel;
    text =
      tunnel?.state === "reconnecting"
        ? t("clusters.state.tunnelReconnecting")
        : tunnel
          ? t("clusters.state.connectedTunnel", {
              version: status.serverVersion,
              host: tunnel.host,
            })
          : t("clusters.state.connected", { version: status.serverVersion });
  } else if (status.state === "connecting") {
    text = t(`clusters.state.${status.step}` as const);
  } else {
    text = t(`clusters.state.${status.state}` as const);
  }
  return (
    <span className="inline-flex shrink-0 items-center gap-1.5 whitespace-nowrap text-xs text-muted">
      <StatusDot tone={statusTone(status)} />
      {text}
    </span>
  );
}
