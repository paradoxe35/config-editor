//! A save no patch applies to writes the host's text anew from the data,
//! which holds no comments (nor an XML file's processing instructions)
//! and none of the forms the host wrote its values in (a TOML hex number
//! or datetime, an HCL expression or heredoc, a YAML tag or quoting, XML
//! markup): a host that has comments waits instead, since a save keeps
//! them (the Edit-as notice says so), and so does a save that would write
//! a value it leaves as it was in another form, so a save no
//! patch knows never passes changing one.

use super::super::Language;
use super::super::codec::{self, ConvertError, written_text_range};
use super::super::edit_as::SaveError;
use super::super::language::{ConfigLanguage, pointer_key};
use super::super::text::{TextRange, slice};
use crate::editor::paths::FieldPath;
use serde_json::Value;

/// Why writing `host`'s text `previous` anew as `rendered` (it read as
/// `old`, the save holds `target`) waits: the comments it would drop, at
/// the first value the save changes, or the first value it leaves as it
/// was that `rendered` writes in another form. `None` when neither.
pub(super) fn refused(
    host: Language,
    (previous, rendered): (&str, &str),
    (old, target): (&Value, &Value),
) -> Option<SaveError> {
    if !host.behavior().has_comments(previous) {
        let pointer = changed_form(host.behavior(), (previous, rendered), (old, target))?;
        return Some(
            ConvertError::Unsupported(format!(
                "this edit can only be saved by writing the whole document anew, which would write the value at {pointer} in another form than the {} file has it: edit it as {} (at {pointer})",
                host.label(),
                host.label()
            ))
            .into(),
        );
    }
    let mut changed = Vec::new();
    codec::differences(old, target, String::new(), &mut changed);
    let pointer = changed
        .into_iter()
        .next()
        .unwrap_or_else(|| "/".to_string());
    Some(
        ConvertError::Unsupported(format!(
            "this edit can only be saved by writing the whole document anew, which would drop the comments of the {} file: make it editing as {} (at {pointer})",
            host.label(),
            host.label()
        ))
        .into(),
    )
}

/// The scalar `old` and `target` hold alike at its pointer that `previous`
/// and `rendered` write in other text (with its tag, quotes or markup),
/// where both write it: of several, the one `rendered` writes nearest to
/// the first value the edit changed, which kept the save from patching
/// the host in place (a `3.10` beside a key added after it), not one
/// elsewhere that a patch would have left alone.
fn changed_form(
    host: &dyn ConfigLanguage,
    (previous, rendered): (&str, &str),
    (old, target): (&Value, &Value),
) -> Option<String> {
    let mut kept = Vec::new();
    unchanged_scalars(old, target, &mut String::new(), &mut kept);
    let mut paths: Vec<FieldPath> = kept.iter().map(FieldPath::from).collect();
    let mut changed = Vec::new();
    codec::differences(old, target, String::new(), &mut changed);
    let edited = changed
        .into_iter()
        .find(|pointer| target.pointer(pointer).is_some());
    paths.extend(edited.as_deref().map(FieldPath::from));
    // Each text's spans placed from its lines split once (see `with_lines`).
    let written = |text: &str, paths: &[FieldPath]| -> Vec<Option<TextRange>> {
        super::with_lines(host, text, || {
            host.value_spans(text, paths)
                .into_iter()
                .map(|span| written_text_range(host, text, span?))
                .collect()
        })
    };
    let before = written(previous, &paths[..kept.len()]);
    let mut after = written(rendered, &paths);
    let edit_at = if edited.is_some() {
        after.pop().flatten()
    } else {
        None
    };
    let mismatches = kept.iter().zip(before.into_iter().zip(after)).filter_map(
        |(pointer, ranges)| match ranges {
            (Some(was), Some(is)) => {
                (slice::slice(previous, was) != slice::slice(rendered, is)).then_some((pointer, is))
            }
            _ => None,
        },
    );
    let distance = |range: TextRange| {
        edit_at.map_or(0, |edit: TextRange| {
            if range.end() <= edit.start() {
                u32::from(edit.start() - range.end())
            } else {
                u32::from(range.start().max(edit.start()) - edit.start())
            }
        })
    };
    mismatches
        .min_by_key(|(_, range)| distance(*range))
        .map(|(pointer, _)| pointer.clone())
}

/// The pointers of the scalars of `old` that `target` holds alike there.
fn unchanged_scalars(old: &Value, target: &Value, pointer: &mut String, out: &mut Vec<String>) {
    match (old, target) {
        (Value::Object(left), Value::Object(right)) => {
            for (key, value) in left {
                if let Some(other) = right.get(key) {
                    let length = pointer.len();
                    pointer.push('/');
                    pointer.push_str(&pointer_key(key));
                    unchanged_scalars(value, other, pointer, out);
                    pointer.truncate(length);
                }
            }
        }
        (Value::Array(left), Value::Array(right)) => {
            for (index, (value, other)) in left.iter().zip(right).enumerate() {
                let length = pointer.len();
                pointer.push_str(&format!("/{index}"));
                unchanged_scalars(value, other, pointer, out);
                pointer.truncate(length);
            }
        }
        (Value::Object(_) | Value::Array(_), _) => {}
        _ if old == target => out.push(pointer.clone()),
        _ => {}
    }
}

