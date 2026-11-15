//! The classes of failure a known finding explains, each naming it: its
//! fix removes the entry (and the class must then hold), or a documented
//! decision that makes the class no failure.

use serde_json::{Value, json};

use super::reference::walk;
use super::runner::Case;
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
    // `completion::least`: the least list is "empty when it asks none or
    // an item has no value to write": an item schema saying nothing of
    // its value (`items: {}`) gives no item to write.
    Known {
        finding: "documented: a least list of items with no value to write is empty",
        property: "fill",
        class: "a fill writes too few list items",
        applies: |case| lists_of_anything(&case.schema),
    },
    Known {
        finding: "documented: a least list of items with no value to write is empty",
        property: "suggestions",
        class: "a suggested key writes a value its schema refuses (min",
        applies: |case| lists_of_anything(&case.schema),
    },
    Known {
        finding: "documented: a least list of items with no value to write is empty",
        property: "suggestions",
        class: "a suggested key writes a value its schema refuses (contains)",
        applies: |case| lists_of_anything(&case.schema),
    },
    // `bridge`: XML shows a one-item list as its lone item (the view warns
    // it cannot show it exactly); without a schema saying it is a list, a
    // save keeps the host's list for it.
    Known {
        finding: "documented: an XML view cannot show a one-item list without a schema",
        property: "edit_as",
        class: "a save reads otherwise than the view",
        applies: |case| case.language != Language::XML && one_item_list(&case.doc),
    },
    // FZ38-8 (new): a list emptied in an Edit-as view of an XML host
    // (`c: []` where the host has `<c>false</c>`) is saved by dropping the
    // element (`<x_two/>`) instead of waiting: the value is lost silently.
    Known {
        finding: "FZ38-8",
        property: "edit_as",
        class: "a save writes what XML cannot hold, without waiting",
        applies: |_| true,
    },
    // FZ38-8 too: a list cut to one item in a view of an XML host with no
    // schema saying it is a list (`[0, 0]` made `[0]`) saves as a lone
    // element, read back as `0`, without waiting.
    Known {
        finding: "FZ38-8",
        property: "edit_as",
        class: "a save reads otherwise than the view",
        applies: |case| case.language == Language::XML && one_item_list(&case.other),
    },
    // `bridge::text_scalars`: XML text the view reads as a number or a
    // boolean is saved as text where the host holds a string and the
    // schema allows both (`"1"` made `12` stays `"12"`).
    Known {
        finding: "documented: XML view text keeps the host's string where the schema allows it",
        property: "edit_as",
        class: "a save keeps the host's text where the XML view's text reads as another type",
        applies: |_| true,
    },
    // `completion::candidates::refused_beside`: a key whose dependent
    // schema (or `dependentRequired`) requires another key is offered,
    // and the problem it adds names the key to write next; one whose
    // dependent schema refuses a value written beside it is not offered.
    Known {
        finding: "documented: a key whose dependency requires another is offered",
        property: "suggestions",
        class: "a suggested key adds a problem elsewhere (required)",
        applies: |case| case.schema.to_string().contains("\"dependen"),
    },
    // FZ38-10 (edge): a key's own list value is written `[]` to type
    // into (each language's insert), so a list asking items at once
    // (`minItems`, `contains`, in an option or member) is refused until
    // they are typed; and list constraints no value meets together
    // (`contains` beside `items` refusing it).
    Known {
        finding: "FZ38-10",
        property: "fill",
        class: "a fill writes a value its schema refuses (",
        applies: |case| asks_list_items(&case.schema),
    },
    Known {
        finding: "FZ38-10",
        property: "suggestions",
        class: "a suggested key writes a value its schema refuses (",
        applies: |case| asks_list_items(&case.schema),
    },
    // FZ38-10 too: a field no value meets (`allOf: [{enum: [true]},
    // {type: string}]`) has no reading a save can keep.
    Known {
        finding: "FZ38-10",
        property: "edit_as",
        class: "a save reads otherwise than the view",
        applies: |case| contradicting_all_of(&case.schema),
    },
    // FZ38-10 too: `oneOf` options a value of one kind meets twice (`{}`
    // beside `{type: object}`): the object a fill writes for the required
    // keys matches both.
    Known {
        finding: "FZ38-10",
        property: "fill",
        class: "a fill leaves out a required child",
        applies: |case| overlapping_one_of(&case.schema),
    },
    Known {
        finding: "FZ38-10",
        property: "fill",
        class: "a fill writes a value its schema refuses (oneOf)",
        applies: |case| overlapping_one_of(&case.schema),
    },
    // XML writes no list of lists: a secret in an item of an inner list
    // is read in another place than its schema marks.
    Known {
        finding: "documented: XML writes no list of lists",
        property: "secrets",
        class: "a problem message quotes a value holding a secret",
        applies: |case| case.language == Language::XML && nested_list(&case.doc),
    },
    // `codec::conform_arrays`: an HCL block where the schema wants a list
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
        class: "a save reads otherwise than the view",
        applies: |case| case.language != Language::XML && empty_text(&case.other),
    },
    // `bridge::schema_readings`: a value the schema takes in no reading
    // (`0` typed under `minimum: 1` over a host's text) is saved in the
    // host's type: the view's reading is kept only where the schema takes it.
    Known {
        finding: "documented: a value no reading of which the schema takes keeps the host's type",
        property: "edit_as",
        class: "a save reads otherwise than the view",
        applies: |case| {
            jsonschema::validator_for(&case.schema)
                .is_ok_and(|reference| !reference.is_valid(&case.other))
        },
    },
    // `bridge::text_scalars`: a number typed in another spelling (`08`,
    // `2.50`) where the host holds a number is that number.
    Known {
        finding: "documented: a number typed in another spelling is the host's number",
        property: "edit_as",
        class: "a save reads otherwise than the view",
        applies: |case| respelled_number(&case.doc, &case.other),
    },
    Known {
        finding: "documented: an empty XML element reads as an empty object",
        property: "suggestions",
        class: "a suggested key adds a problem elsewhere",
        applies: |case| case.language == Language::XML && empty_text(&case.doc),
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
    // SC38-3 (open, qa38-validate): the explanation under recursive
    // options with an error at every level; and by design (SC37-2) each
    // value is explained a bounded number of times, the work following
    // the option tree below that bound.
    Known {
        finding: "SC38-3 / options explained up to a fixed bound (SC37-2)",
        property: "work",
        class: "validate: OptionTried grows",
        applies: |_| true,
    },
    Known {
        finding: "SC38-3 / options explained up to a fixed bound (SC37-2)",
        property: "work",
        class: "validate: BranchWalk grows",
        applies: |case| !deprecates(&case.schema),
    },
    Known {
        finding: "SC38-3 / options explained up to a fixed bound (SC37-2)",
        property: "work",
        class: "validate: ErrorVisit grows",
        applies: |_| true,
    },
];

