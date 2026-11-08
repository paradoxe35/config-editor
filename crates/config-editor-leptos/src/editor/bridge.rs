//! Editing a document in one language while the host keeps it in another
//! (`<ConfigEditor language=Language::XML edit_as=Language::HCL>`).
//!
//! The host's text (`value`, `baseline`, `initial_value`) is shown converted;
//! edits convert back before `on_change`, keeping the host's framing. Both
//! directions go through `codec`, so no language needs bridge support.

use super::Language;
use super::codec::{self, Conversion, ConvertError};
use super::diagnostics::{Diagnostic, SchemaBundle, Severity};
use super::edit_as::{EditAsProblem, EditAsText, SaveError};
use super::language::ConfigLanguage;
use std::sync::Arc;

mod merge;
mod text_scalars;
use merge::{View, merge_changes};

/// A save's patch of the host's text: `previous` made to read as `new`.
type Patch = fn(
    &dyn ConfigLanguage,
    &str,
    &serde_json::Value,
    &serde_json::Value,
    Option<&serde_json::Value>,
) -> Option<String>;

/// The patches that change only what one edit touched, in the order tried.
const PRECISE: [Patch; 5] = [
    codec::patch_reorder,
    codec::patch_scalars,
    codec::patch_renames,
    codec::patch_scalar_to_container,
    codec::patch_inline,
];

