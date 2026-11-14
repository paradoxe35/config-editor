//! The nearest line above a line that is indented less than some column:
//! what the hover walks up for (the block an explicit key's `:` line
//! holds, the `? |` key a block scalar's text belongs to). While a batch
//! runs for the text, each line's parent (the nearest line above indented
//! less) is read once and a lookup jumps from parent to parent, a step
//! per indentation level; outside one, the lines above are read one by
//! one as before. (QA 36: thousands of places asked at once under one key,
//! each walked up over every sibling above it: quadratic.)

use std::rc::Rc;

use crate::editor::work_count::{Work, note};

use super::hover::entry_body;

/// The lines a walk up reads (the others it passes over).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Counting {
    /// Lines holding an entry: not blank, not a comment.
    Entries,
    /// Lines that are not blank (comments count).
    NonBlank,
}

impl Counting {
    fn counts(self, line: &str) -> bool {
        match self {
            Self::Entries => {
                let (_, text) = entry_body(line);
                !text.is_empty() && !text.starts_with('#')
            }
            Self::NonBlank => !line.trim().is_empty(),
        }
    }
}

/// `line`'s indentation (spaces and tabs) in bytes.
pub(super) fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

/// For each line (and past the last), the counted line nearest above it, and each counted
/// line's parent: the counted line nearest above it indented less.
struct Parents {
    indents: Vec<usize>,
    previous: Vec<Option<usize>>,
    parent: Vec<Option<usize>>,
}

impl Parents {
    fn new<'a>(line: &dyn Fn(usize) -> &'a str, count: usize, counting: Counting) -> Self {
        let mut indents = Vec::with_capacity(count);
        let mut previous = Vec::with_capacity(count);
        let mut parent = Vec::with_capacity(count);
        let mut last = None;
        // The counted lines above, indented more each.
        let mut open: Vec<usize> = Vec::new();
        for index in 0..count {
            let text = line(index);
            let at = indent(text);
            indents.push(at);
            previous.push(last);
            if !counting.counts(text) {
                parent.push(None);
                continue;
            }
            while open.last().is_some_and(|&above| indents[above] >= at) {
                open.pop();
            }
            parent.push(open.last().copied());
            open.push(index);
            last = Some(index);
        }
        // Past the last line: the last counted one.
        previous.push(last);
        Parents {
            indents,
            previous,
            parent,
        }
    }

    fn above_less(&self, line_index: usize, column: usize) -> Option<usize> {
        let mut above = self.previous[line_index.min(self.previous.len() - 1)];
        while let Some(at) = above {
            note(Work::WalkUp);
            if self.indents[at] < column {
                return Some(at);
            }
            // Every counted line between it and its parent is indented
            // as far as it at least: past `column` too.
            above = self.parent[at];
        }
        None
    }
}

/// The counted line nearest above line `line_index` of `source` (whose
/// lines `line` gives, `count` of them) indented less than `column`.
pub(super) fn above_less<'a>(
    source: &str,
    line: &dyn Fn(usize) -> &'a str,
    count: usize,
    counting: Counting,
    line_index: usize,
    column: usize,
) -> Option<usize> {
    // Alone, a place's path is a reading of the text above it anyway
    // (counted where `paths::path_at` starts).
    if !crate::editor::paths::in_batch(source) {
        return (0..line_index.min(count)).rev().find(|&above| {
            let text = line(above);
            counting.counts(text) && indent(text) < column
        });
    }
    let make = || Parents::new(line, count, counting);
    // The batch keeps one value of each type: a type for each kind.
    struct Entries(Parents);
    struct NonBlank(Parents);
    match counting {
        Counting::Entries => kept(source, || Entries(make())).0.above_less(line_index, column),
        Counting::NonBlank => kept(source, || NonBlank(make())).0.above_less(line_index, column),
    }
}

fn kept<T: 'static>(source: &str, make: impl FnOnce() -> T) -> Rc<T> {
    crate::editor::paths::batch_cached_as(Work::LineSplit, source, make)
}
