//! OpenAPI's `discriminator` as JSON Schema: the property it names picks
//! the schema a value is checked by. Beside `oneOf`/`anyOf` options (each
//! a reference to a named schema), each option is picked by the values
//! the `mapping` gives it, else by its schema's name; on a base schema
//! (Swagger 2.0's polymorphism, or OpenAPI's without options) the named
//! schemas extending it by `allOf` are the options, the base one too.
//!
//! The pick is an `if`/`then` per option on the property's value, inside
//! an `if` the value is an object (what a discriminator reads): a value
//! is checked by the one schema it names, its problems are its own, and
//! completion follows the option the value picks. What it cannot say (an
//! option written in place, a mapping to another document) keeps the
//! options as they are, and is a loss.

use std::collections::{BTreeSet, HashSet};

use serde_json::{Map, Value, json};

use super::super::language::{pointer_key, unescaped_key};
use super::super::schema::each_schema;
use super::{LossEffect, SchemaLoss};

/// Where a named schema is in the compiled schema.
const DEFS: &str = "#/$defs/";

/// A discriminator read: the property and each schema it picks, with the
/// values picking it.
struct Picks {
    property: String,
    cases: Vec<(String, Vec<String>)>,
}

/// Rewrites every discriminator of `defs` (the named schemas, their
/// references already into `#/$defs/`). `written` is the prefix the
/// document's own references have (`#/components/schemas/`), which a
/// mapping still uses. Gives what it cannot express, each with the named
/// schema it is in.
pub(super) fn compile(defs: &mut Map<String, Value>, written: &str) -> Vec<(String, SchemaLoss)> {
    let mut losses = Vec::new();
    let names: Vec<String> = defs.keys().cloned().collect();
    for name in &names {
        let Some(def) = defs.get_mut(name) else {
            continue;
        };
        for at in option_sites(def) {
            let Some(Value::Object(map)) = def.pointer_mut(&at) else {
                continue;
            };
            if !options_dispatch(map, &names, written) {
                losses.push((
                    name.clone(),
                    SchemaLoss::new(
                        "discriminator",
                        LossEffect::Approximated,
                        format!(
                            "{written}{}{at}: the discriminator is not checked (an option is written in place or mapped outside the document); the options are matched by their schemas",
                            pointer_key(name)
                        ),
                    ),
                ));
            }
        }
    }
    for name in &names {
        base_dispatch(defs, name, written);
    }
    losses
}

/// The places in `def` (JSON pointers, deepest first) of a schema with a
/// discriminator beside `oneOf` or `anyOf` options. A `discriminator`
/// elsewhere (an extension's data, such as Kubernetes'
/// `x-kubernetes-unions`) is none.
fn option_sites(def: &Value) -> Vec<String> {
    let mut sites = Vec::new();
    each_schema(def, &(), &mut |at, members, ()| {
        if members.contains_key("discriminator")
            && (members.contains_key("oneOf") || members.contains_key("anyOf"))
        {
            sites.push(at.to_string());
        }
        Some(())
    });
    sites.sort_by_key(|at| std::cmp::Reverse(at.len()));
    sites
}

/// The property a discriminator names: OpenAPI's `propertyName`, or
/// Swagger 2.0's string.
fn property(discriminator: &Value) -> Option<String> {
    match discriminator {
        Value::String(name) => Some(name.clone()),
        Value::Object(map) => map
            .get("propertyName")
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => None,
    }
}

/// The named schema `reference` (a `$ref` into `#/$defs/`) names.
fn named(reference: &str) -> Option<String> {
    let token = reference.strip_prefix(DEFS)?;
    (!token.contains('/')).then(|| unescaped_key(token))
}

/// The named schema a mapping's target names: a reference as the document
/// writes them, or a schema's name.
fn mapped(target: &str, written: &str, names: &[String]) -> Option<String> {
    let name = match target.strip_prefix(written) {
        Some(token) if !token.contains('/') => unescaped_key(token),
        Some(_) => return None,
        None if !target.contains(['/', '#']) => target.to_string(),
        None => return None,
    };
    names.contains(&name).then_some(name)
}

