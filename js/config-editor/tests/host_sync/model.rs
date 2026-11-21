//! What both simulated views share: the actions of a run, the host's lock
//! as it works it out in its own copy of a text, the kinds of failure,
//! and the editor's core as far as locks go (`Core`).
//!
//! The core is modelled, not run (it needs a browser): an edit touching a
//! lock is refused, an edit before a lock moves it, one after leaves it,
//! and a host text (`Origin::HostValue`) places the ranges the core was
//! given in it at once, while new ranges are placed by an effect, before
//! the next task (`connect_protection`). Texts are ASCII: bytes are
//! UTF-16 units.

use std::ops::Range;

use config_editor_leptos::{TextPosition, TextRange};

/// The locked text: the host locks it wherever it is in its copy.
pub(crate) const LOCKED: &str = "secret: keep";

/// The documents the runs start from: the lock first, in the middle, on
/// the last line with no line break after it, and inside braces.
pub(crate) const BASES: &[&str] = &[
    "name: a\nport: 80\nsecret: keep\nhost: h\n",
    "secret: keep\nname: a\nport: 80\n",
    "name: a\nport: 80\nsecret: keep",
    "{\n  name: a,\n  port: 80,\n  secret: keep,\n  host: h\n}\n",
];

/// How the host writes its ranges: by lines and columns, or by string
/// index (what an update's same-ranges rule compares differs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Form {
    Lines,
    Index,
}

/// What a `replaceValue` gives.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ResetKind {
    /// A newer version of the host's copy (a line changed, added or
    /// removed, never the locked text).
    NewVersion(u16),
    /// The text the editor holds.
    Same,
    /// The text the host listed last.
    LastListed,
    /// The text the document started from (a discard).
    Original,
    /// Two newer versions, one after the other in the same task.
    Twice(u16),
}

/// One step of a run. Positions are taken modulo the text's length, a
/// pick modulo the host's queue, so any list of steps can be replayed
/// (and shrunk).
#[derive(Clone, Debug)]
pub(crate) enum Act {
    /// The user types `ch` at `at` in the shown document.
    Type { at: u16, ch: char },
    /// The user deletes `len` characters from `at`.
    Delete { at: u16, len: u8 },
    /// The user presses Enter at `at`.
    Enter { at: u16 },
    /// The user swaps line `line` with the next (Alt+Down, or Alt+Up on
    /// the next).
    MoveLine { line: u16 },
    /// The host hands back the text queued at `pick` for document `doc`
    /// (`dup`: twice in a row, as a framework rendering twice; a copy
    /// coming again after newer texts came back is the host's own by the
    /// echo contract), with its ranges worked out
    /// in it (`ranges`; a documents list always has them).
    Deliver {
        doc: u8,
        pick: u16,
        dup: bool,
        ranges: bool,
        same_task: bool,
    },
    /// The host fails to hand back the text queued at `pick` now (a
    /// fetch that failed), and hands it back after the others.
    Drop { doc: u8, pick: u16 },
    /// The host calls `replaceValue`; it lists that text later.
    Reset {
        doc: u8,
        kind: ResetKind,
        same_task: bool,
    },
    /// The host gives a text of its own (`update({ value })`: another
    /// version of the file), its pending echoes dropped.
    Own {
        doc: u8,
        edit: u16,
        ranges: bool,
        same_task: bool,
    },
    /// The host gives its ranges again with `rangesFor` its copy (a
    /// `ConfigEditor` only).
    RangesFor { same_task: bool },
    /// The pause after an echo is over.
    Settle,
    /// The user shows document `doc` (a documents view only).
    Select { doc: u8 },
}

impl Act {
    pub(crate) fn same_task(&self) -> bool {
        match self {
            Act::Deliver { same_task, .. }
            | Act::Reset { same_task, .. }
            | Act::Own { same_task, .. }
            | Act::RangesFor { same_task } => *same_task,
            _ => false,
        }
    }
}

/// The kinds of failure, each a class of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Class {
    /// The locked text changed, or the editor's lock no longer covers it.
    LockOff,
    /// The lock covers text beside the locked text while texts are on
    /// their way.
    OverLockInFlight,
    /// The same once every text came back.
    OverLockAtRest,
    /// A text the host handed back replaced newer typing.
    TypingLost,
    /// The same, the text a listing of one the host gave `replaceValue`
    /// (listed after a newer text came back).
    RevertedByListing,
    /// The same, `replaceValue` given the text the editor held then.
    RevertedBySameListing,
    /// Once every text came back, the host and the editor differ.
    NotConverged,
    /// The same, the host holding a text of its own that is the one it
    /// gave last, given again (see `HostDoc::own`).
    NotConvergedOnGivenAgain,
    /// Ranges past the end of the text they are placed in.
    PastEnd,
    /// A panic.
    Panic,
}

