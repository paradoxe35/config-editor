//! Numbers and bare values saved as the edit spelled them (`2.50`, `2e3`,
//! `+6`, a TOML datetime's `.50`), where the host reads that spelling as the
//! same value, instead of in the host writer's own form (`2.5`, `2000.0`,
//! `6`, `.5`); a number typed in the view's own form is saved in the form
//! of the one it replaces (`0x1F90` edited to 8083 is `0x1F93`).

use super::super::codec;
use super::super::language::ConfigLanguage;
use super::changed_scalars;
use serde_json::Value;

/// `saved` (the host text `previous` patched to read as `target`) with
/// each number, and each string the host writes bare (a TOML datetime),
/// that the edit changed from `old` written as `edit_text` spells it (a
/// string's text without its quotes), when the host reads that text as
/// the same value, else each number in the form `previous` wrote it in
/// ([`ConfigLanguage::number_in_form`]); `saved` itself when nothing
/// applies or the result would read back otherwise.
pub(super) fn kept(
    (host, edit): (&dyn ConfigLanguage, &dyn ConfigLanguage),
    (previous, edit_text): (&str, &str),
    (old, target): (&Value, &Value),
    saved: String,
    shape: Option<&Value>,
) -> String {
    let mut changed = Vec::new();
    changed_scalars(old, target, String::new(), &mut changed);
    changed.retain(|pointer| {
        target
            .pointer(pointer)
            .is_some_and(|value| value.is_number() || value.is_string())
    });
    if changed.is_empty() {
        return saved;
    }
    let typed = edit.value_spans(edit_text, &changed);
    let written = host.value_spans(&saved, &changed);
    let was = host.value_spans(previous, &changed);
    let mut edits = Vec::new();
    for (((pointer, typed), written), was) in changed.iter().zip(typed).zip(written).zip(was) {
        let Some(range) = written.and_then(|written| codec::byte_range(&saved, written)) else {
            continue;
        };
        let Some(value) = target.pointer(pointer) else {
            continue;
        };
        let range = trimmed(&saved, range);
        let written = &saved[range.clone()];
        // An anchor or tag before it (`&x 2`, `!!float 2`), or an alias
        // (`*x`), stays the host's.
        if written.is_empty()
            || host.value_properties_len(written) > 0
            || host.reference_len(written).is_some()
        {
            continue;
        }
        let typed = typed
            .and_then(|typed| codec::byte_range(edit_text, typed))
            .and_then(|range| spelled(host, edit, edit_text[range].trim(), written, value));
        let in_form = || {
            let number = value.as_number()?;
            old.pointer(pointer)?.as_number()?;
            let was = codec::byte_range(previous, was?)?;
            host.number_in_form(previous[was].trim(), number)
                .filter(|text| text != written)
        };
        if let Some(text) = typed.or_else(in_form) {
            edits.push((range, text));
        }
    }
    if edits.is_empty() {
        return saved;
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut spelled = saved.clone();
    for (range, text) in edits {
        spelled.replace_range(range, &text);
    }
    let reads = |text: &str| codec::read(host, text, shape).ok();
    if reads(&spelled).is_some() && reads(&spelled) == reads(&saved) {
        spelled
    } else {
        saved
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
        if super::super::flow_syntax::Flow::of(host).starts_quoted(written)
            || written.contains('\n')
        {
            return None;
        }
        unquoted(edit, typed)?
    } else {
        typed
    };
    (!typed.is_empty() && written != typed && host.scalar_reads_as(typed, value))
        .then(|| typed.to_string())
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
