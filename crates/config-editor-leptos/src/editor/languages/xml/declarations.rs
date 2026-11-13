//! Namespace declarations (`xmlns`, `xmlns:m`) and the attributes of the
//! schema-instance namespace (`xsi:schemaLocation`, `xsi:type`) are markup,
//! not data: the reader leaves them out (a `pom.xml` reads as its
//! elements, which a schema need not allow for), other views do not show
//! them, and a save writes them back in each element's start tag exactly
//! as the host wrote them, the way it keeps comments.

use super::*;

/// The namespace of `xsi:schemaLocation`, `xsi:type` and the like.
const SCHEMA_INSTANCE: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// Which attribute names of one document are markup.
pub(super) struct Markup {
    /// The prefixes a declaration binds to the schema-instance namespace
    /// (`xsi` as a rule; a prefix bound to another namespace is data).
    instance_prefixes: Vec<String>,
}

impl Markup {
    pub(super) fn of(source: &str) -> Self {
        let mut instance_prefixes = Vec::new();
        if source.contains(SCHEMA_INSTANCE) {
            for (at, _) in source.match_indices("xmlns:") {
                let rest = &source[at + "xmlns:".len()..];
                let name_end = rest.find(|character: char| character == '=' || character.is_whitespace()).unwrap_or(rest.len());
                let value = rest[name_end..].trim_start().strip_prefix('=').map(str::trim_start);
                let bound = value.and_then(|value| {
                    let quote = value.chars().next().filter(|quote| matches!(quote, '"' | '\''))?;
                    let inner = &value[1..];
                    Some(&inner[..inner.find(quote)?])
                });
                if bound == Some(SCHEMA_INSTANCE) && !instance_prefixes.iter().any(|known| known == &rest[..name_end]) {
                    instance_prefixes.push(rest[..name_end].to_string());
                }
            }
        }
        Markup { instance_prefixes }
    }

    /// Whether the attribute written `name` is markup.
    pub(super) fn holds(&self, name: &str) -> bool {
        name == "xmlns"
            || name.starts_with("xmlns:")
            || name.split_once(':').is_some_and(|(prefix, _)| self.instance_prefixes.iter().any(|known| known == prefix))
    }
}

/// Why `value` cannot be written as XML over `previous`: its first key
/// spelled as markup (`xmlns:y`, or `xsi:type` where `previous` binds
/// `xsi`), which XML keeps in its text, not as data, at its pointer.
pub(super) fn first_markup_key(value: &Value, previous: Option<&str>) -> Option<String> {
    let markup = Markup::of(previous.unwrap_or_default());
    find(value, &markup, &mut String::new())
}

fn find(value: &Value, markup: &Markup, pointer: &mut String) -> Option<String> {
    let children: Box<dyn Iterator<Item = (String, &Value)>> = match value {
        Value::Object(map) => Box::new(map.iter().map(|(key, child)| (key.clone(), child))),
        Value::Array(items) => Box::new(items.iter().enumerate().map(|(index, item)| (index.to_string(), item))),
        _ => return None,
    };
    for (key, child) in children {
        let length = pointer.len();
        pointer.push('/');
        pointer.push_str(&key.replace('~', "~0").replace('/', "~1"));
        if value.is_object() && markup.holds(&key) {
            return Some(format!(
                "{key} is XML markup (a namespace declaration or a schema-instance attribute), which XML keeps in its text, not as data: write it in the XML text (at {pointer})"
            ));
        }
        if let Some(found) = find(child, markup, pointer) {
            return Some(found);
        }
        pointer.truncate(length);
    }
    None
}

/// One attribute in a start tag: the blanks before it, and its bytes from
/// its name to its value's end.
struct Written {
    blanks: std::ops::Range<usize>,
    bytes: std::ops::Range<usize>,
    markup: bool,
}

