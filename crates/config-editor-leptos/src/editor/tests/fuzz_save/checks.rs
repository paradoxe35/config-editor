//! The save promise, checked after each save of a session: the host's
//! text reads, holds what the view shows, writes every value the session
//! left alone (and every value it moved or copied) in its form, and keeps
//! the comments of what the session left alone; or else the save waits
//! with a problem shown.

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
    /// The comments, with the pointer each belongs to.
    comments: Vec<(String, Option<String>)>,
    /// The comments the first view shows.
    shown_comments: Vec<String>,
    /// The text the session's views showed, each switch's added.
    shown: String,
    /// The views' texts at each save since: what the user wrote.
    written: std::cell::RefCell<String>,
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

    /// The user wrote `view_text` (before a save or a switch).
    pub(super) fn note(&self, view_text: &str) {
        self.written.borrow_mut().push_str(view_text);
    }

    /// A switch shows the comments `view_text` writes.
    pub(super) fn seen(&mut self, view: Language, view_text: &str) {
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
                    let key = key.replace('~', "~0").replace('/', "~1");
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
    let behavior = language.behavior();
    let keys = behavior.key_spans(text, &wanted);
    let values = behavior.value_spans(text, &wanted);
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

/// Each comment of `text` with the pointer it belongs to: the deepest one
/// written on its line, else on the next line writing one.
fn owned_comments(language: Language, text: &str, value: &Value) -> Vec<(String, Option<String>)> {
    let all = pointers(value);
    let wanted: Vec<String> = all.iter().map(|(pointer, _)| pointer.clone()).collect();
    let behavior = language.behavior();
    let keys = behavior.key_spans(text, &wanted);
    let values = behavior.value_spans(text, &wanted);
    let lines: Vec<(usize, &String)> = wanted
        .iter()
        .zip(keys.iter().zip(values.iter()))
        .filter_map(|(pointer, (key, at))| Some((key.or(*at)?.line, pointer)))
        .collect();
    let deepest = |line: usize| {
        lines
            .iter()
            .filter(|(at, _)| *at == line)
            .max_by_key(|(_, pointer)| pointer.matches('/').count())
            .map(|(_, pointer)| (*pointer).clone())
    };
    behavior
        .comments(text)
        .into_iter()
        .filter_map(|range| {
            let words = body(text.get(range.clone())?);
            if words.is_empty() {
                return None;
            }
            let line = text[..range.start].matches('\n').count() + 1;
            let owner = deepest(line).or_else(|| {
                let next = lines
                    .iter()
                    .map(|(at, _)| *at)
                    .filter(|at| *at > line)
                    .min()?;
                deepest(next)
            });
            Some((words, owner))
        })
        .collect()
}

/// What the host holds: the last text `on_change` told it.
pub(super) type HostCopy = std::sync::Arc<std::sync::Mutex<String>>;

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
        let shown = bridge
            .to_edit(&saved)
            .ok()
            .and_then(|conversion| view.parse(&conversion.text).ok());
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
        if shown.as_ref().map(read) != meant.as_ref().map(read) {
            return Err(Failure::new(
                Broken::Meaning,
                "",
                "the saved host reads otherwise than the view",
            ));
        }
    }
    check_forms(start, &saved, &now, &view_text)?;
    check_comments(start, view, &view_text, &saved, &now)
}

/// Promise 3: every value left alone keeps its written form; a moved or
/// copied one its source's.
fn check_forms(
    start: &Start,
    saved: &str,
    now: &Value,
    view_text: &str,
) -> Result<(), Box<Failure>> {
    let after = forms(start.host, saved, now);
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
            && (view_text.contains(spelled) || start.written.borrow().contains(spelled))
            && !start.shown.contains(spelled)
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
    for (pointer, value) in pointers(now) {
        let Some(form) = after.get(&pointer) else {
            continue;
        };
        let parent = &pointer[..pointer.rfind('/').unwrap_or(0)];
        let new_parent = !parent.is_empty() && start.value.pointer(parent).is_none();
        if start.value.pointer(&pointer) == Some(value) {
            let Some(was) = before.get(&pointer) else {
                continue;
            };
            if was != form && !changed_anchor(was) && !retyped(form) {
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
        let sources: Vec<&String> = pointers(&start.value)
            .into_iter()
            .filter(|(other, held)| *held == value && named(other) == name)
            .filter_map(|(other, _)| before.get(&other))
            .collect();
        if !sources.is_empty()
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
    for (words, owner) in &start.comments {
        if kept.contains(words) {
            continue;
        }
        let first_shown = start.shown_comments.contains(words);
        let still_shown = shown.contains(words);
        let left_alone = owner
            .as_ref()
            .is_some_and(|owner| start.value.pointer(owner) == now.pointer(owner));
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
