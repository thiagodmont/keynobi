import { type JSX, For, Show, createMemo } from "solid-js";
import { lspState, type Diagnostic } from "@/stores/lsp.store";
import { projectState } from "@/stores/project.store";
import Icon from "@/components/common/Icon";

interface GroupedDiagnostics {
  path: string;
  relativePath: string;
  diagnostics: Diagnostic[];
}

export function ProblemsPanel(): JSX.Element {
  const grouped = createMemo((): GroupedDiagnostics[] => {
    const root = projectState.projectRoot ?? "";
    const groups: GroupedDiagnostics[] = [];

    for (const [path, diags] of Object.entries(lspState.diagnostics)) {
      if (diags.length === 0) continue;

      const sorted = [...diags].sort((a, b) => {
        const severityOrder = { error: 0, warning: 1, information: 2, hint: 3 };
        const sa = severityOrder[a.severity] ?? 4;
        const sb = severityOrder[b.severity] ?? 4;
        if (sa !== sb) return sa - sb;
        return a.range.startLine - b.range.startLine;
      });

      groups.push({
        path,
        relativePath: root && path.startsWith(root)
          ? path.slice(root.length + 1)
          : path,
        diagnostics: sorted,
      });
    }

    groups.sort((a, b) => {
      const aHasError = a.diagnostics.some((d) => d.severity === "error");
      const bHasError = b.diagnostics.some((d) => d.severity === "error");
      if (aHasError !== bHasError) return aHasError ? -1 : 1;
      return a.relativePath.localeCompare(b.relativePath);
    });

    return groups;
  });

  return (
    <div style={{ flex: "1", overflow: "auto", "font-size": "12px" }}>
      <Show
        when={grouped().length > 0}
        fallback={
          <div style={{ padding: "12px", color: "var(--text-muted)", "text-align": "center" }}>
            No problems detected
          </div>
        }
      >
        <For each={grouped()}>
          {(group) => (
            <div>
              <div
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "6px",
                  padding: "2px 8px",
                  background: "var(--bg-tertiary)",
                  "font-weight": "500",
                  color: "var(--text-primary)",
                }}
              >
                <Icon name="file" size={13} color="var(--text-muted)" />
                <span style={{ flex: "1", overflow: "hidden", "text-overflow": "ellipsis", "white-space": "nowrap" }}>
                  {group.relativePath}
                </span>
                <span style={{ "font-size": "10px", color: "var(--text-muted)" }}>
                  {group.diagnostics.length}
                </span>
              </div>
              <For each={group.diagnostics}>
                {(diag) => <DiagnosticRow diag={diag} />}
              </For>
            </div>
          )}
        </For>
      </Show>
    </div>
  );
}

function DiagnosticRow(props: { diag: Diagnostic }): JSX.Element {
  async function handleClick() {
    const { openFileAtLocation } = await import("@/services/project.service");
    openFileAtLocation(
      props.diag.path,
      props.diag.range.startLine + 1,
      props.diag.range.startCol
    );
  }

  const iconName = () => {
    switch (props.diag.severity) {
      case "error": return "error-circle";
      case "warning": return "warning";
      default: return "lightbulb";
    }
  };

  const iconColor = () => {
    switch (props.diag.severity) {
      case "error": return "#f87171";
      case "warning": return "#fbbf24";
      case "information": return "#60a5fa";
      default: return "var(--text-muted)";
    }
  };

  return (
    <div
      role="button"
      tabindex="0"
      onClick={handleClick}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          handleClick();
        }
      }}
      style={{
        display: "flex",
        "align-items": "flex-start",
        gap: "6px",
        padding: "2px 8px 2px 24px",
        cursor: "pointer",
        color: "var(--text-secondary)",
        "line-height": "18px",
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.background = "var(--bg-hover)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.background = "transparent";
      }}
    >
      <Icon name={iconName()} size={13} color={iconColor()} />
      <span style={{ flex: "1", overflow: "hidden", "text-overflow": "ellipsis" }}>
        {props.diag.message}
      </span>
      <span style={{ "font-size": "10px", color: "var(--text-muted)", "flex-shrink": "0" }}>
        [{props.diag.range.startLine + 1}:{props.diag.range.startCol + 1}]
      </span>
    </div>
  );
}

export default ProblemsPanel;
