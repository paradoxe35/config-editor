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

#[cfg(test)]
mod tests {
    use super::Markup;

    #[test]
    fn markup_is_declarations_and_the_schema_instance_prefix_bound_so() {
        let markup = Markup::of("<r xmlns:i=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:xsi='urn:x'/>");
        assert!(markup.holds("xmlns") && markup.holds("xmlns:a") && markup.holds("i:type"));
        assert!(!markup.holds("xsi:type") && !markup.holds("a") && !markup.holds("m:kind"));
    }
}