#[cfg(all(test, feature = "yaml", feature = "xml"))]
mod tests {
    use super::super::super::Language;
    use super::refused;
    use serde_json::json;

    #[test]
    fn a_host_with_comments_is_not_written_anew() {
        let (old, target) = (json!({"r": {"a": 1, "b": 2}}), json!({"r": {"b": 2}}));
        for (host, text) in [
            (Language::XML, "<r><!-- c --><a>1</a><b>2</b></r>\n"),
            (Language::XML, "<r><?pi x?><a>1</a><b>2</b></r>\n"),
            (Language::YAML, "r:\n  a: 1 # c\n  b: 2\n"),
        ] {
            let error =
                refused(host, (text, text), (&old, &target)).map(|error| format!("{error:?}"));
            assert!(
                error
                    .as_deref()
                    .is_some_and(|error| error.contains("would drop the comments")
                        && error.contains("(at /r/a)")),
                "{host}: {error:?}"
            );
        }
        assert_eq!(
            refused(
                Language::XML,
                (
                    "<?xml version=\"1.0\"?>\n<r><a>1</a><b>2</b></r>\n",
                    "<r><b>2</b></r>\n"
                ),
                (&old, &target)
            ),
            None
        );
        assert_eq!(
            refused(
                Language::YAML,
                ("r:\n  a: 1\n  b: 2\n", "r:\n  b: 2\n"),
                (&old, &target)
            ),
            None
        );
    }

    /// Writing anew a host without comments whose values keep forms the
    /// writer does not write (each language's own) waits, naming the first
    /// value it would write otherwise; one whose values the writer writes
    /// as they are does not.
    #[cfg(all(feature = "json", feature = "toml", feature = "hcl"))]
    #[test]
    fn a_host_written_anew_in_other_forms_waits() {
        use super::super::super::codec::serialize;
        for (host, text, at) in [
            (Language::TOML, "[a]\nmask = 0xff\ngone = 1\n", "/a/mask"),
            (
                Language::TOML,
                "[a]\nwhen = 1979-05-27T07:32:00Z\ngone = 1\n",
                "/a/when",
            ),
            (Language::TOML, "[a]\nlit = 'x'\ngone = 1\n", "/a/lit"),
            (Language::HCL, "a {\n  x = var.y\n  gone = 1\n}\n", "/a/x"),
            (
                Language::HCL,
                "a {\n  x = <<EOT\nv\nEOT\n  gone = 1\n}\n",
                "/a/x",
            ),
            (Language::YAML, "a:\n  t: !!str 1\n  gone: 1\n", "/a/t"),
            (Language::YAML, "a:\n  q: 'x'\n  gone: 1\n", "/a/q"),
            (
                Language::JSON,
                "{\"a\": {\"f\": 1.50, \"gone\": 1}}\n",
                "/a/f",
            ),
            (
                Language::XML,
                "<r><a><c><![CDATA[x]]></c><gone>1</gone></a></r>\n",
                "/r/a/c",
            ),
        ] {
            let old = host.parse(text).expect("parses");
            let mut target = old.clone();
            let parent = at.rsplit_once('/').map_or("", |(parent, _)| parent);
            target
                .pointer_mut(parent)
                .and_then(serde_json::Value::as_object_mut)
                .expect("a map")
                .shift_remove("gone");
            let rendered = serialize(host.behavior(), &target, None).expect("writes");
            let error =
                refused(host, (text, &rendered), (&old, &target)).map(|error| format!("{error:?}"));
            assert!(
                error
                    .as_deref()
                    .is_some_and(|error| error.contains("in another form")
                        && error.contains(&format!("(at {at})"))),
                "{host} {text:?} → {rendered:?}: {error:?}"
            );
        }
        // Of several values written in another form, the one beside the
        // edit is named, not the first in the document.
        let text = "a:\n  ref: !Ref X\n  big: 0x1F\nb:\n  items:\n    - 3.9\n    - 0o17\n";
        let old = Language::YAML.parse(text).expect("parses");
        let mut target = old.clone();
        target["b"]["lint"] = json!(1);
        let rendered = serialize(Language::YAML.behavior(), &target, None).expect("writes");
        let error = refused(Language::YAML, (text, &rendered), (&old, &target))
            .map(|error| format!("{error:?}"));
        assert!(
            error
                .as_deref()
                .is_some_and(|error| error.contains("(at /b/items/1)")),
            "{rendered:?}: {error:?}"
        );
        let (old, target) = (json!({"a": {"k": 1, "gone": 1}}), json!({"a": {"k": 1}}));
        assert_eq!(
            refused(
                Language::TOML,
                ("[a]\nk = 1\ngone = 1\n", "[a]\nk = 1\n"),
                (&old, &target)
            ),
            None
        );
    }
}
