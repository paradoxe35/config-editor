use super::super::super::languages::Language;
use super::super::super::line_rows::Highlighting;
use super::super::TokenKind;
use super::{Spanning, covered};
#[cfg(feature = "yaml")]
use crate::language::MultilineConstruct;

/// Each line of `source` as the rows color it: the text of its comment
/// tokens, and of the rest of its tokens (joined by `|`).
fn colored(language: Language, source: &str) -> Vec<(String, String)> {
    let behavior = language.behavior();
    let spanning = Spanning::of(
        source,
        &behavior.comments(source),
        &behavior.multiline_constructs(source),
    );
    let highlighting = Highlighting {
        language: Some(language),
        string_bodies: &behavior.string_body_lines(source),
        spanning: &spanning,
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
    let comments = Spanning::of(source, &[first, second], &[]).colored;
    assert_eq!(comments.len(), 2);
    let seen: Vec<Option<usize>> = (0..7)
        .map(|line| covered(&comments, line).map(|(end, _)| end))
        .collect();
    assert_eq!(
        seen,
        vec![None, None, Some(usize::MAX), Some(5), None, None, Some(1)]
    );
}

#[cfg(feature = "yaml")]
#[test]
fn strings_over_lines_are_colored_as_strings_to_their_end() {
    // A `'''` string on lines 1–3 whose lines look like entries, a block
    // comment on lines 4–5, and an entry after each on its last line.
    let source = "a: '''\nb: 1\n# c\n''', d: 2\n/* e\nf */ g: 3\nh: 4";
    let string = source.find("'''").unwrap()..source.rfind("'''").unwrap() + 3;
    let comment = source.find("/*").unwrap()..source.find("*/").unwrap() + 2;
    // A comment marker inside the string is no comment.
    let inside = source.find("# c").unwrap()..source.find("# c").unwrap() + 3;
    let spanning = Spanning::of(
        source,
        &[inside, comment],
        &[MultilineConstruct::colored(string, TokenKind::String)],
    );
    let seen: Vec<_> = (0..7)
        .map(|line| covered(&spanning.colored, line))
        .collect();
    assert_eq!(
        seen,
        vec![
            None,
            Some((usize::MAX, TokenKind::String)),
            Some((usize::MAX, TokenKind::String)),
            Some((3, TokenKind::String)),
            None,
            Some((4, TokenKind::Comment)),
            None,
        ]
    );
    let highlighting = Highlighting {
        language: Some(Language::YAML),
        string_bodies: &[],
        spanning: &spanning,
    };
    let kinds = |index: usize| {
        let line = source.split('\n').nth(index).unwrap();
        highlighting
            .of(index)
            .tokens(line)
            .into_iter()
            .map(|token| (line[token.start..token.end].to_string(), token.kind))
            .collect::<Vec<_>>()
    };
    assert_eq!(kinds(2), [("# c".to_string(), TokenKind::String)]);
    let last = kinds(3);
    assert_eq!(last[0], ("'''".to_string(), TokenKind::String));
    assert!(
        last[1..].iter().all(|(_, kind)| *kind != TokenKind::String),
        "{last:?}"
    );
}

/// Each line of `source` as the rows color it: `k(…)` for each key token,
/// `s` string, `v` value, `l` literal, `c` comment, `p` punctuation, `_`
/// uncolored text; each text trimmed, blank ones left out.
fn line_tokens(language: Language, source: &str) -> Vec<String> {
    let behavior = language.behavior();
    let spanning = Spanning::of(
        source,
        &behavior.comments(source),
        &behavior.multiline_constructs(source),
    );
    let highlighting = Highlighting {
        language: Some(language),
        string_bodies: &behavior.string_body_lines(source),
        spanning: &spanning,
    };
    source
        .split('\n')
        .enumerate()
        .map(|(index, line)| {
            super::super::runs(line, &highlighting.of(index).tokens(line))
                .into_iter()
                .filter(|(_, text)| !text.trim().is_empty())
                .map(|(kind, text)| {
                    let letter = match kind {
                        Some(TokenKind::Key) => "k",
                        Some(TokenKind::String) => "s",
                        Some(TokenKind::Value) => "v",
                        Some(TokenKind::Literal) => "l",
                        Some(TokenKind::Comment) => "c",
                        Some(TokenKind::Punctuation) => "p",
                        None => "_",
                    };
                    format!("{letter}({})", text.trim())
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

/// Documents with constructs over several lines in each language (strings,
/// plain scalars, arrays and flow collections, start tags, CDATA, heredocs,
/// comment markers in HCL template sequences), and the tokens of each line.
fn construct_cases() -> Vec<(Language, &'static str, Vec<&'static str>)> {
    vec![
        #[cfg(feature = "toml")]
        (
            Language::TOML,
            "clients = [\n  \"a\", # one\n  \"\"\"b\nc\"\"\", 2,\n]\ns = \"\"\"\nline one\nline two\"\"\"\nt = '''\nx\n''' # after\na = [1, \"x\"] # c\nu = \"\"\"x\\\"\"\" # still\"\"\" # c\nv = \"\"\"a\\\"\"\" # still\nb\"\"\"\nlisten = 7835",
            vec![
                "k(clients) p(=) p([)",
                "s(\"a\") p(,) c(# one)",
                "s(\"\"\"b)",
                "s(c\"\"\") p(,) l(2) p(,)",
                "p(])",
                "k(s) p(=) s(\"\"\")",
                "s(line one)",
                "s(line two\"\"\")",
                "k(t) p(=) s(''')",
                "s(x)",
                "s(''') c(# after)",
                "k(a) p(=) p([) l(1) p(,) s(\"x\") p(]) c(# c)",
                "k(u) p(=) s(\"\"\"x\\\"\"\" # still\"\"\") c(# c)",
                "k(v) p(=) s(\"\"\"a\\\"\"\" # still)",
                "s(b\"\"\")",
                "k(listen) p(=) l(7835)",
            ],
        ),
        #[cfg(feature = "yaml")]
        (
            Language::YAML,
            "q: \"line one\n  line two\" # c\np: plain one\n  plain two\n  plain three # c\nf: {max: 5,\n  other: \"x\", list: [1,\n  b]} # c\ns: 'one\n  two'\na: [1, x] # c\nlisten: 7835",
            vec![
                "k(q) p(:) s(\"line one)",
                "s(line two\") c(# c)",
                "k(p) p(:) v(plain one)",
                "v(plain two)",
                "v(plain three) c(# c)",
                "k(f) p(:) p({) k(max) p(:) l(5) p(,)",
                "k(other) p(:) s(\"x\") p(,) k(list) p(:) p([) l(1) p(,)",
                "v(b) p(]) p(}) c(# c)",
                "k(s) p(:) s('one)",
                "s(two')",
                "k(a) p(:) p([) l(1) p(,) v(x) p(]) c(# c)",
                "k(listen) p(:) l(7835)",
            ],
        ),
        #[cfg(feature = "xml")]
        (
            Language::XML,
            "<a x=\"one\n  two\" y=\"3\">text\n  more</a>\n<b\n  k=\"v\"\n  m='w'/>\n<c><![CDATA[c1\nc2\n]]></c>\n<d><![CDATA[x]]></d>\n<e x=\"1 --> 2\" y='<!--'>t</e>\n<f x=\"a -->\n  b\">u</f>\n<listen>7835</listen>",
            vec![
                "p(<) k(a) k(x) p(=) s(\"one)",
                "s(two\") k(y) p(=) s(\"3\") p(>) v(text)",
                "v(more) p(</) k(a) p(>)",
                "p(<) k(b)",
                "k(k) p(=) s(\"v\")",
                "k(m) p(=) s('w') p(/>)",
                "p(<) k(c) p(>) p(<![CDATA[) s(c1)",
                "s(c2)",
                "p(]]>) p(</) k(c) p(>)",
                "p(<) k(d) p(>) p(<![CDATA[) s(x) p(]]>) p(</) k(d) p(>)",
                "p(<) k(e) k(x) p(=) s(\"1 --> 2\") k(y) p(=) s('<!--') p(>) v(t) p(</) k(e) p(>)",
                "p(<) k(f) k(x) p(=) s(\"a -->)",
                "s(b\") p(>) v(u) p(</) k(f) p(>)",
                "p(<) k(listen) p(>) v(7835) p(</) k(listen) p(>)",
            ],
        ),
        #[cfg(feature = "hcl")]
        (
            Language::HCL,
            "a = \"${\"/*\"}\"\nb = \"a${ \"*/\" }b\"\nc = \"a${ x /* c */ }b\"\nd = \"a${\"#\"}b\" # c\ne = \"%{ if true }/*%{ endif }\"\nf = \"${\"${\"/*\"}\"}\"\ng = \"$${\" # c\nh = <<EOT\nx ${\"/*\"} y\nEOT\nl = [\n  \"a\", # c\n  2,\n]\nlisten = 7835",
            vec![
                "k(a) p(=) s(\"${\"/*\"}\")",
                "k(b) p(=) s(\"a${ \"*/\" }b\")",
                "k(c) p(=) s(\"a${ x /* c */ }b\")",
                "k(d) p(=) s(\"a${\"#\"}b\") c(# c)",
                "k(e) p(=) s(\"%{ if true }/*%{ endif }\")",
                "k(f) p(=) s(\"${\"${\"/*\"}\"}\")",
                "k(g) p(=) s(\"$${\") c(# c)",
                "k(h) p(=) s(<<EOT)",
                "s(x ${\"/*\"} y)",
                "s(EOT)",
                "k(l) p(=) p([)",
                "s(\"a\") p(,) c(# c)",
                "l(2) p(,)",
                "p(])",
                "k(listen) p(=) l(7835)",
            ],
        ),
        #[cfg(feature = "json")]
        (
            Language::JSON,
            "{\n  \"a\": [\n    1,\n    \"x\"\n  ]\n}",
            vec![
                "p({)",
                "k(\"a\") p(:) p([)",
                "l(1) p(,)",
                "s(\"x\")",
                "p(])",
                "p(})",
            ],
        ),
    ]
}

#[test]
fn every_line_of_a_construct_over_several_lines_is_colored_as_what_it_is_in_every_language() {
    let mut failures = Vec::new();
    for (language, source, expected) in construct_cases() {
        let lines = line_tokens(language, source);
        assert_eq!(
            lines.len(),
            expected.len(),
            "{}: line count",
            language.info().label
        );
        for (index, (seen, want)) in lines.iter().zip(&expected).enumerate() {
            if seen != want {
                failures.push(format!(
                    "{} line {}: {seen}\n    expected {want}",
                    language.info().label,
                    index + 1
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Every prefix of each construct document (as while typing it, also with
/// the document's last line after it), and its lines rotated: every line's
/// tokens are in order, on character boundaries and inside the line (`runs`
/// checks it).
#[test]
fn constructs_cut_anywhere_still_color_every_line_in_order() {
    for (language, source, expected) in construct_cases() {
        let tail = *expected.last().unwrap();
        let last_line = source.rsplit('\n').next().unwrap();
        let mut cuts: Vec<usize> = (0..=source.len())
            .filter(|at| source.is_char_boundary(*at))
            .collect();
        cuts.dedup();
        for cut in cuts {
            let prefix = &source[..cut];
            line_tokens(language, prefix);
            let closed = format!("{}\n\n{last_line}", &source[..cut]);
            line_tokens(language, &closed);
        }
        let lines: Vec<&str> = source.split('\n').collect();
        for turn in 1..lines.len() {
            let rotated = [&lines[turn..], &lines[..turn]].concat().join("\n");
            line_tokens(language, &rotated);
        }
        let seen = line_tokens(language, source);
        assert_eq!(
            seen.last().map(String::as_str),
            Some(tail),
            "{}",
            language.info().label
        );
    }
}

/// A value of many words on one line is drawn as one run in every language,
/// not a run per word (a 100,000-character XML text line was 32,000 spans).
#[test]
fn a_long_value_of_words_is_one_run_in_every_language() {
    let line = |language: Language, words: usize| -> String {
        let value: Vec<String> = (0..words).map(|index| format!("w{index}")).collect();
        let value = value.join(" ");
        match language.info().label {
            "YAML" => format!("long: {value}"),
            "JSON" => format!("  \"long\": \"{value}\","),
            "XML" => format!("  <long>{value}</long>"),
            _ => format!("long = \"{value}\""),
        }
    };
    let mut grew = Vec::new();
    for &language in Language::ALL {
        let runs = |words: usize| {
            let line = line(language, words);
            colored(language, &line)[0].1.split('|').count()
        };
        let (few, many) = (runs(3), runs(1000));
        if few != many {
            grew.push(format!(
                "{language}: {few} runs for 3 words, {many} for 1000"
            ));
        }
    }
    assert!(grew.is_empty(), "{grew:#?}");
}
