//! A save writes exactly what the edit means, in the host's own forms, or
//! waits with a reason: a TOML datetime stays a datetime, an XML view that
//! cannot show nested lists does not write its approximation over them, an
//! HCL block stays a block, an indented heredoc keeps its indentation, a
//! list over lines keeps its comments, and XML keys in two namespaces are
//! not merged.

use super::Language;
use super::save_matrix::{ALL, from_every_view, report, save_value, slot};
use serde_json::{Value, json};

/// A changed TOML date or time is written bare, as TOML reads it back
/// (`.50` is `.5`), from every view.
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
