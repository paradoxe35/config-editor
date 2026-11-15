//! The editor's core is language-agnostic: code outside
//! `src/editor/languages/` never names a built-in language, nor writes one
//! of their syntax's tokens (`</`, `[[`, `---`, …). What differs by syntax
//! is asked of the `ConfigLanguage` trait (add a hook with a default when
//! none fits), so a custom language gets every feature too. Tests, doc
//! comments and the demo (`main.rs`) may name languages.

mod source_scan;

use std::fs;
use std::path::{Path, PathBuf};

use source_scan::{rust_files, test_cfgs, test_files, test_module};

const LANGUAGES: [&str; 5] = ["YAML", "JSON", "TOML", "HCL", "XML"];

/// Tokens of one built-in language's syntax: markup (`</`, `<?`, `<!`,
/// `/>`, comments, XML's `#text`), TOML's array tables and long strings,
/// YAML's document marker and item dash, HCL's heredoc. A string literal
/// that is one of them in core code is that language's syntax built in.
const SYNTAX_TOKENS: [&str; 15] = [
    "</",
    "<?",
    "<!",
    "/>",
    "<!--",
    "-->",
    "[[",
    "]]",
    "---",
    "\"\"\"",
    "'''",
    "<<",
    "- ",
    "#text",
    "<![CDATA[",
];

/// The lines of `file` (before its first inline test module, without
/// comments) that name a built-in language.
fn naming_lines(file: &Path) -> Vec<(usize, String)> {
    let source = fs::read_to_string(file).unwrap();
    let end = test_cfgs(&source)
        .find(|(_, rest)| matches!(test_module(rest), Some((_, true))))
        .map_or(source.len(), |(at, _)| at);
    source[..end]
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let code = line.split("//").next().unwrap();
            LANGUAGES
                .iter()
                .any(|name| {
                    code.match_indices(&format!("Language::{name}"))
                        .any(|(at, matched)| {
                            !code[at + matched.len()..]
                                .starts_with(|c: char| c.is_alphanumeric() || c == '_')
                        })
                })
                .then(|| (index + 1, line.trim().to_string()))
        })
        .collect()
}

/// The string literals of `code` (one line) before its comment, as
/// written between their quotes.
fn string_literals(code: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut start = None;
    let mut escaped = false;
    for (index, character) in code.char_indices() {
        match start {
            Some(_) if escaped => escaped = false,
            Some(_) if character == '\\' => escaped = true,
            Some(from) if character == '"' => {
                found.push(&code[from..index]);
                start = None;
            }
            Some(_) => {}
            // A char literal `'"'` holds no string.
            None if character == '"' && !code[..index].ends_with('\'') => start = Some(index + 1),
            // A comment ends the code.
            None if code[index..].starts_with("//") => break,
            None => {}
        }
    }
    found
}

/// The lines of `file` (before its first inline test module, without
/// comments) with a string literal that is a built-in syntax token.
fn syntax_lines(file: &Path) -> Vec<(usize, String)> {
    let source = fs::read_to_string(file).unwrap();
    let end = test_cfgs(&source)
        .find(|(_, rest)| matches!(test_module(rest), Some((_, true))))
        .map_or(source.len(), |(at, _)| at);
    source[..end]
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .filter(|(_, line)| {
            string_literals(line).iter().any(|literal| {
                let text = literal.replace("\\\"", "\"");
                SYNTAX_TOKENS.contains(&text.as_str())
            })
        })
        .map(|(index, line)| (index + 1, line.trim().to_string()))
        .collect()
}

/// The core files: outside `languages/`, the demo and test modules.
fn core_files(src: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    rust_files(src, &mut files);
    let tests = test_files(&files);
    let languages = src.join("editor").join("languages");
    files
        .into_iter()
        .filter(|file| {
            !file.starts_with(&languages)
                && *file != src.join("main.rs")
                && !tests.contains(file)
                && !file.ancestors().any(|dir| tests.contains(dir))
        })
        .collect()
}

