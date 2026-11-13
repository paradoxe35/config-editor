//! An XSD document as a tree of elements, read with `quick-xml`: each
//! element's namespace and local name, its attributes, children and text,
//! where it starts, and the namespace prefixes in scope (to resolve the
//! qualified names in attribute values, `type="xs:string"`).

use std::rc::Rc;

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use super::super::super::diagnostics::SchemaError;

/// The XML Schema namespace.
pub(super) const XSD_NS: &str = "http://www.w3.org/2001/XMLSchema";

/// The prefixes in scope at an element (`None`: the default namespace).
type Prefixes = Rc<Vec<(Option<String>, String)>>;

/// One element of the document.
#[derive(Debug)]
pub(super) struct Node {
    pub(super) namespace: Option<String>,
    pub(super) name: String,
    attributes: Vec<(String, String)>,
    pub(super) children: Vec<Node>,
    /// Its text (and its descendants'), for documentation.
    pub(super) text: String,
    pub(super) line: usize,
    pub(super) column: usize,
    prefixes: Prefixes,
}

impl Node {
    /// Whether it is the XSD element `name` (`element`, `complexType`, …).
    pub(super) fn is(&self, name: &str) -> bool {
        self.name == name && self.namespace.as_deref() == Some(XSD_NS)
    }

    /// The value of the unprefixed attribute `name`.
    pub(super) fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The XSD children (annotations included).
    pub(super) fn xsd_children(&self) -> impl Iterator<Item = &Node> {
        self.children
            .iter()
            .filter(|child| child.namespace.as_deref() == Some(XSD_NS))
    }

    /// The first XSD child named `name`.
    pub(super) fn child(&self, name: &str) -> Option<&Node> {
        self.xsd_children().find(|child| child.name == name)
    }

    /// A qualified name written in an attribute value, resolved in this
    /// element's scope: its namespace and local name.
    pub(super) fn resolve(&self, qualified: &str) -> (Option<String>, String) {
        let qualified = qualified.trim();
        let (prefix, local) = match qualified.split_once(':') {
            Some((prefix, local)) => (Some(prefix), local),
            None => (None, qualified),
        };
        let namespace = self
            .prefixes
            .iter()
            .rev()
            .find(|(known, _)| known.as_deref() == prefix)
            .map(|(_, uri)| uri.clone())
            .filter(|uri| !uri.is_empty());
        (namespace, local.to_string())
    }

    /// The text of its first `xs:annotation/xs:documentation` (preferring
    /// one without `xml:lang`, or in English), its whitespace collapsed.
    pub(super) fn documentation(&self) -> Option<String> {
        let annotation = self.child("annotation")?;
        let docs: Vec<&Node> = annotation
            .xsd_children()
            .filter(|node| node.name == "documentation")
            .collect();
        let lang = |node: &Node| {
            node.attributes
                .iter()
                .find(|(key, _)| key == "xml:lang")
                .map(|(_, value)| value.to_ascii_lowercase())
        };
        let chosen = docs
            .iter()
            .find(|node| lang(node).is_none_or(|lang| lang == "en" || lang.starts_with("en-")))
            .or(docs.first())?;
        let text = chosen.text.split_whitespace().collect::<Vec<_>>().join(" ");
        (!text.is_empty()).then_some(text)
    }
}

/// Byte offsets of each line's start, to place an element.
struct Lines(Vec<usize>);

impl Lines {
    fn of(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(at, _)| at + 1));
        Lines(starts)
    }

    /// One-based line and character column of byte `at` in `text`.
    fn place(&self, text: &str, at: usize) -> (usize, usize) {
        let line = self.0.partition_point(|start| *start <= at);
        let start = self.0[line - 1];
        let column = text
            .get(start..at)
            .map_or(1, |before| before.chars().count() + 1);
        (line, column)
    }
}

