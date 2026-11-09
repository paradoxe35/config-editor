//! What a line command makes of the text: the lines it moves, copies or
//! deletes, the selection then, and the closed folds then. Pure, so every
//! case is tested without an editor (see `line_edit_tests`).
//!
//! As in VS Code, a closed fold is one unit: a folded line takes its hidden
//! lines along (and stays folded), and a line moved past a fold jumps over
//! all of it; the line closing a folded construct (`}`, an end tag, shown
//! below the fold) goes with it, as a cut of the folded line takes it. The
//! text keeps its last newline, or its lack of one: the
//! empty line after a last newline is where the text ends, not a line to
//! move.

use super::super::fold_ranges::FoldRange;

/// A line command the model makes (indenting is `commands::indent_lines`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::editor) enum LineCommand {
    Move { down: bool },
    Copy { down: bool },
    Delete,
}

/// The text after a line command, the selection then (UTF-16, in the order
/// given), and the closed folds then.
#[derive(Debug, PartialEq)]
pub(in crate::editor) struct LineEdit {
    pub(in crate::editor) text: String,
    pub(in crate::editor) selection: (usize, usize),
    pub(in crate::editor) folds: Vec<FoldRange>,
}

/// `text` as lines: those before the last newline (or all of them), and
/// whether it ends with one.
struct Lines<'a> {
    lines: Vec<&'a str>,
    final_newline: bool,
}

impl<'a> Lines<'a> {
    fn of(text: &'a str) -> Self {
        let (body, final_newline) = match text.strip_suffix('\n') {
            Some(body) => (body, true),
            None => (text, false),
        };
        Self {
            lines: body.split('\n').collect(),
            final_newline,
        }
    }
}

/// The line and column (UTF-16) of UTF-16 `offset` in `text`; the line
/// after a last newline is `lines.len()`.
fn position(text: &str, offset: usize) -> (usize, usize) {
    let (mut line, mut column, mut units) = (0, 0, 0);
    for character in text.chars() {
        if units >= offset {
            break;
        }
        units += character.len_utf16();
        if character == '\n' {
            line += 1;
            column = 0;
        } else {
            column += character.len_utf16();
        }
    }
    (line, column)
}

fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The UTF-16 offset of `column` (clamped to the line) on `line` of
/// `lines` (`lines.len()` is the empty line after a last newline).
fn offset(lines: &[String], line: usize, column: usize) -> usize {
    let before: usize = lines[..line.min(lines.len())]
        .iter()
        .map(|line| units(line) + 1)
        .sum();
    let length = lines.get(line).map_or(0, |line| units(line));
    before + column.min(length)
}

/// The lines (first, last; 0-based) a line command on `selection` of
/// `text` acts on: the selection's lines (one ending at a line's start
/// leaves that line out), widened over every closed fold's lines among
/// `folds` (see [`Folded`]) they reach. `None` on the empty line after a
/// last newline alone.
pub(in crate::editor) fn block_lines(
    text: &str,
    selection: (usize, usize),
    folded: Folded<'_>,
) -> Option<(usize, usize)> {
    let lines = Lines::of(text);
    let count = lines.lines.len();
    let (start, end) = (selection.0.min(selection.1), selection.0.max(selection.1));
    let (first, _) = position(text, start);
    let (last, column) = position(text, end);
    let mut last = if end > start && column == 0 && last > first {
        last - 1
    } else {
        last
    };
    if first >= count {
        return None;
    }
    last = last.min(count - 1);
    let mut first = first;
    loop {
        // A fold's folded or hidden lines reached: all of its lines (its
        // closing line alone is a line of its own).
        let (wide_first, wide_last) = folded
            .folds
            .iter()
            .zip(folded.units)
            .filter(|(fold, _)| fold.start <= last && fold.end >= first)
            .fold((first, last), |(from, to), (_, unit)| {
                (from.min(unit.start), to.max(unit.end.min(count - 1)))
            });
        if (wide_first, wide_last) == (first, last) {
            return Some((first, last));
        }
        (first, last) = (wide_first, wide_last);
    }
}

/// The closed folds, and the lines each one goes as: from its first line
/// through its last hidden one, and on through the line closing its
/// construct when that line shows (`}`, an end tag), as a cut of the
/// folded line takes it (see [`fold_units`]).
#[derive(Clone, Copy)]
pub(in crate::editor) struct Folded<'a> {
    pub(in crate::editor) folds: &'a [FoldRange],
    pub(in crate::editor) units: &'a [FoldRange],
}

/// The lines each of `folds` goes as in `text` (see [`Folded`]):
/// `behavior`'s whole constructs over the fold's lines.
pub(in crate::editor) fn fold_units(
    text: &str,
    folds: &[FoldRange],
    behavior: &dyn super::super::language::ConfigLanguage,
) -> Vec<FoldRange> {
    if folds.is_empty() {
        return Vec::new();
    }
    // Each line's first UTF-16 offset.
    let mut starts = vec![0];
    let mut at = 0;
    for character in text.chars() {
        at += character.len_utf16();
        if character == '\n' {
            starts.push(at);
        }
    }
    let line_of = |offset: usize| starts.partition_point(|start| *start <= offset) - 1;
    let line_end = |line: usize| starts.get(line + 1).map_or(at, |next| next - 1);
    folds
        .iter()
        .map(|fold| {
            let header = starts[fold.start.min(starts.len() - 1)];
            let (first, last) = behavior.whole_constructs(text, header, line_end(fold.end));
            FoldRange::new(line_of(first).min(fold.start), line_of(last).max(fold.end))
        })
        .collect()
}

