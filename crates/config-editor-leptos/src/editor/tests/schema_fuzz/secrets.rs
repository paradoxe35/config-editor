//! No problem message, hover card or suggestion ever holds a secret's
//! value (a field `writeOnly` or `x-secret` marks), at any depth: secret
//! values are drawn distinct, beside wrong values all around them.

use serde_json::{Value, json};

use super::documents::{SECRET, Shape, document, resolve};
use super::random::Rng;
use super::reference::compile;
use super::runner::{Case, Property, Verdict, failure, language, run};
use super::schemas::{Draw, PATTERN, schema};
use super::suggestions::with_entry;
use crate::editor::{Language, TextPosition};

pub(super) fn draw(rng: &mut Rng, index: usize) -> Case {
    let draw = Draw {
        marks: true,
        docs: rng.chance(40),
        depth: 2 + rng.below(2),
    };
    let schema = schema(rng, draw);
    let shape = Shape {
        wrong: 5 + rng.below(20),
        secrets: true,
        every_key: true,
    };
    let doc = document(rng, &schema, shape);
    Case {
        language: language(index),
        pick: 0,
        schema,
        doc,
        other: Value::Null,
    }
}

/// The secret values of `value`: strings a node marking a secret
/// (`writeOnly`, `x-secret`) applies to, through any option.
fn secrets(root: &Value, node: &Value, value: &Value, depth: usize, out: &mut Vec<String>) {
    let Some(map) = node.as_object().filter(|_| depth < 40) else {
        return;
    };
    let marked =
        map.get("writeOnly") == Some(&json!(true)) || map.get("x-secret") == Some(&json!(true));
    if let Value::String(text) = value
        && marked
        && text.starts_with(SECRET)
    {
        out.push(text.clone());
    }
    if let Some(target) = map
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|at| resolve(root, at))
    {
        secrets(root, target, value, depth + 1, out);
    }
    for keyword in ["allOf", "anyOf", "oneOf"] {
        for option in map
            .get(keyword)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            secrets(root, option, value, depth + 1, out);
        }
    }
    for keyword in ["then", "else"] {
        if let Some(branch) = map.get(keyword) {
            secrets(root, branch, value, depth + 1, out);
        }
    }
    for dependent in map
        .get("dependentSchemas")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|all| all.values())
    {
        secrets(root, dependent, value, depth + 1, out);
    }
    match value {
        Value::Object(children) => {
            for (key, child) in children {
                let named = map.get("properties").and_then(|all| all.get(key));
                let patterned = key
                    .starts_with("x_")
                    .then(|| {
                        map.get("patternProperties")
                            .and_then(|all| all.get(PATTERN))
                    })
                    .flatten();
                if let Some(schema) = named {
                    secrets(root, schema, child, depth + 1, out);
                }
                if let Some(schema) = patterned {
                    secrets(root, schema, child, depth + 1, out);
                }
                if named.is_none()
                    && patterned.is_none()
                    && let Some(schema) = map.get("additionalProperties")
                {
                    secrets(root, schema, child, depth + 1, out);
                }
            }
        }
        Value::Array(items) => {
            let prefix = map.get("prefixItems").and_then(Value::as_array);
            for (index, item) in items.iter().enumerate() {
                let schema = match prefix.and_then(|prefix| prefix.get(index)) {
                    Some(schema) => Some(schema),
                    None if prefix.is_none_or(|prefix| index >= prefix.len()) => map.get("items"),
                    None => None,
                };
                if let Some(schema) = schema {
                    secrets(root, schema, item, depth + 1, out);
                }
            }
        }
        _ => {}
    }
}

pub(super) fn check(case: &Case) -> Verdict {
    let Some(both) = compile(&case.schema) else {
        return Verdict::Skipped;
    };
    let language = case.language;
    let Some(text) = both.written(language, &case.doc) else {
        return Verdict::Skipped;
    };
    // XML writes no list of lists, nor an empty list in one: the items of
    // an inner list are items of the outer one, so a value after them is
    // read at another index (`[[], "s"]` reads `["s"]`), where the schema
    // need not mark it. The secrets are those the schema marks where the
    // editor reads the text.
    let read = (language == Language::XML)
        .then(|| both.read(language, &text))
        .flatten();
    let mut held = Vec::new();
    secrets(
        &case.schema,
        &case.schema,
        read.as_ref().unwrap_or(&case.doc),
        0,
        &mut held,
    );
    if held.is_empty() {
        return Verdict::Skipped;
    }
    let leaked = |shown: &str| {
        held.iter()
            .find(|secret| shown.contains(secret.as_str()))
            .cloned()
    };
    let seen = |where_: &str, shown: &str| format!("{language}:\n{text}{where_} shows: {shown}");
    for problem in both.editor.validate(language, &text) {
        if let Some(secret) = leaked(&problem.message) {
            // Quoted inside a list or object, or alone.
            let before = &problem.message[..problem.message.find(&secret).unwrap_or(0)];
            let class = if before.contains(['{', '[']) {
                "a problem message quotes a value holding a secret"
            } else {
                "a problem message holds a secret"
            };
            return Verdict::Fails(failure(class, seen("a problem", &problem.message)));
        }
    }
    for (index, line) in text.lines().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        for column in 0..chars.len() {
            if column > 0 && chars[column - 1] != ' ' && column + 1 != chars.len() {
                continue;
            }
            let position = TextPosition {
                line: index + 1,
                column: column + 1,
            };
            if let Some(docs) = both.editor.hover(language, &text, position) {
                let shown = format!("{docs:?}");
                if leaked(&shown).is_some() {
                    return Verdict::Fails(failure(
                        "a hover card holds a secret",
                        seen(
                            &format!("hover at {}:{}", position.line, position.column),
                            &shown,
                        ),
                    ));
                }
            }
        }
    }
    // Suggestions at every line's end, and for a new root key.
    let mut carets: Vec<(String, usize)> = text
        .match_indices('\n')
        .map(|(at, _)| (text.clone(), at))
        .collect();
    carets.push(with_entry(language, &text, None));
    for (source, caret) in carets {
        let caret = source[..caret].encode_utf16().count();
        for offered in both.editor.suggestions(language, &source, caret) {
            let shown = format!("{:?} ({})", offered.label(), offered.detail());
            if leaked(&shown).is_some() {
                return Verdict::Fails(failure(
                    "a suggestion holds a secret",
                    seen("a suggestion", &shown),
                ));
            }
        }
    }
    Verdict::Holds
}

#[test]
fn no_message_hover_or_suggestion_shows_a_secret() {
    run(&Property {
        name: "secrets",
        check_path: "super::secrets::check",
        cases: 600,
        shrink_budget: 1500,
        draw,
        check,
    });
}
