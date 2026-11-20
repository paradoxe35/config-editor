//! The classes of failure a known finding explains, each naming it: its
//! fix removes the entry (and the class must then hold), or a documented
//! decision that makes the class no failure.

use serde_json::{Value, json};

use super::reference::walk;
use super::runner::Case;
use super::xml_spelling;
use crate::editor::Language;

pub(super) struct Known {
    /// The finding or decision.
    pub(super) finding: &'static str,
    /// The property whose class it allows.
    pub(super) property: &'static str,
    /// The class names it allows start with this.
    pub(super) class: &'static str,
    /// Whether it explains this (shrunk) case.
    pub(super) applies: fn(&Case) -> bool,
}

pub(super) const KNOWN: &[Known] = &[
    // `completion::least`: a list is written empty only where no list
    // meets every schema at it that the language writes (more items asked
    // than `items: false` lets be, an item only `null` meets in TOML). The
    // checks name this class only where a bounded search of their own
    // (`list_search`), not the editor's plan, finds no such list.
    Known {
        finding: "documented: a list no written list meets is written empty",
        property: "fill",
        class: "a fill writes an empty list where no list meets every schema",
        applies: |_| true,
    },
    Known {
        finding: "documented: a list no written list meets is written empty",
        property: "suggestions",
        class: "a suggested key writes an empty list where no list meets every schema",
        applies: |_| true,
    },
    // The search's bound read off the schema is past the longest list it
    // tries: an empty list there is left undecided, not judged.
    Known {
        finding: "decided: a list longer than the search tries is not judged",
        property: "fill",
        class: "a fill writes an empty list where a bounded search finds none",
        applies: |_| true,
    },
    Known {
        finding: "decided: a list longer than the search tries is not judged",
        property: "suggestions",
        class: "a suggested key writes an empty list where a bounded search finds none",
        applies: |_| true,
    },
    // Each language's insert writes a key's own list as `[]`, the caret
    // inside to type its items (README, "required children on accept"),
    // so a list asking items at once (`minItems`, `contains`) is refused
    // until they are typed. The checks name this class only for the
    // accepted key's own value read as `[]`.
    Known {
        finding: "documented: a key's own list is written empty, the caret inside",
        property: "fill",
        class: "a fill writes the key's own list empty, the caret inside",
        applies: |_| true,
    },
    Known {
        finding: "documented: a key's own list is written empty, the caret inside",
        property: "suggestions",
        class: "a suggested key writes its own list empty, the caret inside",
        applies: |_| true,
    },
    // Open: a fill writes the children of an object without reading the
    // siblings it wrote, so an `if` they decide (`mode: ""` never meets
    // `mode: {const: false}`) is read as undecided, and a value only the
    // branch not taken allows may be written (`a: 0` where `else` asks
    // `exclusiveMinimum: 0`). Found once the broad list allowance went;
    // repro `a_fill_decides_an_if_by_the_siblings_it_writes` (ignored).
    Known {
        finding: "open: a fill does not decide an if by the siblings it writes",
        property: "fill",
        class: "a fill writes a value its schema refuses (",
        applies: |case| if_with_both_branches(&case.schema),
    },
    // The edit_as save classes below are told value by value
    // (`xml_spelling`): each names the one value a documented XML spelling
    // explains, and a save reading otherwise anywhere else stays "a save
    // reads otherwise than the view", which nothing allows.
    //
    // `bridge::merge`: XML shows a list of one as its lone item (the view
    // warns it cannot show it exactly), and a list cut to one item looks
    // the same; without a schema saying it is a list, a save keeps the
    // host's list for it.
    Known {
        finding: "documented: an XML view shows a list of one as its item, and a save keeps the list",
        property: "edit_as",
        class: xml_spelling::KEPT_LIST,
        applies: |_| true,
    },
    // `bridge::collapsed`: a list the host held, cut to one item in a view
    // of an XML host with no schema saying it is a list (`[0, 0]` made
    // `[0]`), saves as a lone element, read back as `0`: the host's own
    // edit writes the same text.
    Known {
        finding: "documented: a list cut to one item in an XML host is written as one element",
        property: "edit_as",
        class: xml_spelling::CUT_TO_ONE,
        applies: |_| true,
    },
    // `bridge::text_scalars`: XML text the view reads as a number or a
    // boolean is saved as text where the host holds a string the schema
    // takes there (`"1"` made `12` stays `"12"`).
    Known {
        finding: "documented: XML view text keeps the host's string where the schema allows it",
        property: "edit_as",
        class: xml_spelling::KEPT_TEXT,
        applies: |_| true,
    },
    // `completion::candidates::refused_beside`: a key that makes another
    // key required (a dependent schema, `dependentRequired`, the `then` of
    // an `if` it decides) is offered, and the problem it adds names the
    // key to write next; one that makes the schemas refuse a value
    // written beside it is not offered. Only a `required` problem is so.
    Known {
        finding: "documented: a key that makes another one required is offered",
        property: "suggestions",
        class: "a suggested key adds a problem elsewhere (required)",
        applies: |_| true,
    },
    // A required key no value meets (`a: {enum: []}`, or `x_a: {const: 0}`
    // under a pattern typing `x_` keys strings) has no fill the schema
    // takes: whatever is written there (or left out) is refused, and the
    // problem names it.
    Known {
        finding: "documented: a field no value meets has no fill its schema takes",
        property: "fill",
        class: "a fill writes a value its schema refuses (",
        applies: |case| meets_nothing(&case.schema),
    },
    Known {
        finding: "documented: a field no value meets has no fill its schema takes",
        property: "fill",
        class: "a fill leaves out a required child",
        applies: |case| meets_nothing(&case.schema),
    },
    // `oneOf` options a value of one kind meets twice (`{}` beside `{type:
    // object}`) in an XML view: its text read so that one option matches
    // (`true` as text) where the host's typed value matches both.
    Known {
        finding: "documented: an XML view may judge a oneOf with an anything option otherwise",
        property: "edit_as",
        class: "the view and its saved host disagree on problems",
        applies: |case| overlapping_one_of(&case.schema),
    },
    // `data_reading`: an HCL block where the schema wants a list
    // is one item of it (`c {}` reads `[{}]`), as HCL writes lists of
    // objects; an object written there changes its verdict.
    Known {
        finding: "documented: an HCL block where a list is wanted is its one item",
        property: "validation",
        class: "writing a document changes its verdict",
        applies: |case| case.language == Language::HCL,
    },
    // `bridge::text_scalars`: an XML element emptied where the host holds
    // text saves as the empty text; with no schema the view reads the
    // empty element as an empty object.
    Known {
        finding: "documented: an emptied XML element saves as the host's empty text",
        property: "edit_as",
        class: xml_spelling::EMPTIED_TEXT,
        applies: |_| true,
    },
    // `bridge::schema_readings`: a value the schema takes in no reading
    // (`0` typed under `minimum: 1` over a host's text, or any value of a
    // field no value meets) is saved in the host's type: the view's
    // reading is kept only where the schema takes it.
    Known {
        finding: "documented: a value no reading of which the schema takes keeps the host's type",
        property: "edit_as",
        class: xml_spelling::KEPT_TYPE,
        applies: |_| true,
    },
    // `bridge::text_scalars`: a number typed in another spelling (`08`,
    // `2.50`) where the host holds a number is that number.
    Known {
        finding: "documented: a number typed in another spelling is the host's number",
        property: "edit_as",
        class: xml_spelling::RESPELLED,
        applies: |_| true,
    },
    Known {
        finding: "documented: an empty XML element reads as an empty object",
        property: "suggestions",
        class: "a suggested key adds a problem elsewhere",
        applies: |case| case.language == Language::XML && empty_text(&case.doc),
    },
    // XML writes a list of one as its lone item: where no schema says
    // the value is a list, it reads as that item (an object), which an
    // object's keywords (`required`) judge, as they do not the list.
    Known {
        finding: "documented: XML reads a list of one as its item where no schema says list",
        property: "validation",
        class: "XML reads a valid document as an invalid one",
        applies: |case| unlisted_one_item_list(&case.schema, &case.doc),
    },
    // An empty XML element (`<a/>`) is an empty object where an object may
    // be (`<cloudflare/>` starts one): where the schema also takes an empty
    // text there, a document written with that text reads otherwise.
    Known {
        finding: "documented: an empty XML element reads as an empty object",
        property: "validation",
        class: "XML reads a valid document as an invalid one",
        applies: |case| empty_text(&case.doc),
    },
    // XML writes no empty list, and TOML no `null`: a required child
    // only such a value meets is left out (the problem names it).
    Known {
        finding: "documented: XML writes no empty list, TOML no null",
        property: "fill",
        class: "a fill leaves out a required child",
        applies: |case| matches!(case.language.label(), "XML" | "TOML"),
    },
    // An empty document has no problems (`diagnostics::validate`): a view
    // holding `{}` whose host's text is empty disagrees by design.
    Known {
        finding: "documented: an empty document has no problems",
        property: "edit_as",
        class: "the view and its saved host disagree on problems",
        applies: |case| {
            case.other
                .as_object()
                .is_some_and(serde_json::Map::is_empty)
        },
    },
    // By design each value's options are explained a bounded
    // number of times, the work following the option tree below that
    // bound: only a schema recursing through options (`anyOf`, `oneOf`,
    // `allOf`) does that work.
    Known {
        finding: "decided: a value's options are explained up to a fixed bound",
        property: "work",
        class: "validate: OptionTried grows",
        applies: |case| recurses_through_options(&case.schema),
    },
    Known {
        finding: "decided: a value's options are explained up to a fixed bound",
        property: "work",
        class: "validate: BranchWalk grows",
        applies: |case| recurses_through_options(&case.schema) && !deprecates(&case.schema),
    },
    Known {
        finding: "decided: a value's options are explained up to a fixed bound",
        property: "work",
        class: "validate: ErrorVisit grows",
        applies: |case| recurses_through_options(&case.schema),
    },
];

