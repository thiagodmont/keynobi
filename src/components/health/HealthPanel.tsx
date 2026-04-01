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
import { mcpState } from "@/stores/mcp.store";
import {
  getMcpSetupStatus,
  configureMcpInClaude,
  type McpSetupStatus,
} from "@/lib/tauri-api";

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

// ── Studio Setup Section ──────────────────────────────────────────────────────

function StudioSetupSection(): JSX.Element {
  const report = () => healthState.systemReport;
  const studioFound = () => report()?.studioCommandFound;

  const pathSnippet = `export PATH="$PATH:/Applications/Android Studio.app/Contents/MacOS"`;

  const handleCopy = async () => {
    await navigator.clipboard.writeText(pathSnippet).catch(() => {});
  };

  const dot = (color: string) => (
    <span
      style={{
        width: "8px",
        height: "8px",
        "border-radius": "50%",
        background: color,
        "flex-shrink": "0",
        display: "inline-block",
      }}
    />
  );

  return (
    <div
      style={{
        "margin-top": "8px",
        "margin-bottom": "16px",
        padding: "14px",
        background: "var(--bg-tertiary)",
        "border-radius": "6px",
        border: "1px solid var(--border)",
      }}
    >
      {/* Header */}
      <div
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
          "margin-bottom": "10px",
        }}
      >
        <div
          style={{
            "font-size": "10px",
            "font-weight": "600",
            "letter-spacing": "0.08em",
            "text-transform": "uppercase",
            color: "var(--text-muted)",
          }}
        >
          Android Studio CLI
        </div>
        <Show when={studioFound() === true}>
          <div
            style={{
              display: "flex",
              "align-items": "center",
              gap: "5px",
              background: "rgba(74,222,128,0.12)",
              border: "1px solid rgba(74,222,128,0.3)",
              "border-radius": "10px",
              padding: "2px 8px",
            }}
          >
            {dot("var(--success)")}
            <span style={{ "font-size": "11px", color: "var(--success)" }}>
              studio command found
            </span>
          </div>
        </Show>
      </div>

      {/* Already configured */}
      <Show when={studioFound() === true}>
        <div style={{ "font-size": "12px", color: "var(--text-secondary)", "line-height": "1.5" }}>
          The <code style={{ "font-family": "var(--font-mono)" }}>studio</code> command is on your
          PATH. Stack frame lines in crash logs have a jump-to-line button.
        </div>
      </Show>

      {/* Not found */}
      <Show when={studioFound() === false}>
        <div
          style={{
            display: "flex",
            "align-items": "flex-start",
            gap: "10px",
            padding: "10px",
            background: "rgba(251,191,36,0.08)",
            border: "1px solid rgba(251,191,36,0.25)",
            "border-radius": "5px",
            "margin-bottom": "12px",
          }}
        >
          <span style={{ "font-size": "14px" }}>⚠</span>
          <div>
            <div
              style={{
                "font-size": "12px",
                color: "var(--text-primary)",
                "font-weight": "500",
                "margin-bottom": "3px",
              }}
            >
              studio command not found
            </div>
            <div
              style={{
                "font-size": "11px",
                color: "var(--text-secondary)",
                "line-height": "1.6",
              }}
            >
              Add{" "}
              <code style={{ "font-family": "var(--font-mono)" }}>
                /Applications/Android Studio.app/Contents/MacOS
              </code>{" "}
              to your{" "}
              <code style={{ "font-family": "var(--font-mono)" }}>$PATH</code>{" "}
              and use{" "}
              <code style={{ "font-family": "var(--font-mono)" }}>studio</code>{" "}
              to run commands. Once added, crash stack frames in Logcat will show a
              jump-to-line button.
            </div>
          </div>
        </div>

        {/* Setup instructions */}
        <div style={{ "font-size": "11px", color: "var(--text-muted)", "margin-bottom": "6px" }}>
          Add to your shell profile (
          <code style={{ "font-family": "var(--font-mono)" }}>.zshrc</code> /{" "}
          <code style={{ "font-family": "var(--font-mono)" }}>.bash_profile</code>
          ):
        </div>
        <div
          style={{
            display: "flex",
            "align-items": "center",
            gap: "8px",
          }}
        >
          <code
            style={{
              flex: "1",
              display: "block",
              "font-family": "var(--font-mono)",
              "font-size": "11px",
              background: "var(--bg-secondary)",
              border: "1px solid var(--border)",
              "border-radius": "4px",
              padding: "8px 10px",
              color: "var(--text-primary)",
              "overflow-x": "auto",
              "white-space": "nowrap",
            }}
          >
            {pathSnippet}
          </code>
          <button
            onClick={handleCopy}
            title="Copy to clipboard"
            style={{
              background: "none",
              border: "1px solid var(--border)",
              color: "var(--text-muted)",
              "border-radius": "3px",
              padding: "2px 8px",
              "font-size": "10px",
              cursor: "pointer",
              "flex-shrink": "0",
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.color = "var(--text-primary)";
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.color = "var(--text-muted)";
            }}
          >
            Copy
          </button>
        </div>
        <div
          style={{
            "font-size": "11px",
            color: "var(--text-muted)",
            "margin-top": "8px",
            "line-height": "1.5",
          }}
        >
          After editing the file run{" "}
          <code style={{ "font-family": "var(--font-mono)" }}>source ~/.zshrc</code>{" "}
          (or restart your terminal), then click{" "}
          <strong>Refresh</strong> above to re-run health checks.
        </div>
      </Show>

      {/* Loading state */}
      <Show when={studioFound() === undefined}>
        <div style={{ "font-size": "12px", color: "var(--text-muted)", padding: "4px 0" }}>
          Checking for studio command…
        </div>
      </Show>
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

