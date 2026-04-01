/**
 * logcat-filter-storage.ts
 *
 * Persistence layer for logcat saved filters and last-active query.
 * Stores everything in localStorage under a single versioned key.
 *
 * Responsibilities:
 *   • Save / load / delete user-defined filter presets
 *   • Persist the last active query so it is restored on next panel mount
 *   • Migrate legacy `logcat_presets_v1` data on first access
 *   • Enforce a bounded cap on the number of saved filters
 */

// ── Public types ──────────────────────────────────────────────────────────────

export interface SavedFilter {
  /** Stable identifier generated at creation time. */
  id: string;
  /** Human-readable label shown in the presets dropdown. */
  name: string;
  /** The raw query string (may include `|`-separated OR groups). */
  query: string;
  /** Unix ms timestamp when this filter was saved. */
  createdAt: number;
}

/** The full shape stored under `STORAGE_KEY`. */
export interface FilterStorage {
  filters: SavedFilter[];
  lastActiveQuery: string;
}

// ── Constants ─────────────────────────────────────────────────────────────────

const STORAGE_KEY = "logcat_saved_filters_v1";
const LEGACY_KEY = "logcat_presets_v1";

/** Maximum number of user-saved filters (bounded collection). */
export const MAX_SAVED_FILTERS = 50;

// ── Legacy types (migration only) ────────────────────────────────────────────

interface LegacyPreset {
  name: string;
  query: string;
  builtin?: true;
}

// ── Internal helpers ──────────────────────────────────────────────────────────

function generateId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  // Fallback for environments without crypto.randomUUID (e.g. older jsdom)
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

function readRaw(): FilterStorage {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { filters: [], lastActiveQuery: "" };
    const parsed = JSON.parse(raw) as Partial<FilterStorage>;
    return {
      filters: Array.isArray(parsed.filters) ? parsed.filters : [],
      lastActiveQuery: typeof parsed.lastActiveQuery === "string" ? parsed.lastActiveQuery : "",
    };
  } catch {
    return { filters: [], lastActiveQuery: "" };
  }
}

function writeRaw(storage: FilterStorage): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(storage));
  } catch {
    // Silently ignore quota errors — filter saving is best-effort
  }
}

/** Migrate legacy `logcat_presets_v1` user presets to the new schema. */
function migrateLegacy(storage: FilterStorage): FilterStorage {
  try {
    const raw = localStorage.getItem(LEGACY_KEY);
    if (!raw) return storage;
    const legacy = JSON.parse(raw) as LegacyPreset[];
    if (!Array.isArray(legacy)) return storage;

    const migrated: SavedFilter[] = legacy
      .filter((p) => !p.builtin && typeof p.name === "string" && typeof p.query === "string")
      .map((p) => ({
        id: generateId(),
        name: p.name,
        query: p.query,
        createdAt: Date.now(),
      }));

    if (migrated.length === 0) {
      localStorage.removeItem(LEGACY_KEY);
      return storage;
    }

    // Merge: existing filters first, then migrated ones (dedup by name)
    const existingNames = new Set(storage.filters.map((f) => f.name));
    const toAdd = migrated.filter((f) => !existingNames.has(f.name));
    const merged = [...storage.filters, ...toAdd].slice(0, MAX_SAVED_FILTERS);

    localStorage.removeItem(LEGACY_KEY);
    return { ...storage, filters: merged };
  } catch {
    return storage;
  }
}

// ── Public API ────────────────────────────────────────────────────────────────

/**
 * Load the complete filter storage, running legacy migration if needed.
 * Always returns a valid `FilterStorage` object even if localStorage is
 * unavailable or corrupted.
 */
export function loadFilterStorage(): FilterStorage {
  const storage = readRaw();
  // Only attempt migration when there are no saved filters yet (first access
  // after upgrade) to avoid redundant localStorage reads on every mount.
  const needsMigration = storage.filters.length === 0 && localStorage.getItem(LEGACY_KEY) !== null;
  if (!needsMigration) return storage;
  const migrated = migrateLegacy(storage);
  // Persist immediately so subsequent readRaw() calls see the migrated data.
  writeRaw(migrated);
  return migrated;
}

/**
 * Persist the complete storage object.
 * Callers should treat this as a low-cost synchronous write.
 */
export function saveFilterStorage(storage: FilterStorage): void {
  const capped: FilterStorage = {
    ...storage,
    filters: storage.filters.slice(0, MAX_SAVED_FILTERS),
  };
  writeRaw(capped);
}

/**
 * Add a new saved filter.
 * If the name already exists, the existing entry is overwritten in-place
 * (updates query + createdAt) rather than creating a duplicate.
 * Returns the created or updated `SavedFilter`.
 * Throws if the cap would be exceeded with a new entry.
 */
export function addSavedFilter(name: string, query: string): SavedFilter {
  const trimmedName = name.trim();
  if (!trimmedName) throw new Error("Filter name must not be empty");

  const storage = loadFilterStorage();
  const existing = storage.filters.find((f) => f.name === trimmedName);

  if (existing) {
    const updated: SavedFilter = { ...existing, query, createdAt: Date.now() };
    saveFilterStorage({
      ...storage,
      filters: storage.filters.map((f) => (f.id === existing.id ? updated : f)),
    });
    return updated;
  }

  if (storage.filters.length >= MAX_SAVED_FILTERS) {
    throw new Error(`Cannot save more than ${MAX_SAVED_FILTERS} filters`);
  }

  const filter: SavedFilter = {
    id: generateId(),
    name: trimmedName,
    query,
    createdAt: Date.now(),
  };

  saveFilterStorage({ ...storage, filters: [...storage.filters, filter] });
  return filter;
}

/**
 * Delete a saved filter by its `id`.
 * Silently does nothing if the id is not found.
 */
export function deleteSavedFilter(id: string): void {
  const storage = loadFilterStorage();
  saveFilterStorage({
    ...storage,
    filters: storage.filters.filter((f) => f.id !== id),
  });
}

/**
 * Rename a saved filter.
 * If `newName` already belongs to a different filter, the call is ignored
 * to avoid duplicates.
 */
export function renameSavedFilter(id: string, newName: string): void {
  const trimmedName = newName.trim();
  if (!trimmedName) return;

  const storage = loadFilterStorage();
  const collision = storage.filters.find((f) => f.name === trimmedName && f.id !== id);
  if (collision) return;

  saveFilterStorage({
    ...storage,
    filters: storage.filters.map((f) => (f.id === id ? { ...f, name: trimmedName } : f)),
  });
}

/**
 * Retrieve the last query the user had active in the logcat panel.
 * Returns empty string if nothing was persisted.
 */
export function getLastActiveQuery(): string {
  return readRaw().lastActiveQuery;
}

/**
 * Persist the current active query so it can be restored on next mount.
 * This is a cheap write — only the `lastActiveQuery` field is updated.
 */
export function setLastActiveQuery(query: string): void {
  const storage = readRaw();
  writeRaw({ ...storage, lastActiveQuery: query });
}
