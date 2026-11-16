//! XML text has no types, so its values are typed by the schema. Where a
//! field's options allow a number or boolean and also a string listed
//! (`oneOf [integer, const "auto"]`), in every shape of union (`oneOf`,
//! `anyOf`, `allOf`, `$ref`, `if`/`then`/`else`, object options told
//! apart by a property), the text is read as an option it fits: `4` is
//! the number, `auto` the string. The diagnostics agree with JSON's for
//! the same values, every Edit-as view shows the type, saves keep the
//! host's text, and a JSON host edited as XML has no false errors.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::editor::Language;
use crate::editor::bridge::Bridge;
use crate::editor::schema::{Schema, build_schema};

const SCHEMA: &str = r##"{"type":"object","properties":{"r":{"type":"object","properties":{
    "k":{"type":"string"},
    "int_or_const":{"oneOf":[{"type":"integer","minimum":1},{"const":"auto"}]},
    "int_or_typed_enum":{"oneOf":[{"type":"integer"},{"type":"string","enum":["auto"]}]},
    "int_or_enum":{"anyOf":[{"type":"integer"},{"enum":["auto","max"]}]},
    "bool_or_typed_enum":{"anyOf":[{"type":"boolean"},{"type":"string","enum":["yes","no"]}]},
    "bool_or_enum":{"anyOf":[{"type":"boolean"},{"enum":["verbose"]}]},
    "num_or_const":{"oneOf":[{"type":"number"},{"const":"max"}]},
    "if_string":{"if":{"type":"string"},"then":{"const":"auto"},"else":{"type":"integer"}},
    "by_ref":{"$ref":"#/$defs/intOrAuto"},
    "all_of_ref":{"allOf":[{"$ref":"#/$defs/intOrAuto"}]},
    "nested":{"anyOf":[{"oneOf":[{"type":"integer"},{"const":"auto"}]},{"type":"null"}]},
    "fits_text":{"oneOf":[{"type":"integer","minimum":10},{"type":"string"}]},
    "list":{"type":"array","items":{"oneOf":[{"type":"integer"},{"const":"auto"}]}},
    "tagged":{"oneOf":[
        {"type":"object","properties":{"t":{"const":"n"},"v":{"type":"integer"}}},
        {"type":"object","properties":{"t":{"const":"s"},"v":{"type":"string"}}}]},
    "decided":{"type":"object","properties":{"kind":{"enum":["a","b"]}},
        "if":{"properties":{"kind":{"const":"a"}},"required":["kind"]},
        "then":{"properties":{"v":{"type":"integer"}}},
        "else":{"properties":{"v":{"type":"string"}}}}
}}},"$defs":{"intOrAuto":{"oneOf":[{"type":"integer"},{"const":"auto"}]}}}"##;

/// A field written in XML (inside `<r><k>x</k>…</r>`), and the value it
/// is (what JSON would write).
fn cases() -> Vec<(&'static str, &'static str, Value)> {
    vec![
        ("int_or_const", "<int_or_const>4</int_or_const>", json!(4)),
        (
            "int_or_const",
            "<int_or_const>auto</int_or_const>",
            json!("auto"),
        ),
        ("int_or_const", "<int_or_const>0</int_or_const>", json!(0)),
        (
            "int_or_typed_enum",
            "<int_or_typed_enum>4</int_or_typed_enum>",
            json!(4),
        ),
        ("int_or_enum", "<int_or_enum>3</int_or_enum>", json!(3)),
        (
            "int_or_enum",
            "<int_or_enum>max</int_or_enum>",
            json!("max"),
        ),
        (
            "bool_or_typed_enum",
            "<bool_or_typed_enum>true</bool_or_typed_enum>",
            json!(true),
        ),
        (
            "bool_or_typed_enum",
            "<bool_or_typed_enum>yes</bool_or_typed_enum>",
            json!("yes"),
        ),
        (
            "bool_or_enum",
            "<bool_or_enum>true</bool_or_enum>",
            json!(true),
        ),
        (
            "num_or_const",
            "<num_or_const>2.5</num_or_const>",
            json!(2.5),
        ),
        ("num_or_const", "<num_or_const>4</num_or_const>", json!(4)),
        ("if_string", "<if_string>4</if_string>", json!(4)),
        ("if_string", "<if_string>auto</if_string>", json!("auto")),
        ("by_ref", "<by_ref>7</by_ref>", json!(7)),
        ("all_of_ref", "<all_of_ref>7</all_of_ref>", json!(7)),
        ("nested", "<nested>4</nested>", json!(4)),
        ("fits_text", "<fits_text>4</fits_text>", json!("4")),
        ("fits_text", "<fits_text>12</fits_text>", json!(12)),
        (
            "list",
            "<list>1</list><list>auto</list>",
            json!([1, "auto"]),
        ),
        (
            "tagged",
            "<tagged><t>n</t><v>5</v></tagged>",
            json!({"t": "n", "v": 5}),
        ),
        (
            "tagged",
            "<tagged><t>s</t><v>123</v></tagged>",
            json!({"t": "s", "v": "123"}),
        ),
        (
            "tagged",
            "<tagged t=\"n\" v=\"5\"/>",
            json!({"t": "n", "v": 5}),
        ),
        (
            "tagged",
            "<tagged t=\"s\" v=\"123\"/>",
            json!({"t": "s", "v": "123"}),
        ),
        (
            "decided",
            "<decided><kind>a</kind><v>5</v></decided>",
            json!({"kind": "a", "v": 5}),
        ),
        (
            "decided",
            "<decided><kind>b</kind><v>5</v></decided>",
            json!({"kind": "b", "v": "5"}),
        ),
    ]
}

