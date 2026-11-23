//! XML Schema on random schemas and documents (feature `xsd`): random XSDs
//! of the constructs JSON Schema and the format's checks hold (see
//! [`xsd_model`](super::xsd_model)), and documents valid for them or
//! changed in one place. The model is the reference: the editor's
//! problems agree with its verdict and with the compiled JSON Schema plus
//! the format's checks; a schema text broken anywhere compiles or says
//! why, never panics; a child the editor suggests is one the content model
//! has there, and accepting it fills what the model requires, in order.

use serde_json::{Map, Value, json};

use super::random::Rng;
use super::reference::messages;
use super::runner::{Case, Property, Verdict, failure, run};
use super::xsd_model::{
    CHANGES, attributes, children, document, max, model, model_with, mutate, name, valid,
    well_formed, xml, xsd,
};
use crate::editor::Language;
use crate::editor::diagnostics::Severity;
use crate::editor::schema::Schema;
use crate::editor::schema_formats::CheckedDocument;
use crate::editor::{SchemaFormat, SchemaInput};

/// What each `pick % CHANGES` changes in the document ([`mutate`]).
const CHANGE_NAMES: [&str; CHANGES] = [
    "unchanged",
    "another value",
    "a child left out",
    "a child written twice",
    "two children swapped",
    "an unknown child",
    "an attribute left out",
    "an unknown attribute",
    "a child written as an attribute",
];

fn options() -> Map<String, Value> {
    let mut options = Map::new();
    options.insert("root".into(), json!("config"));
    options
}

fn compile(text: &str, options: &Map<String, Value>) -> Result<Schema, String> {
    Schema::compile_as(SchemaFormat::XSD, &SchemaInput::new(text).options(options))
        .map_err(|error| error.0)
}

pub(super) fn draw(rng: &mut Rng, _index: usize) -> Case {
    let schema = model(rng);
    let doc = document(rng, &schema);
    Case {
        language: Language::XML,
        pick: rng.below(1 << 12),
        schema,
        doc,
        other: Value::Null,
    }
}

/// A case of the validation property: its sequences may hold choices
/// nested in them.
fn validation_draw(rng: &mut Rng, _index: usize) -> Case {
    let schema = model_with(rng, true);
    let doc = document(rng, &schema);
    Case {
        language: Language::XML,
        pick: rng.below(1 << 12),
        schema,
        doc,
        other: Value::Null,
    }
}

/// The loss an XSD of the model's constructs has: an `xs:sequence`'s
/// order, which only an XML text's check holds (these properties are on
/// XML texts).
const HELD_IN_XML: &str = "xs:sequence order";

/// The model's XSD, compiled; a failure when the editor refuses it or
/// says it loses something else (the model writes only what it holds).
fn compiled(case: &Case) -> Result<Option<(Schema, String)>, Verdict> {
    if !well_formed(&case.schema) {
        return Ok(None);
    }
    let text = xsd(&case.schema);
    match compile(&text, &options()) {
        Ok(schema)
            if schema
                .losses()
                .iter()
                .all(|loss| loss.construct == HELD_IN_XML) =>
        {
            Ok(Some((schema, text)))
        }
        Ok(schema) => Err(Verdict::Fails(failure(
            "an XSD of held constructs reports a loss",
            format!("{:?}\n{text}", schema.losses()),
        ))),
        Err(error) => Err(Verdict::Fails(failure(
            "an XSD of held constructs is refused",
            format!("{error}\n{text}"),
        ))),
    }
}

/// The editor's errors on `text`.
fn errors(schema: &Schema, text: &str) -> Vec<crate::editor::diagnostics::Diagnostic> {
    schema
        .validate(Language::XML, text)
        .into_iter()
        .filter(|problem| problem.severity == Severity::Error)
        .collect()
}

