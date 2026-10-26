import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import type { Sort } from "../features/explorer/rows";

interface ExplorerPrefs {
  /** Columnas que el usuario cambió respecto a su visibilidad por defecto, por tipo. */
  toggled: Record<string, string[]>;
  sort: Record<string, Sort>;
  toggleColumn: (kindKey: string, columnId: string) => void;
  setSort: (kindKey: string, sort: Sort) => void;
}

// Preferencias de vista de este equipo: si el almacenamiento falla, la app sigue con los valores por defecto.
export const useExplorerPrefs = create<ExplorerPrefs>()(
  persist(
    (set) => ({
      toggled: {},
      sort: {},
      toggleColumn: (kindKey, columnId) =>
        set((s) => {
          const current = new Set(s.toggled[kindKey] ?? []);
          if (current.has(columnId)) current.delete(columnId);
          else current.add(columnId);
          return { toggled: { ...s.toggled, [kindKey]: [...current] } };
        }),
      setSort: (kindKey, sort) => set((s) => ({ sort: { ...s.sort, [kindKey]: sort } })),
    }),
    { name: "oculus.explorer", storage: createJSONStorage(() => localStorage) },
  ),
);