fn host(field: &str) -> String {
    format!("<r>\n  <k>x</k>\n  {field}\n</r>\n")
}

fn messages(schema: &Schema, language: Language, text: &str) -> Vec<String> {
    schema
        .validate(language, text)
        .into_iter()
        .map(|problem| problem.message)
        .collect()
}

#[test]
fn xml_text_is_checked_as_the_option_it_fits_as_json_is() {
    let schema = Schema::compile(SCHEMA).unwrap();
    let mut failures = Vec::new();
    for (name, field, value) in cases() {
        let json = json!({"r": {"k": "x", name: value}}).to_string();
        let expected = messages(&schema, Language::JSON, &json);
        let found = messages(&schema, Language::XML, &host(field));
        if found != expected {
            failures.push(format!("{field}: {found:?}, JSON {value}: {expected:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_view_of_an_xml_host_shows_the_type_and_saves_the_host_s_text() {
    let bundle = Arc::new(build_schema(SCHEMA).unwrap());
    let mut failures = Vec::new();
    for (name, field, value) in cases() {
        let host = host(field);
        for view in [
            Language::YAML,
            Language::JSON,
            Language::TOML,
            Language::HCL,
        ] {
            let bridge = Bridge::new(Language::XML, view, Some(bundle.clone())).unwrap();
            let shown = bridge.to_edit(&host).unwrap().text;
            let read = view.parse(&shown).unwrap();
            if read.pointer(&format!("/r/{name}")) != Some(&value) {
                failures.push(format!("{field} in {view}: {shown}"));
                continue;
            }
            let edited = shown.replacen('x', "y", 1);
            match bridge.to_host(&edited, &host) {
                Ok(saved) if saved == host.replace("<k>x</k>", "<k>y</k>") => {}
                other => failures.push(format!("{field} saved from {view}: {other:?}")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_host_edited_as_xml_reads_its_types_back() {
    let schema = Schema::compile(SCHEMA).unwrap();
    let bundle = Arc::new(build_schema(SCHEMA).unwrap());
    let mut failures = Vec::new();
    for (name, field, value) in cases() {
        let document = json!({"r": {"k": "x", name: value}});
        let expected = messages(&schema, Language::JSON, &document.to_string());
        for host_language in [Language::JSON, Language::YAML, Language::TOML] {
            let host = host_language.serialize(&document).unwrap();
            let bridge = Bridge::new(host_language, Language::XML, Some(bundle.clone())).unwrap();
            let shown = bridge.to_edit(&host).unwrap().text;
            let found = messages(&schema, Language::XML, &shown);
            if found != expected {
                failures.push(format!(
                    "{field} from {host_language}: {found:?} in {shown}"
                ));
                continue;
            }
            let edited = shown.replacen('x', "y", 1);
            let saved = bridge
                .to_host(&edited, &host)
                .map(|saved| host_language.parse(&saved));
            let mut wanted = document.clone();
            wanted["r"]["k"] = json!("y");
            match saved {
                Ok(Ok(saved)) if saved == wanted => {}
                other => failures.push(format!("{field} saved to {host_language}: {other:?}")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Typing the text by the options resolves each value's schemas once, at
/// any size: walks grow with the values, not faster.
#[test]
fn typing_xml_text_by_the_options_walks_each_value_s_schemas_once() {
    use crate::editor::completion::conform_arrays;
    use crate::editor::language::Repetition;
    use crate::editor::work_count::{Work, counted};
    let bundle = build_schema(SCHEMA).unwrap();
    let walks = |copies: usize| {
        let fields: String = cases()
            .iter()
            .map(|(_, field, _)| *field)
            .collect::<Vec<_>>()
            .concat();
        let items = "<list>1</list><list>auto</list>".repeat(copies);
        let mut value = Language::XML.parse(&host(&(fields + &items))).unwrap();
        counted(Work::BranchWalk, || {
            conform_arrays(&mut value, &bundle.schema, Repetition::Elements);
        })
        .1
    };
    let (small, large) = (walks(50), walks(200));
    // 150 more list items each cost one walk.
    assert_eq!(large - small, 300, "{small} then {large}");
}