/// The values the discriminator's `mapping` gives each named schema.
fn mapping(
    discriminator: &Value,
    written: &str,
    names: &[String],
) -> Option<Vec<(String, String)>> {
    let Some(entries) = discriminator.get("mapping") else {
        return Some(Vec::new());
    };
    entries
        .as_object()?
        .iter()
        .map(|(value, target)| {
            let name = mapped(target.as_str()?, written, names)?;
            Some((value.clone(), name))
        })
        .collect()
}

/// Each of `options` (named schemas) with the values picking it: the
/// mapping's, else its name; then what the mapping picks besides them.
fn cases(options: &[String], mapped: &[(String, String)]) -> Vec<(String, Vec<String>)> {
    let mut cases: Vec<(String, Vec<String>)> = options
        .iter()
        .map(|option| {
            let values: Vec<String> = mapped
                .iter()
                .filter(|(_, name)| name == option)
                .map(|(value, _)| value.clone())
                .collect();
            let values = if values.is_empty() {
                vec![option.clone()]
            } else {
                values
            };
            (option.clone(), values)
        })
        .collect();
    for (value, name) in mapped {
        if !options.contains(name) {
            match cases.iter_mut().find(|(known, _)| known == name) {
                Some((_, values)) => values.push(value.clone()),
                None => cases.push((name.clone(), vec![value.clone()])),
            }
        }
    }
    cases
}

/// The pick as JSON Schema: the property required and one of the values,
/// and each named schema checking the values picking it.
fn pick(picks: &Picks, own: Option<&[String]>) -> Value {
    let property = &picks.property;
    let mut values: Vec<&String> = picks.cases.iter().flat_map(|(_, values)| values).collect();
    if let Some(own) = own {
        values.extend(own);
    }
    let branches: Vec<Value> = picks
        .cases
        .iter()
        .map(|(name, values)| {
            let value = match values.as_slice() {
                [one] => json!({"const": one}),
                many => json!({"enum": many}),
            };
            json!({
                "if": {"properties": {property.as_str(): value}, "required": [property]},
                "then": {"$ref": format!("{DEFS}{}", pointer_key(name))},
            })
        })
        .collect();
    json!({
        "required": [property],
        "properties": {property.as_str(): {"enum": values}},
        "allOf": branches,
    })
}

/// Rewrites `map` (a schema with a discriminator and options) to pick its
/// option; false (and `map` unchanged) when it cannot.
fn options_dispatch(map: &mut Map<String, Value>, names: &[String], written: &str) -> bool {
    let keyword = if map.contains_key("oneOf") {
        "oneOf"
    } else {
        "anyOf"
    };
    let Some(discriminator) = map.get("discriminator") else {
        return false;
    };
    let Some(property) = property(discriminator) else {
        return false;
    };
    let Some(Value::Array(written_options)) = map.get(keyword) else {
        return false;
    };
    let options: Option<Vec<String>> = written_options
        .iter()
        .map(|option| {
            let reference = option.as_object()?.get("$ref")?.as_str()?;
            named(reference).filter(|name| names.contains(name))
        })
        .collect();
    let (Some(options), Some(mapped)) = (options, mapping(discriminator, written, names)) else {
        return false;
    };
    let picks = Picks {
        property,
        cases: cases(&options, &mapped),
    };
    let then = pick(&picks, None);
    let Some(options) = map.remove(keyword) else {
        return false;
    };
    map.remove("discriminator");
    // A value that is no object is none a discriminator reads: the
    // options still check it.
    add_all_of(
        map,
        json!({"if": {"type": "object"}, "then": then, "else": {keyword: options}}),
    );
    true
}

