//! The gutter's line diff against the baseline, with the change previews
//! it opens and where they show.

use std::borrow::Cow;
use std::ops::Range;

use wasm_bindgen::JsCast;

use super::super::diff::{DiffEdit, DiffKind, myers_edits};
use super::super::metrics::{EditorMetrics, read_editor_metrics};
use super::super::positioning::{
    FALLBACK_VIEWPORT_HEIGHT, FALLBACK_VIEWPORT_WIDTH, POPUP_GAP, POPUP_MARGIN,
};

/// Which gutter marker a change preview belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::editor) enum PeekKind {
    Modified,
    Added,
    /// Lines removed before `line`.
    Removed,
    /// Lines removed after the last line (`line`).
    RemovedAtEnd,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::editor) struct DiffPeek {
    pub(in crate::editor) line: usize,
    pub(in crate::editor) kind: PeekKind,
    pub(in crate::editor) title: String,
    pub(in crate::editor) before: Vec<String>,
    pub(in crate::editor) after: Vec<String>,
}

/// One change against the baseline: a run of current lines that replaced
/// some baseline lines (either side may be empty).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::editor) struct Hunk {
    /// The current lines it covers, zero-based. Empty for lines only
    /// removed: they were before line `current.start`.
    pub(in crate::editor) current: Range<usize>,
    /// The baseline lines those replaced, exactly: reverting writes them back.
    pub(in crate::editor) baseline: Vec<String>,
    pub(in crate::editor) added: usize,
    pub(in crate::editor) modified: usize,
    pub(in crate::editor) removed: usize,
}

/// Whether an open change preview still shows live content after an edit:
/// the same deleted hunk, or a still-changed line whose text matches the
/// snapshot (so a line inserted above, shifting the anchor, dismisses it).
pub(in crate::editor) fn peek_survives_edit(
    peek: &DiffPeek,
    current: &[String],
    line_diff: &LineDiff,
) -> bool {
    let index = peek.line.saturating_sub(1);
    let line_is = |kind: DiffKind| {
        line_diff
            .current
            .get(index)
            .copied()
            .unwrap_or(DiffKind::Unchanged)
            == kind
            && current.get(index) == peek.after.first()
    };
    match peek.kind {
        PeekKind::Modified => line_is(DiffKind::Modified),
        PeekKind::Added => line_is(DiffKind::Added),
        PeekKind::Removed => {
            line_diff
                .deleted_before
                .get(index)
                .copied()
                .unwrap_or(false)
                && line_diff.deleted_lines_before.get(index) == Some(&peek.before)
        }
        PeekKind::RemovedAtEnd => {
            peek.line == current.len()
                && line_diff.deleted_after
                && line_diff.deleted_lines_after == peek.before
        }
    }
}

#[derive(Clone, PartialEq)]
pub(in crate::editor) struct LineDiff {
    pub(in crate::editor) current: Vec<DiffKind>,
    pub(in crate::editor) deleted_before: Vec<bool>,
    pub(in crate::editor) deleted_lines_before: Vec<Vec<String>>,
    pub(in crate::editor) modified_before: Vec<Option<String>>,
    pub(in crate::editor) deleted_after: bool,
    pub(in crate::editor) deleted_lines_after: Vec<String>,
    /// The changes, in document order.
    pub(in crate::editor) hunks: Vec<Hunk>,
}

impl LineDiff {
    /// Every line unchanged, for `show_diff=false`.
    pub(in crate::editor) fn empty(line_count: usize) -> Self {
        Self {
            current: vec![DiffKind::Unchanged; line_count],
            deleted_before: vec![false; line_count],
            deleted_lines_before: vec![Vec::new(); line_count],
            modified_before: vec![None; line_count],
            deleted_after: false,
            deleted_lines_after: Vec::new(),
            hunks: Vec::new(),
        }
    }
}

/// DOM measurements for [`peek_geometry`], read by [`peek_popup_style`].
pub(in crate::editor) struct PeekViewport {
    pub(in crate::editor) viewport_width: f64,
    pub(in crate::editor) viewport_height: f64,
    pub(in crate::editor) editor_top: f64,
    /// Bottom of the text area; the peek stays above it so it never covers
    /// the footer or what follows the editor.
    pub(in crate::editor) editor_bottom: f64,
    pub(in crate::editor) editor_left: f64,
    pub(in crate::editor) editor_scroll_top: f64,
    pub(in crate::editor) root_top: f64,
    pub(in crate::editor) root_left: f64,
}

