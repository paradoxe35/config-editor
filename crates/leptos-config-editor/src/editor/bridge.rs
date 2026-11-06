//! Editing a document in one language while the host keeps it in another
//! (`<ConfigEditor language=Language::XML edit_as=Language::HCL>`).
//!
//! The host's text (`value`, `baseline`, `initial_value`) is shown converted;
//! edits convert back before `on_change`, keeping the host's framing. Both
//! directions go through `codec`, so no language needs bridge support.

use super::Language;
use super::codec::{self, Conversion, ConvertError};
use super::diagnostics::{Diagnostic, SchemaBundle, Severity};
use super::language::ConfigLanguage;
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct Bridge {
    pub(super) host: Language,
    pub(super) edit: Language,
    /// Reads lone values as arrays where the schema expects them (XML).
    pub(super) schema: Option<Arc<SchemaBundle>>,
}

impl Bridge {
    /// `None` when both languages are the same.
    pub(super) fn new(
        host: Language,
        edit: Language,
        schema: Option<Arc<SchemaBundle>>,
    ) -> Option<Self> {
        (host != edit).then_some(Bridge { host, edit, schema })
    }

    fn shape(&self) -> Option<&serde_json::Value> {
        self.schema.as_deref().map(|bundle| &bundle.schema)
    }

    pub(super) fn to_edit(&self, host_text: &str) -> Result<Conversion, ConvertError> {
        if host_text.trim().is_empty() {
            return Ok(Conversion {
                text: String::new(),
                losses: Vec::new(),
            });
        }
        codec::convert(
            host_text,
            self.host.behavior(),
            self.edit.behavior(),
            None,
            self.shape(),
        )
    }

    /// `previous_host` is the host's last text, whose framing is kept.
    pub(super) fn to_host(
        &self,
        edit_text: &str,
        previous_host: &str,
    ) -> Result<Conversion, ConvertError> {
        if edit_text.trim().is_empty() {
            return Ok(Conversion {
                text: String::new(),
                losses: Vec::new(),
            });
        }
        let edited = codec::read(self.edit.behavior(), edit_text, self.shape())
            .map_err(ConvertError::Parse)?;
        let host = self.host.behavior();
        let Ok(old) = codec::read(host, previous_host, self.shape()) else {
            return codec::render(&edited, host, Some(previous_host), self.shape());
        };
        // Only what the user changed is saved: the edit is compared with the
        // host value as the edit language showed it, so values that language
        // cannot tell apart (XML reads `null`, `""` and `"1"` as text) keep
        // the host's own.
        let seen = self
            .to_edit(previous_host)
            .ok()
            .and_then(|shown| codec::read(self.edit.behavior(), &shown.text, self.shape()).ok());
        let value = match &seen {
            Some(seen) => merge_changes(&old, seen, &edited),
            None => edited,
        };
        let rendered = codec::render(&value, host, Some(previous_host), self.shape())?;
        // The host's text is patched to read back exactly as a full render
        // would (losses included), so its comments and formatting survive: a
        // value edit rewrites just those values, a structural one just those
        // lines.
        let Ok(target) = codec::read(host, &rendered.text, self.shape()) else {
            return Ok(rendered);
        };
        let patched = codec::patch_scalars(host, previous_host, &old, &target, self.shape())
            .or_else(|| codec::patch_structure(host, previous_host, &old, &target, self.shape()));
        Ok(match patched {
            Some(text) => Conversion {
                text,
                losses: rendered.losses,
            },
            None => rendered,
        })
    }

    /// A warning on each value the host cannot carry exactly, or an error when
    /// the document cannot be saved. Syntax errors are left to validation.
    pub(super) fn problems(
        &self,
        edit_text: &str,
        result: &Result<Conversion, ConvertError>,
    ) -> Vec<Diagnostic> {
        let host = self.host.label();
        match result {
            Ok(conversion) => conversion
                .losses
                .iter()
                .map(|pointer| {
                    located(
                        self.edit.behavior(),
                        edit_text,
                        pointer,
                        format!("{pointer}: {host} cannot keep this value exactly; it reads back differently"),
                        Severity::Warning,
                    )
                })
                .collect(),
            Err(ConvertError::Parse(_)) => Vec::new(),
            Err(error @ ConvertError::Unsupported(message)) => vec![locate_error(
                self.edit.behavior(),
                edit_text,
                error,
                format!("Not saved as {host}: {message}"),
            )],
        }
    }

