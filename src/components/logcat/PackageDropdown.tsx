/**
 * PackageDropdown — toolbar control for package-based logcat filtering.
 *
 * Renders a compact button showing the active package (or "All packages").
 * On click, opens an absolutely-positioned panel that lists all packages seen
 * in the current logcat session, with a search input for quick narrowing.
 * Selecting a package fires `onSelect(pkg)`; clicking "All packages" fires
 * `onSelect(null)` to clear the filter.
 */

import {
  type JSX,
  createSignal,
  createMemo,
  For,
  Show,
} from "solid-js";
import { getMinePackage } from "@/lib/logcat-query";

// ── Props ─────────────────────────────────────────────────────────────────────

export interface PackageDropdownProps {
  /** Sorted list of package names seen in this logcat session. */
  packages: string[];
  /** Currently active package filter value, or null for "All packages". */
  selected: string | null;
  /** Called with the chosen package name, or null to clear. */
  onSelect: (pkg: string | null) => void;
}

// ── Constants ─────────────────────────────────────────────────────────────────

const MAX_VISIBLE_ROWS = 10;

// ── Component ─────────────────────────────────────────────────────────────────

export function PackageDropdown(props: PackageDropdownProps): JSX.Element {
  const [open, setOpen] = createSignal(false);
  const [search, setSearch] = createSignal("");
  let searchRef!: HTMLInputElement;

  // ── Derived ─────────────────────────────────────────────────────────────────

  const minePackage = createMemo(() => getMinePackage());

  const filteredPackages = createMemo(() => {
    const q = search().toLowerCase();
    return q
      ? props.packages.filter((p) => p.toLowerCase().includes(q))
      : props.packages;
  });

  // Truncate for display in the button (keep last two segments of package name).
  const displayLabel = createMemo(() => {
    const sel = props.selected;
    if (!sel) return "All packages";
    if (sel === "mine") {
      const mine = minePackage();
      return mine ? truncatePkg(mine) : "My App";
    }
    return truncatePkg(sel);
  });

  const isActive = createMemo(() => props.selected !== null);

  // ── Helpers ─────────────────────────────────────────────────────────────────

  function truncatePkg(pkg: string): string {
    const parts = pkg.split(".");
    if (parts.length <= 3) return pkg;
    return `…${parts.slice(-2).join(".")}`;
  }

  // ── Handlers ────────────────────────────────────────────────────────────────

  function handleToggle() {
    const nextOpen = !open();
    setOpen(nextOpen);
    if (nextOpen) {
      setSearch("");
      // Focus the search input after the panel mounts.
      queueMicrotask(() => searchRef?.focus());
    }
  }

  function handleSelect(pkg: string | null) {
    props.onSelect(pkg);
    setOpen(false);
    setSearch("");
  }

  // ── Render ───────────────────────────────────────────────────────────────────

  return (
    <div style={{ position: "relative", "flex-shrink": "0" }}>
      {/* Trigger button */}
      <button
        onClick={handleToggle}
        title={isActive() ? `Package filter: ${props.selected}` : "Filter by package"}
        style={{
          display: "flex",
          "align-items": "center",
          gap: "4px",
          padding: "1px 7px",
          "font-size": "10px",
          background: isActive() ? "rgba(var(--accent-rgb, 59,130,246),0.15)" : "var(--bg-primary)",
          color: isActive() ? "var(--accent)" : "var(--text-muted)",
          border: `1px solid ${isActive() ? "var(--accent)" : "var(--border)"}`,
          "border-radius": "10px",
          cursor: "pointer",
          "white-space": "nowrap",
          "max-width": "160px",
          overflow: "hidden",
          "text-overflow": "ellipsis",
          transition: "all 0.1s",
        }}
      >
        <span style={{ overflow: "hidden", "text-overflow": "ellipsis", "white-space": "nowrap" }}>
          {displayLabel()}
        </span>
        <span style={{ opacity: "0.6", "flex-shrink": "0", "font-size": "9px" }}>▾</span>
      </button>

      {/* Dropdown panel */}
      <Show when={open()}>
        <div
          style={{
            position: "absolute",
            top: "calc(100% + 4px)",
            left: "0",
            "min-width": "220px",
            "max-width": "320px",
            background: "var(--bg-secondary)",
            border: "1px solid var(--border)",
            "border-radius": "4px",
            "box-shadow": "0 6px 20px rgba(0,0,0,0.45)",
            "z-index": "700",
            "font-size": "11px",
          }}
        >
          {/* Search input */}
          <div style={{ padding: "6px 8px 4px", "border-bottom": "1px solid var(--border)" }}>
            <input
              ref={searchRef}
              type="text"
              placeholder="Search packages…"
              value={search()}
              onInput={(e) => setSearch(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  e.preventDefault();
                  setOpen(false);
                }
              }}
              style={{
                width: "100%",
                background: "var(--bg-primary)",
                border: "1px solid var(--border)",
                color: "var(--text-primary)",
                "border-radius": "3px",
                padding: "3px 7px",
                "font-size": "10px",
                "font-family": "var(--font-mono)",
                outline: "none",
                "box-sizing": "border-box",
              }}
            />
          </div>

          {/* Package list */}
          <div
            style={{
              "max-height": `${MAX_VISIBLE_ROWS * 26}px`,
              "overflow-y": "auto",
              padding: "4px 0",
            }}
          >
            {/* "All packages" row */}
            <Show when={!search()}>
              <PackageRow
                label="All packages"
                sublabel={null}
                active={props.selected === null}
                onClick={() => handleSelect(null)}
              />
            </Show>

            {/* "My App" shortcut (only when a project is open) */}
            <Show when={!search() && minePackage() !== null}>
              <PackageRow
                label="My App"
                sublabel="package:mine"
                active={props.selected === "mine"}
                onClick={() => handleSelect("mine")}
              />
            </Show>

            {/* Separator */}
            <Show when={!search() && props.packages.length > 0}>
              <div style={{ height: "1px", background: "var(--border)", margin: "4px 0" }} />
            </Show>

            {/* Actual package list */}
            <For each={filteredPackages()}>
              {(pkg) => (
                <PackageRow
                  label={pkg}
                  sublabel={null}
                  active={props.selected === pkg}
                  onClick={() => handleSelect(pkg)}
                />
              )}
            </For>

            {/* Empty state when search yields no results */}
            <Show when={search() && filteredPackages().length === 0}>
              <div style={{ padding: "8px 12px", color: "var(--text-muted)", "font-size": "10px" }}>
                No packages matching "{search()}"
              </div>
            </Show>

            {/* Empty state when no packages have been seen yet */}
            <Show when={!search() && props.packages.length === 0}>
              <div style={{ padding: "8px 12px", color: "var(--text-muted)", "font-size": "10px" }}>
                No packages seen yet — start logcat to populate this list.
              </div>
            </Show>
          </div>
        </div>

        {/* Click-outside overlay */}
        <div
          style={{ position: "fixed", inset: "0", "z-index": "699" }}
          onClick={() => setOpen(false)}
        />
      </Show>
    </div>
  );
}