/// Change-preview header height, in pixels (sticky `h3` in `preset.css`).
pub(in crate::editor) const PEEK_HEADER_HEIGHT: f64 = 72.0;

/// Height of one diff line, in pixels; mirrors `.config-editor__diff-peek-line`.
pub(in crate::editor) const PEEK_LINE_HEIGHT: f64 = 22.0;

/// Change-preview height cap, in pixels; taller diffs scroll.
pub(in crate::editor) const PEEK_MAX_HEIGHT: f64 = 360.0;

/// Change-preview height cap as a fraction of the viewport height.
pub(in crate::editor) const PEEK_MAX_HEIGHT_VIEWPORT_FRACTION: f64 = 0.55;

/// Change-preview minimum width, in pixels; mirrors `preset.css`.
pub(in crate::editor) const PEEK_MIN_WIDTH: f64 = 180.0;

/// Change-preview preferred width, in pixels: two `minmax(240px, 1fr)`
/// columns plus padding.
pub(in crate::editor) const PEEK_PREFERRED_WIDTH: f64 = 560.0;

/// Shift, in pixels, aligning the preview with the gutter markers.
pub(in crate::editor) const PEEK_GUTTER_NUDGE: f64 = 5.0;

/// Preview placement: below the anchored line when it fits, otherwise above,
/// clamped to the viewport.
pub(in crate::editor) fn peek_geometry(
    viewport: &PeekViewport,
    metrics: &EditorMetrics,
    line: usize,
    old_line_count: usize,
    new_line_count: usize,
) -> String {
    let margin = POPUP_MARGIN;
    let anchor_y = viewport.editor_top
        + metrics.padding_top
        + line.saturating_sub(1) as f64 * metrics.line_height
        - viewport.editor_scroll_top;
    // The visible part of the text area bounds the peek.
    let area_top = viewport.editor_top.max(0.0) + margin;
    let area_bottom = viewport.editor_bottom.min(viewport.viewport_height) - margin;
    // Its line scrolled out of view: hide it rather than pin it to an edge.
    if anchor_y + metrics.line_height < area_top - margin || anchor_y > area_bottom + margin {
        return "visibility:hidden".to_string();
    }
    let space_above = (anchor_y - area_top).max(0.0);
    let space_below = (area_bottom - anchor_y - metrics.line_height).max(0.0);
    let desired_height = (PEEK_HEADER_HEIGHT
        + (old_line_count + new_line_count) as f64 * PEEK_LINE_HEIGHT)
        .min(PEEK_MAX_HEIGHT)
        .min(viewport.viewport_height * PEEK_MAX_HEIGHT_VIEWPORT_FRACTION);
    // Below when it fits there, else on the roomier side.
    let place_below = space_below >= desired_height || space_below >= space_above;
    let available_height = if place_below {
        space_below
    } else {
        space_above
    };
    let height = desired_height.min(available_height);
    let top_in_viewport = if place_below {
        anchor_y + metrics.line_height + POPUP_GAP
    } else {
        anchor_y - height - POPUP_GAP
    }
    .clamp(area_top, (area_bottom - height).max(area_top));
    let left_in_viewport = (viewport.editor_left + PEEK_GUTTER_NUDGE).clamp(
        margin,
        (viewport.viewport_width - PEEK_MIN_WIDTH - margin).max(margin),
    );
    let width =
        PEEK_PREFERRED_WIDTH.min((viewport.viewport_width - left_in_viewport - margin).max(0.0));

    format!(
        "top:{:.1}px;left:{:.1}px;width:{:.1}px;max-height:{:.1}px",
        top_in_viewport - viewport.root_top,
        left_in_viewport - viewport.root_left,
        width,
        height
    )
}