    /// The error shown when the host's text cannot be converted for editing.
    pub(super) fn load_problem(&self, error: &ConvertError) -> Diagnostic {
        Diagnostic {
            line: Some(1),
            column: Some(1),
            end_column: Some(2),
            message: format!(
                "Cannot edit this {} as {}: {error}",
                self.host.label(),
                self.edit.label()
            ),
            severity: Severity::Error,
        }
    }
}

/// The host value with the user's edits applied: where the edit equals what
/// the edit language showed, the host's original value is kept; elsewhere
/// the edit wins. Keys and items added or removed follow the edit.
fn merge_changes(
    host: &serde_json::Value,
    seen: &serde_json::Value,
    edited: &serde_json::Value,
) -> serde_json::Value {
    use serde_json::Value;
    if seen == edited {
        return host.clone();
    }
    match (host, seen, edited) {
        (Value::Object(host), Value::Object(seen), Value::Object(edited)) => {
            let mut merged = serde_json::Map::new();
            for (key, value) in edited {
                let next = match (host.get(key), seen.get(key)) {
                    (Some(host_value), Some(seen_value)) => {
                        merge_changes(host_value, seen_value, value)
                    }
                    _ => value.clone(),
                };
                merged.insert(key.clone(), next);
            }
            // Keys the edit language could not show stay.
            for (key, value) in host {
                if !seen.contains_key(key) && !edited.contains_key(key) {
                    merged.insert(key.clone(), value.clone());
                }
            }
            Value::Object(merged)
        }
        (Value::Array(host), Value::Array(seen), Value::Array(edited))
            if host.len() == seen.len() && seen.len() == edited.len() =>
        {
            Value::Array(
                host.iter()
                    .zip(seen)
                    .zip(edited)
                    .map(|((host, seen), edited)| merge_changes(host, seen, edited))
                    .collect(),
            )
        }
        _ => edited.clone(),
    }
}

/// A diagnostic for a conversion error, on the value it names
/// (`… (at /pointer)`) when the message carries one.
pub(super) fn locate_error(
    behavior: &dyn ConfigLanguage,
    source: &str,
    error: &ConvertError,
    message: String,
) -> Diagnostic {
    let text = error.to_string();
    let pointer = text
        .rsplit_once("(at ")
        .and_then(|(_, rest)| rest.strip_suffix(')'))
        .unwrap_or("");
    located(behavior, source, pointer, message, Severity::Error)
}

