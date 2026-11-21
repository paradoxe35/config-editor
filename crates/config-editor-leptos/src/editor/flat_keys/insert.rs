//! What a key suggestion writes in a language known by its declared syntax:
//! the entry as the language's own `serialize` writes it, so its values
//! (maps, lists, strings its bare words would read otherwise) are in its
//! syntax. The values left to type are written as placeholders and taken
//! out of the text after, the caret at the first.

use serde_json::{Map, Value};

use super::super::completion::{Fill, InsertPlan, Planned, RequiredChild};
use super::super::flow_syntax::Flow;
use super::super::language::{ConfigLanguage, EntryPosition, InsertContext, LanguageConfig};

/// A value left out of the written entry, where a placeholder stands.
#[derive(Clone, Debug)]
enum Hole {
    /// Left to type, the caret may go there; a string keeps its quotes.
    Caret { string: bool },
    /// This value, the caret before it (inside it when it is an empty list).
    CaretAt(Value),
    /// Left empty.
    Empty,
}

/// The placeholder standing for hole `index`: a word no case or schema
/// writes, which a serializer writes as it is (quoted or not).
fn placeholder(index: usize) -> String {
    format!("zqholezq{index}zq")
}

/// The key the entry of a one-line map is written under, to be taken out
/// of the map's brackets.
const HOLDER: &str = "w";

/// [`ConfigLanguage::write_insert`] for a language known by its declared
/// syntax: the entry for `key` and the value `plan` decides, as the
/// language's `serialize` writes it; inside a one-line map (an
/// [`Inline`](EntryPosition::Inline) entry) as it writes an entry of a map.
/// Values left to type are left empty, a string's quotes kept, the caret
/// at the first (else inside an empty map or list, else after the value).
/// `None` (the key not offered) when the entry it writes does not read back
/// with the key; the key and separator alone
/// ([`flat_property_insert`](LanguageConfig::flat_property_insert)) when it
/// refuses to write the entry.
pub fn flat_write_insert(
    language: &dyn ConfigLanguage,
    key: &str,
    plan: &InsertPlan<'_>,
    context: &InsertContext<'_>,
) -> Option<(String, usize)> {
    let config = language.config();
    let mut holes = Vec::new();
    let value = planned_value(plan, &mut holes);
    let flow = Flow::from_config(config);
    let inline = context.position == EntryPosition::Inline;
    let Some(written) = written_entry(language, &flow, key, &value, inline) else {
        return config.flat_property_insert(key);
    };
    if !written.reads_back {
        return None;
    }
    let Some((text, caret)) = filled(language, &flow, &written.text, &holes) else {
        return config.flat_property_insert(key);
    };
    let caret = caret.unwrap_or_else(|| inside_empty_container(&flow, &text));
    let caret = text[..caret].encode_utf16().count();
    Some((text, caret))
}

/// [`ConfigLanguage::format_value`] for a language known by its declared
/// syntax: a map or list as the language's `serialize` writes it after a
/// key inside a map, on one line (else as JSON); a scalar as
/// [`flat_format_value`](LanguageConfig::flat_format_value) writes it.
pub fn flat_value_text(language: &dyn ConfigLanguage, value: &Value) -> String {
    let config = language.config();
    if !(value.is_object() || value.is_array()) {
        return config.flat_format_value(value);
    }
    written_value(language, &Flow::from_config(config), value)
        .unwrap_or_else(|| config.flat_format_value(value))
}

impl LanguageConfig {
    /// [`Typing::empty_value_slots`](super::super::language::Typing::empty_value_slots)
    /// for a language known by its declared syntax: where a key's
    /// separator (and the spacing after it) ends a line, or is followed by
    /// an item separator or a map's closer (`<port: ; mode: >`).
    ///
    /// ```
    /// use leptos_config_editor::language::LanguageConfig;
    /// let dot = LanguageConfig {
    ///     separators: &[":"],
    ///     map_brackets: Some(('<', '>')),
    ///     item_separator: Some(';'),
    ///     ..LanguageConfig::default()
    /// };
    /// assert_eq!(dot.flat_empty_value_slots("a: <b: ; c: >\nd: "), vec![7, 12, 17]);
    /// ```
    pub fn flat_empty_value_slots(&self, text: &str) -> Vec<usize> {
        let flow = Flow::from_config(*self);
        let ends_value = |next: char| {
            matches!(next, '\n' | '\r')
                || flow.is_separator(next)
                || flow.map().is_some_and(|(_, close)| close == next)
        };
        let mut slots = Vec::new();
        for separator in self.separators {
            for (at, _) in text.match_indices(separator) {
                let rest = &text[at + separator.len()..];
                let slot = text.len() - rest.trim_start_matches([' ', '\t']).len();
                if text[slot..].chars().next().is_none_or(ends_value) && !slots.contains(&slot) {
                    slots.push(slot);
                }
            }
        }
        slots.sort_unstable();
        slots
    }
}

/// An entry as the language writes it.
struct Written {
    text: String,
    /// It reads back with its key where it was written.
    reads_back: bool,
}

