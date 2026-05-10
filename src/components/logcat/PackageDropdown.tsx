/**
 * PackageDropdown — toolbar control for package-based logcat filtering.
 *
 * Renders a compact button showing the active package (or "All packages").
 * On click, opens an absolutely-positioned panel that lists all packages seen
 * in the current logcat session, with a search input for quick narrowing.
 * Selecting a package fires `onSelect(pkg)`; clicking "All packages" fires
 * `onSelect(null)` to clear the filter.
 */

import { type JSX, createSignal, createMemo, For, Show } from "solid-js";
import {
  FilterChip,
  Icon,
  Input,
  MenuEmptyState,
  MenuList,
  MenuListItem,
  Popover,
  Separator,
} from "@/components/ui";
import { getMinePackage } from "@/lib/logcat-mine-package";
import styles from "./PackageDropdown.module.css";

// ── Props ─────────────────────────────────────────────────────────────────────

export interface PackageDropdownProps {
  /** Sorted list of package names seen in this logcat session. */
  packages: string[];
  /** Currently active package filter value, or null for "All packages". */
  selected: string | null;
  /** Called with the chosen package name, or null to clear. */
  onSelect: (pkg: string | null) => void;
}

// ── Component ─────────────────────────────────────────────────────────────────

export function PackageDropdown(props: PackageDropdownProps): JSX.Element {
  const [open, setOpen] = createSignal(false);
  const [search, setSearch] = createSignal("");
  let searchRef!: HTMLInputElement;

  // ── Derived ─────────────────────────────────────────────────────────────────

  const minePackage = createMemo(() => getMinePackage());

  const filteredPackages = createMemo(() => {
    const q = search().toLowerCase();
    return q ? props.packages.filter((p) => p.toLowerCase().includes(q)) : props.packages;
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

  function closeDropdown() {
    setOpen(false);
    setSearch("");
  }

  // ── Render ───────────────────────────────────────────────────────────────────

  return (
    <Popover
      open={open()}
      onOpenChange={(nextOpen) => {
        if (nextOpen) handleToggle();
        else closeDropdown();
      }}
      align="left"
      minWidth="220px"
      maxWidth="320px"
      trigger={() => (
        <FilterChip
          active={isActive()}
          activeStyle="soft"
          maxWidth="160px"
          onClick={handleToggle}
          title={isActive() ? `Package filter: ${props.selected}` : "Filter by package"}
        >
          <span class={styles.triggerLabel}>{displayLabel()}</span>
          <Icon name="chevron-down" size={10} class={styles.triggerChevron} />
        </FilterChip>
      )}
    >
      <Show when={open()}>
        <>
          <div class={styles.searchFrame}>
            <Input
              inputRef={(el) => {
                searchRef = el;
              }}
              type="text"
              placeholder="Search packages…"
              value={search()}
              size="xs"
              mono
              onInput={setSearch}
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  e.preventDefault();
                  setOpen(false);
                }
              }}
              class={styles.searchInput}
            />
          </div>

          <MenuList class={styles.packageList}>
            <Show when={!search()}>
              <PackageRow
                label="All packages"
                sublabel={null}
                active={props.selected === null}
                onClick={() => handleSelect(null)}
              />
            </Show>

            <Show when={!search() && minePackage() !== null}>
              <PackageRow
                label="My App"
                sublabel="package:mine"
                active={props.selected === "mine"}
                onClick={() => handleSelect("mine")}
              />
            </Show>

            <Show when={!search() && props.packages.length > 0}>
              <Separator spacing="sm" />
            </Show>

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

            <Show when={search() && filteredPackages().length === 0}>
              <MenuEmptyState>No packages matching "{search()}"</MenuEmptyState>
            </Show>

            <Show when={!search() && props.packages.length === 0}>
              <MenuEmptyState>
                No packages seen yet — start logcat to populate this list.
              </MenuEmptyState>
            </Show>
          </MenuList>
        </>
      </Show>
    </Popover>
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
    <MenuListItem onClick={() => props.onClick()} active={props.active}>
      <span class={styles.activeMarker} aria-hidden="true">
        {props.active ? "●" : ""}
      </span>

      <span class={styles.label} title={props.label}>
        {props.label}
      </span>

      <Show when={props.sublabel}>
        <span class={styles.sublabel}>{props.sublabel}</span>
      </Show>
    </MenuListItem>
  );
}

export default PackageDropdown;