/// The document's root element, or why `text` is no XML.
pub(super) fn read(text: &str) -> Result<Node, SchemaError> {
    let lines = Lines::of(text);
    let error = |at: usize, message: String| {
        let (line, column) = lines.place(text, at.min(text.len()));
        SchemaError(format!("{message} (line {line}, column {column})"))
    };
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(false);
    let root_prefixes: Prefixes = Rc::new(vec![(
        Some("xml".to_string()),
        "http://www.w3.org/XML/1998/namespace".to_string(),
    )]);
    let mut stack: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;
    loop {
        let start = reader.buffer_position() as usize;
        let event = reader
            .read_event()
            .map_err(|problem| error(reader.error_position() as usize, problem.to_string()))?;
        match event {
            Event::Eof => break,
            Event::Start(element) | Event::Empty(element) if root.is_some() && stack.is_empty() => {
                let _ = element;
                return Err(error(start, "more than one root element".to_string()));
            }
            Event::Start(element) => {
                let parent = stack.last().map_or(&root_prefixes, |node| &node.prefixes);
                let node = open(&element, parent, &lines, text, start)
                    .map_err(|message| error(start, message))?;
                stack.push(node);
            }
            Event::Empty(element) => {
                let parent = stack.last().map_or(&root_prefixes, |node| &node.prefixes);
                let node = open(&element, parent, &lines, text, start)
                    .map_err(|message| error(start, message))?;
                close(&mut stack, &mut root, node);
            }
            Event::End(_) => {
                let Some(node) = stack.pop() else {
                    return Err(error(start, "an end tag without its start".to_string()));
                };
                close(&mut stack, &mut root, node);
            }
            Event::Text(content) => {
                if let Some(node) = stack.last_mut() {
                    node.text.push_str(&content.xml10_content());
                }
            }
            Event::CData(content) => {
                if let Some(node) = stack.last_mut() {
                    node.text.push_str(&content.xml10_content());
                }
            }
            Event::GeneralRef(reference) => {
                if let Some(node) = stack.last_mut() {
                    let name = reference.into_inner().into_owned();
                    node.text.push_str(&entity(&name));
                }
            }
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err(error(text.len(), "an element is never closed".to_string()));
    }
    root.ok_or_else(|| SchemaError("no root element".to_string()))
}

/// The text of the entity or character reference `&name;`.
fn entity(name: &str) -> String {
    let named = match name {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => None,
    };
    let numeric = || {
        let digits = name.strip_prefix('#')?;
        let code = match digits.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => digits.parse().ok()?,
        };
        char::from_u32(code)
    };
    named
        .or_else(numeric)
        .map_or_else(|| format!("&{name};"), String::from)
}

/// A finished element, into its parent (its text joins the parent's, for
/// documentation written with markup inside).
fn close(stack: &mut [Node], root: &mut Option<Node>, node: Node) {
    match stack.last_mut() {
        Some(parent) => {
            parent.text.push_str(&node.text);
            parent.children.push(node);
        }
        None => *root = Some(node),
    }
}

/// The element `element` starts, in the scope of `parent`'s prefixes.
fn open(
    element: &BytesStart<'_>,
    parent: &Prefixes,
    lines: &Lines,
    text: &str,
    start: usize,
) -> Result<Node, String> {
    let mut attributes = Vec::new();
    let mut declared: Vec<(Option<String>, String)> = Vec::new();
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|problem| problem.to_string())?;
        let key = attribute.key.as_ref().to_string();
        let value = attribute
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|problem| problem.to_string())?
            .to_string();
        if key == "xmlns" {
            declared.push((None, value));
        } else if let Some(prefix) = key.strip_prefix("xmlns:") {
            declared.push((Some(prefix.to_string()), value));
        } else {
            attributes.push((key, value));
        }
    }
    let prefixes = if declared.is_empty() {
        Rc::clone(parent)
    } else {
        let mut all = parent.as_ref().clone();
        all.extend(declared);
        Rc::new(all)
    };
    let qualified = element.name().as_ref().to_string();
    let (prefix, name) = match qualified.split_once(':') {
        Some((prefix, local)) => (Some(prefix.to_string()), local.to_string()),
        None => (None, qualified),
    };
    let namespace = prefixes
        .iter()
        .rev()
        .find(|(known, _)| *known == prefix)
        .map(|(_, uri)| uri.clone())
        .filter(|uri| !uri.is_empty());
    let at = start + text[start..].find('<').unwrap_or(0);
    let (line, column) = lines.place(text, at);
    Ok(Node {
        namespace,
        name,
        attributes,
        children: Vec::new(),
        text: String::new(),
        line,
        column,
        prefixes,
    })
}
