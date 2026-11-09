//! A save writes exactly what the edit means, in the host's own forms, or
//! waits with a reason: a TOML datetime stays a datetime, an XML view that
//! cannot show nested lists does not write its approximation over them, an
//! HCL block stays a block, an indented heredoc keeps its indentation, a
//! list over lines keeps its comments, and XML keys in two namespaces are
//! not merged.

use super::Language;
use super::save_matrix::{ALL, from_every_view, report, save_value, slot};
use serde_json::{Value, json};

/// A changed TOML date or time is written bare, from every view, as typed
/// (`.999990`); typed over its own value in another spelling (`.50` over
/// `.5`) it is unchanged, so the host keeps its text.
#[test]
fn toml_datetimes_stay_datetimes_through_any_view() {
    let host = "# c\ndt = 1979-05-27T07:32:00Z  # dtc\nf = 1979-05-27T00:32:00.5Z\nodt = 1979-05-27T00:32:00.999999-07:00\nld = 1979-05-27\nlt = 07:32:00\nn = 1\n";
    let mut failures = Vec::new();
    for (key, typed, written) in [
        ("f", "1979-05-27T00:32:00.50Z", "1979-05-27T00:32:00.5Z"),
        ("f", "1979-05-27T00:32:00.6Z", "1979-05-27T00:32:00.6Z"),
        ("dt", "1979-05-27T07:32:01Z", "1979-05-27T07:32:01Z"),
        (
            "odt",
            "1979-05-27T00:32:00.999998-07:00",
            "1979-05-27T00:32:00.999998-07:00",
        ),
        // As typed, where TOML reads the spelling as the same time.
        (
            "odt",
            "1979-05-27T00:32:00.999990-07:00",
            "1979-05-27T00:32:00.999990-07:00",
        ),
        ("ld", "1979-05-28", "1979-05-28"),
        ("lt", "07:33:00", "07:33:00"),
    ] {
        let set = |value: &mut Value| *slot(value, &format!("/{key}")) = typed.into();
        let line = host
            .lines()
            .find(|line| line.starts_with(&format!("{key} = ")))
            .expect("a line");
        let old = line
            .split(" = ")
            .nth(1)
            .expect("a value")
            .split("  ")
            .next()
            .expect("a value");
        let expected = host.replace(line, &line.replace(old, written));
        // The YAML and XML views read a datetime as text too.
        for edit in ALL.into_iter().filter(|edit| *edit != Language::TOML) {
            match save_value(Language::TOML, edit, host, &set) {
                Ok(saved) if saved == expected => {}
                Ok(saved) => failures.push(format!("{key} = {typed} via {edit}:\n{saved}")),
                Err(error) => failures.push(format!("{key} = {typed} via {edit}: {error}")),
            }
        }
    }
    report(failures);
}

/// A datetime made text that is no datetime (typed half-way) waits, so the
/// host keeps its datetime and the finished value is saved as one.
#[test]
fn a_toml_datetime_typed_half_way_waits_instead_of_becoming_a_string() {
    let host = "# c\ndt = 1979-05-27T07:32:00Z  # dtc\nn = 1\n";
    let mut failures = Vec::new();
    for edit in ALL.into_iter().filter(|edit| *edit != Language::TOML) {
        let half = save_value(Language::TOML, edit, host, &|value| {
            *slot(value, "/dt") = "1979-05-27T07:32:0Z".into()
        });
        if !half
            .as_ref()
            .is_err_and(|error| error.contains("not a TOML date or time") && error.contains("/dt"))
        {
            failures.push(format!("via {edit}: {half:?}"));
        }
    }
    report(failures);
}

/// Hosts with nested and empty lists, which the XML view shows flat.
const NESTED_LISTS: &[&str] = &[
    "# top\nnest:\n  - []\n  - [a]\nn: 1  # nc\n",
    "# top\nnest:\n  - [a]\n  - []\nn: 1  # nc\n",
    "# top\nnest:\n  - []\n  - [a, b]\nn: 1  # nc\n",
    "# top\nl: [[], [[]], [[a]]]\nn: 1  # nc\n",
    "# top\nl:\n  - k: []\n    j: a\n  - k: [b]\n    j: c\nn: 1  # nc\n",
    "# top\nnest:\n  - - p\n    - a\n  - - r\nn: 1  # nc\n",
];

/// The first string `a` in `value`, made `Z`.
fn a_to_z(value: &mut Value) -> bool {
    match value {
        Value::String(text) if text == "a" => {
            *text = "Z".to_string();
            true
        }
        Value::Array(items) => items.iter_mut().any(a_to_z),
        Value::Object(map) => map.values_mut().any(a_to_z),
        _ => false,
    }
}

/// An item edited in the XML view of nested or empty lists is saved into
/// its place in the host's lists, or the save waits with a reason: never
/// the view's flattened lists written over the host's.
#[test]
fn an_xml_view_of_nested_lists_saves_the_edit_in_place_or_waits() {
    let mut failures = Vec::new();
    for host in NESTED_LISTS {
        let mut expected = Language::YAML.parse(host).expect("parses");
        assert!(a_to_z(&mut expected), "{host}");
        match save_value(Language::YAML, Language::XML, host, &|value| {
            assert!(a_to_z(value), "the view shows a: {value}");
        }) {
            Ok(saved) if Language::YAML.parse(&saved).ok().as_ref() == Some(&expected) => {}
            Ok(saved) => failures.push(format!("{host}: saved other data\n{saved}")),
            Err(error) if error.contains("cannot show the value at") => {}
            Err(error) => failures.push(format!("{host}: {error}")),
        }
        // An edit elsewhere saves, the lists as they were.
        let saved = save_value(Language::YAML, Language::XML, host, &|value| {
            *slot(value, "/n") = 2.into();
        });
        let expected = host.replace("n: 1", "n: 2");
        if saved.as_ref() != Ok(&expected) {
            failures.push(format!("{host}: n edited: {saved:?}"));
        }
    }
    report(failures);
}