#[derive(Clone, Debug)]
pub(crate) struct Failure {
    pub(crate) class: Class,
    /// The kind of step it failed at (`Reset`, `Reset in the same
    /// task`, `drain` once every text comes back).
    pub(crate) step: String,
    pub(crate) detail: String,
}

impl Class {
    /// The class of a host and an editor that differ at rest; `again`:
    /// the host holds a text of its own that is the one it gave last.
    pub(crate) fn not_converged(again: bool) -> Self {
        if again {
            Class::NotConvergedOnGivenAgain
        } else {
            Class::NotConverged
        }
    }
}

impl Failure {
    pub(crate) fn new(class: Class, detail: impl Into<String>) -> Self {
        Self {
            class,
            step: "drain".to_string(),
            detail: detail.into(),
        }
    }
}

impl Act {
    /// The kind of step, as failures are grouped.
    pub(crate) fn kind(&self) -> String {
        let name = format!("{self:?}");
        let mut name = name
            .split([' ', '{'])
            .next()
            .unwrap_or_default()
            .to_string();
        if let Act::Reset { kind, .. } = self {
            let kind = format!("{kind:?}");
            name = format!("{name} {}", kind.split('(').next().unwrap_or_default());
        }
        if self.same_task() {
            format!("{name} in the same task")
        } else {
            name
        }
    }
}

pub(crate) type Outcome = Result<(), Failure>;

/// The host's lock in its copy `text`, written in `form`: the range and
/// what an update compares it by (its JSON); `None` when the locked text
/// is not there.
pub(crate) fn host_lock(text: &str, form: Form) -> Option<(TextRange, String)> {
    let start = text.find(LOCKED)?;
    let end = start + LOCKED.len();
    let range = TextRange::new(
        TextPosition::from_offset(text, start),
        TextPosition::from_offset(text, end),
    );
    let print = match form {
        Form::Lines => format!(
            "L{}:{}-{}:{}",
            range.start.line, range.start.column, range.end.line, range.end.column
        ),
        Form::Index => format!("I{start}-{end}"),
    };
    Some((range, print))
}

/// Whether `range` is inside `text` (what `ranges_from` and the core
/// check: no "end past the text's end").
pub(crate) fn fits(range: TextRange, text: &str) -> bool {
    let lines: Vec<&str> = text.split('\n').collect();
    let inside = |at: TextPosition| {
        at.line >= 1
            && at.column >= 1
            && lines
                .get(at.line - 1)
                .is_some_and(|line| at.column <= line.len() + 1)
    };
    inside(range.start) && inside(range.end) && range.start <= range.end
}

/// `protect::ranges_in` over positions: `ranges` given with `host_text`
/// are read there; but when they name places it lacks and all fit one of
/// `before` (the texts the last ones came with), they were worked out on
/// the first such one, and are moved from it; ranges fitting none throw
/// ("end past the text's end").
pub(crate) fn read_ranges(
    ranges: &[TextRange],
    host_text: &str,
    before: &[&str],
) -> Result<Vec<TextRange>, Failure> {
    let all_fit = |text: &str| ranges.iter().all(|range| fits(*range, text));
    if !all_fit(host_text)
        && let Some(before) = before
            .iter()
            .find(|before| **before != host_text && all_fit(before))
    {
        return Ok(
            crate::remap::moved_between(before, host_text, Some(ranges.to_vec()), None)
                .0
                .unwrap_or_default(),
        );
    }
    if !all_fit(host_text) {
        return Err(Failure::new(
            Class::PastEnd,
            format!("{ranges:?} read in {host_text:?}"),
        ));
    }
    Ok(ranges.to_vec())
}

/// A newer version of `text`: by `edit`, a line other than the locked one
/// changed, a line added, or one removed.
pub(crate) fn new_version(text: &str, edit: u16) -> String {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let free: Vec<usize> = (0..lines.len())
        .filter(|at| !lines[*at].contains(LOCKED))
        .collect();
    let pick = |n: usize| free[(edit as usize / 3) % n];
    match edit % 3 {
        0 if !free.is_empty() => {
            let at = pick(free.len());
            lines[at].push_str(&format!("v{}", edit % 10));
        }
        1 if free.len() > 1 => {
            let at = pick(free.len());
            lines.remove(at);
        }
        _ => {
            let at = (edit as usize / 3) % (lines.len() + 1);
            lines.insert(at, format!("n{}: 1", edit % 10));
        }
    }
    lines.join("\n")
}

/// The editor's core for one document, as far as locks go.
#[derive(Clone)]
pub(crate) struct Core {
    pub(crate) text: String,
    /// The ranges the host gave (the `read_only_ranges` signal), in the
    /// text they were placed in.
    pub(crate) given: Vec<TextRange>,
    /// The locks where the edits since moved them (`protect().ranges`).
    pub(crate) locks: Vec<Range<usize>>,
    /// New ranges given, not placed yet (the effect waits for the task's
    /// end).
    pub(crate) pending: bool,
}

