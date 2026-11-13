//! Edit-as saves of the odd language: each edit made in every built-in
//! language's view of an odd host changes only the lines it touches, and an
//! edit made in the odd view of a built-in host saves as it would from any
//! other view. Every failing case is listed at once.

use serde_json::{Value, json};

use super::super::Language;
use super::super::bridge::Bridge;
use super::super::save_matrix::{ALL, find_slot, report};
use super::ODD;

fn odd() -> Language {
    Language::new(&ODD)
}

const HOST: &str = "\
; odd config
name := web ; the name
note := don't ; kept as written
port := 80
ports := (80 | 443) ; inline
tls := <cert := a.pem | on := yes>
hosts :=
  ; the first
  @ alpha
  ; the second
  @ beta
limits :=
  max := 5 ; cap
  min := 1
";

/// The host saved after `edit`'s view of it is changed by `change`.
fn save(
    host: Language,
    edit: Language,
    text: &str,
    change: &dyn Fn(&mut Value),
) -> Result<String, String> {
    let bridge = Bridge::new(host, edit, None).ok_or("same language")?;
    let shown = bridge
        .to_edit(text)
        .map_err(|error| error.to_string())?
        .text;
    let mut value = edit.parse(&shown).map_err(|error| error.message)?;
    change(&mut value);
    let edited = super::super::codec::serialize(edit.behavior(), &value, Some(&shown))
        .map_err(|error| error.to_string())?;
    bridge
        .to_host(&edited, text)
        .map_err(|error| format!("{error:?}"))
}

fn set(pointer: &'static str, value: Value) -> impl Fn(&mut Value) {
    move |document: &mut Value| {
        if let Some(slot) = find_slot(document, pointer) {
            *slot = value.clone();
        }
    }
}

type Change = Box<dyn Fn(&mut Value)>;

/// Each edit, and the odd host it must save as.
fn edits() -> Vec<(&'static str, Change, String)> {
    let replaced = |from: &str, to: &str| HOST.replacen(from, to, 1);
    vec![
        (
            "a value changed",
            Box::new(set("/port", json!(81))),
            replaced("port := 80\n", "port := 81\n"),
        ),
        (
            "a string beside a quote changed",
            Box::new(set("/name", json!("api"))),
            replaced("name := web ;", "name := api ;"),
        ),
        (
            "a key added",
            Box::new(|document: &mut Value| {
                document["limits"]["debug"] = json!(true);
            }),
            format!("{HOST}  debug := yes\n"),
        ),
        (
            "a value beside a comment changed",
            Box::new(set("/note", json!("isn't"))),
            replaced("note := don't ;", "note := isn't ;"),
        ),
        (
            "a key renamed",
            Box::new(|document: &mut Value| {
                let map = std::mem::take(document.as_object_mut().unwrap());
                *document = Value::Object(
                    map.into_iter()
                        .map(|(key, value)| match key.as_str() {
                            "port" => ("listen".to_string(), value),
                            _ => (key, value),
                        })
                        .collect(),
                );
            }),
            replaced("port := 80\n", "listen := 80\n"),
        ),
        (
            "a key removed",
            Box::new(|document: &mut Value| {
                document.as_object_mut().unwrap().shift_remove("port");
            }),
            replaced("port := 80\n", ""),
        ),
        (
            "a nested key removed",
            Box::new(|document: &mut Value| {
                document["limits"]
                    .as_object_mut()
                    .unwrap()
                    .shift_remove("min");
            }),
            replaced("  min := 1\n", ""),
        ),
        (
            "list items moved",
            Box::new(set("/hosts", json!(["beta", "alpha"]))),
            replaced(
                "  ; the first\n  @ alpha\n  ; the second\n  @ beta\n",
                "  ; the second\n  @ beta\n  ; the first\n  @ alpha\n",
            ),
        ),
        (
            "a list item added",
            Box::new(|document: &mut Value| {
                document["hosts"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("gamma"));
            }),
            replaced("  @ beta\n", "  @ beta\n  @ gamma\n"),
        ),
        (
            "a list item removed",
            Box::new(|document: &mut Value| {
                document["hosts"].as_array_mut().unwrap().remove(1);
            }),
            replaced("  ; the second\n  @ beta\n", ""),
        ),
        (
            "an item of a list on one line changed",
            Box::new(set("/ports/1", json!(8443))),
            replaced("(80 | 443)", "(80 | 8443)"),
        ),
        (
            "an item added to a list on one line",
            Box::new(|document: &mut Value| {
                document["ports"].as_array_mut().unwrap().push(json!(8080));
            }),
            replaced("(80 | 443)", "(80 | 443 | 8080)"),
        ),
        (
            "an entry added to a map on one line",
            Box::new(|document: &mut Value| {
                document["tls"]["key"] = json!("k.pem");
            }),
            replaced("on := yes>", "on := yes | key := k.pem>"),
        ),
    ]
}

