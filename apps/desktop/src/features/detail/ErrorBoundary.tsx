import type { OculusError } from "@oculus/bindings";
import { Button } from "@oculus/ui";
import { Component, type ReactNode } from "react";
import { ErrorNotice } from "../clusters/ErrorNotice";

interface Props {
  /** Texto del botón de reintento. */
  retryLabel: string;
  children: ReactNode;
}

/** Si una parte cargada en diferido (Monaco) falla, se muestra el error y se puede reintentar. */
export class ErrorBoundary extends Component<Props, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error) {
    console.error("fallo en el panel de detalle:", error);
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    const notice: OculusError = { code: "Internal", message: error.message, details: null };
    return (
      <div className="p-3">
        <ErrorNotice
          error={notice}
          action={
            <Button variant="ghost" onClick={() => this.setState({ error: null })}>
              {this.props.retryLabel}
            </Button>
          }
        />
      </div>
    );
  }
}
