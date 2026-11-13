//! Parsing to a JSON value, and the source spans of keys and values.

use super::*;

/// How text scalars are read: guessed (`<v>1</v>` is the number 1), or
/// left as their exact text for the schema to type (see
/// `ConfigLanguage::parse_text`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Reading {
    Guessed,
    Text,
}

impl Reading {
    /// A text scalar (an attribute value, an element's text) as read.
    fn scalar(self, text: &str) -> Value {
        match self {
            Reading::Guessed => typed_scalar(text),
            Reading::Text => Value::String(text.to_string()),
        }
    }
}

/// Every error carries a one-based line and character column.
pub(super) fn xml_parse(source: &str) -> Result<Value, ParseError> {
    xml_parse_as(source, Reading::Guessed)
}

/// [`xml_parse`], its text scalars read as `reading` says.
pub(super) fn xml_parse_as(source: &str, reading: Reading) -> Result<Value, ParseError> {
    // A character XML 1.0 does not allow (a control character, U+FFFE),
    // which XML readers refuse.
    if let Some((at, character)) = source.char_indices().find(|(_, character)| !is_xml_char(*character)) {
        return Err(syntax_error(source, at, not_xml_char_message(character)));
    }
    // `<>` and `</>` name no element.
    if let Some(at) = source.find("<>").or_else(|| source.find("</>")) {
        let (line, column) = line_column(source, at);
        return Err(ParseError {
            message: "an element needs a name".to_string(),
            line: Some(line),
            column: Some(column),
        });
    }
    build_value_as(source, reading)
}

/// Whether XML 1.0 allows `character` in a document, raw or as a
/// character reference: tab, line feed, carriage return, and U+0020 on but
/// the surrogates, U+FFFE and U+FFFF (its `Char` production).
pub(super) fn is_xml_char(character: char) -> bool {
    matches!(character, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..)
}

/// Why `character` cannot be in an XML document.
pub(super) fn not_xml_char_message(character: char) -> String {
    format!("XML 1.0 cannot hold the character U+{:04X}, not even escaped", u32::from(character))
}

/// The JSON pointer of the first key or string in `value` holding a
/// character XML 1.0 does not allow, and that character.
pub(super) fn first_non_xml_char(value: &Value, pointer: &str) -> Option<(String, char)> {
    let bad = |text: &str| text.chars().find(|character| !is_xml_char(*character));
    match value {
        Value::String(text) => bad(text).map(|character| (pointer.to_string(), character)),
        Value::Object(map) => map.iter().find_map(|(key, child)| {
            let child_pointer = format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1"));
            bad(key)
                .map(|character| (child_pointer.clone(), character))
                .or_else(|| first_non_xml_char(child, &child_pointer))
        }),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .find_map(|(index, child)| first_non_xml_char(child, &format!("{pointer}/{index}"))),
        _ => None,
    }
}