/// Whether the compiled JSON Schema refuses `text`'s value or a check of
/// the format finds a problem in it: what the editor's validation is made
/// of.
fn refused(schema: &Schema, text: &str) -> Option<bool> {
    let bundle = &schema.bundle;
    let behavior = Language::XML.behavior();
    let value = crate::editor::codec::read(behavior, text, Some(&bundle.schema)).ok()?;
    let reference = jsonschema::validator_for(schema.json()).ok()?;
    let strings = crate::strings::default_strings();
    let document = CheckedDocument {
        text,
        language: behavior,
        value: &value,
        schema: &bundle.schema,
        strings,
    };
    let checked = bundle.checks.iter().any(|check| {
        check
            .check(&document)
            .iter()
            .any(|problem| problem.severity == Severity::Error)
    });
    Some(!reference.is_valid(&value) || checked)
}

pub(super) fn validation_check(case: &Case) -> Verdict {
    let (schema, source) = match compiled(case) {
        Ok(Some(found)) => found,
        Ok(None) => return Verdict::Skipped,
        Err(verdict) => return verdict,
    };
    let change = CHANGE_NAMES[case.pick % CHANGES];
    let changed = mutate(&case.doc, case.pick);
    let Some(expected) = valid(&case.schema, &changed) else {
        return Verdict::Skipped;
    };
    let text = xml(&changed);
    let found = errors(&schema, &text);
    let seen = || {
        format!(
            "{change}; the XSD finds it {}:\n{text}problems:\n{}\n{source}",
            if expected { "valid" } else { "invalid" },
            messages(&found)
        )
    };
    if expected && !found.is_empty() {
        return Verdict::Fails(failure(
            format!("a problem on a document the XSD finds valid ({change})"),
            seen(),
        ));
    }
    if !expected && found.is_empty() {
        return Verdict::Fails(failure(
            format!("no problem on a document the XSD finds invalid ({change})"),
            seen(),
        ));
    }
    match refused(&schema, &text) {
        Some(refused) if refused == found.is_empty() => Verdict::Fails(failure(
            "the editor's problems disagree with the compiled schema and its checks",
            format!(
                "compiled schema and checks refuse it: {refused}\n{}",
                seen()
            ),
        )),
        _ => Verdict::Holds,
    }
}

/// The model's XSD broken in one place `pick` chooses (cut, an unknown
/// type, a tag renamed, a stray character), with options that may be
/// wrong too.
fn broken(text: &str, pick: usize) -> (String, Map<String, Value>) {
    let chars: Vec<char> = text.chars().collect();
    let at = (pick / 16) % chars.len().max(1);
    let mut out: String;
    match pick % 8 {
        0 => out = chars[..at].iter().collect(),
        1 => {
            out = chars[..at].iter().collect();
            out.extend(chars.iter().skip(at + 1 + (pick / 4096) % 12));
        }
        2 => out = text.replacen("xs:int", "xs:nope", 1),
        3 => out = text.replacen("type=\"t1\"", "type=\"t9\"", 1),
        4 => out = text.replacen("xs:sequence", "xs:sequenc", 1),
        5 => {
            out = chars[..at].iter().collect();
            out.push(['<', '&', '>', '"', '\u{0}', '\u{fffd}'][(pick / 4096) % 6]);
            out.extend(&chars[at..]);
        }
        6 => out = text.replacen("minInclusive value=\"", "minInclusive value=\"x", 1),
        _ => out = text.replacen("maxOccurs=\"", "maxOccurs=\"-", 1),
    }
    let mut options = Map::new();
    match (pick / 8) % 6 {
        0 => {}
        1 => {
            options.insert("root".into(), json!("nope"));
        }
        2 => {
            options.insert("lax".into(), json!("yes"));
        }
        3 => {
            options.insert("configRoot".into(), json!("config"));
        }
        4 => {
            options.insert("root".into(), json!(1));
        }
        _ => {
            options.insert("rooot".into(), json!("config"));
        }
    }
    (out, options)
}

