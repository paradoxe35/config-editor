//! Entities a document's DTD declares (`<!ENTITY e "x">`, used `&e;`): the
//! document model reads `&e;` as that text, which another language would
//! write as the text `&e;`, not the entity: such a document does not
//! convert. (In an attribute, such a reference is a syntax error.)

use super::*;

/// Why `source` does not convert: the first entity reference in element
/// text that is neither predefined nor a character reference, at its
/// element's pointer. `None` when there is none.
pub(super) fn declared_entity(source: &str) -> Option<String> {
    if !source.contains('&') {
        return None;
    }
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let reason = |stack: &[String], name: &str| {
        let pointer: String = stack.iter().map(|name| format!("/{}", name.replace('~', "~0").replace('/', "~1"))).collect();
        let at = if pointer.is_empty() { "/".to_string() } else { pointer };
        format!("this XML uses the entity &{name};, which no other language can write: edit it as XML (at {at})")
    };
    loop {
        let event = reader.read_event_into(&mut buf).ok()?;
        match event {
            Event::Eof => return None,
            Event::Start(element) => stack.push(element.local_name().as_ref().to_string()),
            Event::End(_) => {
                stack.pop();
            }
            Event::GeneralRef(reference) => {
                let name = reference.into_inner().to_string();
                if resolve_reference(&name).is_none() {
                    return Some(reason(&stack, &name));
                }
            }
            _ => {}
        }
        buf.clear();
    }
}
