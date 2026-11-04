use super::metrics::read_editor_metrics;
use std::borrow::Cow;
use wasm_bindgen::JsCast;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DiffKind {
    Unchanged,
    Added,
    Modified,
}

#[derive(Clone, PartialEq)]
pub(super) struct LineDiff {
    pub(super) current: Vec<DiffKind>,
    pub(super) deleted_before: Vec<bool>,
    pub(super) deleted_lines_before: Vec<Vec<String>>,
    pub(super) modified_before: Vec<Option<String>>,
    pub(super) deleted_after: bool,
    pub(super) deleted_lines_after: Vec<String>,
}

pub(super) fn peek_popup_style(
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
        .unwrap_or(1024.0);
    let viewport_height = window
        .inner_height()
        .ok()
        .and_then(|height| height.as_f64())
        .unwrap_or(768.0);
    let editor_rect = editor.get_bounding_client_rect();
    let root_rect = root.get_bounding_client_rect();
    let element: &web_sys::Element = editor.unchecked_ref();
    let metrics = read_editor_metrics(element);
    let anchor_y = editor_rect.top()
        + metrics.padding_top
        + line.saturating_sub(1) as f64 * metrics.line_height
        - editor.scroll_top() as f64;
    let margin = 8.0;
    let space_above = (anchor_y - margin).max(0.0);
    let space_below = (viewport_height - anchor_y - metrics.line_height - margin).max(0.0);
    let desired_height = (72.0 + (old_line_count + new_line_count) as f64 * 22.0)
        .min(360.0)
        .min(viewport_height * 0.55);
    let place_below = space_below >= desired_height.min(180.0) || space_below >= space_above;
    let available_height = if place_below {
        space_below
    } else {
        space_above
    };
    let height = desired_height.min(available_height);
    let top_in_viewport = if place_below {
        anchor_y + metrics.line_height + 4.0
    } else {
        anchor_y - height - 4.0
    }
    .clamp(margin, (viewport_height - height - margin).max(margin));
    let left_in_viewport =
        (editor_rect.left() + 5.0).clamp(margin, (viewport_width - 180.0 - margin).max(margin));
    let width = 560.0_f64.min((viewport_width - left_in_viewport - margin).max(0.0));

    format!(
        "top:{:.1}px;left:{:.1}px;width:{:.1}px;max-height:{:.1}px",
        top_in_viewport - root_rect.top(),
        left_in_viewport - root_rect.left(),
        width,
        height
    )
}

#[derive(Clone, Copy)]
enum DiffEdit {
    Equal,
    Insert,
    Delete,
}

fn myers_edits(old: &[&str], new: &[&str]) -> Option<Vec<DiffEdit>> {
    let max_depth = old.len().saturating_add(new.len()).min(1024);
    let offset = max_depth as isize + 1;
    let mut frontier = vec![0isize; max_depth * 2 + 3];
    frontier[(offset + 1) as usize] = 0;
    let mut trace = Vec::with_capacity(max_depth + 1);

    for depth in 0..=max_depth {
        trace.push(frontier.clone());
        let depth_signed = depth as isize;
        let mut diagonal = -depth_signed;
        while diagonal <= depth_signed {
            let index = (offset + diagonal) as usize;
            let mut x = if diagonal == -depth_signed
                || (diagonal != depth_signed && frontier[index - 1] < frontier[index + 1])
            {
                frontier[index + 1]
            } else {
                frontier[index - 1] + 1
            };
            let mut y = x - diagonal;
            while x < old.len() as isize
                && y < new.len() as isize
                && old[x as usize] == new[y as usize]
            {
                x += 1;
                y += 1;
            }
            frontier[index] = x;
            if x >= old.len() as isize && y >= new.len() as isize {
                return Some(backtrack_myers(&trace, depth, offset, old.len(), new.len()));
            }
            diagonal += 2;
        }
    }
    None
}

fn backtrack_myers(
    trace: &[Vec<isize>],
    depth: usize,
    offset: isize,
    old_len: usize,
    new_len: usize,
) -> Vec<DiffEdit> {
    let (mut x, mut y) = (old_len as isize, new_len as isize);
    let mut edits = Vec::with_capacity(old_len + new_len);
    for depth in (1..=depth).rev() {
        let diagonal = x - y;
        let frontier = &trace[depth];
        let index = (offset + diagonal) as usize;
        let previous_diagonal = if diagonal == -(depth as isize)
            || (diagonal != depth as isize && frontier[index - 1] < frontier[index + 1])
        {
            diagonal + 1
        } else {
            diagonal - 1
        };
        let previous_x = frontier[(offset + previous_diagonal) as usize];
        let previous_y = previous_x - previous_diagonal;
        while x > previous_x && y > previous_y {
            edits.push(DiffEdit::Equal);
            x -= 1;
            y -= 1;
        }
        if x == previous_x {
            edits.push(DiffEdit::Insert);
        } else {
            edits.push(DiffEdit::Delete);
        }
        x = previous_x;
        y = previous_y;
    }
    while x > 0 && y > 0 {
        edits.push(DiffEdit::Equal);
        x -= 1;
        y -= 1;
    }
    while x > 0 {
        edits.push(DiffEdit::Delete);
        x -= 1;
    }
    while y > 0 {
        edits.push(DiffEdit::Insert);
        y -= 1;
    }
    edits.reverse();
    edits
}

/// CRLF and LF documents must diff as the same lines; the textarea normalizes its
/// value on the first edit, so comparing raw text would mark every line modified.
fn normalize_newlines(text: &str) -> Cow<'_, str> {
    if text.contains('\r') {
        Cow::Owned(text.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(text)
    }
}

