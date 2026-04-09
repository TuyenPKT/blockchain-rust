import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

class ErrorBoundary extends React.Component<
  { children: React.ReactNode },
  { error: string | null }
> {
  constructor(props: { children: React.ReactNode }) {
    super(props);
    this.state = { error: null };
  }
  static getDerivedStateFromError(err: unknown) {
    return { error: String(err) };
  }
  render() {
    if (this.state.error) {
      return (
        <div style={{
          padding: 32, fontFamily: "monospace", color: "#f06060",
          background: "#0b0f1a", minHeight: "100vh",
          display: "flex", flexDirection: "column", gap: 16,
        }}>
          <div style={{ fontSize: 18, fontWeight: 700 }}>⚠ Render Error</div>
          <pre style={{ fontSize: 13, whiteSpace: "pre-wrap", color: "#e6edf3" }}>
            {this.state.error}
          </pre>
          <button
            onClick={() => this.setState({ error: null })}
            style={{
              padding: "8px 20px", background: "#f7a133", border: "none",
              borderRadius: 8, fontWeight: 700, cursor: "pointer", width: "fit-content",
            }}
          >↻ Retry</button>
        </div>
      );
    }
    return this.props.children;
  }
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
