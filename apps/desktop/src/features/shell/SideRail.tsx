import { cn } from "@oculus/ui";
import { Link } from "@tanstack/react-router";
import { House, Settings } from "lucide-react";
import type { ComponentType } from "react";
import { useTranslation } from "react-i18next";

export function SideRail() {
  const { t } = useTranslation();
  return (
    <nav
      aria-label={t("nav.clusters")}
      className="row-span-1 flex flex-col items-center gap-1 border-r border-border bg-surface py-3"
    >
      <RailLink to="/" label={t("nav.clusters")} icon={House} />
      <div className="flex-1" />
      <RailLink to="/settings" label={t("nav.settings")} icon={Settings} />
    </nav>
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
      className={cn(
        "flex size-10 items-center justify-center rounded-lg text-muted transition-colors",
        "hover:bg-surface-2 hover:text-text focus-visible:outline-2 focus-visible:outline-focus",
      )}
      activeProps={{ className: "bg-surface-2 text-text", "aria-current": "page" }}
      activeOptions={{ exact: true }}
    >
      <Icon size={20} aria-hidden />
    </Link>
  );
}
