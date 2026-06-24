import { Component, type ErrorInfo, type ReactNode } from "react";

interface Props {
  // A label for the area being guarded, shown in the fallback ("The Assistant…").
  label?: string;
  children: ReactNode;
}

interface State {
  error: Error | null;
}

// Contains render-time crashes so one view (e.g. a markdown/KaTeX parse error while an
// answer streams in) shows a recoverable fallback instead of unmounting the whole app
// to a blank window. "Try again" clears the error so the next render re-mounts the view.
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // Surface to the console for debugging; the UI shows the friendly fallback.
    console.error("UI error caught by ErrorBoundary:", error, info.componentStack);
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    const what = this.props.label ? `The ${this.props.label}` : "This view";
    return (
      <div className="error-boundary">
        <h3>{what} hit a snag.</h3>
        <p className="muted sm">{error.message || String(error)}</p>
        <button className="btn-open" onClick={() => this.setState({ error: null })}>
          Try again
        </button>
      </div>
    );
  }
}
