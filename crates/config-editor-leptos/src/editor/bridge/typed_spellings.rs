//! Numbers and bare values saved as the edit spelled them (`2.50`, `2e3`,
//! `+6`, a TOML datetime's `.50`), where the host reads that spelling as the
//! same value, instead of in the host writer's own form (`2.5`, `2000.0`,
//! `6`, `.5`); a number typed in the view's own form is saved in the form
//! of the one it replaces (`0x1F90` edited to 8083 is `0x1F93`).

use super::super::codec;
use super::super::language::{ConfigLanguage, Scalars};
use super::changed_scalars;
use serde_json::Value;

/// `saved` (the host text `previous` patched to read as `target`) with
/// each number, and each string the host writes bare (a TOML datetime),
/// that the edit changed from `old` written as `edit_text` spells it (a
/// string's text without its quotes), when the host reads that text as
/// the same value, else each number in the form `previous` wrote it in
/// ([`ConfigLanguage::number_in_form`]); `saved` itself when nothing
/// applies or the result would read back otherwise.
///
/// `respelled` adds the numbers the edit kept but typed in another
/// spelling (`4.80` over `4.8`, see [`respelled`]).
///
/// A host whose values are all text reads a number kept as typed as that
/// text (`4.10`): [`Spelled::as_text`] says which, so the save's read-back
/// check reads them as the numbers they are.
pub(super) fn kept(
    (host, edit): (&dyn ConfigLanguage, &dyn ConfigLanguage),
    (previous, edit_text): (&str, &str),
    (old, target): (&Value, &Value),
    (saved, respelled): (String, Vec<String>),
    shape: Option<&Value>,
) -> Spelled {
    let mut changed = Vec::new();
    changed_scalars(old, target, String::new(), &mut changed);
    changed.retain(|pointer| {
        target
            .pointer(pointer)
            .is_some_and(|value| value.is_number() || value.is_string())
    });
    // (Disjoint: these hold the value they held.)
    changed.extend(respelled);
    if changed.is_empty() {
        return Spelled::as_saved(saved);
    }
    // Where the save writes each value that may be spelled anew.
    let mut places = Vec::new();
    for (pointer, written) in changed.iter().zip(ranges(host, &saved, &changed)) {
        let (Some(range), Some(value)) = (written, target.pointer(pointer)) else {
            continue;
        };
        let range = trimmed(&saved, range);
        // A number the host writes in quotes (an XML attribute's value)
        // is spelled inside them.
        let range = match value.is_number() {
            true => inside_quotes(host, &saved, range),
            false => range,
        };
        let written = &saved[range.clone()];
        // An anchor or tag before it (`&x 2`, `!!float 2`), or an alias
        // (`*x`), stays the host's; so does a string it writes quoted or
        // over lines (see `spelled`).
        if written.is_empty()
            || host.value_properties_len(written) > 0
            || host.reference_len(written).is_some()
            || value.is_string() && !bare_string(host, written)
        {
            continue;
        }
        places.push((pointer, value, range));
    }
    if places.is_empty() {
        return Spelled::as_saved(saved);
    }
    // Only those are looked up in the edit's text, and the numbers among
    // them in the host's last (their form there).
    let pointers: Vec<String> = places
        .iter()
        .map(|(pointer, ..)| (*pointer).clone())
        .collect();
    let typed = ranges(edit, edit_text, &pointers);
    let numbers: Vec<String> = places
        .iter()
        .filter(|(pointer, value, _)| {
            value.is_number() && old.pointer(pointer).is_some_and(Value::is_number)
        })
        .map(|(pointer, ..)| (*pointer).clone())
        .collect();
    let was: std::collections::HashMap<String, std::ops::Range<usize>> = numbers
        .iter()
        .cloned()
        .zip(ranges(host, previous, &numbers))
        .filter_map(|(pointer, range)| Some((pointer, range?)))
        .collect();
    let mut edits = Vec::new();
    // The values a host whose values are all text holds as typed, where
    // the edit read a number (`4.10` is the text `4.10` there).
    let mut as_text = Vec::new();
    for ((pointer, value, range), typed) in places.into_iter().zip(typed) {
        let written = &saved[range.clone()];
        let typed =
            typed.and_then(|range| spelled(host, edit, edit_text[range].trim(), written, value));
        let in_form = || {
            let number = value.as_number()?;
            host.number_in_form(previous[was.get(pointer)?.clone()].trim(), number)
                .filter(|text| text != written)
        };
        if let Some(text) = typed.or_else(in_form) {
            if host.scalars() == Scalars::Text && value.is_number() {
                as_text.push((pointer.clone(), text.clone()));
            }
            edits.push((range, text));
        }
    }
    if edits.is_empty() {
        return Spelled::as_saved(saved);
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut spelled = saved.clone();
    for (range, text) in edits {
        spelled.replace_range(range, &text);
    }
    let reads = |text: &str| codec::read(host, text, shape).ok();
    let (Some(mut read), Some(before)) = (reads(&spelled), reads(&saved)) else {
        return Spelled::as_saved(saved);
    };
    // Each number kept as typed text reads as that text: else the same.
    for (pointer, text) in &as_text {
        if let (Some(slot), Some(was)) = (read.pointer_mut(pointer), before.pointer(pointer))
            && slot.as_str() == Some(text.as_str())
        {
            *slot = was.clone();
        }
    }
    if read == before {
        Spelled {
            text: spelled,
            as_text,
        }
    } else {
        Spelled::as_saved(saved)
    }
}

/// A save's text with numbers spelled as typed, and those a host whose
/// values are all text holds as the typed text (pointer, text).
pub(super) struct Spelled {
    pub(super) text: String,
    pub(super) as_text: Vec<(String, String)>,
}

impl Spelled {
    fn as_saved(text: String) -> Self {
        Spelled {
            text,
            as_text: Vec::new(),
        }
    }
}

/// The numbers of `edited` (the edit's value of `edit_text`) the edit left
/// as `seen` (its value of `shown`, the view of the host before the edit)
/// held them but spelled otherwise there: `4.80` typed over `4.8`. Only
/// numbers in the part of the text the edit changed are looked at.
pub(super) fn respelled(
    edit: &dyn ConfigLanguage,
    (shown, edit_text): (&str, &str),
    (seen, edited): (&Value, &Value),
) -> Vec<String> {
    let prefix = shown
        .bytes()
        .zip(edit_text.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let room = shown.len().min(edit_text.len()) - prefix;
    let suffix = shown
        .bytes()
        .rev()
        .zip(edit_text.bytes().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(room);
    // Only an edit that typed or removed a digit can spell a number anew.
    let (start, end) = (prefix, edit_text.len() - suffix);
    // (Bytes: the ends may part a character the two texts begin alike.)
    let digit = |text: &[u8]| text.iter().any(u8::is_ascii_digit);
    if !digit(&edit_text.as_bytes()[start..end])
        && !digit(&shown.as_bytes()[start..shown.len() - suffix])
    {
        return Vec::new();
    }
    // An edit inside the one value it changed (a digit typed into a
    // string) spells no other number anew: no value is written inside
    // another, so none touches it. Only that value is looked up.
    let mut changed = Vec::new();
    changed_scalars(seen, edited, String::new(), &mut changed);
    if let [_] = changed.as_slice()
        && let Some(Some(range)) = ranges(edit, edit_text, &changed).pop()
        && let range = trimmed(edit_text, range)
        && range.start < start
        && end < range.end
    {
        return Vec::new();
    }
    let mut numbers = Vec::new();
    same_numbers(seen, edited, String::new(), &mut numbers);
    if numbers.is_empty() {
        return Vec::new();
    }
    // The numbers touching the changed bytes.
    let typed = ranges(edit, edit_text, &numbers);
    let touched: Vec<(String, std::ops::Range<usize>)> = numbers
        .into_iter()
        .zip(typed)
        .filter_map(|(pointer, range)| {
            let range = trimmed(edit_text, range?);
            (range.start <= end && start <= range.end).then_some((pointer, range))
        })
        .collect();
    if touched.is_empty() {
        return Vec::new();
    }
    let pointers: Vec<String> = touched.iter().map(|(pointer, _)| pointer.clone()).collect();
    let was = ranges(edit, shown, &pointers);
    touched
        .into_iter()
        .zip(was)
        .filter_map(|((pointer, range), was)| {
            let was = trimmed(shown, was?);
            (shown[was] != edit_text[range]).then_some(pointer)
        })
        .collect()
}

/// The byte ranges of the values of `pointers` in `text`, read in one
/// pass of it: the language's readings of the whole text, and its lines,
/// are made once for all of them (alone, each lookup read the text again,
/// and each span's line was counted from the top).
fn ranges(
    language: &dyn ConfigLanguage,
    text: &str,
    pointers: &[String],
) -> Vec<Option<std::ops::Range<usize>>> {
    super::super::paths::with_spans(language, text, &[], || {
        language
            .value_spans(text, pointers)
            .into_iter()
            .map(|span| codec::byte_range(text, span?))
            .collect()
    })
}

/// The pointers of the numbers `new` holds where `old` holds the same.
fn same_numbers(old: &Value, new: &Value, pointer: String, out: &mut Vec<String>) {
    match (old, new) {
        (Value::Object(left), Value::Object(right)) => {
            for (key, value) in right {
                if let Some(before) = left.get(key) {
                    let child = format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1"));
                    same_numbers(before, value, child, out);
                }
            }
        }
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => {
            for (index, (before, value)) in left.iter().zip(right).enumerate() {
                same_numbers(before, value, format!("{pointer}/{index}"), out);
            }
        }
        (Value::Number(_), Value::Number(_)) if old == new => out.push(pointer),
        // A view whose values are all text reads the number it showed as
        // text when typed otherwise (`4.80`).
        (Value::Number(number), Value::String(text))
            if text.trim().parse::<f64>().ok() == number.as_f64() =>
        {
            out.push(pointer)
        }
        _ => {}
    }
}

/// `range` of `text` inside the quotes of `language` around it, if any.
fn inside_quotes(
    language: &dyn ConfigLanguage,
    text: &str,
    range: std::ops::Range<usize>,
) -> std::ops::Range<usize> {
    let written = &text[range.clone()];
    let flow = super::super::flow_syntax::Flow::of(language);
    match written.chars().next() {
        Some(quote)
            if written.len() >= 2 * quote.len_utf8()
                && written.ends_with(quote)
                && flow.quote(quote).is_some() =>
        {
            range.start + quote.len_utf8()..range.end - quote.len_utf8()
        }
        _ => range,
    }
}

/// `range` of `text` without the whitespace at its ends.
fn trimmed(text: &str, range: std::ops::Range<usize>) -> std::ops::Range<usize> {
    let slice = &text[range.clone()];
    let start = range.start + (slice.len() - slice.trim_start().len());
    start..start + slice.trim().len()
}

/// `typed` (the edit's text of `value`) as the host saves it in place of
/// `written`, when it spells `value` otherwise and the host reads it so.
fn spelled(
    host: &dyn ConfigLanguage,
    edit: &dyn ConfigLanguage,
    typed: &str,
    written: &str,
    value: &Value,
) -> Option<String> {
    let typed = if value.is_string() {
        // Only a string the host writes bare, typed without escapes.
        if !bare_string(host, written) {
            return None;
        }
        unquoted(edit, typed)?
    } else {
        typed
    };
    // A host whose values are all text holds a number as typed.
    let reads =
        host.scalar_reads_as(typed, value) || host.scalars() == Scalars::Text && value.is_number();
    (!typed.is_empty() && written != typed && reads).then(|| typed.to_string())
}

/// Whether the host writes a string `written` so bare: unquoted, on one
/// line.
fn bare_string(host: &dyn ConfigLanguage, written: &str) -> bool {
    !super::super::flow_syntax::Flow::of(host).starts_quoted(written) && !written.contains('\n')
}

/// `typed` (as `edit` writes it) without the quotes around it, when it
/// holds no escape, quote or line break.
fn unquoted<'a>(edit: &dyn ConfigLanguage, typed: &'a str) -> Option<&'a str> {
    let flow = super::super::flow_syntax::Flow::of(edit);
    let inner = typed
        .chars()
        .next()
        .filter(|quote| flow.quote(*quote).is_some())
        .and_then(|quote| typed.strip_prefix(quote)?.strip_suffix(quote))
        .unwrap_or(typed);
    let plain = !inner
        .chars()
        .any(|character| matches!(character, '\\' | '\n') || flow.quote(character).is_some());
    plain.then_some(inner)
}
