//! The unified diff (`diff -u`) of two texts.

use std::fmt::Write as _;

use super::super::diff::{DiffEdit, middle_edits, normalize_newlines};

/// The unified diff of `baseline` against `current`, as `diff -u` writes it,
/// with `context` unchanged lines around each change: `--- baseline` and
/// `+++ current` headers, then `@@ -l,n +l,n @@` hunks. Empty when the texts
/// are the same.
///
/// Line endings are compared as LF (the editor's own); a last line without
/// a newline is marked `\ No newline at end of file`, so the patch applies
/// with `patch` or `git apply` (after renaming the headers to the file).
///
/// ```
/// use config_editor_leptos::unified_diff;
///
/// let patch = unified_diff("listen: 7835\nlog: info\n", "listen: 7836\nlog: info\n", 3);
/// assert_eq!(
///     patch,
///     "--- baseline\n+++ current\n@@ -1,2 +1,2 @@\n-listen: 7835\n+listen: 7836\n log: info\n"
/// );
/// ```
pub fn unified_diff(baseline: &str, current: &str, context: usize) -> String {
    let baseline = normalize_newlines(baseline);
    let current = normalize_newlines(current);
    // Lines keep their newline: a last line without one differs from the
    // same text with one, as in `diff`.
    let old: Vec<&str> = baseline.split_inclusive('\n').collect();
    let new: Vec<&str> = current.split_inclusive('\n').collect();
    let ops = line_ops(&old, &new);
    let mut out = String::new();
    for range in hunk_ranges(&ops, context) {
        if out.is_empty() {
            out.push_str("--- baseline\n+++ current\n");
        }
        write_hunk(&mut out, &ops[range], &old, &new);
    }
    out
}

/// One line of the edit script with the indices it consumes.
#[derive(Clone, Copy)]
struct Op {
    edit: DiffEdit,
    old: usize,
    new: usize,
}

/// The whole edit script: the common prefix and suffix around the middle.
fn line_ops(old: &[&str], new: &[&str]) -> Vec<Op> {
    let prefix = old
        .iter()
        .zip(new)
        .take_while(|(left, right)| left == right)
        .count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    let middle = middle_edits(
        &old[prefix..old.len() - suffix],
        &new[prefix..new.len() - suffix],
    );
    let edits = std::iter::repeat_n(DiffEdit::Equal, prefix)
        .chain(middle)
        .chain(std::iter::repeat_n(DiffEdit::Equal, suffix));
    let (mut old_index, mut new_index) = (0, 0);
    edits
        .map(|edit| {
            let op = Op {
                edit,
                old: old_index,
                new: new_index,
            };
            match edit {
                DiffEdit::Equal => {
                    old_index += 1;
                    new_index += 1;
                }
                DiffEdit::Delete => old_index += 1,
                DiffEdit::Insert => new_index += 1,
            }
            op
        })
        .collect()
}

/// The op ranges each hunk prints: changes with `context` lines around them,
/// merged when their contexts touch.
fn hunk_ranges(ops: &[Op], context: usize) -> Vec<std::ops::Range<usize>> {
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    let changes = ops
        .iter()
        .enumerate()
        .filter(|(_, op)| op.edit != DiffEdit::Equal)
        .map(|(index, _)| index);
    for index in changes {
        let start = index.saturating_sub(context);
        let end = (index + 1 + context).min(ops.len());
        match ranges.last_mut() {
            Some(last) if start <= last.end => last.end = end,
            _ => ranges.push(start..end),
        }
    }
    ranges
}

fn write_hunk(out: &mut String, ops: &[Op], old: &[&str], new: &[&str]) {
    let old_count = ops.iter().filter(|op| op.edit != DiffEdit::Insert).count();
    let new_count = ops.iter().filter(|op| op.edit != DiffEdit::Delete).count();
    let first = ops[0];
    let _ = writeln!(
        out,
        "@@ -{} +{} @@",
        range_label(first.old, old_count),
        range_label(first.new, new_count)
    );
    for op in ops {
        let (sign, line) = match op.edit {
            DiffEdit::Equal => (' ', new[op.new]),
            DiffEdit::Delete => ('-', old[op.old]),
            DiffEdit::Insert => ('+', new[op.new]),
        };
        out.push(sign);
        out.push_str(line);
        if !line.ends_with('\n') {
            out.push_str("\n\\ No newline at end of file\n");
        }
    }
}

/// `start,count` as `diff -u` writes it: one-based, the count left out when
/// it is 1, and the line before for an empty range.
fn range_label(start: usize, count: usize) -> String {
    match count {
        0 => format!("{start},0"),
        1 => format!("{}", start + 1),
        _ => format!("{},{count}", start + 1),
    }
}
