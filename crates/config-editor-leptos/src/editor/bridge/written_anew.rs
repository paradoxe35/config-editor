//! A save no patch applies to writes the host's text anew from the data,
//! which holds no comments (nor an XML file's processing instructions): a
//! host that has some waits instead, since a save keeps them (the Edit-as
//! notice says so).

use super::super::Language;
use super::super::codec::{self, ConvertError};
use super::super::edit_as::SaveError;
use serde_json::Value;

/// Why writing `host`'s text `previous` anew (it read as `old`, the save
/// holds `target`) waits: the comments it would drop, at the first value
/// the save changes. `None` when `previous` has none.
pub(super) fn refused(
    host: Language,
    previous: &str,
    (old, target): (&Value, &Value),
) -> Option<SaveError> {
    if !host.behavior().has_comments(previous) {
        return None;
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
            let error = refused(host, text, (&old, &target)).map(|error| format!("{error:?}"));
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
                "<?xml version=\"1.0\"?>\n<r><a>1</a><b>2</b></r>\n",
                (&old, &target)
            ),
            None
        );
        assert_eq!(
            refused(Language::YAML, "r:\n  a: 1\n  b: 2\n", (&old, &target)),
            None
        );
    }
}
