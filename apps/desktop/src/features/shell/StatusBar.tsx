import { Kbd } from "@oculus/ui";
import { useTranslation } from "react-i18next";
import { modKey } from "../../lib/platform";
import { useUi } from "../../stores/ui";

export function StatusBar() {
  const { t } = useTranslation();
  const setPaletteOpen = useUi((s) => s.setPaletteOpen);

  return (
    <footer className="col-span-2 flex h-6 items-center justify-between border-t border-border bg-surface px-3 text-xs text-muted">
      <span>{t("status.noClusters")}</span>
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