/// An element whose end tag has not been read yet.
pub(super) struct OpenElement {
    pub(super) name: String,
    pub(super) attrs: Map<String, Value>,
    pub(super) children: Map<String, Value>,
    /// The text between its children, each run trimmed (mixed content).
    pub(super) text: String,
    /// All of its text exactly as read, whitespace included.
    pub(super) exact: String,
    /// Some of its text came from a CDATA section.
    pub(super) cdata: bool,
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

/// Deepest element nesting read, as JSON's parser limits it: deeper documents
/// are an error, not a stack overflow further on.
const MAX_NESTING: usize = 128;

/// Builds the document model from `quick-xml` events, reporting structural
/// errors at the offending markup.
pub(super) fn build_value(source: &str) -> Result<Value, ParseError> {
    build_value_as(source, Reading::Guessed)
}

/// [`build_value`], its text scalars read as `reading` says.
pub(super) fn build_value_as(source: &str, reading: Reading) -> Result<Value, ParseError> {
    if source.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut stack: Vec<OpenElement> = Vec::new();
    let mut roots: Vec<TopLevel> = Vec::new();
    let mut pending_text = String::new();
    let markup = declarations::Markup::of(source);
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
                if stack.len() >= MAX_NESTING {
                    return Err(syntax_error(
                        source,
                        event_start,
                        super::super::nesting::too_deep("Elements"),
                    ));
                }
                let top_level = stack.is_empty();
                stack.push(open_element(source, &element, (event_start, top_level), &markup, reading)?);
            }
            Event::Empty(element) => {
                flush_text(&mut pending_text, &mut stack);
                let top_level = stack.is_empty();
                let open = open_element(source, &element, (event_start, top_level), &markup, reading)?;
                push_child(&mut stack, &mut roots, open, reading);
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
                push_child(&mut stack, &mut roots, open, reading);
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
                if let Some(open) = stack.last_mut() {
                    open.cdata = true;
                }
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
                if let Some(message) = bad_character_reference(&name) {
                    return Err(syntax_error(source, event_start, message));
                }
                let content = resolve_reference(&name).unwrap_or_else(|| format!("&{name};"));
                add_text(
                    source,
                    event_start,
                    &content,
                    &mut pending_text,
                    stack.is_empty(),
                )?;
            }
            Event::Comment(_) => check_comment(source, event_start)?,
            Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {}
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

/// Duplicate attributes are an error. Markup attributes (namespace
/// declarations, `xsi:*`) are no data (see `declarations`).
pub(super) fn open_element(
    source: &str,
    element: &BytesStart<'_>,
    (event_start, top_level): (usize, bool),
    markup: &declarations::Markup,
    reading: Reading,
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
        // Not well-formed XML, though the reader would take it.
        if attribute.value.contains('<') {
            return Err(syntax_error(source, start, format!("\"<\" must be written &lt; in an attribute value (<{name} {key}=…>)")));
        }
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|error| syntax_error(source, start, error.to_string()))?;
        // A reference to a character XML 1.0 cannot hold (`&#1;`), which
        // the reader resolves (the raw character is refused before).
        if let Some(character) = value.chars().find(|character| !is_xml_char(*character)) {
            return Err(syntax_error(source, start, not_xml_char_message(character)));
        }
        if !markup.holds(&key) {
            attrs.insert(key, reading.scalar(&value));
        }
    }
    let config_root = top_level
        && matches!(attrs.get(CONFIG_ROOT_ATTRIBUTE), Some(Value::Bool(true)))
        || top_level
            && reading == Reading::Text
            && attrs.get(CONFIG_ROOT_ATTRIBUTE).and_then(Value::as_str).map(str::trim) == Some("true");
    if config_root {
        attrs.clear();
    }
    Ok(OpenElement {
        name,
        attrs,
        children: Map::new(),
        text: String::new(),
        exact: String::new(),
        cdata: false,
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
    if let Some(open) = stack.last_mut() {
        open.exact.push_str(&text);
        open.text.push_str(text.trim());
    }
}

/// Turns a finished element into its value. A text-only element is a typed
/// scalar; children and attributes make an object, with `#text` only for mixed
/// content. An empty element is an empty object.
///
/// Text beside no child element is the value exactly as an XML reader reports
/// it, its leading and trailing whitespace and line breaks included (a
/// number, boolean or `null` is read without the whitespace around it): what
/// the host's program reads, so editing it elsewhere keeps it. Whitespace
/// alone is the empty element's formatting unless written as CDATA. Beside
/// child elements, the text runs are trimmed: there the whitespace is the
/// children's indentation.
pub(super) fn finalize(open: OpenElement, reading: Reading) -> Value {
    let text = if open.children.is_empty() {
        if open.exact.trim().is_empty() && !open.cdata { String::new() } else { open.exact }
    } else {
        open.text.trim().to_string()
    };
    if open.children.is_empty() && open.attrs.is_empty() && (!text.is_empty() || open.cdata) {
        return reading.scalar(&text);
    }
    // An attribute a child element shares its name with is `@name`: the
    // two stay apart, as the serializer writes them.
    let mut map: Map<String, Value> = open
        .attrs
        .into_iter()
        .map(|(name, value)| match open.children.contains_key(&name) {
            true => (format!("@{name}"), value),
            false => (name, value),
        })
        .collect();
    for (name, value) in open.children {
        insert_child(&mut map, name, value);
    }
    if !text.is_empty() {
        map.insert("#text".to_string(), Value::String(text));
    }
    Value::Object(map)
}

pub(super) fn push_child(stack: &mut [OpenElement], roots: &mut Vec<TopLevel>, open: OpenElement, reading: Reading) {
    let (name, start, config_root) = (open.name.clone(), open.start, open.config_root);
    let value = finalize(open, reading);
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
            _ => Value::String(text.to_string()),
        },
    }
}

/// XML entity references: the five predefined entities and character
/// references. Unknown entities stay literal.
pub(super) fn resolve_reference(name: &str) -> Option<String> {
    if let Some(character) = named_entity(name) {
        return Some(character.to_string());
    }
    // `numeric_entity` takes the reference with its `#` (`#65`, `#x41`).
    Some(numeric_entity(name)?.to_string())
}

