import type { ClusterSummary } from "@oculus/bindings";
import { cn, StatusDot } from "@oculus/ui";
import { Link } from "@tanstack/react-router";
import { House, Settings } from "lucide-react";
import type { ComponentType } from "react";
import { useTranslation } from "react-i18next";
import { useClusters } from "../../stores/clusters";
import { statusTone } from "../clusters/ClusterStatusLabel";
import { displayName, initials } from "../clusters/naming";

const railItem = cn(
  "relative flex size-10 items-center justify-center rounded-lg text-muted transition-colors",
  "hover:bg-surface-2 hover:text-text focus-visible:outline-2 focus-visible:outline-focus",
);

export function SideRail() {
  const { t } = useTranslation();
  // Los clústeres "activos" (conectados, conectando o con error) quedan a mano.
  const active = (useClusters().data ?? []).filter((c) => c.status.state !== "disconnected");

  return (
    <nav
      aria-label={t("nav.clusters")}
      className="flex flex-col items-center gap-1 border-r border-border bg-surface py-3"
    >
      <RailLink to="/" label={t("nav.clusters")} icon={House} />
      {active.length > 0 && <div className="my-1 h-px w-6 bg-border" />}
      {active.map((c) => (
        <ClusterAvatar key={c.id} cluster={c} />
      ))}
      <div className="flex-1" />
      <RailLink to="/settings" label={t("nav.settings")} icon={Settings} />
    </nav>
  );
}

function ClusterAvatar({ cluster }: { cluster: ClusterSummary }) {
  const name = displayName(cluster);
  return (
    <Link
      to="/clusters/$clusterId"
      params={{ clusterId: cluster.id }}
      title={name}
      aria-label={name}
      className={railItem}
      activeProps={{ className: "bg-surface-2 text-text", "aria-current": "page" }}
    >
      <span
        className={cn(
          "flex size-8 items-center justify-center rounded-md text-xs font-semibold",
          // Sin color propio: fondo neutro con el color de texto normal, legible en ambos temas.
          cluster.prefs.color ? "text-white" : "border border-border bg-surface-2 text-text",
        )}
        style={cluster.prefs.color ? { backgroundColor: cluster.prefs.color } : undefined}
      >
        {initials(cluster)}
      </span>
      <StatusDot
        tone={statusTone(cluster.status)}
        className="absolute right-0.5 bottom-0.5 ring-2 ring-surface"
      />
    </Link>
  );
}

interface RailLinkProps {
  to: "/" | "/settings";
  label: string;
  icon: ComponentType<{ size?: number; "aria-hidden"?: boolean }>;
}

function RailLink({ to, label, icon: Icon }: RailLinkProps) {
  return (
    <Link
      to={to}
      title={label}
      aria-label={label}
      className={railItem}
      activeProps={{ className: "bg-surface-2 text-text", "aria-current": "page" }}
      activeOptions={{ exact: true }}
    >
      <Icon size={20} aria-hidden />
    </Link>
  );
}
