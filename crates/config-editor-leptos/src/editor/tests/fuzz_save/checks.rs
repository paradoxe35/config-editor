//! The save promise, checked after each save of a session: the host's
//! text reads, holds what the view shows, writes every value the session
//! left alone (and every value it moved or copied) in its form, and keeps
//! the comments of what the session left alone; or else the save waits
//! with a problem shown.

use crate::editor::language::pointer_key;
use crate::editor::paths::FieldPath;
use leptos::prelude::*;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::editor::bridge::Bridge;
use crate::editor::languages::Language;
use crate::editor::state::EditorState;

/// What a save broke.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) enum Broken {
    /// The saved host does not read in its language.
    Unreadable,
    /// The saved host reads otherwise than the view shows.
    Meaning,
    /// A value left alone (or moved, or copied) is written otherwise.
    Form,
    /// A comment of what was left alone is gone.
    Comment,
    /// The save waits with no problem shown.
    SilentWait,
    /// A view back to a text shown or saved before saves another host
    /// text than that one stood for.
    NotRestored,
    /// The editor panicked.
    Panic,
}

/// A broken save: what broke, and how.
#[derive(Clone, Debug)]
pub(super) struct Failure {
    pub(super) broken: Broken,
    /// The form (or comment) concerned, as the host wrote it first.
    pub(super) was: String,
    /// How the save writes it (a form), empty otherwise.
    pub(super) now: String,
    /// The value's pointer (a form's), empty otherwise.
    pub(super) pointer: String,
    /// Whether the value was moved or copied (not left alone), and whether
    /// its parent is new (renamed, or moved itself).
    pub(super) moved: bool,
    pub(super) new_parent: bool,
    /// The language on screen, its text, and the host's text saved.
    pub(super) view: Option<Language>,
    pub(super) view_text: String,
    pub(super) saved: String,
    pub(super) detail: String,
}

impl Failure {
    pub(super) fn new(
        broken: Broken,
        was: impl Into<String>,
        detail: impl Into<String>,
    ) -> Box<Self> {
        Box::new(Self {
            broken,
            was: was.into(),
            now: String::new(),
            pointer: String::new(),
            moved: false,
            new_parent: false,
            view: None,
            view_text: String::new(),
            saved: String::new(),
            detail: detail.into(),
        })
    }

    fn written(
        mut self: Box<Self>,
        now: &str,
        pointer: &str,
        moved: bool,
        new_parent: bool,
    ) -> Box<Self> {
        self.now = now.to_string();
        self.pointer = pointer.to_string();
        self.moved = moved;
        self.new_parent = new_parent;
        self
    }
}

/// What the session started from, read once.
pub(super) struct Start {
    pub(super) host: Language,
    pub(super) text: String,
    value: Value,
    forms: HashMap<String, String>,
    /// The comments, with the items of the line each belongs to.
    comments: Vec<(String, Vec<String>)>,
    /// The comments the first view shows.
    shown_comments: Vec<String>,
    /// The text the session's views showed, each switch's added.
    shown: String,
    /// The names a save so far held a value under that the start did not.
    made: std::cell::RefCell<std::collections::HashSet<String>>,
    /// The scalars a save so far held otherwise than the start.
    changed: std::cell::RefCell<std::collections::HashSet<String>>,
    /// The items a save so far did not leave alone: the lines of their
    /// comments were edited, whatever a later save holds there.
    edited: std::cell::RefCell<std::collections::HashSet<String>>,
    /// Each value's text as a view showed it (at the start, each switch).
    shown_values: std::cell::RefCell<std::collections::HashSet<String>>,
    /// The views' texts at each save since: what the user wrote.
    written: std::cell::RefCell<String>,
    /// The texts of the values the user wrote as no view showed them
    /// (`350.00` typed over `350.0`): their own spelling, whatever views
    /// built from a save show it after.
    typed: std::cell::RefCell<std::collections::HashSet<String>>,
    /// Each view text shown or saved in sync, with its language and the
    /// host text it stands for: a view back to one saves that text.
    synced: std::cell::RefCell<Vec<(Language, String, String)>>,
}

