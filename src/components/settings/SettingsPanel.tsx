import { type JSX, Show, For, createSignal, createResource } from "solid-js";
import { getVersion } from "@tauri-apps/api/app";
import { settingsState, updateSetting, resetSettings } from "@/stores/settings.store";
import {
  SettingRow,
  SettingToggle,
  SettingNumberInput,
} from "@/components/settings/SettingRow";
import { AndroidSdkStatus, JavaStatus } from "@/components/settings/ToolStatus";
import Icon from "@/components/common/Icon";

type Category = "user" | "tools" | "advanced";

const [settingsOpen, setSettingsOpen] = createSignal(false);

export function openSettings() {
  setSettingsOpen(true);
}

export function closeSettings() {
  setSettingsOpen(false);
}

export function SettingsPanel(): JSX.Element {
  const [category, setCategory] = createSignal<Category>("user");
  const [search, setSearch] = createSignal("");
  const [appVersion] = createResource(getVersion);

  let searchRef!: HTMLInputElement;

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      closeSettings();
    }
  }

  const categories: { id: Category; label: string; icon: string }[] = [
    { id: "user", label: "User", icon: "pencil" },
    { id: "tools", label: "Tools", icon: "terminal" },
    { id: "advanced", label: "Advanced", icon: "gear" },
  ];

  const matchesSearch = (label: string, desc?: string): boolean => {
    const q = search().toLowerCase();
    if (!q) return true;
    return label.toLowerCase().includes(q) || (desc?.toLowerCase().includes(q) ?? false);
  };

  return (
    <Show when={settingsOpen()}>
      {/* Backdrop */}
      <div
        onClick={() => closeSettings()}
        style={{
          position: "fixed",
          inset: "0",
          "z-index": "2000",
          background: "rgba(0,0,0,0.4)",
        }}
      />
      {/* Panel */}
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        onKeyDown={handleKeyDown}
        style={{
          position: "fixed",
          top: "5%",
          left: "50%",
          transform: "translateX(-50%)",
          width: "min(800px, 90vw)",
          height: "min(600px, 85vh)",
          background: "var(--bg-secondary)",
          border: "1px solid var(--border)",
          "border-radius": "10px",
          "box-shadow": "0 12px 48px rgba(0,0,0,0.5)",
          "z-index": "2001",
          display: "flex",
          "flex-direction": "column",
          overflow: "hidden",
        }}
      >
        {/* Header */}
        <div
          style={{
            display: "flex",
            "align-items": "center",
            "justify-content": "space-between",
            padding: "12px 16px",
            "border-bottom": "1px solid var(--border)",
            "flex-shrink": "0",
          }}
        >
          <div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
            <Icon name="gear" size={16} color="var(--text-secondary)" />
            <span
              style={{ "font-size": "14px", "font-weight": "600", color: "var(--text-primary)" }}
            >
              Settings
            </span>
          </div>
          <div style={{ display: "flex", gap: "8px" }}>
            <button
              onClick={async () => {
                await resetSettings();
              }}
              style={{
                background: "none",
                border: "1px solid var(--border)",
                color: "var(--text-muted)",
                padding: "3px 10px",
                "border-radius": "4px",
                cursor: "pointer",
                "font-size": "11px",
              }}
            >
              Reset to Defaults
            </button>
            <button
              onClick={() => closeSettings()}
              style={{
                background: "none",
                border: "none",
                color: "var(--text-muted)",
                cursor: "pointer",
                padding: "4px",
              }}
            >
              <Icon name="close" size={16} />
            </button>
          </div>
        </div>

        {/* Search */}
        <div
          style={{
            padding: "8px 16px",
            "border-bottom": "1px solid var(--border)",
            "flex-shrink": "0",
          }}
        >
          <input
            ref={searchRef}
            type="text"
            placeholder="Search settings..."
            value={search()}
            onInput={(e) => setSearch(e.currentTarget.value)}
            style={{
              width: "100%",
              background: "var(--bg-primary)",
              border: "1px solid var(--border)",
              color: "var(--text-primary)",
              padding: "6px 10px",
              "border-radius": "4px",
              outline: "none",
              "font-size": "12px",
              "font-family": "inherit",
            }}
          />
        </div>

        {/* Body */}
        <div style={{ display: "flex", flex: "1", "min-height": "0" }}>
          {/* Category sidebar */}
          <div
            style={{
              width: "120px",
              "border-right": "1px solid var(--border)",
              padding: "8px 0",
              "flex-shrink": "0",
              display: "flex",
              "flex-direction": "column",
              gap: "2px",
            }}
          >
            <For each={categories}>
              {(cat) => (
                <button
                  onClick={() => setCategory(cat.id)}
                  style={{
                    display: "flex",
                    "align-items": "center",
                    gap: "8px",
                    padding: "8px 12px",
                    background: category() === cat.id ? "var(--bg-active)" : "transparent",
                    border: "none",
                    "border-left":
                      category() === cat.id ? "2px solid var(--accent)" : "2px solid transparent",
                    color: category() === cat.id ? "var(--text-primary)" : "var(--text-muted)",
                    cursor: "pointer",
                    "font-size": "12px",
                    "text-align": "left",
                    width: "100%",
                  }}
                >
                  <Icon name={cat.icon} size={14} />
                  {cat.label}
                </button>
              )}
            </For>
          </div>

          {/* Settings content */}
          <div style={{ flex: "1", overflow: "auto", padding: "12px 20px" }}>
            <Show when={category() === "user"}>
              <UserSettings matchesSearch={matchesSearch} />
            </Show>
            <Show when={category() === "tools"}>
              <ToolsSettings matchesSearch={matchesSearch} />
            </Show>
            <Show when={category() === "advanced"}>
              <AdvancedSettings matchesSearch={matchesSearch} />
            </Show>
          </div>
        </div>

        {/* Footer with version */}
        <div
          style={{
            padding: "8px 16px",
            "border-top": "1px solid var(--border)",
            "background-color": "var(--bg-primary)",
            "font-size": "11px",
            color: "var(--text-muted)",
            "text-align": "right",
            "flex-shrink": "0",
          }}
        >
          Keynobi v{appVersion() ?? "—"}
        </div>
      </div>
    </Show>
  );
}

