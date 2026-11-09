//! A quoted string emptied, saved (the typing pause ended), then typed
//! again keeps the quotes it had before the edit session: the host's `""`
//! alone does not say whether quotes were the text's style or only how an
//! empty string is written.

use super::super::codec;
use super::super::language::ConfigLanguage;
use super::changed_scalars;
use serde_json::Value;

/// `previous` with each string the edit fills again (empty in `old`, text
/// in `value`) written as `origin` (the host's text when the session began)
/// wrote it, when it was quoted there and `previous` writes only `""` or
/// `''`: the patch then writes the new text in those quotes. `None` when
/// nothing applies.
pub(super) fn restored(
    host: &dyn ConfigLanguage,
    previous: &str,
    origin: &str,
    (old, value): (&Value, &Value),
    shape: Option<&Value>,
) -> Option<String> {
    if origin == previous {
        return None;
    }
    let mut changed = Vec::new();
    changed_scalars(old, value, String::new(), &mut changed);
    changed.retain(|pointer| {
        old.pointer(pointer).and_then(Value::as_str) == Some("")
            && value
                .pointer(pointer)
                .and_then(Value::as_str)
                .is_some_and(|text| !text.is_empty())
    });
    if changed.is_empty() {
        return None;
    }
    let before = codec::read(host, origin, shape).ok()?;
    let written_in = |text: &str, pointer: &str| {
        let span = host.value_span(text, pointer)?;
        codec::byte_range(text, span)
    };
    let mut edits = Vec::new();
    for pointer in changed {
        if !before.pointer(&pointer).is_some_and(Value::is_string) {
            continue;
        }
        let (Some(was), Some(now)) = (written_in(origin, &pointer), written_in(previous, &pointer))
        else {
            continue;
        };
        let was_text = &origin[was];
        let flow = super::super::flow_syntax::Flow::of(host);
        let quoted = was_text
            .chars()
            .next()
            .filter(|quote| flow.quote(*quote).is_some())
            .is_some_and(|quote| {
                was_text.len() >= 2 * quote.len_utf8()
                    && was_text.ends_with(quote)
                    && !was_text.contains('\n')
            });
        let emptied = previous[now.clone()].trim();
        let empty_quotes = emptied.chars().count() == 2
            && emptied
                .chars()
                .next()
                .is_some_and(|quote| flow.quote(quote).is_some() && emptied.ends_with(quote));
        if quoted && empty_quotes {
            edits.push((now, was_text.to_string()));
        }
    }
    if edits.is_empty() {
        return None;
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut restyled = previous.to_string();
    for (range, text) in edits {
        restyled.replace_range(range, &text);
    }
    Some(restyled)
}
