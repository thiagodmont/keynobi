import {
  type JSX,
  Show,
  For,
  createSignal,
  onMount,
  onCleanup,
} from "solid-js";
import {
  variantState,
  selectVariant,
  loadVariants,
} from "@/stores/variant.store";
import { projectState } from "@/stores/project.store";
import Icon from "@/components/common/Icon";

// ── Variant picker signal ─────────────────────────────────────────────────────

const [pickerOpen, setPickerOpen] = createSignal(false);

export function openVariantPicker() {
  setPickerOpen(true);
}

// ── Status bar pill ───────────────────────────────────────────────────────────

export function VariantSelectorPill(): JSX.Element {
  const label = () => {
    if (variantState.loading) return "Detecting…";
    return variantState.activeVariant ?? "No Variant";
  };
  const hasVariants = () => variantState.variants.length > 0;
  const isSpinning = () => variantState.loading || variantState.gradleLoading;

  onMount(() => {
    if (projectState.gradleRoot || projectState.projectRoot) {
      loadVariants().catch(console.error);
    }
  });

  return (
    <>
      <button
        onClick={() => setPickerOpen(true)}
        title={`Active build variant: ${label()} — click to change (Cmd+Shift+V)`}
        style={{
          display: "flex",
          "align-items": "center",
          gap: "4px",
          padding: "0 6px",
          height: "18px",
          background: "rgba(255,255,255,0.08)",
          border: "1px solid rgba(255,255,255,0.12)",
          "border-radius": "3px",
          cursor: "pointer",
          "flex-shrink": "0",
          transition: "background 0.1s",
          color: hasVariants() ? "var(--text-primary, #fff)" : "rgba(255,255,255,0.4)",
          "font-size": "11px",
          "white-space": "nowrap",
        }}
        onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.15)"; }}
        onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.08)"; }}
      >
        <Show
          when={isSpinning()}
          fallback={<span style={{ opacity: "0.7", "font-size": "10px" }}>▾</span>}
        >
          <span class="lsp-spinner" style={{ "line-height": "0", "flex-shrink": "0" }}>
            <Icon name="spinner" size={10} color="rgba(255,255,255,0.7)" />
          </span>
        </Show>
        {label()}
      </button>

      <Show when={pickerOpen()}>
        <VariantPickerModal onClose={() => setPickerOpen(false)} />
      </Show>
    </>
  );
}

// ── Full picker modal ─────────────────────────────────────────────────────────