/// The edit `command` makes of the lines of `selection` in `text`, with
/// `folded` closed; `None` when it changes nothing (the first line moved
/// up).
pub(in crate::editor) fn line_edit(
    text: &str,
    selection: (usize, usize),
    command: LineCommand,
    folded: Folded<'_>,
) -> Option<LineEdit> {
    let Folded {
        folds,
        units: spans,
    } = folded;
    let Lines {
        lines,
        final_newline,
    } = Lines::of(text);
    let count = lines.len();
    let caret = position(text, selection.0);
    let other = position(text, selection.1);
    let Some((first, last)) = block_lines(text, selection, folded) else {
        // The empty line after the last newline: a copy adds one line.
        return match command {
            LineCommand::Copy { down } => {
                let mut text = text.to_string();
                text.push('\n');
                let at = units(&text) - usize::from(!down);
                Some(LineEdit {
                    text,
                    selection: (at, at),
                    folds: folds.to_vec(),
                })
            }
            _ => None,
        };
    };
    let size = last - first + 1;
    let owned = |range: &[&str]| {
        range
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
    };
    let block = owned(&lines[first..=last]);
    let in_block = |fold: &FoldRange| (first..=last).contains(&fold.start);
    let shifted = |fold: &FoldRange, by: isize| {
        FoldRange::new(
            fold.start.saturating_add_signed(by),
            fold.end.saturating_add_signed(by),
        )
    };
    // The new lines, how far the selection's lines move, and the folds.
    let (new_lines, by, new_folds): (Vec<String>, isize, Vec<FoldRange>) = match command {
        LineCommand::Move { down: false } => {
            if first == 0 {
                return None;
            }
            // The line above, or the whole closed fold it ends.
            let above = spans
                .iter()
                .filter(|fold| fold.contains(first - 1))
                .map(|fold| fold.start)
                .min()
                .unwrap_or(first - 1);
            let over = first - above;
            let new_lines = [
                owned(&lines[..above]),
                block,
                owned(&lines[above..first]),
                owned(&lines[last + 1..]),
            ]
            .concat();
            let folds = folds
                .iter()
                .map(|fold| match fold {
                    fold if in_block(fold) => shifted(fold, -(over as isize)),
                    fold if (above..first).contains(&fold.start) => shifted(fold, size as isize),
                    fold => *fold,
                })
                .collect();
            (new_lines, -(over as isize), folds)
        }
        LineCommand::Move { down: true } => {
            if last + 1 >= count {
                return None;
            }
            // The line below, or the whole closed fold it starts.
            let below = spans
                .iter()
                .filter(|fold| fold.contains(last + 1))
                .map(|fold| fold.end.min(count - 1))
                .max()
                .unwrap_or(last + 1);
            let over = below - last;
            let new_lines = [
                owned(&lines[..first]),
                owned(&lines[last + 1..=below]),
                block,
                owned(&lines[below + 1..]),
            ]
            .concat();
            let folds = folds
                .iter()
                .map(|fold| match fold {
                    fold if in_block(fold) => shifted(fold, over as isize),
                    fold if (last + 1..=below).contains(&fold.start) => {
                        shifted(fold, -(size as isize))
                    }
                    fold => *fold,
                })
                .collect();
            (new_lines, over as isize, folds)
        }
        LineCommand::Copy { down } => {
            let new_lines = [owned(&lines[..=last]), block, owned(&lines[last + 1..])].concat();
            // Both copies stay folded.
            let folds = folds
                .iter()
                .flat_map(|fold| match fold {
                    fold if in_block(fold) => vec![*fold, shifted(fold, size as isize)],
                    fold if fold.start > last => vec![shifted(fold, size as isize)],
                    fold => vec![*fold],
                })
                .collect();
            (new_lines, if down { size as isize } else { 0 }, folds)
        }
        LineCommand::Delete => {
            let new_lines = [owned(&lines[..first]), owned(&lines[last + 1..])].concat();
            let folds = folds
                .iter()
                .filter(|fold| !in_block(fold))
                .map(|fold| match fold {
                    fold if fold.start > last => shifted(fold, -(size as isize)),
                    fold => *fold,
                })
                .collect();
            if new_lines.is_empty() {
                return (!text.is_empty()).then(|| LineEdit {
                    text: String::new(),
                    selection: (0, 0),
                    folds,
                });
            }
            let mut joined = new_lines.join("\n");
            if final_newline {
                joined.push('\n');
            }
            // The caret keeps its column on the line that takes the place
            // of the first deleted one (the one above, at the end).
            let line = first.min(new_lines.len() - usize::from(!final_newline));
            let at = offset(&new_lines, line, caret.1);
            return Some(LineEdit {
                text: joined,
                selection: (at, at),
                folds,
            });
        }
    };
    let mut joined = new_lines.join("\n");
    if final_newline {
        joined.push('\n');
    }
    let place = |(line, column): (usize, usize)| {
        // A selection ending at the start of the line after the block
        // ends there still.
        offset(&new_lines, line.saturating_add_signed(by), column)
    };
    let selection = (place(caret), place(other));
    (joined != text).then_some(LineEdit {
        text: joined,
        selection,
        folds: new_folds,
    })
}
