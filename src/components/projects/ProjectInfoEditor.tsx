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
import { Alert, modalFocus, showToast } from "@/components/ui";
import type { ProjectAppInfo } from "@/bindings";

/** The largest version code Google Play accepts. */
export const MAX_VERSION_CODE = 2_100_000_000;

const VERSION_CODE_RANGE_MESSAGE = `Version code must be a whole number from 1 to ${MAX_VERSION_CODE}.`;

// ── Module-level open/close signal ────────────────────────────────────────────

const [open, setOpen] = createSignal(false);

export function openProjectInfoEditor(): void {
  setOpen(true);
}

export function closeProjectInfoEditor(): void {
  setOpen(false);
}

/**
 * Validate the editor fields. A `null` field cannot be edited here (it is not a
 * single literal in the build file) and is left out of the save.
 */
export function validateProjectInfoInput(
  rawVersionName: string | null,
  rawVersionCode: string | null
):
  | { ok: true; versionName: string | null; versionCode: number | null }
  | { ok: false; message: string } {
  let versionName: string | null = null;
  if (rawVersionName !== null) {
    versionName = rawVersionName.trim();
    if (!versionName) {
      return { ok: false, message: "Version name cannot be empty." };
    }
    if (/["\\$\r\n]/.test(versionName)) {
      return {
        ok: false,
        message: "Version name cannot contain quotes, backslashes, '$', or line breaks.",
      };
    }
  }

  let versionCode: number | null = null;
  if (rawVersionCode !== null) {
    const versionCodeText = rawVersionCode.trim();
    if (!/^\d+$/.test(versionCodeText)) {
      return { ok: false, message: VERSION_CODE_RANGE_MESSAGE };
    }
    versionCode = Number(versionCodeText);
    if (versionCode < 1 || versionCode > MAX_VERSION_CODE) {
      return { ok: false, message: VERSION_CODE_RANGE_MESSAGE };
    }
  }

  if (versionName === null && versionCode === null) {
    return { ok: false, message: "Neither field can be edited here." };
  }
  return { ok: true, versionName, versionCode };
}

// ── Component ─────────────────────────────────────────────────────────────────

export function ProjectInfoEditor(): JSX.Element {
  const [info, setInfo] = createSignal<ProjectAppInfo | null>(null);
  const [versionName, setVersionName] = createSignal("");
  const [versionCode, setVersionCode] = createSignal("");
  const [saving, setSaving] = createSignal(false);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const nameEditable = () => !info()?.versionNameUnavailable;
  const codeEditable = () => !info()?.versionCodeUnavailable;

  // Load app info whenever the modal opens.
  createEffect(() => {
    if (!open()) return;
    if (!projectState.projectRoot) return;

    setInfo(null);
    setError(null);
    setLoading(true);
    getProjectAppInfo()
      .then((data) => {
        setInfo(data);
        setVersionName(data.versionName ?? "");
        setVersionCode(
          data.versionCode !== null && data.versionCode !== undefined
            ? String(data.versionCode)
            : ""
        );
      })
      .catch((err) => {
        showToast(`Failed to read app info: ${formatError(err)}`, "error");
        setOpen(false);
      })
      .finally(() => setLoading(false));
  });

  async function handleSave() {
    const validation = validateProjectInfoInput(
      nameEditable() ? versionName() : null,
      codeEditable() ? versionCode() : null
    );
    if (!validation.ok) {
      setError(validation.message);
      return;
    }

    setError(null);
    setSaving(true);
    try {
      await saveProjectAppInfo(validation.versionName, validation.versionCode);
      showToast("App info saved successfully.", "success");
      setOpen(false);
    } catch (err) {
      setError(`Failed to save app info: ${formatError(err)}`);
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

  const readOnlyStyle = {
    ...inputStyle,
    color: "var(--text-muted)",
    background: "var(--bg-secondary)",
    cursor: "default",
  } as const;

  const noteStyle = {
    "font-size": "11px",
    color: "var(--text-muted)",
    "margin-top": "4px",
    "line-height": "1.4",
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
            ref={(el) => modalFocus(el, { onEscape: closeProjectInfoEditor })}
            role="dialog"
            aria-modal="true"
            aria-label="Project App Info"
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
                <div style={readOnlyStyle}>{info()?.applicationId ?? "—"}</div>
              </div>

              {/* Version Name */}
              <div style={{ "margin-bottom": "14px" }}>
                <label style={labelStyle} for="project-info-version-name">
                  Version Name
                </label>
                <Show
                  when={nameEditable()}
                  fallback={
                    <>
                      <div style={readOnlyStyle}>—</div>
                      <div style={noteStyle}>{info()?.versionNameUnavailable}</div>
                    </>
                  }
                >
                  <input
                    id="project-info-version-name"
                    type="text"
                    placeholder="e.g. 1.0.0"
                    value={versionName()}
                    onInput={(e) => {
                      setVersionName(e.currentTarget.value);
                      setError(null);
                    }}
                    style={inputStyle}
                  />
                </Show>
              </div>

              {/* Version Code */}
              <div style={{ "margin-bottom": "22px" }}>
                <label style={labelStyle} for="project-info-version-code">
                  Version Code
                </label>
                <Show
                  when={codeEditable()}
                  fallback={
                    <>
                      <div style={readOnlyStyle}>—</div>
                      <div style={noteStyle}>{info()?.versionCodeUnavailable}</div>
                    </>
                  }
                >
                  <input
                    id="project-info-version-code"
                    type="number"
                    min="1"
                    max={MAX_VERSION_CODE}
                    step="1"
                    placeholder="e.g. 1"
                    value={versionCode()}
                    onInput={(e) => {
                      setVersionCode(e.currentTarget.value);
                      setError(null);
                    }}
                    style={inputStyle}
                  />
                </Show>
              </div>

              <Show when={error()}>
                {(message) => (
                  <div style={{ "margin-bottom": "14px" }}>
                    <Alert variant="error">{message()}</Alert>
                  </div>
                )}
              </Show>

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
                  disabled={saving() || (!nameEditable() && !codeEditable())}
                  style={{
                    padding: "6px 16px",
                    "border-radius": "4px",
                    "font-size": "13px",
                    cursor: saving() ? "not-allowed" : "pointer",
                    background: "var(--accent)",
                    color: "#fff",
                    border: "none",
                    opacity: saving() || (!nameEditable() && !codeEditable()) ? "0.6" : "1",
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
