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
        // A null the host would read back as something else (XML writes it
        // as empty text) is refused, as TOML refuses it: the save waits.
        if let Some(pointer) = codec::first_pointer(&value, &serde_json::Value::is_null)
            && target.pointer(&pointer).is_some_and(|read| !read.is_null())
        {
            return Err(ConvertError::Unsupported(format!(
                "{} has no null value (at {pointer})",
                self.host.label()
            )));
        }
        // The host writer may order keys its own way (TOML puts `reach = {}`
        // after arrays); the patch keeps the edit's order, the host's own.
        let target = in_order_of(target, &value);
        let shape = self.shape();
        // What the host writes once and reads in several places (a YAML
        // alias) is written out where the edit parts it from the rest.
        let detached = host
            .detach_shared(previous_host, &old, &target)
            .filter(|text| codec::read(host, text, shape).ok().as_ref() == Some(&old));
        let previous_host = detached.as_deref().unwrap_or(previous_host);
        // The patches that change only what one edit touched.
        let precise = |previous: &str, old: &serde_json::Value, target: &serde_json::Value| {
            codec::patch_reorder(host, previous, old, target, shape)
                .or_else(|| codec::patch_scalars(host, previous, old, target, shape))
                .or_else(|| codec::patch_renames(host, previous, old, target, shape))
                .or_else(|| codec::patch_scalar_to_container(host, previous, old, target, shape))
                .or_else(|| codec::patch_inline(host, previous, old, target, shape))
        };
        let patched = precise(previous_host, &old, &target)
            // Several edits in one save: one step at a time.
            .or_else(|| {
                let steps = codec::steps(&old, &target);
                if steps.is_empty() {
                    return None;
                }
                let (mut text, mut current) = (previous_host.to_string(), old.clone());
                for step in steps {
                    text = precise(&text, &current, &step)
                        .or_else(|| codec::patch_structure(host, &text, &current, &step, shape))?;
                    current = step;
                }
                Some(text)
            })
            .or_else(|| codec::patch_structure(host, previous_host, &old, &target, shape));
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
    /// A warning on each value the edit language cannot show exactly (XML has
    /// no nested or empty lists): editing it there may change it.
    pub(super) fn view_problems(&self, shown: &Conversion) -> Vec<Diagnostic> {
        let edit = self.edit.label();
        shown
            .losses
            .iter()
            .map(|pointer| {
                located(
                    self.edit.behavior(),
                    &shown.text,
                    pointer,
                    format!("{pointer}: {edit} cannot show this value exactly; editing it here may change it"),
                    Severity::Warning,
                )
            })
            .collect()
    }

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
            // In the host's key order (the edit language may order keys its
            // own way: TOML puts plain keys before tables).
            // A renamed key: gone from the edit, and a new key holding what
            // the edit showed for it. It keeps its place and the host's own
            // value (which the edit language may not show exactly).
            let mut renamed_to: Vec<(&String, &String)> = Vec::new();
            for (key, value) in edited {
                if host.contains_key(key) {
                    continue;
                }
                let from = host.keys().find(|old| {
                    !edited.contains_key(*old)
                        && seen.get(*old) == Some(value)
                        && !renamed_to.iter().any(|(taken, _)| taken == old)
                });
                if let Some(from) = from {
                    renamed_to.push((from, key));
                }
            }
            let mut entries: Vec<(String, Value)> = Vec::new();
            for (key, host_value) in host {
                match (seen.get(key), edited.get(key)) {
                    (Some(seen_value), Some(value)) => {
                        entries.push((key.clone(), merge_changes(host_value, seen_value, value)));
                    }
                    // Renamed by the user.
                    (Some(_), None)
                        if let Some((_, to)) = renamed_to.iter().find(|(from, _)| *from == key) =>
                    {
                        entries.push(((*to).clone(), host_value.clone()));
                    }
                    // Removed by the user.
                    (Some(_), None) => {}
                    // Re-added after the edit language could not show it, or
                    // never shown: the edit wins, else the host's stays.
                    (None, Some(value)) => entries.push((key.clone(), value.clone())),
                    (None, None) => entries.push((key.clone(), host_value.clone())),
                }
            }
            // A new key goes after the key before it in the edit, so a renamed
            // key keeps its place.
            let mut previous: Option<&String> = None;
            for (key, value) in edited {
                let renamed = renamed_to.iter().any(|(_, to)| *to == key);
                if !host.contains_key(key) && !renamed {
                    let at = previous
                        .and_then(|before| entries.iter().position(|(name, _)| name == before))
                        .map_or(0, |index| index + 1);
                    entries.insert(at, (key.clone(), value.clone()));
                }
                previous = Some(key);
            }
            Value::Object(entries.into_iter().collect())
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
    // A value missing from the text (the view dropped it) is placed on its
    // nearest ancestor that is there.
    let mut at = pointer;
    let span = loop {
        if let Some(span) = behavior
            .value_span(source, at)
            .or_else(|| behavior.key_span(source, at))
        {
            break Some(span);
        }
        match at.rfind('/') {
            Some(parent) if parent > 0 => at = &at[..parent],
            _ => break behavior.first_span(source),
        }
    };
    Diagnostic {
        line: span.map(|span| span.line),
        column: span.map(|span| span.start_column),
        end_column: span.map(|span| span.end_column),
        message,
        severity,
    }
}

