import { createStore } from "solid-js/store";
import type { BuildVariant, VariantList } from "@/bindings";
import {
  getVariantsPreview,
  getVariantsFromGradle,
  setActiveVariant,
  formatError,
} from "@/lib/tauri-api";
import { showToast } from "@/components/ui";
import { projectState } from "@/stores/project.store";

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

// ── Session cache ─────────────────────────────────────────────────────────────
// Keyed by project root path. Survives project switches and is bounded: past
// 8 distinct roots the oldest-inserted entry is evicted (FIFO), so switching
// back to an evicted project reruns the expensive `./gradlew :app:tasks`
// query. Fully cleared on app restart (module re-initialisation).
export interface CachedGradleVariants {
  variants: BuildVariant[];
  defaultVariant: string | null;
}

interface VariantCache {
  get(root: string): CachedGradleVariants | undefined;
  set(root: string, entry: CachedGradleVariants): void;
  delete(root: string): void;
  clear(): void;
  readonly size: number;
}

/**
 * FIFO-bounded cache keyed by project root, mirroring `createLogStore`'s
 * injectable-cap pattern so tests can use a small cap.
 */
export function createVariantCache(options: { maxEntries: number }): VariantCache {
  const maxEntries = Math.max(0, Math.floor(options.maxEntries));
  const map = new Map<string, CachedGradleVariants>();
  return {
    get size() {
      return map.size;
    },
    get: (root) => map.get(root),
    set: (root, entry) => {
      map.delete(root);
      map.set(root, entry);
      while (map.size > maxEntries) {
        const oldest = map.keys().next().value;
        if (oldest === undefined) break;
        map.delete(oldest);
      }
    },
    delete: (root) => map.delete(root),
    clear: () => map.clear(),
  };
}

const variantCache = createVariantCache({ maxEntries: 8 });

/** Clear the cache for a specific root (or all roots when called with no argument). */
export function clearVariantCache(root?: string): void {
  if (root !== undefined) {
    variantCache.delete(root);
  } else {
    variantCache.clear();
  }
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

export function activeVariantObj(): BuildVariant | null {
  return variantState.variants.find((v) => v.name === variantState.activeVariant) ?? null;
}

export function hasVariants(): boolean {
  return variantState.variants.length > 0;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/** Pick the active variant: respect saved setting, Gradle default hint, then first in list. */
function resolveActive(list: VariantList, currentActive: string | null): string | null {
  // Honour the saved/restored setting if it's present in the returned list.
  if (list.active && list.variants.some((v) => v.name === list.active)) {
    return list.active;
  }
  // Keep the currently-selected variant if it still exists after a refresh.
  if (currentActive && list.variants.some((v) => v.name === currentActive)) {
    return currentActive;
  }
  // Android Studio–aligned default from Gradle (`isDefault`, `.idea`, heuristics).
  if (list.defaultVariant && list.variants.some((v) => v.name === list.defaultVariant)) {
    return list.defaultVariant;
  }
  // Fall back to the first variant.
  return list.variants[0]?.name ?? null;
}

// ── Actions ───────────────────────────────────────────────────────────────────

/** Coalesces concurrent callers for the same project root onto one Gradle run. */
const loadVariantsPending = new Map<string, Promise<void>>();

/**
 * Load variants using a two-phase approach:
 *
 * 1. **Preview** (instant): parses build.gradle statically, populates the UI
 *    immediately with whatever is explicitly declared.
 * 2. **Gradle** (authoritative): runs `./gradlew :app:tasks --console=plain`,
 *    gets every variant the project actually has (including implicit `debug`,
 *    custom types, product flavors), then replaces the preview list. The result
 *    is cached in memory for the session — subsequent switches back to the same
 *    project skip the Gradle invocation entirely.
 *
 * Both phases update the store independently so the UI is always responsive.
 *
 * Pass `{ force: true }` to bypass the cache (e.g. the Refresh button).
 */
export function loadVariants(opts?: { force?: boolean }): Promise<void> {
  const root = projectState.projectRoot;
  if (opts?.force) {
    if (root !== null) variantCache.delete(root);
  }
  const pendingKey = root ?? "__no_project__";
  const pending = loadVariantsPending.get(pendingKey);
  if (pending) return pending;

  const next = runLoadVariants(root).finally(() => {
    loadVariantsPending.delete(pendingKey);
  });
  loadVariantsPending.set(pendingKey, next);
  return next;
}

function isCurrentProject(root: string | null): boolean {
  return projectState.projectRoot === root;
}

async function runLoadVariants(rootAtStart: string | null): Promise<void> {
  if (isCurrentProject(rootAtStart)) {
    setVariantState({
      loading: true,
      gradleLoading: true,
      error: null,
      gradleError: null,
      fromGradle: false,
    });
  }

  // ── Phase 1: instant preview from static parse ─────────────────────────────
  try {
    const preview = await getVariantsPreview();
    if (!isCurrentProject(rootAtStart)) return;
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
    if (isCurrentProject(rootAtStart)) {
      setVariantState({ loading: false });
    }
  }

  // ── Phase 2: authoritative list from Gradle (or session cache) ───────────────
  const cacheKey = rootAtStart;
  const cached = cacheKey !== null ? variantCache.get(cacheKey) : undefined;

  if (cached) {
    if (!isCurrentProject(rootAtStart)) return;
    setVariantState({
      variants: cached.variants,
      activeVariant: resolveActive(
        {
          variants: cached.variants,
          active: null,
          defaultVariant: cached.defaultVariant,
        },
        variantState.activeVariant
      ),
      fromGradle: true,
      gradleLoading: false,
      gradleError: null,
      error: null,
    });
    return;
  }

  try {
    const full = await getVariantsFromGradle();
    if (!isCurrentProject(rootAtStart)) return;
    if (cacheKey !== null) {
      variantCache.set(cacheKey, {
        variants: full.variants,
        defaultVariant: full.defaultVariant,
      });
    }
    setVariantState({
      variants: full.variants,
      activeVariant: resolveActive(full, variantState.activeVariant),
      fromGradle: true,
      gradleLoading: false,
      gradleError: null,
      error: null,
    });
  } catch (e) {
    if (!isCurrentProject(rootAtStart)) return;
    const msg = typeof e === "string" ? e : ((e as Error).message ?? String(e));
    setVariantState({
      gradleLoading: false,
      gradleError: msg,
      // Fatal error only when we have nothing at all to show.
      error: variantState.variants.length === 0 ? msg : null,
    });
  }
}

export async function selectVariant(name: string): Promise<void> {
  const previous = variantState.activeVariant;
  setVariantState("activeVariant", name);
  try {
    await setActiveVariant(name);
  } catch (err) {
    // The backend persists last_build_variant for this project (the same
    // state the MCP set_active_variant tool writes), so a silent failure
    // would leave settings diverged from the UI — worse than a visible error.
    setVariantState("activeVariant", previous);
    showToast(`Failed to select variant: ${formatError(err)}`, "error");
    return;
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
