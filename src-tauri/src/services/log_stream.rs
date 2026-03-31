use crate::models::logcat::ProcessedEntry;
use crate::services::logcat::LogcatFilter;

// ── StreamState ───────────────────────────────────────────────────────────────

/// Holds the active filter for backend-side stream filtering.
///
/// Stored inside `LogcatStateInner`. The batcher task reads this every tick
/// to decide which processed entries to forward to the frontend.
///
/// When `active_filter` is `None`, all entries are forwarded (no filtering).
#[derive(Default)]
pub struct StreamState {
    pub active_filter: Option<LogcatFilter>,
}

impl StreamState {
    pub fn new() -> Self {
        StreamState { active_filter: None }
    }

    /// Update the active filter.  Passing `None` disables filtering.
    pub fn set_filter(&mut self, filter: Option<LogcatFilter>) {
        self.active_filter = filter;
    }

    /// Clone the active filter for use in the batcher task without holding
    /// the LogcatState lock during emit.
    pub fn clone_filter(&self) -> Option<LogcatFilter> {
        self.active_filter.clone()
    }

    /// Apply the current filter to a batch of processed entries, returning
    /// only those that match.  If no filter is active, returns the full batch.
    pub fn filter_batch(&self, batch: Vec<ProcessedEntry>) -> Vec<ProcessedEntry> {
        match &self.active_filter {
            None => batch,
            Some(f) => batch.into_iter().filter(|e| f.matches(e)).collect(),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::logcat::{EntryCategory, LogcatKind, LogcatLevel};
    use crate::services::logcat::LogcatFilter;

    fn make_entry(level: LogcatLevel, tag: &str, msg: &str) -> ProcessedEntry {
        ProcessedEntry {
            id: 1,
            timestamp: "".into(),
            pid: 0,
            tid: 0,
            level,
            tag: tag.into(),
            message: msg.into(),
            package: None,
            kind: LogcatKind::Normal,
            is_crash: false,
            flags: 0,
            category: EntryCategory::General,
            crash_group_id: None,
            json_body: None,
        }
    }

    #[test]
    fn no_filter_passes_all() {
        let state = StreamState::new();
        let batch = vec![
            make_entry(LogcatLevel::Debug, "T", "m"),
            make_entry(LogcatLevel::Verbose, "T", "m"),
        ];
        let result = state.filter_batch(batch);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn filter_by_level_removes_low_priority() {
        let mut state = StreamState::new();
        state.set_filter(Some(LogcatFilter::new(
            Some(LogcatLevel::Error),
            None,
            None,
            None,
            false,
        )));

        let batch = vec![
            make_entry(LogcatLevel::Debug, "T", "m"),
            make_entry(LogcatLevel::Error, "T", "m"),
        ];
        let result = state.filter_batch(batch);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].level, LogcatLevel::Error);
    }

    #[test]
    fn set_filter_to_none_removes_filter() {
        let mut state = StreamState::new();
        state.set_filter(Some(LogcatFilter::new(
            Some(LogcatLevel::Fatal),
            None,
            None,
            None,
            false,
        )));
        state.set_filter(None);

        let batch = vec![make_entry(LogcatLevel::Verbose, "T", "m")];
        let result = state.filter_batch(batch);
        assert_eq!(result.len(), 1);
    }
}
