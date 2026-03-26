import {
  type JSX,
  Show,
  For,
  createSignal,
  createEffect,
  createMemo,
} from "solid-js";
import { searchActions } from "@/lib/action-registry";
import Icon from "@/components/common/Icon";

export type PaletteMode = "commands";

interface PaletteState {
  open: boolean;
  mode: PaletteMode;
}

const [paletteState, setPaletteState] = createSignal<PaletteState>({
  open: false,
  mode: "commands",
});

export function openPalette(_mode: string = "commands") {
  setPaletteState({ open: true, mode: "commands" });
}

export function closePalette() {
  setPaletteState({ open: false, mode: "commands" });
}

export function isPaletteOpen(): boolean {
  return paletteState().open;
}

interface PaletteItem {
  id: string;
  label: string;
  detail?: string;
  icon?: string;
  action: () => void;
}

export function CommandPalette(): JSX.Element {
  let inputRef!: HTMLInputElement;
  const [query, setQuery] = createSignal("");
  const [selectedIndex, setSelectedIndex] = createSignal(0);

  createEffect(() => {
    if (paletteState().open) {
      setQuery("");
      setSelectedIndex(0);
      setTimeout(() => inputRef?.focus(), 10);
    }
  });

  const results = createMemo<PaletteItem[]>(() => {
    const q = query().startsWith(">") ? query().slice(1).trim() : query();
    const actions = searchActions(q);
    return actions.map((a) => ({
      id: a.id,
      label: a.label,
      detail: a.shortcut ?? (a.category ? `${a.category}` : ""),
      icon: a.icon,
      action: () => {
        closePalette();
        a.action();
      },
    }));
  });

  function handleKeyDown(e: KeyboardEvent) {
    const items = results();
    if (e.key === "Escape") {
      e.preventDefault();
      closePalette();
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((i) => Math.min(i + 1, items.length - 1));
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((i) => Math.max(i - 1, 0));
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      const item = items[selectedIndex()];
      if (item) item.action();
      return;
    }
  }

  return (
    <Show when={paletteState().open}>
      {/* Backdrop */}
      <div
        onClick={() => closePalette()}
        style={{
          position: "fixed",
          inset: "0",
          "z-index": "1000",
          background: "rgba(0,0,0,0.3)",
        }}
      />
      {/* Palette */}
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Command Palette"
        style={{
          position: "fixed",
          top: "20%",
          left: "50%",
          transform: "translateX(-50%)",
          width: "560px",
          "max-height": "400px",
          background: "var(--bg-secondary)",
          border: "1px solid var(--border)",
          "border-radius": "8px",
          "box-shadow": "0 8px 32px rgba(0,0,0,0.5)",
          "z-index": "1001",
          display: "flex",
          "flex-direction": "column",
          overflow: "hidden",
        }}
      >
        {/* Input */}
        <div style={{ padding: "8px", "border-bottom": "1px solid var(--border)" }}>
          <input
            ref={inputRef}
            type="text"
            placeholder="Type a command..."
            aria-label="Command Palette"
            role="combobox"
            aria-expanded="true"
            aria-autocomplete="list"
            value={query()}
            onInput={(e) => { setQuery(e.currentTarget.value); setSelectedIndex(0); }}
            onKeyDown={handleKeyDown}
            style={{
              width: "100%",
              background: "var(--bg-primary)",
              border: "1px solid var(--border)",
              color: "var(--text-primary)",
              padding: "6px 10px",
              "border-radius": "4px",
              outline: "none",
              "font-size": "13px",
              "font-family": "inherit",
            }}
          />
        </div>
        {/* Results */}
        <div role="listbox" style={{ flex: "1", overflow: "auto", "min-height": "0" }}>
          <For each={results()}>
            {(item, index) => (
              <div
                role="option"
                aria-selected={index() === selectedIndex()}
                onClick={() => item.action()}
                onMouseEnter={() => setSelectedIndex(index())}
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "8px",
                  padding: "6px 12px",
                  cursor: "pointer",
                  background:
                    index() === selectedIndex()
                      ? "var(--accent)"
                      : "transparent",
                  color:
                    index() === selectedIndex()
                      ? "#fff"
                      : "var(--text-primary)",
                }}
              >
                <Show when={item.icon}>
                  <Icon name={item.icon!} size={14} />
                </Show>
                <span
                  style={{
                    flex: "1",
                    overflow: "hidden",
                    "text-overflow": "ellipsis",
                    "white-space": "nowrap",
                    "font-size": "13px",
                  }}
                >
                  {item.label}
                </span>
                <Show when={item.detail}>
                  <span
                    style={{
                      "font-size": "11px",
                      color:
                        index() === selectedIndex()
                          ? "rgba(255,255,255,0.7)"
                          : "var(--text-muted)",
                      "flex-shrink": "0",
                    }}
                  >
                    {item.detail}
                  </span>
                </Show>
              </div>
            )}
          </For>
          <Show when={results().length === 0}>
            <div style={{ padding: "16px", "text-align": "center", color: "var(--text-muted)", "font-size": "12px" }}>
              {query().trim() ? "No commands found" : "Type to search commands"}
            </div>
          </Show>
        </div>
      </div>
    </Show>
  );
}

export default CommandPalette;
