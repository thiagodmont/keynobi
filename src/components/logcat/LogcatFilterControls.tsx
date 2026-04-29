import { For, Show, type JSX } from "solid-js";
import { QueryBar } from "@/components/logcat/QueryBar";
import { PackageDropdown } from "@/components/logcat/PackageDropdown";
import { btnStyle } from "./logcat-styles";

export const LOGCAT_AGE_PILLS = [
  { label: "30s", value: "30s" },
  { label: "1m", value: "1m" },
  { label: "5m", value: "5m" },
  { label: "15m", value: "15m" },
  { label: "1h", value: "1h" },
  { label: "All", value: null },
] as const;

export type LogcatAgePillValue = (typeof LOGCAT_AGE_PILLS)[number]["value"];

export function LogcatFilterControls(props: {
  query: string;
  knownTags: string[];
  knownPackages: string[];
  hasAgeFilter: boolean;
  activeAge: string | null;
  activePackage: string | null;
  isFiltered: boolean;
  onQueryChange: (query: string) => void;
  onAgeSelect: (value: LogcatAgePillValue) => void;
  onPackageSelect: (pkg: string | null) => void;
  onClear: () => void;
}): JSX.Element {
  return (
    <div
      style={{
        display: "flex",
        "flex-wrap": "wrap",
        "align-items": "flex-start",
        gap: "6px",
        padding: "5px 10px",
        background: "var(--bg-secondary)",
        "border-bottom": "1px solid var(--border)",
        "flex-shrink": "0",
      }}
    >
      <QueryBar
        value={props.query}
        onChange={props.onQueryChange}
        knownTags={props.knownTags}
        knownPackages={props.knownPackages}
      />

      <div style={{ display: "flex", "align-items": "center", gap: "4px", "flex-shrink": "0" }}>
        <span
          style={{
            "font-size": "10px",
            color: "var(--text-muted)",
            "margin-right": "2px",
            "flex-shrink": "0",
          }}
        >
          Age:
        </span>
        <For each={LOGCAT_AGE_PILLS}>
          {(pill) => {
            const isActive = () =>
              pill.value === null ? !props.hasAgeFilter : props.activeAge === pill.value;
            return (
              <button
                onClick={() => props.onAgeSelect(pill.value)}
                style={{
                  padding: "1px 7px",
                  "font-size": "10px",
                  background: isActive() ? "var(--accent)" : "var(--bg-primary)",
                  color: isActive() ? "#fff" : "var(--text-muted)",
                  border: `1px solid ${isActive() ? "var(--accent)" : "var(--border)"}`,
                  "border-radius": "10px",
                  cursor: "pointer",
                  "flex-shrink": "0",
                  transition: "all 0.1s",
                }}
              >
                {pill.label}
              </button>
            );
          }}
        </For>

        <div
          style={{
            width: "1px",
            height: "14px",
            background: "var(--border)",
            "flex-shrink": "0",
            "margin-left": "2px",
          }}
        />

        <PackageDropdown
          packages={props.knownPackages}
          selected={props.activePackage}
          onSelect={props.onPackageSelect}
        />

        <Show when={props.isFiltered}>
          <button
            onClick={() => props.onClear()}
            title="Clear all filters"
            style={{
              ...btnStyle("var(--text-muted)"),
              "font-size": "10px",
              padding: "1px 7px",
            }}
          >
            ✕ Clear
          </button>
        </Show>
      </div>
    </div>
  );
}
