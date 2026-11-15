//! The JSON reader's errors in plain words, as the other languages'
//! problems are told: what is missing or wrong and how to write it, for a
//! JSON document and a schema text written as JSON alike.

/// What a string or key in single quotes is told.
const SINGLE_QUOTES: &str =
    "JSON writes text and keys in double quotes, not single quotes: write \"…\"";

/// `error` (the reader's, of `text`) in plain words, without its place;
/// text past the end is said to follow the end of the `whole` ("document",
/// "schema").
pub(crate) fn plain_words(error: &serde_json::Error, text: &str, whole: &str) -> String {
    let message = error.to_string();
    let (line, column) = (error.line(), error.column());
    let bare = message
        .strip_suffix(&format!(" at line {line} column {column}"))
        .unwrap_or(&message);
    let at = char_at(text, line, column);
    worded(bare, at, whole).unwrap_or_else(|| bare.to_string())
}

/// The reader's `message` in plain words, `at` the character it stops at.
fn worded(message: &str, at: Option<char>, whole: &str) -> Option<String> {
    let text = match message {
        "EOF while parsing a list" => "the text ends before the list is closed: a ] is missing",
        "EOF while parsing an object" => "the text ends before the map is closed: a } is missing",
        "EOF while parsing a string" => "the text ends inside a string: a closing \" is missing",
        "EOF while parsing a value" => "the text ends where a value should follow",
        "trailing characters" => return Some(format!("more text follows the end of the {whole}")),
        "trailing comma" => "a comma before a closing ] or } is not allowed in JSON: remove it",
        "expected `:`" => "a colon must follow the key, as in \"key\": value",
        "expected `,` or `]`" => {
            "a comma or ] is missing: the items of a list are separated by commas"
        }
        "expected `,` or `}`" => {
            "a comma or } is missing: the entries of a map are separated by commas"
        }
        "expected `\"`" => "a key in double quotes is missing here",
        "expected ident" => {
            "this word is no JSON value: write true, false or null, or text in double quotes"
        }
        "expected value" | "key must be a string" if at == Some('\'') => SINGLE_QUOTES,
        "expected value" if at == Some(',') => "a value is missing before this comma",
        "expected value" => {
            "no JSON value starts here: write text in double quotes, or a number, true, false or null"
        }
        "key must be a string" => "a key must be text in double quotes, as in \"name\"",
        "invalid number" => {
            "this number is not written as JSON writes numbers: no leading zero, no + sign, digits on both sides of a point"
        }
        "number out of range" => "this number is too large to read",
        "invalid escape" => {
            "unknown escape in a string: JSON knows \\\" \\\\ \\/ \\b \\f \\n \\r \\t and \\u with four hex digits"
        }
        "invalid unicode code point"
        | "lone leading surrogate in hex escape"
        | "unexpected end of hex escape" => {
            "a \\u escape must be four hex digits naming a character"
        }
        "control character (\\u0000-\\u001F) found while parsing a string" => {
            "a string holds a tab, line break or other control character as it is: write it as an escape, as in \\t or \\n"
        }
        _ => return None,
    };
    Some(text.to_string())
}

/// The character of `text` at one-based `line` and `column`.
fn char_at(text: &str, line: usize, column: usize) -> Option<char> {
    text.split('\n')
        .nth(line.checked_sub(1)?)?
        .chars()
        .nth(column.checked_sub(1)?)
}

#[cfg(test)]
mod tests {
    use super::plain_words;

    /// Every reader error a JSON text can meet, in plain words.
    #[test]
    fn every_json_error_reads_plainly() {
        let cases = [
            (
                "{\"a\": 1,}",
                "a comma before a closing ] or } is not allowed in JSON: remove it",
            ),
            ("{'a': 1}", super::SINGLE_QUOTES),
            ("{\"a\": 'x'}", super::SINGLE_QUOTES),
            (
                "{\"a\": tru}",
                "this word is no JSON value: write true, false or null, or text in double quotes",
            ),
            (
                "{\"a\": 01}",
                "this number is not written as JSON writes numbers: no leading zero, no + sign, digits on both sides of a point",
            ),
            ("[1,,2]", "a value is missing before this comma"),
            (
                "[1, x]",
                "no JSON value starts here: write text in double quotes, or a number, true, false or null",
            ),
            (
                "{\"a\" 1}",
                "a colon must follow the key, as in \"key\": value",
            ),
            (
                "[1 2]",
                "a comma or ] is missing: the items of a list are separated by commas",
            ),
            (
                "{\"a\": 1 \"b\": 2}",
                "a comma or } is missing: the entries of a map are separated by commas",
            ),
            (
                "{1: 2}",
                "a key must be text in double quotes, as in \"name\"",
            ),
            (
                "{\"a\": \"x\ty\"}",
                "a string holds a tab, line break or other control character as it is: write it as an escape, as in \\t or \\n",
            ),
            (
                "{\"a\": \"\\q\"}",
                "unknown escape in a string: JSON knows \\\" \\\\ \\/ \\b \\f \\n \\r \\t and \\u with four hex digits",
            ),
            (
                "{\"a\": \"\\uD800\"}",
                "a \\u escape must be four hex digits naming a character",
            ),
            ("{} x", "more text follows the end of the document"),
            (
                "[1",
                "the text ends before the list is closed: a ] is missing",
            ),
        ];
        let mut failures = Vec::new();
        for (text, wanted) in cases {
            let error = serde_json::from_str::<serde_json::Value>(text).unwrap_err();
            let said = plain_words(&error, text, "document");
            if said != wanted {
                failures.push(format!("{text:?}\n  said   {said:?}\n  wanted {wanted:?}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// A JSON document's syntax problem is told in the same words.
    #[cfg(feature = "json")]
    #[test]
    fn a_json_document_says_its_syntax_problem_plainly() {
        let behavior = crate::editor::Language::JSON.behavior();
        for (text, wanted) in [
            ("{'a': 1}", super::SINGLE_QUOTES),
            (
                "{\"a\": 01}",
                "this number is not written as JSON writes numbers: no leading zero, no + sign, digits on both sides of a point",
            ),
            (
                "{\"a\": 1,}",
                "a comma before a closing ] or } is not allowed in JSON: remove it",
            ),
        ] {
            let error = behavior.parse(text).unwrap_err();
            assert_eq!(error.message, wanted, "{text:?}");
        }
        // The trailing comma is still marked where it is written.
        let error = behavior.parse("{\"a\": 1,}").unwrap_err();
        assert_eq!((error.line, error.column), (Some(1), Some(8)));
    }
}