impl Core {
    pub(crate) fn new(text: &str, given: Vec<TextRange>) -> Result<Self, Failure> {
        let mut core = Self {
            text: text.to_string(),
            given,
            locks: Vec::new(),
            pending: false,
        };
        core.place()?;
        Ok(core)
    }

    /// Places the ranges given in the text (`place_ranges`; a position
    /// past a line's or the text's end is that end, as `to_offset` reads
    /// it: ranges given for another text land somewhere, and the checks
    /// tell where).
    pub(crate) fn place(&mut self) -> Outcome {
        let mut locks = Vec::new();
        for range in &self.given {
            locks.push(range.start.to_offset(&self.text)..range.end.to_offset(&self.text));
        }
        self.locks = locks;
        self.pending = false;
        Ok(())
    }

    /// The effect placing new ranges runs (the task is over).
    pub(crate) fn flush(&mut self) -> Outcome {
        if self.pending {
            self.place()?;
        }
        Ok(())
    }

    /// An edit replacing `start..end` with `insert`: refused when it
    /// touches a lock (the guard), else made, the locks moved with it.
    pub(crate) fn edit(&mut self, start: usize, end: usize, insert: &str) -> bool {
        let touches = self.locks.iter().any(|lock| {
            !lock.is_empty()
                && if start == end {
                    lock.start < start && start < lock.end
                } else {
                    start < lock.end && end > lock.start
                }
        });
        if touches {
            return false;
        }
        let grown = insert.len() as isize - (end - start) as isize;
        for lock in &mut self.locks {
            if end <= lock.start {
                lock.start = (lock.start as isize + grown) as usize;
                lock.end = (lock.end as isize + grown) as usize;
            }
        }
        self.text.replace_range(start..end, insert);
        true
    }

    /// A text from the host (`take_host_value`): shown, the ranges given
    /// placed in it.
    pub(crate) fn host_value(&mut self, text: &str) -> Outcome {
        if text == self.text {
            return Ok(());
        }
        text.clone_into(&mut self.text);
        self.place()
    }

    /// New ranges from the host: placed once the task is over.
    pub(crate) fn set_given(&mut self, ranges: Vec<TextRange>) {
        if ranges != self.given {
            self.given = ranges;
            self.pending = true;
        }
    }

    /// Checks the locked text is there once and locked, and (`exact`)
    /// that nothing beside it is.
    pub(crate) fn check(&self, what: &str, exact: Option<Class>) -> Outcome {
        check_locks(&self.text, &self.locks, what, exact)
    }
}

/// See [`Core::check`]; `locks` are offsets of `text`.
pub(crate) fn check_locks(
    text: &str,
    locks: &[Range<usize>],
    what: &str,
    exact: Option<Class>,
) -> Outcome {
    if text.matches(LOCKED).count() != 1 {
        return Err(Failure::new(
            Class::LockOff,
            format!("{what}: the locked text changed: {text:?}"),
        ));
    }
    let start = text.find(LOCKED).unwrap_or_default();
    let end = start + LOCKED.len();
    let covered = (start..end).all(|at| locks.iter().any(|lock| lock.contains(&at)));
    if !covered {
        return Err(Failure::new(
            Class::LockOff,
            format!("{what}: locks {locks:?} miss {start}..{end} of {text:?}"),
        ));
    }
    if let Some(class) = exact {
        let over = locks
            .iter()
            .any(|lock| !lock.is_empty() && (lock.start < start || lock.end > end));
        if over {
            return Err(Failure::new(
                class,
                format!("{what}: locks {locks:?} beyond {start}..{end} of {text:?}"),
            ));
        }
    }
    Ok(())
}

/// Where a user action edits `text`: the span replaced and the text put
/// there; `None` when it does nothing.
pub(crate) fn user_edit(act: &Act, text: &str) -> Option<(usize, usize, String)> {
    let at = |at: u16| at as usize % (text.len() + 1);
    match act {
        Act::Type { at: place, ch } => Some((at(*place), at(*place), ch.to_string())),
        Act::Enter { at: place } => Some((at(*place), at(*place), "\n".to_string())),
        Act::Delete { at: place, len } => {
            let start = at(*place).min(text.len().saturating_sub(1));
            let end = (start + (*len as usize % 3) + 1).min(text.len());
            (start < end).then(|| (start, end, String::new()))
        }
        Act::MoveLine { line } => {
            let lines: Vec<&str> = text.split('\n').collect();
            if lines.len() < 2 {
                return None;
            }
            let first = *line as usize % (lines.len() - 1);
            let start: usize = lines[..first].iter().map(|line| line.len() + 1).sum();
            let end = start + lines[first].len() + 1 + lines[first + 1].len();
            Some((
                start,
                end,
                format!("{}\n{}", lines[first + 1], lines[first]),
            ))
        }
        _ => None,
    }
}
