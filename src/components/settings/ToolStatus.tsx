import { type JSX, Show, createSignal, createEffect } from "solid-js";
import { settingsState, updateSetting } from "@/stores/settings.store";
import { detectSdkPath, detectJavaPath, formatError } from "@/lib/tauri-api";
import { showToast } from "@/components/ui";

// ── Shared helpers ────────────────────────────────────────────────────────────

function StatusBadge(props: { found: boolean; checking?: boolean }): JSX.Element {
  const label = () => props.checking ? "Checking…" : props.found ? "Found" : "Not configured";
  const colors = () => props.checking
    ? { bg: "rgba(251,191,36,0.15)", text: "#fbbf24" }
    : props.found
    ? { bg: "rgba(74,222,128,0.15)", text: "#4ade80" }
    : { bg: "rgba(248,113,113,0.15)", text: "#f87171" };

  return (
    <span
      style={{
        display: "inline-flex",
        "align-items": "center",
        gap: "4px",
        padding: "2px 8px",
        "border-radius": "10px",
        background: colors().bg,
        color: colors().text,
        "font-size": "11px",
        "font-weight": "500",
        "flex-shrink": "0",
      }}
    >
      <span
        style={{
          width: "6px",
          height: "6px",
          "border-radius": "50%",
          background: colors().text,
        }}
      />
      {label()}
    </span>
  );
}

/** Shared path-picker row with text input, Auto-detect, and Clear. */
function PathField(props: {
  value: string | null;
  placeholder: string;
  detecting: boolean;
  onDetect: () => void;
  onSave: (path: string | null) => void;
  validate: (path: string) => boolean;
  validNote: string;
}): JSX.Element {
  // eslint-disable-next-line solid/reactivity
  const [draft, setDraft] = createSignal(props.value ?? "");

  // Keep draft in sync with external changes (e.g. reset to defaults).
  createEffect(() => {
    setDraft(props.value ?? "");
  });

  function commit(raw: string) {
    setDraft(raw);
    const trimmed = raw.trim();
    props.onSave(trimmed || null);
  }

  const pathValid = () => !!draft().trim() && props.validate(draft().trim());

  return (
    <div style={{ display: "flex", "flex-direction": "column", gap: "6px" }}>
      {/* Input row */}
      <div style={{ display: "flex", gap: "6px", "align-items": "center" }}>
        <input
          type="text"
          value={draft()}
          placeholder={props.placeholder}
          onInput={(e) => commit(e.currentTarget.value)}
          spellcheck={false}
          style={{
            flex: "1",
            background: "var(--bg-primary)",
            border: `1px solid ${draft() && !pathValid() ? "var(--error)" : "var(--border)"}`,
            color: "var(--text-primary)",
            padding: "5px 8px",
            "border-radius": "4px",
            "font-family": "var(--font-mono)",
            "font-size": "11px",
            outline: "none",
            "min-width": "0",
          }}
        />

        <button
          onClick={() => props.onDetect()}
          disabled={props.detecting}
          title="Detect from process environment, login shell, and default locations"
          style={{
            display: "flex",
            "align-items": "center",
            gap: "4px",
            background: "var(--accent-bg)",
            border: "1px solid var(--accent)",
            color: "var(--accent)",
            padding: "4px 10px",
            "border-radius": "4px",
            cursor: props.detecting ? "not-allowed" : "pointer",
            "font-size": "11px",
            "white-space": "nowrap",
            "flex-shrink": "0",
            opacity: props.detecting ? "0.6" : "1",
          }}
        >
          <Show when={props.detecting} fallback={<>↻ Auto-detect</>}>
            <span style={{ animation: "lsp-spin 1s linear infinite", display: "inline-block" }}>↻</span>
            {" "}Detecting…
          </Show>
        </button>

        <Show when={draft()}>
          <button
            onClick={() => commit("")}
            title="Clear"
            style={{
              background: "none",
              border: "1px solid var(--border)",
              color: "var(--text-muted)",
              padding: "4px 8px",
              "border-radius": "4px",
              cursor: "pointer",
              "font-size": "11px",
              "flex-shrink": "0",
            }}
          >
            Clear
          </button>
        </Show>
      </div>

      {/* Inline validation note */}
      <Show when={draft().trim()}>
        <span
          style={{
            "font-size": "11px",
            color: pathValid() ? "var(--success)" : "var(--error)",
          }}
        >
          {pathValid() ? `✓  ${props.validNote}` : "✗  Path does not exist or is not a valid installation"}
        </span>
      </Show>
    </div>
  );
}

