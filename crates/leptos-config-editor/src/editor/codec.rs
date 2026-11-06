//! Conversion between languages through the shared data model: XML to HCL is
//! `xml.parse` then `hcl.serialize`. A language implements only its own side
//! ([`ConfigLanguage::parse`] and [`ConfigLanguage::serialize`]).

use super::completion::conform_arrays;
use super::language::{ConfigLanguage, ParseError, Span};
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
    if let Some(schema) = schema.filter(|_| language.repeats_arrays()) {
        conform_arrays(&mut value, schema);
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
        let span = language.value_span(previous, &pointer)?;
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