// Conversion tests pair languages, so they need every built-in one.

/// `value` with each object's keys in `order`'s order (keys `order` lacks
/// last, as they were).
fn in_order_of(value: serde_json::Value, order: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match (value, order) {
        (Value::Object(mut map), Value::Object(reference)) => {
            let mut ordered = serde_json::Map::new();
            for (key, child_order) in reference {
                if let Some(child) = map.remove(key) {
                    ordered.insert(key.clone(), in_order_of(child, child_order));
                }
            }
            ordered.extend(map);
            Value::Object(ordered)
        }
        (Value::Array(items), Value::Array(reference)) => Value::Array(
            items
                .into_iter()
                .enumerate()
                .map(|(index, item)| match reference.get(index) {
                    Some(child_order) => in_order_of(item, child_order),
                    None => item,
                })
                .collect(),
        ),
        (value, _) => value,
    }
}

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
        assert_eq!(saved.text, host.replace("token: x", "token: y"));
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

    #[test]
    fn a_value_edit_inside_a_flow_map_keeps_the_comments() {
        let bridge = Bridge::new(Language::YAML, Language::JSON, None).unwrap();
        let host = "# head\npool: {min_idle: 4, max_idle: 8} # tuned\nlisten: 7835\n";
        let shown = bridge.to_edit(host).unwrap().text;
        let edited = shown.replace("\"max_idle\": 8", "\"max_idle\": 9");
        let saved = bridge.to_host(&edited, host).unwrap().text;
        assert_eq!(saved, host.replace("max_idle: 8", "max_idle: 9"));
    }

    #[test]
    fn renaming_a_key_keeps_its_trailing_comment() {
        let bridge = Bridge::new(Language::YAML, Language::JSON, None).unwrap();
        let host = "# head\nlisten: 7835 # main port\ninsecure_plaintext: false\n";
        let shown = bridge.to_edit(host).unwrap().text;
        let edited = shown.replace("\"listen\"", "\"listen_port\"");
        let saved = bridge.to_host(&edited, host).unwrap().text;
        assert!(saved.contains("# head"), "{saved}");
        assert!(saved.contains("listen_port: 7835 # main port"), "{saved}");
    }
}

