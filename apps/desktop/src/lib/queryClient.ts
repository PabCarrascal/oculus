import { QueryClient } from "@tanstack/react-query";

// Los datos llegan de un proceso local, no de la red: sin reintentos ni refetch
// al recuperar el foco.
export function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, refetchOnWindowFocus: false, staleTime: Number.POSITIVE_INFINITY },
      mutations: { retry: false },
    },
  });
}

export const queryClient = createQueryClient();
