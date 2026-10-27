import { commands, type ObjectRef, type OculusError, type ResourceDetail } from "@oculus/bindings";
import { useQuery } from "@tanstack/react-query";
import { unwrap } from "../lib/ipc";

export const detailKey = (clusterId: string, target: ObjectRef) =>
  ["detail", clusterId, target] as const;

export function useDetail(clusterId: string, target: ObjectRef | null) {
  return useQuery<ResourceDetail, OculusError>({
    queryKey: target ? detailKey(clusterId, target) : ["detail", clusterId, null],
    queryFn: () => unwrap(commands.resourceGet(clusterId, target as ObjectRef)),
    enabled: target != null,
    // El objeto puede cambiar en cualquier momento: el panel decide cuándo recargar.
    staleTime: 0,
  });
}

/** Esquema JSON del tipo para validar el editor. `null` si el clúster no lo publica. */
export function useSchema(clusterId: string, group: string, version: string, kind: string) {
  return useQuery<object | null, OculusError>({
    queryKey: ["schema", clusterId, group, version, kind],
    queryFn: async () => {
      const text = await unwrap(commands.openapiSchema(clusterId, group, version, kind));
      return text ? (JSON.parse(text) as object) : null;
    },
    staleTime: Number.POSITIVE_INFINITY,
  });
}
