//! A digit typed into a value of a view, saved to the host: the numbers
//! the edit may have spelled anew are found in one read of the view's
//! text, not one read per number of the document (a key typed in a YAML
//! view of a 7,500-line HCL file paused five seconds), and placed without
//! reading the text from the top for each one (a pause growing with the
//! square of the document's length).

use serde_json::{Value, json};

use super::super::languages::Language;
use super::super::work_count::{Work, counted};
use super::Bridge;

/// The `work` saving a digit typed after the middle service's `typed`
/// value (its `port`, a number, or its `image`, a string), in a `view` of
/// a `host` text of `services` services.
fn work(work: Work, typed: &str, (host, view): (Language, Language), services: usize) -> usize {
    let list: serde_json::Map<String, Value> = (0..services)
        .map(|index| {
            (
                format!("svc_{index}"),
                json!({"image": format!("app-{index}"), "port": 1000 + index, "weight": index}),
            )
        })
        .collect();
    let data = match host.label() {
        "XML" => json!({ "root": list }),
        _ => Value::Object(list),
    };
    let host_text = host.serialize(&data).expect("writes");
    let bridge = Bridge::new(host, view, None).expect("two languages");
    let shown = bridge.to_edit(&host_text).expect("shows").text;
    let value = match typed {
        "port" => format!("{}", 1000 + services / 2),
        _ => format!("app-{}", services / 2),
    };
    let at = shown.find(&value).expect("shown") + value.len();
    let edited = format!("{}7{}", &shown[..at], &shown[at..]);
    let (saved, count) = counted(work, || bridge.to_host(&edited, &host_text));
    assert!(
        saved.expect("saves").contains(&format!("{value}7")),
        "{host} as {view}"
    );
    count
}

/// Each pair of languages whose `work` saving a digit typed into `typed`
/// grows from 20 services to 200.
fn grown(kind: Work, typed: &str) -> Vec<String> {
    let mut grew = Vec::new();
    for &host in Language::ALL {
        for &view in Language::ALL {
            if host == view {
                continue;
            }
            let few = work(kind, typed, (host, view), 20);
            let many = work(kind, typed, (host, view), 200);
            if many > few {
                grew.push(format!(
                    "{host} as {view}, {typed}: {few} for 20 services, {many} for 200"
                ));
            }
        }
    }
    grew
}

#[test]
fn a_digit_typed_in_a_view_is_saved_in_reads_however_many_numbers_the_text_has() {
    let grew: Vec<String> = ["port", "image"]
        .into_iter()
        .flat_map(|typed| grown(Work::DocumentRead, typed))
        .collect();
    assert!(grew.is_empty(), "{grew:#?}");
}

#[test]
fn a_digit_typed_in_a_view_is_placed_without_reading_the_text_from_the_top_per_number() {
    let grew: Vec<String> = ["port", "image"]
        .into_iter()
        .flat_map(|typed| grown(Work::LineSeek, typed))
        .collect();
    assert!(grew.is_empty(), "{grew:#?}");
}