/// A diagnostic on the value at `pointer` in `source`, or on the document's
/// first content when the value has no span.
fn located(
    behavior: &dyn ConfigLanguage,
    source: &str,
    pointer: &str,
    message: String,
    severity: Severity,
) -> Diagnostic {
    let span = behavior
        .value_span(source, pointer)
        .or_else(|| behavior.key_span(source, pointer))
        .or_else(|| behavior.first_span(source));
    Diagnostic {
        line: span.map(|span| span.line),
        column: span.map(|span| span.start_column),
        end_column: span.map(|span| span.end_column),
        message,
        severity,
    }
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
    use super::super::diagnostics::Severity;
    use super::Bridge;

    #[test]
    fn the_same_language_needs_no_bridge() {
        assert!(Bridge::new(Language::YAML, Language::YAML, None).is_none());
    }

    #[test]
    fn host_text_is_shown_converted_and_edits_convert_back_keeping_framing() {
        let bridge = Bridge::new(Language::XML, Language::HCL, None).unwrap();
        let host = "<?xml version=\"1.0\"?>\n<burrow config-root=\"true\">\n  <listen>7835</listen>\n</burrow>\n";
        let shown = bridge.to_edit(host).unwrap();
        assert_eq!(shown.text, "listen = 7835\n");
        let saved = bridge.to_host("listen = 9000\n", host).unwrap();
        assert_eq!(
            saved.text,
            "<?xml version=\"1.0\"?>\n<burrow config-root=\"true\">\n  <listen>9000</listen>\n</burrow>\n"
        );
    }

    #[test]
    fn a_value_the_host_cannot_carry_is_a_warning_on_that_value() {
        let bridge = Bridge::new(Language::XML, Language::YAML, None).unwrap();
        let edit = "hosts:\n  - only.example\nport: 80\n";
        let result = bridge.to_host(edit, "");
        let problems = bridge.problems(edit, &result);
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].severity, Severity::Warning);
        assert_eq!(problems[0].line, Some(1));
        assert!(
            problems[0].message.contains("XML cannot keep"),
            "{}",
            problems[0].message
        );
    }

    #[test]
    fn a_document_the_host_cannot_express_is_an_error_and_not_saved() {
        let bridge = Bridge::new(Language::TOML, Language::JSON, None).unwrap();
        let edit = "{\n  \"https\": null\n}\n";
        let result = bridge.to_host(edit, "");
        assert!(result.is_err());
        let problems = bridge.problems(edit, &result);
        assert_eq!(problems[0].severity, Severity::Error);
        assert!(
            problems[0].message.starts_with("Not saved as TOML"),
            "{}",
            problems[0].message
        );
    }

    #[test]
    fn syntax_errors_are_left_to_validation() {
        let bridge = Bridge::new(Language::JSON, Language::YAML, None).unwrap();
        let edit = "a: [\n";
        let result = bridge.to_host(edit, "");
        assert!(bridge.problems(edit, &result).is_empty());
    }

    /// Host documents with comments and hand formatting, the port to change,
    /// and the same document with only the port changed.
    const HOSTS: [(Language, &str, &str); 5] = [
        (
            Language::YAML,
            "# Burrow\nlisten: 7835 # the port\npublic_address: a.example\npool: {min_idle: 4, max_idle: 64}\n",
            "# Burrow\nlisten: 9000 # the port\npublic_address: a.example\npool: {min_idle: 4, max_idle: 64}\n",
        ),
        (
            Language::TOML,
            "# Burrow\nlisten = 7835 # the port\npublic_address = \"a.example\"\npool = { min_idle = 4, max_idle = 64 }\n",
            "# Burrow\nlisten = 9000 # the port\npublic_address = \"a.example\"\npool = { min_idle = 4, max_idle = 64 }\n",
        ),
        (
            Language::HCL,
            "# Burrow\nlisten = 7835 // the port\npublic_address = \"a.example\"\npool = { min_idle = 4, max_idle = 64 }\n",
            "# Burrow\nlisten = 9000 // the port\npublic_address = \"a.example\"\npool = { min_idle = 4, max_idle = 64 }\n",
        ),
        (
            Language::XML,
            "<!-- Burrow -->\n<listen>7835</listen> <!-- the port -->\n<public_address>a.example</public_address>\n",
            "<!-- Burrow -->\n<listen>9000</listen> <!-- the port -->\n<public_address>a.example</public_address>\n",
        ),
        (
            Language::JSON,
            "{ \"public_address\": \"a.example\", \"listen\": 7835,\n  \"pool\": {\"min_idle\": 4} }\n",
            "{ \"public_address\": \"a.example\", \"listen\": 9000,\n  \"pool\": {\"min_idle\": 4} }\n",
        ),
    ];

    #[test]
    fn a_value_edit_changes_only_that_value_in_the_host_text() {
        for (host, original, expected) in HOSTS {
            for &edit in Language::ALL.iter().filter(|edit| **edit != host) {
                let bridge = Bridge::new(host, edit, None).unwrap();
                let shown = bridge.to_edit(original).unwrap().text;
                let Some(at) = shown.find("7835") else {
                    panic!("{host} -> {edit}: port not shown in {shown}");
                };
                let edited = format!("{}9000{}", &shown[..at], &shown[at + 4..]);
                let saved = bridge.to_host(&edited, original).unwrap();
                assert_eq!(saved.text, expected, "{host} edited as {edit}");
                assert!(saved.losses.is_empty());
            }
        }
    }

    #[test]
    fn a_structural_edit_adds_only_its_line_and_keeps_the_comment() {
        let bridge = Bridge::new(Language::YAML, Language::JSON, None).unwrap();
        let saved = bridge
            .to_host(
                "{\"listen\": 1, \"public_address\": \"b\"}",
                "listen: 1 # c\n",
            )
            .unwrap();
        assert_eq!(saved.text, "listen: 1 # c\npublic_address: b\n");
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn one_item_arrays_round_trip_through_xml_with_a_schema() {
        let schema = super::super::diagnostics::build_schema(include_str!(
            "../../tests/fixtures/burrow.schema.json"
        ))
        .unwrap();
        let host = "web:\n  letsencrypt:\n    email: a@b.c\n    dns:\n      - domains:\n          - a.example\n        cloudflare:\n          token: x\n";
        let bridge = Bridge::new(
            Language::YAML,
            Language::XML,
            Some(std::sync::Arc::new(schema)),
        )
        .unwrap();
        let shown = bridge.to_edit(host).unwrap();
        assert_eq!(shown.losses, Vec::<String>::new(), "{}", shown.text);
        let edited = shown.text.replace(">x<", ">y<");
        let saved = bridge.to_host(&edited, host).unwrap();
        assert_eq!(saved.text, host.replace("token: x", "token: \"y\""));
    }

    #[test]
    fn adding_or_removing_a_key_keeps_the_hosts_comments() {
        let hosts = [
            (
                Language::YAML,
                "# Burrow server\nlisten: 7835 # port\npublic_address: a.example\n",
            ),
            (
                Language::TOML,
                "# Burrow server\nlisten = 7835 # port\npublic_address = \"a.example\"\n",
            ),
            (
                Language::HCL,
                "# Burrow server\nlisten = 7835 # port\npublic_address = \"a.example\"\n",
            ),
        ];
        for (host, text) in hosts {
            for edit in [Language::JSON, Language::YAML, Language::TOML] {
                let Some(bridge) = Bridge::new(host, edit, None) else {
                    continue;
                };
                let shown = bridge.to_edit(text).unwrap().text;
                // Add a key.
                let added_value = {
                    let mut value = edit.parse(&shown).unwrap();
                    value["insecure_plaintext"] = serde_json::Value::Bool(true);
                    edit.serialize(&value).unwrap()
                };
                let saved = bridge.to_host(&added_value, text).unwrap().text;
                assert!(
                    saved.contains("# Burrow server") && saved.contains("# port"),
                    "{host} as {edit} add:\n{saved}"
                );
                assert!(saved.contains("insecure_plaintext"), "{saved}");
                // Remove a key.
                let removed_value = {
                    let mut value = edit.parse(&shown).unwrap();
                    value.as_object_mut().unwrap().remove("public_address");
                    edit.serialize(&value).unwrap()
                };
                let saved = bridge.to_host(&removed_value, text).unwrap().text;
                assert!(
                    saved.contains("# Burrow server") && saved.contains("# port"),
                    "{host} as {edit} remove:\n{saved}"
                );
                assert!(!saved.contains("public_address"), "{saved}");
            }
        }
    }

    #[test]
    fn values_the_edit_language_cannot_tell_apart_keep_the_hosts_own() {
        let bridge = Bridge::new(Language::YAML, Language::XML, None).unwrap();
        let host = "public_address: null\nname: \"\"\nflag: \"true\"\ncount: \"1\"\ninsecure_plaintext: false\n";
        let shown = bridge.to_edit(host).unwrap().text;
        let edited = shown.replace(">false<", ">true<");
        let saved = bridge.to_host(&edited, host).unwrap().text;
        assert_eq!(
            saved,
            host.replace("insecure_plaintext: false", "insecure_plaintext: true")
        );
    }

    #[test]
    fn a_key_added_after_a_line_with_a_trailing_comment_keeps_the_comments() {
        for (host_language, host) in [
            (
                Language::YAML,
                "# Burrow\nlisten: 7835 # main port\ninsecure_plaintext: false\n",
            ),
            (
                Language::TOML,
                "# Burrow\nlisten = 7835 # main port\ninsecure_plaintext = false\n",
            ),
            (
                Language::HCL,
                "# Burrow\nlisten = 7835 # main port\ninsecure_plaintext = false\n",
            ),
        ] {
            let bridge = Bridge::new(host_language, Language::JSON, None).unwrap();
            let shown = bridge.to_edit(host).unwrap().text;
            let edited = shown.replace(
                "\"listen\": 7835,",
                "\"listen\": 7835,\n  \"public_address\": \"x\",",
            );
            let saved = bridge.to_host(&edited, host).unwrap().text;
            assert!(
                saved.contains("# Burrow") && saved.contains("# main port"),
                "{host_language}:\n{saved}"
            );
            assert!(saved.contains("public_address"), "{saved}");
        }
    }
}