pub(super) fn compile_check(case: &Case) -> Verdict {
    if !well_formed(&case.schema) {
        return Verdict::Skipped;
    }
    let (text, options) = broken(&xsd(&case.schema), case.pick);
    match compile(&text, &options) {
        Err(message) => {
            if message.trim().is_empty() {
                return Verdict::Fails(failure(
                    "a refused XSD says nothing",
                    format!("{options:?}\n{text}"),
                ));
            }
            Verdict::Holds
        }
        Ok(schema) => {
            if jsonschema::validator_for(schema.json()).is_err()
                || Schema::compile(&schema.json().to_string()).is_err()
            {
                return Verdict::Fails(failure(
                    "a compiled XSD gives a JSON Schema that does not compile",
                    format!("{}\n{text}", schema.json()),
                ));
            }
            if let Some(loss) = schema
                .losses()
                .iter()
                .find(|loss| loss.message.trim().is_empty())
            {
                return Verdict::Fails(failure("a loss says nothing", format!("{loss:?}\n{text}")));
            }
            // Its validation and suggestions run on a document (no panic).
            let document = xml(&case.doc);
            let _ = schema.validate(Language::XML, &document);
            let _ = schema.suggestions(Language::XML, &document, 1);
            Verdict::Holds
        }
    }
}

/// A complex element of `document` (by `pick`) and a child its model has
/// room for there, with the child's place among the element's children:
/// the path to the document element, the child's model and the index
/// it goes at.
fn place<'m>(
    model: &'m Value,
    document: &Value,
    pick: usize,
) -> Option<(Vec<usize>, &'m Value, usize)> {
    let mut found = Vec::new();
    candidates(model, document, &mut Vec::new(), &mut found);
    if found.is_empty() {
        return None;
    }
    let chosen = found.swap_remove(pick % found.len());
    Some(chosen)
}

fn candidates<'m>(
    model: &'m Value,
    document: &Value,
    path: &mut Vec<usize>,
    out: &mut Vec<(Vec<usize>, &'m Value, usize)>,
) {
    let Some((items, choice)) = children(model) else {
        return;
    };
    let empty = Vec::new();
    let present = document
        .get("c")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    if !choice {
        for (rank, item) in items.iter().enumerate() {
            let count = present
                .iter()
                .filter(|child| name(child) == name(item))
                .count();
            if max(item).is_some_and(|most| count >= most) {
                continue;
            }
            // After every child declared before it, and its own.
            let at = present
                .iter()
                .rposition(|child| {
                    items
                        .iter()
                        .position(|declared| name(declared) == name(child))
                        .is_some_and(|declared| declared <= rank)
                })
                .map_or(0, |index| index + 1);
            out.push((path.clone(), item, at));
        }
    } else if present.is_empty() {
        for item in items {
            out.push((path.clone(), item, 0));
        }
    }
    for (index, child) in present.iter().enumerate() {
        if let Some(item) = items.iter().find(|item| name(item) == name(child)) {
            path.push(index);
            candidates(item, child, path, out);
            path.pop();
        }
    }
}

/// `document` as XML with a key being typed (`<`) as child `at` of the
/// element at `path`, and the caret's byte offset after it.
fn typing(document: &Value, path: &[usize], at: usize) -> (String, usize) {
    let mut marked = document.clone();
    let mut node = &mut marked;
    for index in path {
        node = &mut node["c"][*index];
    }
    if node.get("c").and_then(Value::as_array).is_none() {
        node["c"] = json!([]);
    }
    if let Some(Value::Array(children)) = node.get_mut("c") {
        children.insert(at.min(children.len()), json!({"n": "\u{1}"}));
    }
    let text = xml(&marked);
    let mark = text.find("<\u{1}/>").unwrap_or(0);
    let caret = mark + 1;
    (text.replacen("<\u{1}/>", "<", 1), caret)
}