/// Why the character reference `&{name};` (`#1`, `#xFFFE`) is not well-formed
/// XML: it names a character XML 1.0 cannot hold, or no character at all.
/// `None` for one that reads, and for an entity (`amp`, a declared one).
pub(super) fn bad_character_reference(name: &str) -> Option<String> {
    let digits = name.strip_prefix('#')?;
    if numeric_entity(name).is_some() {
        return None;
    }
    let code = match digits.strip_prefix(['x', 'X']) {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => digits.parse::<u32>().ok(),
    };
    Some(match code {
        Some(code) if code <= 0x10_ffff => format!("XML 1.0 cannot hold the character U+{code:04X}, not even escaped"),
        _ => format!("&{name}; is no character reference"),
    })
}

pub(super) fn syntax_error(source: &str, byte: usize, message: impl Into<String>) -> ParseError {
    let (line, column) = line_column(source, byte.min(source.len()));
    ParseError {
        message: message.into(),
        line: Some(line),
        column: Some(column),
    }
}

/// The comment starting at or after byte `from` keeps to XML's rule: no
/// `--` inside it, and no `-` just before its `-->` (`<!-- a -- b -->`
/// and `<!-- a --->` are not well-formed). The error points at the `--`.
pub(super) fn check_comment(source: &str, from: usize) -> Result<(), ParseError> {
    let Some(open) = source.get(from..).and_then(|rest| rest.find("<!--")) else {
        return Ok(());
    };
    let body_start = from + open + 4;
    let body_end = source[body_start..]
        .find("-->")
        .map_or(source.len(), |end| body_start + end);
    let body = &source[body_start..body_end];
    let dashes = body.find("--").or_else(|| body.ends_with('-').then(|| body.len() - 1));
    match dashes {
        Some(at) => Err(syntax_error(
            source,
            body_start + at,
            "a comment cannot contain \"--\" (nor end with \"-\" before \"-->\")",
        )),
        None => Ok(()),
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

/// Name and value spans of a whole document, as `analyze` records them.
type DocumentSpans = (Vec<(Vec<String>, usize, usize)>, Vec<(Vec<String>, usize, usize)>);

thread_local! {
    /// The last document's spans: diagnostics, hover and saves look up many
    /// positions in one text, and each scan reads all of it.
    static LAST_SPANS: std::cell::RefCell<Option<(u64, std::rc::Rc<DocumentSpans>)>> = const { std::cell::RefCell::new(None) };
}

/// Forgets the last document's spans.
pub(super) fn forget_spans() {
    let _ = LAST_SPANS.try_with(|last| last.borrow_mut().take());
}

#[cfg(test)]
pub(super) fn spans_kept() -> bool {
    LAST_SPANS.with(|last| last.borrow().is_some())
}

fn document_spans(source: &str) -> std::rc::Rc<DocumentSpans> {
    // In a batch of problems, read once without hashing the text each time.
    if crate::editor::paths::in_batch(source) {
        struct Read(std::rc::Rc<DocumentSpans>);
        return crate::editor::paths::batch_cached(source, || Read(remembered_spans(source))).0.clone();
    }
    remembered_spans(source)
}

/// The spans of `source`, remembered for the last text.
fn remembered_spans(source: &str) -> std::rc::Rc<DocumentSpans> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    let key = hasher.finish();
    if let Some(spans) = LAST_SPANS.with(|last| last.borrow().as_ref().filter(|(cached, _)| *cached == key).map(|(_, spans)| spans.clone())) {
        return spans;
    }
    let scan = scan_text(source, 0, true);
    let spans = std::rc::Rc::new((scan.key_spans, scan.value_spans));
    LAST_SPANS.with(|last| *last.borrow_mut() = Some((key, spans.clone())));
    spans
}

/// The lines of `text_bodies::text_body_lines` read off the full span scan,
/// which it must match.
#[cfg(test)]
pub(super) fn text_body_lines_from_spans(source: &str) -> Vec<std::ops::Range<usize>> {
    let line_of = |byte: usize| source[..byte].matches('\n').count();
    document_spans(source)
        .1
        .iter()
        .filter(|(_, start, end)| !source[*start..].starts_with(['"', '\'']) && source[*start..*end].contains('\n'))
        .map(|(_, start, end)| {
            let rest_of_line = source[*end..].split('\n').next().unwrap_or("");
            let last = line_of(*end) + usize::from(rest_of_line.trim().is_empty());
            line_of(*start) + 1..last
        })
        .filter(|lines| !lines.is_empty())
        .collect()
}

/// [`DocumentSpans`] by path (the first recorded at each), for a batch of
/// problems: each lookup is then no read of the list.
struct SpansByPath {
    names: std::collections::HashMap<Vec<String>, usize>,
    values: std::collections::HashMap<Vec<String>, usize>,
}

/// The spans recorded at `wanted` (names, or values): from the batch's
/// index of them while one runs, else read from the list. A lone element
/// read as a one-item array is addressed as `/dns/0` but recorded as
/// `/dns`: the `0` right after an element written once is left out too.
fn recorded(source: &str, names: bool, wanted: &[String]) -> Option<(usize, usize)> {
    let spans = document_spans(source);
    let index = crate::editor::paths::in_batch(source).then(|| {
        crate::editor::paths::batch_cached(source, || {
            let by_path = |list: &[(Vec<String>, usize, usize)]| {
                let mut found = std::collections::HashMap::with_capacity(list.len());
                for (at, (path, _, _)) in list.iter().enumerate() {
                    found.entry(path.clone()).or_insert(at);
                }
                found
            };
            SpansByPath { names: by_path(&spans.0), values: by_path(&spans.1) }
        })
    });
    let find = |names: bool, path: &[String]| -> Option<usize> {
        match &index {
            Some(index) => if names { &index.names } else { &index.values }.get(path).copied(),
            None => if names { &spans.0 } else { &spans.1 }.iter().position(|(segments, _, _)| segments == path),
        }
    };
    let list = if names { &spans.0 } else { &spans.1 };
    let lone = || lone_spelling(wanted, |path| lone_element(source, &spans.0, path, |path| find(true, path)));
    let found = find(names, wanted).or_else(|| find(names, &lone()?))?;
    let (_, start, end) = &list[found];
    Some((*start, *end))
}

/// `wanted` without the `0` after each element written once (the rest as
/// they are); `None` when it has none.
fn lone_spelling(wanted: &[String], is_lone: impl Fn(&[String]) -> bool) -> Option<Vec<String>> {
    let mut spelling: Vec<String> = Vec::with_capacity(wanted.len());
    let mut left_out = false;
    for segment in wanted {
        // An index on an element left out: an element is no list.
        if left_out && crate::editor::language::pointer_index(segment).is_some() {
            return None;
        }
        if segment == "0" && !spelling.is_empty() && is_lone(&spelling) {
            left_out = true;
            continue;
        }
        left_out = false;
        spelling.push(segment.clone());
    }
    (spelling.len() < wanted.len()).then_some(spelling)
}

/// Whether `path` is an element written once: its name recorded there (not
/// an attribute's, not an item of repeated ones), and no second one.
fn lone_element(source: &str, names: &[(Vec<String>, usize, usize)], path: &[String], find: impl Fn(&[String]) -> Option<usize>) -> bool {
    if path.last().is_none_or(|last| crate::editor::language::pointer_index(last).is_some()) {
        return false;
    }
    let element = find(path).is_some_and(|at| source[..names[at].1].ends_with('<'));
    element && {
        let mut second = path.to_vec();
        second.push("1".to_string());
        find(&second).is_none()
    }
}

/// Whether `path` names an element written once (see
/// `ConfigLanguage::lone_item`).
pub(super) fn xml_lone_element(source: &str, path: &str) -> bool {
    let spans = document_spans(source);
    let names = &spans.0;
    lone_element(source, names, &pointer_segments(path), |path| names.iter().position(|(segments, _, _)| segments == path))
}

pub(super) fn xml_name_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let (start, end) = recorded(source, true, &pointer_segments(path))?;
    Some(span_between(source, start, end))
}

