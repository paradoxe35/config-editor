//! The JSON Schema properties again, with the schema given through a
//! schema format: as an OpenAPI 3.1, OpenAPI 3.0 or Swagger 2.0 document
//! (its `$defs` the document's named schemas, 3.0 and Swagger keywords
//! where they differ), or split into files (each `$defs` entry a resource
//! the main file refers to by a relative `$ref`). A format only changes how
//! the schema is written, so validation, suggestions and fill must give
//! exactly what the plain schema gives: a check that fails through a
//! format and holds on the plain schema (or the other way round) is a
//! format's failure. A check failing alike on both is the plain property's
//! (its own suite runs and allows it).

use serde_json::{Map, Value, json};

use super::random::Rng;
use super::reference::Both;
use super::runner::{Case, Property, Verdict, failure, run};
use super::{fill, suggestions, validation};
use crate::editor::schema::{Schema, each_schema_mut};
use crate::editor::{SchemaFormat, SchemaInput, SchemaResources};

/// How a case writes its schema, chosen by `pick`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Written {
    OpenApi31,
    OpenApi30,
    Swagger,
    Files,
}

const WRITTEN: [Written; 4] = [
    Written::OpenApi31,
    Written::OpenApi30,
    Written::Swagger,
    Written::Files,
];

/// A schema in a format: its text, location, resources and options.
pub(super) struct Source {
    pub(super) format: SchemaFormat,
    pub(super) text: String,
    pub(super) location: Option<&'static str>,
    pub(super) resources: SchemaResources,
    pub(super) options: Map<String, Value>,
}

impl Source {
    pub(super) fn compile(&self) -> Result<Schema, String> {
        let mut input = SchemaInput::new(&self.text)
            .resources(&self.resources)
            .options(&self.options);
        if let Some(location) = self.location {
            input = input.at(location);
        }
        Schema::compile_as(self.format, &input).map_err(|error| error.0)
    }
}

/// `schema` written as `written`; `None` when that format cannot say it
/// (a 3.0 or Swagger schema has no type list but `[type, "null"]`, one
/// bound of each side, one example).
pub(super) fn source(schema: &Value, written: Written) -> Option<Source> {
    let mut root = schema.clone();
    let defs = match root.as_object_mut().and_then(|map| map.remove("$defs")) {
        Some(Value::Object(defs)) => defs,
        _ => Map::new(),
    };
    match written {
        Written::OpenApi31 | Written::OpenApi30 | Written::Swagger => {
            let (prefix, legacy) = match written {
                Written::OpenApi31 => ("#/components/schemas/", false),
                Written::OpenApi30 => ("#/components/schemas/", true),
                _ => ("#/definitions/", true),
            };
            // The root is one more named schema, under a name no
            // definition has.
            let mut name = "Root".to_string();
            while defs.contains_key(&name) {
                name.push('_');
            }
            let mut named = Map::new();
            for (key, def) in defs.into_iter().chain([(name.clone(), root)]) {
                let mut def = def;
                if !refs_to(&mut def, &|target| format!("{prefix}{target}"))
                    || (legacy && !as_legacy(&mut def))
                {
                    return None;
                }
                named.insert(key, def);
            }
            let document = match written {
                Written::OpenApi31 => {
                    json!({"openapi": "3.1.0", "components": {"schemas": named}})
                }
                Written::OpenApi30 => {
                    json!({"openapi": "3.0.3", "components": {"schemas": named}})
                }
                _ => json!({"swagger": "2.0", "definitions": named}),
            };
            let mut options = Map::new();
            options.insert("schema".into(), json!(name));
            Some(Source {
                format: SchemaFormat::OPENAPI,
                text: serde_json::to_string_pretty(&document).ok()?,
                location: None,
                resources: SchemaResources::new(),
                options,
            })
        }
        Written::Files => {
            // Each definition in a file of its own beside the others (by
            // index: a name needs no escaping); the main file one folder
            // up. A definition refers to itself by its own `#`.
            let files: Map<String, Value> = defs
                .keys()
                .enumerate()
                .map(|(index, key)| (key.clone(), json!(format!("d{index}.json"))))
                .collect();
            let file = |key: &str| files.get(key).and_then(Value::as_str).map(str::to_string);
            let mut resources = SchemaResources::new();
            for (key, def) in defs.clone() {
                let mut def = def;
                let own = file(&key)?;
                let mut ok = true;
                ok &= refs_to(&mut def, &|target| match file(target) {
                    Some(name) if name == own => "#".to_string(),
                    Some(name) => name,
                    None => format!("#/$defs/{target}"),
                });
                if !ok {
                    return None;
                }
                resources.insert(format!("schemas/defs/{own}"), def.to_string());
            }
            if !refs_to(&mut root, &|target| match file(target) {
                Some(name) => format!("defs/{name}"),
                None => format!("#/$defs/{target}"),
            }) {
                return None;
            }
            Some(Source {
                format: SchemaFormat::JSON_SCHEMA,
                text: root.to_string(),
                location: Some("schemas/main.json"),
                resources,
                options: Map::new(),
            })
        }
    }
}

