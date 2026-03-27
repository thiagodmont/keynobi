import { createStore } from "solid-js/store";
import { createMemo } from "solid-js";
import type { BuildVariant, VariantList } from "@/bindings";
import { getVariantsPreview, getVariantsFromGradle, setActiveVariant } from "@/lib/tauri-api";

// ── Types ─────────────────────────────────────────────────────────────────────

export interface VariantStoreState {
  variants: BuildVariant[];
  activeVariant: string | null;
  /** True while the initial preview/parse is running. */
  loading: boolean;
  /** True while the Gradle task query is running (may take a few seconds). */
  gradleLoading: boolean;
  /** True once the variant list was populated from the real Gradle query. */
  fromGradle: boolean;
  /** Fatal error when no variants at all were found. */
  error: string | null;
  /** Gradle-specific error shown in the picker footer even when preview has results. */
  gradleError: string | null;
}

// ── State ─────────────────────────────────────────────────────────────────────

const [variantState, setVariantState] = createStore<VariantStoreState>({
  variants: [],
  activeVariant: null,
  loading: false,
  gradleLoading: false,
  fromGradle: false,
  error: null,
  gradleError: null,
});

export { variantState };

// ── Derived ───────────────────────────────────────────────────────────────────

export const activeVariantObj = createMemo(() =>
  variantState.variants.find((v) => v.name === variantState.activeVariant) ?? null
);

export const hasVariants = createMemo(() => variantState.variants.length > 0);

// ── Helpers ───────────────────────────────────────────────────────────────────

/** Pick the active variant: respect saved setting, fall back to first in list. */
function resolveActive(list: VariantList, currentActive: string | null): string | null {
  // Honour the saved/restored setting if it's present in the returned list.
  if (list.active && list.variants.some((v) => v.name === list.active)) {
    return list.active;
  }
  // Keep the currently-selected variant if it still exists after a refresh.
  if (currentActive && list.variants.some((v) => v.name === currentActive)) {
    return currentActive;
  }
  // Fall back to the first variant.
  return list.variants[0]?.name ?? null;
}

// ── Actions ───────────────────────────────────────────────────────────────────

/**
 * Load variants using a two-phase approach:
 *
 * 1. **Preview** (instant): parses build.gradle statically, populates the UI
 *    immediately with whatever is explicitly declared.
 * 2. **Gradle** (authoritative): runs `./gradlew :app:tasks --console=plain`,
 *    gets every variant the project actually has (including implicit `debug`,
 *    custom types, product flavors), then replaces the preview list.
 *
 * Both phases update the store independently so the UI is always responsive.
 */
export async function loadVariants(): Promise<void> {
  setVariantState({ loading: true, gradleLoading: true, error: null, gradleError: null, fromGradle: false });

  // ── Phase 1: instant preview from static parse ─────────────────────────────
  try {
    const preview = await getVariantsPreview();
    if (preview.variants.length > 0) {
      setVariantState({
        variants: preview.variants,
        activeVariant: resolveActive(preview, variantState.activeVariant),
        loading: false,
      });
    } else {
      setVariantState({ loading: false });
    }
  } catch {
    // Preview failure is non-fatal — Gradle query will still run.
    setVariantState({ loading: false });
  }

  // ── Phase 2: authoritative list from Gradle ────────────────────────────────
  try {
    const full = await getVariantsFromGradle();
    setVariantState({
      variants: full.variants,
      activeVariant: resolveActive(full, variantState.activeVariant),
      fromGradle: true,
      gradleLoading: false,
      gradleError: null,
      error: null,
    });
  } catch (e) {
    const msg = typeof e === "string" ? e : (e as Error).message ?? String(e);
    setVariantState({
      gradleLoading: false,
      gradleError: msg,
      // Fatal error only when we have nothing at all to show.
      error: variantState.variants.length === 0 ? msg : null,
    });
  }
}

export async function selectVariant(name: string): Promise<void> {
  setVariantState("activeVariant", name);
  try {
    await setActiveVariant(name);
  } catch {
    // Non-fatal — the in-memory selection is still updated.
  }
  // Notify the project service so it can persist per-project meta.
  _onVariantChange?.(name);
}

/** Registered by project.service.ts to avoid circular imports. */
let _onVariantChange: ((variant: string) => void) | null = null;
export function onVariantChange(cb: (variant: string) => void): void {
  _onVariantChange = cb;
}

export function clearVariants(): void {
  setVariantState({
    variants: [],
    activeVariant: null,
    error: null,
    gradleError: null,
    fromGradle: false,
    gradleLoading: false,
  });
}

export function resetVariantState(): void {
  setVariantState({
    variants: [],
    activeVariant: null,
    loading: false,
    gradleLoading: false,
    fromGradle: false,
    error: null,
    gradleError: null,
  });
}