#[test]
fn edits_in_every_view_of_an_odd_host_change_only_their_lines() {
    let mut failures = Vec::new();
    for (label, change, expected) in edits() {
        for edit in ALL {
            match save(odd(), edit, HOST, &*change) {
                Ok(saved) if saved == expected => {}
                Ok(saved) => failures.push(format!(
                    "{label}, edited as {edit}:\n--- expected\n{expected}--- saved\n{saved}"
                )),
                Err(error) => failures.push(format!("{label}, edited as {edit}: {error}")),
            }
        }
    }
    report(failures);
}

/// A YAML host edited in the odd view saves like an edit from any view.
#[test]
fn edits_in_the_odd_view_of_a_built_in_host_change_only_their_lines() {
    let yaml = "# head\nname: web # the name\nport: 80\nhosts:\n  # first\n  - alpha\n  - beta\n";
    let cases: Vec<(&str, Change, &str)> = vec![
        (
            "a value changed",
            Box::new(set("/port", json!(81))),
            "# head\nname: web # the name\nport: 81\nhosts:\n  # first\n  - alpha\n  - beta\n",
        ),
        (
            "a list item added",
            Box::new(|document: &mut Value| {
                document["hosts"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("gamma"));
            }),
            "# head\nname: web # the name\nport: 80\nhosts:\n  # first\n  - alpha\n  - beta\n  - gamma\n",
        ),
        (
            "a key removed",
            Box::new(|document: &mut Value| {
                document.as_object_mut().unwrap().shift_remove("port");
            }),
            "# head\nname: web # the name\nhosts:\n  # first\n  - alpha\n  - beta\n",
        ),
    ];
    let mut failures = Vec::new();
    for (label, change, expected) in cases {
        match save(Language::YAML, odd(), yaml, &*change) {
            Ok(saved) if saved == expected => {}
            Ok(saved) => failures.push(format!(
                "{label}:\n--- expected\n{expected}--- saved\n{saved}"
            )),
            Err(error) => failures.push(format!("{label}: {error}")),
        }
    }
    report(failures);
}

/// The odd host reads as its value, and its writer's text reads back.
#[test]
fn the_odd_language_reads_and_writes_its_own_documents() {
    let value = Language::new(&ODD).parse(HOST).unwrap();
    assert_eq!(
        value,
        json!({
            "name": "web", "note": "don't", "port": 80, "ports": [80, 443],
            "tls": {"cert": "a.pem", "on": true},
            "hosts": ["alpha", "beta"], "limits": {"max": 5, "min": 1}
        })
    );
    let written = super::write::document(&value).unwrap();
    assert_eq!(Language::new(&ODD).parse(&written).unwrap(), value);
}

/// A key is written as the language's own writer writes it (the trait's
/// default `written_key`), not by a rule of the built-in languages.
#[test]
fn a_key_is_written_as_the_odd_writer_writes_it() {
    use super::super::language::ConfigLanguage;
    assert_eq!(ODD.written_key("", "", "listen").as_deref(), Some("listen"));
    assert_eq!(ODD.written_key("", "", "a b").as_deref(), Some("\"a b\""));
    assert_eq!(ODD.written_key("", "", "x.y").as_deref(), Some("\"x.y\""));
}