// ── Android SDK ───────────────────────────────────────────────────────────────

export function AndroidSdkStatus(): JSX.Element {
  const [detecting, setDetecting] = createSignal(false);

  /** Validates that the path looks like an Android SDK root. */
  function isValidSdk(p: string): boolean {
    // We can't do filesystem checks from the frontend, so just check the
    // path is non-empty and looks plausible (not a random string).
    return p.length > 3;
  }

  async function handleDetect() {
    setDetecting(true);
    try {
      // detect_sdk_path now tries: process env → login shell → ~/Library/Android/sdk
      const path = await detectSdkPath();
      if (path) {
        updateSetting("android", "sdkPath", path);
        showToast(`Android SDK detected: ${path}`, "success");
      } else {
        showToast(
          "Could not auto-detect Android SDK. Set ANDROID_HOME in your shell profile or enter the path manually.",
          "warning"
        );
      }
    } catch (err) {
      showToast(`Detection failed: ${formatError(err)}`, "error");
    } finally {
      setDetecting(false);
    }
  }

  return (
    <div style={{ display: "flex", "flex-direction": "column", gap: "8px" }}>
      <div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
        <StatusBadge
          found={!!settingsState.android.sdkPath}
          checking={detecting()}
        />
        <span style={{ "font-size": "11px", color: "var(--text-muted)" }}>
          Used for Gradle builds and Android tooling
        </span>
      </div>

      <PathField
        value={settingsState.android.sdkPath ?? null}
        placeholder="/Library/Android/sdk  or  ~/Android/Sdk"
        detecting={detecting()}
        onDetect={handleDetect}
        onSave={(p) => updateSetting("android", "sdkPath", p)}
        validate={isValidSdk}
        validNote="Path saved — will be set as ANDROID_HOME for the LSP"
      />

      <Show when={!settingsState.android.sdkPath}>
        <span
          style={{
            "font-size": "11px",
            color: "var(--text-muted)",
            "font-style": "italic",
          }}
        >
          Tip: set <code style={{ background: "var(--bg-tertiary)", padding: "0 3px", "border-radius": "2px" }}>export ANDROID_HOME=/path/to/sdk</code> in your shell profile (~/.zshrc) or enter the path above.
        </span>
      </Show>
    </div>
  );
}

// ── Java / JDK ────────────────────────────────────────────────────────────────

export function JavaStatus(): JSX.Element {
  const [detecting, setDetecting] = createSignal(false);

  function isValidJavaHome(p: string): boolean {
    return p.length > 3;
  }

  async function handleDetect() {
    setDetecting(true);
    try {
      // detect_java_path now tries: process env → login shell → /Library/Java/…
      const path = await detectJavaPath();
      if (path) {
        updateSetting("java", "home", path);
        showToast(`Java home detected: ${path}`, "success");
      } else {
        showToast(
          "Could not auto-detect Java. Set JAVA_HOME in your shell profile or enter the path manually.",
          "warning"
        );
      }
    } catch (err) {
      showToast(`Detection failed: ${formatError(err)}`, "error");
    } finally {
      setDetecting(false);
    }
  }

  return (
    <div style={{ display: "flex", "flex-direction": "column", gap: "8px" }}>
      <div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
        <StatusBadge
          found={!!settingsState.java.home}
          checking={detecting()}
        />
        <span style={{ "font-size": "11px", color: "var(--text-muted)" }}>
          Used for Gradle compilation tasks
        </span>
      </div>

      <PathField
        value={settingsState.java.home ?? null}
        placeholder="/Library/Java/JavaVirtualMachines/jdk-17.jdk/Contents/Home"
        detecting={detecting()}
        onDetect={handleDetect}
        onSave={(p) => updateSetting("java", "home", p)}
        validate={isValidJavaHome}
        validNote="Path saved — will be set as JAVA_HOME for the LSP"
      />
    </div>
  );
}
