//! Parsing to a JSON value, and the source spans of keys and values.

use super::*;

/// Every error carries a one-based line and character column.
pub(super) fn xml_parse(source: &str) -> Result<Value, ParseError> {
    // `<>` and `</>` name no element.
    if let Some(at) = source.find("<>").or_else(|| source.find("</>")) {
        let (line, column) = line_column(source, at);
        return Err(ParseError {
            message: "an element needs a name".to_string(),
            line: Some(line),
            column: Some(column),
        });
    }
    build_value(source)
}

/// An element whose end tag has not been read yet.
pub(super) struct OpenElement {
    pub(super) name: String,
    pub(super) attrs: Map<String, Value>,
    pub(super) children: Map<String, Value>,
    pub(super) text: String,
    pub(super) start: usize,
    /// A top-level element marked `config-root="true"`.
    pub(super) config_root: bool,
}

/// A finished top-level element.
pub(super) struct TopLevel {
    pub(super) name: String,
    pub(super) value: Value,
    pub(super) start: usize,
    pub(super) config_root: bool,
}

/// Builds the document model from `quick-xml` events, reporting structural
/// errors at the offending markup.
pub(super) fn build_value(source: &str) -> Result<Value, ParseError> {
    if source.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut stack: Vec<OpenElement> = Vec::new();
    let mut roots: Vec<TopLevel> = Vec::new();
    let mut pending_text = String::new();
    loop {
        // `buffer_position` before a read is where the next event begins.
        let event_start = reader.buffer_position() as usize;
        let event = match reader.read_event_into(&mut buf) {
            Ok(event) => event,
            Err(error) => {
                let position = reader.error_position() as usize;
                // Noticed only at the end of the document (a start tag missing
                // its `>` swallows what follows): the mistake is in the
                // innermost element still open.
                if position >= source.trim_end().len().saturating_sub(1)
                    && let Some(open) = stack.last()
                {
                    return Err(syntax_error(
                        source,
                        open.start,
                        format!("<{}> is never closed: {error}", open.name),
                    ));
                }
                return Err(syntax_error(source, position, error.to_string()));
            }
        };
        match event {
            Event::Eof => break,
            Event::Start(element) => {
                flush_text(&mut pending_text, &mut stack);
                let top_level = stack.is_empty();
                stack.push(open_element(source, &element, event_start, top_level)?);
            }
            Event::Empty(element) => {
                flush_text(&mut pending_text, &mut stack);
                let top_level = stack.is_empty();
                let open = open_element(source, &element, event_start, top_level)?;
                push_child(&mut stack, &mut roots, open);
            }
            Event::End(element) => {
                flush_text(&mut pending_text, &mut stack);
                let name = element.local_name().into_inner().to_string();
                let start = markup_start(source, event_start);
                let Some(open) = stack.pop() else {
                    return Err(syntax_error(
                        source,
                        start,
                        format!("unexpected closing tag \"{name}\""),
                    ));
                };
                if open.name != name {
                    return Err(syntax_error(
                        source,
                        start,
                        format!(
                            "mismatched closing tag: expected \"{}\", found \"{name}\"",
                            open.name
                        ),
                    ));
                }
                push_child(&mut stack, &mut roots, open);
            }
            Event::Text(text) => {
                let content = text.xml10_content();
                add_text(
                    source,
                    event_start,
                    &content,
                    &mut pending_text,
                    stack.is_empty(),
                )?;
            }
            Event::CData(data) => {
                let content = data.xml10_content();
                add_text(
                    source,
                    event_start,
                    &content,
                    &mut pending_text,
                    stack.is_empty(),
                )?;
            }
            Event::GeneralRef(reference) => {
                let name = reference.into_inner().into_owned();
                let content = resolve_reference(&name).unwrap_or_else(|| format!("&{name};"));
                add_text(
                    source,
                    event_start,
                    &content,
                    &mut pending_text,
                    stack.is_empty(),
                )?;
            }
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {}
        }
        buf.clear();
    }
    if let Some(open) = stack.last() {
        return Err(syntax_error(
            source,
            open.start,
            format!("unclosed tag \"{}\"", open.name),
        ));
    }

    document_value(source, roots)
}

