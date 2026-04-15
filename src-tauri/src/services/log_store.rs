use crate::models::logcat::{EntryFlags, LogStats, ProcessedEntry};
use crate::models::settings::{LOGCAT_RING_ABS_MAX, LOGCAT_RING_DEFAULT, LOGCAT_RING_MIN};
use std::collections::VecDeque;

/// Hard ceiling for logcat ring size (must match `settings::LOGCAT_RING_ABS_MAX`).
pub const LOGCAT_RING_ABS_MAX_ENTRIES: usize = LOGCAT_RING_ABS_MAX as usize;

/// Default ring capacity when settings are unavailable.
pub const LOGCAT_RING_DEFAULT_CAPACITY: usize = LOGCAT_RING_DEFAULT as usize;

/// Minimum ring capacity (must match `settings::LOGCAT_RING_MIN`).
pub const LOGCAT_RING_MIN_ENTRIES: usize = LOGCAT_RING_MIN as usize;

/// Pre-allocation hint: large enough that a busy app rarely reallocates early on.
const INITIAL_CAPACITY_MAX: usize = 10_000;

// ── LogStore ──────────────────────────────────────────────────────────────────

/// An indexed ring buffer for logcat entries.
///
/// Primary storage is a `VecDeque<ProcessedEntry>` with a configurable
/// capacity cap.  In addition to the main buffer, two secondary indexes
/// (crash_ids, json_ids) hold the IDs of entries with those flags set,
/// enabling O(1) jump-to-crash and O(log n) pagination over crashes.
///
/// Maintaining a full secondary index for every level/tag combination would
/// create eviction complexity at high throughput, so only the two most
/// common targeted lookups get dedicated indexes.  Level/tag/package
/// filtering is done as a sequential Rust scan (< 1 ms for 50 K entries).
pub struct LogStore {
    entries: VecDeque<ProcessedEntry>,
    capacity: usize,

    /// IDs of entries whose flags have `EntryFlags::CRASH` set.
    crash_ids: VecDeque<u64>,
    /// IDs of entries whose flags have `EntryFlags::JSON_BODY` set.
    json_ids: VecDeque<u64>,

    pub stats: LogStats,
}

impl LogStore {
    pub fn new() -> Self {
        Self::with_capacity(LOGCAT_RING_DEFAULT_CAPACITY)
    }

    /// Ring buffer capacity (oldest entries are evicted once `len == capacity`).
    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity
            .clamp(LOGCAT_RING_MIN_ENTRIES, LOGCAT_RING_ABS_MAX_ENTRIES);
        let initial = usize::min(INITIAL_CAPACITY_MAX, capacity).max(256);
        LogStore {
            entries: VecDeque::with_capacity(initial),
            capacity,
            crash_ids: VecDeque::new(),
            json_ids: VecDeque::new(),
            stats: LogStats::default(),
        }
    }

    /// Configured maximum entries before eviction (ring size).
    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Change ring capacity. When shrinking, evicts oldest entries until `len <= new_cap`.
    pub fn set_capacity(&mut self, new_cap: usize) {
        let new_cap = new_cap.clamp(LOGCAT_RING_MIN_ENTRIES, LOGCAT_RING_ABS_MAX_ENTRIES);
        if new_cap == self.capacity {
            return;
        }
        while self.entries.len() > new_cap {
            if let Some(evicted) = self.entries.pop_front() {
                self.remove_from_indexes(evicted.id, evicted.flags);
            }
        }
        self.capacity = new_cap;
    }

    /// Insert a processed entry into the store.
    ///
    /// If the buffer is at capacity, the oldest entry is evicted first.
    /// Secondary indexes and stats are updated inline.
    #[inline]
    pub fn push(&mut self, entry: ProcessedEntry) {
        // Evict oldest when at capacity, removing from indexes too.
        if self.entries.len() >= self.capacity {
            if let Some(evicted) = self.entries.pop_front() {
                self.remove_from_indexes(evicted.id, evicted.flags);
            }
        }

        // Update secondary indexes.
        if entry.flags & EntryFlags::CRASH != 0 {
            self.crash_ids.push_back(entry.id);
        }
        if entry.flags & EntryFlags::JSON_BODY != 0 {
            self.json_ids.push_back(entry.id);
        }

        // Update stats.
        self.stats.total_ingested += 1;
        let level_idx = entry.level.priority() as usize;
        if level_idx < self.stats.counts_by_level.len() {
            self.stats.counts_by_level[level_idx] += 1;
        }
        if entry.flags & EntryFlags::CRASH != 0 {
            self.stats.crash_count += 1;
        }
        if entry.flags & EntryFlags::JSON_BODY != 0 {
            self.stats.json_count += 1;
        }
        if let Some(pkg) = &entry.package {
            if !pkg.is_empty() {
                // packages_seen is updated separately via known_packages in state
                let _ = pkg;
            }
        }

        self.entries.push_back(entry);
    }

    /// Push a batch of entries efficiently.
    pub fn push_batch(&mut self, entries: Vec<ProcessedEntry>) {
        for entry in entries {
            self.push(entry);
        }
    }

    /// Clear all entries, indexes, and stats.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.crash_ids.clear();
        self.json_ids.clear();
        self.stats = LogStats::default();
    }

    /// Number of entries currently in the buffer.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate entries (oldest first).
    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, ProcessedEntry> {
        self.entries.iter()
    }

    /// Return the most recent `limit` entries that match the filter,
    /// returned in chronological order (oldest first).
    pub fn query(
        &self,
        filter: &crate::services::logcat::LogcatFilter,
        limit: usize,
    ) -> Vec<ProcessedEntry> {
        // Collect newest-first (rev), then reverse in-place.
        // Avoids the double-collect pattern of .collect().into_iter().rev().collect().
        let mut result: Vec<ProcessedEntry> = self
            .entries
            .iter()
            .rev()
            .filter(|e| filter.matches(e))
            .take(limit)
            .cloned()
            .collect();
        result.reverse();
        result
    }

    /// Return all crash entry IDs in ascending order.
    pub fn crash_ids(&self) -> &VecDeque<u64> {
        &self.crash_ids
    }

    /// Return all JSON entry IDs in ascending order.
    pub fn json_ids(&self) -> &VecDeque<u64> {
        &self.json_ids
    }

    // Remove evicted entry's ID from the secondary indexes.
    //
    // IDs are monotonically increasing and entries always evict oldest-first
    // (VecDeque is FIFO). Therefore the evicted ID, if present in an index,
    // is guaranteed to be at the *front* of that index deque. O(1) pop instead
    // of O(n) linear scan.
    fn remove_from_indexes(&mut self, id: u64, flags: u32) {
        if flags & EntryFlags::CRASH != 0
            && self.crash_ids.front() == Some(&id) {
                self.crash_ids.pop_front();
            }
        if flags & EntryFlags::JSON_BODY != 0
            && self.json_ids.front() == Some(&id) {
                self.json_ids.pop_front();
            }
    }
}

