import { type JSX, Show, For, createSignal } from "solid-js";
import {
  settingsState,
  updateSetting,
  resetSettings,
} from "@/stores/settings.store";
import {
  SettingRow,
  SettingToggle,
  SettingNumberInput,
  SettingTextInput,
  SettingSelect,
  SettingTagList,
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
    return (
      label.toLowerCase().includes(q) ||
      (desc?.toLowerCase().includes(q) ?? false)
    );
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
            <span style={{ "font-size": "14px", "font-weight": "600", color: "var(--text-primary)" }}>
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
        <div style={{ padding: "8px 16px", "border-bottom": "1px solid var(--border)", "flex-shrink": "0" }}>
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
                    "border-left": category() === cat.id ? "2px solid var(--accent)" : "2px solid transparent",
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
  const m = props.matchesSearch;
  return (
    <>
      <SectionHeader title="Editor" />
      <Show when={m("Font Family", "Editor font")}>
        <SettingRow label="Font Family" description="Editor font face">
          <SettingTextInput
            value={settingsState.editor.fontFamily}
            onChange={(v) => updateSetting("editor", "fontFamily", v)}
          />
        </SettingRow>
      </Show>
      <Show when={m("Font Size", "Editor font size in pixels")}>
        <SettingRow label="Font Size" description="Editor font size in pixels">
          <SettingNumberInput
            value={settingsState.editor.fontSize}
            min={10} max={30}
            onChange={(v) => updateSetting("editor", "fontSize", v)}
          />
        </SettingRow>
      </Show>
      <Show when={m("Tab Size", "Number of spaces per tab")}>
        <SettingRow label="Tab Size" description="Number of spaces per tab">
          <SettingSelect
            value={String(settingsState.editor.tabSize)}
            options={["2", "4", "8"]}
            onChange={(v) => updateSetting("editor", "tabSize", parseInt(v, 10))}
          />
        </SettingRow>
      </Show>
      <Show when={m("Insert Spaces", "Use spaces instead of tabs")}>
        <SettingRow label="Insert Spaces" description="Use spaces instead of tabs">
          <SettingToggle
            checked={settingsState.editor.insertSpaces}
            onChange={(v) => updateSetting("editor", "insertSpaces", v)}
          />
        </SettingRow>
      </Show>
      <Show when={m("Word Wrap", "Wrap long lines")}>
        <SettingRow label="Word Wrap" description="Wrap long lines instead of horizontal scrolling">
          <SettingToggle
            checked={settingsState.editor.wordWrap}
            onChange={(v) => updateSetting("editor", "wordWrap", v)}
          />
        </SettingRow>
      </Show>
      <Show when={m("Line Numbers", "Show line numbers")}>
        <SettingRow label="Line Numbers" description="Show line numbers in the gutter">
          <SettingToggle
            checked={settingsState.editor.lineNumbers}
            onChange={(v) => updateSetting("editor", "lineNumbers", v)}
          />
        </SettingRow>
      </Show>
      <Show when={m("Bracket Matching", "Highlight matching brackets")}>
        <SettingRow label="Bracket Matching" description="Highlight matching brackets">
          <SettingToggle
            checked={settingsState.editor.bracketMatching}
            onChange={(v) => updateSetting("editor", "bracketMatching", v)}
          />
        </SettingRow>
      </Show>
      <Show when={m("Active Line Highlight", "Highlight the current line")}>
        <SettingRow label="Active Line Highlight" description="Highlight the current line">
          <SettingToggle
            checked={settingsState.editor.highlightActiveLine}
            onChange={(v) => updateSetting("editor", "highlightActiveLine", v)}
          />
        </SettingRow>
      </Show>
      <Show when={m("Auto-close Brackets", "Automatically insert closing brackets")}>
        <SettingRow label="Auto-close Brackets" description="Automatically insert closing brackets, quotes, etc.">
          <SettingToggle
            checked={settingsState.editor.autoCloseBrackets}
            onChange={(v) => updateSetting("editor", "autoCloseBrackets", v)}
          />
        </SettingRow>
      </Show>

      <SectionHeader title="Appearance" />
      <Show when={m("UI Font Size", "Font size for UI elements")}>
        <SettingRow label="UI Font Size" description="Font size for panels, sidebar, and status bar">
          <SettingNumberInput
            value={settingsState.appearance.uiFontSize}
            min={10} max={16}
            onChange={(v) => updateSetting("appearance", "uiFontSize", v)}
          />
        </SettingRow>
      </Show>

      <SectionHeader title="Search" />
      <Show when={m("Context Lines", "Lines shown before and after each match")}>
        <SettingRow label="Context Lines" description="Lines shown before and after each search match">
          <SettingNumberInput
            value={settingsState.search.contextLines}
            min={0} max={10}
            onChange={(v) => updateSetting("search", "contextLines", v)}
          />
        </SettingRow>
      </Show>
      <Show when={m("Max Results", "Maximum number of search matches")}>
        <SettingRow label="Max Results" description="Maximum number of search matches before capping">
          <SettingNumberInput
            value={settingsState.search.maxResults}
            min={100} max={100000} step={1000}
            onChange={(v) => updateSetting("search", "maxResults", v)}
          />
        </SettingRow>
      </Show>
      <Show when={m("Max Files", "Maximum number of files to search")}>
        <SettingRow label="Max Files" description="Maximum number of files to include in search results">
          <SettingNumberInput
            value={settingsState.search.maxFiles}
            min={50} max={10000} step={100}
            onChange={(v) => updateSetting("search", "maxFiles", v)}
          />
        </SettingRow>
      </Show>

      <SectionHeader title="Files" />
      <Show when={m("Excluded Directories", "Directories hidden from file tree and search")}>
        <SettingRow label="Excluded Directories" description="Directories hidden from file tree and search">
          <SettingTagList
            tags={settingsState.files.excludedDirs}
            onChange={(v) => updateSetting("files", "excludedDirs", v)}
          />
        </SettingRow>
      </Show>
      <Show when={m("Excluded Extensions", "File extensions hidden from file tree")}>
        <SettingRow label="Excluded Extensions" description="File extensions hidden from file tree">
          <SettingTagList
            tags={settingsState.files.excludedExtensions}
            onChange={(v) => updateSetting("files", "excludedExtensions", v)}
          />
        </SettingRow>
      </Show>
      <Show when={m("Max File Size", "Maximum file size the editor will open")}>
        <SettingRow label="Max File Size (MB)" description="Maximum file size the editor will open">
          <SettingNumberInput
            value={settingsState.files.maxFileSizeMb}
            min={1} max={100}
            onChange={(v) => updateSetting("files", "maxFileSizeMb", v)}
          />
        </SettingRow>
      </Show>
    </>
  );
}