/// Rewrites each `$ref` to a definition (`#/$defs/<name>`) in `schema` to
/// `to(name)`; false when a `$ref` points elsewhere (no format here says
/// such a reference the same way).
fn refs_to(schema: &mut Value, to: &dyn Fn(&str) -> String) -> bool {
    let mut ok = true;
    each_schema_mut(schema, &mut |map| {
        if let Some(Value::String(reference)) = map.get_mut("$ref") {
            match reference
                .strip_prefix("#/$defs/")
                .filter(|name| !name.contains('/') && !name.contains('~'))
            {
                Some(name) => *reference = to(name),
                None => ok = false,
            }
        }
    });
    ok
}

/// `schema` in OpenAPI 3.0 / Swagger keywords (the inverse of what the
/// format rewrites); false when they cannot say it.
fn as_legacy(schema: &mut Value) -> bool {
    let mut ok = true;
    each_schema_mut(schema, &mut |map| {
        if map.contains_key("nullable") {
            ok = false;
        }
        if let Some(Value::Array(types)) = map.get("type") {
            let named: Vec<&Value> = types.iter().filter(|kind| *kind != "null").collect();
            if named.len() == 1 && types.len() == 2 {
                let kind = named[0].clone();
                map.insert("type".into(), kind);
                map.insert("nullable".into(), json!(true));
            } else {
                ok = false;
            }
        } else if map.get("type") == Some(&json!("null")) {
            ok = false;
        }
        for (flag, bound) in [
            ("exclusiveMinimum", "minimum"),
            ("exclusiveMaximum", "maximum"),
        ] {
            if let Some(value) = map.get(flag).cloned() {
                if map.contains_key(bound) || !value.is_number() {
                    ok = false;
                } else {
                    map.insert(bound.into(), value);
                    map.insert(flag.into(), json!(true));
                }
            }
        }
        match map.remove("examples") {
            Some(Value::Array(mut examples)) if examples.len() == 1 => {
                map.insert("example".into(), examples.remove(0));
            }
            Some(_) => ok = false,
            None => {}
        }
    });
    ok
}

/// The editor's schema from `case.schema` written as `pick` says, with the
/// reference on the plain schema; `Err` when the format refuses what the
/// plain schema compiles.
fn wrapped(case: &Case) -> Result<Option<(Both, Written)>, Verdict> {
    let mut written = WRITTEN[case.pick % WRITTEN.len()];
    let Ok(reference) = jsonschema::validator_for(&case.schema) else {
        return Ok(None);
    };
    if Schema::compile(&case.schema.to_string()).is_err() {
        return Ok(None);
    }
    // A schema 3.0 or Swagger cannot say is written as OpenAPI 3.1.
    let source = match source(&case.schema, written) {
        Some(source) => source,
        None => {
            written = Written::OpenApi31;
            match source(&case.schema, written) {
                Some(source) => source,
                None => return Ok(None),
            }
        }
    };
    match source.compile() {
        Ok(editor) => Ok(Some((Both { editor, reference }, written))),
        Err(error) => Err(Verdict::Fails(failure(
            format!("{written:?}: a schema the plain JSON Schema compiles is refused"),
            format!("{error}\n{}", source.text),
        ))),
    }
}

/// `plain` and its check through a format agree: a class only the format
/// shows (or only the plain schema) is the format's.
fn agree(case: &Case, plain: fn(&Case) -> Verdict, with: fn(&Both, &Case) -> Verdict) -> Verdict {
    let (both, written) = match wrapped(case) {
        Ok(Some(found)) => found,
        Ok(None) => return Verdict::Skipped,
        Err(verdict) => return verdict,
    };
    let through = with(&both, case);
    let plain = plain(case);
    match (plain, through) {
        (Verdict::Fails(plain), Verdict::Fails(through)) if plain.class == through.class => {
            Verdict::Skipped
        }
        (_, Verdict::Fails(through)) => Verdict::Fails(failure(
            format!("{written:?}: {}", through.class),
            through.detail,
        )),
        (Verdict::Fails(plain), _) => Verdict::Fails(failure(
            format!(
                "{written:?}: holds where the plain schema fails ({})",
                plain.class
            ),
            plain.detail,
        )),
        (Verdict::Skipped, _) | (_, Verdict::Skipped) => Verdict::Skipped,
        (Verdict::Holds, Verdict::Holds) => Verdict::Holds,
    }
}

