//! A break on the locked line itself, from the host: adding the closer that
//! completes the line (a quote, a bracket, `>`, an end tag, the missing
//! `:`) makes the field read exactly as the host wrote it, so it goes
//! through (I2); writing anything else there with it, or deleting the line,
//! is refused naming the field (I1).

use super::super::Language;
use super::super::codec;
use super::super::paths::FieldPath;
use super::guard::check;
use super::locks::{Protected, Sources, protect};
use crate::EditorStrings;

const LOCK: &str = "/b3";

fn locks(language: Language, text: &str, later: bool) -> Protected {
    let paths = [FieldPath::from(LOCK)];
    let unlocked = later.then(|| protect(language.behavior(), text, Sources::default()));
    protect(
        language.behavior(),
        text,
        Sources {
            paths: &paths,
            previous: unlocked.as_ref(),
            ..Sources::default()
        },
    )
}

/// The host's text broken on the locked line, the text with the closer
/// added there, and texts writing something else with it.
fn cases() -> Vec<(Language, &'static str, &'static str, Vec<&'static str>)> {
    vec![
        (
            Language::YAML,
            "a: 1\nb3: \"x\nc: 2\n",
            "a: 1\nb3: \"x\"\nc: 2\n",
            vec!["a: 1\nb3: \"xy\"\nc: 2\n", "a: 1\nb3: \"\"\nc: 2\n"],
        ),
        (
            Language::YAML,
            "a: 1\nb3: [1, 2\nc: 2\n",
            "a: 1\nb3: [1, 2]\nc: 2\n",
            vec![
                "a: 1\nb3: [1, 3]\nc: 2\n",
                "a: 1\nb3: [\"1\", 2]\nc: 2\n",
                "a: 1\nb3: [1]\nc: 2\n",
            ],
        ),
        (
            Language::JSON,
            "{\n  \"a\": 1,\n  \"b3\" \"x\",\n  \"c\": 2\n}\n",
            "{\n  \"a\": 1,\n  \"b3\": \"x\",\n  \"c\": 2\n}\n",
            vec!["{\n  \"a\": 1,\n  \"b3\": \"y\",\n  \"c\": 2\n}\n"],
        ),
        (
            Language::JSON,
            "{\n  \"a\": 1,\n  \"b3\": \"x,\n  \"c\": 2\n}\n",
            "{\n  \"a\": 1,\n  \"b3\": \"x\",\n  \"c\": 2\n}\n",
            vec!["{\n  \"a\": 1,\n  \"b3\": \"xz\",\n  \"c\": 2\n}\n"],
        ),
        (
            Language::JSON,
            "{\n  \"a\": 1,\n  \"b3\": [1, 2,\n  \"c\": 2\n}\n",
            "{\n  \"a\": 1,\n  \"b3\": [1, 2],\n  \"c\": 2\n}\n",
            vec![
                "{\n  \"a\": 1,\n  \"b3\": [1, 5],\n  \"c\": 2\n}\n",
                "{\n  \"a\": 1,\n  \"b3\": [\"1\", 2],\n  \"c\": 2\n}\n",
            ],
        ),
        (
            Language::TOML,
            "a = 1\nb3 = \"x\nc = 2\n",
            "a = 1\nb3 = \"x\"\nc = 2\n",
            vec!["a = 1\nb3 = \"xx\"\nc = 2\n"],
        ),
        (
            Language::TOML,
            "a = 1\nb3 = [1, 2\nc = 2\n",
            "a = 1\nb3 = [1, 2]\nc = 2\n",
            vec!["a = 1\nb3 = [1, 2, 3]\nc = 2\n"],
        ),
        (
            Language::HCL,
            "a = 1\nb3 = \"x\nc = 2\n",
            "a = 1\nb3 = \"x\"\nc = 2\n",
            vec!["a = 1\nb3 = \"q\"\nc = 2\n"],
        ),
        (
            Language::HCL,
            "a = 1\nb3 {\n  k = 1\nc = 2\n",
            "a = 1\nb3 {\n  k = 1\n}\nc = 2\n",
            vec!["a = 1\nb3 {\n  k = 2\n}\nc = 2\n"],
        ),
        (
            Language::XML,
            "<a>1</a>\n<b3>x</b3\n<c>2</c>\n",
            "<a>1</a>\n<b3>x</b3>\n<c>2</c>\n",
            vec!["<a>1</a>\n<b3>y</b3>\n<c>2</c>\n"],
        ),
        (
            Language::XML,
            "<a>1</a>\n<b3>x\n<c>2</c>\n",
            "<a>1</a>\n<b3>x</b3>\n<c>2</c>\n",
            vec!["<a>1</a>\n<b3>xx</b3>\n<c>2</c>\n"],
        ),
    ]
}

/// The text without the line naming the locked field.
fn without_locked_line(text: &str) -> String {
    let at = text.find("b3").unwrap();
    let start = text[..at].rfind('\n').map_or(0, |found| found + 1);
    let end = text[at..]
        .find('\n')
        .map_or(text.len(), |found| at + found + 1);
    format!("{}{}", &text[..start], &text[end..])
}

#[test]
fn closing_the_locked_line_goes_through_and_changing_it_is_refused_by_name() {
    let strings = EditorStrings::default();
    let mut failures = Vec::new();
    for (language, broken, closed, changed) in cases() {
        let behavior = language.behavior();
        assert!(codec::read(behavior, broken, None).is_err(), "{broken:?}");
        assert!(codec::read(behavior, closed, None).is_ok(), "{closed:?}");
        for later in [false, true] {
            let protected = locks(language, broken, later);
            let order = if later { "after" } else { "with" };
            if let Err(blocked) = check(behavior, None, &protected, broken, closed) {
                failures.push(format!(
                    "{language} lock {order} the text: closing {broken:?} refused: {}",
                    blocked.message(&strings)
                ));
            }
            let deleted = without_locked_line(broken);
            let mut refusals = changed.clone();
            if codec::read(behavior, &deleted, None).is_ok() {
                refusals.push(&deleted);
            }
            for after in refusals {
                match check(behavior, None, &protected, broken, after) {
                    Ok(()) => failures.push(format!(
                        "{language} lock {order} the text: {broken:?} -> {after:?} allowed"
                    )),
                    Err(blocked) => {
                        let message = blocked.message(&strings);
                        if !message.contains(LOCK) {
                            failures.push(format!(
                                "{language} lock {order} the text: {after:?} refused with \
                                 {message:?}, not naming {LOCK}"
                            ));
                        }
                    }
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
