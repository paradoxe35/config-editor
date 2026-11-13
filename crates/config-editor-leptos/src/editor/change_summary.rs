//! [`ChangeSummary`], the type of the `on_change_summary` prop: present in
//! every build, as the prop is; the `diff` feature counts them (`changes`).

/// How the current text differs from the baseline, by lines, as the diff
/// gutter shows it: what `on_change_summary` reports. Counting needs the
/// `diff` feature; without it, nothing reports one.
///
/// A final line break added is the empty last line the editor shows added;
/// one removed is no change (no line goes: the last one only loses its
/// break). [`unified_diff`](crate::unified_diff) writes both as `diff -u`
/// does, the last line changed and `\ No newline at end of file`.
///
/// ```
/// # #[cfg(feature = "diff")] {
/// use config_editor_leptos::{ChangeSummary, unified_diff};
///
/// let summary = ChangeSummary::between("a: 1\nb: 2\nc: 3\n", "a: 1\nb: 20\nc: 3\nd: 4\n");
/// assert_eq!((summary.added, summary.modified, summary.removed), (1, 1, 0));
/// assert_eq!(summary.hunks, 2);
///
/// let summary = ChangeSummary::between("a\nb", "a\nb\n");
/// assert_eq!((summary.added, summary.modified, summary.removed), (1, 0, 0));
/// assert_eq!(
///     unified_diff("a\nb", "a\nb\n", 0),
///     "--- baseline\n+++ current\n@@ -2 +2 @@\n-b\n\\ No newline at end of file\n+b\n"
/// );
/// # }
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ChangeSummary {
    /// Lines added.
    pub added: usize,
    /// Lines removed.
    pub removed: usize,
    /// Lines changed in place.
    pub modified: usize,
    /// Separate changes (runs of changed lines), each one a gutter marker
    /// group that "Revert this change" restores at once.
    pub hunks: usize,
}

impl ChangeSummary {
    /// Whether the texts have the same lines.
    pub fn is_empty(&self) -> bool {
        self.hunks == 0
    }
}