// ── MCP Setup Section ─────────────────────────────────────────────────────────

function McpSetupSection(): JSX.Element {
  const [status, setStatus] = createSignal<McpSetupStatus | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [configuring, setConfiguring] = createSignal(false);
  const [configResult, setConfigResult] = createSignal<{ ok: boolean; msg: string } | null>(null);

  onMount(async () => {
    try {
      const s = await getMcpSetupStatus();
      setStatus(s);
    } catch {
      // Non-fatal: show fallback UI
    } finally {
      setLoading(false);
    }
  });

  const handleConfigure = async () => {
    setConfiguring(true);
    setConfigResult(null);
    try {
      const msg = await configureMcpInClaude();
      setConfigResult({ ok: true, msg });
      // Refresh status after successful configuration
      const s = await getMcpSetupStatus();
      setStatus(s);
    } catch (e: unknown) {
      setConfigResult({ ok: false, msg: String(e) });
    } finally {
      setConfiguring(false);
    }
  };

  const handleCopy = async () => {
    const s = status();
    if (!s) return;
    const cmd = `claude mcp add android-companion --command "${s.setupCommand}"`;
    await navigator.clipboard.writeText(cmd).catch(() => {});
  };

  const runningDot = (color: string) => (
    <span
      style={{
        width: "8px",
        height: "8px",
        "border-radius": "50%",
        background: color,
        "flex-shrink": "0",
        display: "inline-block",
      }}
    />
  );

  return (
    <div
      style={{
        "margin-top": "8px",
        "margin-bottom": "16px",
        padding: "14px",
        background: "var(--bg-tertiary)",
        "border-radius": "6px",
        border: "1px solid var(--border)",
      }}
    >
      {/* Header */}
      <div
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
          "margin-bottom": "10px",
        }}
      >
        <div
          style={{
            "font-size": "10px",
            "font-weight": "600",
            "letter-spacing": "0.08em",
            "text-transform": "uppercase",
            color: "var(--text-muted)",
          }}
        >
          Claude Code Integration (MCP)
        </div>

        {/* Live server connection badge */}
        <Show when={mcpState.running}>
          <div
            style={{
              display: "flex",
              "align-items": "center",
              gap: "5px",
              background: "rgba(74,222,128,0.12)",
              border: "1px solid rgba(74,222,128,0.3)",
              "border-radius": "10px",
              padding: "2px 8px",
            }}
          >
            {runningDot("var(--success)")}
            <span style={{ "font-size": "11px", color: "var(--success)" }}>
              {mcpState.clientName ? mcpState.clientName : "Connected"}
            </span>
          </div>
        </Show>
      </div>

      {/* Loading state */}
      <Show when={loading()}>
        <div style={{ "font-size": "12px", color: "var(--text-muted)", padding: "4px 0" }}>
          Checking Claude Code installation…
        </div>
      </Show>

      {/* Loaded state */}
      <Show when={!loading()}>
        {/* State 1: Claude not found */}
        <Show when={status() && !status()!.claudeFound}>
          <div
            style={{
              display: "flex",
              "align-items": "flex-start",
              gap: "10px",
              padding: "10px",
              background: "rgba(251,191,36,0.08)",
              border: "1px solid rgba(251,191,36,0.25)",
              "border-radius": "5px",
              "margin-bottom": "10px",
            }}
          >
            <span style={{ "font-size": "14px" }}>⚠</span>
            <div>
              <div style={{ "font-size": "12px", color: "var(--text-primary)", "font-weight": "500", "margin-bottom": "3px" }}>
                Claude Code CLI not found
              </div>
              <div style={{ "font-size": "11px", color: "var(--text-secondary)", "line-height": "1.5" }}>
                Install Claude Code to enable one-click setup. The{" "}
                <code style={{ "font-family": "var(--font-mono)" }}>claude</code> CLI must be accessible.
              </div>
            </div>
          </div>
          <a
            href="https://claude.ai/download"
            target="_blank"
            rel="noopener"
            style={{
              display: "inline-flex",
              "align-items": "center",
              gap: "6px",
              padding: "6px 14px",
              background: "var(--accent)",
              color: "#fff",
              "border-radius": "5px",
              "font-size": "12px",
              "font-weight": "500",
              "text-decoration": "none",
              cursor: "pointer",
              "margin-bottom": "12px",
            }}
          >
            Download Claude Code →
          </a>
        </Show>

        {/* State 2: Already configured */}
        <Show when={status()?.isConfigured}>
          <div
            style={{
              display: "flex",
              "align-items": "center",
              gap: "8px",
              "margin-bottom": "10px",
            }}
          >
            {runningDot("var(--success)")}
            <span style={{ "font-size": "13px", color: "var(--text-primary)", "font-weight": "500" }}>
              Registered in Claude Code
            </span>
          </div>
          <Show when={status()?.configuredCommand}>
            <div style={{ "font-size": "11px", color: "var(--text-secondary)", "margin-bottom": "8px" }}>
              Currently configured command:
            </div>
            <code
              style={{
                display: "block",
                "font-family": "var(--font-mono)",
                "font-size": "11px",
                background: "var(--bg-secondary)",
                border: "1px solid var(--border)",
                "border-radius": "4px",
                padding: "8px 10px",
                color: "var(--text-primary)",
                "overflow-x": "auto",
                "white-space": "nowrap",
                "margin-bottom": "10px",
              }}
            >
              {status()!.configuredCommand}
            </code>
          </Show>
          {/* Reconfigure button (e.g. if the app was moved) */}
          <button
            onClick={handleConfigure}
            disabled={configuring()}
            style={{
              padding: "5px 12px",
              background: "transparent",
              border: "1px solid var(--border)",
              "border-radius": "4px",
              color: "var(--text-secondary)",
              cursor: configuring() ? "not-allowed" : "pointer",
              "font-size": "11px",
              opacity: configuring() ? "0.6" : "1",
            }}
          >
            {configuring() ? "Updating…" : "Reconfigure (if app was moved)"}
          </button>
        </Show>

        {/* State 3: Claude found but not configured */}
        <Show when={status()?.claudeFound && !status()?.isConfigured}>
          <div style={{ "font-size": "12px", color: "var(--text-secondary)", "margin-bottom": "12px", "line-height": "1.5" }}>
            Claude Code is installed but this app is not registered yet. Click below to configure it automatically.
          </div>

          {/* One-click configure button */}
          <button
            onClick={handleConfigure}
            disabled={configuring()}
            style={{
              display: "inline-flex",
              "align-items": "center",
              gap: "7px",
              padding: "8px 16px",
              background: configuring() ? "var(--bg-active)" : "var(--accent)",
              color: "#fff",
              border: "none",
              "border-radius": "5px",
              cursor: configuring() ? "not-allowed" : "pointer",
              "font-size": "13px",
              "font-weight": "500",
              opacity: configuring() ? "0.7" : "1",
              transition: "opacity 0.15s",
              "margin-bottom": "12px",
            }}
          >
            <Show when={configuring()}>
              <span style={{ "font-size": "12px" }}>⟳</span>
            </Show>
            {configuring() ? "Configuring…" : "Configure in Claude Code"}
          </button>
        </Show>

        {/* Fallback: no status loaded (first-run or error) — always show manual command */}
        <Show when={!status()}>
          <div style={{ "font-size": "12px", color: "var(--text-secondary)", "margin-bottom": "8px" }}>
            Run this command in your terminal to register the MCP server:
          </div>
        </Show>

        {/* Result feedback */}
        <Show when={configResult()}>
          <div
            style={{
              padding: "8px 12px",
              "border-radius": "4px",
              background: configResult()!.ok ? "rgba(74,222,128,0.1)" : "rgba(248,113,113,0.1)",
              border: `1px solid ${configResult()!.ok ? "rgba(74,222,128,0.3)" : "rgba(248,113,113,0.3)"}`,
              "font-size": "12px",
              color: configResult()!.ok ? "var(--success)" : "var(--error)",
              "margin-bottom": "10px",
              "line-height": "1.5",
              "word-break": "break-word",
            }}
          >
            {configResult()!.ok ? "✓ " : "✗ "}
            {configResult()!.msg}
          </div>
        </Show>

        {/* Always show the manual command as a copy-able fallback */}
        <div style={{ "margin-top": status()?.isConfigured ? "14px" : "4px", "border-top": status()?.isConfigured ? "1px solid var(--border)" : "none", "padding-top": status()?.isConfigured ? "12px" : "0" }}>
          <div style={{ display: "flex", "align-items": "center", "justify-content": "space-between", "margin-bottom": "6px" }}>
            <span style={{ "font-size": "11px", color: "var(--text-muted)" }}>
              Manual setup command:
            </span>
            <button
              onClick={handleCopy}
              style={{
                background: "none",
                border: "1px solid var(--border)",
                color: "var(--text-muted)",
                "border-radius": "3px",
                padding: "1px 8px",
                "font-size": "10px",
                cursor: "pointer",
              }}
              onMouseEnter={(e) => { e.currentTarget.style.color = "var(--text-primary)"; }}
              onMouseLeave={(e) => { e.currentTarget.style.color = "var(--text-muted)"; }}
            >
              Copy
            </button>
          </div>
          <code
            style={{
              display: "block",
              "font-family": "var(--font-mono)",
              "font-size": "11px",
              background: "var(--bg-secondary)",
              border: "1px solid var(--border)",
              "border-radius": "4px",
              padding: "8px 10px",
              color: "var(--text-primary)",
              "overflow-x": "auto",
              "white-space": "nowrap",
            }}
          >
            {status()
              ? `claude mcp add android-companion --command "${status()!.setupCommand}"`
              : `claude mcp add android-companion --command "/Applications/AndroidDevCompanion.app/Contents/MacOS/android-dev-companion --mcp"`}
          </code>
          <div style={{ "font-size": "11px", color: "var(--text-muted)", "margin-top": "6px" }}>
            Optionally append{" "}
            <code style={{ "font-family": "var(--font-mono)" }}>--project /path/to/project</code>
            {" "}to scope to a specific project.
          </div>
        </div>
      </Show>
    </div>
  );
}

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

          {/* MCP Server section */}
          <McpSetupSection />

          {/* Android Studio CLI section */}
          <StudioSetupSection />

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
