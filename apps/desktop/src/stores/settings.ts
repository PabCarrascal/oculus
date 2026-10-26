import { commands, type OculusError, type Settings, type SettingsPatch } from "@oculus/bindings";
import { setLocale } from "@oculus/i18n";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { applyTheme } from "../lib/theme";

const KEY = ["settings"] as const;

/**
 * main.tsx precarga los ajustes, así que aquí nunca hay estado de carga. Solo
 * cambian por `useUpdateSettings`, que actualiza la caché: nunca se vuelven a pedir.
 */
export function useSettings(): Settings {
  const { data } = useQuery({
    queryKey: KEY,
    queryFn: () => commands.settingsGet(),
    staleTime: Number.POSITIVE_INFINITY,
  });
  if (!data) throw new Error("ajustes no precargados");
  return data;
}

export function useUpdateSettings() {
  const client = useQueryClient();
  return useMutation<Settings, OculusError, SettingsPatch>({
    mutationFn: async (patch) => {
      const res = await commands.settingsSet(patch);
      if (res.status === "error") throw res.error;
      return res.data;
    },
    onSuccess: (settings) => {
      client.setQueryData(KEY, settings);
      applyTheme(settings.theme);
      setLocale(settings.locale);
    },
  });
}
