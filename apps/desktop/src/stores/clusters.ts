import {
  type ClusterPrefs,
  type ClusterStatus,
  type ClusterSummary,
  commands,
  events,
  type KubeconfigSource,
  type OculusError,
  type ResourceKind,
} from "@oculus/bindings";
import { type QueryClient, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { unwrap } from "../lib/ipc";

export const keys = {
  clusters: ["clusters"] as const,
  sources: ["kubeconfigSources"] as const,
  discovery: (id: string) => ["discovery", id] as const,
  namespaces: (id: string) => ["namespaces", id] as const,
};

export function useClusters() {
  return useQuery({ queryKey: keys.clusters, queryFn: () => unwrap(commands.clustersList()) });
}

export function useCluster(id: string): ClusterSummary | undefined {
  return useClusters().data?.find((c) => c.id === id);
}

export function useSources() {
  return useQuery({
    queryKey: keys.sources,
    queryFn: () => unwrap(commands.kubeconfigListSources()),
  });
}

export function useDiscovery(id: string, enabled: boolean) {
  return useQuery<ResourceKind[], OculusError>({
    queryKey: keys.discovery(id),
    queryFn: () => unwrap(commands.discoveryGet(id)),
    enabled,
  });
}

export function useNamespaces(id: string, enabled: boolean) {
  return useQuery<string[], OculusError>({
    queryKey: keys.namespaces(id),
    queryFn: () => unwrap(commands.namespacesList(id)),
    enabled,
  });
}

/**
 * Mantiene la caché al día con los eventos del backend. Se monta una vez, en el shell.
 */
export function useClusterEvents() {
  const client = useQueryClient();
  useEffect(() => {
    const unlisten = [
      events.clusterStatusChanged.listen(({ payload }) =>
        applyStatus(client, payload.clusterId, payload.status),
      ),
      events.kubeconfigChanged.listen(() => {
        void client.invalidateQueries({ queryKey: keys.clusters });
        void client.invalidateQueries({ queryKey: keys.sources });
      }),
    ];
    return () => {
      for (const p of unlisten) void p.then((stop) => stop());
    };
  }, [client]);
}

export function applyStatus(client: QueryClient, id: string, status: ClusterStatus) {
  client.setQueryData<ClusterSummary[]>(keys.clusters, (list) =>
    list?.map((c) => (c.id === id ? { ...c, status } : c)),
  );
  // Lo que se pidió con la sesión anterior ya no vale.
  if (status.state !== "connected") {
    client.removeQueries({ queryKey: keys.discovery(id) });
    client.removeQueries({ queryKey: keys.namespaces(id) });
  }
}

export function useConnect() {
  return useMutation<null, OculusError, string>({
    mutationFn: (id) => unwrap(commands.clusterConnect(id)),
  });
}

export function useDisconnect() {
  return useMutation<null, OculusError, string>({
    mutationFn: (id) => unwrap(commands.clusterDisconnect(id)),
  });
}

export function useSetPrefs() {
  const client = useQueryClient();
  return useMutation<ClusterPrefs, OculusError, { id: string; prefs: ClusterPrefs }>({
    mutationFn: ({ id, prefs }) => unwrap(commands.clusterSetPrefs(id, prefs)),
    onSuccess: (prefs, { id }) =>
      client.setQueryData<ClusterSummary[]>(keys.clusters, (list) =>
        list?.map((c) => (c.id === id ? { ...c, prefs } : c)),
      ),
  });
}

export function usePickSource() {
  return useMutation<KubeconfigSource | null, OculusError>({
    mutationFn: () => unwrap(commands.kubeconfigPickSource()),
  });
}

export function useRemoveSource() {
  return useMutation<null, OculusError, string>({
    mutationFn: (sourceId) => unwrap(commands.kubeconfigRemoveSource(sourceId)),
  });
}
