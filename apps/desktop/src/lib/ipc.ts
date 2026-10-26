import type { OculusError } from "@oculus/bindings";

type Result<T> = { status: "ok"; data: T } | { status: "error"; error: OculusError };

/** Convierte el Result de tauri-specta en una promesa que rechaza con el OculusError. */
export async function unwrap<T>(call: Promise<Result<T>>): Promise<T> {
  const res = await call;
  if (res.status === "error") throw res.error;
  return res.data;
}

export function isOculusError(e: unknown): e is OculusError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e;
}
