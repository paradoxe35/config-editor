use super::matcher::{MAX_MATCHES, Matcher, SearchOptions, find_matches};
use super::replace::{replace_all, replace_one};
use super::{SearchResults, count_label};

const PLAIN: SearchOptions = SearchOptions {
    case_sensitive: false,
    whole_word: false,
    regex: false,
};

fn options(case_sensitive: bool, whole_word: bool, regex: bool) -> SearchOptions {
    SearchOptions {
        case_sensitive,
        whole_word,
        regex,
    }
}

/// The matched texts.
fn found(text: &str, query: &str, options: SearchOptions) -> Vec<String> {
    let matcher = Matcher::new(query, options).unwrap().unwrap();
    find_matches(text, &matcher)
        .0
        .iter()
        .map(|found| text[found.start..found.end].to_string())
        .collect()
}

fn starts(text: &str, query: &str, options: SearchOptions) -> Vec<usize> {
    let matcher = Matcher::new(query, options).unwrap().unwrap();
    find_matches(text, &matcher)
        .0
        .iter()
        .map(|found| found.start)
        .collect()
}

#[test]
fn an_empty_query_matches_nothing() {
    assert!(Matcher::new("", PLAIN).unwrap().is_none());
    assert!(
        Matcher::new("", options(true, true, true))
            .unwrap()
            .is_none()
    );
}

#[test]
fn plain_text_ignores_case_unless_asked() {
    let text = "Port: 1\nport: 2\nPORT: 3\nexport: 4";
    assert_eq!(found(text, "port", PLAIN), ["Port", "port", "PORT", "port"]);
    assert_eq!(
        found(text, "port", options(true, false, false)),
        ["port", "port"]
    );
    // Characters outside ASCII fold too, and offsets stay on byte boundaries.
    assert_eq!(found("Ünïcode ünïcode", "ÜNÏ", PLAIN), ["Ünï", "ünï"]);
    assert_eq!(starts("😀a 😀A", "a", PLAIN), [4, 10]);
}

#[test]
fn plain_text_is_not_a_pattern() {
    assert_eq!(found("a.b axb (a)", "a.b", PLAIN), ["a.b"]);
    assert_eq!(found("a.b axb (a)", "(a)", PLAIN), ["(a)"]);
    assert_eq!(found("$1 [x]", "[x]", PLAIN), ["[x]"]);
}

#[test]
fn whole_words_skip_matches_inside_words() {
    let text = "port export port_name ports port";
    let whole = options(false, true, false);
    assert_eq!(starts(text, "port", whole), [0, 28]);
    // A match next to punctuation is a word on that side.
    assert_eq!(found("a-x -x", "-x", whole), ["-x", "-x"]);
    // A rejected candidate does not hide a whole word overlapping it.
    assert_eq!(starts("xa a a", "a a", whole), [3]);
    // With a regular expression too.
    assert_eq!(starts(text, "po?rt", options(false, true, true)), [0, 28]);
}

#[test]
fn regular_expressions_match_with_their_flags() {
    let text = "listen: 7835\nLISTEN: 80\nlog: info";
    let regex = options(false, false, true);
    assert_eq!(found(text, r"\d+", regex), ["7835", "80"]);
    assert_eq!(found(text, "^l\\w+", regex), ["listen", "LISTEN", "log"]);
    assert_eq!(
        found(text, "^l\\w+", options(true, false, true)),
        ["listen", "log"]
    );
    // `$` ends lines, not only the document.
    assert_eq!(found(text, r"\d+$", regex), ["7835", "80"]);
    // Empty matches are skipped instead of looping.
    assert_eq!(found("ab", "x*", regex), Vec::<String>::new());
    assert_eq!(found("aab", "a*", regex), ["aa"]);
}

#[test]
fn an_invalid_pattern_is_an_error_not_a_panic() {
    for pattern in ["(", "[a-", "a{2,1}", "*", "(?<", "\\"] {
        let error = Matcher::new(pattern, options(false, false, true))
            .err()
            .unwrap_or_else(|| panic!("{pattern} should not compile"));
        assert!(!error.is_empty(), "{pattern}");
        assert!(!error.contains('\n'), "{pattern}: {error}");
    }
    // The same text is fine as plain text.
    assert!(Matcher::new("(", PLAIN).unwrap().is_some());
}

