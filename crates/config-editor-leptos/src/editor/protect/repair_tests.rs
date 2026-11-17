//! Repairs of a text from the host that does not parse, with the host's
//! read-only paths: closing the block that holds a lock, a lock on a path
//! the text never had, a lock beside the break. Each repair keeps every
//! locked value, so each goes through (I2), whichever order the text and
//! the paths arrive in; changing the locked value while the text does not
//! parse is refused at once (I1): the value is a span of the text, followed
//! from where it was found, whatever breaks the text around it.

use super::super::Language;
use super::super::codec;
use super::super::paths::FieldPath;
use super::guard::check;
use super::locks::{Protected, Sources, protect};

fn locks(
    language: Language,
    text: &str,
    paths: &[&str],
    previous: Option<&Protected>,
) -> Protected {
    let paths: Vec<FieldPath> = paths.iter().map(|path| FieldPath::from(*path)).collect();
    protect(
        language.behavior(),
        text,
        Sources {
            paths: &paths,
            previous,
            ..Sources::default()
        },
    )
}

fn allowed(language: Language, before: &str, after: &str, protected: &Protected) -> bool {
    check(language.behavior(), None, protected, before, after).is_ok()
}

/// A text missing its last closer, the closer, and the fields: the block
/// left open, a field inside it, a field beside it.
fn unclosed() -> Vec<(Language, &'static str, &'static str, [&'static str; 3])> {
    vec![
        (
            Language::YAML,
            "name: x\nweb: {listen: 80\n",
            "}\n",
            ["/web", "/web/listen", "/name"],
        ),
        (
            Language::JSON,
            "{\n  \"name\": \"x\",\n  \"web\": {\n    \"listen\": 80\n",
            "  }\n}\n",
            ["/web", "/web/listen", "/name"],
        ),
        (
            Language::HCL,
            "name = \"x\"\nweb {\n  listen = 80\n",
            "}\n",
            ["/web", "/web/listen", "/name"],
        ),
        (
            Language::XML,
            "<name>x</name>\n<web>\n  <listen>80</listen>\n",
            "</web>\n",
            ["/web", "/web/listen", "/name"],
        ),
        (
            Language::XML,
            "<root>\n  <web>\n    <listen>80</listen>\n  </web>\n  <name>x</name>\n",
            "</root>\n",
            ["/root/web", "/root/web/listen", "/root/name"],
        ),
        (
            Language::JSON,
            "{\n  \"web\": {\n    \"listen\": 80\n  },\n  \"name\": \"x\"\n",
            "}\n",
            ["/web", "/web/listen", "/name"],
        ),
    ]
}

#[test]
fn closing_what_the_host_left_open_goes_through_in_any_arrival_order() {
    let mut failures = Vec::new();
    for (language, text, closer, fields) in unclosed() {
        let repaired = format!("{text}{closer}");
        assert!(
            codec::read(language.behavior(), text, None).is_err(),
            "{language}: {text:?} parses"
        );
        assert!(
            codec::read(language.behavior(), &repaired, None).is_ok(),
            "{language}: {repaired:?}"
        );
        for path in fields.iter().chain(&["/ghost"]) {
            let given = locks(language, text, &[path], None);
            let unlocked = locks(language, text, &[], None);
            let later = locks(language, text, &[path], Some(&unlocked));
            for (protected, order) in [(&given, "with the text"), (&later, "after the text")] {
                if !allowed(language, text, &repaired, protected) {
                    failures.push(format!(
                        "{language} lock {path} given {order}: closing {text:?} refused"
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{failures:#?}",
        failures.len()
    );
}

/// The locked value is a span while the text does not parse: changing it
/// is refused, with the field's reason.
#[test]
fn changing_a_locked_value_while_the_text_does_not_parse_is_refused() {
    let mut failures = Vec::new();
    for (language, text, _, fields) in unclosed() {
        let lock = fields[1];
        let protected = locks(language, text, &[lock], None);
        let shown = text.replacen("80", "81", 1);
        match check(language.behavior(), None, &protected, text, &shown) {
            Ok(()) => failures.push(format!("{language}: {lock} changed: {shown:?}")),
            Err(blocked) => {
                let message = blocked.message(&crate::EditorStrings::default());
                if !message.contains("read-only") {
                    failures.push(format!(
                        "{language}: the message {message:?} does not say why"
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{failures:#?}",
        failures.len()
    );
}
