//! XML text read by the schema in every spelling and every path: a number
//! written another way than JSON writes it (`1.50`, `1e3`, ` 4 `) is that
//! number where the schema wants one (validation, conversion, every view
//! of an XML host, whose saves keep the text as written); text with spaces
//! around it is the string with them where the schema wants a string;
//! empty text is `null` where the schema allows `null` and not a string;
//! and a value typed in an XML view of another host is saved as the view
//! read and validated it, not in the host's type, where the schema allows
//! only the view's reading.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::editor::Language;
use crate::editor::bridge::Bridge;
use crate::editor::diagnostics::{Schema, build_schema};

const NUMBERS: &str = r##"{"type":"object","properties":{"r":{"type":"object","properties":{
    "k":{"type":"string"},
    "n":{"type":"number"},
    "i":{"type":"integer"},
    "nb":{"type":["number","boolean","null"]},
    "nc":{"oneOf":[{"type":"number"},{"const":"max"}]},
    "nr":{"$ref":"#/$defs/n"},
    "nl":{"type":"array","items":{"type":"number"}}
}}},"$defs":{"n":{"allOf":[{"type":"number","minimum":-5}]}}}"##;

/// Number spellings JSON does not write, the number each is, and whether
/// it is whole (an `integer` takes it).
fn spellings() -> Vec<(&'static str, f64, bool)> {
    vec![
        ("1.50", 1.5, false),
        ("1e3", 1000.0, true),
        ("1E3", 1000.0, true),
        (".5", 0.5, false),
        ("5.", 5.0, true),
        ("-0", 0.0, true),
        ("+4", 4.0, true),
        ("007", 7.0, true),
        (" 4 ", 4.0, true),
        ("\n  4\n", 4.0, true),
        ("-1.5e-3", -0.0015, false),
    ]
}

fn messages(schema: &Schema, language: Language, text: &str) -> Vec<String> {
    schema
        .validate(language, text)
        .into_iter()
        .map(|problem| problem.message)
        .collect()
}

/// The fields of `NUMBERS` that take `whole` numbers, as elements (or
/// attributes of `<r>`) holding `text`.
fn fields(text: &str, whole: bool, attributes: bool) -> (String, Vec<&'static str>) {
    let mut names = vec!["n", "nb", "nc", "nr", "nl"];
    if whole {
        names.push("i");
    }
    let text = text.replace('\n', if attributes { " " } else { "\n" });
    let xml = if attributes {
        let pairs: Vec<String> = names
            .iter()
            .filter(|name| **name != "nl")
            .map(|name| format!("{name}=\"{text}\""))
            .collect();
        format!("<r {}>\n  <k>x</k>\n</r>\n", pairs.join(" "))
    } else {
        let elements: String = names
            .iter()
            .map(|name| format!("  <{name}>{text}</{name}>\n"))
            .collect();
        format!("<r>\n  <k>x</k>\n{elements}</r>\n")
    };
    (xml, names)
}