/// Unwraps a config root, which must stand alone; otherwise every top-level
/// element is a document property.
pub(super) fn document_value(source: &str, roots: Vec<TopLevel>) -> Result<Value, ParseError> {
    let mut marked = roots.iter().filter(|root| root.config_root);
    if let Some(root) = marked.next() {
        if let Some(second) = marked.next() {
            return Err(syntax_error(
                source,
                second.start,
                format!("only one element may carry {CONFIG_ROOT_ATTRIBUTE}=\"true\""),
            ));
        }
        if let Some(other) = roots.iter().find(|other| !other.config_root) {
            return Err(syntax_error(
                source,
                other.start,
                format!(
                    "<{}> is outside the config root <{}>; a {CONFIG_ROOT_ATTRIBUTE} element must be the only top-level element",
                    other.name, root.name
                ),
            ));
        }
        let root = roots.into_iter().next().expect("the marked root");
        return match root.value {
            Value::Object(map) => Ok(Value::Object(map)),
            _ => Err(syntax_error(
                source,
                root.start,
                format!("the config root <{}> holds elements, not text", root.name),
            )),
        };
    }
    let mut map = Map::new();
    for root in roots {
        insert_child(&mut map, root.name, root.value);
    }
    Ok(Value::Object(map))
}

/// Duplicate attributes are an error.
pub(super) fn open_element(
    source: &str,
    element: &BytesStart<'_>,
    event_start: usize,
    top_level: bool,
) -> Result<OpenElement, ParseError> {
    let start = markup_start(source, event_start);
    let name = element.local_name().into_inner().to_string();
    let mut attrs = Map::new();
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|error| match error {
            AttrError::Duplicated(..) => {
                syntax_error(source, start, format!("duplicate attribute in <{name}>"))
            }
            other => syntax_error(source, start, other.to_string()),
        })?;
        let key = attribute.key.into_inner().to_string();
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|error| syntax_error(source, start, error.to_string()))?;
        attrs.insert(key, typed_scalar(&value));
    }
    let config_root = top_level && attrs.get(CONFIG_ROOT_ATTRIBUTE) == Some(&Value::Bool(true));
    if config_root {
        attrs.clear();
    }
    Ok(OpenElement {
        name,
        attrs,
        children: Map::new(),
        text: String::new(),
        start,
        config_root,
    })
}

/// Collects text inside an element; non-blank text outside any element is an
/// error.
pub(super) fn add_text(
    source: &str,
    event_start: usize,
    content: &str,
    pending: &mut String,
    at_root: bool,
) -> Result<(), ParseError> {
    if !at_root {
        pending.push_str(content);
    } else if !content.trim().is_empty() {
        let at = markup_start(source, event_start);
        return Err(syntax_error(source, at, "text outside the root element"));
    }
    Ok(())
}

/// Appends pending text to the innermost open element. Text split by entity
/// references is joined in document order.
pub(super) fn flush_text(pending: &mut String, stack: &mut [OpenElement]) {
    let text = std::mem::take(pending);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    if let Some(open) = stack.last_mut() {
        open.text.push_str(trimmed);
    }
}

/// Turns a finished element into its value. A text-only element is a typed
/// scalar; children and attributes make an object, with `#text` only for mixed
/// content. An empty element is an empty object.
pub(super) fn finalize(open: OpenElement) -> Value {
    let text = open.text.trim().to_string();
    if open.children.is_empty() && open.attrs.is_empty() && !text.is_empty() {
        return typed_scalar(&text);
    }
    let mut map = open.attrs;
    for (name, value) in open.children {
        insert_child(&mut map, name, value);
    }
    if !text.is_empty() {
        map.insert("#text".to_string(), Value::String(text));
    }
    Value::Object(map)
}