// ── PackageRow ─────────────────────────────────────────────────────────────────

function PackageRow(props: {
  label: string;
  sublabel: string | null;
  active: boolean;
  onClick: () => void;
}): JSX.Element {
  return (
    <div
      onClick={props.onClick}
      style={{
        display: "flex",
        "align-items": "center",
        gap: "6px",
        padding: "4px 12px",
        cursor: "pointer",
        background: props.active ? "rgba(var(--accent-rgb, 59,130,246),0.15)" : "transparent",
        color: props.active ? "var(--accent)" : "var(--text-primary)",
      }}
      onMouseEnter={(e) => {
        if (!props.active) (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.06)";
      }}
      onMouseLeave={(e) => {
        if (!props.active) (e.currentTarget as HTMLElement).style.background = "transparent";
      }}
    >
      {/* Active indicator */}
      <span style={{ width: "8px", "flex-shrink": "0", "font-size": "9px" }}>
        {props.active ? "●" : ""}
      </span>

      <span
        style={{
          flex: "1",
          "font-family": "var(--font-mono)",
          "font-size": "10px",
          overflow: "hidden",
          "text-overflow": "ellipsis",
          "white-space": "nowrap",
        }}
        title={props.label}
      >
        {props.label}
      </span>

      <Show when={props.sublabel}>
        <span style={{ "font-size": "9px", color: "var(--text-muted)", "flex-shrink": "0", "font-family": "var(--font-mono)" }}>
          {props.sublabel}
        </span>
      </Show>
    </div>
  );
}

export default PackageDropdown;