function SectionHeader(props: { title: string }): JSX.Element {
  return (
    <h3
      style={{
        "font-size": "11px",
        "font-weight": "600",
        "text-transform": "uppercase",
        "letter-spacing": "0.5px",
        color: "var(--text-muted)",
        margin: "16px 0 8px",
        padding: "0",
      }}
    >
      {props.title}
    </h3>
  );
}

// ── User Settings ─────────────────────────────────────────────────────────────

function UserSettings(props: { matchesSearch: (l: string, d?: string) => boolean }): JSX.Element {
  return (
    <>
      <SectionHeader title="Appearance" />
      <Show when={props.matchesSearch("UI Font Size", "Font size for UI elements")}>
        <SettingRow
          label="UI Font Size"
          description="Font size for panels, sidebar, and status bar"
        >
          <SettingNumberInput
            value={settingsState.appearance.uiFontSize}
            min={10}
            max={16}
            onChange={(v) => updateSetting("appearance", "uiFontSize", v)}
          />
        </SettingRow>
      </Show>

      <SectionHeader title="Search" />
      <Show when={props.matchesSearch("Context Lines", "Lines shown before and after each match")}>
        <SettingRow
          label="Context Lines"
          description="Lines shown before and after each search match"
        >
          <SettingNumberInput
            value={settingsState.search.contextLines}
            min={0}
            max={10}
            onChange={(v) => updateSetting("search", "contextLines", v)}
          />
        </SettingRow>
      </Show>
      <Show when={props.matchesSearch("Max Results", "Maximum number of search matches")}>
        <SettingRow
          label="Max Results"
          description="Maximum number of search matches before capping"
        >
          <SettingNumberInput
            value={settingsState.search.maxResults}
            min={100}
            max={100000}
            step={1000}
            onChange={(v) => updateSetting("search", "maxResults", v)}
          />
        </SettingRow>
      </Show>
      <Show when={props.matchesSearch("Max Files", "Maximum number of files to search")}>
        <SettingRow
          label="Max Files"
          description="Maximum number of files to include in search results"
        >
          <SettingNumberInput
            value={settingsState.search.maxFiles}
            min={50}
            max={10000}
            step={100}
            onChange={(v) => updateSetting("search", "maxFiles", v)}
          />
        </SettingRow>
      </Show>
    </>
  );
}

