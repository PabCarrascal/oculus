import type { Locale, Theme } from "@oculus/bindings";
import { Kbd } from "@oculus/ui";
import { useNavigate } from "@tanstack/react-router";
import { Command } from "cmdk";
import {
  Boxes,
  House,
  Languages,
  type LucideIcon,
  Moon,
  Plug,
  Settings,
  Unplug,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { modKey } from "../../lib/platform";
import { useClusters, useConnect, useDisconnect } from "../../stores/clusters";
import { useSettings, useUpdateSettings } from "../../stores/settings";
import { useUi } from "../../stores/ui";
import { displayName } from "../clusters/naming";

const THEMES: Theme[] = ["system", "light", "dark"];
const LOCALES: Locale[] = ["es", "en"];
const THEME_LABEL = { system: "themeSystem", light: "themeLight", dark: "themeDark" } as const;

export function CommandPalette() {
  const { t } = useTranslation();
  const open = useUi((s) => s.paletteOpen);
  const setOpen = useUi((s) => s.setPaletteOpen);
  const navigate = useNavigate();
  const settings = useSettings();
  const update = useUpdateSettings();
  const clusters = useClusters().data ?? [];
  const connect = useConnect();
  const disconnect = useDisconnect();

  const run = (action: () => void) => () => {
    setOpen(false);
    action();
  };

  return (
    <Command.Dialog
      open={open}
      onOpenChange={setOpen}
      label={t("status.palette")}
      overlayClassName="fixed inset-0 bg-overlay"
      contentClassName="fixed left-1/2 top-[15vh] w-[560px] max-w-[calc(100vw-32px)] -translate-x-1/2 overflow-hidden rounded-xl border border-border bg-surface shadow-2xl"
    >
      <Command.Input
        placeholder={t("palette.placeholder")}
        className="h-12 w-full border-b border-border bg-transparent px-4 text-sm text-text outline-none placeholder:text-muted"
      />
      <Command.List className="max-h-80 overflow-y-auto p-2">
        <Command.Empty className="px-3 py-6 text-center text-sm text-muted">
          {t("palette.empty")}
        </Command.Empty>

        <Command.Group heading={t("palette.groupNavigation")} className={groupClass}>
          <Item icon={House} onSelect={run(() => void navigate({ to: "/" }))}>
            {t("palette.goClusters")}
          </Item>
          <Item
            icon={Settings}
            shortcut={`${modKey},`}
            onSelect={run(() => void navigate({ to: "/settings" }))}
          >
            {t("palette.openSettings")}
          </Item>
        </Command.Group>

        {clusters.length > 0 && (
          <Command.Group heading={t("paletteClusters.group")} className={groupClass}>
            {clusters.map((c) => {
              const name = displayName(c);
              const live = c.status.state === "connected" || c.status.state === "connecting";
              return [
                <Item
                  key={`open-${c.id}`}
                  icon={Boxes}
                  onSelect={run(
                    () =>
                      void navigate({ to: "/clusters/$clusterId", params: { clusterId: c.id } }),
                  )}
                >
                  {t("paletteClusters.open", { name })}
                </Item>,
                <Item
                  key={`toggle-${c.id}`}
                  icon={live ? Unplug : Plug}
                  onSelect={run(() => (live ? disconnect.mutate(c.id) : connect.mutate(c.id)))}
                >
                  {t(live ? "paletteClusters.disconnect" : "paletteClusters.connect", { name })}
                </Item>,
              ];
            })}
          </Command.Group>
        )}

        <Command.Group heading={t("palette.groupPreferences")} className={groupClass}>
          {THEMES.filter((theme) => theme !== settings.theme).map((theme) => (
            <Item key={theme} icon={Moon} onSelect={run(() => update.mutate({ theme }))}>
              {t("palette.themeTo", { theme: t(`settings.${THEME_LABEL[theme]}`) })}
            </Item>
          ))}
          {LOCALES.filter((locale) => locale !== settings.locale).map((locale) => (
            <Item key={locale} icon={Languages} onSelect={run(() => update.mutate({ locale }))}>
              {t("palette.languageTo", { language: t(`languages.${locale}`) })}
            </Item>
          ))}
        </Command.Group>
      </Command.List>
    </Command.Dialog>
  );
}

const groupClass =
  "[&_[cmdk-group-heading]]:px-3 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:pt-2 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group-heading]]:text-muted";

interface ItemProps {
  icon: LucideIcon;
  shortcut?: string;
  onSelect: () => void;
  children: string;
}

function Item({ icon: Icon, shortcut, onSelect, children }: ItemProps) {
  return (
    <Command.Item
      onSelect={onSelect}
      className="flex h-9 items-center gap-3 rounded-md px-3 text-sm text-text data-[selected=true]:bg-surface-2"
    >
      <Icon size={16} className="text-muted" aria-hidden />
      <span className="flex-1">{children}</span>
      {shortcut && <Kbd>{shortcut}</Kbd>}
    </Command.Item>
  );
}
