import {
  type ColumnDef,
  commands,
  type KindRef,
  type OculusError,
  type ResourceEvent,
  type Row,
  type Scope,
  type WatchState,
} from "@oculus/bindings";
import { Channel } from "@tauri-apps/api/core";
import { useEffect, useState, useSyncExternalStore } from "react";

export interface ResourceView {
  columns: ColumnDef[];
  rows: Row[];
  state: WatchState;
  error: OculusError | null;
  /** Ya llegó el primer snapshot. */
  loaded: boolean;
}

const EMPTY: ResourceView = {
  columns: [],
  rows: [],
  state: "initializing",
  error: null,
  loaded: false,
};

/** Filas de una suscripción: aplica snapshot y deltas y avisa a React. */
export class ResourceStore {
  private byUid = new Map<string, Row>();
  private view: ResourceView = EMPTY;
  private listeners = new Set<() => void>();

  subscribe = (cb: () => void) => {
    this.listeners.add(cb);
    return () => this.listeners.delete(cb);
  };

  getSnapshot = () => this.view;

  apply(event: ResourceEvent) {
    switch (event.type) {
      case "snapshot":
        this.byUid = new Map(event.rows.map((r) => [r.uid, r]));
        this.publish({ columns: event.columns, loaded: true });
        break;
      case "delta":
        for (const uid of event.deletes) this.byUid.delete(uid);
        for (const row of event.upserts) this.byUid.set(row.uid, row);
        this.publish({});
        break;
      case "status":
        this.publish({ state: event.state, error: event.error }, false);
        break;
    }
  }

  fail(error: OculusError) {
    this.publish({ state: "error", error }, false);
  }

  private publish(patch: Partial<ResourceView>, rowsChanged = true) {
    this.view = {
      ...this.view,
      ...patch,
      ...(rowsChanged ? { rows: [...this.byUid.values()] } : {}),
    };
    for (const l of this.listeners) l();
  }
}

/**
 * Se suscribe a un tipo mientras el componente está montado. Al cambiar de
 * tipo o de ámbito se da de baja de la anterior; el backend mantiene el
 * watcher unos segundos (linger) por si se vuelve.
 */
export function useResourceSubscription(
  clusterId: string,
  kind: KindRef | null,
  scope: Scope,
): ResourceView {
  const [store, setStore] = useState(() => new ResourceStore());
  const scopeKey = JSON.stringify(scope);
  const kindKey = kind ? `${kind.group}/${kind.version}/${kind.plural}` : "";

  // biome-ignore lint/correctness/useExhaustiveDependencies: kindKey y scopeKey resumen kind y scope.
  useEffect(() => {
    if (!kind) return;
    const next = new ResourceStore();
    setStore(next);
    const channel = new Channel<ResourceEvent>();
    channel.onmessage = (event) => next.apply(event);
    let subscription: string | null = null;
    let cancelled = false;

    // specta tipa el Channel con la forma de entrada del evento; lo que llega es la de salida.
    const ipcChannel = channel as unknown as Parameters<typeof commands.resourcesSubscribe>[3];
    void commands
      .resourcesSubscribe(clusterId, kind, JSON.parse(scopeKey) as Scope, ipcChannel)
      .then((res) => {
        if (res.status === "error") {
          next.fail(res.error);
        } else if (cancelled) {
          void commands.resourcesUnsubscribe(clusterId, res.data);
        } else {
          subscription = res.data;
        }
      });
    return () => {
      cancelled = true;
      if (subscription) void commands.resourcesUnsubscribe(clusterId, subscription);
    };
  }, [clusterId, kindKey, scopeKey]);

  return useSyncExternalStore(store.subscribe, store.getSnapshot);
}
