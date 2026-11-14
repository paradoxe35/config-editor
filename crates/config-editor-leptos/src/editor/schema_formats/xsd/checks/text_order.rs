//! The elements of an XML document as written: the data model folds
//! repeated siblings into one list (at the first one's place) and keys
//! names without their namespace; the content check needs the children in
//! their order, and the namespaces of the ones a wildcard allows.

use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;
use quick_xml::reader::NsReader;

use super::super::super::CheckedDocument;

/// The namespace of `xsi:type` and the like (markup, not data).
const SCHEMA_INSTANCE: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// An element: its local name and namespace, its attributes (local name
/// and namespace; markup left out) and its child elements, in order.
#[derive(Debug, Default)]
pub(super) struct Element {
    pub(super) name: String,
    pub(super) namespace: Option<String>,
    pub(super) attributes: Vec<(String, Option<String>)>,
    pub(super) children: Vec<Element>,
}

/// The top-level elements of `document` when it is XML (a top element
/// marked `config-root="true"` stands for its children, as the reader
/// makes it); `None` in another language, or when the text does not read.
pub(super) fn read(document: &CheckedDocument<'_>) -> Option<Vec<Element>> {
    if document.language.info().label != "XML" {
        return None;
    }
    let mut reader = NsReader::from_str(document.text);
    let mut stack: Vec<Element> = vec![Element::default()];
    let mut config_root = false;
    loop {
        let (resolved, event) = reader.read_resolved_event().ok()?;
        let namespace = namespace_of(&resolved);
        match event {
            Event::Eof => break,
            Event::Start(start) | Event::Empty(start)
                if stack.len() == 1 && is_config_root(&start) =>
            {
                config_root = true;
            }
            Event::Start(start) => {
                let element = element(&reader, &start, namespace);
                stack.push(element);
            }
            Event::Empty(start) => {
                let element = element(&reader, &start, namespace);
                stack.last_mut()?.children.push(element);
            }
            Event::End(_) => {
                if stack.len() == 1 && config_root {
                    config_root = false;
                    continue;
                }
                let done = stack.pop()?;
                stack.last_mut()?.children.push(done);
            }
            _ => {}
        }
    }
    (stack.len() == 1)
        .then(|| stack.pop().map(|root| root.children))
        .flatten()
}

fn namespace_of(resolved: &ResolveResult<'_>) -> Option<String> {
    match resolved {
        ResolveResult::Bound(namespace) => Some(namespace.as_ref().to_string()),
        _ => None,
    }
}

fn element(reader: &NsReader<&[u8]>, start: &BytesStart<'_>, namespace: Option<String>) -> Element {
    let attributes = start
        .attributes()
        .flatten()
        .filter_map(|attribute| {
            let qualified = attribute.key.into_inner();
            if qualified == "xmlns" || qualified.starts_with("xmlns:") {
                return None;
            }
            let (resolved, local) = reader.resolver().resolve_attribute(attribute.key);
            let namespace = namespace_of(&resolved);
            (namespace.as_deref() != Some(SCHEMA_INSTANCE))
                .then(|| (local.into_inner().to_string(), namespace))
        })
        .collect();
    Element {
        name: start.local_name().into_inner().to_string(),
        namespace,
        attributes,
        children: Vec::new(),
    }
}

/// Whether a top-level start tag marks the transparent config root.
fn is_config_root(start: &BytesStart<'_>) -> bool {
    start.attributes().flatten().any(|attribute| {
        attribute.key.into_inner() == "config-root" && attribute.value.trim() == "true"
    })
}