/// A case of `draw`, its `pick` (the written form, and the plain check's
/// own choice where it has one) drawn anew.
fn with_pick(draw: fn(&mut Rng, usize) -> Case, rng: &mut Rng, index: usize) -> Case {
    let mut case = draw(rng, index);
    case.pick = rng.below(64);
    case
}

pub(super) fn validation_check(case: &Case) -> Verdict {
    agree(case, validation::check, validation::check_with)
}

pub(super) fn suggestions_check(case: &Case) -> Verdict {
    agree(case, suggestions::check, suggestions::check_with)
}

pub(super) fn fill_check(case: &Case) -> Verdict {
    agree(case, fill::check, fill::check_with)
}

fn validation_draw(rng: &mut Rng, index: usize) -> Case {
    with_pick(validation::draw, rng, index)
}

fn suggestions_draw(rng: &mut Rng, index: usize) -> Case {
    with_pick(suggestions::draw, rng, index)
}

fn fill_draw(rng: &mut Rng, index: usize) -> Case {
    with_pick(fill::draw, rng, index)
}

#[test]
fn a_schema_through_openapi_or_files_validates_as_the_plain_schema() {
    run(&Property {
        name: "formats_validation",
        check_path: "super::formats::validation_check",
        cases: 300,
        shrink_budget: 1500,
        draw: validation_draw,
        check: validation_check,
    });
}

#[test]
fn a_schema_through_openapi_or_files_suggests_as_the_plain_schema() {
    run(&Property {
        name: "formats_suggestions",
        check_path: "super::formats::suggestions_check",
        cases: 150,
        shrink_budget: 1000,
        draw: suggestions_draw,
        check: suggestions_check,
    });
}

#[test]
fn a_schema_through_openapi_or_files_fills_as_the_plain_schema() {
    run(&Property {
        name: "formats_fill",
        check_path: "super::formats::fill_check",
        cases: 150,
        shrink_budget: 1000,
        draw: fill_draw,
        check: fill_check,
    });
}

/// Every way of writing a schema here compiles through its format and
/// keeps its `$ref`s working (a recursive definition, one definition
/// referring to another), so the properties above reach every format.
#[test]
fn each_written_form_of_a_schema_with_definitions_compiles() {
    let schema = json!({
        "type": "object",
        "properties": {"a": {"$ref": "#/$defs/node"}, "b": {"$ref": "#/$defs/leaf"}},
        "$defs": {
            "node": {"type": "object", "properties": {"next": {"$ref": "#/$defs/node"},
                "leaf": {"$ref": "#/$defs/leaf"}}},
            "leaf": {"type": ["integer", "null"], "minimum": 2, "examples": [3]},
        },
    });
    for written in WRITTEN {
        let source = source(&schema, written).unwrap_or_else(|| panic!("{written:?} says it"));
        let editor = source
            .compile()
            .unwrap_or_else(|error| panic!("{written:?}: {error}\n{}", source.text));
        let text = "a:\n  next:\n    leaf: 1\nb: null\n";
        let problems = editor.validate(crate::editor::Language::YAML, text);
        assert_eq!(problems.len(), 1, "{written:?}: {problems:?}");
        assert!(
            problems[0].message.contains("minimum"),
            "{written:?}: {problems:?}"
        );
    }
}

/// A property named like a keyword (`nullable`, `$ref`, `example`) is a
/// property, not a keyword, in every written form.
#[test]
fn a_property_named_like_a_keyword_stays_a_property_in_every_written_form() {
    let schema = json!({
        "type": "object",
        "properties": {
            "nullable": {"type": "integer"},
            "example": {"type": "string"},
            "$ref": {"type": "boolean"},
        },
        "required": ["nullable"],
    });
    for written in WRITTEN {
        let editor = source(&schema, written).and_then(|source| source.compile().ok());
        let editor = editor.unwrap_or_else(|| panic!("{written:?} compiles"));
        let problems = editor.validate(crate::editor::Language::JSON, "{\"nullable\": \"x\"}");
        assert_eq!(problems.len(), 1, "{written:?}: {problems:?}");
    }
}
