//! Numbers and bare values saved as the edit spelled them (`2.50`, `2e3`,
//! `+6`, a TOML datetime's `.50`), where the host reads that spelling as the
//! same value, instead of in the host writer's own form (`2.5`, `2000.0`,
//! `6`, `.5`).

use super::super::codec;
use super::super::language::ConfigLanguage;
use super::changed_scalars;
use serde_json::Value;

/// `saved` (the host text patched to read as `target`) with each number,
/// and each string the host writes bare (a TOML datetime), that the edit
/// changed from `old` written as `edit_text` spells it (a string's text
/// without its quotes), when the host reads that text as the same value;
/// `saved` itself when nothing applies or the result would read back
/// otherwise.
pub(super) fn kept(
    (host, edit): (&dyn ConfigLanguage, &dyn ConfigLanguage),
    edit_text: &str,
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
    let mut edits = Vec::new();
    for ((pointer, typed), written) in changed.iter().zip(typed).zip(written) {
        let (Some(typed), Some(written)) = (typed, written) else {
            continue;
        };
        let (Some(typed), Some(range)) = (
            codec::byte_range(edit_text, typed).map(|range| edit_text[range].trim()),
            codec::byte_range(&saved, written),
        ) else {
            continue;
        };
        let Some(value) = target.pointer(pointer) else {
            continue;
        };
        let written = saved[range.clone()].trim();
        // An anchor or tag before it (`&x 2`, `!!float 2`), or an alias
        // (`*x`), stays the host's.
        if host.value_properties_len(written) > 0 || host.reference_len(written).is_some() {
            continue;
        }
        let typed = if value.is_string() {
            // Only a string the host writes bare, typed without escapes.
            if written.starts_with(['"', '\'']) || written.contains('\n') {
                continue;
            }
            match unquoted(typed) {
                Some(text) => text,
                None => continue,
            }
        } else {
            typed
        };
        if typed.is_empty() || written == typed {
            continue;
        }
        if host.scalar_reads_as(typed, value) {
            edits.push((range, typed.to_string()));
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

/// `typed` without the quotes around it, when it holds no escape, quote or
/// line break.
fn unquoted(typed: &str) -> Option<&str> {
    let inner = ['"', '\'']
        .into_iter()
        .find_map(|quote| typed.strip_prefix(quote)?.strip_suffix(quote))
        .unwrap_or(typed);
    (!inner.contains(['\\', '"', '\'', '\n'])).then_some(inner)
}
