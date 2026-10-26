import type { WatchState } from "@oculus/bindings";
import { create } from "zustand";

export interface ActiveWatch {
  state: WatchState;
  rows: number;
}

interface UiState {
  paletteOpen: boolean;
  setPaletteOpen: (open: boolean) => void;
  togglePalette: () => void;
  /** Estado del watch de la vista de recursos abierta, para la barra de estado. */
  activeWatch: ActiveWatch | null;
  setActiveWatch: (watch: ActiveWatch | null) => void;
}

export const useUi = create<UiState>((set) => ({
  paletteOpen: false,
  setPaletteOpen: (paletteOpen) => set({ paletteOpen }),
  togglePalette: () => set((s) => ({ paletteOpen: !s.paletteOpen })),
  activeWatch: null,
  setActiveWatch: (activeWatch) => set({ activeWatch }),
}));
