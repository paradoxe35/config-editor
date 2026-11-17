//! A schema's text, spelled at random in JSON (compact, pretty, keys in
//! another order) or YAML (block and flow, plain and quoted scalars),
//! compiles to the same schema whatever the spelling; the same text with
//! one key written twice is refused.

use serde_json::{Map, Value};

use super::random::Rng;
use super::runner::{Case, Property, Verdict, failure, language, run};
use super::schemas::{Draw, schema};
use crate::editor::schema::{Schema, build_schema};

pub(super) fn draw(rng: &mut Rng, index: usize) -> Case {
    let draw = Draw {
        marks: rng.chance(30),
        docs: rng.chance(50),
        depth: 2 + rng.below(2),
    };
    Case {
        language: language(index),
        pick: rng.below(1 << 16),
        schema: schema(rng, draw),
        doc: Value::Null,
        other: Value::Null,
    }
}

/// `value` with each object's keys in another order.
fn reordered(value: &Value, rng: &mut Rng) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            for index in (1..entries.len()).rev() {
                entries.swap(index, rng.below(index + 1));
            }
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, child)| (key.clone(), reordered(child, rng)))
                    .collect::<Map<_, _>>(),
            )
        }
        Value::Array(items) => {
            Value::Array(items.iter().map(|item| reordered(item, rng)).collect())
        }
        other => other.clone(),
    }
}

/// A scalar or key YAML may write plain: one reading back as the same
/// text.
fn plain(text: &str) -> bool {
    let reserved = [
        "true", "false", "null", "yes", "no", "on", "off", "y", "n", "~",
    ];
    text.chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_' || first == '$')
        && text
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_-$. ".contains(character))
        && !text.ends_with(' ')
        && !reserved.contains(&text.to_ascii_lowercase().as_str())
}

fn yaml_text(text: &str, rng: &mut Rng) -> String {
    match rng.below(3) {
        0 if plain(text) => text.to_string(),
        1 if !text.contains(['\n', '\\']) => format!("'{}'", text.replace('\'', "''")),
        _ => Value::String(text.to_string()).to_string(),
    }
}

fn yaml_scalar(value: &Value, rng: &mut Rng) -> String {
    match value {
        Value::String(text) => yaml_text(text, rng),
        Value::Null if rng.chance(50) => "~".to_string(),
        other => other.to_string(),
    }
}

/// `value` written as a YAML flow container or scalar.
fn yaml_flow(value: &Value, rng: &mut Rng) -> String {
    match value {
        Value::Object(map) => {
            let entries: Vec<String> = map
                .iter()
                .map(|(key, child)| format!("{}: {}", yaml_text(key, rng), yaml_flow(child, rng)))
                .collect();
            format!("{{{}}}", entries.join(", "))
        }
        Value::Array(items) => {
            let items: Vec<String> = items.iter().map(|item| yaml_flow(item, rng)).collect();
            format!("[{}]", items.join(", "))
        }
        other => yaml_scalar(other, rng),
    }
}

/// The lines of a non-empty container's content at `indent`, each
/// container in it written as a block or in flow at random; with one
/// key written twice when `twice` is still set (then cleared).
fn yaml_block(value: &Value, indent: usize, rng: &mut Rng, twice: &mut bool) -> Vec<String> {
    let pad = " ".repeat(indent);
    let mut lines = Vec::new();
    let nested = |child: &Value, rng: &mut Rng| match child {
        Value::Object(map) if !map.is_empty() => !rng.chance(25),
        Value::Array(items) if !items.is_empty() => !rng.chance(25),
        _ => false,
    };
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let key = yaml_text(key, rng);
                if nested(child, rng) {
                    lines.push(format!("{pad}{key}:"));
                    lines.extend(yaml_block(child, indent + 2, rng, twice));
                } else {
                    lines.push(format!("{pad}{key}: {}", yaml_flow(child, rng)));
                }
                if *twice && rng.chance(30) {
                    *twice = false;
                    lines.push(format!("{pad}{key}: 1"));
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                if nested(item, rng) {
                    let mut inner = yaml_block(item, indent + 2, rng, twice);
                    if item.is_object() && rng.chance(60) {
                        inner[0] = format!("{pad}- {}", inner[0].trim_start());
                    } else {
                        lines.push(format!("{pad}-"));
                    }
                    lines.extend(inner);
                } else {
                    lines.push(format!("{pad}- {}", yaml_flow(item, rng)));
                }
            }
        }
        other => lines.push(format!("{pad}{}", yaml_scalar(other, rng))),
    }
    lines
}

/// A random spelling of `schema`, and whether it writes a key twice (when
/// `twice` asks it, and the schema has a place for it).
fn spelling(schema: &Value, rng: &mut Rng, twice: bool) -> (String, bool) {
    let value = if rng.chance(50) {
        reordered(schema, rng)
    } else {
        schema.clone()
    };
    let mut left = twice;
    let text = match rng.below(4) {
        0 | 1 => {
            let mut lines;
            loop {
                left = twice;
                lines = yaml_block(&value, 0, rng, &mut left);
                if !left || !twice {
                    break;
                }
            }
            lines.join("\n") + "\n"
        }
        2 => {
            let text = serde_json::to_string_pretty(&value).unwrap();
            if twice {
                left = false;
                text.replacen("{\n", "{\n  \"type\": \"object\",\n", 1)
            } else {
                text
            }
        }
        _ => {
            let text = value.to_string();
            if twice {
                left = false;
                text.replacen('{', "{\"type\":\"object\",", 1)
            } else {
                text
            }
        }
    };
    (text, twice && !left)
}

pub(super) fn check(case: &Case) -> Verdict {
    let Ok(expected) = build_schema(&case.schema.to_string()) else {
        return Verdict::Skipped;
    };
    let mut rng = Rng::new(case.pick as u64);
    let (text, _) = spelling(&case.schema, &mut rng, false);
    match build_schema(&text) {
        Ok(spelled) if spelled.schema == expected.schema => {}
        Ok(spelled) => {
            return Verdict::Fails(failure(
                "a spelling compiles to another schema",
                format!(
                    "{text}\ncompiles to {}\nnot {}",
                    spelled.schema, expected.schema
                ),
            ));
        }
        Err(error) => {
            return Verdict::Fails(failure(
                "a spelling of a schema is refused",
                format!("{text}\nrefused: {error}"),
            ));
        }
    }
    // A key written twice. The root object is known to hold `type`: the
    // JSON spellings write it again in front.
    let (text, doubled) = spelling(&case.schema, &mut rng, true);
    if doubled && Schema::compile(&text).is_ok() {
        return Verdict::Fails(failure(
            "a schema text writing a key twice is accepted",
            text,
        ));
    }
    Verdict::Holds
}

#[test]
fn every_spelling_of_a_schema_compiles_alike_and_keys_written_twice_are_refused() {
    run(&Property {
        name: "schema_text",
        check_path: "super::schema_text::check",
        cases: 300,
        shrink_budget: 1500,
        draw,
        check,
    });
}