// ── Tools Settings ────────────────────────────────────────────────────────────

function ToolsSettings(props: { matchesSearch: (l: string, d?: string) => boolean }): JSX.Element {
  const m = props.matchesSearch;
  return (
    <>
      <Show when={m("Android SDK", "Android SDK path")}>
        <SectionHeader title="Android SDK" />
        <SettingRow label="SDK Path" description="Path to the Android SDK installation">
          <AndroidSdkStatus />
        </SettingRow>
      </Show>

      <Show when={m("Java JDK", "JAVA_HOME")}>
        <SectionHeader title="Java / JDK" />
        <SettingRow label="JAVA_HOME" description="Path to the JDK (required for Gradle builds)">
          <JavaStatus />
        </SettingRow>
      </Show>
    </>
  );
}

// ── Advanced Settings ─────────────────────────────────────────────────────────

function AdvancedSettings(props: { matchesSearch: (l: string, d?: string) => boolean }): JSX.Element {
  const m = props.matchesSearch;
  return (
    <>
      <div style={{ "font-size": "11px", color: "var(--text-muted)", padding: "8px 0 12px", "border-bottom": "1px solid var(--border)" }}>
        These settings control internal limits and timing. Most users should leave these at their default values.
      </div>

      <SectionHeader title="Build" />
      <Show when={m("Gradle Parallel Builds", "Run Gradle tasks in parallel")}>
        <SettingRow label="Parallel Builds" description="Pass --parallel to Gradle for faster multi-module builds">
          <SettingToggle
            checked={settingsState.build.gradleParallel}
            onChange={(v) => updateSetting("build", "gradleParallel", v)}
          />
        </SettingRow>
      </Show>
      <Show when={m("Gradle Offline Mode", "Prevent Gradle from accessing the network")}>
        <SettingRow label="Offline Mode" description="Pass --offline to Gradle to skip dependency downloads">
          <SettingToggle
            checked={settingsState.build.gradleOffline}
            onChange={(v) => updateSetting("build", "gradleOffline", v)}
          />
        </SettingRow>
      </Show>
      <Show when={m("Gradle JVM Args", "Extra JVM arguments for the Gradle daemon")}>
        <SettingRow label="Gradle JVM Args" description='Extra JVM arguments passed to the Gradle daemon (e.g. "-Xmx4g")'>
          <SettingTextInput
            value={settingsState.build.gradleJvmArgs ?? ""}
            placeholder="-Xmx4g"
            onChange={(v) => updateSetting("build", "gradleJvmArgs", v || null)}
          />
        </SettingRow>
      </Show>
      <Show when={m("Auto Install on Build", "Automatically install APK after a successful build")}>
        <SettingRow label="Auto Install on Build" description="Automatically install and launch the app after a successful build">
          <SettingToggle
            checked={settingsState.build.autoInstallOnBuild}
            onChange={(v) => updateSetting("build", "autoInstallOnBuild", v)}
          />
        </SettingRow>
      </Show>
    </>
  );
}

export default SettingsPanel;