/// Whether some value of `schema` is one no value meets: a schema listing
/// none (`enum: []`), or schemas applying to one value together (a schema,
/// its `allOf` members and an always-taken `then`; a property's schemas
/// from those, by name and by each pattern its key matches) that type it
/// apart or list only values another one's `type` refuses
/// (`allOf: [{enum: [true]}, {type: string}]`; `properties: {x_a:
/// {const: 0}}` beside `patternProperties: {^x_: {type: string}}`; a
/// required key's dependent schema typing another required key a number
/// where `additionalProperties` types it a string). The dependent schemas
/// of the required keys count among the schemas applying together.
fn meets_nothing(schema: &Value) -> bool {
    let mut found = false;
    walk(schema, &mut |node| {
        found |= node
            .get("enum")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty);
        let always = node
            .get("if")
            .is_some_and(|condition| *condition == json!({}) || *condition == json!(true));
        let mut together: Vec<&Value> = std::iter::once(node)
            .chain(
                node.get("allOf")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten(),
            )
            .chain(node.get("then").filter(|_| always))
            .collect();
        // A key the schemas require is always there, so the dependent
        // schema it brings applies too.
        let required: Vec<&str> = together
            .iter()
            .filter_map(|source| source.get("required").and_then(Value::as_array))
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let dependent: Vec<&Value> = together
            .iter()
            .flat_map(|source| {
                ["dependentSchemas", "dependencies"]
                    .into_iter()
                    .filter_map(|keyword| source.get(keyword).and_then(Value::as_object))
            })
            .flatten()
            .filter(|(key, schema)| required.contains(&key.as_str()) && schema.is_object())
            .map(|(_, schema)| schema)
            .collect();
        together.extend(dependent);
        found |= apart(&together);
        let keys: std::collections::BTreeSet<&String> = together
            .iter()
            .filter_map(|source| source.get("properties").and_then(Value::as_object))
            .flat_map(|properties| properties.keys())
            .collect();
        for key in keys {
            let applying: Vec<&Value> = together
                .iter()
                .flat_map(|source| {
                    let named = source.get("properties").and_then(|named| named.get(key));
                    let patterned = source
                        .get("patternProperties")
                        .and_then(Value::as_object)
                        .into_iter()
                        .flatten()
                        .filter(|(pattern, _)| {
                            crate::editor::completion::pattern_decides(pattern, key) == Some(true)
                        })
                        .map(|(_, schema)| schema)
                        .collect::<Vec<_>>();
                    // A source naming the key neither way gives it its
                    // `additionalProperties`.
                    let additional = source
                        .get("additionalProperties")
                        .filter(|_| named.is_none() && patterned.is_empty());
                    named.into_iter().chain(patterned).chain(additional)
                })
                .collect();
            found |= apart(&applying);
        }
    });
    found
}