pub(in crate::editor) fn peek_popup_style(
    editor: &web_sys::HtmlTextAreaElement,
    root: &web_sys::HtmlDivElement,
    line: usize,
    old_line_count: usize,
    new_line_count: usize,
) -> String {
    let Some(window) = web_sys::window() else {
        return "visibility:hidden".to_string();
    };
    let viewport_width = window
        .inner_width()
        .ok()
        .and_then(|width| width.as_f64())
        .unwrap_or(FALLBACK_VIEWPORT_WIDTH);
    let viewport_height = window
        .inner_height()
        .ok()
        .and_then(|height| height.as_f64())
        .unwrap_or(FALLBACK_VIEWPORT_HEIGHT);
    let editor_rect = editor.get_bounding_client_rect();
    let root_rect = root.get_bounding_client_rect();
    let element: &web_sys::Element = editor.unchecked_ref();
    let metrics = read_editor_metrics(element);
    peek_geometry(
        &PeekViewport {
            viewport_width,
            viewport_height,
            editor_top: editor_rect.top(),
            editor_bottom: editor_rect.bottom(),
            editor_left: editor_rect.left(),
            editor_scroll_top: editor.scroll_top() as f64,
            root_top: root_rect.top(),
            root_left: root_rect.left(),
        },
        &metrics,
        line,
        old_line_count,
        new_line_count,
    )
}

pub(in crate::editor) fn diff_lines(baseline: &str, current: &str) -> LineDiff {
    let current_ends_with_newline = current.ends_with('\n') || current.ends_with('\r');
    let baseline_ends_with_newline = baseline.ends_with('\n') || baseline.ends_with('\r');
    let baseline = normalize_newlines(baseline);
    let current = normalize_newlines(current);
    if baseline.is_empty() && current.is_empty() {
        return LineDiff::empty(1);
    }
    let baseline = document_lines(&baseline);
    let current = document_lines(&current);
    // When both end in a newline, their trailing empty lines pair up, so added
    // blank lines are marked before it rather than at the end.
    let baseline_eof_slot = baseline
        .len()
        .checked_sub(1)
        .filter(|_| baseline.last() == Some(&"") && baseline_ends_with_newline);
    let reserve_baseline_eof = current_ends_with_newline && baseline_eof_slot.is_some();
    let mut changes = vec![DiffKind::Unchanged; current.len()];
    let mut deleted_before = vec![false; current.len()];
    let mut deleted_lines_before = vec![Vec::new(); current.len()];
    let mut modified_before = vec![None; current.len()];
    let mut deleted_after = false;
    let mut deleted_lines_after = Vec::new();
    let mut hunks = Vec::new();
    let mut prefix = 0;
    while prefix < baseline.len()
        && prefix < current.len()
        && !(reserve_baseline_eof && Some(prefix) == baseline_eof_slot)
        && baseline[prefix] == current[prefix]
    {
        prefix += 1;
    }

    let mut suffix = 0;
    while suffix < baseline.len().saturating_sub(prefix)
        && suffix < current.len().saturating_sub(prefix)
        && baseline[baseline.len() - suffix - 1] == current[current.len() - suffix - 1]
    {
        suffix += 1;
    }

    let old_middle = &baseline[prefix..baseline.len() - suffix];
    let new_middle = &current[prefix..current.len() - suffix];
    let edits = middle_edits(old_middle, new_middle);

    let mut new_index = prefix;
    let mut old_index = 0;
    let mut edit_index = 0;
    while edit_index < edits.len() {
        if matches!(edits[edit_index], DiffEdit::Equal) {
            new_index += 1;
            old_index += 1;
            edit_index += 1;
            continue;
        }
        let hunk_start = new_index;
        let mut deleted = Vec::new();
        let mut inserted = Vec::new();
        while edit_index < edits.len() && !matches!(edits[edit_index], DiffEdit::Equal) {
            match edits[edit_index] {
                DiffEdit::Delete => {
                    deleted.push((prefix + old_index, old_middle[old_index].to_string()));
                    old_index += 1;
                }
                DiffEdit::Insert => {
                    inserted.push(new_index);
                    new_index += 1;
                }
                DiffEdit::Equal => unreachable!(),
            }
            edit_index += 1;
        }

        let pairable_deleted: Vec<_> = deleted
            .iter()
            .filter(|(old_line_index, _)| Some(*old_line_index) != baseline_eof_slot)
            .collect();
        let paired = pairable_deleted.len().min(inserted.len());
        for (index, (_, old_line)) in inserted.iter().take(paired).zip(&pairable_deleted) {
            changes[*index] = DiffKind::Modified;
            modified_before[*index] = Some(old_line.clone());
        }
        for index in inserted.iter().skip(paired) {
            changes[*index] = DiffKind::Added;
        }
        let removed_lines: Vec<String> = pairable_deleted
            .iter()
            .skip(paired)
            .map(|(_, line)| line.clone())
            .collect();
        // A hunk that only drops the baseline's final newline shows nothing,
        // so it is not a change either.
        if !inserted.is_empty() || !removed_lines.is_empty() {
            hunks.push(Hunk {
                current: hunk_start..new_index,
                baseline: deleted.iter().map(|(_, line)| line.clone()).collect(),
                added: inserted.len() - paired,
                modified: paired,
                removed: removed_lines.len(),
            });
        }
        if !removed_lines.is_empty() {
            if new_index >= current.len() {
                deleted_after = true;
                deleted_lines_after.extend(removed_lines);
            } else {
                deleted_before[new_index] = true;
                deleted_lines_before[new_index].extend(removed_lines);
            }
        }
    }

    LineDiff {
        current: changes,
        deleted_before,
        deleted_lines_before,
        modified_before,
        deleted_after,
        deleted_lines_after,
        hunks,
    }
}