function VariantPickerModal(props: { onClose: () => void }): JSX.Element {
  const [search, setSearch] = createSignal("");
  let overlayRef!: HTMLDivElement;

  const filtered = () => {
    const q = search().toLowerCase();
    return variantState.variants.filter(
      (v) =>
        v.name.toLowerCase().includes(q) ||
        v.buildType.toLowerCase().includes(q) ||
        v.flavors.some((f) => f.toLowerCase().includes(q))
    );
  };

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") props.onClose();
  }

  onMount(() => {
    document.addEventListener("keydown", handleKeyDown);
  });
  onCleanup(() => {
    document.removeEventListener("keydown", handleKeyDown);
  });

  async function handleSelect(variantName: string) {
    await selectVariant(variantName);
    props.onClose();
  }

  async function handleRefresh() {
    await loadVariants().catch(console.error);
  }

  const statusLabel = () => {
    if (variantState.gradleError) return `Gradle error: ${variantState.gradleError}`;
    if (variantState.gradleLoading) return "Detecting variants from Gradle…";
    if (variantState.fromGradle) return "Detected via Gradle";
    if (variantState.variants.length > 0) return "From build.gradle (preview)";
    return null;
  };

  const isError = () => !!variantState.gradleError;

  return (
    <div
      ref={overlayRef}
      style={{
        position: "fixed",
        inset: "0",
        "z-index": "1000",
        display: "flex",
        "align-items": "center",
        "justify-content": "center",
        background: "rgba(0,0,0,0.4)",
      }}
      onClick={(e) => { if (e.target === overlayRef) props.onClose(); }}
    >
      <div
        style={{
          background: "var(--bg-secondary)",
          border: "1px solid var(--border)",
          "border-radius": "8px",
          width: "360px",
          "max-height": "460px",
          display: "flex",
          "flex-direction": "column",
          "box-shadow": "0 8px 32px rgba(0,0,0,0.5)",
          overflow: "hidden",
        }}
      >
        {/* ── Header ── */}
        <div
          style={{
            display: "flex",
            "align-items": "center",
            padding: "12px 16px",
            "border-bottom": "1px solid var(--border)",
            gap: "8px",
          }}
        >
          <span
            style={{
              flex: "1",
              "font-size": "13px",
              "font-weight": "600",
              color: "var(--text-primary)",
            }}
          >
            Select Build Variant
          </span>

          {/* Refresh button */}
          <button
            onClick={handleRefresh}
            disabled={variantState.gradleLoading}
            title="Re-detect variants from Gradle"
            style={{
              display: "flex",
              "align-items": "center",
              "justify-content": "center",
              width: "24px",
              height: "24px",
              background: "transparent",
              border: "none",
              "border-radius": "4px",
              cursor: variantState.gradleLoading ? "not-allowed" : "pointer",
              opacity: variantState.gradleLoading ? "0.4" : "1",
              color: "var(--text-muted)",
            }}
            onMouseEnter={(e) => {
              if (!variantState.gradleLoading)
                (e.currentTarget as HTMLElement).style.background = "var(--bg-hover, rgba(255,255,255,0.08))";
            }}
            onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "transparent"; }}
          >
            <Show
              when={variantState.gradleLoading}
              fallback={<Icon name="refresh" size={13} />}
            >
              <span class="lsp-spinner" style={{ "line-height": "0" }}>
                <Icon name="spinner" size={13} color="var(--accent, #60a5fa)" />
              </span>
            </Show>
          </button>
        </div>

        {/* ── Search ── */}
        <div style={{ padding: "8px 12px", "border-bottom": "1px solid var(--border)" }}>
          <input
            type="text"
            placeholder="Filter variants…"
            value={search()}
            onInput={(e) => setSearch(e.currentTarget.value)}
            // eslint-disable-next-line solid/reactivity
            ref={(el) => setTimeout(() => el?.focus(), 50)}
            style={{
              width: "100%",
              padding: "6px 10px",
              background: "var(--bg-quaternary, var(--bg-tertiary))",
              border: "1px solid var(--border)",
              "border-radius": "4px",
              color: "var(--text-primary)",
              "font-size": "12px",
              outline: "none",
              "box-sizing": "border-box",
            }}
          />
        </div>

        {/* ── Variant list ── */}
        <div style={{ "overflow-y": "auto", flex: "1" }}>
          <Show
            when={variantState.loading}
          >
            <div style={{ padding: "16px", color: "var(--text-muted)", "font-size": "12px", "text-align": "center" }}>
              Loading…
            </div>
          </Show>

          <Show when={!variantState.loading}>
            <Show
              when={filtered().length > 0}
              fallback={
                <Show
                  when={!variantState.gradleLoading}
                  fallback={
                    <div style={{ padding: "16px", color: "var(--text-muted)", "font-size": "12px", "text-align": "center" }}>
                      Waiting for Gradle…
                    </div>
                  }
                >
                  <div style={{ padding: "16px", color: "var(--text-muted)", "font-size": "12px", "text-align": "center" }}>
                    {search() ? "No variants match" : "No variants found"}
                  </div>
                </Show>
              }
            >
              <For each={filtered()}>
                {(v) => (
                  <button
                    onClick={() => handleSelect(v.name)}
                    style={{
                      display: "flex",
                      "align-items": "center",
                      "justify-content": "space-between",
                      padding: "10px 16px",
                      width: "100%",
                      background: variantState.activeVariant === v.name
                        ? "var(--bg-active, rgba(255,255,255,0.08))"
                        : "transparent",
                      border: "none",
                      "border-left": `3px solid ${variantState.activeVariant === v.name ? "var(--accent)" : "transparent"}`,
                      cursor: "pointer",
                      "text-align": "left",
                      gap: "12px",
                    }}
                    onMouseEnter={(e) => {
                      if (variantState.activeVariant !== v.name)
                        (e.currentTarget as HTMLElement).style.background = "var(--bg-hover, rgba(255,255,255,0.04))";
                    }}
                    onMouseLeave={(e) => {
                      if (variantState.activeVariant !== v.name)
                        (e.currentTarget as HTMLElement).style.background = "transparent";
                    }}
                  >
                    <div>
                      <div style={{ "font-size": "13px", color: "var(--text-primary)" }}>
                        {v.name}
                      </div>
                      <div style={{ "font-size": "10px", color: "var(--text-muted)", "margin-top": "2px" }}>
                        {v.assembleTask}
                      </div>
                    </div>
                    <Show when={variantState.activeVariant === v.name}>
                      <span style={{ color: "var(--accent)", "font-size": "14px", "flex-shrink": "0" }}>✓</span>
                    </Show>
                  </button>
                )}
              </For>
            </Show>
          </Show>
        </div>

        {/* ── Gradle status footer ── */}
        <Show when={statusLabel()}>
          <div
            style={{
              display: "flex",
              "align-items": "center",
              gap: "6px",
              padding: "7px 14px",
              "border-top": "1px solid var(--border)",
              "font-size": "11px",
              color: isError()
                ? "var(--error, #f87171)"
                : variantState.fromGradle
                ? "var(--success, #4ade80)"
                : "var(--text-muted)",
              "flex-shrink": "0",
              "overflow": "hidden",
            }}
          >
            <Show
              when={variantState.gradleLoading}
              fallback={
                <span style={{ opacity: "0.8", "flex-shrink": "0" }}>
                  {isError() ? "✗" : variantState.fromGradle ? "✓" : "ℹ"}
                </span>
              }
            >
              <span class="lsp-spinner" style={{ "line-height": "0", "flex-shrink": "0" }}>
                <Icon name="spinner" size={11} color="var(--accent, #60a5fa)" />
              </span>
            </Show>
            <span style={{ overflow: "hidden", "text-overflow": "ellipsis", "white-space": "nowrap" }}>
              {statusLabel()}
            </span>
          </div>
        </Show>
      </div>
    </div>
  );
}