/// Whether `schemas`, applying to one value together, type it apart, or
/// one lists only values another one's `type` refuses.
fn apart(schemas: &[&Value]) -> bool {
    let types: Vec<&str> = schemas
        .iter()
        .filter_map(|schema| schema.get("type").and_then(Value::as_str))
        .collect();
    let meets = |have: &str, wanted: &str| {
        have == wanted || matches!((have, wanted), ("number" | "integer", "number" | "integer"))
    };
    if types
        .iter()
        .any(|one| types.iter().any(|other| !meets(one, other)))
    {
        return true;
    }
    let kind = |value: &Value| match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    schemas.iter().any(|lister| {
        let listed: Vec<&Value> = lister
            .get("const")
            .into_iter()
            .chain(
                lister
                    .get("enum")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten(),
            )
            .collect();
        !listed.is_empty()
            && types
                .iter()
                .any(|wanted| listed.iter().all(|value| !meets(kind(value), wanted)))
    })
}

/// Whether some `oneOf` of `schema` has an option allowing anything
/// (`{}`) beside others.
fn overlapping_one_of(schema: &Value) -> bool {
    let mut found = false;
    walk(schema, &mut |node| {
        found |= node
            .get("oneOf")
            .and_then(Value::as_array)
            .is_some_and(|options| {
                options.len() > 1 && options.iter().any(|option| *option == json!({}))
            });
    });
    found
}

