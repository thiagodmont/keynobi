import { ErrorBoundary as SolidErrorBoundary, type JSX } from "solid-js";
import { captureSentryException } from "@/lib/telemetry/sentry-web";

const CONTAINER_STYLE = {
  flex: "1",
  display: "flex",
  "align-items": "center",
  "justify-content": "center",
  "flex-direction": "column",
  gap: "12px",
  padding: "48px",
  color: "var(--text-primary)",
  background: "var(--bg-primary)",
  "font-family": "var(--font-ui)",
} as const;

const ERROR_BOX_STYLE = {
  background: "var(--bg-tertiary)",
  border: "1px solid var(--error)",
  "border-radius": "8px",
  padding: "16px 24px",
  "max-width": "560px",
  "max-height": "300px",
  overflow: "auto",
  "font-family": "var(--font-mono)",
  "font-size": "12px",
  color: "var(--error)",
  "white-space": "pre-wrap",
  "word-break": "break-word",
} as const;

const RELOAD_BTN_STYLE = {
  padding: "8px 20px",
  background: "var(--accent)",
  color: "#ffffff",
  "border-radius": "4px",
  cursor: "pointer",
  "font-size": "13px",
  "font-weight": "500",
} as const;

/**
 * Top-level error boundary that prevents a single component crash from
 * taking down the entire app. Renders a fallback UI with the error message
 * and a reload button.
 *
 * Usage: wrap the main App content (or individual panels) with this.
 */
export function AppErrorBoundary(props: { children: JSX.Element }): JSX.Element {
  return (
    <SolidErrorBoundary
      fallback={(err, reset) => {
        captureSentryException(err instanceof Error ? err : new Error(String(err)));
        return (
        <div style={CONTAINER_STYLE}>
          <h2 style={{ "font-size": "18px", "font-weight": "600" }}>
            Something went wrong
          </h2>
          <p style={{ color: "var(--text-secondary)", "font-size": "13px" }}>
            An unexpected error occurred in a component. You can try reloading
            the panel or restart the app.
          </p>
          <div style={ERROR_BOX_STYLE}>
            {err instanceof Error ? err.message : String(err)}
            {err instanceof Error && err.stack && (
              <details style={{ "margin-top": "8px" }}>
                <summary style={{ cursor: "pointer", color: "var(--text-muted)" }}>
                  Stack trace
                </summary>
                {err.stack}
              </details>
            )}
          </div>
          <button style={RELOAD_BTN_STYLE} onClick={reset}>
            Try Again
          </button>
        </div>
        );
      }}
    >
      {props.children}
    </SolidErrorBoundary>
  );
}
