import type { WatchState } from "@oculus/bindings";
import { Kbd, StatusDot, type Tone } from "@oculus/ui";
import { useTranslation } from "react-i18next";
import { modKey } from "../../lib/platform";
import { useClusters } from "../../stores/clusters";
import { useUi } from "../../stores/ui";

const WATCH_TONE: Record<WatchState, Tone> = {
  initializing: "progress",
  ready: "ok",
  relisting: "progress",
  backoff: "warning",
  error: "error",
};

export function StatusBar() {
  const { t } = useTranslation();
  const setPaletteOpen = useUi((s) => s.setPaletteOpen);
  const connected = (useClusters().data ?? []).filter((c) => c.status.state === "connected").length;
  const watch = useUi((s) => s.activeWatch);

  return (
    <footer className="col-span-2 flex h-6 items-center justify-between border-t border-border bg-surface px-3 text-xs text-muted">
      <span className="flex items-center gap-3">
        {connected > 0 ? t("statusBar.connected", { count: connected }) : t("status.noClusters")}
        {watch && (
          <span className="flex items-center gap-1.5">
            <StatusDot tone={WATCH_TONE[watch.state]} />
            {t(`explorer.watch.${watch.state}`)} · {t("explorer.rows", { count: watch.rows })}
          </span>
        )}
      </span>
      <button
        type="button"
        onClick={() => setPaletteOpen(true)}
        className="flex items-center gap-1.5 rounded px-1 hover:text-text focus-visible:outline-2 focus-visible:outline-focus"
      >
        <Kbd>{modKey}K</Kbd>
        {t("status.palette")}
      </button>
    </footer>
  );
}