fn syntax_offenders(src: &Path) -> Vec<String> {
    core_files(src)
        .iter()
        .flat_map(|file| {
            syntax_lines(file).into_iter().map(move |(line, text)| {
                format!(
                    "{}:{line}: {text}",
                    file.strip_prefix(src).unwrap().display()
                )
            })
        })
        .collect()
}

fn offenders(src: &Path) -> Vec<String> {
    let mut files = Vec::new();
    rust_files(src, &mut files);
    let tests = test_files(&files);
    let languages = src.join("editor").join("languages");
    files
        .iter()
        .filter(|file| {
            !file.starts_with(&languages)
                && *file != &src.join("main.rs")
                && !tests.contains(*file)
                && !file.ancestors().any(|dir| tests.contains(dir))
        })
        .flat_map(|file| {
            naming_lines(file).into_iter().map(move |(line, text)| {
                format!(
                    "{}:{line}: {text}",
                    file.strip_prefix(src).unwrap().display()
                )
            })
        })
        .collect()
}

#[test]
fn the_core_never_names_a_built_in_language() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let found = offenders(&src);
    assert!(
        found.is_empty(),
        "the core names a language; ask the ConfigLanguage trait instead \
         (a hook with a default, mapped in e2e/tests/api-parity.spec.mjs):\n{}",
        found.join("\n")
    );
}

/// The check itself catches a core file that names a language, and lets
/// test modules and comments through.
#[test]
fn the_check_catches_a_named_language() {
    let dir = std::env::temp_dir().join(format!("agnostic-core-{}", std::process::id()));
    let editor = dir.join("editor");
    fs::create_dir_all(editor.join("languages")).unwrap();
    fs::write(
        dir.join("lib.rs"),
        "#[cfg(test)]\nmod checks;\nfn f() {} // Language::YAML in a comment\n",
    )
    .unwrap();
    fs::write(dir.join("checks.rs"), "fn t() { Language::JSON; }\n").unwrap();
    fs::write(
        editor.join("hover.rs"),
        "fn g() { if language == Language::YAML { } }\n#[cfg(all(test, feature = \"x\"))]\nmod tests { fn h() { Language::TOML; } }\n",
    )
    .unwrap();
    fs::write(
        editor.join("languages").join("yaml.rs"),
        "fn y() { Language::YAML; }\n",
    )
    .unwrap();
    let found = offenders(&dir);
    fs::remove_dir_all(&dir).unwrap();
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].starts_with("editor/hover.rs:1:"), "{found:#?}");
}

#[test]
fn the_core_writes_no_built_in_syntax() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let found = syntax_offenders(&src);
    assert!(
        found.is_empty(),
        "the core writes one language's syntax; ask the ConfigLanguage trait \
         (its config, or a hook with a default that a language overrides):\n{}",
        found.join("\n")
    );
}

/// The syntax check catches a token in a core file, and lets languages,
/// test modules, comments and longer strings through.
#[test]
fn the_check_catches_a_syntax_token() {
    let dir = std::env::temp_dir().join(format!("agnostic-syntax-{}", std::process::id()));
    let editor = dir.join("editor");
    fs::create_dir_all(editor.join("languages")).unwrap();
    fs::write(
        dir.join("lib.rs"),
        "fn f() { g(\"--- expected\"); } // \"</\" in a comment\n#[cfg(test)]\nmod tests { fn t() { \"[[\"; } }\n",
    )
    .unwrap();
    fs::write(
        editor.join("codec.rs"),
        "fn g(line: &str) -> bool {\n    line.starts_with(\"</\") || line == \"x\"\n}\n",
    )
    .unwrap();
    fs::write(
        editor.join("languages").join("xml.rs"),
        "fn y(line: &str) -> bool { line.starts_with(\"</\") }\n",
    )
    .unwrap();
    let found = syntax_offenders(&dir);
    fs::remove_dir_all(&dir).unwrap();
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].starts_with("editor/codec.rs:2:"), "{found:#?}");
}
