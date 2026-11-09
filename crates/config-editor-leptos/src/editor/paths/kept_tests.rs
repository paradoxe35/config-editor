//! The spans kept for one text (an editor's `range_of`, `reveal`): the
//! document is read once for any number of lookups while the text stays
//! the same, every lookup giving what `field_range` alone gives, and read
//! again once it changes.

use super::super::Language;
use super::super::work_count::{Work, counted};
use super::{KeptSpans, field_range};

/// `blocks` services in `language`, each with a `host` and a list of ports.
fn text(language: Language, blocks: usize) -> String {
    let mut root = serde_json::Map::new();
    for index in 0..blocks {
        root.insert(
            format!("svc_{index}"),
            serde_json::json!({"host": format!("h{index}"), "ports": [index, index + 1]}),
        );
    }
    language
        .behavior()
        .serialize(&serde_json::json!({ "root": root }), None)
        .expect("every language writes it")
}

/// Pointers to look up: fields of the first `count` services, and one
/// written nowhere.
fn pointers(count: usize) -> Vec<String> {
    (0..count)
        .flat_map(|index| {
            [
                format!("/root/svc_{index}/host"),
                format!("/root/svc_{index}/ports/1"),
                format!("/root/svc_{index}"),
            ]
        })
        .chain(["/root/missing".to_string()])
        .collect()
}

/// The document reads `count` services' lookups take through one kept
/// reading, twice over (the second time from what was kept), and whether
/// each found what `field_range` alone finds.
fn reads(language: Language, text: &str, count: usize) -> (usize, usize) {
    let behavior = language.behavior();
    let mut kept = KeptSpans::default();
    let pointers = pointers(count);
    let lookup = |kept: &mut KeptSpans| {
        counted(Work::DocumentRead, || {
            pointers
                .iter()
                .map(|pointer| kept.with(behavior, text, || field_range(behavior, text, pointer)))
                .collect::<Vec<_>>()
        })
    };
    let (first, first_reads) = lookup(&mut kept);
    let (again, again_reads) = lookup(&mut kept);
    let alone: Vec<_> = pointers
        .iter()
        .map(|pointer| field_range(behavior, text, pointer))
        .collect();
    assert_eq!(first, alone, "{language}");
    assert_eq!(again, alone, "{language}");
    (first_reads, again_reads)
}

#[test]
fn lookups_on_the_same_text_read_it_once_in_every_language() {
    let mut failures = Vec::new();
    for &language in Language::ALL {
        let text = text(language, 60);
        let one = reads(language, &text, 1);
        let many = reads(language, &text, 50);
        if one.0 != many.0 || many.1 != 0 {
            failures.push(format!(
                "{language}: (first time, again) {one:?} document reads for 1 service's lookups, {many:?} for 50"
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A lookup on another text (an edit made, another language shown) reads
/// that one, and finds what `field_range` alone finds in it.
#[test]
fn a_changed_text_is_read_again() {
    for &language in Language::ALL {
        let behavior = language.behavior();
        let mut kept = KeptSpans::default();
        let pointer = "/root/svc_3/host";
        let before = text(language, 3);
        let after = text(language, 4);
        kept.with(behavior, &before, || {
            field_range(behavior, &before, pointer)
        });
        let (found, reads) = counted(Work::DocumentRead, || {
            kept.with(behavior, &after, || field_range(behavior, &after, pointer))
        });
        assert_eq!(found, field_range(behavior, &after, pointer), "{language}");
        assert!(found.is_some() && reads > 0, "{language}: {reads} reads");
        // The same text in another language's reading is not the same.
        let other = Language::ALL
            .iter()
            .find(|other| **other != language)
            .expect("another language")
            .behavior();
        let (_, reads) = counted(Work::DocumentRead, || {
            kept.with(other, &after, || field_range(other, &after, pointer))
        });
        assert!(reads > 0, "{language}: read as another language");
    }
}
