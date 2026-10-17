import { Component, type ErrorInfo, type ReactNode } from "react";

// Error Boundary pro Projekt-Route. Ein Renderfehler in BSC lässt Project 0 und die
// Navigation weiterlaufen. Setzt sich bei Routenwechsel (resetKey) zurück.

interface Props {
  name: string;
  resetKey: string;
  children: ReactNode;
}

interface State {
  error: Error | null;
}

export class FeatureBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    // Kein externer Reporter in V1; Konsole genügt für lokale Diagnose.
    if (import.meta.env.DEV) console.error(`[FeatureBoundary:${this.props.name}]`, error, info.componentStack);
  }

  componentDidUpdate(prev: Props): void {
    if (prev.resetKey !== this.props.resetKey && this.state.error) {
      this.setState({ error: null });
    }
  }

  render(): ReactNode {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <section role="alert" className="mx-auto mt-10 max-w-3xl rounded-md border border-border bg-background p-6 text-sm">
        <h2 className="font-semibold">{this.props.name} UI failed</h2>
        <p className="mt-2 text-muted-foreground">
          This project view could not render. Other projects remain usable. No backend action was taken.
        </p>
        <p className="mt-3 text-xs text-muted-foreground">The view was isolated. See local developer diagnostics for details.</p>
        <button
          type="button"
          className="mt-4 rounded-md border border-border px-3 py-1.5 hover:bg-secondary"
          onClick={() => this.setState({ error: null })}
        >
          Retry
        </button>
      </section>
    );
  }
}