/// `text` read back as a model document; `None` when it does not parse.
fn parse(text: &str) -> Option<Value> {
    use quick_xml::Reader;
    use quick_xml::events::Event;
    let mut reader = Reader::from_str(text);
    let mut stack: Vec<Value> = Vec::new();
    let mut root = None;
    let open = |element: &quick_xml::events::BytesStart<'_>| -> Option<Value> {
        let mut attributes = Map::new();
        for attribute in element.attributes() {
            let attribute = attribute.ok()?;
            attributes.insert(
                AsRef::<str>::as_ref(&attribute.key).to_string(),
                json!(attribute.value.to_string()),
            );
        }
        let name = element.name();
        let name: &str = name.as_ref();
        Some(json!({"n": name, "a": attributes, "c": []}))
    };
    loop {
        match reader.read_event().ok()? {
            Event::Eof => break,
            Event::Start(element) => stack.push(open(&element)?),
            Event::Empty(element) => {
                let node = open(&element)?;
                close(&mut stack, &mut root, node);
            }
            Event::End(_) => {
                let node = stack.pop()?;
                close(&mut stack, &mut root, node);
            }
            Event::Text(content) => {
                let text = content.xml10_content().trim().to_string();
                if let (false, Some(node)) = (text.is_empty(), stack.last_mut()) {
                    node["t"] = json!(text);
                }
            }
            _ => {}
        }
    }
    root
}

fn close(stack: &mut [Value], root: &mut Option<Value>, mut node: Value) {
    // An element with text has no children or attributes to keep empty.
    if node.get("t").is_some() {
        if node["c"].as_array().is_some_and(Vec::is_empty) {
            node.as_object_mut().map(|map| map.remove("c"));
        }
        if node["a"].as_object().is_some_and(Map::is_empty) {
            node.as_object_mut().map(|map| map.remove("a"));
        }
    }
    match stack
        .last_mut()
        .and_then(|parent| parent["c"].as_array_mut())
    {
        Some(children) => children.push(node),
        None => *root = Some(node),
    }
}

/// `document` with each empty value (a text or attribute left to type)
/// given a valid one of its model, so the model judges what was written
/// around them (an empty value with no model stays empty).
fn slots_filled(model: &Value, document: &mut Value) {
    let mut rng = Rng::new(7);
    match children(model) {
        Some((items, _)) => {
            if let Some(Value::Object(given)) = document.get_mut("a") {
                for attribute in attributes(model) {
                    let key = name(attribute).unwrap_or("");
                    if given.get(key).is_some_and(|value| value == "") {
                        let kind = json!({"n": key, "s": attribute["s"].clone()});
                        let sample = super::xsd_model::document(&mut rng, &kind);
                        given.insert(key.into(), sample["t"].clone());
                    }
                }
            }
            if let Some(Value::Array(found)) = document.get_mut("c") {
                for child in found.iter_mut() {
                    if let Some(item) = items.iter().find(|item| name(item) == name(child)) {
                        slots_filled(item, child);
                    }
                }
            }
        }
        None => {
            let empty = document.get("t").is_none_or(|text| text == "")
                && document
                    .get("c")
                    .and_then(Value::as_array)
                    .is_none_or(Vec::is_empty);
            if empty {
                let sample = super::xsd_model::document(&mut rng, model);
                *document = sample;
            }
        }
    }
}