/// Whether `value` holds an empty text.
fn empty_text(value: &Value) -> bool {
    let mut found = false;
    walk(value, &mut |node| {
        found |= node.as_str().is_some_and(str::is_empty);
    });
    found
}

/// Whether the recursive node of a `work` schema is options (`anyOf`,
/// `oneOf`, `allOf`), each holding the node again: the option tree the
/// explanation follows grows with each level.
fn recurses_through_options(schema: &Value) -> bool {
    schema.pointer("/$defs/node").is_some_and(|node| {
        ["anyOf", "oneOf", "allOf"]
            .iter()
            .any(|keyword| node.get(*keyword).is_some())
    })
}

/// Whether `schema` marks some field deprecated.
fn deprecates(schema: &Value) -> bool {
    schema.to_string().contains("\"deprecated\":true")
}

/// Whether `doc` holds a list of one item where no schema of `schema`
/// says the value is a list (its type, its items): XML reads it as its
/// item there.
fn unlisted_one_item_list(schema: &Value, doc: &Value) -> bool {
    fn lists(schema: &Value, doc: &Value, path: &mut Vec<String>) -> bool {
        let here = match doc {
            Value::Array(items) => {
                items.len() == 1 && {
                    let mut at = Vec::new();
                    schemas_at(schema, schema, path, 0, &mut at);
                    !at.into_iter().any(says_list)
                }
            }
            _ => false,
        };
        let children: Vec<(String, &Value)> = match doc {
            Value::Array(items) => items
                .iter()
                .enumerate()
                .map(|(index, item)| (index.to_string(), item))
                .collect(),
            Value::Object(map) => map
                .iter()
                .map(|(key, value)| (key.clone(), value))
                .collect(),
            _ => Vec::new(),
        };
        here || children.into_iter().any(|(segment, child)| {
            path.push(segment);
            let found = lists(schema, child, path);
            path.pop();
            found
        })
    }
    lists(schema, doc, &mut Vec::new())
}