/// The attributes of the start tag `tag`, in order, and where the text
/// after the last one (or after the name) begins.
fn attributes_of(source: &str, tag: &Token, markup: &Markup) -> (Vec<Written>, usize) {
    let mut after = local_name_start(source, tag) + name_len(source, tag);
    let mut found = Vec::new();
    let inner = tokenize_tag_inside(source, tag);
    for window in inner.windows(3) {
        if let [name, Token { kind: Kind::Equals, .. }, value] = window
            && matches!(name.kind, Kind::Text { quoted: false })
            && matches!(value.kind, Kind::Text { quoted: true })
            && name.start >= after
        {
            found.push(Written {
                blanks: after..name.start,
                bytes: name.start..value.end,
                markup: markup.holds(&source[name.start..name.end]),
            });
            after = value.end;
        }
    }
    (found, after)
}

/// `written` (a rewrite of `previous`) with each element's markup
/// attributes put back in its start tag as `previous` writes them, in
/// their places among the element's other attributes, which keep the
/// order and values of `written`.
pub(super) fn kept(written: &str, previous: &str) -> String {
    let markup = Markup::of(previous);
    // Every markup attribute has a declaration somewhere.
    if !previous.contains("xmlns") {
        return written.to_string();
    }
    // The host's start tags holding markup, by path.
    let host_tokens = tokenize(previous);
    let host_plan = plan_paths(previous, &host_tokens);
    let mut holding: HashMap<Vec<String>, &Token> = HashMap::new();
    for token in &host_tokens {
        if !matches!(token.kind, Kind::OpenTag { closing: false, .. }) {
            continue;
        }
        let Some(path) = host_plan.path_of(token.start) else { continue };
        let (attributes, _) = attributes_of(previous, token, &markup);
        if attributes.iter().any(|attribute| attribute.markup) {
            holding.insert(strings(path), token);
        }
    }
    if holding.is_empty() {
        return written.to_string();
    }
    let tokens = tokenize(written);
    let plan = plan_paths(written, &tokens);
    let mut out = String::with_capacity(written.len() + previous.len() / 8);
    let mut copied = 0;
    for token in &tokens {
        if !matches!(token.kind, Kind::OpenTag { closing: false, .. }) {
            continue;
        }
        let Some(host) = plan.path_of(token.start).and_then(|path| holding.get(&strings(path))) else {
            continue;
        };
        // What the writer wrote: the element's data attributes (it
        // writes no markup, see `first_markup_key`).
        let (data, tail) = attributes_of(written, token, &markup);
        let (slots, _) = attributes_of(previous, host, &markup);
        let mut data = data.into_iter();
        out.push_str(&written[copied..local_name_start(written, token) + name_len(written, token)]);
        for slot in &slots {
            if slot.markup {
                out.push_str(&previous[slot.blanks.start..slot.bytes.end]);
            } else if let Some(attribute) = data.next() {
                out.push_str(&previous[slot.blanks.clone()]);
                out.push_str(&written[attribute.bytes]);
            }
        }
        for attribute in data {
            out.push_str(&written[attribute.blanks.start..attribute.bytes.end]);
        }
        copied = tail;
    }
    out.push_str(&written[copied..]);
    out
}

#[cfg(test)]
mod tests {
    use super::{Markup, kept};

    #[test]
    fn markup_is_declarations_and_the_schema_instance_prefix_bound_so() {
        let markup = Markup::of("<r xmlns:i=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:xsi='urn:x'/>");
        assert!(markup.holds("xmlns") && markup.holds("xmlns:a") && markup.holds("i:type"));
        assert!(!markup.holds("xsi:type") && !markup.holds("a") && !markup.holds("m:kind"));
    }

    #[test]
    fn declarations_go_back_in_their_places_as_written() {
        let host = "<r a='1'\n   xmlns:x=\"urn:x\"  b=\"2\"><s xmlns=\"u\">1</s></r>";
        assert_eq!(
            kept("<r a=\"3\" b=\"2\" c=\"4\"><s>1</s></r>", host),
            "<r a=\"3\"\n   xmlns:x=\"urn:x\"  b=\"2\" c=\"4\"><s xmlns=\"u\">1</s></r>"
        );
        // An attribute removed takes its blanks; the markup stays.
        assert_eq!(kept("<r b=\"2\"><s>1</s></r>", host), "<r b=\"2\"\n   xmlns:x=\"urn:x\"><s xmlns=\"u\">1</s></r>");
        assert_eq!(kept("<r/>", "<r xmlns=\"u\"/>"), "<r xmlns=\"u\"/>");
    }
}