#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod structural_saves {
    use super::*;

    /// The host text saved after `change` is made to the shown text.
    fn save(
        host: Language,
        edit: Language,
        host_text: &str,
        change: impl Fn(&str) -> String,
    ) -> String {
        let bridge = Bridge::new(host, edit, None).unwrap();
        let shown = bridge.to_edit(host_text).unwrap().text;
        bridge.to_host(&change(&shown), host_text).unwrap().text
    }

    #[test]
    fn deleting_a_table_removes_it_and_its_comment_only() {
        let toml = "# head\nlisten = 7835 # port\n\n# acme client\n[clients.acme]\nsecret = \"s1\" # the secret\n\n# beta client\n[clients.beta]\nsecret = \"s2\"\n";
        let saved = save(Language::TOML, Language::YAML, toml, |shown| {
            shown.replace("  beta:\n    secret: s2\n", "")
        });
        assert_eq!(
            saved,
            "# head\nlisten = 7835 # port\n\n# acme client\n[clients.acme]\nsecret = \"s1\" # the secret\n"
        );
    }

    #[test]
    fn deleting_a_block_removes_it_and_its_comment_only() {
        let hcl = "# head\nlisten = 7835 # port\n\nclients {\n  # acme\n  acme {\n    secret = \"s1\" # s\n  }\n\n  # beta\n  beta {\n    secret = \"s2\"\n  }\n}\n";
        let saved = save(Language::HCL, Language::YAML, hcl, |shown| {
            shown.replace("  beta:\n    secret: s2\n", "")
        });
        assert_eq!(
            saved,
            "# head\nlisten = 7835 # port\n\nclients {\n  # acme\n  acme {\n    secret = \"s1\" # s\n  }\n}\n"
        );
    }

    #[test]
    fn an_edit_language_with_its_own_key_order_keeps_the_hosts() {
        let yaml = "# head\nclients:\n  acme:\n    secret: s1\npublic_address: x.example # addr\nlisten: 1 # port\n";
        let saved = save(Language::YAML, Language::TOML, yaml, |shown| {
            shown.replace(
                "secret = \"s1\"\n",
                "secret = \"s1\"\nmax_connections = 3\n",
            )
        });
        assert_eq!(
            saved,
            "# head\nclients:\n  acme:\n    secret: s1\n    max_connections: 3\npublic_address: x.example # addr\nlisten: 1 # port\n"
        );
    }

    #[test]
    fn adding_to_an_inline_container_rewrites_only_that_container() {
        let flow = "# head\ntcp: {nodelay: true} # tcp\nlisten: 1 # port\n";
        let saved = save(Language::YAML, Language::JSON, flow, |shown| {
            shown.replace(
                "\"nodelay\": true",
                "\"nodelay\": true,\n    \"keepalive_secs\": 3",
            )
        });
        assert_eq!(
            saved,
            "# head\ntcp: {nodelay: true, keepalive_secs: 3} # tcp\nlisten: 1 # port\n"
        );
        let sequence = "# head\nallow: [a, b] # s\nlisten: 1\n";
        let saved = save(Language::YAML, Language::JSON, sequence, |shown| {
            shown.replace("\"b\"", "\"b\",\n    \"c\"")
        });
        assert_eq!(saved, "# head\nallow: [a, b, c] # s\nlisten: 1\n");
        let inline = "# head\ntcp = { nodelay = true } # tcp\nlisten = 1\n";
        let saved = save(Language::TOML, Language::YAML, inline, |shown| {
            shown.replace("nodelay: true", "nodelay: true\n  keepalive_secs: 3")
        });
        assert_eq!(
            saved,
            "# head\ntcp = { nodelay = true, keepalive_secs = 3 } # tcp\nlisten = 1\n"
        );
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn a_value_in_a_lone_block_read_as_a_list_changes_only_that_value() {
        let schema = super::super::diagnostics::build_schema(include_str!(
            "../../tests/fixtures/burrow.schema.json"
        ))
        .unwrap();
        let bridge = Bridge::new(
            Language::HCL,
            Language::YAML,
            Some(std::sync::Arc::new(schema)),
        )
        .unwrap();
        let host = "web {\n  # certs\n  certificates {\n    hosts = [\"a\"]\n    cert = \"c\" # c\n  }\n}\n";
        let shown = bridge.to_edit(host).unwrap().text;
        assert!(shown.contains("cert: c"), "{shown}");
        let saved = bridge
            .to_host(&shown.replace("cert: c", "cert: d"), host)
            .unwrap()
            .text;
        assert_eq!(saved, host.replace("cert = \"c\"", "cert = \"d\""));
    }

    #[test]
    fn a_value_in_multi_line_string_syntax_keeps_what_follows_it() {
        let toml = "public_address = \'\'\'ab\'\'\' # real\nlisten = 1\n";
        let saved = save(Language::TOML, Language::YAML, toml, |shown| {
            shown.replace("public_address: ab", "public_address: cd")
        });
        assert_eq!(saved, "public_address = \"cd\" # real\nlisten = 1\n");
        let hcl = "public_address = <<EOT\nab\nEOT\n\n# after\nlisten = 1\n";
        let saved = save(Language::HCL, Language::YAML, hcl, |shown| {
            shown.replace("public_address: |\n  ab", "public_address: |\n  cd")
        });
        assert_eq!(
            saved,
            "public_address = <<EOT\ncd\nEOT\n\n# after\nlisten = 1\n"
        );
        let yaml = "public_address: >- # c\n  ab\nlisten: 1\n";
        let saved = save(Language::YAML, Language::JSON, yaml, |shown| {
            shown.replace("\"ab\"", "\"cd\"")
        });
        assert_eq!(saved, "public_address: >- # c\n  cd\nlisten: 1\n");
    }

    #[test]
    fn values_the_view_cannot_show_are_warned_about() {
        let bridge = Bridge::new(Language::YAML, Language::XML, None).unwrap();
        let shown = bridge
            .to_edit("listen: 1\nx:\n  - [1, 2]\n  - [3]\nallow: []\n")
            .unwrap();
        let warnings = bridge.view_problems(&shown);
        let messages: Vec<&str> = warnings
            .iter()
            .map(|warning| warning.message.as_str())
            .collect();
        assert!(
            messages.iter().any(|message| message.starts_with("/x")),
            "{messages:?}\n{}",
            shown.text
        );
        assert!(
            messages.iter().any(|message| message.starts_with("/allow")),
            "{messages:?}\n{}",
            shown.text
        );
        assert!(
            warnings
                .iter()
                .all(|warning| warning.severity == Severity::Warning)
        );
    }

    #[test]
    fn xml_attributes_stay_attributes() {
        let xml = "<!-- head -->\n<services>\n  <web mode=\"balance\" target=\"80\">\n    <!-- c -->\n    <client>acme</client>\n  </web>\n</services>\n<listen>1</listen>\n";
        let saved = save(Language::XML, Language::YAML, xml, |shown| {
            shown.replace("mode: balance", "mode: failover")
        });
        assert_eq!(saved, xml.replace("mode=\"balance\"", "mode=\"failover\""));
        let saved = save(Language::XML, Language::YAML, xml, |shown| {
            shown.replace(
                "    client: acme\n",
                "    client: acme\n    bandwidth: 1m\n",
            )
        });
        assert!(
            saved.contains("<!-- head -->") && saved.contains("<!-- c -->"),
            "{saved}"
        );
        assert!(
            saved.contains("<web mode=\"balance\" target=\"80\">"),
            "{saved}"
        );
        assert!(saved.contains("<bandwidth>1m</bandwidth>"), "{saved}");
    }

    #[test]
    fn toml_dotted_keys_stay_dotted() {
        let toml = "# head\nlisten = 7835 # port\nclients.acme.secret = \"a\" # sa\n\n# limits\n[limits]\nmax_pending = 5\n";
        let saved = save(Language::TOML, Language::YAML, toml, |shown| {
            shown.replace("    secret: a\n", "    secret: a\n    max_connections: 2\n")
        });
        assert!(
            saved.contains("# head") && saved.contains("# sa") && saved.contains("# limits"),
            "{saved}"
        );
        assert!(
            saved.contains("clients.acme.secret = \"a\" # sa\n"),
            "{saved}"
        );
        assert!(
            saved.contains("clients.acme.max_connections = 2\n"),
            "{saved}"
        );
        assert!(!saved.contains("[clients"), "{saved}");
    }

    #[test]
    fn hcl_labelled_blocks_stay_labelled() {
        let hcl = "# header\nlisten = 7835 // port\n\n# acme\nclients \"acme\" {\n  secret = \"abc\" # s\n}\n\nservices \"web\" {\n  client = \"acme\"\n  target = 8080\n}\n";
        let saved = save(Language::HCL, Language::YAML, hcl, |shown| {
            shown.replace(
                "    target: 8080\n",
                "    target: 8080\n    mode: balance\n",
            )
        });
        assert_eq!(
            saved,
            hcl.replace(
                "  target = 8080\n",
                "  target = 8080\n  mode   = \"balance\"\n"
            )
            .replace("mode   =", "mode ="),
            "{saved}"
        );
    }

    #[test]
    fn a_json_edit_at_the_end_of_an_object_changes_only_its_lines() {
        let json = "{\n    \"listen\": 7835,\n    \"clients\": {\"acme\": {\"secret\": \"abc\"}},\n    \"pool\": {\n        \"min_idle\": 4,\n        \"max_idle\": 8\n    }\n}\n";
        let saved = save(Language::JSON, Language::YAML, json, |shown| {
            shown.replace("  max_idle: 8\n", "")
        });
        assert_eq!(
            saved,
            "{\n    \"listen\": 7835,\n    \"clients\": {\"acme\": {\"secret\": \"abc\"}},\n    \"pool\": {\n        \"min_idle\": 4\n    }\n}\n"
        );
    }

    #[test]
    fn renaming_a_table_keeps_the_comment_above_it() {
        let toml = "listen = 7835\n\n# heartbeat section\n[heartbeat]\ninterval_secs = 15\n\n# acme client\n[clients.acme]\nsecret = \"a\"\n";
        let saved = save(Language::TOML, Language::YAML, toml, |shown| {
            shown.replace("  acme:", "  acme2:")
        });
        assert_eq!(saved, toml.replace("[clients.acme]", "[clients.acme2]"));
        let saved = save(Language::TOML, Language::YAML, toml, |shown| {
            shown.replace("heartbeat:", "limits:")
        });
        assert_eq!(saved, toml.replace("[heartbeat]", "[limits]"));
    }

    #[test]
    fn a_deleted_nested_entry_takes_its_own_comment() {
        let hcl = "clients {\n  # acme\n  acme {\n    secret = \"a\"\n  }\n\n  # beta\n  beta {\n    secret = \"b\"\n  }\n}\n";
        let saved = save(Language::HCL, Language::YAML, hcl, |shown| {
            shown.replace("  acme:\n    secret: a\n", "")
        });
        assert_eq!(
            saved,
            "clients {\n  # beta\n  beta {\n    secret = \"b\"\n  }\n}\n"
        );
        let yaml =
            "# head\nlisten: 1\n# public comment\npublic_address: x\ninsecure_plaintext: false\n";
        let saved = save(Language::YAML, Language::JSON, yaml, |shown| {
            shown.replace("  \"public_address\": \"x\",\n", "")
        });
        assert_eq!(saved, "# head\nlisten: 1\ninsecure_plaintext: false\n");
    }

    #[test]
    fn deleting_the_first_of_repeated_tables_removes_that_one() {
        let toml = "listen = 1\n\n# first\n[[web.certificates]]\nhosts = [\"a\"] # n\ncert = \"a.pem\"\n\n# second\n[[web.certificates]]\nhosts = [\"b\"]\n";
        let saved = save(Language::TOML, Language::JSON, toml, |shown| {
            let value: serde_json::Value = serde_json::from_str(shown).unwrap();
            let mut value = value;
            value["web"]["certificates"]
                .as_array_mut()
                .unwrap()
                .remove(0);
            serde_json::to_string_pretty(&value).unwrap()
        });
        assert_eq!(
            saved,
            "listen = 1\n\n# second\n[[web.certificates]]\nhosts = [\"b\"]\n"
        );
    }

    #[test]
    fn a_table_inserted_between_two_gets_one_blank_line_each_side() {
        let toml = "[clients.acme]\nsecret = \"abc\"\n\n# beta\n[clients.beta]\nsecret = \"b\"\n";
        let saved = save(Language::TOML, Language::JSON, toml, |shown| {
            let mut value: serde_json::Value = serde_json::from_str(shown).unwrap();
            let clients = value["clients"].as_object_mut().unwrap();
            let beta = clients.remove("beta").unwrap();
            clients.insert("gamma".into(), serde_json::json!({"secret": "g"}));
            clients.insert("beta".into(), beta);
            serde_json::to_string_pretty(&value).unwrap()
        });
        assert_eq!(
            saved,
            "[clients.acme]\nsecret = \"abc\"\n\n[clients.gamma]\nsecret = \"g\"\n\n# beta\n[clients.beta]\nsecret = \"b\"\n"
        );
    }

    #[test]
    fn an_empty_host_filled_through_edit_as_is_written_whole() {
        let saved = save(Language::TOML, Language::YAML, "", |_| {
            "listen: 1\nheartbeat:\n  interval_secs: 3\n".to_string()
        });
        assert_eq!(saved, "listen = 1\n\n[heartbeat]\ninterval_secs = 3\n");
    }

    #[test]
    fn a_warning_for_a_value_the_view_drops_sits_on_its_nearest_shown_ancestor() {
        let bridge = Bridge::new(Language::JSON, Language::XML, None).unwrap();
        let shown = bridge
            .to_edit("{\"listen\": 99999, \"self\": {\"allow\": []}}")
            .unwrap();
        let warnings = bridge.view_problems(&shown);
        let allow = warnings
            .iter()
            .find(|warning| warning.message.starts_with("/self/allow"))
            .expect("a warning for /self/allow");
        let line = shown.text.lines().nth(allow.line.unwrap() - 1).unwrap();
        assert!(line.contains("self"), "{line:?} in\n{}", shown.text);
    }

    #[test]
    fn the_first_key_of_a_comment_only_document_goes_under_its_header() {
        let saved = save(Language::YAML, Language::JSON, "# header\n", |_| {
            "{\"listen\": 1}".to_string()
        });
        assert_eq!(saved, "# header\nlisten: 1\n");
    }
}