/// Every schema of `root` that may apply at `path` below `schema` (a key
/// or an index, each read both ways): through references, applicators
/// (both branches of a condition), properties, patterns (unless one
/// surely does not match), additional properties and items.
fn schemas_at<'a>(
    root: &'a Value,
    schema: &'a Value,
    path: &[String],
    depth: usize,
    out: &mut Vec<&'a Value>,
) {
    if depth > 32 {
        return;
    }
    if let Some(target) = schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| super::documents::resolve(root, reference))
    {
        schemas_at(root, target, path, depth + 1, out);
    }
    for keyword in ["allOf", "anyOf", "oneOf"] {
        for member in schema
            .get(keyword)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            schemas_at(root, member, path, depth + 1, out);
        }
    }
    for keyword in ["then", "else"] {
        if let Some(branch) = schema.get(keyword) {
            schemas_at(root, branch, path, depth + 1, out);
        }
    }
    let Some((head, rest)) = path.split_first() else {
        out.push(schema);
        return;
    };
    let mut below: Vec<&Value> = Vec::new();
    below.extend(schema.get("properties").and_then(|named| named.get(head)));
    let patterns = schema
        .get("patternProperties")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(pattern, _)| {
            crate::editor::completion::pattern_decides(pattern, head) != Some(false)
        })
        .map(|(_, schema)| schema);
    below.extend(patterns);
    below.extend(schema.get("additionalProperties"));
    if let Ok(index) = head.parse::<usize>() {
        below.extend(
            schema
                .get("prefixItems")
                .and_then(Value::as_array)
                .and_then(|prefix| prefix.get(index)),
        );
        below.extend(schema.get("items"));
    }
    for next in below {
        schemas_at(root, next, rest, depth + 1, out);
    }
}

/// Whether `schema` itself says its value is a list (its type, its items).
fn says_list(schema: &Value) -> bool {
    schema
        .get("type")
        .is_some_and(|kind| kind.to_string().contains("array"))
        || [
            "items",
            "prefixItems",
            "contains",
            "minItems",
            "maxItems",
            "uniqueItems",
        ]
        .iter()
        .any(|keyword| schema.get(*keyword).is_some())
}

/// Whether `schema` has an `if` testing a property with both a `then`
/// and an `else` saying something of properties.
fn if_with_both_branches(schema: &Value) -> bool {
    let mut found = false;
    walk(schema, &mut |node| {
        let branch = |keyword: &str| {
            node.get(keyword)
                .and_then(|branch| branch.get("properties"))
        };
        found |= node
            .get("if")
            .and_then(|test| test.get("properties"))
            .is_some()
            && branch("then").is_some()
            && branch("else").is_some();
    });
    found
}

