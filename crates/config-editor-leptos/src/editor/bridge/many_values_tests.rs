//! Edits of many values at once in a view (a replace-all, a paste of many
//! entries, a reorder of many keys or items, keys renamed all at once),
//! saved to the host: each value the save writes is placed from the text's
//! lines split once, not by reading the text from the top for each one (a
//! pause growing with the square of the document's length).

use serde_json::{Map, Value, json};

use super::super::languages::Language;
use super::super::work_count::{Work, counted};
use super::Bridge;

/// `count` services made by `service`.
fn services(count: usize, service: impl Fn(usize) -> (String, Value)) -> Map<String, Value> {
    (0..count).map(service).collect()
}

fn service(index: usize) -> (String, Value) {
    (
        format!("svc_{index}"),
        json!({"image": format!("app-{index}"), "port": 1000 + index, "weight": index}),
    )
}

/// `count` services, the host's data before the edit.
fn before(count: usize) -> Map<String, Value> {
    services(count, service)
}

/// `count` services in a list.
fn listed(count: usize, order: impl Fn(usize) -> usize) -> Map<String, Value> {
    let items: Vec<Value> = (0..count).map(|index| service(order(index)).1).collect();
    Map::from_iter([("services".to_string(), Value::Array(items))])
}

/// An edit: its name, the data before it and the data it leaves, of
/// `count` services.
type Edit = (
    &'static str,
    fn(usize) -> Map<String, Value>,
    fn(usize) -> Map<String, Value>,
);

/// The ways of editing many values at once.
fn edits() -> Vec<Edit> {
    vec![
        // Every image renamed and every port changed.
        ("replace-all", before, |count| {
            services(count, |index| {
                (
                    format!("svc_{index}"),
                    json!({"image": format!("img-{index}"), "port": 2000 + index, "weight": index}),
                )
            })
        }),
        // As many services again pasted after the last.
        ("paste", before, |count| {
            services(2 * count, |index| {
                let (_, mut value) = service(index % count);
                value["image"] = json!(format!("copy-{index}"));
                (format!("svc_{index}"), value)
            })
        }),
        // The services in the opposite order.
        ("reorder keys", before, |count| {
            services(count, |index| service(count - 1 - index))
        }),
        // Every service's key renamed.
        ("rename keys", before, |count| {
            services(count, |index| {
                (format!("service_{index}"), service(index).1)
            })
        }),
        // A list's items in the opposite order.
        (
            "reorder items",
            |count| listed(count, |index| index),
            |count| listed(count, |index| count - 1 - index),
        ),
    ]
}

/// The data `host` writes, as its text holds `list`.
fn data(host: Language, list: Map<String, Value>) -> Value {
    match host.label() {
        "XML" => json!({ "root": list }),
        _ => Value::Object(list),
    }
}

/// The `work` saving `edit` of `count` services made in a `view` of a
/// `host` text.
fn work(
    work: Work,
    (host, view): (Language, Language),
    (_, from, to): Edit,
    count: usize,
) -> usize {
    let bridge = Bridge::new(host, view, None).expect("two languages");
    let host_text = host.serialize(&data(host, from(count))).expect("writes");
    let wanted = data(host, to(count));
    // The view's text of the edited data, as the view writes it.
    let edited = bridge
        .to_edit(&host.serialize(&wanted).expect("writes"))
        .expect("shows")
        .text;
    let (saved, done) = counted(work, || bridge.to_host(&edited, &host_text));
    let saved = saved.unwrap_or_else(|error| panic!("{host} as {view}: {error:?}"));
    let read = host.behavior().parse(&saved.text).expect("reads");
    assert_eq!(read, wanted, "{host} as {view}");
    done
}

/// Each edit and pair of languages whose `kind` of work grows from 20
/// services to 200: ten times the services, done once each whatever
/// their number.
fn grown(kind: Work) -> Vec<String> {
    let mut grew = Vec::new();
    for edit in edits() {
        for &host in Language::ALL {
            for &view in Language::ALL {
                if host == view {
                    continue;
                }
                let few = work(kind, (host, view), edit, 20);
                let many = work(kind, (host, view), edit, 200);
                if many > few {
                    grew.push(format!(
                        "{}, {host} as {view}: {few} for 20 services, {many} for 200",
                        edit.0
                    ));
                }
            }
        }
    }
    grew
}

#[test]
fn keys_renamed_in_a_view_are_found_in_one_scan_of_the_host_text() {
    let grew = grown(Work::KeyScan);
    assert!(grew.is_empty(), "{grew:#?}");
}

#[test]
fn many_values_edited_in_a_view_are_placed_without_reading_the_text_from_the_top_per_value() {
    let grew = grown(Work::LineSeek);
    assert!(grew.is_empty(), "{grew:#?}");
}
