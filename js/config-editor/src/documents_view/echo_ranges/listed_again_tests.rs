//! QA 35, HO35-1 and FW35-1: ranges a host works out in an echo of a text
//! the user typed past, listed again (the same echo twice, two echoes
//! giving the same ranges, three lists, older echoes after the newest),
//! with typing between the lists. The list as a
//! documents view reads it (`given_ranges::Listed`, then [`take`]), and
//! the editor as it takes its ranges (only ranges that differ from the
//! ones it was given replace the ones it moved), over every language,
//! ranges by lines and by index, and typing above, next to and below the
//! lock: the lock stays on its field.

use config_editor_leptos::{Document, TextPosition, TextRange};

use super::take;
use super::tests::SAMPLES;
use crate::given_ranges::{Again, Listed, stay};
use crate::remap::moved_between;

/// How the host writes its ranges (what a list is compared by).
#[derive(Clone, Copy, Debug)]
enum Form {
    Lines,
    Index,
}

/// Where the user types between lists: a line above the lock, the line
/// right before it, or a line below it.
#[derive(Clone, Copy, Debug)]
enum Place {
    Above,
    At,
    Below,
}

/// The lists a host sends, each an index into the texts the editor sent
/// (`NEWEST`: the newest echo, `NEWEST - 1` the one before it, `MIDDLE`
/// the one halfway).
const LISTS: &[(&str, &[usize])] = &[
    ("the same echo twice", &[NEWEST, NEWEST]),
    ("two echoes", &[NEWEST - 1, NEWEST]),
    ("three lists", &[NEWEST, NEWEST, NEWEST]),
    // FW35-1: older echoes after the newest, each with its own ranges.
    ("newest, first, middle", &[NEWEST, 0, MIDDLE]),
    ("newest, second, middle", &[NEWEST, 1, MIDDLE]),
    ("newest, first, second", &[NEWEST, 0, 1]),
    ("first, newest, first", &[0, NEWEST, 0]),
];
const NEWEST: usize = usize::MAX;
const MIDDLE: usize = usize::MAX / 2;

/// The texts the editor sends as `line` is typed on a new line after the
/// line `after` of `text`: the line break, then each character.
fn typing(text: &str, after: &str, line: &str) -> Vec<String> {
    let at = text.find(after).expect("after") + after.len();
    let (head, tail) = text.split_at(at);
    (0..=line.len())
        .map(|end| format!("{head}\n{}{tail}", &line[..end]))
        .collect()
}

/// The locked line of `text` (the sample's line `lock_line` of `first`)
/// and its range, worked out in `text` as a host's `rangeOf` would.
fn lock_in(text: &str, first: &str, lock: TextRange) -> TextRange {
    let locked = first.lines().nth(lock.start.line - 1).expect("locked line");
    let line = text.lines().position(|line| line == locked).expect("lock") + 1;
    TextRange::new(
        TextPosition::new(line, lock.start.column),
        TextPosition::new(line, lock.end.column),
    )
}

/// The ranges as the host writes them for `text`: what lists compare.
fn print(range: TextRange, text: &str, form: Form) -> String {
    match form {
        Form::Lines => format!("{range:?}"),
        Form::Index => format!(
            "{}..{}",
            range.start.to_offset(text),
            range.end.to_offset(text)
        ),
    }
}

/// `text` with the line `line` typed at `place` relative to the lock.
fn typed_at(text: &str, lock: TextRange, place: Place, line: &str) -> String {
    let mut lines: Vec<&str> = text.split('\n').collect();
    let at = match place {
        Place::Above => 1,
        Place::At => lock.start.line - 1,
        Place::Below => lock.start.line,
    };
    lines.insert(at, line);
    lines.join("\n")
}