/// A key of an HCL block renamed to what is no identifier is refused: the
/// block would have to become an attribute (`db = { "host name" = … }`).
#[test]
fn an_hcl_block_key_renamed_to_a_non_identifier_is_refused_not_made_an_attribute() {
    let host = "# top\ndb {\n  host = \"x\"  # hc\n}\nn = 1\n";
    let mut failures = Vec::new();
    for to in ["host name", "1host", "a.b"] {
        for edit in ALL.into_iter().filter(|edit| *edit != Language::HCL) {
            let saved = save_value(Language::HCL, edit, host, &|value| {
                let db = slot(value, "/db").as_object_mut().expect("a map");
                let item = db.remove("host").expect("host");
                db.insert(to.to_string(), item);
            });
            match saved {
                Err(error)
                    if error.contains("not an HCL identifier") && error.contains("block db") => {}
                // A view that cannot hold the key refuses it itself.
                Err(error) if edit == Language::XML => drop(error),
                other => failures.push(format!("{to:?} via {edit}: {other:?}")),
            }
        }
    }
    report(failures);
}

/// An indented heredoc (`<<-EOT`) keeps the indentation of its lines.
#[test]
fn an_indented_heredoc_keeps_its_indentation_through_any_view() {
    let mut failures = Vec::new();
    let host = "# c\nml = <<-EOT\n  l1\n  l2\n  EOT\nn = 1 # nc\n";
    from_every_view(
        &mut failures,
        "<<-",
        Language::HCL,
        host,
        &|value| *slot(value, "/ml") = "l1\nl2Q\n".into(),
        &host.replace("  l2\n", "  l2Q\n"),
    );
    let host = "# c\nb {\n  ml = <<-EOT\n    l1\n      deeper\n    l2\n    EOT\n}\nn = 1 # nc\n";
    from_every_view(
        &mut failures,
        "<<- nested",
        Language::HCL,
        host,
        &|value| *slot(value, "/b/ml") = "l1\n  deeper\nl2Q\n".into(),
        &host.replace("    l2\n", "    l2Q\n"),
    );
    report(failures);
}

/// An item added to or removed from a list written one item per line, with
/// comments, changes just that line.
#[test]
fn a_list_over_lines_gains_and_loses_items_in_place() {
    let toml = "# top\nn = 1  # nn\n\n[server]  # sc\nname = \"edge\"  # nc\nports = [  # pc\n  80,  # p80\n  443,\n  8443,\n]\ntls = true\n";
    let hcl = "# top\nn = 1  # nn\n\nserver {  # sc\n  name = \"edge\"  # nc\n  ports = [  # pc\n    80,  # p80\n    443,\n    8443,\n  ]\n  tls = true\n}\n";
    let mut failures = Vec::new();
    for (host, text, item) in [(Language::TOML, toml, "  "), (Language::HCL, hcl, "    ")] {
        from_every_view(
            &mut failures,
            "remove 80",
            host,
            text,
            &|value| {
                slot(value, "/server/ports")
                    .as_array_mut()
                    .expect("a list")
                    .remove(0);
            },
            &text.replace(&format!("{item}80,  # p80\n"), ""),
        );
        from_every_view(
            &mut failures,
            "add 8080",
            host,
            text,
            &|value| {
                slot(value, "/server/ports")
                    .as_array_mut()
                    .expect("a list")
                    .push(8080.into())
            },
            &text.replace(
                &format!("{item}8443,\n"),
                &format!("{item}8443,\n{item}8080,\n"),
            ),
        );
        // Two items made one (a selection over both retyped as one of
        // them), and none, then one again: each kept line as it was, a new
        // one at the items' indent with their trailing comma (round 19:
        // `443` at the start of the line, its comma gone).
        let two = text.replace(&format!("{item}8443,\n"), "");
        from_every_view(
            &mut failures,
            "two items, keep 443",
            host,
            &two,
            &|value| {
                *slot(value, "/server/ports") = serde_json::json!([443]);
            },
            &two.replace(&format!("{item}80,  # p80\n"), ""),
        );
        from_every_view(
            &mut failures,
            "two items, retyped 8080",
            host,
            &two,
            &|value| {
                *slot(value, "/server/ports") = serde_json::json!([8080]);
            },
            &two.replace(
                &format!("{item}80,  # p80\n{item}443,\n"),
                &format!("{item}8080,\n"),
            ),
        );
        // XML has no empty list to show.
        let emptied = two.replace(&format!("{item}80,  # p80\n{item}443,\n"), "");
        let one = two.replace(&format!("{item}80,  # p80\n"), "");
        for edit in ALL
            .into_iter()
            .filter(|edit| ![host, Language::XML].contains(edit))
        {
            for (label, from, items, expected) in [
                ("two items, emptied", &two, json!([]), &emptied),
                ("emptied, then one", &emptied, json!([443]), &one),
            ] {
                let saved = save_value(host, edit, from, &|value| {
                    *slot(value, "/server/ports") = items.clone();
                });
                if saved.as_ref() != Ok(expected) {
                    failures.push(format!(
                        "{label}: {host} host via {edit}:\n--- expected\n{expected}--- saved\n{}",
                        saved.unwrap_or_else(|error| error)
                    ));
                }
            }
        }
    }
    // JSON takes no comma after the last item.
    let json = "{\n    \"ports\": [\n    ],\n    \"n\": 1\n}\n";
    for edit in ALL
        .into_iter()
        .filter(|edit| ![Language::JSON, Language::XML].contains(edit))
    {
        let saved = save_value(Language::JSON, edit, json, &|value| {
            value["ports"] = json!([443]);
        });
        let expected = json.replace("[\n", "[\n        443\n");
        if saved.as_ref() != Ok(&expected) {
            failures.push(format!("JSON emptied, then one, via {edit}: {saved:?}"));
        }
    }
    report(failures);
}