// How many saves wrote the host whole (its comments and formatting gone):
// the save property tests require none for a value edit.
#[cfg(test)]
thread_local! {
    pub(super) static WHOLE_RENDERS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

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
    ) -> Result<Conversion, SaveError> {
        if edit_text.trim().is_empty() {
            return Ok(Conversion {
                text: String::new(),
                losses: Vec::new(),
            });
        }
        let edited = codec::read(self.edit.behavior(), edit_text, self.shape())
            .map_err(ConvertError::Parse)?;
        codec::convertible(self.edit.behavior(), edit_text)?;
        let host = self.host.behavior();
        let Ok(old) = codec::read(host, previous_host, self.shape()) else {
            return Ok(codec::render(
                &edited,
                host,
                Some(previous_host),
                self.shape(),
            )?);
        };
        // Only what the user changed is saved: the edit is compared with the
        // host value as the edit language showed it, so values that language
        // cannot tell apart (XML reads `null`, `""` and `"1"` as text) keep
        // the host's own.
        let shown = self.to_edit(previous_host).ok();
        let seen = shown
            .as_ref()
            .and_then(|shown| codec::read(self.edit.behavior(), &shown.text, self.shape()).ok());
        let unshown = shown.map(|shown| shown.losses).unwrap_or_default();
        let value = match &seen {
            // An edit where the view could not show the host's value (an
            // XML view of nested or empty lists) that does not map back onto
            // it waits: saving the view's approximation would change data.
            Some(seen) => merge_changes(
                &old,
                seen,
                &edited,
                &View {
                    unshown: &unshown,
                    repetition: self.edit.behavior().repetition(),
                },
            )
            .map_err(|pointer| {
                ConvertError::Unsupported(format!(
                    "{} cannot show the value at {pointer} exactly, so this edit of it cannot be saved: edit it as {} (at {pointer})",
                    self.edit.label(),
                    self.host.label()
                ))
            })?,
            None => edited,
        };
        // A view that writes every scalar as text (XML) only guesses types:
        // the edited values keep the host's.
        let value = match &seen {
            Some(seen) if self.edit.behavior().scalars_are_text() => {
                text_scalars::in_host_types(&old, seen, value).map_err(|pointer| {
                    ConvertError::Unsupported(format!(
                        "{} reads an empty element here, and {} holds no empty number or boolean: the edit is saved once it holds a value again (at {pointer})",
                        self.edit.label(),
                        self.host.label()
                    ))
                })?
            }
            _ => value,
        };
        // Each changed value in the host's own form (a TOML datetime read
        // back normalized), or the reason the save waits.
        let value = host_scalars(host, previous_host, &old, value)?;
        let rendered = codec::render(&value, host, Some(previous_host), self.shape())?;
        // The host's text is patched to read back exactly as a full render
        // would (losses included), so its comments and formatting survive: a
        // value edit rewrites just those values, a structural one just those
        // lines.
        let Ok(target) = codec::read(host, &rendered.text, self.shape()) else {
            #[cfg(test)]
            WHOLE_RENDERS.with(|count| count.set(count.get() + 1));
            return Ok(rendered);
        };
        // A null the host would read back as something else (XML writes it
        // as empty text) is refused, as TOML refuses it: the save waits.
        if let Some(pointer) = codec::first_pointer(&value, &serde_json::Value::is_null)
            && target.pointer(&pointer).is_some_and(|read| !read.is_null())
        {
            return Err(SaveError::NoNull { pointer });
        }
        // So is an infinity or NaN the host has no number for (it would
        // write it as text).
        if let Some(pointer) =
            codec::first_pointer(&value, &|value| codec::non_finite::name(value).is_some())
            && target.pointer(&pointer) != value.pointer(&pointer)
        {
            return Err(SaveError::NoNonFinite { pointer });
        }
        // The host writer may order keys its own way (TOML puts `reach = {}`
        // after arrays); the patch keeps the edit's order, the host's own.
        let target = in_order_of(target, &value);
        // A value the edit left as it was that the host would write
        // otherwise (a number the view showed in its own form) keeps the
        // host's text: the patches leave it, and it is no loss.
        let (target, kept) = with_untouched_kept(target, &value, &old);
        let mut losses = rendered.losses.clone();
        losses.retain(|pointer| !kept.contains(pointer));
        let shape = self.shape();
        // What the host writes once and reads in several places (a YAML
        // alias) is written out where the edit parts it from the rest.
        let detached = host
            .detach_shared(previous_host, &old, &target)
            .filter(|text| codec::read(host, text, shape).ok().as_ref() == Some(&old));
        let previous_host = detached.as_deref().unwrap_or(previous_host);
        // The first patch that applies, of those that change only what one
        // edit touched. A patch that writes a map in another form (an HCL
        // block as an attribute) is not taken.
        let first = |patches: &[Patch],
                     previous: &str,
                     old: &serde_json::Value,
                     target: &serde_json::Value| {
            patches.iter().find_map(|patch| {
                patch(host, previous, old, target, shape)
                    .filter(|patched| host.keeps_forms(previous, patched))
            })
        };
        let precise = |previous: &str, old: &serde_json::Value, target: &serde_json::Value| {
            first(&PRECISE, previous, old, target)
        };
        let structure = |previous: &str, old: &serde_json::Value, target: &serde_json::Value| {
            first(&[codec::patch_structure], previous, old, target)
        };
        // Items moved that cannot take their comments along wait, rather
        // than leave the comments on the items now in their places.
        if let Some(pointer) = codec::unmoved_comments(host, previous_host, &old, &target) {
            return Err(ConvertError::Unsupported(format!(
                "these items cannot be moved with their comments: move them editing as {} (at {pointer})",
                self.host.label()
            ))
            .into());
        }
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
                        .or_else(|| structure(&text, &current, &step))?;
                    current = step;
                }
                Some(text)
            })
            .or_else(|| structure(previous_host, &old, &target));
        Ok(match patched {
            Some(text) => Conversion { text, losses },
            None => {
                #[cfg(test)]
                WHOLE_RENDERS.with(|count| count.set(count.get() + 1));
                rendered
            }
        })
    }

    /// A warning on each value the host cannot carry exactly, or an error when
    /// the document cannot be saved. Syntax errors are left to validation.
    pub(super) fn problems(
        &self,
        edit_text: &str,
        result: &Result<Conversion, SaveError>,
    ) -> Vec<EditAsProblem> {
        let host = self.host;
        match result {
            Ok(conversion) => conversion
                .losses
                .iter()
                .map(|pointer| EditAsProblem {
                    at: located(self.edit.behavior(), edit_text, pointer, Severity::Warning),
                    text: EditAsText::HostCannotKeep {
                        pointer: pointer.clone(),
                        host,
                    },
                })
                .collect(),
            Err(SaveError::Convert(ConvertError::Parse(_))) => Vec::new(),
            Err(error) => vec![EditAsProblem {
                at: locate_error(self.edit.behavior(), edit_text, error),
                text: EditAsText::NotSaved {
                    host,
                    error: error.clone(),
                },
            }],
        }
    }

    /// A warning on each value the edit language cannot show exactly (XML has
    /// no nested or empty lists): editing it there may change it.
    pub(super) fn view_problems(&self, shown: &Conversion) -> Vec<EditAsProblem> {
        shown
            .losses
            .iter()
            .map(|pointer| EditAsProblem {
                at: located(
                    self.edit.behavior(),
                    &shown.text,
                    pointer,
                    Severity::Warning,
                ),
                text: EditAsText::EditCannotShow {
                    pointer: pointer.clone(),
                    edit: self.edit,
                },
            })
            .collect()
    }

    /// The error shown when the host's text cannot be converted for editing.
    pub(super) fn load_problem(&self, error: &ConvertError) -> EditAsProblem {
        EditAsProblem {
            at: Diagnostic {
                line: Some(1),
                column: Some(1),
                end_column: Some(2),
                severity: Severity::Error,
                ..Diagnostic::default()
            },
            text: EditAsText::CannotEditAs {
                host: self.host,
                edit: self.edit,
                error: error.clone(),
            },
        }
    }
}

/// `value` with each scalar changed from `old` as the host saves it there
/// ([`ConfigLanguage::scalar_edit`]), or the reason the save waits.
fn host_scalars(
    host: &dyn ConfigLanguage,
    previous: &str,
    old: &serde_json::Value,
    mut value: serde_json::Value,
) -> Result<serde_json::Value, SaveError> {
    let mut changed = Vec::new();
    changed_scalars(old, &value, String::new(), &mut changed);
    for pointer in changed {
        let Some(slot) = value.pointer_mut(&pointer) else {
            continue;
        };
        match host.scalar_edit(previous, &pointer, slot) {
            Ok(Some(own)) => *slot = own,
            Ok(None) => {}
            Err(reason) => {
                return Err(ConvertError::Unsupported(format!("{reason} (at {pointer})")).into());
            }
        }
    }
    Ok(value)
}

