//! A digit typed into a number of a view, saved to the host: the numbers
//! the edit may have spelled anew are found in one read of the view's
//! text, not one read per number of the document (a key typed in a YAML
//! view of a 7,500-line HCL file paused five seconds).

use serde_json::{Value, json};

use super::super::languages::Language;
use super::super::work_count::{Work, counted};
use super::Bridge;

/// The reads of the document saving a digit typed into the middle
/// service's port, in a `view` of a `host` text of `services` services.
fn reads(host: Language, view: Language, services: usize) -> usize {
    let list: serde_json::Map<String, Value> = (0..services)
        .map(|index| {
            (
                format!("svc_{index}"),
                json!({"port": 1000 + index, "weight": index}),
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
    let port = format!("{}", 1000 + services / 2);
    let at = shown.find(&port).expect("shown") + port.len();
    let typed = format!("{}7{}", &shown[..at], &shown[at..]);
    let (saved, reads) = counted(Work::DocumentRead, || bridge.to_host(&typed, &host_text));
    assert!(
        saved.expect("saves").text.contains(&format!("{port}7")),
        "{host} as {view}"
    );
    reads
}

#[test]
fn a_digit_typed_in_a_view_is_saved_in_reads_however_many_numbers_the_text_has() {
    let mut grew = Vec::new();
    for &host in Language::ALL {
        for &view in Language::ALL {
            if host == view {
                continue;
            }
            let (few, many) = (reads(host, view, 20), reads(host, view, 200));
            if many > few {
                grew.push(format!(
                    "{host} as {view}: {few} reads for 20 services, {many} for 200"
                ));
            }
        }
    }
    assert!(grew.is_empty(), "{grew:#?}");
}
