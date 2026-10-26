import { commands } from "@oculus/bindings";
import { useQuery } from "@tanstack/react-query";

/** Espera a que el navegador haya pintado de verdad: rAF corre antes de pintar, el segundo ya después. */
function afterPaint() {
  return new Promise<void>((resolve) =>
    requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
  );
}

export function useStartupReport() {
  return useQuery({
    queryKey: ["startup"],
    queryFn: async () => {
      await afterPaint();
      return commands.startupReport();
    },
  });
}
