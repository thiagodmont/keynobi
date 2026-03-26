import { createStore } from "solid-js/store";
import { createMemo } from "solid-js";
import type { BuildVariant, VariantList } from "@/bindings";
import { getBuildVariants, setActiveVariant } from "@/lib/tauri-api";

// ── Types ─────────────────────────────────────────────────────────────────────

export interface VariantStoreState {
  variants: BuildVariant[];
  activeVariant: string | null;
  loading: boolean;
  error: string | null;
}

// ── State ─────────────────────────────────────────────────────────────────────

const [variantState, setVariantState] = createStore<VariantStoreState>({
  variants: [],
  activeVariant: null,
  loading: false,
  error: null,
});

export { variantState };

// ── Derived ───────────────────────────────────────────────────────────────────

export const activeVariantObj = createMemo(() =>
  variantState.variants.find((v) => v.name === variantState.activeVariant) ?? null
);

export const hasVariants = createMemo(() => variantState.variants.length > 0);

// ── Actions ───────────────────────────────────────────────────────────────────

export async function loadVariants(): Promise<void> {
  setVariantState({ loading: true, error: null });
  try {
    const list: VariantList = await getBuildVariants();
    setVariantState({
      variants: list.variants,
      activeVariant: list.active ?? (list.variants[0]?.name ?? null),
      loading: false,
    });
  } catch (e) {
    setVariantState({
      loading: false,
      error: typeof e === "string" ? e : String(e),
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
}

export function clearVariants(): void {
  setVariantState({ variants: [], activeVariant: null, error: null });
}

export function resetVariantState(): void {
  setVariantState({
    variants: [],
    activeVariant: null,
    loading: false,
    error: null,
  });
}
