import { Outlet, useNavigate } from "@tanstack/react-router";
import { useEffect } from "react";
import { hasMod } from "../../lib/platform";
import { useUi } from "../../stores/ui";
import { CommandPalette } from "../command-palette/CommandPalette";
import { SideRail } from "./SideRail";
import { StatusBar } from "./StatusBar";
import { useStartupReport } from "./startup";

export function AppShell() {
  useGlobalShortcuts();
  // Se dispara desde aquí porque el shell es lo primero que se pinta con contenido.
  useStartupReport();

  return (
    <div className="grid h-full grid-cols-[56px_1fr] grid-rows-[1fr_auto]">
      <SideRail />
      <main className="min-h-0 overflow-auto">
        <Outlet />
      </main>
      <StatusBar />
      <CommandPalette />
    </div>
  );
}

function useGlobalShortcuts() {
  const togglePalette = useUi((s) => s.togglePalette);
  const navigate = useNavigate();

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (!hasMod(e) || e.altKey || e.shiftKey) return;
      if (e.key === "k") {
        e.preventDefault();
        togglePalette();
      } else if (e.key === ",") {
        e.preventDefault();
        void navigate({ to: "/settings" });
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [togglePalette, navigate]);
}