/// Elements of one parent with the same local name in two namespaces are
/// not merged into one key: the document is refused for editing as
/// another language, saying why.
#[test]
fn xml_keys_in_two_namespaces_refuse_to_be_edited_as_another_language() {
    let mut failures = Vec::new();
    for (host, clash) in [
        (
            "<?xml version=\"1.0\"?>\n<c:conf xmlns:c=\"urn:c\" xmlns=\"urn:d\" v=\"1\">\n  <c:item id=\"a\">one</c:item>\n  <item id=\"b\"/>\n  <n>1</n>\n</c:conf>\n",
            "<c:item> and <item>",
        ),
        (
            "<r xmlns:a=\"urn:a\" xmlns:b=\"urn:b\">\n  <a:x>1</a:x>\n  <b:x>2</b:x>\n</r>\n",
            "<a:x> and <b:x>",
        ),
        (
            "<r xmlns:a=\"urn:a\">\n  <x a:k=\"1\" k=\"2\"/>\n</r>\n",
            "a:k and k",
        ),
    ] {
        for edit in ALL.into_iter().filter(|edit| *edit != Language::XML) {
            match Language::XML.convert(host, edit) {
                Err(error)
                    if error.to_string().contains(clash)
                        && error.to_string().contains("namespaces") => {}
                other => failures.push(format!("{clash} as {edit}: {other:?}")),
            }
        }
    }
    // One prefixed name alone converts.
    let single = "<c:conf xmlns:c=\"urn:c\">\n  <c:n>1</c:n>\n</c:conf>\n";
    if let Err(error) = Language::XML.convert(single, Language::YAML) {
        failures.push(format!("a lone prefixed name: {error}"));
    }
    report(failures);
}

/// A character XML 1.0 cannot hold (a control character, U+FFFE) is
/// refused by the reader, raw or as a character reference's text, and by
/// the writer: never written into a file XML readers reject.
#[test]
fn xml_refuses_characters_xml_cannot_hold_in_reading_and_writing() {
    let mut failures = Vec::new();
    for text in ["<r>a\u{1}b</r>", "<r k=\"\u{fffe}\"/>", "<r>\u{0}</r>"] {
        if Language::XML.parse(text).is_ok() {
            failures.push(format!("{text:?} read"));
        }
    }
    // A reference to such a character is not one (left as text).
    if Language::XML.parse("<r>&#1;</r>").ok() != Some(json!({"r": "&#1;"})) {
        failures.push(format!("&#1;: {:?}", Language::XML.parse("<r>&#1;</r>")));
    }
    for value in [
        json!({"r": "a\u{1}b"}),
        json!({"r": {"k": "\u{ffff}"}}),
        json!({"r\u{1}": 1}),
    ] {
        match Language::XML.serialize(&value) {
            Err(error) if error.to_string().contains("XML 1.0 cannot hold") => {}
            other => failures.push(format!("{value}: {other:?}")),
        }
    }
    report(failures);
}

/// A string written whole is written as a spliced one is: plain where the
/// plain-scalar rule allows it (`-Xmx512m`), quoted where it does not.
#[test]
fn a_yaml_string_written_whole_is_written_as_a_spliced_one() {
    let mut failures = Vec::new();
    for value in [
        "-Xmx512m",
        "--flag",
        "-x",
        "a:b",
        "x,y",
        "http://h/p?q=1",
        "?x",
        ":x",
        "yes",
        "a b",
    ] {
        let whole = Language::YAML
            .serialize(&json!({ "k": value }))
            .expect("writes");
        let spliced = save_value(
            Language::YAML,
            Language::JSON,
            "k: old # c\n",
            &|document| {
                *slot(document, "/k") = value.into();
            },
        );
        let spliced = spliced.map(|text| text.replace(" # c", ""));
        if spliced.as_deref() != Ok(whole.as_str()) {
            failures.push(format!("{value:?}: whole {whole:?}, spliced {spliced:?}"));
        }
    }
    report(failures);
}

/// The Edit-as notice that the host's comments are not shown counts XML
/// processing instructions, which no other language shows either.
#[test]
fn xml_processing_instructions_count_as_comments_the_view_does_not_show() {
    let xml = Language::XML.behavior();
    assert!(xml.has_comments("<r><?pi data?><a>1</a></r>\n"));
    assert!(xml.has_comments("<r><!-- c --><a>1</a></r>\n"));
    assert!(!xml.has_comments("<?xml version=\"1.0\"?>\n<r><a>1</a></r>\n"));
}

