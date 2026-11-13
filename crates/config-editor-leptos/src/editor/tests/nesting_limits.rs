//! A text nested deeper than its reader goes says so in one sentence in
//! every language, at the place it goes too deep (D23.a11y-4); and YAML
//! reads a file as large as it is written, with no count of keys, items or
//! documents of its own (a stream is told it is one, however long) (D23.editing-2).

use crate::editor::languages::Language;

/// `levels` lists inside each other in `language`, on a line after a first
/// one, so the place reported is past the start.
fn nested(language: Language, levels: usize) -> String {
    let lists = format!("{}{}", "[".repeat(levels), "]".repeat(levels));
    match language.label().to_ascii_lowercase().as_str() {
        "json" => format!("{{\n\"a\": {lists}}}"),
        "yaml" => format!("first: 1\na: {lists}\n"),
        "toml" | "hcl" => format!("first = 1\na = {lists}\n"),
        _ => {
            let open: String = (0..levels).map(|level| format!("<e{level}>")).collect();
            let close: String = (0..levels)
                .rev()
                .map(|level| format!("</e{level}>"))
                .collect();
            format!("<r>\n{open}{close}</r>")
        }
    }
}

#[test]
fn every_language_says_where_a_text_nests_too_deeply_in_one_sentence() {
    for &language in Language::ALL {
        // The README's promise: 64 levels read in every language.
        let deep = nested(language, 63);
        assert!(
            language.behavior().parse(&deep).is_ok(),
            "{language:?}: 63 levels inside the root"
        );
        let error = language
            .behavior()
            .parse(&nested(language, 300))
            .expect_err(&format!("{language:?}: 300 levels"));
        assert!(
            error.message.contains("nest too deeply here")
                && error.message.ends_with("the editor reads no deeper"),
            "{language:?}: {}",
            error.message
        );
        assert_eq!(error.line, Some(2), "{language:?}: {}", error.message);
        assert!(
            error.column.is_some_and(|column| column > 10),
            "{language:?}: {:?}",
            error.column
        );
    }
}

#[test]
fn yaml_block_nesting_too_deep_is_placed_on_its_line() {
    let text: String = (0..200)
        .map(|level| format!("{}k{level}:\n", "  ".repeat(level)))
        .collect::<String>()
        + &"  ".repeat(200)
        + "v: 1\n";
    let error = Language::YAML
        .behavior()
        .parse(&text)
        .expect_err("200 levels");
    assert!(
        error.message.contains("nest too deeply here"),
        "{}",
        error.message
    );
    let line = error.line.expect("a line");
    assert!((120..140).contains(&line), "line {line}");
}

#[cfg(feature = "yaml")]
#[test]
fn yaml_reads_as_many_keys_items_and_documents_as_the_text_holds() {
    let keys: String = (0..70_000).map(|index| format!("k{index}: 1\n")).collect();
    let value = Language::YAML.behavior().parse(&keys).expect("70,000 keys");
    assert_eq!(value["k69999"], 1);
    let items = "- 1\n".repeat(70_000);
    let value = Language::YAML
        .behavior()
        .parse(&items)
        .expect("70,000 items");
    assert_eq!(value.as_array().map(Vec::len), Some(70_000));
    let documents = "---\na: 1\n".repeat(1_836);
    // A stream is refused for what it is, however long, never for a count.
    let error = Language::YAML
        .behavior()
        .parse(&documents)
        .expect_err("1,836 documents");
    assert!(
        error.message.contains("more than one YAML document"),
        "{}",
        error.message
    );
}
