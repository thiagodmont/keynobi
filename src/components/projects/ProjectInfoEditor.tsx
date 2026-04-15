/**
 * ProjectInfoEditor.tsx
 *
 * Modal for viewing and editing versionName and versionCode
 * in the app-level build.gradle(.kts) of the active project.
 */

import { type JSX, createSignal, Show, createEffect } from "solid-js";
import { Portal } from "solid-js/web";
import { getProjectAppInfo, saveProjectAppInfo, formatError } from "@/lib/tauri-api";
import { projectState } from "@/stores/project.store";
import { showToast } from "@/components/ui";
import type { ProjectAppInfo } from "@/bindings";

// ── Module-level open/close signal ────────────────────────────────────────────

const [open, setOpen] = createSignal(false);

export function openProjectInfoEditor(): void {
  setOpen(true);
}

export function closeProjectInfoEditor(): void {
  setOpen(false);
}

// ── Component ─────────────────────────────────────────────────────────────────

export function ProjectInfoEditor(): JSX.Element {
  const [info, setInfo] = createSignal<ProjectAppInfo | null>(null);
  const [versionName, setVersionName] = createSignal("");
  const [versionCode, setVersionCode] = createSignal("");
  const [saving, setSaving] = createSignal(false);
  const [loading, setLoading] = createSignal(false);

  // Load app info whenever the modal opens.
  createEffect(() => {
    if (!open()) return;
    if (!projectState.projectRoot) return;

    setLoading(true);
    getProjectAppInfo()
      .then((data) => {
        setInfo(data);
        setVersionName(data.versionName ?? "");
        setVersionCode(data.versionCode !== null && data.versionCode !== undefined ? String(data.versionCode) : "");
      })
      .catch((err) => {
        showToast(`Failed to read app info: ${formatError(err)}`, "error");
        setOpen(false);
      })
      .finally(() => setLoading(false));
  });

  async function handleSave() {
    const name = versionName().trim();
    const codeStr = versionCode().trim();
    if (!name) {
      showToast("Version name cannot be empty.", "error");
      return;
    }
    const code = parseInt(codeStr, 10);
    if (isNaN(code) || code < 0) {
      showToast("Version code must be a non-negative integer.", "error");
      return;
    }

    setSaving(true);
    try {
      await saveProjectAppInfo(name, BigInt(code));
      showToast("App info saved successfully.", "success");
      setOpen(false);
    } catch (err) {
      showToast(`Failed to save app info: ${formatError(err)}`, "error");
    } finally {
      setSaving(false);
    }
  }

  const inputStyle = {
    width: "100%",
    padding: "6px 10px",
    background: "var(--bg-primary)",
    border: "1px solid var(--border)",
    "border-radius": "4px",
    color: "var(--text-primary)",
    "font-size": "13px",
    outline: "none",
    "box-sizing": "border-box",
  } as const;

  const labelStyle = {
    "font-size": "11px",
    color: "var(--text-muted)",
    "text-transform": "uppercase",
    "letter-spacing": "0.05em",
    "margin-bottom": "4px",
    display: "block",
  } as const;

  return (
    <Show when={open()}>
      <Portal>
        {/* Backdrop */}
        <div
          onClick={closeProjectInfoEditor}
          style={{
            position: "fixed",
            inset: "0",
            background: "rgba(0,0,0,0.5)",
            "z-index": "9000",
            display: "flex",
            "align-items": "center",
            "justify-content": "center",
          }}
        >
          {/* Dialog box */}
          <div
            onClick={(e) => e.stopPropagation()}
            style={{
              background: "var(--bg-tertiary)",
              border: "1px solid var(--border)",
              "border-radius": "8px",
              padding: "24px",
              width: "360px",
              "box-shadow": "0 8px 32px rgba(0,0,0,0.6)",
            }}
          >
            <h3
              style={{
                "font-size": "14px",
                "font-weight": "600",
                color: "var(--text-primary)",
                "margin-bottom": "16px",
              }}
            >
              App Info — {projectState.projectName}
            </h3>

            <Show when={loading()}>
              <div
                style={{
                  "font-size": "12px",
                  color: "var(--text-muted)",
                  "text-align": "center",
                  padding: "16px 0",
                }}
              >
                Loading…
              </div>
            </Show>

            <Show when={!loading()}>
              {/* Application ID (read-only) */}
              <div style={{ "margin-bottom": "14px" }}>
                <label style={labelStyle}>Application ID</label>
                <div
                  style={{
                    ...inputStyle,
                    color: "var(--text-muted)",
                    background: "var(--bg-secondary)",
                    cursor: "default",
                  }}
                >
                  {info()?.applicationId ?? "—"}
                </div>
              </div>

              {/* Version Name */}
              <div style={{ "margin-bottom": "14px" }}>
                <label style={labelStyle}>Version Name</label>
                <input
                  type="text"
                  placeholder="e.g. 1.0.0"
                  value={versionName()}
                  onInput={(e) => setVersionName(e.currentTarget.value)}
                  style={inputStyle}
                />
              </div>

              {/* Version Code */}
              <div style={{ "margin-bottom": "22px" }}>
                <label style={labelStyle}>Version Code</label>
                <input
                  type="number"
                  min="0"
                  step="1"
                  placeholder="e.g. 1"
                  value={versionCode()}
                  onInput={(e) => setVersionCode(e.currentTarget.value)}
                  style={inputStyle}
                />
              </div>

              {/* Buttons */}
              <div style={{ display: "flex", gap: "8px", "justify-content": "flex-end" }}>
                <button
                  onClick={closeProjectInfoEditor}
                  style={{
                    padding: "6px 16px",
                    "border-radius": "4px",
                    "font-size": "13px",
                    cursor: "pointer",
                    background: "transparent",
                    color: "var(--text-secondary)",
                    border: "1px solid var(--border)",
                  }}
                >
                  Cancel
                </button>
                <button
                  onClick={handleSave}
                  disabled={saving()}
                  style={{
                    padding: "6px 16px",
                    "border-radius": "4px",
                    "font-size": "13px",
                    cursor: saving() ? "not-allowed" : "pointer",
                    background: "var(--accent)",
                    color: "#fff",
                    border: "none",
                    opacity: saving() ? "0.6" : "1",
                  }}
                >
                  {saving() ? "Saving…" : "Save"}
                </button>
              </div>
            </Show>
          </div>
        </div>
      </Portal>
    </Show>
  );
}

export default ProjectInfoEditor;
