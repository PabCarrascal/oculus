import type { ClusterSummary } from "@oculus/bindings";
import { Button } from "@oculus/ui";
import { useTranslation } from "react-i18next";
import { useConnect, useDisconnect } from "../../stores/clusters";

/** Botón principal según el estado: conectar, reintentar o desconectar. */
export function ClusterActions({ cluster }: { cluster: ClusterSummary }) {
  const { t } = useTranslation();
  const connect = useConnect();
  const disconnect = useDisconnect();
  const { state } = cluster.status;

  if (state === "connected" || state === "connecting") {
    return (
      <Button variant="ghost" onClick={() => disconnect.mutate(cluster.id)}>
        {t("clusters.disconnect")}
      </Button>
    );
  }
  return (
    <Button
      variant={state === "error" ? "secondary" : "primary"}
      onClick={() => connect.mutate(cluster.id)}
    >
      {state === "error" ? t("clusters.retry") : t("clusters.connect")}
    </Button>
  );
}
