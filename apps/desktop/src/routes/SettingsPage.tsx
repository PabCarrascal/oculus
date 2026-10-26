import { commands, type Locale, type Theme } from "@oculus/bindings";
import { Field, SegmentedControl, Switch } from "@oculus/ui";
import { useQuery } from "@tanstack/react-query";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { useStartupReport } from "../features/shell/startup";
import { useSettings, useUpdateSettings } from "../stores/settings";

export function SettingsPage() {
  const { t } = useTranslation();
  const settings = useSettings();
  const update = useUpdateSettings();

  return (
    <div className="mx-auto max-w-3xl px-8 py-6">
      <h1 className="text-lg font-semibold">{t("settings.title")}</h1>

      {update.isError && (
        <p
          role="alert"
          className="mt-4 rounded-md border border-error px-3 py-2 text-sm text-error"
        >
          {t("settings.saveError")}: {update.error.message}
          {update.error.details?.hint && ` — ${update.error.details.hint}`}
        </p>
      )}

      <Section title={t("settings.appearance")}>
        <Field label={t("settings.theme")}>
          <SegmentedControl<Theme>
            aria-label={t("settings.theme")}
            value={settings.theme}
            onChange={(theme) => update.mutate({ theme })}
            options={[
              { value: "system", label: t("settings.themeSystem") },
              { value: "light", label: t("settings.themeLight") },
              { value: "dark", label: t("settings.themeDark") },
            ]}
          />
        </Field>
        <Field label={t("settings.language")}>
          <SegmentedControl<Locale>
            aria-label={t("settings.language")}
            value={settings.locale}
            onChange={(locale) => update.mutate({ locale })}
            options={[
              { value: "es", label: t("languages.es") },
              { value: "en", label: t("languages.en") },
            ]}
          />
        </Field>
      </Section>

      <Section title={t("settings.kubernetes")}>
        <Field
          label={t("settings.extraPath")}
          hint={t("settings.extraPathHint")}
          htmlFor="extra-path"
        >
          <ExtraPathEditor
            value={settings.extraPath}
            onCommit={(extraPath) => update.mutate({ extraPath })}
          />
        </Field>
      </Section>

      <Section title={t("settings.privacy")}>
        <Field
          label={t("settings.crashReports")}
          hint={t("settings.crashReportsHint")}
          htmlFor="crash-reports"
        >
          <Switch
            id="crash-reports"
            checked={settings.telemetry.crashReports}
            onCheckedChange={(crashReports) => update.mutate({ telemetry: { crashReports } })}
          />
        </Field>
      </Section>

      <About />
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="mt-6">
      <h2 className="text-xs font-semibold uppercase tracking-wide text-muted">{title}</h2>
      <div className="mt-2 divide-y divide-border rounded-lg border border-border bg-surface px-4">
        {children}
      </div>
    </section>
  );
}

/** Se guarda al salir del campo, no en cada tecla. */
function ExtraPathEditor({
  value,
  onCommit,
}: {
  value: string[];
  onCommit: (paths: string[]) => void;
}) {
  const [text, setText] = useState(value.join("\n"));
  return (
    <textarea
      id="extra-path"
      rows={3}
      value={text}
      spellCheck={false}
      onChange={(e) => setText(e.target.value)}
      onBlur={() => {
        const paths = text.split("\n");
        if (paths.join("\n") !== value.join("\n")) onCommit(paths);
      }}
      placeholder="/opt/homebrew/bin"
      className="w-72 resize-y rounded-md border border-border bg-bg px-2 py-1.5 font-mono text-xs text-text outline-none focus-visible:border-focus"
    />
  );
}

function About() {
  const { t } = useTranslation();
  const info = useQuery({ queryKey: ["appInfo"], queryFn: () => commands.appInfo() });
  const startup = useStartupReport();
  const ms = startup.data?.processToFirstPaintMs;

  return (
    <Section title={t("settings.about")}>
      <dl className="grid grid-cols-[max-content_1fr] gap-x-8 gap-y-2 py-3 text-sm">
        <dt className="text-muted">{t("settings.version")}</dt>
        <dd>{info.data?.version ?? "—"}</dd>
        <dt className="text-muted">{t("settings.platform")}</dt>
        <dd>{info.data ? `${info.data.os} · ${info.data.arch}` : "—"}</dd>
        <dt className="text-muted">{t("settings.startup")}</dt>
        <dd>{ms != null ? `${Math.round(ms)} ms` : "—"}</dd>
      </dl>
    </Section>
  );
}