// ── Tools Settings ────────────────────────────────────────────────────────────

function ToolsSettings(props: { matchesSearch: (l: string, d?: string) => boolean }): JSX.Element {
  return (
    <>
      <Show when={props.matchesSearch("Android SDK", "Android SDK path")}>
        <SectionHeader title="Android SDK" />
        <SettingRow label="SDK Path" description="Path to the Android SDK installation">
          <AndroidSdkStatus />
        </SettingRow>
      </Show>

      <Show when={props.matchesSearch("Java JDK", "JAVA_HOME")}>
        <SectionHeader title="Java / JDK" />
        <SettingRow label="JAVA_HOME" description="Path to the JDK (required for Gradle builds)">
          <JavaStatus />
        </SettingRow>
      </Show>

      <SectionHeader title="Logcat" />
      <Show
        when={props.matchesSearch(
          "Auto-start Logcat",
          "Automatically start logcat when a device connects"
        )}
      >
        <SettingRow
          label="Auto-start on Connect"
          description="Automatically start logcat streaming when a device connects"
        >
          <SettingToggle
            checked={settingsState.logcat.autoStart}
            onChange={(v) => updateSetting("logcat", "autoStart", v)}
          />
        </SettingRow>
      </Show>

      <SectionHeader title="Claude Code (MCP)" />
      <Show when={props.matchesSearch("MCP Auto-start", "Start MCP server when the app launches")}>
        <SettingRow
          label="Auto-start MCP Server"
          description="Automatically start the MCP stdio server when the app opens. Lets Claude Code connect without a manual trigger."
        >
          <SettingToggle
            checked={settingsState.mcp.autoStart}
            onChange={(v) => updateSetting("mcp", "autoStart", v)}
          />
        </SettingRow>
      </Show>
      <Show
        when={props.matchesSearch(
          "MCP Build Timeout",
          "Maximum seconds to wait for a Gradle build via MCP"
        )}
      >
        <SettingRow
          label="Build Timeout (seconds)"
          description="Maximum time to wait for a Gradle build triggered via the run_gradle_task MCP tool. Increase for large projects."
        >
          <SettingNumberInput
            value={settingsState.mcp.buildTimeoutSec}
            min={60}
            max={3600}
            step={60}
            onChange={(v) => updateSetting("mcp", "buildTimeoutSec", v)}
          />
        </SettingRow>
      </Show>
      <Show
        when={props.matchesSearch(
          "MCP Logcat Count",
          "Default logcat entries returned by get_logcat_entries"
        )}
      >
        <SettingRow
          label="Default Logcat Count"
          description="Default number of logcat entries returned by get_logcat_entries when the AI agent does not specify a count."
        >
          <SettingNumberInput
            value={settingsState.mcp.logcatDefaultCount}
            min={50}
            max={5000}
            step={50}
            onChange={(v) => updateSetting("mcp", "logcatDefaultCount", v)}
          />
        </SettingRow>
      </Show>
      <Show
        when={props.matchesSearch(
          "MCP Build Log Lines",
          "Default build log lines returned by get_build_log"
        )}
      >
        <SettingRow
          label="Default Build Log Lines"
          description="Default number of raw build output lines returned by get_build_log when the AI agent does not specify a count."
        >
          <SettingNumberInput
            value={settingsState.mcp.buildLogDefaultLines}
            min={50}
            max={2000}
            step={50}
            onChange={(v) => updateSetting("mcp", "buildLogDefaultLines", v)}
          />
        </SettingRow>
      </Show>
    </>
  );
}

// ── Advanced Settings ─────────────────────────────────────────────────────────