/// Number forms each host writes its own way (`1.0`, `-0.0`, `1e3`, `0x1F`,
/// `1_000`), which a view shows as plain numbers it may write otherwise.
fn number_hosts() -> Vec<(Language, String)> {
    let xml_values = [
        "1.0",
        "2.0",
        "10.0",
        "0.0",
        "-0.0",
        "-0",
        "1.10",
        "  1 ",
        "1e3",
        "1.5E3",
        "0.1",
        "3.0e0",
        "+1",
        "9007199254740993",
        "12345678901234567890",
    ];
    let elements: String = xml_values
        .iter()
        .enumerate()
        .map(|(index, value)| format!("  <k{index}>{value}</k{index}>\n"))
        .collect();
    let attributes: String = xml_values
        .iter()
        .enumerate()
        .map(|(index, value)| format!(" a{index}=\"{value}\""))
        .collect();
    let lines = |values: &[&str], separator: &str| -> String {
        values
            .iter()
            .enumerate()
            .map(|(index, value)| format!("k{index}{separator}{value}\n"))
            .collect()
    };
    let toml = [
        "1.0",
        "-0.0",
        "1e3",
        "1.5E3",
        "100.0",
        "0.10",
        "1_000",
        "0x1F",
        "0o17",
        "0b11",
        "+1",
        "inf",
        "-inf",
        "nan",
        "9007199254740993",
        "1.0e+3",
        "-0",
    ];
    let yaml = [
        "1.0",
        "-0.0",
        "1e3",
        "1.5E3",
        "100.0",
        "0.10",
        "0x1F",
        "0o17",
        "+1",
        ".inf",
        "-.inf",
        "9007199254740993",
        "1.0e+3",
        "12345678901234567890",
        "-0",
    ];
    let hcl = [
        "1.0",
        "-0.0",
        "1e3",
        "1.5E3",
        "100.0",
        "0.10",
        "9007199254740993",
        "1.0e+3",
    ];
    let json = [
        "1.0",
        "-0.0",
        "1e3",
        "1.5E3",
        "100.0",
        "0.10",
        "9007199254740993",
        "1.0e+3",
        "12345678901234567890",
        "-0",
    ];
    let json_entries: Vec<String> = json
        .iter()
        .enumerate()
        .map(|(index, value)| format!("  \"k{index}\": {value}"))
        .collect();
    vec![
        (
            Language::XML,
            format!("<!-- top -->\n<r>\n  <name>edge</name>\n{elements}</r>\n"),
        ),
        (
            Language::XML,
            format!("<!-- top -->\n<r{attributes}>\n  <name>edge</name>\n</r>\n"),
        ),
        (
            Language::TOML,
            format!("# top\nname = \"edge\"\n{}", lines(&toml, " = ")),
        ),
        (
            Language::YAML,
            format!("# top\nname: edge\n{}", lines(&yaml, ": ")),
        ),
        (
            Language::HCL,
            format!("# top\nname = \"edge\"\n{}", lines(&hcl, " = ")),
        ),
        (
            Language::JSON,
            format!(
                "{{\n  \"name\": \"edge\",\n{}\n}}\n",
                json_entries.join(",\n")
            ),
        ),
    ]
}

/// An edit elsewhere leaves every value it did not touch as the host wrote
/// it: a number the view shows (and would write) otherwise is only shown
/// that way (round 19: XML `1.0` saved as `1`).
#[test]
fn untouched_numbers_stay_as_written_through_any_view() {
    let mut failures = Vec::new();
    for (host, text) in number_hosts() {
        let expected = text.replace("edge", "edgeQ");
        let name = if host == Language::XML {
            "/r/name"
        } else {
            "/name"
        };
        for edit in ALL.into_iter().filter(|edit| *edit != host) {
            // A view that cannot hold a value (TOML integers end at 2^63)
            // does not open.
            let bridge = super::bridge::Bridge::new(host, edit, None).expect("two languages");
            if bridge.to_edit(&text).is_err() {
                continue;
            }
            let saved = save_value(host, edit, &text, &|value| {
                *slot(value, name) = "edgeQ".into();
            });
            match saved {
                Ok(saved) if saved == expected => {}
                Ok(saved) => failures.push(format!(
                    "{host} via {edit}:\n--- expected\n{expected}--- saved\n{saved}"
                )),
                Err(error) => failures.push(format!("{host} via {edit}: {error}")),
            }
        }
    }
    // XML writes a float as JSON does, so it reads back as that float.
    let floats = r#"{"a": 1.0, "b": -0.0, "c": 10.0, "d": {"e": 2.0, "f": [0.0, 1.5]}}"#;
    match Language::JSON.convert(floats, Language::XML) {
        Ok(converted) if converted.losses.is_empty() => {}
        other => failures.push(format!("floats to XML: {other:?}")),
    }
    report(failures);
}

