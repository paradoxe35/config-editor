//! A text holding an object of many entries with an error anywhere (in
//! the object, beside it, a key written twice) reports the error the whole
//! parse reports, at its place, without parsing the object whole; a key
//! written twice in two spellings reads as the whole parse reads it.

use super::read::read_with;
use crate::editor::work_count::{Work, counted};

/// `locals { hosts = { … } }` of `entries` entries, entry `index` written
/// as `entry` writes it (`None`: the usual one), then `after`.
fn big(entries: usize, entry: impl Fn(usize) -> Option<String>, after: &str) -> String {
    let mut text = String::from("a = 1\nlocals {\n  hosts = {\n");
    for index in 0..entries {
        let usual = format!("    web_{index} = \"10.0.{}.{}\" # host\n", index >> 8, index & 255);
        text.push_str(&entry(index).unwrap_or(usual));
    }
    text.push_str("  }\n}\n");
    text.push_str(after);
    text
}

/// Texts with an error, and the texts reading as the whole parse reads
/// them, each found without a whole parse.
fn texts(entries: usize) -> Vec<(&'static str, String)> {
    let at = |wanted: usize, written: &'static str| move |index: usize| (index == wanted).then(|| written.to_string());
    let middle = entries / 2;
    vec![
        ("an attribute without a value after", big(entries, |_| None, "broken = \n")),
        ("an attribute without a value before", format!("broken = \n{}", big(entries, |_| None, ""))),
        ("an attribute written twice after", big(entries, |_| None, "a = 2\n")),
        ("an entry typed without its value", big(entries, at(middle, "    typed = \n"), "")),
        ("an entry without its value first", big(entries, at(0, "    typed = \n"), "")),
        ("an entry without its value last", big(entries, at(entries - 1, "    typed = \n"), "")),
        ("a stray closer", big(entries, at(middle, "    }\n"), "")),
        ("an unclosed string", big(entries, at(middle, "    s = \"open\n"), "")),
        ("a key written twice far apart", big(entries, at(entries - 3, "    web_1 = \"dup\"\n"), "")),
        ("a key written twice in one group", big(entries, at(middle, "    web_2 = 1\n    web_2 = 2\n"), "")),
        ("a quoted key written twice", big(entries, at(middle, "    \"web_3\" = 1\n"), "")),
        ("a key written twice, and an error after", big(entries, at(middle, "    \"web_3\" = 1\n    web_4 = 2\n"), "broken = \n")),
        ("an error after a key written twice", big(entries, at(middle, "    web_4 = 2\n    bad = \n"), "")),
        ("a duplicate after an error", big(entries, at(middle, "    bad = \n    web_4 = 2\n"), "")),
        ("several entries a line, one broken", big(entries, at(middle, "    p = 1, q = , r = 3\n"), "")),
        ("a nested object broken", big(entries, at(middle, "    n = {\n      x = \n    }\n"), "")),
        ("a value over lines broken", big(entries, at(middle, "    l = [\n      1,\n      ,\n    ]\n"), "")),
    ]
}

/// The reading of `text` (the data, or the error as the parser words it,
/// its line and column included).
fn reading(text: &str, grouping: bool) -> Result<serde_json::Value, String> {
    read_with(text, false, grouping).map_err(|error| error.to_string())
}

#[test]
fn an_error_beside_or_in_an_object_of_many_entries_is_the_whole_parse_s_without_one() {
    let mut failures = Vec::new();
    for (name, text) in texts(2_000) {
        let (grouped, whole_parses) = counted(Work::UngroupedParse, || reading(&text, true));
        let whole = reading(&text, false);
        if grouped != whole {
            failures.push(format!("{name}: {grouped:?}\nwhole: {whole:?}"));
        }
        if whole_parses > 0 {
            failures.push(format!("{name}: parsed whole {whole_parses} times"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn a_key_written_twice_in_two_spellings_reads_as_parsed_whole() {
    for spelled in ["    \"web_7\" = \"again\"\n", "    \"web_8\" = 1\n    \"web_9\" = \"again\"\n"] {
        let text = big(2_000, |index| (index == 1_500).then(|| spelled.to_string()), "");
        let (grouped, whole_parses) = counted(Work::UngroupedParse, || reading(&text, true));
        assert_eq!(grouped, reading(&text, false), "{spelled}");
        assert!(grouped.is_ok(), "{spelled}: {grouped:?}");
        assert_eq!(whole_parses, 0, "{spelled}");
    }
}

/// Small objects, and texts the groups cannot tell (one line of many
/// entries), still read as parsed whole.
#[test]
fn texts_the_groups_cannot_tell_read_as_parsed_whole() {
    let line = |broken: &str| {
        let entries: Vec<String> = (0..400).map(|index| format!("k{index} = {index}")).collect();
        format!("o = {{ {}{broken} }}\n", entries.join(", "))
    };
    let expression = |index: usize| (index == 150).then(|| "    k = 1 +\n      2\n".to_string());
    for text in [line(""), line(", bad = "), line(", k3 = 1"), big(300, expression, ""), big(300, expression, "x = \n")] {
        assert_eq!(reading(&text, true), reading(&text, false), "{}", &text[..120]);
    }
    for (_, text) in texts(100) {
        assert_eq!(reading(&text, true), reading(&text, false), "{}", &text[..120]);
    }
}
