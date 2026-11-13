//! An element's attributes put in another order by a view (`b` before `a`):
//! written in its start tag in that order, each as the host wrote it, so the
//! save reads back in the order the view holds.

use super::*;

/// `source` with the attributes of the element at `pointer` in `order`'s
/// order among themselves (keys as the model reads them: `@a` beside a
/// child `a`); `None` when it has fewer than two, or they are in it.
pub(super) fn in_order(source: &str, pointer: &str, order: &[String]) -> Option<String> {
    let wanted = pointer_segments(pointer);
    let tokens = tokenize(source);
    let plan = plan_paths(source, &tokens);
    let tag = tokens.iter().find(|token| {
        matches!(token.kind, Kind::OpenTag { closing: false, .. })
            && plan.root != Some(token.start)
            && plan.path_of(token.start).is_some_and(|path| strings(path) == wanted)
    })?;
    // Each attribute's key and bytes, from its name to its value's end.
    let inner = tokenize_tag_inside(source, tag);
    let mut attributes: Vec<(String, std::ops::Range<usize>)> = Vec::new();
    for window in inner.windows(3) {
        if let [name, Token { kind: Kind::Equals, .. }, value] = window
            && matches!(name.kind, Kind::Text { quoted: false })
            && matches!(value.kind, Kind::Text { quoted: true })
            // Markup stays where it is written (`declarations`).
            && !plan.markup.holds(&source[name.start..name.end])
        {
            let key = plan.attribute_key(tag.start, &source[name.start..name.end]);
            attributes.push((key, name.start..value.end));
        }
    }
    if attributes.len() < 2 {
        return None;
    }
    let place = |key: &String| order.iter().position(|other| other == key).unwrap_or(usize::MAX);
    let mut sorted = attributes.clone();
    sorted.sort_by_key(|(key, _)| place(key));
    if sorted.iter().map(|(key, _)| key).eq(attributes.iter().map(|(key, _)| key)) {
        return None;
    }
    let mut out = String::with_capacity(source.len());
    let mut copied = 0;
    for ((_, slot), (_, from)) in attributes.iter().zip(&sorted) {
        out.push_str(&source[copied..slot.start]);
        out.push_str(&source[from.clone()]);
        copied = slot.end;
    }
    out.push_str(&source[copied..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::in_order;

    #[test]
    fn attributes_take_the_order_each_as_written() {
        let order = |keys: &[&str]| keys.iter().map(|key| key.to_string()).collect::<Vec<_>>();
        let host = "<r xmlns:n=\"urn:x\" n:a='1'  b=\"2\"><s>v</s></r>\n";
        assert_eq!(
            in_order(host, "/r", &order(&["xmlns:n", "b", "n:a", "s"])).as_deref(),
            Some("<r xmlns:n=\"urn:x\" b=\"2\"  n:a='1'><s>v</s></r>\n")
        );
        assert_eq!(in_order(host, "/r", &order(&["xmlns:n", "n:a", "b", "s"])), None);
        // Beside a child of its name, an attribute is `@a`.
        let host = "<r>\n  <c a=\"1\" b=\"2\"><a>3</a></c>\n</r>\n";
        assert_eq!(
            in_order(host, "/r/c", &order(&["b", "@a", "a"])).as_deref(),
            Some("<r>\n  <c b=\"2\" a=\"1\"><a>3</a></c>\n</r>\n")
        );
    }
}