function AdvancedSettings(props: {
  matchesSearch: (l: string, d?: string) => boolean;
}): JSX.Element {
  return (
    <>
      <div
        style={{
          "font-size": "11px",
          color: "var(--text-muted)",
          padding: "8px 0 12px",
          "border-bottom": "1px solid var(--border)",
        }}
      >
        These settings control internal limits and timing. Most users should leave these at their
        default values.
      </div>

      <SectionHeader title="Build" />
      <Show
        when={props.matchesSearch(
          "Auto Install on Build",
          "Automatically install APK after a successful build"
        )}
      >
        <SettingRow
          label="Auto Install on Build"
          description="Automatically install and launch the app after a successful build"
        >
          <SettingToggle
            checked={settingsState.build.autoInstallOnBuild}
            onChange={(v) => updateSetting("build", "autoInstallOnBuild", v)}
          />
        </SettingRow>
      </Show>

      <Show when={props.matchesSearch("Build log retention", "Days to keep build log files")}>
        <SettingRow
          label="Build log retention (days)"
          description="Days to keep build log files in ~/.keynobi/build-logs/ before they are deleted"
        >
          <SettingNumberInput
            value={settingsState.build.buildLogRetentionDays}
            min={1}
            max={365}
            onChange={(v) => updateSetting("build", "buildLogRetentionDays", v)}
          />
        </SettingRow>
      </Show>

      <Show when={props.matchesSearch("Build log folder limit", "Max size of build log folder")}>
        <SettingRow
          label="Build log folder limit (MB)"
          description="Max total size of ~/.keynobi/build-logs/ before oldest files are deleted"
        >
          <SettingNumberInput
            value={settingsState.build.buildLogMaxFolderMb}
            min={10}
            max={2048}
            onChange={(v) => updateSetting("build", "buildLogMaxFolderMb", v)}
          />
        </SettingRow>
      </Show>

      <SectionHeader title="Logging" />
      <Show when={props.matchesSearch("Log retention", "Days to keep log files")}>
        <SettingRow label="Log retention" description="Days to keep log files in ~/.keynobi/logs/">
          <input
            type="number"
            min={1}
            max={365}
            value={settingsState.advanced.logRetentionDays}
            onInput={(e) =>
              updateSetting("advanced", "logRetentionDays", parseInt(e.currentTarget.value) || 7)
            }
          />
        </SettingRow>
      </Show>

      <Show
        when={props.matchesSearch(
          "Max log folder size",
          "Size limit for log files before rotation"
        )}
      >
        <SettingRow
          label="Max log folder size (MB)"
          description="Size limit for ~/.keynobi/logs/ before oldest files are deleted. Takes effect on next app restart."
        >
          <SettingNumberInput
            value={settingsState.advanced.logMaxSizeMb}
            min={50}
            max={10000}
            step={50}
            onChange={(v) => updateSetting("advanced", "logMaxSizeMb", v)}
          />
        </SettingRow>
      </Show>

      <SectionHeader title="Privacy" />
      <Show
        when={props.matchesSearch(
          "Anonymous crash reporting",
          "Send minimal app crash diagnostics Enable Do not send"
        )}
      >
        <SettingRow
          label="Anonymous crash reporting"
          description="Send minimal app crash diagnostics (no project paths, source, or Gradle/log output). Restart required to apply."
        >
          <div
            role="radiogroup"
            aria-label="Anonymous crash reporting"
            style={{
              display: "flex",
              "flex-direction": "row",
              "flex-wrap": "wrap",
              gap: "14px",
              "justify-content": "flex-end",
            }}
          >
            <label
              style={{
                display: "flex",
                "align-items": "center",
                gap: "6px",
                cursor: "pointer",
                "font-size": "12px",
                color: "var(--text-primary)",
                "user-select": "none",
              }}
            >
              <input
                type="radio"
                name="settings-telemetry"
                checked={settingsState.telemetry?.enabled === true}
                onChange={() => updateSetting("telemetry", "enabled", true)}
              />
              Enable
            </label>
            <label
              style={{
                display: "flex",
                "align-items": "center",
                gap: "6px",
                cursor: "pointer",
                "font-size": "12px",
                color: "var(--text-primary)",
                "user-select": "none",
              }}
            >
              <input
                type="radio"
                name="settings-telemetry"
                checked={settingsState.telemetry?.enabled !== true}
                onChange={() => updateSetting("telemetry", "enabled", false)}
              />
              Do not send
            </label>
          </div>
        </SettingRow>
      </Show>
    </>
  );
}

export default SettingsPanel;