/// The textarea turns CRLF into LF on the first edit, so both sides are
/// compared as LF to keep every line from showing as modified.
pub(in crate::editor) fn normalize_newlines(text: &str) -> Cow<'_, str> {
    if text.contains('\r') {
        Cow::Owned(text.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(text)
    }
}

/// The lines of an LF text, none for an empty one: an empty baseline has no
/// line to modify, so everything typed into it is added (and reverting it
/// gives the empty text back).
pub(in crate::editor) fn document_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        Vec::new()
    } else {
        text.split('\n').collect()
    }
}

/// LCS table cells past which the diff switches to Myers.
const LCS_MAX_CELLS: usize = 1_000_000;

/// The edits turning `old` into `new`, without a common prefix or suffix
/// (the callers trim those): an exact LCS when small, Myers up to its depth
/// cap, else one delete-then-insert block.
pub(in crate::editor) fn middle_edits(old: &[&str], new: &[&str]) -> Vec<DiffEdit> {
    let cell_count = (old.len() + 1).saturating_mul(new.len() + 1);
    if cell_count > LCS_MAX_CELLS {
        if let Some(edits) = myers_edits(old, new) {
            return edits;
        }
        let mut edits = Vec::with_capacity(old.len() + new.len());
        edits.extend(std::iter::repeat_n(DiffEdit::Delete, old.len()));
        edits.extend(std::iter::repeat_n(DiffEdit::Insert, new.len()));
        return edits;
    }
    let columns = new.len() + 1;
    let mut lcs = vec![0u32; cell_count];
    for old_index in (0..old.len()).rev() {
        for new_index in (0..new.len()).rev() {
            let index = old_index * columns + new_index;
            lcs[index] = if old[old_index] == new[new_index] {
                1 + lcs[(old_index + 1) * columns + new_index + 1]
            } else {
                lcs[(old_index + 1) * columns + new_index].max(lcs[index + 1])
            };
        }
    }
    let mut edits = Vec::with_capacity(old.len() + new.len());
    let (mut old_index, mut new_index) = (0, 0);
    while old_index < old.len() || new_index < new.len() {
        if old_index < old.len() && new_index < new.len() && old[old_index] == new[new_index] {
            edits.push(DiffEdit::Equal);
            old_index += 1;
            new_index += 1;
        } else if old_index < old.len()
            && (new_index == new.len()
                || lcs[(old_index + 1) * columns + new_index]
                    >= lcs[old_index * columns + new_index + 1])
        {
            edits.push(DiffEdit::Delete);
            old_index += 1;
        } else {
            edits.push(DiffEdit::Insert);
            new_index += 1;
        }
    }
    edits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::metrics::EditorMetrics;

    fn test_viewport() -> PeekViewport {
        PeekViewport {
            viewport_width: 1100.0,
            viewport_height: 620.0,
            editor_top: 100.0,
            editor_bottom: 600.0,
            editor_left: 50.0,
            editor_scroll_top: 0.0,
            root_top: 80.0,
            root_left: 40.0,
        }
    }

    #[test]
    fn peek_sizing_constants_match_the_preset_stylesheet() {
        // `preset.css` sources: `.config-editor__diff-peek` min-width,
        // `.config-editor__diff-peek-columns` minmax(240px, 1fr),
        // `.config-editor__diff-peek-line` 22px rows, and the sticky header.
        assert_eq!(PEEK_MIN_WIDTH, 180.0);
        assert_eq!(PEEK_PREFERRED_WIDTH, 560.0);
        assert_eq!(PEEK_LINE_HEIGHT, 22.0);
        assert_eq!(PEEK_HEADER_HEIGHT, 72.0);
        assert_eq!(PEEK_MAX_HEIGHT, 360.0);
    }

    #[test]
    fn peek_geometry_places_the_popup_below_the_line_when_space_allows() {
        let style = peek_geometry(&test_viewport(), &EditorMetrics::fallback(), 2, 1, 1);
        // Anchor is at editor_top + one line height; the popup sits below it
        // and the style is relative to the root element.
        assert!(style.starts_with("top:67.2px;"), "{style}");
        assert!(style.contains("left:15.0px;"), "{style}");
    }

    #[test]
    fn peek_geometry_flips_above_the_line_when_space_below_runs_out() {
        let mut viewport = test_viewport();
        viewport.viewport_height = 200.0;
        viewport.editor_top = 20.0;
        viewport.editor_bottom = 200.0;
        viewport.root_top = 0.0;
        let style = peek_geometry(&viewport, &EditorMetrics::fallback(), 5, 1, 1);
        // Anchor near the bottom of a short viewport: popup goes above it and
        // stays inside the viewport margin.
        let top: f64 = style
            .strip_prefix("top:")
            .and_then(|rest| rest.split("px").next())
            .and_then(|top| top.parse().ok())
            .unwrap();
        assert!(top >= 8.0, "{style}");
        assert!(top < 110.0, "{style}");
    }

    #[test]
    fn peek_geometry_stays_inside_the_text_area() {
        let mut viewport = test_viewport();
        viewport.editor_bottom = 300.0;
        // A large change near the area's bottom edge.
        let style = peek_geometry(&viewport, &EditorMetrics::fallback(), 8, 6, 6);
        let number = |name: &str| -> f64 {
            style
                .split(';')
                .find_map(|part| part.strip_prefix(name))
                .and_then(|value| value.strip_suffix("px"))
                .and_then(|value| value.parse().ok())
                .unwrap()
        };
        let bottom = viewport.root_top + number("top:") + number("max-height:");
        assert!(bottom <= 300.0, "{style}");
        assert!(
            viewport.root_top + number("top:") >= viewport.editor_top,
            "{style}"
        );
    }

    #[test]
    fn line_diff_keeps_old_text_for_modified_lines_and_removed_hunks() {
        let diff = diff_lines(
            "first:\nold value\nremoved too\nlast:",
            "first:\nnew value\nlast:",
        );

        assert_eq!(diff.current[1], DiffKind::Modified);
        assert_eq!(diff.modified_before[1].as_deref(), Some("old value"));
        assert!(diff.deleted_before[2]);
        assert_eq!(diff.deleted_lines_before[2], vec!["removed too"]);
    }

    #[test]
    fn line_diff_keeps_removed_lines_at_the_end_for_peek() {
        let diff = diff_lines("keep:\nremoved one\nremoved two", "keep:");

        assert!(diff.deleted_after);
        assert_eq!(diff.deleted_lines_after, vec!["removed one", "removed two"]);
    }

    fn modified_peek(line: usize, before: &str, after: &str) -> DiffPeek {
        DiffPeek {
            line,
            kind: PeekKind::Modified,
            title: "Modified line".to_string(),
            before: vec![before.to_string()],
            after: vec![after.to_string()],
        }
    }

    #[test]
    fn peek_survives_typing_elsewhere_but_not_on_its_own_line() {
        let baseline = "k1: 1\nk2: 2\nk3: 3\n";
        let current: Vec<String> = "k1: 1\nk2: 99\nk3: 3\n"
            .split('\n')
            .map(str::to_string)
            .collect();
        let peek = modified_peek(2, "k2: 2", "k2: 99");
        assert!(peek_survives_edit(
            &peek,
            &current,
            &diff_lines(baseline, &current.join("\n"))
        ));

        // Appending an unrelated line keeps the anchor intact.
        let appended: Vec<String> = "k1: 1\nk2: 99\nk3: 3\nk4: 4\n"
            .split('\n')
            .map(str::to_string)
            .collect();
        assert!(peek_survives_edit(
            &peek,
            &appended,
            &diff_lines(baseline, &appended.join("\n"))
        ));

        // Editing the peeked line itself changes its text: dismiss.
        let edited: Vec<String> = "k1: 1\nk2: 100\nk3: 3\n"
            .split('\n')
            .map(str::to_string)
            .collect();
        assert!(!peek_survives_edit(
            &peek,
            &edited,
            &diff_lines(baseline, &edited.join("\n"))
        ));

        // Inserting a line above shifts the anchor onto another line: the
        // snapshot no longer matches, so dismiss instead of showing the
        // wrong lines.
        let shifted: Vec<String> = "k0: 0\nk1: 1\nk2: 99\nk3: 3\n"
            .split('\n')
            .map(str::to_string)
            .collect();
        assert!(!peek_survives_edit(
            &peek,
            &shifted,
            &diff_lines(baseline, &shifted.join("\n"))
        ));
    }

    #[test]
    fn peek_survives_while_its_deleted_hunk_remains() {
        let baseline = "keep:\ngone: 1\ntail: 2\n";
        let current: Vec<String> = "keep:\ntail: 2\n".split('\n').map(str::to_string).collect();
        let diff = diff_lines(baseline, &current.join("\n"));
        assert!(diff.deleted_before[1]);
        let peek = DiffPeek {
            line: 2,
            kind: PeekKind::Removed,
            title: "Removed lines".to_string(),
            before: vec!["gone: 1".to_string()],
            after: Vec::new(),
        };
        assert!(peek_survives_edit(&peek, &current, &diff));

        // Editing an unrelated line keeps the hunk: the peek stays.
        let edited: Vec<String> = "keep: 1\ntail: 2\n"
            .split('\n')
            .map(str::to_string)
            .collect();
        assert!(peek_survives_edit(
            &peek,
            &edited,
            &diff_lines(baseline, &edited.join("\n"))
        ));

        // Restoring the deleted line clears the hunk: dismiss.
        let restored: Vec<String> = "keep:\ngone: 1\ntail: 2\n"
            .split('\n')
            .map(str::to_string)
            .collect();
        assert!(!peek_survives_edit(
            &peek,
            &restored,
            &diff_lines(baseline, &restored.join("\n"))
        ));
    }

    #[test]
    fn peek_survives_while_trailing_deleted_lines_remain() {
        let baseline = "keep:\ngone one\ngone two";
        let current: Vec<String> = vec!["keep:".to_string()];
        let diff = diff_lines(baseline, &current.join("\n"));
        assert!(diff.deleted_after);
        let peek = DiffPeek {
            line: 1,
            kind: PeekKind::RemovedAtEnd,
            title: "Removed lines at end of document".to_string(),
            before: vec!["gone one".to_string(), "gone two".to_string()],
            after: Vec::new(),
        };
        assert!(peek_survives_edit(&peek, &current, &diff));
    }
}