/// The finding allowing `class` of `property` for `case`, if one does.
pub(super) fn excused(property: &str, class: &str, case: &Case) -> Option<&'static str> {
    KNOWN
        .iter()
        .find(|known| {
            known.property == property && class.starts_with(known.class) && (known.applies)(case)
        })
        .map(|known| known.finding)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(language: Language, schema: Value, doc: Value) -> Case {
        Case {
            language,
            pick: 0,
            schema,
            doc,
            other: Value::Null,
        }
    }

    /// Fields no value meets are allowed; the same shapes with a value to
    /// meet are not, and no secret shown is.
    #[test]
    fn allowances_reach_exactly_their_shapes() {
        let fill = |schema: Value| {
            excused(
                "fill",
                "a fill writes a value its schema refuses (type)",
                &case(Language::JSON, schema, json!({})),
            )
        };
        let empty_enum = json!({"properties": {"c": {"allOf": [
            {"properties": {"b": {"required": ["a"], "allOf": [{"properties": {"a": {"type": "integer"}}}]}}, "required": ["b"]},
            {"properties": {"b": {"properties": {"a": {"enum": []}}}}}]}}});
        let patterned = |listed: Value| {
            json!({"$defs": {"d0": {"properties": {"x_one": {"const": listed}}, "required": ["x_one"],
                "patternProperties": {"^x_": {"type": "string"}}}}, "properties": {"size": {"$ref": "#/$defs/d0"}}})
        };
        assert!(fill(empty_enum).is_some());
        assert!(fill(patterned(json!(0))).is_some());
        assert!(fill(patterned(json!("zero"))).is_none());
        // A key one `allOf` member types and another gives its
        // `additionalProperties`, which no value of that type meets.
        let additional = |other: Value| {
            json!({"properties": {"a": {"allOf": [{"additionalProperties": {"const": other}},
                {"properties": {"c": {"type": "null"}}, "required": ["x", "c"]}]}}})
        };
        assert!(fill(additional(json!("f"))).is_some());
        assert!(fill(additional(Value::Null)).is_none());
        assert!(fill(json!({"properties": {"x_one": {"const": 0}}, "patternProperties": {"^y_": {"type": "string"}}})).is_none());

        // A list written too short is excused by no schema shape: the
        // checks name the key's own `[]` and the lists no search meets
        // classes of their own.
        let strings = json!({"minItems": 2, "items": {"type": "string"}});
        for schema in [
            json!({"properties": {"c": strings}}),
            json!({"properties": {"c": {"properties": {"l": strings}}}}),
        ] {
            for class in [
                "a fill writes too few list items (minItems)",
                "a fill writes a value its schema refuses (anyOf)",
            ] {
                let found = excused(
                    "fill",
                    class,
                    &case(Language::YAML, schema.clone(), json!({})),
                );
                assert!(found.is_none(), "{class} {schema}");
            }
        }

        let secret = |language: Language, doc: Value| {
            excused(
                "secrets",
                "a problem message holds a secret",
                &case(language, json!({}), doc),
            )
        };
        // No allowance excuses a secret shown (P1): the oracle judges XML's
        // secrets where XML reads them (see `secrets::check`).
        assert!(secret(Language::XML, json!({"size": [[], "sekret1q477"]})).is_none());
        assert!(secret(Language::XML, json!({"name": [{}, [false, "sekret1q625"]]})).is_none());
        assert!(secret(Language::JSON, json!({"size": [[], "sekret1q477"]})).is_none());

        // A list of one XML reads as its item: only where no schema there
        // says list, whatever the schema says of other values.
        let one_item = |schema: Value| {
            excused(
                "validation",
                "XML reads a valid document as an invalid one",
                &case(Language::XML, schema, json!({"a": [{"name": 0}]})),
            )
        };
        let inner_list = json!({"allOf": [{"properties": {"a": {"properties": {"c": {"items": {}}},
            "required": ["c"]}}}]});
        assert!(one_item(inner_list).is_some());
        assert!(one_item(json!({"properties": {"a": {"type": "array"}}})).is_none());
        let referenced = json!({"$defs": {"l": {"minItems": 1}},
            "properties": {"a": {"$ref": "#/$defs/l"}}});
        assert!(one_item(referenced).is_none());

        // Work bounded by the options explained: only where the schema
        // recurses through options.
        let work = |node: Value| {
            excused(
                "work",
                "validate: ErrorVisit grows faster than the document",
                &case(Language::YAML, json!({"$defs": {"node": node}}), json!({})),
            )
        };
        let link = json!({"properties": {"c": {"$ref": "#/$defs/node"}}});
        assert!(work(json!({"oneOf": [link.clone(), {"type": "object"}]})).is_some());
        assert!(work(link).is_none());
    }
}