pub(super) fn assist_check(case: &Case) -> Verdict {
    let (schema, source) = match compiled(case) {
        Ok(Some(found)) => found,
        Ok(None) => return Verdict::Skipped,
        Err(verdict) => return verdict,
    };
    if valid(&case.schema, &case.doc) != Some(true) {
        return Verdict::Skipped;
    }
    let Some((path, item, at)) = place(&case.schema, &case.doc, case.pick) else {
        return Verdict::Skipped;
    };
    let wanted = name(item).unwrap_or("");
    let (text, caret) = typing(&case.doc, &path, at);
    let caret = text[..caret].encode_utf16().count();
    let offered = schema.suggestions(Language::XML, &text, caret);
    let labels: Vec<&str> = offered.iter().map(|option| option.label()).collect();
    let seen =
        |what: &str| format!("at {path:?} child {at}, wanting {wanted}:\n{text}{what}\n{source}");
    let Some(option) = offered
        .iter()
        .find(|option| !option.is_value() && option.label() == wanted)
    else {
        return Verdict::Fails(failure(
            "a child the content model has room for is not suggested",
            seen(&format!("offered {labels:?}")),
        ));
    };
    let (accepted, _) = option.accept(&text);
    let Some(parsed) = parse(&accepted) else {
        return Verdict::Fails(failure(
            "an accepted child is no well-formed XML",
            seen(&format!("accepted:\n{accepted}")),
        ));
    };
    let mut read = parsed.clone();
    slots_filled(&case.schema, &mut read);
    if valid(&case.schema, &read) != Some(false) {
        return Verdict::Holds;
    }
    let mut picked = read.clone();
    choices_picked(&case.schema, &mut picked);
    let mut lifted = parsed;
    attributes_lifted(&case.schema, &mut lifted);
    slots_filled(&case.schema, &mut lifted);
    choices_picked(&case.schema, &mut lifted);
    let class = if valid(&case.schema, &picked) != Some(false) {
        "a fill leaves a choice of several options for the user to pick"
    } else if valid(&case.schema, &lifted) != Some(false) {
        "a fill writes a list item's required attributes as elements or leaves the list out"
    } else {
        "accepting a suggested child leaves the document invalid beyond its empty values"
    };
    Verdict::Fails(failure(
        class,
        seen(&format!(
            "accepted:\n{accepted}read with its slots filled: {read}"
        )),
    ))
}

/// `document` with each child element its model has as an attribute (and
/// not as an element) made that attribute, its text the value.
fn attributes_lifted(model: &Value, document: &mut Value) {
    let Some((items, _)) = children(model) else {
        return;
    };
    let declared: Vec<&str> = attributes(model)
        .iter()
        .filter_map(|attribute| name(attribute))
        .collect();
    let Some(Value::Array(found)) = document.get("c").cloned() else {
        return;
    };
    let mut kept = Vec::new();
    for mut child in found {
        let key = name(&child).unwrap_or("").to_string();
        let element = items.iter().any(|item| name(item) == Some(key.as_str()));
        if !element && declared.contains(&key.as_str()) {
            let text = child.get("t").cloned().unwrap_or(json!(""));
            if !document["a"].is_object() {
                document["a"] = json!({});
            }
            document["a"][key.as_str()] = text;
            continue;
        }
        if let Some(item) = items.iter().find(|item| name(item) == Some(key.as_str())) {
            attributes_lifted(item, &mut child);
        }
        kept.push(child);
    }
    // A required list whose items have required attributes, left out (the
    // fill cannot write its items' attributes): its items, as typed.
    missing_added(model, &mut kept, &|item| {
        attributes(item)
            .iter()
            .any(|attribute| attribute.get("req").and_then(Value::as_bool) == Some(true))
    });
    document["c"] = Value::Array(kept);
}

/// `document` with each choice of several options that has none written
/// given its first option (a valid one): what the user picks.
fn choices_picked(model: &Value, document: &mut Value) {
    let Some((items, choice)) = children(model) else {
        return;
    };
    let Some(Value::Array(found)) = document.get_mut("c") else {
        return;
    };
    if choice && items.len() > 1 && found.is_empty() {
        found.push(super::xsd_model::document(&mut Rng::new(7), &items[0]));
    }
    // A required list whose items are such a choice, left out: its items.
    missing_added(model, found, &|item| {
        children(item).is_some_and(|(options, choice)| choice && options.len() > 1)
    });
    for child in found.iter_mut() {
        if let Some(item) = items.iter().find(|item| name(item) == name(child)) {
            choices_picked(item, child);
        }
    }
}