/// The entry `key: value` as the language writes it: a whole document's
/// only line(s), or with `inline`, as an entry of a one-line map (the
/// map's inside, without its brackets). `None` when it refuses to.
fn written_entry(
    language: &dyn ConfigLanguage,
    flow: &Flow,
    key: &str,
    value: &Value,
    inline: bool,
) -> Option<Written> {
    let entry = Value::Object(Map::from_iter([(key.to_string(), value.clone())]));
    if !inline {
        let text = language.serialize(&entry, None).ok()?;
        let reads_back = language
            .parse(&text)
            .is_ok_and(|read| read.get(key).is_some());
        let text = text.trim_end_matches(['\n', '\r']).to_string();
        return Some(Written { text, reads_back });
    }
    let (open, _) = flow.map()?;
    let document = Value::Object(Map::from_iter([(HOLDER.to_string(), entry)]));
    let text = language.serialize(&document, None).ok()?;
    let start = text.find(open)?;
    let inside = start + open.len_utf8();
    let close = start + flow.matching_close(&text[start..])?;
    // A map holding a value written over lines is not one-line.
    let entry = text[inside..close].trim();
    if entry.contains('\n') {
        return None;
    }
    let reads_back = language.parse(&text).is_ok_and(|read| {
        read.get(HOLDER)
            .and_then(|holder| holder.get(key))
            .is_some()
    });
    Some(Written {
        text: entry.to_string(),
        reads_back,
    })
}

/// `value` as the language writes it after a key inside a map, on one
/// line.
fn written_value(language: &dyn ConfigLanguage, flow: &Flow, value: &Value) -> Option<String> {
    let inline = flow.map().is_some();
    let entry = written_entry(language, flow, HOLDER, value, inline)?;
    let (_, _, written) = flow.entry_parts(&entry.text)?;
    (!written.contains('\n')).then(|| written.trim_end().to_string())
}

/// Where the caret goes in `text`, an entry with no value left to type:
/// inside the empty map or list it ends with, else at its end.
fn inside_empty_container(flow: &Flow, text: &str) -> usize {
    let mut characters = text.chars().rev();
    match (characters.next(), characters.next()) {
        (Some(close), Some(open)) if flow.opener_of(close) == Some(open) => {
            text.len() - close.len_utf8()
        }
        _ => text.len(),
    }
}

/// `text` with each hole's placeholder (and the quotes around it)
/// replaced by what the hole writes, and the byte the caret goes to: the
/// first hole that takes it. `None` when a placeholder is not in the text.
fn filled(
    language: &dyn ConfigLanguage,
    flow: &Flow,
    text: &str,
    holes: &[Hole],
) -> Option<(String, Option<usize>)> {
    let mut found = Vec::with_capacity(holes.len());
    for (index, hole) in holes.iter().enumerate() {
        let name = placeholder(index);
        let at = text.find(&name)?;
        let (mut start, mut end) = (at, at + name.len());
        let quote = text[..start]
            .chars()
            .next_back()
            .filter(|open| flow.quote(*open).is_some() && text[end..].starts_with(*open));
        if let Some(quote) = quote {
            start -= quote.len_utf8();
            end += quote.len_utf8();
        }
        found.push((start, end, quote, hole));
    }
    found.sort_by_key(|(start, ..)| *start);
    let mut out = String::with_capacity(text.len());
    let mut caret = None;
    let mut copied = 0;
    for (start, end, quote, hole) in found {
        out.push_str(&text[copied..start]);
        copied = end;
        match hole {
            Hole::Caret { string } => {
                let quote = quote.filter(|_| *string);
                out.extend(quote);
                caret.get_or_insert(out.len());
                out.extend(quote);
            }
            Hole::CaretAt(value) => {
                let written = written_value(language, flow, value)
                    .unwrap_or_else(|| language.config().flat_format_value(value));
                let empty_list = flow
                    .list()
                    .is_some_and(|(open, close)| written == format!("{open}{close}"));
                caret.get_or_insert(out.len() + usize::from(empty_list));
                out.push_str(&written);
            }
            Hole::Empty => {}
        }
    }
    out.push_str(&text[copied..]);
    Some((out, caret))
}

/// The value `plan` writes for its key, with a placeholder for each value
/// left out (in `holes`, by index).
fn planned_value(plan: &InsertPlan<'_>, holes: &mut Vec<Hole>) -> Value {
    match &plan.value {
        Planned::Value(value) => (*value).clone(),
        Planned::List { .. } if plan.list_of_objects && !plan.children.is_empty() => {
            Value::Array(vec![children_value(&plan.children, holes)])
        }
        Planned::List { .. } => Value::Array(Vec::new()),
        Planned::Object { .. } => children_value(&plan.children, holes),
        Planned::Empty { string } => hole(holes, Hole::Caret { string: *string }),
    }
}

/// The map of `children`, each as its fill writes it.
fn children_value(children: &[RequiredChild<'_>], holes: &mut Vec<Hole>) -> Value {
    let mut map = Map::new();
    for child in children {
        let value = match &child.fill {
            Fill::Value(value) => value.clone(),
            Fill::Children(nested) => children_value(nested, holes),
            Fill::Caret { string } => hole(holes, Hole::Caret { string: *string }),
            Fill::CaretAt(value) => hole(holes, Hole::CaretAt(value.clone())),
            Fill::Empty(_) => hole(holes, Hole::Empty),
        };
        map.insert(child.key.to_string(), value);
    }
    Value::Object(map)
}

/// A placeholder for `kind`, recorded in `holes`.
fn hole(holes: &mut Vec<Hole>, kind: Hole) -> Value {
    holes.push(kind);
    Value::String(placeholder(holes.len() - 1))
}