/// A view that reads a whole float as an integer cannot tell it from one:
/// a number typed over it keeps the host's float type (round 20: the HCL
/// view saved an integer, before HCL read `1.0` as a float), and only that
/// line changes.
#[test]
fn a_whole_float_edited_in_a_view_that_shows_it_as_an_integer_stays_a_float() {
    fn whole_floats(value: &Value, pointer: String, out: &mut Vec<String>) {
        match value {
            Value::Number(number) if number.is_f64() => {
                if number.as_f64().is_some_and(|float| float.fract() == 0.0) {
                    out.push(pointer);
                }
            }
            Value::Object(map) => {
                for (key, child) in map {
                    whole_floats(child, format!("{pointer}/{key}"), out);
                }
            }
            _ => {}
        }
    }
    let mut failures = Vec::new();
    let mut tried = 0;
    for (host, text) in number_hosts() {
        let old = host.parse(&text).expect("parses");
        let mut pointers = Vec::new();
        whole_floats(&old, String::new(), &mut pointers);
        for edit in ALL.into_iter().filter(|edit| *edit != host) {
            let bridge = super::bridge::Bridge::new(host, edit, None).expect("two languages");
            let Ok(shown) = bridge.to_edit(&text) else {
                continue;
            };
            let Ok(mut seen) = edit.parse(&shown.text) else {
                continue;
            };
            for pointer in &pointers {
                // Only where the view shows an integer.
                if !super::save_matrix::find_slot(&mut seen, pointer)
                    .is_some_and(|shown| shown.is_u64() || shown.is_i64())
                {
                    continue;
                }
                tried += 1;
                let case = format!("{host} host, {pointer} = 7 via {edit}");
                match save_value(host, edit, &text, &|value| {
                    *slot(value, pointer) = json!(7);
                }) {
                    Ok(saved) => {
                        let read = host.parse(&saved).ok();
                        let held = read.as_ref().and_then(|read| read.pointer(pointer));
                        if !held.is_some_and(|held| held.is_f64() && held.as_f64() == Some(7.0)) {
                            failures.push(format!("{case}: reads {held:?}\n{saved}"));
                        }
                        let changed = text
                            .lines()
                            .zip(saved.lines())
                            .filter(|(before, after)| before != after)
                            .count();
                        if changed != 1 || text.lines().count() != saved.lines().count() {
                            failures.push(format!("{case}: other lines changed\n{saved}"));
                        }
                    }
                    Err(error) => failures.push(format!("{case}: {error}")),
                }
            }
        }
    }
    assert!(tried > 0, "only {tried} cases");
    report(failures);
}

/// Items moved in another view take their comments along at any depth: a
/// list inside a list item, maps as items, own-line comments above each
/// item (the first one's too), in YAML, TOML and HCL lists over lines and
/// XML repeated elements (round 20: they stayed where they were).
#[test]
fn moved_items_take_their_comments_at_any_depth_from_every_view() {
    let cases: &[(Language, &str, &str, &str)] = &[
        (
            Language::YAML,
            "/l/0/sub",
            "l:\n  - name: a\n    sub:\n      - 1  # one\n      - 2  # two\nn: 1\n",
            "l:\n  - name: a\n    sub:\n      - 2  # two\n      - 1  # one\nn: 1\n",
        ),
        (
            Language::YAML,
            "/l/0/sub",
            "l:\n  - name: a\n    sub:\n      - x  # one\n      - y  # two\n  - name: b\nn: 1\n",
            "l:\n  - name: a\n    sub:\n      - y  # two\n      - x  # one\n  - name: b\nn: 1\n",
        ),
        (
            Language::YAML,
            "/l/0/sub",
            "l:\n  - name: a\n    sub:\n      - k: 1  # one\n      - k: 2  # two\nn: 1\n",
            "l:\n  - name: a\n    sub:\n      - k: 2  # two\n      - k: 1  # one\nn: 1\n",
        ),
        (
            Language::YAML,
            "/l",
            "l:\n  # about a\n  - a\n  # about b\n  - b\nn: 1\n",
            "l:\n  # about b\n  - b\n  # about a\n  - a\nn: 1\n",
        ),
        (
            Language::YAML,
            "/l",
            "# top\nl:\n  # about a\n  - a  # ta\n  # about b\n  # more b\n  - b\n  - c  # tc\nn: 1\n",
            "# top\nl:\n  - c  # tc\n  # about b\n  # more b\n  - b\n  # about a\n  - a  # ta\nn: 1\n",
        ),
        (
            Language::YAML,
            "/m/l",
            "m:\n  l:\n  # about a\n  - a\n  # about b\n  - b\nn: 1\n",
            "m:\n  l:\n  # about b\n  - b\n  # about a\n  - a\nn: 1\n",
        ),
        (
            Language::YAML,
            "/l/1/sub",
            "l:\n  - x\n  - sub:\n      # about 1\n      - 1\n      # about 2\n      - 2\nn: 1\n",
            "l:\n  - x\n  - sub:\n      # about 2\n      - 2\n      # about 1\n      - 1\nn: 1\n",
        ),
        (
            Language::TOML,
            "/l",
            "# top\nl = [\n  # about a\n  \"a\",\n  # about b\n  \"b\",\n]\nn = 1\n",
            "# top\nl = [\n  # about b\n  \"b\",\n  # about a\n  \"a\",\n]\nn = 1\n",
        ),
        (
            Language::HCL,
            "/l",
            "# top\nl = [\n  # about a\n  \"a\",\n  # about b\n  \"b\",\n]\nn = 1\n",
            "# top\nl = [\n  # about b\n  \"b\",\n  # about a\n  \"a\",\n]\nn = 1\n",
        ),
        (
            Language::TOML,
            "/l",
            "# top\nl = [\n  1,  # one\n  2,  # two\n]\nn = 1\n",
            "# top\nl = [\n  2,  # two\n  1,  # one\n]\nn = 1\n",
        ),
        (
            Language::XML,
            "/r/l",
            "<r>\n  <l>1</l>  <!-- one -->\n  <l>2</l>  <!-- two -->\n  <n>1</n>\n</r>\n",
            "<r>\n  <l>2</l>  <!-- two -->\n  <l>1</l>  <!-- one -->\n  <n>1</n>\n</r>\n",
        ),
        (
            Language::XML,
            "/r/l",
            "<r>\n  <!-- about a -->\n  <l>a</l>\n  <!-- about b -->\n  <l>b</l>\n</r>\n",
            "<r>\n  <!-- about b -->\n  <l>b</l>\n  <!-- about a -->\n  <l>a</l>\n</r>\n",
        ),
    ];
    let mut failures = Vec::new();
    for (host, pointer, text, expected) in cases {
        from_every_view(
            &mut failures,
            &format!("{pointer} reversed"),
            *host,
            text,
            &|value| {
                if let Some(Value::Array(items)) = super::save_matrix::find_slot(value, pointer) {
                    items.reverse();
                }
            },
            expected,
        );
    }
    report(failures);
}