#[cfg(test)]
mod blank_line_properties {
    use super::*;

    #[test]
    fn existing_eof_slot_is_not_confused_with_new_blank_lines_or_eof_slot() {
        let baseline = "a: 1\nb: 2\n";

        // The baseline EOF slot aligns with the current EOF slot; the new row
        // before it is Added even though both rows are empty.
        let one_enter = diff_lines(baseline, "a: 1\nb: 2\n\n");
        assert_eq!(
            one_enter.current,
            vec![
                DiffKind::Unchanged,
                DiffKind::Unchanged,
                DiffKind::Added,
                DiffKind::Unchanged,
            ]
        );

        // Another Enter adds another blank row before the unhighlighted EOF slot.
        let two_enters = diff_lines(baseline, "a: 1\nb: 2\n\n\n");
        assert_eq!(
            two_enters.current,
            vec![
                DiffKind::Unchanged,
                DiffKind::Unchanged,
                DiffKind::Added,
                DiffKind::Added,
                DiffKind::Unchanged,
            ]
        );

        // The current EOF slot stays unhighlighted while new rows before it stay Added.
        let text_in_added_rows = diff_lines(baseline, "a: 1\nb: 2\n\nupdate: true\n\n");
        assert_eq!(
            text_in_added_rows.current,
            vec![
                DiffKind::Unchanged,
                DiffKind::Unchanged,
                DiffKind::Added,
                DiffKind::Added,
                DiffKind::Added,
                DiffKind::Unchanged,
            ]
        );

        // Typing into the current EOF slot materializes it as an added content row.
        let text_at_eof = diff_lines(baseline, "a: 1\nb: 2\n\nupdate: true\n\nlast: true");
        assert_eq!(
            text_at_eof.current,
            vec![
                DiffKind::Unchanged,
                DiffKind::Unchanged,
                DiffKind::Unchanged,
                DiffKind::Added,
                DiffKind::Added,
                DiffKind::Added,
            ]
        );

        // Typing a real value directly into the baseline's empty EOF slot is
        // content insertion, not a replacement of a real baseline line.
        let direct_value = diff_lines("a: 1\n", "a: 1\nreal: value");
        assert_eq!(
            direct_value.current,
            vec![DiffKind::Unchanged, DiffKind::Added]
        );
    }
}