#[test]
fn a_number_in_any_spelling_is_that_number_where_the_schema_wants_one() {
    let schema = Schema::compile(NUMBERS).unwrap();
    let mut failures = Vec::new();
    for (text, number, whole) in spellings() {
        for attributes in [false, true] {
            let (xml, names) = fields(text, whole, attributes);
            let found = messages(&schema, Language::XML, &xml);
            if !found.is_empty() {
                failures.push(format!("{xml:?}: {found:?}"));
            }
            let Ok(converted) = schema.convert(Language::XML, &xml, Language::JSON) else {
                failures.push(format!("{xml:?} does not convert"));
                continue;
            };
            let read: Value = serde_json::from_str(&converted.text).unwrap();
            for name in names.iter().filter(|name| !(attributes && **name == "nl")) {
                let at = read.pointer(&format!("/r/{name}"));
                let at =
                    at.and_then(|at| at.as_array().and_then(|items| items.first()).or(Some(at)));
                if at.and_then(Value::as_f64) != Some(number) {
                    failures.push(format!("{xml:?} converts {name} to {at:?}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_view_of_an_xml_host_shows_the_number_and_saves_its_spelling() {
    let bundle = Arc::new(build_schema(NUMBERS).unwrap());
    let mut failures = Vec::new();
    for (text, number, whole) in spellings() {
        let (host, _) = fields(text, whole, false);
        for view in [
            Language::YAML,
            Language::JSON,
            Language::TOML,
            Language::HCL,
        ] {
            let bridge = Bridge::new(Language::XML, view, Some(bundle.clone())).unwrap();
            let Ok(shown) = bridge.to_edit(&host) else {
                failures.push(format!("{host:?} is not shown in {view}"));
                continue;
            };
            let read = view.parse(&shown.text).unwrap();
            if read.pointer("/r/n").and_then(Value::as_f64) != Some(number) {
                failures.push(format!("{host:?} in {view}: {}", shown.text));
                continue;
            }
            let edited = shown.text.replacen('x', "y", 1);
            match bridge.to_host(&edited, &host) {
                Ok(saved) if saved == host.replace("<k>x</k>", "<k>y</k>") => {}
                other => failures.push(format!("{host:?} saved from {view}: {other:?}")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

const TEXTS: &str = r##"{"type":"object","properties":{"r":{"type":"object","properties":{
    "s":{"type":"string"},
    "sn":{"type":["string","null"]},
    "in":{"anyOf":[{"type":"integer"},{"type":"null"}]},
    "nl":{"type":"null"},
    "en":{"enum":[1,null]}
}}}}"##;

#[test]
fn spaced_text_is_a_string_with_its_spaces_and_empty_text_is_null_where_allowed() {
    let schema = Schema::compile(TEXTS).unwrap();
    let cases = [
        ("<r><s> 4 </s></r>\n", "/r/s", json!(" 4 ")),
        ("<r><s> true </s></r>\n", "/r/s", json!(" true ")),
        ("<r s=\" 4 \"/>\n", "/r/s", json!(" 4 ")),
        ("<r><sn/></r>\n", "/r/sn", json!("")),
        ("<r><in/></r>\n", "/r/in", Value::Null),
        ("<r><in></in></r>\n", "/r/in", Value::Null),
        ("<r in=\"\"/>\n", "/r/in", Value::Null),
        ("<r><in> 4 </in></r>\n", "/r/in", json!(4)),
        ("<r><nl/></r>\n", "/r/nl", Value::Null),
        ("<r en=\"\"/>\n", "/r/en", Value::Null),
    ];
    let mut failures = Vec::new();
    for (xml, pointer, value) in cases {
        let found = messages(&schema, Language::XML, xml);
        if !found.is_empty() {
            failures.push(format!("{xml:?}: {found:?}"));
        }
        let converted = schema.convert(Language::XML, xml, Language::JSON).unwrap();
        let read: Value = serde_json::from_str(&converted.text).unwrap();
        if read.pointer(pointer) != Some(&value) {
            failures.push(format!("{xml:?} converts to {}", converted.text));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

const UNIONS: &str = r##"{"type":"object","properties":{"server":{"type":"object","properties":{
    "workers":{"oneOf":[{"type":"integer","minimum":1},{"const":"auto"}]},
    "debug":{"anyOf":[{"type":"boolean"},{"enum":["verbose"]}]},
    "flag":{"type":"boolean"},
    "ratio":{"type":"number"},
    "nothing":{"type":["null","integer"]},
    "name":{"anyOf":[{"type":"integer"},{"type":"string"}]},
    "mode":{"if":{"type":"string"},"then":{"enum":["a","b"]},"else":{"type":"integer"}},
    "by_ref":{"$ref":"#/$defs/intOrAuto"},
    "items":{"type":"array","items":{"oneOf":[{"type":"integer"},{"const":"auto"}]}}
}}},"$defs":{"intOrAuto":{"oneOf":[{"type":"integer"},{"const":"auto"}]}}}"##;

/// A field a host holds as a string, the text typed over it in an XML
/// view, and the value saved: what the view reads where the schema allows
/// only that, the host's type where it allows both.
fn typed_over_strings() -> Vec<(&'static str, &'static str, &'static str, Value)> {
    vec![
        ("workers", "auto", "8", json!(8)),
        ("debug", "verbose", "true", json!(true)),
        ("flag", "auto", "true", json!(true)),
        ("ratio", "auto", "1.50", json!(1.5)),
        ("ratio", "auto", "2", json!(2)),
        ("by_ref", "auto", "3", json!(3)),
        ("mode", "a", "7", json!(7)),
        ("name", "x", "12", json!("12")),
    ]
}

#[test]
fn a_value_typed_in_an_xml_view_is_saved_as_the_view_read_it() {
    let bundle = Arc::new(build_schema(UNIONS).unwrap());
    let schema = Schema::compile(UNIONS).unwrap();
    let mut failures = Vec::new();
    for (field, before, typed, saved_as) in typed_over_strings() {
        let document = json!({"server": {field: before}});
        for host_language in [
            Language::JSON,
            Language::YAML,
            Language::TOML,
            Language::HCL,
        ] {
            let host = host_language.serialize(&document).unwrap();
            let bridge = Bridge::new(host_language, Language::XML, Some(bundle.clone())).unwrap();
            let shown = bridge.to_edit(&host).unwrap().text;
            let edited = shown.replacen(&format!(">{before}<"), &format!(">{typed}<"), 1);
            let in_view = messages(&schema, Language::XML, &edited);
            let saved = bridge
                .to_host(&edited, &host)
                .map(|saved| (host_language.parse(&saved), saved));
            match saved {
                Ok((Ok(read), text)) if read == json!({"server": {field: saved_as}}) => {
                    let found = messages(&schema, host_language, &text);
                    if found != in_view {
                        failures.push(format!(
                            "{field} {typed} in {host_language}: view {in_view:?}, saved {found:?}"
                        ));
                    }
                }
                other => failures.push(format!(
                    "{field} {typed} saved to {host_language}: {other:?}"
                )),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn an_item_typed_in_an_xml_view_is_saved_as_the_view_read_it() {
    let bundle = Arc::new(build_schema(UNIONS).unwrap());
    let mut failures = Vec::new();
    let document = json!({"server": {"items": ["auto", "auto"]}});
    for host_language in [Language::JSON, Language::YAML, Language::TOML] {
        let host = host_language.serialize(&document).unwrap();
        let bridge = Bridge::new(host_language, Language::XML, Some(bundle.clone())).unwrap();
        let shown = bridge.to_edit(&host).unwrap().text;
        let edited = shown.replacen(">auto<", ">5<", 1);
        let saved = bridge
            .to_host(&edited, &host)
            .map(|saved| host_language.parse(&saved));
        match saved {
            Ok(Ok(read)) if read == json!({"server": {"items": [5, "auto"]}}) => {}
            other => failures.push(format!("saved to {host_language}: {other:?}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Saving the view's reading looks up the schema of each value retyped,
/// not of the whole document: a larger document costs the save no more
/// schema walks for it than one without the retyped value.
#[test]
fn saving_a_retyped_value_walks_its_schema_only() {
    use crate::editor::work_count::{Work, counted};
    let bundle = Arc::new(build_schema(UNIONS).unwrap());
    let walks = |others: usize, typed: &str| {
        let mut server = serde_json::Map::new();
        server.insert("workers".into(), json!("auto"));
        for index in 0..others {
            server.insert(format!("other{index}"), json!(index));
        }
        let host = Language::JSON
            .serialize(&json!({ "server": server }))
            .unwrap();
        let bridge = Bridge::new(Language::JSON, Language::XML, Some(bundle.clone())).unwrap();
        let shown = bridge.to_edit(&host).unwrap().text;
        let edited = shown.replacen(">auto<", &format!(">{typed}<"), 1);
        let (saved, walks) = counted(Work::BranchWalk, || bridge.to_host(&edited, &host));
        assert!(saved.is_ok(), "{saved:?}");
        walks
    };
    // `9x` is no number: the host's string type keeps it, nothing retyped.
    let plain = walks(200, "9x") - walks(50, "9x");
    let retyped = walks(200, "8") - walks(50, "8");
    assert_eq!(retyped, plain, "retyped {retyped}, plain {plain}");
}