/// A plain YAML string cleared and then typed again in another view is
/// written as typing over it writes it: plain where every reader takes it as text,
/// quoted where one would not (a date, `yes`), not quoted because the empty
/// string in between was (round 20: the XML view gave `name: "abc"`).
#[test]
fn a_yaml_string_cleared_then_typed_is_written_as_typed_over() {
    let host = "# top\nname: edge  # nc\nq: \"edge\"\nn: 1\n";
    let mut failures = Vec::new();
    for edit in ALL.into_iter().filter(|edit| *edit != Language::YAML) {
        // (A string the host quoted is typed in plain after it was cleared:
        // `""` says nothing of how its text was written.)
        for key in ["name"] {
            for typed in ["abc", "a b", "2024-01-01", "yes", "x: y", "12"] {
                let pointer = format!("/{key}");
                let set = |to: &str| {
                    let to = to.to_string();
                    let pointer = pointer.clone();
                    move |value: &mut Value| *slot(value, &pointer) = Value::from(to.as_str())
                };
                let over = save_value(Language::YAML, edit, host, &set(typed));
                let cleared = save_value(Language::YAML, edit, host, &set(""));
                let again = cleared
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|cleared| save_value(Language::YAML, edit, cleared, &set(typed)));
                if over != again {
                    failures.push(format!(
                        "{key} = {typed:?} via {edit}:\n--- typed over\n{over:?}\n--- cleared then typed\n{again:?}"
                    ));
                }
            }
        }
    }
    report(failures);
}

/// A YAML list or map emptied from another view (`ports: []`), and filled
/// again, keeps the spacing before its key line's comment as written
/// (round 20: `ports:    # pc` became `ports: [] # pc`, then `ports: # pc`),
/// with any gap, nested or at the top.
#[test]
fn a_yaml_container_emptied_and_refilled_keeps_the_gap_before_its_comment() {
    let mut failures = Vec::new();
    for gap in [" ", "    ", "  \t"] {
        for (pointer, block, empty, filled) in [
            (
                "/ports",
                format!("# top\nports:{gap}# pc\n  - 80\n  - 443\nn: 1\n"),
                format!("# top\nports: []{gap}# pc\nn: 1\n"),
                format!("# top\nports:{gap}# pc\n  - 8\nn: 1\n"),
            ),
            (
                "/m",
                format!("# top\nm:{gap}# mc\n  a: 1\n  b: 2\nn: 1\n"),
                format!("# top\nm: {{}}{gap}# mc\nn: 1\n"),
                format!("# top\nm:{gap}# mc\n  k: 8\nn: 1\n"),
            ),
            (
                "/s/ports",
                format!("# top\ns:\n  ports:{gap}# pc\n    - 80\n  x: 1\nn: 1\n"),
                format!("# top\ns:\n  ports: []{gap}# pc\n  x: 1\nn: 1\n"),
                format!("# top\ns:\n  ports:{gap}# pc\n    - 8\n  x: 1\nn: 1\n"),
            ),
        ] {
            let emptied = if pointer.ends_with("ports") {
                json!([])
            } else {
                json!({})
            };
            let refill = if pointer.ends_with("ports") {
                json!([8])
            } else {
                json!({"k": 8})
            };
            for edit in ALL.into_iter().filter(|edit| *edit != Language::YAML) {
                for (label, host, value, expected) in [
                    ("emptied", &block, &emptied, &empty),
                    ("refilled", &empty, &refill, &filled),
                ] {
                    // XML shows no empty container.
                    let shown = std::cell::Cell::new(edit != Language::XML || label == "refilled");
                    let saved = save_value(Language::YAML, edit, host, &|document| {
                        match super::save_matrix::find_slot(document, pointer) {
                            Some(slot) => *slot = value.clone(),
                            None => shown.set(false),
                        }
                    });
                    match saved {
                        _ if !shown.get() => {}
                        Ok(saved) if saved == *expected => {}
                        Ok(saved) => failures.push(format!(
                            "{pointer} {label}, gap {gap:?}, via {edit}:\n--- expected\n{expected}--- saved\n{saved}"
                        )),
                        Err(error) => failures.push(format!(
                            "{pointer} {label}, gap {gap:?}, via {edit}: {error}"
                        )),
                    }
                }
            }
        }
    }
    report(failures);
}