pub(super) fn diff_lines(baseline: &str, current: &str) -> LineDiff {
    let baseline = normalize_newlines(baseline);
    let current = normalize_newlines(current);
    let baseline: Vec<_> = baseline.split('\n').collect();
    let current: Vec<_> = current.split('\n').collect();
    let mut changes = vec![DiffKind::Unchanged; current.len()];
    let mut deleted_before = vec![false; current.len()];
    let mut deleted_lines_before = vec![Vec::new(); current.len()];
    let mut modified_before = vec![None; current.len()];
    let mut deleted_after = false;
    let mut deleted_lines_after = Vec::new();
    let mut prefix = 0;
    while prefix < baseline.len() && prefix < current.len() && baseline[prefix] == current[prefix] {
        if baseline[prefix].is_empty()
            && baseline[prefix..].iter().all(|line| line.is_empty())
            && current[prefix..].iter().all(|line| line.is_empty())
        {
            break;
        }
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
    let cell_count = (old_middle.len() + 1).saturating_mul(new_middle.len() + 1);
    let mut edits = Vec::new();
    if cell_count <= 1_000_000 {
        let columns = new_middle.len() + 1;
        let mut lcs = vec![0u32; cell_count];
        for old_index in (0..old_middle.len()).rev() {
            for new_index in (0..new_middle.len()).rev() {
                let index = old_index * columns + new_index;
                lcs[index] = if old_middle[old_index] == new_middle[new_index] {
                    1 + lcs[(old_index + 1) * columns + new_index + 1]
                } else {
                    lcs[(old_index + 1) * columns + new_index].max(lcs[index + 1])
                };
            }
        }

        let (mut old_index, mut new_index) = (0, 0);
        while old_index < old_middle.len() || new_index < new_middle.len() {
            if old_index < old_middle.len()
                && new_index < new_middle.len()
                && old_middle[old_index] == new_middle[new_index]
            {
                edits.push(DiffEdit::Equal);
                old_index += 1;
                new_index += 1;
            } else if old_index < old_middle.len()
                && (new_index == new_middle.len()
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
    } else if let Some(myers) = myers_edits(old_middle, new_middle) {
        edits = myers;
    } else {
        edits.extend(std::iter::repeat_n(DiffEdit::Delete, old_middle.len()));
        edits.extend(std::iter::repeat_n(DiffEdit::Insert, new_middle.len()));
    }

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
        let mut deleted = Vec::new();
        let mut inserted = Vec::new();
        while edit_index < edits.len() && !matches!(edits[edit_index], DiffEdit::Equal) {
            match edits[edit_index] {
                DiffEdit::Delete => {
                    deleted.push(old_middle[old_index].to_string());
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

        let paired = deleted.len().min(inserted.len());
        for (index, old_line) in inserted.iter().take(paired).zip(&deleted) {
            changes[*index] = DiffKind::Modified;
            modified_before[*index] = Some(old_line.clone());
        }
        for index in inserted.iter().skip(paired) {
            changes[*index] = DiffKind::Added;
        }
        if deleted.len() > paired {
            let removed_lines = deleted.into_iter().skip(paired).collect::<Vec<_>>();
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn myers_fallback_preserves_context_for_large_line_ranges() {
        let old: Vec<_> = (0..1200).map(|line| format!("line-{line}")).collect();
        let mut new = old.clone();
        new.insert(600, "inserted".to_string());
        let old_refs: Vec<_> = old.iter().map(String::as_str).collect();
        let new_refs: Vec<_> = new.iter().map(String::as_str).collect();

        let edits = myers_edits(&old_refs, &new_refs).unwrap();
        assert_eq!(
            edits
                .iter()
                .filter(|edit| matches!(edit, DiffEdit::Equal))
                .count(),
            1200
        );
        assert_eq!(
            edits
                .iter()
                .filter(|edit| matches!(edit, DiffEdit::Insert))
                .count(),
            1
        );
        assert_eq!(
            edits
                .iter()
                .filter(|edit| matches!(edit, DiffEdit::Delete))
                .count(),
            0
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
}

#[cfg(test)]
mod blank_line_properties {
    use super::*;

    fn assert_only_added(baseline: &str, current: &str, expected_added: usize) {
        let diff = diff_lines(baseline, current);
        let added = diff
            .current
            .iter()
            .filter(|kind| **kind == DiffKind::Added)
            .count();
        let modified = diff
            .current
            .iter()
            .filter(|kind| **kind == DiffKind::Modified)
            .count();
        assert_eq!(
            added, expected_added,
            "added for {baseline:?} -> {current:?}: {:?}",
            diff.current
        );
        assert_eq!(
            modified, 0,
            "modified for {baseline:?} -> {current:?}: {:?} {:?}",
            diff.current, diff.modified_before
        );
    }

    #[test]
    fn inserting_blank_lines_anywhere_is_always_an_insertion() {
        let bases = [
            "a: 1\nb: 2\nc: 3\n",
            "a: 1\nb: 2\nc: 3",
            "a: 1\n\nb: 2\n",
            "\n\na: 1\n\nb: 2\n\n",
        ];
        for base in bases {
            let lines: Vec<&str> = base.split('\n').collect();
            for position in 0..=lines.len() {
                for blanks in 1..=3 {
                    let mut candidate: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
                    for _ in 0..blanks {
                        candidate.insert(position.min(candidate.len()), String::new());
                    }
                    let current = candidate.join("\n");
                    assert_only_added(base, &current, blanks);
                }
            }
        }
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
                DiffKind::Unchanged
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