/// `found` (the children of an element of sequence `model`, or of a
/// choice of one option, which XSD requires as a sequence of it) with a
/// valid one of each required child `wanted` says the fill leaves out, in
/// its place.
fn missing_added(model: &Value, found: &mut Vec<Value>, wanted: &dyn Fn(&Value) -> bool) {
    let Some((items, choice)) = children(model) else {
        return;
    };
    if choice && items.len() != 1 {
        return;
    }
    for (rank, item) in items.iter().enumerate() {
        let missing = !found.iter().any(|child| name(child) == name(item));
        if missing && wanted(item) && item.get("min").and_then(Value::as_u64) != Some(0) {
            let at = found
                .iter()
                .rposition(|child| {
                    items
                        .iter()
                        .position(|declared| name(declared) == name(child))
                        .is_some_and(|declared| declared < rank)
                })
                .map_or(0, |index| index + 1);
            found.insert(at, super::xsd_model::document(&mut Rng::new(7), item));
        }
    }
}

/// An `xs:choice` of one element requires it as a sequence of it would:
/// a required list there whose items are a choice of several is left out
/// for the user to pick, as in a sequence.
#[test]
fn a_choice_of_one_list_is_judged_as_a_sequence_of_it() {
    let case = Case {
        language: Language::XML,
        pick: 1861,
        schema: json!({"seq":[{"s":{"text":[]},"n":"mode"},{"s":{"dec":[0,0]},"n":"i","min":0},{"alt":[{"alt":[{"s":{"text":["tcp"]},"n":"mode"},{"s":{"text":[]},"n":"opt","min":1,"max":1,"named":true}],"max":"u","n":"path"}],"min":0,"n":"host"},{"seq":[{"alt":[{"s":{"text":[]},"n":"mode","min":1,"max":1},{"s":{"text":["udp","tcp","auto"]},"n":"user","min":1,"max":1}],"at":[{"n":"id","s":{"text":["tcp"]},"req":true},{"n":"on","s":{"text":["tcp"]},"req":true}],"named":true,"n":"path","min":0,"max":2},{"s":{"text":["udp","off"]},"n":"port","min":1,"max":1,"named":true}],"at":[{"n":"on","s":{"text":["tcp","off"]},"req":false},{"n":"id","s":{"bool":1},"req":false}],"n":"rule","min":1,"max":2}],"at":[{"n":"kind","s":{"dec":[0,2]},"req":true}],"n":"config"}),
        doc: json!({"n":"config","a":{"kind":"1"},"c":[{"n":"mode","t":"a"},{"n":"rule","c":[{"n":"port","t":"udp"}]}]}),
        other: Value::Null,
    };
    if let Verdict::Fails(found) = assist_check(&case) {
        assert!(
            super::known::excused("xsd_assist", &found.class, &case).is_some(),
            "{}\n{}",
            found.class,
            found.detail
        );
    }
}

#[test]
fn random_documents_get_problems_exactly_when_the_xsd_finds_them_invalid() {
    run(&Property {
        name: "xsd_validation",
        check_path: "super::xsd::validation_check",
        cases: 300,
        shrink_budget: 1500,
        draw: validation_draw,
        check: validation_check,
    });
}

#[test]
fn a_broken_xsd_compiles_or_says_why_and_never_panics() {
    run(&Property {
        name: "xsd_compile",
        check_path: "super::xsd::compile_check",
        cases: 300,
        shrink_budget: 800,
        draw,
        check: compile_check,
    });
}

#[test]
fn suggested_children_follow_the_xsd_and_fill_what_it_requires() {
    run(&Property {
        name: "xsd_assist",
        check_path: "super::xsd::assist_check",
        cases: 200,
        shrink_budget: 1000,
        draw,
        check: assist_check,
    });
}