/// The XML view writes every scalar as text and guesses its type on
/// reading, so a value edited there keeps the host's type: a string stays a
/// string whatever is typed (`"1"` made `12` is `"12"`), an emptied element
/// is the empty string where the host has a string, and the save waits
/// where the host has a number or a boolean (round 19: `{}` was saved).
/// The host's other lines, and the spacing before its comment, stay.
#[test]
fn a_scalar_edited_in_the_xml_view_keeps_the_hosts_type() {
    let hosts = [
        (
            Language::YAML,
            "# top\nname: edge  # nc\nver: \"1\"\nflag: \"yes\"\nport: 80  # pc\nok: true\nn: 1\n",
        ),
        (
            Language::TOML,
            "# top\nname = \"edge\"  # nc\nver = \"1\"\nflag = \"true\"\nport = 80  # pc\nok = true\nn = 1\n",
        ),
        (
            Language::HCL,
            "# top\nname = \"edge\"  # nc\nver = \"1\"\nflag = \"true\"\nport = 80  # pc\nok = true\nn = 1\n",
        ),
        (
            Language::JSON,
            "{\n  \"name\": \"edge\",\n  \"ver\": \"1\",\n  \"flag\": \"true\",\n  \"port\": 80,\n  \"ok\": true,\n  \"n\": 1\n}\n",
        ),
    ];
    // What is typed into the element, and what the host then holds (`None`:
    // the save waits).
    let cases: &[(&str, Value, Option<Value>)] = &[
        ("name", json!(""), Some(json!(""))),
        ("name", json!(9), Some(json!("9"))),
        ("name", json!(true), Some(json!("true"))),
        ("name", json!("x y"), Some(json!("x y"))),
        ("ver", json!(12), Some(json!("12"))),
        ("ver", json!(""), Some(json!(""))),
        ("flag", json!(false), Some(json!("false"))),
        ("port", json!(""), None),
        ("port", json!(81), Some(json!(81))),
        ("port", json!(1.5), Some(json!(1.5))),
        ("port", json!("abc"), Some(json!("abc"))),
        // A hexadecimal, octal or binary integer is that number (round 20:
        // the text `0x50`).
        ("port", json!("0x50"), Some(json!(80))),
        ("port", json!("0o17"), Some(json!(15))),
        ("port", json!("0b11"), Some(json!(3))),
        ("port", json!("-0x10"), Some(json!(-16))),
        ("ver", json!("0x50"), Some(json!("0x50"))),
        ("ok", json!(""), None),
        ("ok", json!(false), Some(json!(false))),
    ];
    let mut failures = Vec::new();
    for (host, text) in hosts {
        for (key, typed, held) in cases {
            let case = format!("{host} host, {key} = {typed} via XML");
            let saved = save_value(host, Language::XML, text, &|value| {
                *slot(value, &format!("/{key}")) = typed.clone();
            });
            match (saved, held) {
                (Err(error), None) if error.contains(&format!("/{key}")) => {}
                (Err(error), _) => failures.push(format!("{case}: {error}")),
                (Ok(saved), None) => failures.push(format!("{case}: saved\n{saved}")),
                (Ok(saved), Some(held)) => {
                    let mut expected = host.parse(text).expect("parses");
                    *slot(&mut expected, &format!("/{key}")) = held.clone();
                    if host.parse(&saved).ok() != Some(expected) {
                        failures.push(format!("{case}: reads back otherwise\n{saved}"));
                    }
                    // Only the edited line changed, its comment as it was.
                    let other_lines = |text: &str| -> Vec<String> {
                        text.lines()
                            .filter(|line| !line.contains(&format!("{key}\"")))
                            .filter(|line| !line.starts_with(&format!("{key}:")))
                            .filter(|line| !line.starts_with(&format!("{key} =")))
                            .map(str::to_string)
                            .collect()
                    };
                    let edited_line = |text: &str| {
                        text.lines()
                            .find(|line| {
                                line.starts_with(&format!("{key}:"))
                                    || line.starts_with(&format!("{key} ="))
                            })
                            .map(|line| line.contains("  # ") == line.contains('#'))
                    };
                    if other_lines(&saved) != other_lines(text)
                        || edited_line(&saved) != edited_line(text)
                    {
                        failures.push(format!("{case}: other lines changed\n{saved}"));
                    }
                }
            }
        }
        // A value changing type from a typed view keeps the spacing before
        // its comment too.
        for edit in ALL.into_iter().filter(|edit| *edit != host) {
            for (key, typed) in [("name", json!(9)), ("port", json!("x"))] {
                let saved = save_value(host, edit, text, &|value| {
                    *slot(value, &format!("/{key}")) = typed.clone();
                });
                let gap_kept = saved.as_ref().is_ok_and(|saved| {
                    saved.lines().filter(|line| line.contains("  # ")).count()
                        == text.lines().filter(|line| line.contains("  # ")).count()
                });
                if !gap_kept {
                    failures.push(format!(
                        "{host} host, {key} = {typed} via {edit}: {saved:?}"
                    ));
                }
            }
        }
    }
    report(failures);
}

/// A TOML datetime emptied or typed half-way in any view (the XML view
/// reads `2` as a number) waits, and the finished one is written bare.
#[test]
fn a_toml_datetime_retyped_in_any_view_stays_a_datetime() {
    let host = "# c\ndt = 1979-05-27T07:32:00Z  # dtc\nn = 1\n";
    let mut failures = Vec::new();
    for edit in ALL.into_iter().filter(|edit| *edit != Language::TOML) {
        for typed in [json!(""), json!(2), json!(2024), json!(true)] {
            let saved = save_value(Language::TOML, edit, host, &|value| {
                *slot(value, "/dt") = typed.clone();
            });
            if !saved.as_ref().is_err_and(|error| error.contains("/dt")) {
                failures.push(format!("{typed} via {edit}: {saved:?}"));
            }
        }
        let saved = save_value(Language::TOML, edit, host, &|value| {
            *slot(value, "/dt") = "2024-01-02T03:04:05Z".into();
        });
        let expected = host.replace("1979-05-27T07:32:00Z", "2024-01-02T03:04:05Z");
        if saved.as_ref() != Ok(&expected) {
            failures.push(format!("finished via {edit}: {saved:?}"));
        }
    }
    report(failures);
}

