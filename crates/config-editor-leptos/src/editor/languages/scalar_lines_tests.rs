//! `ConfigLanguage::edits_one_scalar` says yes only when it is so, in
//! every language: every character typed at every place of a text full of
//! constructs over lines (comments, multi-line strings and containers, YAML
//! anchors and aliases), and every character deleted, that it takes for a
//! scalar edit leaves a text that parses with the same structure and at
//! most one scalar changed (an alias reading an anchored one would change
//! two). And it says yes to plain typing in a value, which keeps locks
//! away from it free (`protect::reach`).

use serde_json::Value;

use super::Language;

/// A text of each language with constructs over lines.
fn sample(language: Language) -> &'static str {
    match language.label() {
        "YAML" => concat!(
            "x-base: &base\n  restart: always # policy\n  level: info\n",
            "web:\n  <<: *base\n  image: \"nginx:1.25\"\n  name: web one\n",
            "  note: |\n    port: 80\n    keep it\n  plain: first\n    second\n",
            "  quoted: \"a\n    b: c\"\n  flow: {a: 1,\n    b: two}\n",
            "  list:\n    - x: 1\n      y: why\n    - plain item\n  copy: &one solo\n",
            "  again: *one\n  empty:\n  single: 'x\n    y: z'\n  seq: [a,\n    b: c,\n    d]\n",
            "  url: postgres://db:5432/app\n",
        ),
        "JSON" => concat!(
            "{\n  \"web\": {\n    \"image\": \"nginx:1.25\",\n    \"port\": 80,\n",
            "    \"tags\": [\n      \"a\",\n      \"b\"\n    ],\n    \"on\": true,\n",
            "    \"esc\": \"a\\\"b\"\n  }\n}\n",
        ),
        "TOML" => concat!(
            "# top\n[web]\nimage = \"nginx:1.25\" # pinned\nport = 80\n",
            "text = \"\"\"\nkey = \"v\"\nmore\"\"\"\nlit = 'C:\\path'\n",
            "ports = [\n  80,\n  443,\n]\nwhen = 1979-05-27\n",
            "[web.inner]\nname = \"x\"\nraw = \'\'\'\nk = 1\nend\'\'\'\n",
        ),
        "HCL" => concat!(
            "/* a = 1\n   b = 2 */\nweb {\n  image = \"nginx:1.25\"\n  port  = 80\n",
            "  text = <<EOT\n  key = \"v\"\nEOT\n  tags = {\n    Name = \"web\"\n  }\n",
            "  list = [\n    1,\n    2,\n  ]\n  ref = \"${var.x}\" # c\n  multi = \"one\n  two = 2\n  three\"\n}\n",
        ),
        _ => concat!(
            "<root>\n  <!-- a comment\n  <a>1</a>\n  -->\n  <web>\n",
            "    <image>nginx:1.25</image>\n    <port>80</port>\n",
            "    <data><![CDATA[\n    <k>v</k>\n    ]]></data>\n",
            "    <tag kind=\"x\">t</tag>\n    <text>\n      long\n    </text>\n  </web>\n</root>\n",
        ),
    }
}

/// The scalars `before` and `after` differ in, when they have the same
/// structure (`None` otherwise).
fn changed_scalars(before: &Value, after: &Value) -> Option<usize> {
    match (before, after) {
        (Value::Object(old), Value::Object(new)) => {
            if !old.keys().eq(new.keys()) {
                return None;
            }
            old.values()
                .zip(new.values())
                .map(|(old, new)| changed_scalars(old, new))
                .sum()
        }
        (Value::Array(old), Value::Array(new)) => {
            if old.len() != new.len() {
                return None;
            }
            old.iter()
                .zip(new)
                .map(|(old, new)| changed_scalars(old, new))
                .sum()
        }
        (Value::Object(_) | Value::Array(_), _) | (_, Value::Object(_) | Value::Array(_)) => None,
        (old, new) => Some(usize::from(old != new)),
    }
}

const TYPED: &str = "aZ7 \"'\\#&*!|>,[]{}:-=<$%/()@?;.\t";

#[test]
fn an_edit_taken_for_one_scalar_changes_one_scalar_in_every_language() {
    let mut wrong = Vec::new();
    for &language in Language::ALL {
        let behavior = language.behavior();
        let text = sample(language);
        let document = behavior.parse(text).expect("the sample parses");
        let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
        for (at, character) in text.char_indices() {
            edits.push((at..at + character.len_utf8(), String::new()));
            edits.extend(TYPED.chars().map(|typed| (at..at, typed.to_string())));
        }
        let mut taken = 0;
        for (range, inserted) in edits {
            if !behavior.edits_one_scalar(text, range.clone(), &inserted) {
                continue;
            }
            taken += 1;
            let after = format!("{}{inserted}{}", &text[..range.start], &text[range.end..]);
            let changed = behavior
                .parse(&after)
                .ok()
                .and_then(|edited| changed_scalars(&document, &edited));
            if !matches!(changed, Some(0 | 1)) {
                wrong.push(format!(
                    "{language}: {range:?} -> {inserted:?}: {changed:?}\n{after}"
                ));
            }
        }
        assert!(taken > 100, "{language}: only {taken} edits taken");
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Typing a letter, a digit or a space inside a value, or deleting one, is
/// taken: the plain typing a lock elsewhere must not slow.
#[test]
fn typing_inside_a_value_is_one_scalar_edit_in_every_language() {
    for &language in Language::ALL {
        let behavior = language.behavior();
        let text = sample(language);
        let at = text.find("nginx").expect("a value") + 2;
        for typed in ["q", "7", " ", "."] {
            assert!(
                behavior.edits_one_scalar(text, at..at, typed),
                "{language}: typing {typed:?}"
            );
        }
        assert!(
            behavior.edits_one_scalar(text, at..at + 1, ""),
            "{language}: a deletion"
        );
        let port = text.find("80").expect("a number");
        assert!(
            behavior.edits_one_scalar(text, port..port, "1"),
            "{language}: a number"
        );
    }
}

/// What a scalar edit may not be: in a key, or making the value a
/// container or a comment, or in a value a YAML alias reads.
#[test]
fn edits_changing_more_than_a_scalar_are_not_taken() {
    let yaml = Language::YAML.behavior();
    let text = sample(Language::YAML);
    let in_anchored = text.find("info").expect("anchored");
    assert!(!yaml.edits_one_scalar(text, in_anchored..in_anchored, "x"));
    let solo = text.find("solo").expect("anchored scalar");
    assert!(!yaml.edits_one_scalar(text, solo..solo, "x"));
    let key = text.find("image").expect("a key");
    assert!(!yaml.edits_one_scalar(text, key..key, "x"));
    let plain = text.find("first").expect("a multi-line plain scalar");
    assert!(!yaml.edits_one_scalar(text, plain..plain, "x"));
    let empty = text.find("empty:").expect("an empty value") + "empty:".len();
    assert!(yaml.edits_one_scalar(text, empty..empty, " x"));
    let json = Language::JSON.behavior();
    let text = sample(Language::JSON);
    let port = text.find("80").expect("a number");
    assert!(!json.edits_one_scalar(text, port..port, "a"));
    assert!(!json.edits_one_scalar(text, port..port + 2, "[1]"));
}