pub(super) fn xml_value_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    // The text of an element with attributes (`#text`) is recorded as the
    // element's.
    let mut segments = pointer_segments(path);
    if segments.last().is_some_and(|last| last == "#text") {
        segments.pop();
    }
    let (start, end) = recorded(source, false, &segments)?;
    Some(span_between(source, start, end))
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

/// The JSON pointer of the first element holding text beside child
/// elements (mixed content), which the value model reads as a `#text`
/// entry no other language could tell from a key: such a document does
/// not convert. `None` when there is none, or when `source` does not parse.
/// Two elements (or attributes) of one parent with the same local name
/// under different prefixes (`<c:item>` and `<item>`): the document model
/// keys them by local name, so another language would merge them. Says
/// which, at the parent's pointer.
pub(super) fn namespace_clash(source: &str) -> Option<String> {
    build_value(source).ok()?;
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    // Each open element: its local name, and its children's and
    // attributes' qualified names by local name.
    let mut stack: Vec<(String, HashMap<String, String>)> = vec![(String::new(), HashMap::new())];
    let text = |name: &str| name.to_string();
    let markup = declarations::Markup::of(source);
    loop {
        let (element, empty) = match reader.read_event_into(&mut buf).ok()? {
            Event::Eof => return None,
            Event::Start(element) => (element.into_owned(), false),
            Event::Empty(element) => (element.into_owned(), true),
            Event::End(_) => {
                stack.pop();
                buf.clear();
                continue;
            }
            _ => {
                buf.clear();
                continue;
            }
        };
        let (local, qualified) = (text(element.local_name().as_ref()), text(element.name().as_ref()));
        let pointer: String = stack
            .iter()
            .skip(1)
            .map(|(name, _)| format!("/{}", name.replace('~', "~0").replace('/', "~1")))
            .collect();
        let siblings = &mut stack.last_mut()?.1;
        if let Some(other) = siblings.get(&local).filter(|other| **other != qualified) {
            let at = if pointer.is_empty() { "/".to_string() } else { pointer };
            return Some(format!(
                "<{other}> and <{qualified}> are in different namespaces but read as the same key {local}, which another language cannot tell apart (at {at})"
            ));
        }
        siblings.insert(local.clone(), qualified);
        let element_local = local.clone();
        // Its attributes, beside its children (markup aside).
        let mut attributes: HashMap<String, String> = HashMap::new();
        for attribute in element.attributes().flatten() {
            let qualified = text(attribute.key.as_ref());
            if markup.holds(&qualified) {
                continue;
            }
            let local = text(attribute.key.local_name().as_ref());
            if let Some(other) = attributes.get(&local).filter(|other| **other != qualified) {
                // At the element holding them.
                let element = element_local.replace('~', "~0").replace('/', "~1");
                return Some(format!(
                    "the attributes {other} and {qualified} are in different namespaces but read as the same key {local}, which another language cannot tell apart (at {pointer}/{element})"
                ));
            }
            attributes.insert(local, qualified);
        }
        if !empty {
            stack.push((local, attributes));
        }
        buf.clear();
    }
}