/// Whether `schema` has a list asking items (`minItems`, `contains`).
fn asks_list_items(schema: &Value) -> bool {
    let mut found = false;
    walk(schema, &mut |node| {
        found |= node.get("minItems").is_some() || node.get("contains").is_some();
    });
    found
}

/// Whether some `allOf` of `schema` lists values (`enum`, `const`) none of
/// which another member's `type` allows, or a schema lists none at all
/// (`enum: []`).
fn contradicting_all_of(schema: &Value) -> bool {
    let mut found = false;
    walk(schema, &mut |node| {
        found |= node
            .get("enum")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty);
        // A property two schemas applying together type apart (its own
        // `properties` beside an `allOf` member's or an always-taken
        // `then`'s).
        let always = node
            .get("if")
            .is_some_and(|condition| *condition == json!({}) || *condition == json!(true));
        let together: Vec<&Value> = std::iter::once(node)
            .chain(
                node.get("allOf")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten(),
            )
            .chain(node.get("then").filter(|_| always))
            .collect();
        let typed = |source: &Value, key: &str| {
            source
                .get("properties")?
                .get(key)?
                .get("type")?
                .as_str()
                .map(str::to_string)
        };
        for source in &together {
            for key in source
                .get("properties")
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|properties| properties.keys())
            {
                let types: Vec<String> = together
                    .iter()
                    .filter_map(|other| typed(other, key))
                    .collect();
                found |= types.iter().any(|one| {
                    types.iter().any(|other| {
                        one != other
                            && !matches!(
                                (one.as_str(), other.as_str()),
                                ("number", "integer") | ("integer", "number")
                            )
                    })
                });
            }
        }
        let Some(members) = node.get("allOf").and_then(Value::as_array) else {
            return;
        };
        let listed: Vec<&Value> = members
            .iter()
            .flat_map(|member| {
                member.get("const").into_iter().chain(
                    member
                        .get("enum")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten(),
                )
            })
            .collect();
        let types: Vec<&str> = members
            .iter()
            .filter_map(|member| member.get("type").and_then(Value::as_str))
            .collect();
        let kind = |value: &Value| match value {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        };
        found |= !listed.is_empty()
            && types.iter().any(|wanted| {
                listed.iter().all(|value| {
                    let have = kind(value);
                    have != *wanted && !(have == "number" && *wanted == "integer")
                })
            });
    });
    found
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

/// Whether `typed` writes, where `host` holds a number, text spelling a
/// number another way (`"08"`).
fn respelled_number(host: &Value, typed: &Value) -> bool {
    match (host, typed) {
        (Value::Number(_), Value::String(text)) => {
            crate::editor::decimal_text::decimal(text.trim(), false).is_some()
        }
        (Value::Object(host), Value::Object(typed)) => host.iter().any(|(key, value)| {
            typed
                .get(key)
                .is_some_and(|typed| respelled_number(value, typed))
        }),
        (Value::Array(host), Value::Array(typed)) => host
            .iter()
            .zip(typed)
            .any(|(host, typed)| respelled_number(host, typed)),
        _ => false,
    }
}

/// Whether `value` holds a list inside a list.
fn nested_list(value: &Value) -> bool {
    let mut found = false;
    walk(value, &mut |node| {
        found |= node
            .as_array()
            .is_some_and(|items| items.iter().any(Value::is_array));
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

/// Whether `schema` marks some field deprecated.
fn deprecates(schema: &Value) -> bool {
    schema.to_string().contains("\"deprecated\":true")
}

/// Whether `value` holds a list of one item.
fn one_item_list(value: &Value) -> bool {
    let mut found = false;
    walk(value, &mut |node| {
        found |= node.as_array().is_some_and(|items| items.len() == 1);
    });
    found
}

/// Whether `schema` asks items of a list whose items may be anything.
fn lists_of_anything(schema: &Value) -> bool {
    let mut found = false;
    walk(schema, &mut |node| {
        let asks = node.get("minItems").is_some() || node.get("contains").is_some();
        let anything = node
            .get("prefixItems")
            .and_then(Value::as_array)
            .is_none_or(|prefix| prefix.iter().all(|item| *item == json!({})))
            && node.get("items").is_none_or(|items| *items == json!({}));
        found |= asks && anything;
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
