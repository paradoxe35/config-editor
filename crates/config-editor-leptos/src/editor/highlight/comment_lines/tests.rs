use super::super::super::languages::Language;
use super::super::super::line_rows::Highlighting;
use super::super::TokenKind;
use super::{comment_lines, covered};

/// Each line of `source` as the rows color it: the text of its comment
/// tokens, and of the rest of its tokens (joined by `|`).
fn colored(language: Language, source: &str) -> Vec<(String, String)> {
    let comments = comment_lines(source, &language.behavior().comments(source));
    let highlighting = Highlighting {
        language: Some(language),
        string_bodies: &language.behavior().string_body_lines(source),
        comments: &comments,
    };
    source
        .split('\n')
        .enumerate()
        .map(|(index, line)| {
            let tokens = highlighting.of(index).tokens(line);
            let text = |comment: bool| {
                tokens
                    .iter()
                    .filter(|token| (token.kind == TokenKind::Comment) == comment)
                    .map(|token| &line[token.start..token.end])
                    .collect::<Vec<_>>()
                    .join("|")
            };
            (text(true), text(false))
        })
        .collect()
}

/// Languages with comments over several lines, each a document with one
/// opened mid-line and closed mid-line with markup after it, one on lines of
/// its own, one inside a nested block, an unclosed one at the end, and the
/// expected comment text of each line.
fn cases() -> Vec<(Language, &'static str, Vec<&'static str>)> {
    vec![
        #[cfg(feature = "xml")]
        (
            Language::XML,
            "<config>\n  <a>1</a> <!-- <listen>7835</listen>\n  <b>x</b> --> <c>2</c>\n  <limits>\n    <!--\n    <max>5</max>\n    -->\n  </limits>\n  <!-- one line -->\n  <!--\n  <open>",
            vec![
                "",
                "<!-- <listen>7835</listen>",
                "  <b>x</b> -->",
                "",
                "<!--",
                "    <max>5</max>",
                "    -->",
                "",
                "<!-- one line -->",
                "<!--",
                "  <open>",
            ],
        ),
        #[cfg(feature = "hcl")]
        (
            Language::HCL,
            "a = 1 /* listen = 7835\n  b = \"x\" */ c = 2\nlimits {\n  /*\n  max = 5\n  */\n}\ns = \"/* not a comment\"\nh = <<EOT\n/* body\nEOT\n/* one line */\n/*\nopen = 1",
            vec![
                "/* listen = 7835",
                "  b = \"x\" */",
                "",
                "/*",
                "  max = 5",
                "  */",
                "",
                "",
                "",
                "",
                "",
                "/* one line */",
                "/*",
                "open = 1",
            ],
        ),
    ]
}

#[test]
fn every_line_of_a_comment_over_several_lines_is_a_comment_in_every_language() {
    let mut failures = Vec::new();
    for (language, source, expected) in cases() {
        let lines = colored(language, source);
        assert_eq!(
            lines.len(),
            expected.len(),
            "{}: line count",
            language.info().label
        );
        for (index, ((comment, rest), want)) in lines.iter().zip(&expected).enumerate() {
            if comment != want {
                failures.push(format!(
                    "{} line {}: comment {comment:?}, expected {want:?} (rest {rest:?})",
                    language.info().label,
                    index + 1
                ));
            }
        }
    }
    // After a comment closes, the line goes on in the language's colors.
    #[cfg(feature = "xml")]
    {
        let lines = colored(Language::XML, "<!--\n--> <c>2</c>");
        if lines[1].1 != "<|c|>|2|</|c|>" {
            failures.push(format!("XML after the comment: {:?}", lines[1].1));
        }
    }
    #[cfg(feature = "hcl")]
    {
        let lines = colored(Language::HCL, "/*\n*/ c = 2");
        if lines[1].1 != "c|=|2" {
            failures.push(format!("HCL after the comment: {:?}", lines[1].1));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A document in each language with comments, and markers that are not
/// comments (in strings, in multi-line string bodies), and its comments.
fn comment_cases() -> Vec<(Language, &'static str, Vec<&'static str>)> {
    vec![
        #[cfg(feature = "yaml")]
        (
            Language::YAML,
            "a: 1 # one\ns: \"# not\"\nb: |\n  # body\nc: 2\n# two",
            vec!["# one", "# two"],
        ),
        #[cfg(feature = "json")]
        (
            Language::JSON,
            "{\"a\": \"# not // not /* not */\"}",
            vec![],
        ),
        #[cfg(feature = "toml")]
        (
            Language::TOML,
            "a = 1 # one\ns = \"# not\"\nm = \"\"\"\n# body\n\"\"\"\n# two",
            vec!["# one", "# two"],
        ),
        #[cfg(feature = "hcl")]
        (
            Language::HCL,
            "a = 1 # one\nb = 2 // two\ns = \"# not /* not\"\n/* three\n */\nh = <<EOT\n# body\nEOT",
            vec!["# one", "// two", "/* three\n */"],
        ),
        #[cfg(feature = "xml")]
        (
            Language::XML,
            "<a># not</a> <!-- one -->\n<b x=\"&lt;!-- not\"/>\n<!-- two\n-->",
            vec!["<!-- one -->", "<!-- two\n-->"],
        ),
    ]
}

#[test]
fn comments_are_found_in_every_language() {
    let mut failures = Vec::new();
    for (language, source, expected) in comment_cases() {
        let found: Vec<&str> = language
            .behavior()
            .comments(source)
            .into_iter()
            .map(|range| &source[range])
            .collect();
        if found != expected {
            failures.push(format!(
                "{}: {found:?}, expected {expected:?}",
                language.info().label
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn covered_reads_the_lines_past_each_comments_first() {
    // Comments on lines 2–4 (ending at byte 5 of line 4) and 6–7 (unclosed).
    let source = "a\n<!--\nb\nc -->d\ne\n<!--\nf";
    let first = source.find("<!--").unwrap()..source.find("-->").unwrap() + 3;
    let second = source.rfind("<!--").unwrap()..source.len();
    let comments = comment_lines(source, &[first, second]);
    assert_eq!(comments.len(), 2);
    let seen: Vec<Option<usize>> = (0..7).map(|line| covered(&comments, line)).collect();
    assert_eq!(
        seen,
        vec![None, None, Some(usize::MAX), Some(5), None, None, Some(1)]
    );
}