impl Default for LogStore {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::logcat::{EntryCategory, LogcatKind, LogcatLevel};

    fn make_entry(id_hint: u64, flags: u32) -> ProcessedEntry {
        ProcessedEntry {
            id: id_hint,
            timestamp: "01-01 00:00:00.000".into(),
            pid: 1,
            tid: 1,
            level: LogcatLevel::Info,
            tag: "T".into(),
            message: "m".into(),
            package: None,
            kind: LogcatKind::Normal,
            is_crash: flags & EntryFlags::CRASH != 0,
            flags,
            category: EntryCategory::General,
            crash_group_id: None,
            json_body: None,
        }
    }

    #[test]
    fn push_indexes_crash_entries() {
        let mut store = LogStore::new();
        store.push(make_entry(1, EntryFlags::CRASH));
        store.push(make_entry(2, 0));
        assert_eq!(store.crash_ids().len(), 1);
        assert_eq!(*store.crash_ids().front().unwrap(), 1);
    }

    #[test]
    fn push_indexes_json_entries() {
        let mut store = LogStore::new();
        store.push(make_entry(1, EntryFlags::JSON_BODY));
        store.push(make_entry(2, 0));
        assert_eq!(store.json_ids().len(), 1);
    }

    #[test]
    fn eviction_removes_from_indexes() {
        let mut store = LogStore { capacity: 2, ..LogStore::with_capacity(2) };
        // entry 1: crash
        store.push(make_entry(1, EntryFlags::CRASH));
        store.push(make_entry(2, 0));
        assert_eq!(store.crash_ids().len(), 1);
        // Push 3rd entry — entry 1 should be evicted, removing from crash_ids
        store.push(make_entry(3, 0));
        assert_eq!(store.crash_ids().len(), 0, "crash entry should be removed on eviction");
    }

    #[test]
    fn set_capacity_shrink_evicts_oldest() {
        let mut store = LogStore::with_capacity(10_000);
        for i in 1..=2000 {
            store.push(make_entry(i, 0));
        }
        assert_eq!(store.len(), 2000);
        // Minimum ring size is LOGCAT_RING_MIN (1000); shrinking evicts oldest first.
        store.set_capacity(1000);
        assert_eq!(store.len(), 1000);
        assert_eq!(store.iter().next().unwrap().id, 1001);
        assert_eq!(store.iter().last().unwrap().id, 2000);
        assert_eq!(store.capacity(), 1000);
    }

    #[test]
    fn stats_track_totals() {
        let mut store = LogStore::new();
        store.push(make_entry(1, EntryFlags::CRASH));
        store.push(make_entry(2, EntryFlags::JSON_BODY));
        assert_eq!(store.stats.total_ingested, 2);
        assert_eq!(store.stats.crash_count, 1);
        assert_eq!(store.stats.json_count, 1);
    }

    #[test]
    fn clear_resets_everything() {
        let mut store = LogStore::new();
        store.push(make_entry(1, EntryFlags::CRASH | EntryFlags::JSON_BODY));
        store.clear();
        assert!(store.is_empty());
        assert!(store.crash_ids().is_empty());
        assert!(store.json_ids().is_empty());
        assert_eq!(store.stats.total_ingested, 0);
    }
}
