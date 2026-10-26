import type { OculusError } from "@oculus/bindings";
import { CircleAlert } from "lucide-react";
import type { ReactNode } from "react";

/** Error del backend con su pista, que es lo que el usuario puede hacer. */
export function ErrorNotice({ error, action }: { error: OculusError; action?: ReactNode }) {
  return (
    <div
      role="alert"
      className="flex gap-3 rounded-md border border-error/40 bg-error/5 px-3 py-2.5 text-sm"
    >
      <CircleAlert size={16} className="mt-0.5 shrink-0 text-error" aria-hidden />
      <div className="min-w-0 flex-1">
        <p className="break-words text-text">{error.message}</p>
        {error.details?.hint && <p className="mt-1 text-muted">{error.details.hint}</p>}
      </div>
      {action && <div className="shrink-0">{action}</div>}
    </div>
  );
}