/// A key added to a JSON object holding one key goes on its own line at the
/// object's indentation, as in an object holding more (round 19: `"max":
/// 10, "min": 1` on one line), whatever the host indents with.
#[test]
fn a_key_added_to_a_one_key_json_object_gets_its_own_line() {
    let mut failures = Vec::new();
    for unit in ["  ", "    ", "\t"] {
        let host = format!(
            "{{\n{unit}\"name\": \"edge\",\n{unit}\"limits\": {{\n{unit}{unit}\"max\": 10\n{unit}}},\n{unit}\"n\": 1\n}}\n"
        );
        let expected = host.replace(
            "\"max\": 10\n",
            &format!("\"max\": 10,\n{unit}{unit}\"min\": 1\n"),
        );
        from_every_view(
            &mut failures,
            &format!("indent {unit:?}"),
            Language::JSON,
            &host,
            &|value| {
                value["limits"]["min"] = 1.into();
            },
            &expected,
        );
        // At the top level too.
        let host = format!("{{\n{unit}\"max\": 10\n}}\n");
        let expected = format!("{{\n{unit}\"max\": 10,\n{unit}\"min\": 1\n}}\n");
        from_every_view(
            &mut failures,
            &format!("top level, indent {unit:?}"),
            Language::JSON,
            &host,
            &|value| {
                value["min"] = 1.into();
            },
            &expected,
        );
    }
    // An HCL object over lines goes without commas; a YAML flow map needs
    // them.
    for (host, text, expected) in [
        (
            Language::HCL,
            "# c\nm = {\n  max = 10\n}\n",
            "# c\nm = {\n  max = 10\n  min = 1\n}\n",
        ),
        (
            Language::YAML,
            "# c\nm: {\n  max: 10\n}\n",
            "# c\nm: {\n  max: 10,\n  min: 1\n}\n",
        ),
    ] {
        from_every_view(
            &mut failures,
            "one entry over lines",
            host,
            text,
            &|value| {
                value["m"]["min"] = 1.into();
            },
            expected,
        );
    }
    report(failures);
}

/// Items of a YAML block list moved through any view take all their lines
/// along, their comments included: maps, block scalars, indentless and
/// nested lists (round 19: a map item's comments stayed where it was).
#[test]
fn yaml_list_items_moved_through_any_view_take_their_comments() {
    let mut failures = Vec::new();
    for (text, pointer, order, expected) in [
        (
            "# top\nl:\n  - name: a  # ac\n    # inner a\n    v: 1\n  - name: b  # bc\n    # inner b\n    v: 2\nn: 1  # nn\n",
            "/l",
            &[1, 0][..],
            "# top\nl:\n  - name: b  # bc\n    # inner b\n    v: 2\n  - name: a  # ac\n    # inner a\n    v: 1\nn: 1  # nn\n",
        ),
        (
            "l:  # lc\n  # first\n  - k: 1\n    j: x  # jx\n  # second\n  - k: 2\n\n  - k: 3  # three\n    j: z\nn: 1\n",
            "/l",
            &[2, 0, 1][..],
            // The comment line above the first item is that item's too.
            "l:  # lc\n  - k: 3  # three\n    j: z\n  # first\n  - k: 1\n    j: x  # jx\n\n  # second\n  - k: 2\nn: 1\n",
        ),
        (
            "l:\n- name: a # ac\n  v: 1\n- name: b\n  v: 2 # bv\nn: 1\n",
            "/l",
            &[1, 0][..],
            "l:\n- name: b\n  v: 2 # bv\n- name: a # ac\n  v: 1\nn: 1\n",
        ),
        (
            "l:\n  - |\n    x\n    y\n  - z # zc\nn: 1\n",
            "/l",
            &[1, 0][..],
            "l:\n  - z # zc\n  - |\n    x\n    y\nn: 1\n",
        ),
        (
            "m:\n  l:\n    - k: 1 # one\n      t: [a, b]\n    - k: 2 # two\n  n: 1 # nc\n",
            "/m/l",
            &[1, 0][..],
            "m:\n  l:\n    - k: 2 # two\n    - k: 1 # one\n      t: [a, b]\n  n: 1 # nc\n",
        ),
    ] {
        from_every_view(
            &mut failures,
            "moved",
            Language::YAML,
            text,
            &|value| {
                let items = slot(value, pointer).as_array().expect("a list").clone();
                *slot(value, pointer) =
                    Value::Array(order.iter().map(|at| items[*at].clone()).collect());
            },
            expected,
        );
    }
    // An anchor moved after its alias cannot be moved as written: the save
    // waits rather than leave each comment on the item now in its place.
    let anchored = "l:\n  - &a\n    k: 1 # one\n  - *a\n  - k: 2 # two\nn: 1\n";
    for edit in ALL.into_iter().filter(|edit| *edit != Language::YAML) {
        let saved = save_value(Language::YAML, edit, anchored, &|value| {
            slot(value, "/l").as_array_mut().expect("a list").reverse();
        });
        let comments_follow = saved
            .as_ref()
            .is_ok_and(|saved| saved.contains("k: 2 # two") && saved.contains("k: 1 # one"));
        let waits = saved
            .as_ref()
            .is_err_and(|error| error.contains("cannot be moved") && error.contains("/l"));
        if !comments_follow && !waits {
            failures.push(format!("anchored via {edit}: {saved:?}"));
        }
    }
    report(failures);
}
