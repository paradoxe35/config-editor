//! Conversion between languages through the shared data model: XML to HCL is
//! `xml.parse` then `hcl.serialize`. A language implements only its own side
//! ([`ConfigLanguage::parse`] and [`ConfigLanguage::serialize`]).

use super::completion::conform_arrays;
use super::language::{ConfigLanguage, ParseError, Repetition, Span};
use serde_json::Value;
use std::fmt;

/// A converted document, and the values the target language could not carry
/// exactly. Empty `losses` means the conversion round-trips.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conversion {
    /// The document in the target language.
    pub text: String,
    /// JSON pointers of values the target reads back differently (for example
    /// an empty array, which XML cannot write).
    pub losses: Vec<String>,
}

/// Why a conversion failed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConvertError {
    /// The source text does not parse in its language.
    Parse(ParseError),
    /// The target language cannot express the document (for example TOML has
    /// no `null`, and XML element names cannot hold `/docs`).
    Unsupported(String),
}

impl fmt::Display for ConvertError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConvertError::Parse(error) => write!(formatter, "{error}"),
            ConvertError::Unsupported(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for ConvertError {}

/// Parses `source`, reading lone values as the arrays `schema` expects in a
/// language that repeats keys for arrays.
pub(super) fn read(
    language: &dyn ConfigLanguage,
    source: &str,
    schema: Option<&Value>,
) -> Result<Value, ParseError> {
    let mut value = language.parse(source)?;
    if let Some(schema) = schema.filter(|_| language.repetition() != Repetition::None) {
        conform_arrays(&mut value, schema, language.repetition());
    }
    Ok(value)
}

/// Renders `value` in `target`, reading the result back to find the losses.
pub(super) fn render(
    value: &Value,
    target: &dyn ConfigLanguage,
    previous: Option<&str>,
    schema: Option<&Value>,
) -> Result<Conversion, ConvertError> {
    let text = serialize(target, value, previous)?;
    let reread = read(target, &text, schema).map_err(|error| {
        ConvertError::Unsupported(format!(
            "the converted document does not read back: {}",
            error.message
        ))
    })?;
    let mut losses = Vec::new();
    differences(value, &reread, String::new(), &mut losses);
    Ok(Conversion { text, losses })
}

/// Serializes through `language`, ending with exactly one newline.
pub(super) fn serialize(
    language: &dyn ConfigLanguage,
    value: &Value,
    previous: Option<&str>,
) -> Result<String, ConvertError> {
    let mut text = language
        .serialize(value, previous)
        .map_err(ConvertError::Unsupported)?;
    text.truncate(text.trim_end_matches('\n').len());
    text.push('\n');
    Ok(text)
}

pub(super) fn convert(
    source: &str,
    from: &dyn ConfigLanguage,
    to: &dyn ConfigLanguage,
    previous: Option<&str>,
    schema: Option<&Value>,
) -> Result<Conversion, ConvertError> {
    let value = read(from, source, schema).map_err(ConvertError::Parse)?;
    render(&value, to, previous, schema)
}

/// `previous` with only its changed scalar values rewritten in place, when
/// `old` and `new` have the same shape (same keys, same array lengths) and
/// every changed value can be located: comments, formatting, and key order
/// survive a value edit. `None` falls back to serializing the whole document.
pub(super) fn patch_scalars(
    language: &dyn ConfigLanguage,
    previous: &str,
    old: &Value,
    new: &Value,
    schema: Option<&Value>,
) -> Option<String> {
    // Whatever the language cannot express must still be refused, even when
    // a patch would read back equal (TOML writes null as `nan`).
    language.serialize(new, Some(previous)).ok()?;
    let mut changes = Vec::new();
    scalar_changes(old, new, String::new(), &mut changes)?;
    if changes.is_empty() {
        return Some(previous.to_string());
    }
    let mut edits = Vec::with_capacity(changes.len());
    for (pointer, value) in changes {
        // Values inside flow collections and inline tables are found by the
        // shared scanner when the language has no span for them.
        let span = language
            .value_span(previous, &pointer)
            .or_else(|| inline_location(language, previous, &pointer, false))?;
        edits.push((byte_range(previous, span)?, language.format_value(value)));
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut patched = previous.to_string();
    for (range, text) in edits {
        patched.replace_range(range, &text);
    }
    // Trust the patch only if it reads back as exactly the new document.
    (read(language, &patched, schema).ok()? == *new).then_some(patched)
}

/// `previous` with a structural edit (keys or items added or removed) applied
/// line by line, keeping its comments and formatting elsewhere: the host
/// serializer renders the old and the new value, their line diff is mapped
/// onto `previous`, and the result is kept only if it reads back as `new`.
pub(super) fn patch_structure(
    language: &dyn ConfigLanguage,
    previous: &str,
    old: &Value,
    new: &Value,
    schema: Option<&Value>,
) -> Option<String> {
    use super::diff::{DiffEdit, myers_edits};
    let before = serialize(language, old, Some(previous)).ok()?;
    let after = serialize(language, new, Some(previous)).ok()?;
    let rendered_old: Vec<&str> = before.lines().collect();
    let rendered_new: Vec<&str> = after.lines().collect();
    let host: Vec<&str> = previous.lines().collect();
    // Where each line of the old rendering sits in the host's text: a
    // forward match that skips the host's comments and blank lines, and
    // compares lines without quotes or trailing comments.
    let normalized_host: Vec<String> = host.iter().map(|line| comparable(line)).collect();
    let mut place = vec![None; rendered_old.len()];
    let mut next = 0;
    for (index, line) in rendered_old.iter().enumerate() {
        let wanted = comparable(line);
        if wanted.is_empty() {
            continue;
        }
        // Look ahead a bounded distance so a missing line cannot make the
        // rest of the document unmatched.
        let window = (next + 400).min(host.len());
        if let Some(found) = (next..window).find(|candidate| normalized_host[*candidate] == wanted)
        {
            place[index] = Some(found);
            next = found + 1;
        }
    }
    // The old → new edits, applied to the host's lines.
    let mut removed = vec![false; host.len()];
    let mut inserted: Vec<Vec<&str>> = vec![Vec::new(); host.len() + 1];
    let (mut old_index, mut new_index) = (0, 0);
    for edit in myers_edits(&rendered_old, &rendered_new)? {
        match edit {
            DiffEdit::Equal => {
                old_index += 1;
                new_index += 1;
            }
            DiffEdit::Delete => {
                removed[place[old_index]?] = true;
                old_index += 1;
            }
            DiffEdit::Insert => {
                // After the host line of the previous rendered line.
                let slot = match old_index {
                    0 => 0,
                    index => place[index - 1]? + 1,
                };
                inserted[slot].push(rendered_new[new_index]);
                new_index += 1;
            }
        }
    }
    let mut inserted: Vec<Vec<String>> = inserted
        .into_iter()
        .map(|slot| slot.into_iter().map(str::to_string).collect())
        .collect();
    // A replaced line (a renamed key) keeps its trailing comment: the
    // comment moves to the line inserted in its place at the same indent.
    for index in 0..host.len() {
        if !removed[index] {
            continue;
        }
        let comment = trailing_comment(host[index]);
        let indent = |line: &str| line.len() - line.trim_start().len();
        if let Some(first) = inserted[index + 1].first_mut()
            && !comment.is_empty()
            && indent(first) == indent(host[index])
            && trailing_comment(first).is_empty()
        {
            first.push(' ');
            first.push_str(comment);
        }
    }
    let mut lines: Vec<&str> = Vec::with_capacity(host.len() + rendered_new.len());
    for index in 0..=host.len() {
        lines.extend(inserted[index].iter().map(String::as_str));
        if index < host.len() && !removed[index] {
            lines.push(host[index]);
        }
    }
    let mut patched = lines.join("\n");
    if previous.ends_with('\n') {
        patched.push('\n');
    }
    (read(language, &patched, schema).ok()? == *new).then_some(patched)
}

/// A line as the structural patch compares it: trimmed, without quotes and
/// without a trailing `#`, `//`, or `<!-- -->` comment.
fn comparable(line: &str) -> String {
    line[..comment_start(line)].trim().replace(['"', '\''], "")
}

/// The trailing `#`, `//`, or `<!-- -->` comment of `line`, or `""`.
fn trailing_comment(line: &str) -> &str {
    line[comment_start(line)..].trim_end()
}

/// Where the trailing comment of `line` starts, or its length.
fn comment_start(line: &str) -> usize {
    let mut code = line.len();
    let mut quote: Option<char> = None;
    let mut previous = ' ';
    for (index, character) in line.char_indices() {
        match quote {
            Some(open) if character == open && previous != '\\' => quote = None,
            Some(_) => {}
            None if character == '"' || character == '\'' => quote = Some(character),
            None if (character == '#'
                || line[index..].starts_with("//")
                || line[index..].starts_with("<!--"))
                && (index == 0 || previous.is_whitespace() || previous == '>') =>
            {
                code = index;
                break;
            }
            None => {}
        }
        previous = character;
    }
    code
}

/// The span of what `pointer` names when it lies inside an inline container
/// (a flow collection, inline table, or one-line object): the deepest
/// ancestor whose value is `{…}` or `[…]` is located by the language, the
/// rest by the shared scanner. `key` asks for the key rather than the value.
pub(super) fn inline_location(
    behavior: &dyn ConfigLanguage,
    source: &str,
    pointer: &str,
    key: bool,
) -> Option<Span> {
    use super::language::{SchemaPathSegment, pointer_segments};
    let segments = pointer_segments(pointer);
    for depth in (1..segments.len()).rev() {
        let ancestor: String = segments[..depth]
            .iter()
            .map(|segment| format!("/{}", segment.replace('~', "~0").replace('/', "~1")))
            .collect();
        let Some(span) = behavior.value_span(source, &ancestor) else {
            continue;
        };
        let line_start: usize = source
            .split_inclusive('\n')
            .take(span.line.saturating_sub(1))
            .map(str::len)
            .sum();
        let start = line_start
            + source[line_start..]
                .char_indices()
                .nth(span.start_column.saturating_sub(1))
                .map_or(0, |(index, _)| index);
        let text = &source[start..];
        if !text.starts_with(['{', '[']) {
            return None;
        }
        let rest: Vec<SchemaPathSegment> = segments[depth..]
            .iter()
            .map(|segment| match segment.parse::<usize>() {
                Ok(index) => SchemaPathSegment::ArrayItem(index),
                Err(_) => SchemaPathSegment::Key(segment.clone()),
            })
            .collect();
        let (from, to) = super::inline::inline_path_span(text, &rest, key)?;
        let (from, to) = (start + from, start + to);
        let line = source[..from].matches('\n').count() + 1;
        let from_line = source[..from].rfind('\n').map_or(0, |index| index + 1);
        let column = source[from_line..from].chars().count() + 1;
        let to_column = if source[from..to].contains('\n') {
            column + 1
        } else {
            column + source[from..to].chars().count()
        };
        return Some(Span {
            line,
            start_column: column,
            end_column: to_column.max(column + 1),
        });
    }
    None
}

/// Collects `(pointer, new value)` for every changed scalar; `None` when the
/// shape changed (keys added or removed, array lengths, types).
fn scalar_changes<'a>(
    old: &Value,
    new: &'a Value,
    pointer: String,
    out: &mut Vec<(String, &'a Value)>,
) -> Option<()> {
    match (old, new) {
        (Value::Object(left), Value::Object(right)) => {
            // Only values are rewritten in place, so key order does not matter.
            if left.len() != right.len() || !right.keys().all(|key| left.contains_key(key)) {
                return None;
            }
            for (key, value) in right {
                scalar_changes(&left[key], value, format!("{pointer}/{}", escape(key)), out)?;
            }
            Some(())
        }
        (Value::Array(left), Value::Array(right)) => {
            if left.len() != right.len() {
                return None;
            }
            for (index, (before, after)) in left.iter().zip(right).enumerate() {
                scalar_changes(before, after, format!("{pointer}/{index}"), out)?;
            }
            Some(())
        }
        (Value::Object(_) | Value::Array(_), _) | (_, Value::Object(_) | Value::Array(_)) => None,
        _ => {
            if old != new {
                out.push((pointer, new));
            }
            Some(())
        }
    }
}

/// The byte range of a one-based line/character-column span.
fn byte_range(source: &str, span: Span) -> Option<std::ops::Range<usize>> {
    let line_start = if span.line <= 1 {
        0
    } else {
        source.match_indices('\n').nth(span.line - 2)?.0 + 1
    };
    let line = &source[line_start..];
    let byte_of = |column: usize| {
        line.char_indices()
            .nth(column.saturating_sub(1))
            .map_or(line.len(), |(index, _)| index)
    };
    let (start, end) = (byte_of(span.start_column), byte_of(span.end_column));
    (start < end).then_some(line_start + start..line_start + end)
}

/// JSON pointers where `expected` and `actual` differ, at the deepest level
/// where both still have the same shape.
fn differences(expected: &Value, actual: &Value, pointer: String, out: &mut Vec<String>) {
    match (expected, actual) {
        (Value::Object(left), Value::Object(right)) => {
            for (key, value) in left {
                let child = format!("{pointer}/{}", escape(key));
                match right.get(key) {
                    Some(other) => differences(value, other, child, out),
                    None => out.push(child),
                }
            }
            for key in right.keys().filter(|key| !left.contains_key(*key)) {
                out.push(format!("{pointer}/{}", escape(key)));
            }
        }
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => {
            for (index, (value, other)) in left.iter().zip(right).enumerate() {
                differences(value, other, format!("{pointer}/{index}"), out);
            }
        }
        _ if expected == actual => {}
        _ => out.push(if pointer.is_empty() {
            "/".to_string()
        } else {
            pointer
        }),
    }
}

/// The JSON pointer of the first value, depth-first, matching `predicate`;
/// lets a codec name exactly what it cannot express.
pub fn first_pointer(value: &Value, predicate: &dyn Fn(&Value) -> bool) -> Option<String> {
    if predicate(value) {
        return Some(String::new());
    }
    match value {
        Value::Object(map) => map.iter().find_map(|(key, child)| {
            first_pointer(child, predicate).map(|rest| format!("/{}{rest}", escape(key)))
        }),
        Value::Array(items) => items.iter().enumerate().find_map(|(index, child)| {
            first_pointer(child, predicate).map(|rest| format!("/{index}{rest}"))
        }),
        _ => None,
    }
}

fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

// Conversion tests pair languages, so they need every built-in one.
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod tests {
    use super::super::Language;
    use super::{ConvertError, convert, differences};
    use serde_json::json;

    /// Carried exactly by every language: arrays of two or more items (XML),
    /// no nulls (TOML), identifier keys.
    fn portable() -> serde_json::Value {
        json!({
            "listen": "0.0.0.0:7835",
            "insecure_plaintext": false,
            "pool": {"min_idle": 4, "max_idle": 64},
            "web": {"hosts": ["a.example", "b.example"], "letsencrypt": {"email": "ops@example.com"}},
            "clients": {"acme-prod": {"secret": "«secret 1»"}, "beta": {"secret": ["x", "y"]}},
            "reach": {}
        })
    }

    #[test]
    fn a_portable_document_round_trips_between_every_pair_of_languages() {
        let value = portable();
        for &from in Language::ALL {
            let source = from.serialize(&value).expect("serializes");
            assert_eq!(from.parse(&source).expect("reads back"), value, "{from:?}");
            for &to in Language::ALL {
                let converted = from.convert(&source, to).expect("converts");
                assert!(
                    converted.losses.is_empty(),
                    "{from:?} -> {to:?}: {:?}",
                    converted.losses
                );
                assert_eq!(
                    to.parse(&converted.text).expect("parses"),
                    value,
                    "{from:?} -> {to:?}"
                );
            }
        }
    }

    #[test]
    fn conversion_keeps_the_key_order_of_the_source() {
        let converted = Language::JSON
            .convert(r#"{"zeta": 1, "alpha": 2, "mid": 3}"#, Language::YAML)
            .expect("converts");
        assert_eq!(converted.text, "zeta: 1\nalpha: 2\nmid: 3\n");
    }

    #[test]
    fn a_source_syntax_error_is_reported_with_its_position() {
        let Err(ConvertError::Parse(error)) =
            Language::JSON.convert("{\n  \"a\": ,\n}", Language::YAML)
        else {
            panic!("a parse error");
        };
        assert_eq!(error.line, Some(2));
    }

    #[test]
    fn toml_names_the_null_it_cannot_express() {
        let Err(ConvertError::Unsupported(message)) =
            Language::JSON.convert(r#"{"web": {"https": null}}"#, Language::TOML)
        else {
            panic!("TOML rejects null");
        };
        assert!(message.contains("/web/https"), "{message}");
    }

    #[test]
    fn xml_reports_a_one_item_array_it_reads_back_as_a_scalar() {
        let converted = Language::JSON
            .convert(r#"{"hosts": ["only.example"], "port": 80}"#, Language::XML)
            .expect("converts");
        assert_eq!(
            converted.text,
            "<hosts>only.example</hosts>\n<port>80</port>\n"
        );
        assert_eq!(converted.losses, vec!["/hosts"]);
    }

    #[test]
    fn xml_keeps_its_config_root_and_declaration_from_the_previous_text() {
        let previous = "<?xml version=\"1.0\"?>\n<burrow config-root=\"true\">\n  <listen>1</listen>\n</burrow>\n";
        let value = json!({"listen": 2, "pool": {"min_idle": 4}});
        let converted =
            super::render(&value, Language::XML.behavior(), Some(previous), None).expect("renders");
        assert_eq!(
            converted.text,
            "<?xml version=\"1.0\"?>\n<burrow config-root=\"true\">\n  <listen>2</listen>\n  <pool>\n    <min_idle>4</min_idle>\n  </pool>\n</burrow>\n"
        );
        assert!(converted.losses.is_empty());
    }

    #[test]
    fn json_keeps_the_indentation_of_the_previous_text() {
        let previous = "{\n    \"a\": 1\n}\n";
        let converted = convert(
            "b: 2\n",
            Language::YAML.behavior(),
            Language::JSON.behavior(),
            Some(previous),
            None,
        )
        .expect("converts");
        assert_eq!(converted.text, "{\n    \"b\": 2\n}\n");
    }

    #[test]
    fn hcl_writes_blocks_for_identifier_objects_and_quotes_other_keys() {
        let text = Language::HCL
            .serialize(&json!({"web": {"http": 80}, "names": {"old.example": "new.example"}}))
            .expect("serializes");
        assert_eq!(
            text,
            "web {\n  http = 80\n}\n\nnames = {\n  \"old.example\" = \"new.example\"\n}\n"
        );
    }

    #[test]
    fn hcl_escapes_template_markers_in_strings() {
        let value = json!({"token": "${CLOUDFLARE_API_TOKEN}"});
        let text = Language::HCL.serialize(&value).expect("serializes");
        assert_eq!(Language::HCL.parse(&text).expect("reads back"), value);
    }

    #[test]
    fn differences_point_at_the_deepest_changed_value() {
        let mut out = Vec::new();
        differences(
            &json!({"a": {"b": [1, 2], "c": "x"}, "d/e": 1}),
            &json!({"a": {"b": 1, "c": "x"}, "d/e": 2, "f": 0}),
            String::new(),
            &mut out,
        );
        assert_eq!(out, vec!["/a/b", "/d~1e", "/f"]);
    }

    #[test]
    fn identical_documents_have_no_differences() {
        let mut out = Vec::new();
        let value = json!({"a": [1, {"b": null}]});
        differences(&value, &value, String::new(), &mut out);
        assert!(out.is_empty());
    }
}