#[cfg(test)]
mod crlf {
    use super::*;

    #[test]
    fn crlf_and_lf_documents_are_the_same_lines() {
        let baseline = "a: 1\nb: 2\n";
        let current = "a: 1\r\nb: 2\r\n\r\n";
        let diff = diff_lines(baseline, current);
        assert_eq!(
            diff.current,
            vec![
                DiffKind::Unchanged,
                DiffKind::Unchanged,
                DiffKind::Added,
                DiffKind::Unchanged,
            ]
        );
        assert!(diff.modified_before.iter().all(Option::is_none));
    }

    #[test]
    fn mixed_line_endings_do_not_mark_lines_modified() {
        let baseline = "a: 1\r\nb: 2\n";
        let current = "a: 1\nb: 2\r\n";
        let diff = diff_lines(baseline, current);
        assert_eq!(
            diff.current,
            vec![
                DiffKind::Unchanged,
                DiffKind::Unchanged,
                DiffKind::Unchanged
            ]
        );
    }
}

#[cfg(test)]
mod trailing_blank_lines {
    use super::*;

    #[test]
    fn adding_empty_lines_to_trailing_newline_file() {
        let baseline = "a: 1\n";
        let after_one = "a: 1\n\n";
        let after_two = "a: 1\n\n\n";

        let d1 = diff_lines(baseline, after_one);
        assert_eq!(
            d1.current,
            vec![DiffKind::Unchanged, DiffKind::Added, DiffKind::Unchanged]
        );

        let d2 = diff_lines(baseline, after_two);
        assert_eq!(
            d2.current,
            vec![
                DiffKind::Unchanged,
                DiffKind::Added,
                DiffKind::Added,
                DiffKind::Unchanged
            ]
        );
    }