pub(super) fn push_child(stack: &mut [OpenElement], roots: &mut Vec<TopLevel>, open: OpenElement) {
    let (name, start, config_root) = (open.name.clone(), open.start, open.config_root);
    let value = finalize(open);
    match stack.last_mut() {
        Some(parent) => insert_child(&mut parent.children, name, value),
        None => roots.push(TopLevel {
            name,
            value,
            start,
            config_root,
        }),
    }
}

/// Repeated sibling names fold into an array.
pub(super) fn insert_child(map: &mut Map<String, Value>, name: String, value: Value) {
    match map.get_mut(&name) {
        Some(Value::Array(items)) => items.push(value),
        Some(existing) => {
            let first = std::mem::replace(existing, Value::Null);
            *existing = Value::Array(vec![first, value]);
        }
        None => {
            map.insert(name, value);
        }
    }
}

/// Numbers, booleans, and `null` are recognized; everything else is a string.
/// Where the schema wants a string, validation reads them back as text.
pub(super) fn typed_scalar(text: &str) -> Value {
    let trimmed = text.trim();
    match trimmed {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        // Only text a number writes back unchanged (`1e3`, `1.50` and `-0`
        // stay text, so saving them keeps what was typed).
        _ => match serde_json::from_str::<Value>(trimmed) {
            Ok(Value::Number(number)) if number.to_string() == trimmed => Value::Number(number),
            _ => Value::String(trimmed.to_string()),
        },
    }
}

/// XML entity references: the five predefined entities and character
/// references. Unknown entities stay literal.
pub(super) fn resolve_reference(name: &str) -> Option<String> {
    if let Some(character) = named_entity(name) {
        return Some(character.to_string());
    }
    let numeric = numeric_entity(name.strip_prefix('#')?)?;
    Some(numeric.to_string())
}

pub(super) fn syntax_error(source: &str, byte: usize, message: impl Into<String>) -> ParseError {
    let (line, column) = line_column(source, byte.min(source.len()));
    ParseError {
        message: message.into(),
        line: Some(line),
        column: Some(column),
    }
}

/// First non-whitespace byte at or after `from`, so errors point at `<`.
pub(super) fn markup_start(source: &str, from: usize) -> usize {
    let from = from.min(source.len());
    source[from..]
        .find(|character: char| !character.is_whitespace())
        .map_or(from, |offset| from + offset)
}

pub(super) fn span_between(source: &str, start: usize, end: usize) -> (usize, usize, usize) {
    let (line, column) = line_column(source, start);
    (line, column, column + source[start..end].chars().count())
}

pub(super) fn xml_name_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let (start, end) = span_at(analyze(source, 0).key_spans, &pointer_segments(path))?;
    Some(span_between(source, start, end))
}

pub(super) fn xml_value_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let (start, end) = span_at(analyze(source, 0).value_spans, &pointer_segments(path))?;
    Some(span_between(source, start, end))
}

/// The span recorded for `wanted`. A lone element read as a one-item array
/// is addressed as `/dns/0` but recorded as `/dns`, so a `0` the recorded
/// path lacks also matches.
pub(super) fn span_at(spans: Vec<(Vec<String>, usize, usize)>, wanted: &[String]) -> Option<(usize, usize)> {
    let lone_items = |segments: &[String]| {
        let mut recorded = segments.iter().peekable();
        for segment in wanted {
            if recorded.peek() == Some(&segment) {
                recorded.next();
            } else if segment != "0" {
                return false;
            }
        }
        recorded.next().is_none()
    };
    spans
        .iter()
        .find(|(segments, _, _)| segments == wanted)
        .or_else(|| spans.iter().find(|(segments, _, _)| lone_items(segments)))
        .map(|(_, start, end)| (*start, *end))
}

pub(super) fn first_content_span(source: &str) -> Option<(usize, usize, usize)> {
    source.lines().enumerate().find_map(|(index, line)| {
        let start = line.find(|character: char| !character.is_whitespace())?;
        Some((
            index + 1,
            line[..start].chars().count() + 1,
            line.chars().count() + 1,
        ))
    })
}