/// A documents view's editor of one document, and the host listing it.
struct View<'a> {
    first: &'a str,
    lock: TextRange,
    file: &'a str,
    form: Form,
    listed: Listed,
    /// The ranges the editor was given last (its `read_only_ranges`
    /// signal: set only when a list's differ).
    given: Vec<TextRange>,
    /// The ranges as the editor moved them with each edit.
    ranges: Vec<TextRange>,
    text: String,
}

impl<'a> View<'a> {
    fn new(file: &'a str, first: &'a str, lock: TextRange, form: Form) -> Self {
        let mut listed = Listed::default();
        listed.read("m", Some(print(lock, first, form)), vec![lock]);
        listed.finish();
        Self {
            first,
            lock,
            file,
            form,
            listed,
            given: vec![lock],
            ranges: vec![lock],
            text: first.to_string(),
        }
    }

    /// The user types: the editor sends `text`, and moves its ranges.
    fn type_to(&mut self, text: &str) {
        let (ranges, _) = moved_between(&self.text, text, Some(self.ranges.clone()), None);
        self.ranges = ranges.expect("ranges");
        text.clone_into(&mut self.text);
    }

    /// The host lists `echo` (a text the editor sent and typed past) with
    /// its lock worked out in it.
    fn list_echo(&mut self, echo: &str) {
        let range = lock_in(echo, self.first, self.lock);
        let given = print(range, echo, self.form);
        // As `documents_list::with_protection` reads it: an echo does not
        // replace the editor's text.
        let replaces = false;
        let (ranges, again) = match self.listed.again("m", Some(&given)) {
            Some(ranges) if stay(true, replaces) => (ranges, Some(Again::Stayed)),
            last => (vec![range], last.map(Again::Read)),
        };
        self.listed.read("m", Some(given), ranges.clone());
        self.listed.finish();
        let mut document = Document::new("m", self.file, echo.to_string()).read_only_ranges(ranges);
        if let Some(kept) = take(&mut document, &self.text, again, false) {
            self.listed.kept("m", kept);
        }
        assert_eq!(document.value, self.text);
        if document.read_only_ranges != self.given {
            self.given.clone_from(&document.read_only_ranges);
            self.ranges = document.read_only_ranges;
        }
    }

    fn assert_locked(&self, what: &str) {
        let lock = lock_in(&self.text, self.first, self.lock);
        assert_eq!(self.ranges, vec![lock], "{what}\n{}", self.text);
    }
}

#[test]
fn ranges_read_in_an_echo_and_listed_again_stay_on_their_field() {
    for sample in SAMPLES {
        // Two lines typed above the lock, as FW35-1's host did.
        let mut texts = sample.typed();
        let first = texts.last().expect("typed").clone();
        let second = format!("{}2", sample.line);
        texts.extend(typing(&first, sample.line, &second));
        let lock = sample.lock();
        for form in [Form::Lines, Form::Index] {
            for place in [Place::Above, Place::At, Place::Below] {
                for &(lists, order) in LISTS {
                    let what = format!("{} {form:?} {place:?} {lists}", sample.file);
                    let mut view = View::new(sample.file, sample.text, lock, form);
                    for text in &texts {
                        view.type_to(text);
                    }
                    // Typed on past the echoes: they are older than the text.
                    let more = |view: &View, n: usize| {
                        let at = lock_in(&view.text, sample.text, lock);
                        typed_at(&view.text, at, place, &format!("{}{n}", sample.line))
                    };
                    let next = more(&view, 0);
                    view.type_to(&next);
                    let last = texts.len() - 1;
                    for (n, &echo) in order.iter().enumerate() {
                        let echo = match echo {
                            NEWEST => last,
                            MIDDLE => last / 2,
                            echo if echo > MIDDLE => last - (NEWEST - echo),
                            echo => echo,
                        };
                        view.list_echo(&texts[echo]);
                        view.assert_locked(&format!("{what}: list {n}"));
                        let next = more(&view, n + 1);
                        view.type_to(&next);
                        view.assert_locked(&format!("{what}: typed after list {n}"));
                    }
                }
            }
        }
    }
}