    #[test]
    fn baseline_trailing_blank_line_stays_unchanged_as_new_lines_are_edited() {
        let baseline = "a: 1\nb: 2\n";
        let after_adding_empty_line = "a: 1\nb: 2\n\n";
        let after_typing_in_the_middle = "a: 1\nb: 2\n\nmiddle\n";
        let after_typing_at_the_end = "a: 1\nb: 2\n\nmiddle\n\nlast";

        assert_eq!(
            diff_lines(baseline, after_adding_empty_line).current,
            vec![
                DiffKind::Unchanged,
                DiffKind::Unchanged,
                DiffKind::Added,
                DiffKind::Unchanged,
            ]
        );
        let middle = diff_lines(baseline, after_typing_in_the_middle);
        assert_eq!(
            middle.current,
            vec![
                DiffKind::Unchanged,
                DiffKind::Unchanged,
                DiffKind::Added,
                DiffKind::Added,
                DiffKind::Unchanged,
            ]
        );
        let end = diff_lines(baseline, after_typing_at_the_end);
        assert_eq!(
            end.current,
            vec![
                DiffKind::Unchanged,
                DiffKind::Unchanged,
                DiffKind::Unchanged,
                DiffKind::Added,
                DiffKind::Added,
                DiffKind::Added,
            ]
        );
        assert!(middle.modified_before.iter().all(Option::is_none));
        assert!(end.modified_before.iter().all(Option::is_none));
    }
}

