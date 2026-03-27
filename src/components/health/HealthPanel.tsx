import {
  type JSX,
  For,
  Show,
  createSignal,
  onMount,
} from "solid-js";
import {
  healthChecks,
  healthState,
  healthSummary,
  overallHealth,
  refreshHealthChecks,
  type CheckStatus,
  type HealthCheck,
} from "@/stores/health.store";

// ── Panel visibility signal (module-level, like SettingsPanel) ────────────────

const [healthOpen, setHealthOpen] = createSignal(false);

export function openHealthPanel() {
  setHealthOpen(true);
}

export function closeHealthPanel() {
  setHealthOpen(false);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

const STATUS_ICON: Record<CheckStatus, string> = {
  ok: "✓",
  warning: "⚠",
  error: "✗",
  loading: "⟳",
  skip: "—",
};

const STATUS_COLOR: Record<CheckStatus, string> = {
  ok: "var(--success)",
  warning: "var(--warning)",
  error: "var(--error)",
  loading: "var(--text-muted)",
  skip: "var(--text-disabled)",
};

const STATUS_BG: Record<CheckStatus, string> = {
  ok: "rgba(76,175,80,0.08)",
  warning: "rgba(204,167,0,0.08)",
  error: "rgba(241,76,76,0.08)",
  loading: "transparent",
  skip: "transparent",
};

const CATEGORY_LABEL: Record<string, string> = {
  project: "Project",
  environment: "Environment",
  system: "System",
};

// ── Sub-components ────────────────────────────────────────────────────────────

function CheckRow(props: { check: HealthCheck }): JSX.Element {
  const c = () => props.check;
  const color = () => STATUS_COLOR[c().status];

  return (
    <div
      style={{
        display: "flex",
        "align-items": "flex-start",
        gap: "12px",
        padding: "10px 14px",
        "border-radius": "6px",
        background: STATUS_BG[c().status],
        "border-left": `3px solid ${color()}`,
        transition: "background 0.1s",
      }}
    >
      {/* Status icon */}
      <span
        style={{
          width: "18px",
          height: "18px",
          "border-radius": "50%",
          background: color(),
          color: "#fff",
          "font-size": "11px",
          "font-weight": "700",
          display: "flex",
          "align-items": "center",
          "justify-content": "center",
          "flex-shrink": "0",
          "margin-top": "1px",
        }}
      >
        {STATUS_ICON[c().status]}
      </span>

      {/* Name + detail */}
      <div style={{ flex: "1", "min-width": "0" }}>
        <div
          style={{
            display: "flex",
            "align-items": "center",
            gap: "8px",
            "flex-wrap": "wrap",
          }}
        >
          <span
            style={{
              "font-size": "13px",
              "font-weight": "500",
              color: "var(--text-primary)",
            }}
          >
            {c().name}
          </span>
          <Show when={c().fix}>
            <button
              onClick={() => c().fix?.action()}
              style={{
                background: "none",
                border: "1px solid var(--border)",
                color: "var(--accent)",
                padding: "1px 8px",
                "border-radius": "4px",
                cursor: "pointer",
                "font-size": "11px",
                "font-family": "var(--font-ui)",
              }}
            >
              {c().fix?.label}
            </button>
          </Show>
        </div>
        <div
          style={{
            "font-size": "12px",
            color: c().status === "error"
              ? "var(--error)"
              : c().status === "warning"
              ? "var(--warning)"
              : "var(--text-secondary)",
            "margin-top": "2px",
            "word-break": "break-word",
            "font-family": c().status === "ok" && c().detail.startsWith("/")
              ? "var(--font-mono)"
              : "var(--font-ui)",
          }}
        >
          {c().detail}
        </div>
      </div>
    </div>
  );
}

function CategorySection(props: {
  category: string;
  checks: HealthCheck[];
}): JSX.Element {
  return (
    <div style={{ "margin-bottom": "20px" }}>
      <div
        style={{
          "font-size": "10px",
          "font-weight": "600",
          "letter-spacing": "0.08em",
          "text-transform": "uppercase",
          color: "var(--text-muted)",
          "margin-bottom": "8px",
          padding: "0 2px",
        }}
      >
        {CATEGORY_LABEL[props.category] ?? props.category}
      </div>
      <div style={{ display: "flex", "flex-direction": "column", gap: "6px" }}>
        <For each={props.checks}>
          {(check) => <CheckRow check={check} />}
        </For>
      </div>
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

export function HealthPanel(): JSX.Element {
  onMount(() => {
    // Always re-run when the panel is opened — the last result may be stale
    // (e.g. the project wasn't open yet when checks first ran).
    if (!healthState.isRunning) {
      refreshHealthChecks();
    }
  });

  const byCategory = () => {
    const checks = healthChecks();
    const order: HealthCheck["category"][] = [
      "project",
      "environment",
      "system",
    ];
    return order
      .map((cat) => ({
        category: cat,
        checks: checks.filter((c) => c.category === cat),
      }))
      .filter((g) => g.checks.length > 0);
  };

  const summary = () => healthSummary();
  const overall = () => overallHealth();

  const overallColor = () => {
    const s = overall();
    if (s === "ok") return "var(--success)";
    if (s === "warning") return "var(--warning)";
    if (s === "error") return "var(--error)";
    return "var(--text-muted)";
  };

  const overallLabel = () => {
    const s = overall();
    if (s === "ok") return "All systems operational";
    if (s === "warning") return "Some checks need attention";
    if (s === "error") return "Action required";
    return "Checking…";
  };

  return (
    <Show when={healthOpen()}>
      {/* Backdrop */}
      <div
        onClick={closeHealthPanel}
        style={{
          position: "fixed",
          inset: "0",
          "z-index": "2000",
          background: "rgba(0,0,0,0.45)",
        }}
      />

      {/* Panel */}
      <div
        role="dialog"
        aria-modal="true"
        aria-label="IDE Health Center"
        style={{
          position: "fixed",
          top: "6%",
          left: "50%",
          transform: "translateX(-50%)",
          width: "min(640px, 92vw)",
          "max-height": "86vh",
          display: "flex",
          "flex-direction": "column",
          "z-index": "2001",
          background: "var(--bg-secondary)",
          border: "1px solid var(--border)",
          "border-radius": "8px",
          "box-shadow": "0 20px 60px rgba(0,0,0,0.5)",
          overflow: "hidden",
        }}
      >
        {/* Header */}
        <div
          style={{
            display: "flex",
            "align-items": "center",
            padding: "18px 20px 16px",
            "border-bottom": "1px solid var(--border)",
            "flex-shrink": "0",
            gap: "14px",
          }}
        >
          {/* Overall health badge */}
          <div
            style={{
              width: "36px",
              height: "36px",
              "border-radius": "50%",
              background: overallColor(),
              display: "flex",
              "align-items": "center",
              "justify-content": "center",
              "font-size": "16px",
              "flex-shrink": "0",
              color: "#fff",
              "font-weight": "700",
            }}
          >
            {overall() === "ok"
              ? "✓"
              : overall() === "warning"
              ? "⚠"
              : overall() === "error"
              ? "✗"
              : "⟳"}
          </div>

          <div style={{ flex: "1" }}>
            <div
              style={{
                "font-size": "15px",
                "font-weight": "600",
                color: "var(--text-primary)",
                display: "flex",
                "align-items": "center",
                gap: "8px",
              }}
            >
              IDE Health Center
              <span
                style={{
                  "font-size": "12px",
                  "font-weight": "400",
                  color: overallColor(),
                  background: `color-mix(in srgb, ${overallColor()} 12%, transparent)`,
                  padding: "1px 8px",
                  "border-radius": "10px",
                }}
              >
                {summary().ok}/{summary().total} checks passing
              </span>
            </div>
            <div
              style={{
                "font-size": "12px",
                color: "var(--text-secondary)",
                "margin-top": "2px",
              }}
            >
              {overallLabel()}
              <Show when={healthState.lastCheckedAt}>
                <span style={{ color: "var(--text-muted)", "margin-left": "6px" }}>
                  · Last checked{" "}
                  {healthState.lastCheckedAt?.toLocaleTimeString([], {
                    hour: "2-digit",
                    minute: "2-digit",
                    second: "2-digit",
                  })}
                </span>
              </Show>
            </div>
          </div>

          {/* Actions */}
          <div style={{ display: "flex", gap: "6px", "flex-shrink": "0" }}>
            <button
              onClick={refreshHealthChecks}
              disabled={healthState.isRunning}
              title="Re-run all checks"
              style={{
                display: "flex",
                "align-items": "center",
                gap: "4px",
                padding: "5px 12px",
                background: "var(--bg-tertiary)",
                border: "1px solid var(--border)",
                "border-radius": "5px",
                color: "var(--text-primary)",
                cursor: healthState.isRunning ? "not-allowed" : "pointer",
                "font-size": "12px",
                opacity: healthState.isRunning ? "0.6" : "1",
              }}
            >
              <span
                style={{
                  "display": "inline-block",
                  animation: healthState.isRunning ? "lsp-spin 1s linear infinite" : "none",
                }}
              >
                ↻
              </span>
              Refresh
            </button>
            <button
              onClick={closeHealthPanel}
              title="Close"
              style={{
                display: "flex",
                "align-items": "center",
                "justify-content": "center",
                width: "28px",
                height: "28px",
                background: "none",
                border: "none",
                "border-radius": "4px",
                color: "var(--text-muted)",
                cursor: "pointer",
                "font-size": "14px",
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.background = "var(--bg-hover)";
                e.currentTarget.style.color = "var(--text-primary)";
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.background = "none";
                e.currentTarget.style.color = "var(--text-muted)";
              }}
            >
              ✕
            </button>
          </div>
        </div>

        {/* Check list */}
        <div
          style={{
            flex: "1",
            "overflow-y": "auto",
            padding: "18px 20px",
          }}
        >
          <For each={byCategory()}>
            {(group) => (
              <CategorySection
                category={group.category}
                checks={group.checks}
              />
            )}
          </For>

          {/* Footer hint */}
          <div
            style={{
              "margin-top": "8px",
              "padding-top": "14px",
              "border-top": "1px solid var(--border)",
              "font-size": "11px",
              color: "var(--text-muted)",
              "text-align": "center",
            }}
          >
            Configure Android SDK and Java paths in{" "}
            <button
              onClick={() => {
                closeHealthPanel();
                import("@/components/settings/SettingsPanel").then(
                  ({ openSettings }) => openSettings()
                );
              }}
              style={{
                background: "none",
                border: "none",
                color: "var(--accent)",
                cursor: "pointer",
                "font-size": "11px",
                padding: "0",
                "text-decoration": "underline",
              }}
            >
              Settings → Tools
            </button>
            {"  ·  "}Logs saved to{" "}
            <code
              style={{
                "font-family": "var(--font-mono)",
                background: "var(--bg-tertiary)",
                padding: "0 4px",
                "border-radius": "3px",
              }}
            >
              ~/.androidide/
            </code>
          </div>
        </div>
      </div>
    </Show>
  );
}