/// Appends `schema` to `map`'s `allOf`.
fn add_all_of(map: &mut Map<String, Value>, schema: Value) {
    match map.get_mut("allOf") {
        Some(Value::Array(all)) => all.push(schema),
        _ => {
            map.insert("allOf".to_string(), Value::Array(vec![schema]));
        }
    }
}

/// The named schemas whose `allOf` refers to `base`, directly.
fn extending(defs: &Map<String, Value>, base: &str) -> Vec<String> {
    let target = format!("{DEFS}{}", pointer_key(base));
    defs.iter()
        .filter(|(_, def)| {
            def.get("allOf")
                .and_then(Value::as_array)
                .is_some_and(|all| {
                    all.iter().any(|item| {
                        item.get("$ref").and_then(Value::as_str) == Some(target.as_str())
                    })
                })
        })
        .map(|(name, _)| name.clone())
        .collect()
}

/// Rewrites named schema `name` when it is a base with a discriminator:
/// the schemas extending it (at any depth) are its options. The base
/// itself moves to a name of its own, which its direct extensions refer
/// to; `name` picks.
fn base_dispatch(defs: &mut Map<String, Value>, name: &str, written: &str) {
    let Some(def) = defs.get(name) else {
        return;
    };
    let Some(discriminator) = def.get("discriminator").cloned() else {
        return;
    };
    if def.get("oneOf").is_some() || def.get("anyOf").is_some() {
        return;
    }
    let names: Vec<String> = defs.keys().cloned().collect();
    let (Some(property), Some(mapped)) = (
        property(&discriminator),
        mapping(&discriminator, written, &names),
    ) else {
        return;
    };
    let direct = extending(defs, name);
    let mut options: Vec<String> = Vec::new();
    let mut queue = direct.clone();
    let mut seen: HashSet<String> = HashSet::from([name.to_string()]);
    while let Some(next) = queue.pop() {
        if seen.insert(next.clone()) {
            queue.extend(extending(defs, &next));
            options.push(next);
        }
    }
    let Some(Value::Object(mut base)) = defs.remove(name) else {
        return;
    };
    base.remove("discriminator");
    if options.is_empty() {
        // Nothing extends it: the base is the only schema it can name.
        defs.insert(name.to_string(), Value::Object(base));
        return;
    }
    options.sort();
    let mut base_name = format!("{name}.base");
    let taken: BTreeSet<&String> = defs.keys().collect();
    while taken.contains(&base_name) {
        base_name.push('_');
    }
    let own: Vec<String> = mapped
        .iter()
        .filter(|(_, target)| target == name)
        .map(|(value, _)| value.clone())
        .collect();
    let own = if own.is_empty() {
        vec![name.to_string()]
    } else {
        own
    };
    let mapped: Vec<(String, String)> = mapped
        .into_iter()
        .filter(|(_, target)| target != name)
        .collect();
    let picks = Picks {
        property,
        cases: cases(&options, &mapped),
    };
    let base_ref = format!("{DEFS}{}", pointer_key(&base_name));
    let from = format!("{DEFS}{}", pointer_key(name));
    for extension in &direct {
        let Some(Value::Array(all)) = defs.get_mut(extension).and_then(|def| def.get_mut("allOf"))
        else {
            continue;
        };
        for item in all {
            if item.get("$ref").and_then(Value::as_str) == Some(from.as_str()) {
                item["$ref"] = Value::String(base_ref.clone());
            }
        }
    }
    let mut dispatch = Map::new();
    for note in ["title", "description"] {
        if let Some(text) = base.get(note) {
            dispatch.insert(note.to_string(), text.clone());
        }
    }
    dispatch.insert(
        "allOf".to_string(),
        json!([
            {"$ref": base_ref},
            {"if": {"type": "object"}, "then": pick(&picks, Some(&own))},
        ]),
    );
    defs.insert(base_name, Value::Object(base));
    defs.insert(name.to_string(), Value::Object(dispatch));
}
