//! [`ChangeSummary`], the type of the `on_change_summary` prop: present in
//! every build, as the prop is; the `diff` feature counts them (`changes`).

/// How the current text differs from the baseline, by lines, as the diff
/// gutter shows it: what `on_change_summary` reports. Counting needs the
/// `diff` feature; without it, nothing reports one.
///
/// ```
/// # #[cfg(feature = "diff")] {
/// use config_editor_leptos::ChangeSummary;
///
/// let summary = ChangeSummary::between("a: 1\nb: 2\nc: 3\n", "a: 1\nb: 20\nc: 3\nd: 4\n");
/// assert_eq!((summary.added, summary.modified, summary.removed), (1, 1, 0));
/// assert_eq!(summary.hunks, 2);
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
