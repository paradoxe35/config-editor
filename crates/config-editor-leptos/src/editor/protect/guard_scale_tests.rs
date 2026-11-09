//! Edits next to many locks, in every language: checking one (refused, or
//! leaving the text broken next to a lock) reads the document, its lines
//! and the paths of the lines it touches as many times with 200 locks as
//! with 2, never once per lock.

use super::super::Language;
use super::super::paths::FieldPath;
use super::super::work_count::{Work, counted};
use super::guard::check;
use super::locks::{Protected, Sources, protect};

/// `blocks` services, each with a `host` (and a comment after it where the
/// language has one), a `port` and a flag.
fn text(language: Language, blocks: usize) -> String {
    let each = |index: usize| match language.label() {
        "YAML" => {
            format!("svc_{index}:\n  host: h{index}.example.com # c\n  port: {index}\n  on: true\n")
        }
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

/// The `host` of the first `locks` services locked by the host's paths.
fn locked(language: Language, text: &str, locks: usize) -> Protected {
    let root = if language.label() == "XML" {
        "/root"
    } else {
        ""
    };
    let paths: Vec<FieldPath> = (0..locks)
        .map(|index| FieldPath::from(format!("{root}/svc_{index}/host").as_str()))
        .collect();
    protect(
        language.behavior(),
        text,
        Sources {
            paths: &paths,
            ..Sources::default()
        },
    )
}

/// Edits at the locked `host` line of service `service`: Backspace before
/// its key (in its indent, else joining it to the line above), Delete at
/// the end of the line above, a character typed right after its value,
/// one typed past the space after it (before a comment), one typed inside
/// it and one three characters before the line's end, and Backspace at the
/// start of the line below.
fn edits(text: &str, service: usize) -> Vec<(&'static str, String)> {
    let value = format!("h{service}.example.com");
    let at = text.find(&value).expect("written");
    let start = text[..at].rfind('\n').expect("a line above") + 1;
    let end = at + text[at..].find('\n').expect("a line below");
    let value_end = at + value.len() + usize::from(text[..at].ends_with('"'));
    let first = start + text[start..].len() - text[start..].trim_start().len();
    let cut = |from: usize, to: usize| format!("{}{}", &text[..from], &text[to..]);
    let typed = |at: usize| format!("{}x{}", &text[..at], &text[at..]);
    vec![
        ("Backspace before the key", cut(first - 1, first)),
        ("Delete at the end of the line above", cut(start - 1, start)),
        ("typing after the value", typed(value_end)),
        (
            "typing past the space after the value",
            typed((value_end + 1).min(end)),
        ),
        ("typing inside the value", typed(at + 3)),
        (
            "typing three characters before the line's end",
            typed(end - 3),
        ),
        (
            "Backspace at the start of the line below",
            cut(end, end + 1),
        ),
    ]
}

/// The verdict on each edit next to the lock checked last (locks are in
/// pointer order), and the document reads, line splits and path lookups
/// checking it took.
fn checked(language: Language, text: &str, locks: usize) -> Vec<(&'static str, bool, [usize; 3])> {
    let protected = locked(language, text, locks);
    assert_eq!(
        protected.locks.len(),
        locks,
        "{language}: every lock placed"
    );
    let last = (0..locks)
        .max_by_key(|index| format!("svc_{index}"))
        .expect("a lock");
    edits(text, last)
        .into_iter()
        .map(|(name, after)| {
            let (((verdict, lookups), splits), reads) = counted(Work::DocumentRead, || {
                counted(Work::LineSplit, || {
                    counted(Work::PathLookup, || {
                        check(language.behavior(), None, &protected, text, &after).is_ok()
                    })
                })
            });
            (name, verdict, [reads, splits, lookups])
        })
        .collect()
}

#[test]
fn checking_an_edit_next_to_many_locks_reads_as_much_as_next_to_two() {
    let mut grew = Vec::new();
    for &language in Language::ALL {
        let text = text(language, 400);
        let few = checked(language, &text, 2);
        let many = checked(language, &text, 200);
        for ((name, few_verdict, few), (_, many_verdict, many)) in few.into_iter().zip(many) {
            assert_eq!(few_verdict, many_verdict, "{language}, {name}");
            if few != many {
                grew.push(format!(
                    "{language}, {name}: 2 locks {few:?}, 200 locks {many:?} (reads, line splits, path lookups)"
                ));
            }
        }
    }
    assert!(grew.is_empty(), "{grew:#?}");
}

/// `text` broken by a line added after service `service`'s port, as the
/// language's syntax breaks it, and the same text after an Enter at the
/// end of that line.
fn broken_then_entered(language: Language, text: &str, service: usize) -> (String, String) {
    let value = format!("h{service}.example.com");
    let at = text.find(&value).expect("written");
    // The end of the port's line, below the host's.
    let host_end = at + text[at..].find('\n').expect("a line below");
    let port_end = host_end + 1 + text[host_end + 1..].find('\n').expect("a line below");
    let breaking = match language.label() {
        "YAML" => "\n  - x: [",
        "JSON" => "{",
        "TOML" => "\n[[",
        "HCL" => "\n{{",
        _ => "\n<a",
    };
    let broken = format!("{}{breaking}{}", &text[..port_end], &text[port_end..]);
    let enter_at = port_end + breaking.len();
    let entered = format!("{}\n{}", &broken[..enter_at], &broken[enter_at..]);
    (broken, entered)
}

/// The bytes read placing offsets (as UTF-16 units) checking an Enter in
/// a broken text of `blocks` services, with five read-only paths the text
/// does not have or, `present`, the first service's port locked; and
/// whether it was let through.
fn enter_in_broken_text(language: Language, blocks: usize, present: bool) -> (usize, bool) {
    let good = text(language, blocks);
    let root = if language.label() == "XML" {
        "/root"
    } else {
        ""
    };
    let paths: Vec<FieldPath> = if present {
        vec![FieldPath::from(format!("{root}/svc_0/port").as_str())]
    } else {
        (0..5)
            .map(|index| FieldPath::from(format!("{root}/nope_{index}/host").as_str()))
            .collect()
    };
    let sources = |previous| Sources {
        paths: &paths,
        previous,
        ..Sources::default()
    };
    let whole = protect(language.behavior(), &good, sources(None));
    let (broken, entered) = broken_then_entered(language, &good, blocks / 2);
    let protected = protect(language.behavior(), &broken, sources(Some(&whole)));
    let (ok, bytes) = counted(Work::OffsetBytes, || {
        check(language.behavior(), None, &protected, &broken, &entered).is_ok()
    });
    (bytes, ok)
}

/// Each Enter in a long text that does not parse, with read-only paths
/// (one it has, or only ones it does not have), is checked in work growing
/// with the text, in every language: four times the text, about four
/// times the bytes read to place offsets (QA 26 and 27: XML counted each
/// element's text from the top, 0.5 s per Enter at 8,000 lines with one
/// lock once a tag name was typed into, 5.6 s at 20,000).
#[test]
fn an_enter_in_a_long_broken_text_with_locks_is_checked_in_linear_time() {
    let mut slow = Vec::new();
    for &language in Language::ALL {
        for present in [false, true] {
            let (small, small_ok) = enter_in_broken_text(language, 500, present);
            let (large, large_ok) = enter_in_broken_text(language, 2000, present);
            assert!(small_ok && large_ok, "{language}: the Enter is let through");
            // Linear is 4 times; a count from the top for each element was 16.
            if large > 8 * small.max(1_000) {
                let locks = if present { "a lock" } else { "missing paths" };
                slow.push(format!(
                    "{language} ({locks}): {small} bytes read for 500 services, {large} for 2000"
                ));
            }
        }
    }
    assert!(slow.is_empty(), "{slow:#?}");
}