impl Start {
    pub(super) fn new(host: Language, text: &str, view_text: &str, view: Language) -> Option<Self> {
        if text.trim().is_empty() {
            return None;
        }
        let value = host.parse(text).ok()?;
        let forms = forms(host, text, &value);
        let comments = owned_comments(host, text, &value);
        Some(Self {
            host,
            text: text.to_string(),
            value,
            forms,
            comments,
            shown_comments: comment_bodies(view, view_text),
            shown: view_text.to_string(),
            written: Default::default(),
            typed: Default::default(),
            changed: Default::default(),
            edited: Default::default(),
            made: Default::default(),
            shown_values: std::cell::RefCell::new(value_texts(view, view_text)),
            synced: std::cell::RefCell::new(vec![(view, view_text.to_string(), text.to_string())]),
        })
    }
}

impl Start {
    /// The session goes on from the host's own text typed in its language.
    pub(super) fn rebase(&mut self, text: &str) {
        if text == self.text {
            return;
        }
        if let Some(start) = Start::new(self.host, text, text, self.host) {
            *self = start;
        }
    }

    /// The user wrote `view_text` (in `view`, before a save or a switch).
    pub(super) fn note(&self, view: Language, view_text: &str) {
        self.written.borrow_mut().push_str(view_text);
        let shown = self.shown_values.borrow();
        let typed: Vec<String> = value_texts(view, view_text)
            .into_iter()
            .filter(|text| !shown.contains(text))
            .collect();
        self.typed.borrow_mut().extend(typed);
    }

    /// A switch shows the comments `view_text` writes.
    /// `host_text`: the host's text the view stands for, when in sync.
    pub(super) fn seen(&mut self, view: Language, view_text: &str, host_text: Option<&str>) {
        if let Some(host_text) = host_text {
            self.synced
                .borrow_mut()
                .push((view, view_text.to_string(), host_text.to_string()));
        }
        // The values of a view the switch built from the host's text (not
        // the user's edits carried over).
        let built = host_text.is_some_and(|host_text| {
            Bridge::new(self.host, view, None)
                .and_then(|bridge| bridge.to_edit(host_text).ok())
                .is_some_and(|shown| shown.text == view_text)
        });
        if built {
            self.shown_values
                .borrow_mut()
                .extend(value_texts(view, view_text));
        }
        self.shown_comments = comment_bodies(view, view_text);
        self.shown.push_str(view_text);
    }
}

/// Every pointer in `value` with its value, parents first (the root left
/// out).
fn pointers(value: &Value) -> Vec<(String, &Value)> {
    fn walk<'a>(value: &'a Value, pointer: String, out: &mut Vec<(String, &'a Value)>) {
        if !pointer.is_empty() {
            out.push((pointer.clone(), value));
        }
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    let key = pointer_key(key);
                    walk(child, format!("{pointer}/{key}"), out);
                }
            }
            Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    walk(child, format!("{pointer}/{index}"), out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(value, String::new(), &mut out);
    out
}

/// The character range `start..end` (zero-based) of line `line` (one-based).
fn line_text(text: &str, line: usize) -> Option<&str> {
    text.split('\n').nth(line.checked_sub(1)?)
}

fn characters(line: &str, from: usize, to: usize) -> String {
    line.chars()
        .skip(from)
        .take(to.saturating_sub(from))
        .collect()
}

/// Runs of spaces as one, ends trimmed, a trailing comma dropped.
fn squeezed(text: &str) -> String {
    let text = text.trim().trim_end_matches(',').trim_end();
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// How `text` writes each of `value`'s pointers: from its key (on the
/// value's line) through its value's end for a scalar, through the line's
/// end for a container (its comment left out).
fn forms(language: Language, text: &str, value: &Value) -> HashMap<String, String> {
    let all = pointers(value);
    let wanted: Vec<String> = all.iter().map(|(pointer, _)| pointer.clone()).collect();
    let wanted_paths: Vec<FieldPath> = wanted.iter().map(FieldPath::from).collect();
    let behavior = language.behavior();
    let keys = behavior.key_spans(text, &wanted_paths);
    let values = behavior.value_spans(text, &wanted_paths);
    let comments = behavior.comments(text);
    let mut out = HashMap::new();
    for (((pointer, child), key), at) in all.iter().zip(keys).zip(values) {
        let empty = child.as_object().is_some_and(|map| map.is_empty())
            || child.as_array().is_some_and(|items| items.is_empty());
        let Some(at) = at.filter(|_| !empty) else {
            continue;
        };
        let Some(line) = line_text(text, at.line) else {
            continue;
        };
        let from = match key {
            Some(key) if key.line == at.line && key.start_column <= at.start_column => {
                key.start_column - 1
            }
            _ => at.start_column - 1,
        };
        let to = if child.is_object() || child.is_array() {
            // The line's end, before a comment on it.
            let line_start: usize = text
                .split('\n')
                .take(at.line - 1)
                .map(|l| l.len() + 1)
                .sum();
            let end = comments
                .iter()
                .find(|range| range.start >= line_start && range.start <= line_start + line.len())
                .map_or(line.len(), |range| range.start - line_start);
            line[..end].chars().count()
        } else {
            at.end_column - 1
        };
        let mut form = characters(line, from, to);
        if child.is_object() || child.is_array() {
            form = inline_part(&form);
        }
        out.insert(pointer.clone(), squeezed(&without_namespaces(&form)));
    }
    out
}

/// A container's text on its first line: through its closing bracket when
/// it closes on the line, else through its opener and `…`.
fn inline_part(text: &str) -> String {
    let mut depth = 0usize;
    let mut quote = None;
    let mut out = String::new();
    for character in text.chars() {
        out.push(character);
        match (quote, character) {
            (Some(open), _) if character == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(character),
            (None, '{' | '[' | '(') => depth += 1,
            (None, '}' | ']' | ')') => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return out;
                }
            }
            _ => {}
        }
    }
    match text.find(['{', '[']) {
        Some(at) if depth > 0 => format!("{}…", &text[..=at]),
        _ => out,
    }
}

