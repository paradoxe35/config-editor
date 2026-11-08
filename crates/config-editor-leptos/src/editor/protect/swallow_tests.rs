//! A comment or a string opened inside a locked block, running over the
//! block's lines below it (an XML `<!--`, a YAML quote, a TOML `"""`, an
//! HCL heredoc on a line of the block), would hide them from the locks
//! while the text does not parse: such an edit is refused at once, in every
//! language. Opened above a locked value, outside any locked block, it is
//! typing as usual and goes through (the repair decides the value), and so
//! does one opened below the locked lines. JSON has neither.

use super::super::Language;
use super::super::paths::FieldPath;
use super::guard::check;
use super::locks::{Protected, Sources, protect};

fn locks(language: Language, text: &str, path: &str) -> Protected {
    let paths = [FieldPath::from(path)];
    protect(
        language.behavior(),
        text,
        Sources {
            paths: &paths,
            ..Sources::default()
        },
    )
}

/// The text, its locked line, and an opener put at the start of that line
/// (the rest of the text then inside it).
fn cases() -> Vec<(Language, &'static str, &'static str, &'static str)> {
    vec![
        (
            Language::YAML,
            "name: a\nlimits:\n  max: 10\n  burst: 5\nend: 1\n",
            "  max: 10",
            "  y: \"\n",
        ),
        (
            Language::TOML,
            "name = \"a\"\n[limits]\nmax = 10\nburst = 5\n",
            "max = 10",
            "y = \"\"\"\n",
        ),
        (
            Language::HCL,
            "name = \"a\"\nlimits {\n  max = 10\n  burst = 5\n}\n",
            "  max = 10",
            "  y = <<EOT\n",
        ),
        (
            Language::XML,
            "<name>a</name>\n<limits>\n  <max>10</max>\n  <burst>5</burst>\n</limits>\n",
            "  <max>10</max>",
            "<!--\n",
        ),
    ]
}

#[test]
fn opening_a_comment_or_string_inside_a_locked_block_is_refused() {
    let mut failures = Vec::new();
    for (language, text, line, opener) in cases() {
        for path in ["/limits/max", "/limits"] {
            let protected = locks(language, text, path);
            let at = text.find(line).unwrap();
            let opened = format!("{}{opener}{}", &text[..at], &text[at..]);
            let refused = check(language.behavior(), None, &protected, text, &opened).is_err();
            if refused != (path == "/limits") {
                failures.push(format!(
                    "{language} lock {path}: {opened:?} {}",
                    if refused { "refused" } else { "allowed" }
                ));
            }
            let below = format!("{text}{opener}");
            if path == "/limits/max"
                && check(language.behavior(), None, &protected, text, &below).is_err()
            {
                failures.push(format!(
                    "{language} lock {path}: {opener:?} at the end refused"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{failures:#?}",
        failures.len()
    );
}
