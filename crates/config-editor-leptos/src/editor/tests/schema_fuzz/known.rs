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
    // FZ38-5 (new): XML reads a lone element as a one-item list where a
    // schema's list keywords have no `type` (`oneOf: [{prefixItems:
    // [{}]}]`), though the schema accepts the lone value itself: the view
    // reads `[false]` where its host holds `false`.
    Known {
        finding: "FZ38-5",
        property: "edit_as",
        class: "a save reads otherwise than the view",
        applies: |case| untyped_lists(&case.schema),
    },
    // FZ38-6 (new, schema reading): XML text is typed as the string one
    // `oneOf` option's property wants although that makes the object match
    // two options (`oneOf: [{}, {anyOf: [{properties: {port: {type:
    // string}}}]}]`, `<port>0</port>` read "0"): a valid document gets "matches
    // more than one of the oneOf options".
    Known {
        finding: "FZ38-6",
        property: "validation",
        class: "XML reads a valid document as an invalid one",
        applies: |_| true,
    },
    // FZ38-7 (new, schema reading): a secret marked through an option or
    // `allOf` member (`anyOf: [{properties: {size: {writeOnly: true,
    // maxLength: 0}}}, …]`) is quoted in its own problem's message
    // (`/b/b/size: "<secret>" is longer than 0 characters`), JSON, TOML and
    // HCL.
    Known {
        finding: "FZ38-7",
        property: "secrets",
        class: "a problem message holds a secret",
        applies: |_| true,
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
    // FZ38-16 (new, schema reading, SC37-1/SC38-2 class): XML view text
    // saved in the host's type where the view read it as the schema
    // types it: `allOf: [{type: string}]` over a host's `true` (the view
    // reads "true", valid; the host keeps `true`, refused), or conflicting
    // types (`const: 3` beside a pattern's boolean) saving "3" over a
    // host's string. Covers this whole class for hosts other than XML
    // until fixed.
    Known {
        finding: "FZ38-16",
        property: "edit_as",
        class: "a save reads otherwise than the view",
        applies: |case| case.language != Language::XML,
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
    // FZ38-9 (new, schema reading): a fill writes only the required
    // children its schema describes as an object with properties: a
    // required key with no property schema (`required: [d, b]`,
    // `properties: {b: {}}`), or an object schema with no `type`, is left
    // out.
    Known {
        finding: "FZ38-9",
        property: "fill",
        class: "a fill leaves out a required child",
        applies: |_| true,
    },
    // FZ38-10 (new, edge): a fill or suggestion does not weigh combined
    // constraints: a value matching two `oneOf` options (`oneOf:
    // [{const: "0"}, {}]`), `contains` beside `prefixItems`, an option no
    // value meets.
    Known {
        finding: "FZ38-10",
        property: "fill",
        class: "a fill writes a value its schema refuses (",
        applies: |case| !case.schema.to_string().contains("exclusiveM"),
    },
    Known {
        finding: "FZ38-10",
        property: "suggestions",
        class: "a suggested value is refused (oneOf)",
        applies: |_| true,
    },
    Known {
        finding: "FZ38-10",
        property: "suggestions",
        class: "a suggested key writes a value its schema refuses (",
        applies: |case| {
            case.language != Language::XML && !case.schema.to_string().contains("exclusiveM")
        },
    },
    // FZ38-11 (new, schema reading): a block's fills are listed from an
    // option or member the insert does not write (`allOf: [{const: 0},
    // {anyOf: [{required: [...]}]}]` writes `0` and lists the object's
    // children), so their values get no Tab stop.
    Known {
        finding: "FZ38-11",
        property: "fill",
        class: "a filled value gets no Tab stop",
        applies: |_| true,
    },
    // FZ38-12 (new): a fill's placeholder number ignores
    // `exclusiveMinimum` (`{type: integer, exclusiveMinimum: 0}` filled 0).
    Known {
        finding: "FZ38-12",
        property: "fill",
        class: "a fill writes a value its schema refuses (exclusiveMinimum)",
        applies: |_| true,
    },
    Known {
        finding: "FZ38-12",
        property: "suggestions",
        class: "a suggested key writes a value its schema refuses (exclusiveMinimum)",
        applies: |_| true,
    },
    // FZ38-13 (new, XML): a key suggested again in XML for a list adds an
    // empty item in front of the written ones, moving them to places
    // `prefixItems`/`items` type otherwise (`[{}, 0]`).
    Known {
        finding: "FZ38-13",
        property: "suggestions",
        class: "a suggested key writes a value its schema refuses (",
        applies: |case| case.language == Language::XML,
    },
    // FZ38-14 (new, schema reading): value suggestions read the field's
    // `properties` entry only, not the pattern property or `else` that
    // applies too (`properties: {x_one: {type: boolean}}` beside
    // `patternProperties: {^x_: {enum: [true]}}` offers `false`).
    Known {
        finding: "FZ38-14",
        property: "suggestions",
        class: "a suggested value is refused (",
        applies: |_| true,
    },
    // FZ38-15 (new): keys are suggested in an object that already holds
    // its `maxProperties`.
    Known {
        finding: "FZ38-15",
        property: "suggestions",
        class: "a suggested key adds a problem elsewhere (maxProperties)",
        applies: |_| true,
    },
    // FZ38-1 (new, round 38's random tests): a key a dependent schema or
    // `dependentRequired` makes require (or restrict) another is suggested
    // and accepted alone: the document gets a problem on the other key
    // (`required`, or the dependent schema's `const`/`enum`/`type`).
    Known {
        finding: "FZ38-1",
        property: "suggestions",
        class: "a suggested key adds a problem elsewhere",
        applies: |case| case.schema.to_string().contains("\"dependent"),
    },
    // FZ38-2 (new): a problem on a value that holds a secret field quotes
    // the whole value, the secret's text with it (`/x_two:
    // {"name":"…"} is not of type "string"`).
    Known {
        finding: "FZ38-2",
        property: "secrets",
        class: "a problem message quotes a value holding a secret",
        applies: |_| true,
    },
    // FZ38-4 (new): the deprecation warnings under a recursive
    // `oneOf`/`anyOf` holding a deprecated field: their walk of the
    // options doubles with each level (BranchWalk 607 / 4654 / 36925 at
    // depths 6 / 9 / 12; 41 ms at 12 against 11 ms with no `deprecated`),
    // and in HCL and XML each deprecated key's place is a read of the
    // whole document (DocumentRead 312 -> 1204 when the depth doubles).
    Known {
        finding: "FZ38-4",
        property: "work",
        class: "validate: its time grows faster than the document",
        applies: |case| deprecates(&case.schema),
    },
    Known {
        finding: "FZ38-4",
        property: "work",
        class: "validate: DocumentRead grows",
        applies: |case| deprecates(&case.schema),
    },
    Known {
        finding: "FZ38-4",
        property: "work",
        class: "validate: BranchWalk grows",
        applies: |case| deprecates(&case.schema),
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

/// Whether `schema` has list keywords (`items`, `prefixItems`) with no
/// `type`, which allow any value that is no list too.
fn untyped_lists(schema: &Value) -> bool {
    let mut found = false;
    walk(schema, &mut |node| {
        found |= (node.get("items").is_some() || node.get("prefixItems").is_some())
            && node.get("type").is_none();
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