pub(super) fn mixed_content(source: &str) -> Option<String> {
    struct Open {
        /// Its name and how many same-named siblings came before it.
        step: Option<(String, usize)>,
        siblings: HashMap<String, usize>,
        text: bool,
        children: bool,
    }
    let value = build_value(source).ok()?;
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut top_level: HashMap<String, usize> = HashMap::new();
    let mut stack: Vec<Open> = Vec::new();
    let mut step = |stack: &mut Vec<Open>, element: &BytesStart<'_>| {
        let name = element.local_name().into_inner().to_string();
        let config_root = stack.is_empty()
            && element
                .try_get_attribute(CONFIG_ROOT_ATTRIBUTE)
                .ok()
                .flatten()
                .is_some_and(|attribute| attribute.value.as_ref() == "true");
        let siblings = match stack.last_mut() {
            Some(parent) => {
                parent.children = true;
                &mut parent.siblings
            }
            None => &mut top_level,
        };
        let seen = siblings.entry(name.clone()).or_insert(0);
        *seen += 1;
        (!config_root).then(|| (name, *seen - 1))
    };
    loop {
        match reader.read_event_into(&mut buf).ok()? {
            Event::Eof => return None,
            Event::Start(element) => {
                let step = step(&mut stack, &element);
                stack.push(Open { step, siblings: HashMap::new(), text: false, children: false });
            }
            Event::Empty(element) => {
                step(&mut stack, &element);
            }
            Event::Text(text) => {
                if let Some(open) = stack.last_mut()
                    && !text.xml10_content().trim().is_empty()
                {
                    open.text = true;
                }
            }
            Event::CData(_) | Event::GeneralRef(_) => {
                if let Some(open) = stack.last_mut() {
                    open.text = true;
                }
            }
            Event::End(_) => {
                if stack.last().is_some_and(|open| open.text && open.children) {
                    // Its path, an index where the name repeats (an array).
                    let mut pointer = String::new();
                    let mut at = &value;
                    for (name, index) in stack.iter().filter_map(|open| open.step.as_ref()) {
                        pointer.push('/');
                        pointer.push_str(&name.replace('~', "~0").replace('/', "~1"));
                        at = at.get(name)?;
                        if let Value::Array(items) = at {
                            pointer.push_str(&format!("/{index}"));
                            at = items.get(*index)?;
                        }
                    }
                    return Some(pointer);
                }
                stack.pop();
            }
            _ => {}
        }
        buf.clear();
    }
}