/// `text` without XML namespace declarations: a prefix's declaration
/// goes with an element moved out of the one declaring it.
fn without_namespaces(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(" xmlns") {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        // Past `xmlns…="…"`.
        let end = after
            .find('"')
            .and_then(|open| after[open + 1..].find('"').map(|close| open + close + 2))
            .unwrap_or(after.len());
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// A comment's words, its markers left out.
fn body(comment: &str) -> String {
    comment
        .trim()
        .trim_start_matches("<!--")
        .trim_end_matches("-->")
        .trim_start_matches('#')
        .trim_start_matches("//")
        .trim()
        .to_string()
}

/// The comments `text` writes, by their words.
pub(super) fn comment_bodies(language: Language, text: &str) -> Vec<String> {
    language
        .behavior()
        .comments(text)
        .into_iter()
        .filter_map(|range| text.get(range))
        .map(body)
        .filter(|body| !body.is_empty())
        .collect()
}

/// Each comment of `text` with the items of the line it belongs to: its
/// own line when code is written before it there (or after it, on its
/// last line); else the next line writing an item, unless the comment is
/// indented deeper than that line (it ends the block above) or no line
/// below writes one: then the last line above writing one.
fn owned_comments(language: Language, text: &str, value: &Value) -> Vec<(String, Vec<String>)> {
    let all = pointers(value);
    let wanted: Vec<String> = all.iter().map(|(pointer, _)| pointer.clone()).collect();
    let wanted_paths: Vec<FieldPath> = wanted.iter().map(FieldPath::from).collect();
    let behavior = language.behavior();
    let keys = behavior.key_spans(text, &wanted_paths);
    let values = behavior.value_spans(text, &wanted_paths);
    // The items written on each line (from 1).
    let mut on_line: std::collections::BTreeMap<usize, Vec<String>> = Default::default();
    for (pointer, (key, at)) in wanted.iter().zip(keys.iter().zip(values.iter())) {
        let mut lines: Vec<usize> = key.iter().chain(at.iter()).map(|span| span.line).collect();
        lines.sort_unstable();
        lines.dedup();
        for line in lines {
            on_line.entry(line).or_default().push(pointer.clone());
        }
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let indent = |line: usize| {
        let text = lines.get(line - 1).copied().unwrap_or("");
        text.len() - text.trim_start().len()
    };
    behavior
        .comments(text)
        .into_iter()
        .filter_map(|range| {
            let words = body(text.get(range.clone())?);
            if words.is_empty() {
                return None;
            }
            let first = text[..range.start].matches('\n').count() + 1;
            let last = text[..range.end].matches('\n').count() + 1;
            let line_start = text[..range.start].rfind('\n').map_or(0, |at| at + 1);
            let line_end = text[range.end..]
                .find('\n')
                .map_or(text.len(), |at| range.end + at);
            let owner = if !text[line_start..range.start].trim().is_empty() {
                Some(first)
            } else if !text[range.end..line_end].trim().is_empty() {
                Some(last)
            } else {
                let above = on_line.range(..first).next_back().map(|(line, _)| *line);
                match on_line.range(last + 1..).next().map(|(line, _)| *line) {
                    Some(below) if indent(first) > indent(below) => above.or(Some(below)),
                    Some(below) => Some(below),
                    None => above,
                }
            };
            let items = owner
                .and_then(|line| on_line.get(&line))
                .cloned()
                .unwrap_or_default();
            Some((words, items))
        })
        .collect()
}

/// Whether the save (reading `now`) leaves the item at `pointer` of
/// `start` alone, at the same pointer: a scalar or list item (and each
/// list item holding it) holding what it held, a map or list still one.
fn left_alone(start: &Value, now: &Value, pointer: &str) -> bool {
    let in_list = |pointer: &str| {
        pointer
            .rsplit_once('/')
            .and_then(|(parent, _)| start.pointer(parent))
            .is_some_and(Value::is_array)
    };
    let mut prefix = pointer;
    loop {
        if in_list(prefix) && start.pointer(prefix) != now.pointer(prefix) {
            return false;
        }
        match prefix.rfind('/') {
            Some(at) if at > 0 => prefix = &prefix[..at],
            _ => break,
        }
    }
    match (start.pointer(pointer), now.pointer(pointer)) {
        (Some(Value::Object(_)), Some(Value::Object(_)))
        | (Some(Value::Array(_)), Some(Value::Array(_))) => true,
        (Some(Value::Object(_) | Value::Array(_)), _) | (None, _) => false,
        (was, is) => was == is,
    }
}

/// What the host holds: the last text `on_change` told it.
pub(super) type HostCopy = std::sync::Arc<std::sync::Mutex<String>>;

/// `meant` (the view's value) with each scalar that `shown` (the saved
/// host read) holds with the same text in another type, as the session's
/// start held it, taken from `shown`.
fn host_kept(meant: &Value, shown: &Value, start: Option<&Value>) -> Value {
    // A list of one the view writes as its item (XML).
    let start = match start {
        Some(Value::Array(items)) if items.len() == 1 && !shown.is_array() => items.first(),
        other => other,
    };
    match (meant, shown) {
        (Value::Object(map), Value::Object(held)) => Value::Object(
            map.iter()
                .map(|(key, child)| {
                    let kept = match held.get(key) {
                        Some(held) => {
                            host_kept(child, held, start.and_then(|start| start.get(key)))
                        }
                        None => child.clone(),
                    };
                    (key.clone(), kept)
                })
                .collect(),
        ),
        (Value::Array(items), Value::Array(held)) if items.len() == held.len() => Value::Array(
            items
                .iter()
                .zip(held)
                .enumerate()
                .map(|(index, (child, held))| {
                    host_kept(child, held, start.and_then(|start| start.get(index)))
                })
                .collect(),
        ),
        _ if meant != shown
            && scalar_text(meant).is_some()
            && scalar_text(meant) == scalar_text(shown)
            && start == Some(shown) =>
        {
            shown.clone()
        }
        _ => meant.clone(),
    }
}

/// A scalar's text, whatever its type (`"8080"` and `8080` alike).
fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// Numbers as floats: a view and its host may write `1` and `1.0` alike.
fn loose(value: &Value) -> Value {
    match value {
        Value::Number(number) => number.as_f64().map_or(value.clone(), Value::from),
        Value::Array(items) => Value::Array(items.iter().map(loose).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, child)| (key.clone(), loose(child)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// `value` with each list of one map that `view` writes as that map (an
/// HCL block written once) replaced by the map.
fn blocks_as_written(value: &Value, view: &Value) -> Value {
    match (value, view) {
        (Value::Array(items), Value::Object(_)) if items.len() == 1 && items[0].is_object() => {
            blocks_as_written(&items[0], view)
        }
        (Value::Array(items), Value::Array(written)) => Value::Array(
            items
                .iter()
                .enumerate()
                .map(|(index, item)| match written.get(index) {
                    Some(written) => blocks_as_written(item, written),
                    None => item.clone(),
                })
                .collect(),
        ),
        (Value::Object(map), Value::Object(written)) => Value::Object(
            map.iter()
                .map(|(key, child)| {
                    let child = match written.get(key) {
                        Some(written) => blocks_as_written(child, written),
                        None => child.clone(),
                    };
                    (key.clone(), child)
                })
                .collect(),
        ),
        _ => value.clone(),
    }
}

/// `value` with every one-item list replaced by its item.
/// HCL writes a list of one map as one block, read back as the map.
fn lone_blocks(value: &Value) -> Value {
    match value {
        Value::Array(items) if items.len() == 1 && items[0].is_object() => lone_blocks(&items[0]),
        Value::Array(items) => Value::Array(items.iter().map(lone_blocks).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, child)| (key.clone(), lone_blocks(child)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// XML writes every value as text: a text a view reads as a number or a
/// boolean is that, and an empty element is null, as the view's null,
/// empty string or empty map is.
fn lone_items(value: &Value) -> Value {
    match value {
        Value::Array(items) if items.len() == 1 => lone_items(&items[0]),
        Value::Array(items) => Value::Array(items.iter().map(lone_items).collect()),
        Value::Object(map) if map.is_empty() => Value::Null,
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, child)| (key.clone(), lone_items(child)))
                .collect(),
        ),
        Value::String(text) if text.trim().is_empty() => Value::Null,
        Value::String(text) => match text.trim() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            number => number
                .parse::<f64>()
                .ok()
                .filter(|number| number.is_finite())
                .map_or_else(|| value.clone(), Value::from),
        },
        Value::Bool(_) | Value::Number(_) | Value::Null => value.clone(),
    }
}

/// The anchors or aliases (`&name`, `*name`) a YAML form names.
fn anchors(form: &str) -> Vec<&str> {
    form.split_whitespace()
        .filter_map(|word| word.strip_prefix('&').or_else(|| word.strip_prefix('*')))
        .collect()
}

/// Saves checked in a view (written, then waiting), for the summary.
pub(super) static WRITTEN: AtomicUsize = AtomicUsize::new(0);
pub(super) static WAITING: AtomicUsize = AtomicUsize::new(0);

/// The save promise after a save, or what it broke.
pub(super) fn check(
    state: EditorState,
    start: &Start,
    held: &HostCopy,
) -> Result<(), Box<Failure>> {
    check_save(state, start, held).map_err(|mut failure| {
        failure.view = Some(state.sync().edit_language.get_untracked());
        failure.view_text = state.doc().text.get_untracked();
        failure.saved = held.lock().unwrap().clone();
        failure
    })
}

fn check_save(state: EditorState, start: &Start, held: &HostCopy) -> Result<(), Box<Failure>> {
    start
        .written
        .borrow_mut()
        .push_str(&state.doc().text.get_untracked());
    let host = start.host;
    let view = state.sync().edit_language.get_untracked();
    let view_text = state.doc().text.get_untracked();
    if view != host && !state.sync().in_sync.get_untracked() {
        WAITING.fetch_add(1, Ordering::Relaxed);
        // Waiting is allowed, with a problem shown.
        let shown = !state.sync().conversion_problems.get_untracked().is_empty()
            || !state.sync().load_problems.get_untracked().is_empty();
        if shown || view.parse(&view_text).is_err() {
            return Ok(());
        }
        return Err(Failure::new(
            Broken::SilentWait,
            "",
            "the save waits, no problem shown",
        ));
    }
    if view != host {
        WRITTEN.fetch_add(1, Ordering::Relaxed);
    }
    let saved = held.lock().unwrap().clone();
    // GetPut: a view back to a text it showed or saved (an undo) saves
    // the host text that text stood for, exactly.
    if view != host {
        let stood_for = start
            .synced
            .borrow()
            .iter()
            .rev()
            .find(|(language, text, _)| *language == view && *text == view_text)
            .map(|(_, _, host_text)| host_text.clone());
        if let Some(stood_for) = stood_for
            && stood_for != saved
        {
            let mut failure = Failure::new(
                Broken::NotRestored,
                "",
                "the view is back to a text it had, the host is not",
            );
            failure.now = stood_for;
            return Err(failure);
        }
        start
            .synced
            .borrow_mut()
            .push((view, view_text.clone(), saved.clone()));
    }
    let Ok(now) = host.parse(&saved) else {
        // Typed in the host's language: the user's own text; or the empty
        // document, shown empty.
        let empty = saved.trim().is_empty()
            && state
                .doc()
                .text
                .with_untracked(|text| text.trim().is_empty());
        if view == host || empty {
            return Ok(());
        }
        return Err(Failure::new(
            Broken::Unreadable,
            "",
            "the saved host does not read",
        ));
    };
    if view != host {
        let bridge = Bridge::new(host, view, None).expect("two languages");
        // An empty host is shown as an empty view: it holds what the
        // host reads it as (a view emptied to `{}` saves an empty host).
        let shown = if saved.trim().is_empty() && !view_text.trim().is_empty() {
            Some(now.clone())
        } else {
            bridge
                .to_edit(&saved)
                .ok()
                .and_then(|conversion| view.parse(&conversion.text).ok())
        };
        let meant = view.parse(&view_text).ok();
        // XML writes a list of one as its item.
        let xml = host.label() == "XML" || view.label() == "XML";
        let hcl = host.label() == "HCL";
        let read = |value: &Value| {
            if xml {
                loose(&lone_items(value))
            } else if hcl {
                loose(&lone_blocks(value))
            } else {
                loose(value)
            }
        };
        // An HCL view's block written once is also its list's only item
        // (`Repeats::lone_item`), which the host may hold as a list.
        let shown = match (&shown, &meant) {
            (Some(shown), Some(meant)) if view.label() == "HCL" => {
                Some(blocks_as_written(shown, meant))
            }
            _ => shown,
        };
        // A scalar the view reads with the same text as the host's start
        // but another type (XML text read as a number by a switch) is the
        // host's value, kept as it was (lossless, see `check_forms`).
        let meant = match (&shown, meant) {
            (Some(shown), Some(meant)) => Some(host_kept(&meant, shown, Some(&start.value))),
            (_, meant) => meant,
        };
        if shown.as_ref().map(read) != meant.as_ref().map(read) {
            return Err(Failure::new(
                Broken::Meaning,
                "",
                "the saved host reads otherwise than the view",
            ));
        }
    }
    check_forms(start, &saved, &now, (view, &view_text))?;
    check_comments(start, view, &view_text, &saved, &now)
}

/// Promise 3: every value left alone keeps its written form; a moved or
/// copied one its source's.
fn check_forms(
    start: &Start,
    saved: &str,
    now: &Value,
    (view, view_text): (Language, &str),
) -> Result<(), Box<Failure>> {
    let after = forms(start.host, saved, now);
    // What the user wrote in the view: a value spelled there as no view
    // showed it (`1000.0` split by Enter and joined back as `1000.`), or a
    // map whose keys the view holds in another order (lines cut and
    // pasted over others): the save writes it as the user wrote it, in
    // the host's spelling of that.
    let view_value = view.parse(view_text).ok();
    let view_spans = |pointer: &str| {
        let span = view
            .behavior()
            .value_spans(view_text, &[FieldPath::from(pointer)])
            .pop()
            .flatten()?;
        let line = line_text(view_text, span.line)?;
        Some(characters(line, span.start_column - 1, span.end_column - 1))
    };
    // A map the view holds (at `pointer`) with its keys in another order
    // than the start held it (at `from`).
    let reordered = |pointer: &str, from: &str| match (
        start.value.pointer(from),
        view_value.as_ref().and_then(|value| value.pointer(pointer)),
    ) {
        (Some(Value::Object(was)), Some(Value::Object(is))) => {
            let order = |map: &serde_json::Map<String, Value>,
                         other: &serde_json::Map<String, Value>| {
                map.keys()
                    .filter(|key| other.contains_key(*key))
                    .cloned()
                    .collect::<Vec<_>>()
            };
            order(was, is) != order(is, was)
        }
        _ => false,
    };
    let users_own = |pointer: &str| {
        let respelled = now
            .pointer(pointer)
            .is_some_and(|value| !value.is_object() && !value.is_array())
            && view_spans(pointer).is_some_and(|spelled| {
                !spelled.trim().is_empty() && !start.shown_values.borrow().contains(spelled.trim())
            });
        respelled || reordered(pointer, pointer)
    };
    let before = &start.forms;
    // Anchors and aliases whose values all stay may not be written out.
    let changed_anchor = |form: &str| {
        anchors(form).iter().any(|name| {
            before.iter().any(|(pointer, other)| {
                anchors(other).contains(name)
                    && start.value.pointer(pointer) != now.pointer(pointer)
            })
        })
    };
    // A spelling the user wrote in the view (`1.5` made `1.50`): their
    // own form, not the save's.
    let retyped = |form: &str| {
        let spelled = form
            .rsplit(['=', ':', '>'])
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches(['"', '\'']);
        !spelled.is_empty()
            && ((view_text.contains(spelled) || start.written.borrow().contains(spelled))
                && !start.shown.contains(spelled)
                || start.typed.borrow().contains(spelled))
    };
    // A list item's name: its list's and its index.
    let named = |pointer: &str| {
        let mut segments = pointer.rsplit('/');
        let last = segments.next().unwrap_or("");
        match last.parse::<usize>() {
            Ok(_) => format!("{}/#", segments.next().unwrap_or("")),
            Err(_) => last.to_string(),
        }
    };
    // A scalar an earlier save held otherwise (the user changed it and
    // typed it back), or that the user spelled anew, is the user's: the
    // save writes what they typed. An undo back is judged by GetPut,
    // exactly.
    let changed_before = start.changed.borrow().clone();
    {
        let mut changed = start.changed.borrow_mut();
        for (pointer, was) in pointers(&start.value) {
            // Changed, or spelled by the user (their spelling, saved, is
            // the host's from then on, in the views built from it too).
            let changed_now = now.pointer(&pointer) != Some(was);
            if !was.is_object() && !was.is_array() && (changed_now || users_own(&pointer)) {
                changed.insert(pointer);
            } else if reshaped(was, now.pointer(&pointer)) {
                // A map or list an earlier save held with other keys or
                // items, or not at all (its entries moved out, then back):
                // the user rebuilt it, and its layout is the save's.
                changed.insert(pointer);
            }
        }
    }
    // A new key an earlier save held with a value the session did not start
    // with under that name (the user made it: a line pasted over another
    // joined their values) may be where a later value of that name comes
    // from: its form is the one that save wrote, not the start's.
    let made_before = start.made.borrow().clone();
    {
        let mut made = start.made.borrow_mut();
        for (pointer, value) in pointers(now) {
            if value.is_object() || value.is_array() {
                continue;
            }
            let name = named(&pointer);
            let started = pointers(&start.value)
                .into_iter()
                .any(|(other, held)| held == value && named(&other) == name);
            if !started && start.value.pointer(&pointer).is_none() {
                made.insert(name);
            }
        }
    }
    for (pointer, value) in pointers(now) {
        let Some(form) = after.get(&pointer) else {
            continue;
        };
        let parent = &pointer[..pointer.rfind('/').unwrap_or(0)];
        let new_parent = !parent.is_empty() && start.value.pointer(parent).is_none();
        // A scalar left alone across views that read it otherwise (an XML
        // view reads every scalar as text, a switch writes the next view
        // from that reading) keeps the host's value and text: `'8080'`
        // stays a string, not the number with the same text.
        if let Some(held) = start.value.pointer(&pointer)
            && held != value
            && scalar_text(held).is_some()
            && scalar_text(held) == scalar_text(value)
            && !changed_before.contains(&pointer)
            && !users_own(&pointer)
            && !retyped(form)
            && let Some(was) = before.get(&pointer)
        {
            return Err(Failure::new(
                Broken::Form,
                was.clone(),
                format!("{pointer} left alone, its type changed: `{was}` -> `{form}`"),
            )
            .written(form, &pointer, false, new_parent));
        }
        if start.value.pointer(&pointer) == Some(value) {
            if changed_before.contains(&pointer) {
                continue;
            }
            let Some(was) = before.get(&pointer) else {
                continue;
            };
            if was != form && changed_anchor(was) {
                // An alias written out once its anchor moved stays written
                // out: the save that did so held no anchor before it, and
                // later saves keep the form it wrote.
                start.changed.borrow_mut().insert(pointer.clone());
                continue;
            }
            if was != form && !retyped(form) && !users_own(&pointer) {
                return Err(Failure::new(
                    Broken::Form,
                    was.clone(),
                    format!("{pointer} left alone: `{was}` -> `{form}`"),
                )
                .written(form, &pointer, false, new_parent));
            }
            continue;
        }
        // Moved or copied: from where the same key held the same value.
        let name = named(&pointer);
        if made_before.contains(&name) {
            continue;
        }
        let candidates: Vec<String> = pointers(&start.value)
            .into_iter()
            .filter(|(other, held)| *held == value && named(other) == name)
            .map(|(other, _)| other)
            .collect();
        // A source with no text of its own (a key a YAML merge brings in)
        // has no form to keep: P3 writes it in the host's default form,
        // which the read back checks.
        let textless = candidates
            .iter()
            .any(|other| !before.contains_key(other) && !is_empty_container(value));
        let sources: Vec<&String> = candidates
            .iter()
            .filter_map(|other| before.get(other))
            .collect();
        // A moved map whose keys the user put in another order is theirs.
        let reordered_moved = candidates.iter().any(|from| reordered(&pointer, from));
        if !textless
            && !users_own(&pointer)
            && !retyped(form)
            && !reordered_moved
            && !sources.is_empty()
            && !sources.contains(&form)
            && !sources.iter().any(|was| changed_anchor(was))
        {
            return Err(Failure::new(
                Broken::Form,
                sources[0].clone(),
                format!("{pointer} moved or copied: `{}` -> `{form}`", sources[0]),
            )
            .written(form, &pointer, true, new_parent));
        }
    }
    Ok(())
}

/// Promise 4: the comments of what was left alone stay, and those the
/// view still shows.
fn check_comments(
    start: &Start,
    view: Language,
    view_text: &str,
    saved: &str,
    now: &Value,
) -> Result<(), Box<Failure>> {
    let kept = comment_bodies(start.host, saved);
    let shown = comment_bodies(view, view_text);
    // An item cut and a key of its name written again later (the line
    // pasted elsewhere) is the user's edit of the comment's line: the
    // save that dropped the item may drop its comment.
    {
        let mut edited = start.edited.borrow_mut();
        for pointer in start.comments.iter().flat_map(|(_, owner)| owner) {
            if !left_alone(&start.value, now, pointer) {
                edited.insert(pointer.clone());
            }
        }
    }
    let edited = start.edited.borrow();
    for (words, owner) in &start.comments {
        if kept.contains(words) {
            continue;
        }
        let left_alone = !owner.is_empty()
            && owner
                .iter()
                .all(|pointer| !edited.contains(pointer) && left_alone(&start.value, now, pointer));
        let first_shown = start.shown_comments.contains(words);
        let still_shown = shown.contains(words);
        // Deleted from a view that showed it: the user's own edit.
        let deleted = first_shown && view.label() != "JSON" && !still_shown;
        let wanted = still_shown || (left_alone && !deleted);
        if wanted {
            return Err(Failure::new(
                Broken::Comment,
                words.clone(),
                format!("comment `{words}` (of {owner:?}) lost"),
            ));
        }
    }
    Ok(())
}

/// Whether the map or list `was` (as the session started) is held as
/// `now` with other keys or another length, as another kind, or not at
/// all; `false` for a scalar, and for a container whose members only
/// changed their values.
fn reshaped(was: &Value, now: Option<&Value>) -> bool {
    match (was, now) {
        (Value::Object(was), Some(Value::Object(now))) => {
            was.len() != now.len() || was.keys().any(|key| !now.contains_key(key))
        }
        (Value::Array(was), Some(Value::Array(now))) => was.len() != now.len(),
        (Value::Object(_) | Value::Array(_), _) => true,
        _ => false,
    }
}

/// A container counts as rebuilt only when its keys, its length or its
/// kind changed: a member's new value leaves its layout judged.
#[test]
fn only_a_container_held_with_other_entries_counts_as_rebuilt() {
    use serde_json::json;
    let map = json!({"node": ">=18"});
    assert!(reshaped(&map, Some(&Value::Null)));
    assert!(reshaped(&map, None));
    assert!(reshaped(&map, Some(&json!({}))));
    assert!(reshaped(&map, Some(&json!({"other": ">=18"}))));
    assert!(!reshaped(&map, Some(&json!({"node": ">=20"}))));
    assert!(!reshaped(&json!([1, 2]), Some(&json!([3, 4]))));
    assert!(reshaped(&json!([1, 2]), Some(&json!([1]))));
    assert!(!reshaped(&json!(1), Some(&json!(2))));
}

/// Whether `value` is a map or list holding nothing (which `forms` leaves
/// out).
fn is_empty_container(value: &Value) -> bool {
    value.as_object().is_some_and(|map| map.is_empty())
        || value.as_array().is_some_and(|items| items.is_empty())
}

/// The text of each scalar value `text` (in `language`) writes, trimmed.
fn value_texts(language: Language, text: &str) -> std::collections::HashSet<String> {
    let Ok(value) = language.parse(text) else {
        return Default::default();
    };
    let wanted: Vec<String> = pointers(&value)
        .into_iter()
        .filter(|(_, value)| !value.is_object() && !value.is_array())
        .map(|(pointer, _)| pointer)
        .collect();
    let wanted_paths: Vec<FieldPath> = wanted.iter().map(FieldPath::from).collect();
    language
        .behavior()
        .value_spans(text, &wanted_paths)
        .into_iter()
        .flatten()
        .filter_map(|span| {
            let line = line_text(text, span.line)?;
            Some(
                characters(line, span.start_column - 1, span.end_column - 1)
                    .trim()
                    .to_string(),
            )
        })
        .collect()
}
