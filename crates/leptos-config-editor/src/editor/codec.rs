//! Language-to-language conversion through the shared document model.
//! Language-free: a language only turns its own text into a `serde_json::Value`
//! ([`LanguageBehavior::parse`]) and a `Value` back into its own text
//! ([`Codec::serialize`]); it never knows which language is on the other side.
//! Converting XML to HCL is `xml.parse` then `hcl.serialize`.
//!
//! Serialization lives in its own trait (and its own registry,
//! `Language::codec`) rather than on `LanguageBehavior`, so an editor that
//! never converts does not reach any serializer through the behavior vtable.

use super::language::{LanguageBehavior, ParseError};
use serde_json::Value;
use std::fmt;

/// Renders the shared document model as text in one language.
pub(super) trait Codec: Send + Sync {
    /// Renders `value` as a complete document. `previous` is the text this
    /// language last held for the same document, if any: a language may keep
    /// its own document framing from it (for example XML's config root and
    /// declaration), never anything that depends on another language.
    fn serialize(&self, value: &Value, previous: Option<&str>) -> Result<String, String>;
}

/// The result of a conversion: the converted text, plus every JSON pointer
/// whose value the target language could not carry exactly (for example a
/// one-item array in XML reads back as a scalar). Empty `losses` means the
/// conversion round-trips.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conversion {
    pub text: String,
    pub losses: Vec<String>,
}

/// Why a conversion failed.
#[derive(Clone, Debug, PartialEq, Eq)]
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

/// One side of a conversion: how to read and how to write a language.
pub(super) struct Side<'a> {
    pub(super) behavior: &'a dyn LanguageBehavior,
    pub(super) codec: &'a dyn Codec,
}

/// Renders an already parsed document in `target`, then parses the result
/// back to list what the target could not carry.
pub(super) fn render(
    value: &Value,
    target: &Side<'_>,
    previous: Option<&str>,
) -> Result<Conversion, ConvertError> {
    let text = serialize(target.codec, value, previous)?;
    let reread = target.behavior.parse(&text).map_err(|error| {
        ConvertError::Unsupported(format!(
            "the converted document does not read back: {}",
            error.message
        ))
    })?;
    let mut losses = Vec::new();
    differences(value, &reread, String::new(), &mut losses);
    Ok(Conversion { text, losses })
}

/// Serializes through `codec`, ending the document with exactly one newline
/// whatever the underlying library writes.
pub(super) fn serialize(
    codec: &dyn Codec,
    value: &Value,
    previous: Option<&str>,
) -> Result<String, ConvertError> {
    let mut text = codec
        .serialize(value, previous)
        .map_err(ConvertError::Unsupported)?;
    text.truncate(text.trim_end_matches('\n').len());
    text.push('\n');
    Ok(text)
}

/// Parses `source` with `from` and renders it with `to`.
pub(super) fn convert(
    source: &str,
    from: &dyn LanguageBehavior,
    to: &Side<'_>,
    previous: Option<&str>,
) -> Result<Conversion, ConvertError> {
    let value = from.parse(source).map_err(ConvertError::Parse)?;
    render(&value, to, previous)
}

/// JSON pointers where `expected` and `actual` differ, at the deepest level
/// that still has the same shape.
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

/// The JSON pointer of the first value (depth-first, document order) that
/// matches `predicate`; lets a language name exactly what it cannot express.
pub(super) fn first_pointer(value: &Value, predicate: &dyn Fn(&Value) -> bool) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::super::Language;
    use super::{ConvertError, convert, differences};
    use serde_json::json;

    const ALL: [Language; 5] = [
        Language::Yaml,
        Language::Json,
        Language::Toml,
        Language::Hcl,
        Language::Xml,
    ];

    /// A document every language can carry exactly (arrays hold two or more
    /// items for XML; no nulls for TOML; keys are identifiers).
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
        for from in ALL {
            let source = from.serialize(&value).expect("serializes");
            assert_eq!(from.parse(&source).expect("reads back"), value, "{from:?}");
            for to in ALL {
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
        let converted = Language::Json
            .convert(r#"{"zeta": 1, "alpha": 2, "mid": 3}"#, Language::Yaml)
            .expect("converts");
        assert_eq!(converted.text, "zeta: 1\nalpha: 2\nmid: 3\n");
    }

    #[test]
    fn a_source_syntax_error_is_reported_with_its_position() {
        let Err(ConvertError::Parse(error)) =
            Language::Json.convert("{\n  \"a\": ,\n}", Language::Yaml)
        else {
            panic!("a parse error");
        };
        assert_eq!(error.line, Some(2));
    }

    #[test]
    fn toml_names_the_null_it_cannot_express() {
        let Err(ConvertError::Unsupported(message)) =
            Language::Json.convert(r#"{"web": {"https": null}}"#, Language::Toml)
        else {
            panic!("TOML rejects null");
        };
        assert!(message.contains("/web/https"), "{message}");
    }

    #[test]
    fn xml_reports_a_one_item_array_it_reads_back_as_a_scalar() {
        let converted = Language::Json
            .convert(r#"{"hosts": ["only.example"], "port": 80}"#, Language::Xml)
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
            super::render(&value, &Language::Xml.side(), Some(previous)).expect("renders");
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
            Language::Yaml.behavior(),
            &Language::Json.side(),
            Some(previous),
        )
        .expect("converts");
        assert_eq!(converted.text, "{\n    \"b\": 2\n}\n");
    }

    #[test]
    fn hcl_writes_blocks_for_identifier_objects_and_quotes_other_keys() {
        let text = Language::Hcl
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
        let text = Language::Hcl.serialize(&value).expect("serializes");
        assert_eq!(Language::Hcl.parse(&text).expect("reads back"), value);
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