#[test]
fn matches_know_their_line() {
    let text = "a: x\nb: y\n\nc: x x";
    let matcher = Matcher::new("x", PLAIN).unwrap().unwrap();
    let (matches, capped) = find_matches(text, &matcher);
    assert!(!capped);
    let lines: Vec<(usize, usize)> = matches
        .iter()
        .map(|found| (found.line, found.line_start))
        .collect();
    assert_eq!(lines, [(0, 0), (3, 11), (3, 11)]);
}

#[test]
fn matches_are_capped() {
    let text = "a ".repeat(MAX_MATCHES + 5);
    let matcher = Matcher::new("a", PLAIN).unwrap().unwrap();
    let (matches, capped) = find_matches(&text, &matcher);
    assert_eq!(matches.len(), MAX_MATCHES);
    assert!(capped);
}

#[test]
fn replace_all_rewrites_every_match_in_one_text() {
    let text = "port: 1\nPort: 2\nexport: 3";
    let matcher = Matcher::new("port", PLAIN).unwrap().unwrap();
    let (result, count, first) = replace_all(text, &matcher, "listen").unwrap();
    assert_eq!(result, "listen: 1\nlisten: 2\nexlisten: 3");
    assert_eq!(count, 3);
    assert_eq!(first, 0..6);
    // Every match is replaced, past the highlight cap too.
    let many = "a ".repeat(MAX_MATCHES + 5);
    let (result, count, _) = replace_all(&many, &matcher_for("a"), "b").unwrap();
    assert_eq!(count, MAX_MATCHES + 5);
    assert!(!result.contains('a'));
    // No match, no edit.
    assert!(replace_all(text, &matcher_for("missing"), "x").is_none());
}

fn matcher_for(query: &str) -> Matcher {
    Matcher::new(query, PLAIN).unwrap().unwrap()
}

#[test]
fn replace_all_is_not_fooled_by_a_replacement_containing_the_query() {
    let matcher = matcher_for("a");
    let (result, count, _) = replace_all("a a", &matcher, "aa").unwrap();
    assert_eq!((result.as_str(), count), ("aa aa", 2));
}

#[test]
fn regex_replacements_expand_groups_and_escapes() {
    let regex = options(false, false, true);
    let matcher = Matcher::new(r"(\w+): (\d+)", regex).unwrap().unwrap();
    let (result, _, _) = replace_all("a: 1\nb: 2", &matcher, "$2 = ${1}").unwrap();
    assert_eq!(result, "1 = a\n2 = b");
    let matcher = Matcher::new(r"(?P<key>\w+),", regex).unwrap().unwrap();
    let (result, _, _) = replace_all("a,b,", &matcher, r"${key}\n").unwrap();
    assert_eq!(result, "a\nb\n");
    let (result, _, _) = replace_all("a,", &matcher, r"\t$$\\").unwrap();
    assert_eq!(result, "\t$\\");
    // Plain text replaces literally.
    let (result, _, _) = replace_all("x", &matcher_for("x"), r"$1\n").unwrap();
    assert_eq!(result, r"$1\n");
}

#[test]
fn replace_one_reports_where_the_replacement_went() {
    let text = "a: 1\nb: 1";
    let matcher = matcher_for("1");
    let (result, range) = replace_one(text, &matcher, 8..9, "22");
    assert_eq!(result, "a: 1\nb: 22");
    assert_eq!(range, 8..10);
}

#[test]
fn the_count_reads_like_vs_code() {
    let matcher = matcher_for("a");
    let (matches, capped) = find_matches("a a a", &matcher);
    let results = SearchResults {
        matches,
        capped,
        error: None,
    };
    let strings = crate::EditorStrings::default();
    assert_eq!(count_label(&strings, &results, Some(1), false), "2 of 3");
    assert_eq!(count_label(&strings, &results, None, false), "? of 3");
    // A stale index (the document changed) is not shown.
    assert_eq!(count_label(&strings, &results, Some(7), false), "? of 3");
    assert_eq!(
        count_label(&strings, &SearchResults::default(), None, false),
        "No results"
    );
    assert_eq!(
        count_label(&strings, &SearchResults::default(), None, true),
        "No results"
    );
    let invalid = SearchResults {
        error: Some("unclosed group".into()),
        ..SearchResults::default()
    };
    assert_eq!(
        count_label(&strings, &invalid, None, false),
        "Invalid pattern"
    );
    let many = "a ".repeat(MAX_MATCHES + 1);
    let (matches, capped) = find_matches(&many, &matcher);
    let results = SearchResults {
        matches,
        capped,
        error: None,
    };
    assert_eq!(
        count_label(&strings, &results, Some(0), false),
        "1 of 10000+"
    );
}