#[cfg(test)]
mod bench_probe {
    #[test]
    #[ignore]
    fn diff_cost_on_10k_lines() {
        let baseline: String = (0..10000)
            .map(|i| format!("  key_{i}: value number {i}\n"))
            .collect();
        let mut edited = baseline.clone();
        edited.insert(baseline.len() / 2, 'x');
        let started = std::time::Instant::now();
        for _ in 0..20 {
            let _ = super::diff_lines(&baseline, &edited);
        }
        eprintln!("diff_lines: {:?} per call", started.elapsed() / 20);
    }
}

#[cfg(test)]
mod gutter_marks {
    use super::*;

    #[test]
    fn line_diff_marks_inserted_lines_and_preserves_unchanged_lines() {
        let diff = diff_lines("first:\nsecond:\nthird:", "first:\nadded:\nsecond:\nthird:");
        assert_eq!(
            diff.current,
            vec![
                DiffKind::Unchanged,
                DiffKind::Added,
                DiffKind::Unchanged,
                DiffKind::Unchanged,
            ]
        );
        assert_eq!(diff.deleted_before, vec![false; 4]);
    }

    #[test]
    fn line_diff_keeps_existing_baseline_content_unchanged_and_marks_only_new_content() {
        let baseline = "name: app\nlisten: 7835\n";
        let current = "name: app\nlisten: 7835\ninsecure_plaintext: false\n";
        let diff = diff_lines(baseline, current);
        assert_eq!(
            diff.current,
            vec![
                DiffKind::Unchanged,
                DiffKind::Unchanged,
                DiffKind::Added,
                DiffKind::Unchanged,
            ]
        );
        assert!(diff.deleted_before.iter().all(|deleted| !deleted));
        assert!(!diff.deleted_after);
    }

    #[test]
    fn line_diff_marks_replacements_as_modified_and_deletions_in_the_gutter() {
        let diff = diff_lines("first:\nold:\nremove:\nlast:", "first:\nnew:\nlast:");
        assert_eq!(
            diff.current,
            vec![DiffKind::Unchanged, DiffKind::Modified, DiffKind::Unchanged,]
        );
        assert_eq!(diff.deleted_before, vec![false, false, true]);
    }

    #[test]
    fn line_diff_marks_inserted_blank_lines_without_shifting_following_lines() {
        let baseline = "first:\nsecond:\nthird:\n";
        let current = "first:\n\n\nsecond:\nthird:\n";
        let diff = diff_lines(baseline, current);
        assert_eq!(
            diff.current,
            vec![
                DiffKind::Unchanged,
                DiffKind::Added,
                DiffKind::Added,
                DiffKind::Unchanged,
                DiffKind::Unchanged,
                DiffKind::Unchanged,
            ]
        );
        assert!(diff.deleted_before.iter().all(|deleted| !deleted));
    }

    #[test]
    fn line_diff_keeps_following_lines_unchanged_after_insertions_at_eof() {
        let baseline = "first:\nsecond:\n";
        let current = "first:\nsecond:\n\n\n";
        let diff = diff_lines(baseline, current);
        assert_eq!(
            diff.current,
            vec![
                DiffKind::Unchanged,
                DiffKind::Unchanged,
                DiffKind::Added,
                DiffKind::Added,
                DiffKind::Unchanged,
            ]
        );
    }

    #[test]
    fn line_diff_tracks_deletions_before_first_and_after_last_lines() {
        let beginning = diff_lines("remove:\nkeep:\n", "keep:\n");
        assert_eq!(
            beginning.current,
            vec![DiffKind::Unchanged, DiffKind::Unchanged]
        );
        assert_eq!(beginning.deleted_before, vec![true, false]);

        let ending = diff_lines("keep:\nremove:", "keep:");
        assert_eq!(ending.current, vec![DiffKind::Unchanged]);
        assert_eq!(ending.deleted_before, vec![false]);
        assert!(ending.deleted_after);
    }

    #[test]
    fn line_diff_clears_after_baseline_is_accepted() {
        let current = "first:\nnew:\nlast:";
        let diff = diff_lines(current, current);
        assert!(
            diff.current
                .iter()
                .all(|change| *change == DiffKind::Unchanged)
        );
        assert!(diff.deleted_before.iter().all(|deleted| !deleted));
    }
}
