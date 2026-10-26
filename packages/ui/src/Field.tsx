import type { ReactNode } from "react";

interface FieldProps {
  label: string;
  hint?: string;
  htmlFor?: string;
  children: ReactNode;
}

/** Fila de formulario: etiqueta y ayuda a la izquierda, control a la derecha. */
export function Field({ label, hint, htmlFor, children }: FieldProps) {
  return (
    <div className="flex items-start justify-between gap-8 py-3">
      <div className="min-w-0 max-w-md">
        <label htmlFor={htmlFor} className="text-sm font-medium text-text">
          {label}
        </label>
        {hint && <p className="mt-0.5 text-xs text-muted">{hint}</p>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}
