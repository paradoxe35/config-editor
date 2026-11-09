//! Keys that cannot name an XML element or attribute (`/*`, `1x`, `a b`,
//! a bare `@`): refused with the key and its place before anything is
//! written, whatever the key holds and however it starts (`@/*` is no
//! attribute `/*`).

use serde_json::Value;

/// Whether `character` may start an XML name (the XML 1.0 `NameStartChar`).
fn starts_name(character: char) -> bool {
    matches!(character,
        ':' | 'A'..='Z' | '_' | 'a'..='z'
        | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}'
        | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}' | '\u{200C}'..='\u{200D}'
        | '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}' | '\u{3001}'..='\u{D7FF}'
        | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}' | '\u{10000}'..='\u{EFFFF}')
}

/// Whether `character` may follow in an XML name (`NameChar`).
fn continues_name(character: char) -> bool {
    starts_name(character)
        || matches!(character, '-' | '.' | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
}

pub(super) fn is_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters.next().is_some_and(starts_name) && characters.all(continues_name)
}

/// The first key of `value` (an `@` attribute's name after its `@`) that is
/// no XML name, with the reason and its pointer. The empty key and the text
/// key (`#text`) are left to their own checks.
pub(super) fn first_non_name(value: &Value) -> Option<String> {
    find(value, &mut String::new())
}

fn find(value: &Value, pointer: &mut String) -> Option<String> {
    let length = pointer.len();
    let found = match value {
        Value::Object(map) => map.iter().find_map(|(key, child)| {
            let at = pointer.len();
            pointer.push('/');
            pointer.push_str(&key.replace('~', "~0").replace('/', "~1"));
            let name = key.strip_prefix('@').unwrap_or(key);
            let found = if !key.is_empty() && key != "#text" && !is_name(name) {
                Some(format!("`{key}` is not an XML name, so it cannot name an element or an attribute (at {pointer})"))
            } else {
                find(child, pointer)
            };
            pointer.truncate(at);
            found
        }),
        Value::Array(items) => items.iter().enumerate().find_map(|(index, item)| {
            let at = pointer.len();
            pointer.push_str(&format!("/{index}"));
            let found = find(item, pointer);
            pointer.truncate(at);
            found
        }),
        _ => None,
    };
    pointer.truncate(length);
    found
}
