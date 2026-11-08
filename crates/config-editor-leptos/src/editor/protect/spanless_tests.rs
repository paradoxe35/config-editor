//! Protected fields in a language that cannot say where a field is written
//! (only `parse` and `serialize`, no spans): every locked value still holds
//! (I1), checked by value on every edit whose text parses, and edits that
//! keep every locked value stay free (I2).

use serde_json::{Map, Value, json};

use super::super::language::{ConfigLanguage, LanguageInfo, ParseError};
use super::super::paths::FieldPath;
use super::guard::check;
use super::locks::{Protected, Sources, protect};
use super::save::locked_pointers;

/// `KEY=value` lines, `#` comments; nothing else.
struct Env;

impl ConfigLanguage for Env {
    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: "Env",
            icon: "E",
            extensions: &["env"],
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        let mut map = Map::new();
        for (index, line) in source.split('\n').enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(ParseError {
                    message: "expected KEY=value".to_string(),
                    line: Some(index + 1),
                    column: Some(1),
                });
            };
            let value = value.trim();
            let value = value
                .parse::<i64>()
                .map_or_else(|_| json!(value), |number| json!(number));
            map.insert(key.trim().to_string(), value);
        }
        Ok(Value::Object(map))
    }

    fn serialize(&self, value: &Value, _previous: Option<&str>) -> Result<String, String> {
        let Value::Object(map) = value else {
            return Err("not an object".to_string());
        };
        Ok(map
            .iter()
            .map(|(key, value)| match value {
                Value::String(text) => format!("{key}={text}\n"),
                other => format!("{key}={other}\n"),
            })
            .collect())
    }
}

const TEXT: &str = "# c\nID=srv-1\nNAME=a\nPORT=80\n";

fn schema() -> Value {
    json!({"type": "object", "properties": {
        "ID": {"type": "string", "readOnly": true},
        "PORT": {"type": "integer"}
    }})
}

fn locks(text: &str, previous: Option<&Protected>) -> Protected {
    let schema = schema();
    let paths = [FieldPath::from("/PORT")];
    protect(
        &Env,
        text,
        Sources {
            marking_schema: Some(&schema),
            reading_schema: Some(&schema),
            paths: &paths,
            previous,
            ..Sources::default()
        },
    )
}

fn allowed(protected: &Protected, before: &str, after: &str) -> bool {
    check(&Env, Some(&schema()), protected, before, after).is_ok()
}

#[test]
fn every_edit_changing_a_locked_value_is_refused_without_spans() {
    let protected = locks(TEXT, None);
    let cases = [
        (
            "type in a locked value",
            "# c\nID=srv-19\nNAME=a\nPORT=80\n",
        ),
        (
            "type in a host-locked value",
            "# c\nID=srv-1\nNAME=a\nPORT=809\n",
        ),
        ("delete a locked line", "# c\nNAME=a\nPORT=80\n"),
        ("rename a locked key", "# c\nXD=srv-1\nNAME=a\nPORT=80\n"),
        (
            "duplicate overriding",
            "# c\nID=srv-1\nNAME=a\nPORT=80\nID=evil\n",
        ),
        ("comment a locked line", "# c\n#ID=srv-1\nNAME=a\nPORT=80\n"),
        ("replace all", "ID=srv-2\nPORT=81\n"),
    ];
    let failures: Vec<&str> = cases
        .iter()
        .filter(|(_, after)| allowed(&protected, TEXT, after))
        .map(|(name, _)| *name)
        .collect();
    assert!(failures.is_empty(), "accepted: {failures:?}");
}

#[test]
fn edits_keeping_every_locked_value_stay_free_without_spans() {
    let protected = locks(TEXT, None);
    let cases = [
        ("edit a comment", "# comment\nID=srv-1\nNAME=a\nPORT=80\n"),
        ("edit a free value", "# c\nID=srv-1\nNAME=abc\nPORT=80\n"),
        ("add a key", "# c\nID=srv-1\nNAME=a\nPORT=80\nNEW=1\n"),
        ("delete a free line", "# c\nID=srv-1\nPORT=80\n"),
        ("comment a free line", "# c\nID=srv-1\n#NAME=a\nPORT=80\n"),
        (
            "space after a locked value",
            "# c\nID=srv-1 \nNAME=a\nPORT=80\n",
        ),
    ];
    let failures: Vec<&str> = cases
        .iter()
        .filter(|(_, after)| !allowed(&protected, TEXT, after))
        .map(|(name, _)| *name)
        .collect();
    assert!(failures.is_empty(), "refused: {failures:?}");
}

#[test]
fn a_text_broken_on_the_way_parses_again_only_with_the_locked_values() {
    let protected = locks(TEXT, None);
    let broken = "# c\nIDsrv-1\nNAME=a\nPORT=80\n";
    assert!(
        allowed(&protected, TEXT, broken),
        "breaking the text is free"
    );
    let held = locks(broken, Some(&protected));
    assert!(
        !allowed(&held, broken, "# c\nID=srv-2\nNAME=a\nPORT=80\n"),
        "repaired with another value"
    );
    assert!(
        !allowed(&held, broken, "# c\nNAME=a\nPORT=80\n"),
        "repaired by deleting the locked line"
    );
    assert!(
        allowed(&held, broken, TEXT),
        "repaired with the locked value"
    );
}

#[test]
fn the_save_check_names_the_fields_the_language_cannot_locate() {
    let schema = schema();
    let paths = [FieldPath::from("/PORT")];
    let mut pointers = locked_pointers(
        &Env,
        TEXT,
        Sources {
            marking_schema: Some(&schema),
            reading_schema: Some(&schema),
            paths: &paths,
            ..Sources::default()
        },
    );
    pointers.sort();
    assert_eq!(pointers, ["/ID", "/PORT"]);
}