/// `target` (what the host reads back from writing `value`) with the host's
/// own `old` value wherever `value` keeps it but the host would write it
/// otherwise, and those pointers.
fn with_untouched_kept(
    mut target: serde_json::Value,
    value: &serde_json::Value,
    old: &serde_json::Value,
) -> (serde_json::Value, Vec<String>) {
    let mut differing = Vec::new();
    codec::differences(value, &target, String::new(), &mut differing);
    let mut kept = Vec::new();
    for pointer in differing {
        let at = if pointer == "/" { "" } else { pointer.as_str() };
        let Some(own) = old
            .pointer(at)
            .filter(|own| value.pointer(at) == Some(*own))
        else {
            continue;
        };
        if let Some(slot) = target.pointer_mut(at) {
            *slot = own.clone();
            kept.push(pointer);
        }
    }
    (target, kept)
}

/// The pointers of the scalars `new` changes where `old` holds one too.
fn changed_scalars(
    old: &serde_json::Value,
    new: &serde_json::Value,
    pointer: String,
    out: &mut Vec<String>,
) {
    use serde_json::Value;
    match (old, new) {
        (Value::Object(left), Value::Object(right)) => {
            for (key, value) in right {
                if let Some(before) = left.get(key) {
                    let child = format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1"));
                    changed_scalars(before, value, child, out);
                }
            }
        }
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => {
            for (index, (before, value)) in left.iter().zip(right).enumerate() {
                changed_scalars(before, value, format!("{pointer}/{index}"), out);
            }
        }
        (Value::Object(_) | Value::Array(_), _) | (_, Value::Object(_) | Value::Array(_)) => {}
        _ if old != new => out.push(pointer),
        _ => {}
    }
}

/// Where an error is, on the value it names (`… (at /pointer)`) when it
/// names one.
pub(super) fn locate_error(
    behavior: &dyn ConfigLanguage,
    source: &str,
    error: &SaveError,
) -> Diagnostic {
    located(behavior, source, &error.pointer(), Severity::Error)
}

/// A place on the value at `pointer` in `source`, or on the document's
/// first content when the value has no span (the message is left for the
/// problem to word).
fn located(
    behavior: &dyn ConfigLanguage,
    source: &str,
    pointer: &str,
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
        severity,
        ..Diagnostic::default()
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
    use super::super::diagnostics::{Diagnostic, Severity};
    use super::super::edit_as::EditAsProblem;
    use super::Bridge;

    fn worded(problems: Vec<EditAsProblem>) -> Vec<Diagnostic> {
        problems
            .iter()
            .map(|problem| problem.worded(crate::strings::default_strings()))
            .collect()
    }

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
        let problems = worded(bridge.problems(edit, &result));
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
        let problems = worded(bridge.problems(edit, &result));
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

    /// The XML view repeats elements for nested lists and shows a one-item
    /// list as its item: a value edited there lands in its place in the
    /// host, the nesting and the list kept (round 17 flattened them).
    #[cfg(all(feature = "yaml", feature = "xml"))]
    #[test]
    fn a_value_edited_in_a_view_without_nested_lists_keeps_the_nesting() {
        let host = "# c\nm:\n  - - a # ac\n    - b\n  - - - e\n    - f\n  - x\ndeep:\n  - k: v # kc\nn: 1 # n\n";
        let bridge = Bridge::new(Language::YAML, Language::XML, None).unwrap();
        for (from, to, expected) in [
            (">b<", ">B<", host.replace("- b", "- B")),
            (">e<", ">E<", host.replace("- e", "- E")),
            (">f<", ">F<", host.replace("- f", "- F")),
            (">x<", ">X<", host.replace("- x", "- X")),
            (">v<", ">w<", host.replace("k: v", "k: w")),
        ] {
            let shown = bridge.to_edit(host).unwrap().text;
            assert!(shown.contains(from), "{from} in {shown}");
            let saved = bridge.to_host(&shown.replace(from, to), host).unwrap();
            assert_eq!(saved.text, expected, "{from} -> {to}");
        }
    }

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
        let edited = shown.text.replace(">x<", ">z<");
        let saved = bridge.to_host(&edited, host).unwrap();
        assert_eq!(saved.text, host.replace("token: x", "token: z"));
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

    fn worded(problems: Vec<EditAsProblem>) -> Vec<Diagnostic> {
        problems
            .iter()
            .map(|problem| problem.worded(crate::strings::default_strings()))
            .collect()
    }

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
        assert_eq!(saved, "public_address = '''cd''' # real\nlisten = 1\n");
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
        let warnings = worded(bridge.view_problems(&shown));
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
        let warnings = worded(bridge.view_problems(&shown));
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
