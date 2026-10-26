import { Boxes } from "lucide-react";
import { useTranslation } from "react-i18next";

export function HomePage() {
  const { t } = useTranslation();
  return (
    <div className="mx-auto flex h-full max-w-4xl flex-col px-8 py-6">
      <h1 className="text-lg font-semibold">{t("home.title")}</h1>
      <div className="flex flex-1 flex-col items-center justify-center gap-3 text-center">
        <Boxes size={40} className="text-muted" aria-hidden />
        <h2 className="text-base font-medium">{t("home.emptyTitle")}</h2>
        <p className="max-w-sm text-sm text-muted">{t("home.emptyBody")}</p>
      </div>
    </div>
  );
}
