//! Many locks in a large text, in every language: placing them reads the
//! document a fixed number of times, not once (or more) per lock, whether
//! they come from the host's paths (single values or blocks) or the
//! schema's `readOnly`, and each lock is where placing it alone puts it;
//! and the fields the host's read-only ranges cover are found in one read
//! however many fields the text has.

use serde_json::{Value, json};

use super::super::Language;
use super::super::codec;
use super::super::paths::FieldPath;
use super::super::work_count::{Work, counted};
use super::locate::{Extent, LineIndex, extent, lookup};
use super::locks::{Protected, Sources, protect};
use super::save::range_pointers;

/// The fields two ranges cover (the `host` values of services 1 and 3) in
/// a text of `blocks` services, and the document reads finding them took.
fn covered(language: Language, blocks: usize) -> (Vec<String>, usize) {
    let text = text(language, blocks);
    let behavior = language.behavior();
    let document = codec::read(behavior, &text, None).expect("parses");
    let ranges: Vec<std::ops::Range<usize>> = [1, 3]
        .iter()
        .map(|index| {
            let host = format!("h{index}.example.com");
            let at = text[..text.find(&host).expect("written")]
                .encode_utf16()
                .count();
            at + 1..at + 2
        })
        .collect();
    counted(Work::DocumentRead, || {
        range_pointers(behavior, &text, &document, &ranges)
    })
}

#[test]
fn the_fields_ranges_cover_are_found_in_a_fixed_number_of_reads_in_every_language() {
    let mut grew = Vec::new();
    for &language in Language::ALL {
        let (few, few_reads) = covered(language, 20);
        let (many, many_reads) = covered(language, 200);
        let wanted: Vec<String> = [1, 3]
            .iter()
            .map(|index| format!("{}/svc_{index}/host", root(language)))
            .collect();
        let sorted = |mut found: Vec<String>| {
            found.sort();
            found
        };
        assert_eq!(sorted(few), wanted, "{language}");
        assert_eq!(sorted(many), wanted, "{language}");
        if few_reads != many_reads {
            grew.push(format!(
                "{language}: {few_reads} reads for 20 services, {many_reads} for 200"
            ));
        }
    }
    assert!(grew.is_empty(), "{grew:#?}");
}

/// `blocks` services, each with a `host` and a `port`.
fn text(language: Language, blocks: usize) -> String {
    let each = |index: usize| match language.label() {
        "YAML" => format!("svc_{index}:\n  host: h{index}.example.com # c\n  port: {index}\n"),
        "JSON" => format!(
            "  \"svc_{index}\": {{\n    \"host\": \"h{index}.example.com\",\n    \"port\": {index}\n  }}{}\n",
            if index + 1 == blocks { "" } else { "," }
        ),
        "TOML" => format!("[svc_{index}]\nhost = \"h{index}.example.com\" # c\nport = {index}\n\n"),
        "HCL" => {
            format!("svc_{index} {{\n  host = \"h{index}.example.com\"\n  port = {index}\n}}\n")
        }
        _ => format!(
            "  <svc_{index}>\n    <host>h{index}.example.com</host>\n    <port>{index}</port>\n  </svc_{index}>\n"
        ),
    };
    let body: String = (0..blocks).map(each).collect();
    match language.label() {
        "JSON" => format!("{{\n{body}}}\n"),
        "XML" => format!("<root>\n{body}</root>\n"),
        _ => body,
    }
}

/// Where the services are: under the root element in XML.
fn root(language: Language) -> &'static str {
    if language.label() == "XML" {
        "/root"
    } else {
        ""
    }
}

/// What is locked, and by whom.
#[derive(Clone, Copy, Debug)]
enum Kind {
    /// The host's paths, each service's `host`.
    Values,
    /// The host's paths, whole services.
    Blocks,
    /// The schema's `readOnly`, each service's `host`.
    Schema,
}

const KINDS: [Kind; 3] = [Kind::Values, Kind::Blocks, Kind::Schema];

/// A schema marking the `host` of the first `locks` services read-only.
fn marking(language: Language, locks: usize) -> Value {
    let properties: serde_json::Map<String, Value> = (0..locks)
        .map(|index| {
            (
                format!("svc_{index}"),
                json!({"type": "object", "properties": {"host": {"type": "string", "readOnly": true}}}),
            )
        })
        .collect();
    let services = json!({"type": "object", "properties": properties});
    if root(language).is_empty() {
        services
    } else {
        json!({"type": "object", "properties": {"root": services}})
    }
}

fn locked(language: Language, text: &str, locks: usize, kind: Kind) -> Protected {
    let marks = marking(language, locks);
    let field = match kind {
        Kind::Values => "/host",
        _ => "",
    };
    let paths: Vec<FieldPath> = match kind {
        Kind::Schema => Vec::new(),
        _ => (0..locks)
            .map(|index| FieldPath::from(format!("{}/svc_{index}{field}", root(language)).as_str()))
            .collect(),
    };
    protect(
        language.behavior(),
        text,
        Sources {
            marking_schema: matches!(kind, Kind::Schema).then_some(&marks),
            reading_schema: None,
            paths: &paths,
            ..Sources::default()
        },
    )
}

/// Reads of the whole document (span lookups and the readings shared with
/// them, line splits) placing `locks` locks.
fn reads(language: Language, text: &str, locks: usize, kind: Kind) -> (usize, usize) {
    let ((protected, splits), reads) = counted(Work::DocumentRead, || {
        counted(Work::LineSplit, || locked(language, text, locks, kind))
    });
    assert_eq!(
        protected.locks.len(),
        locks,
        "{language} ({kind:?}): every lock placed"
    );
    (reads, splits)
}

#[test]
fn placing_many_locks_reads_the_document_a_fixed_number_of_times() {
    let mut grew = Vec::new();
    for &language in Language::ALL {
        let text = text(language, 400);
        for kind in KINDS {
            let few = reads(language, &text, 2, kind);
            let many = reads(language, &text, 200, kind);
            if many != few {
                grew.push(format!(
                    "{language} ({kind:?}): 2 locks {few:?}, 200 locks {many:?} (reads, line splits)"
                ));
            }
        }
    }
    assert!(grew.is_empty(), "{grew:#?}");
}

/// The locks placed together are where each one is placed alone, its spans
/// read on their own (outside any batch).
#[test]
fn locks_placed_together_are_where_each_one_alone_is() {
    for &language in Language::ALL {
        let text = text(language, 30);
        let behavior = language.behavior();
        let document = codec::read(behavior, &text, None).expect("parses");
        let lines = LineIndex::new(&text);
        for kind in KINDS {
            let together = locked(language, &text, 30, kind).locks;
            assert_eq!(together.len(), 30, "{language} ({kind:?})");
            for lock in together {
                let pointer = lock.pointer.clone().expect("a field");
                let value = lookup(&document, &pointer);
                let alone = extent(behavior, &text, &lines, value, None, &pointer);
                assert_eq!(
                    alone,
                    Some(Extent {
                        key: lock.key,
                        ranges: lock.ranges
                    }),
                    "{language} ({kind:?}) {pointer}"
                );
            }
        }
    }
}
