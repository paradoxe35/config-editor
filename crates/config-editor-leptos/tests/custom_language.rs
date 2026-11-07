//! An external language built only on the public API: `.env` files
//! (`KEY=value` lines). It gets schema validation with positions, highlighting,
//! and conversion to and from every built-in language.

use config_editor_leptos::Language;
#[cfg(feature = "schema-validation")]
use config_editor_leptos::Schema;
use config_editor_leptos::language::{
    ConfigLanguage, LanguageInfo, Lexical, ParseError, Span, Token, TokenKind, Tokens,
};
use serde_json::{Map, Value};

struct DotEnv;

impl DotEnv {
    /// `(line, key_start, key_end, value_start, value_end)` for each entry,
    /// in byte offsets within its line.
    fn entries(source: &str) -> impl Iterator<Item = (usize, &str, usize, usize, usize)> {
        source.lines().enumerate().filter_map(|(index, line)| {
            let equals = line.find('=')?;
            let key = line[..equals].trim();
            let key_start = line.find(key)?;
            Some((index + 1, key, key_start, equals + 1, line.len()))
        })
    }
}

impl ConfigLanguage for DotEnv {
    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: "ENV",
            icon: "E",
            extensions: &["env"],
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        let mut map = Map::new();
        for (index, line) in source.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(ParseError {
                    message: "expected KEY=value".to_string(),
                    line: Some(index + 1),
                    column: Some(1),
                });
            };
            let value = value.trim();
            let value = value
                .parse::<i64>()
                .map(Value::from)
                .unwrap_or_else(|_| Value::String(value.to_string()));
            map.insert(key.trim().to_string(), value);
        }
        Ok(Value::Object(map))
    }

    fn serialize(&self, value: &Value, _previous: Option<&str>) -> Result<String, String> {
        let Value::Object(map) = value else {
            return Err("an env file is a set of KEY=value pairs".to_string());
        };
        let mut text = String::new();
        for (key, value) in map {
            let value = match value {
                Value::String(text) => text.clone(),
                Value::Number(_) | Value::Bool(_) => value.to_string(),
                _ => return Err(format!("{key}: env values are scalars")),
            };
            text.push_str(&format!("{key}={value}\n"));
        }
        Ok(text)
    }

    fn key_span(&self, source: &str, pointer: &str) -> Option<Span> {
        let wanted = pointer.strip_prefix('/')?;
        Self::entries(source)
            .find(|(_, key, ..)| *key == wanted)
            .map(|(line, key, start, ..)| Span {
                line,
                start_column: start + 1,
                end_column: start + key.len() + 1,
            })
    }

    fn value_span(&self, source: &str, pointer: &str) -> Option<Span> {
        let wanted = pointer.strip_prefix('/')?;
        Self::entries(source)
            .find(|(_, key, ..)| *key == wanted)
            .map(|(line, _, _, start, end)| Span {
                line,
                start_column: start + 1,
                end_column: end + 1,
            })
    }

    fn highlight(&self, line: &str) -> Vec<Token> {
        let mut tokens = Tokens::new();
        if line.trim_start().starts_with('#') {
            tokens.push(0, line.len(), TokenKind::Comment);
        } else if let Some(equals) = line.find('=') {
            tokens.push(0, equals, TokenKind::Key);
            tokens.push(equals, equals + 1, TokenKind::Punctuation);
            tokens.push(equals + 1, line.len(), TokenKind::Value);
        }
        tokens.finish()
    }

    fn lexical_state(&self, source: &str, cursor: usize) -> Lexical {
        let line = source[..cursor.min(source.len())]
            .rsplit('\n')
            .next()
            .unwrap_or_default();
        Lexical {
            quote: None,
            in_comment: line.trim_start().starts_with('#'),
        }
    }
}

static DOTENV: DotEnv = DotEnv;
const ENV: Language = Language::new(&DOTENV);

#[cfg(feature = "schema-validation")]
const SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "PORT": {"type": "integer", "description": "Listen port"},
        "HOST": {"type": "string"}
    },
    "additionalProperties": false
}"#;

#[test]
fn an_external_language_is_a_first_class_language_handle() {
    assert_eq!(ENV.label(), "ENV");
    assert_eq!(ENV.file_icon(), "E");
    assert_eq!(ENV.default_file_name(), "untitled.env");
    assert_eq!(ENV, Language::new(&DOTENV));
    assert_eq!(format!("{ENV:?}"), "Language(\"ENV\")");
    // Built-in lookups stay built-in only.
    assert!(!Language::ALL.contains(&ENV));
    assert!("env".parse::<Language>().is_err());
}

#[test]
fn an_external_language_parses_and_reports_syntax_errors() {
    let value = ENV
        .parse("# comment\nPORT=7835\nHOST=example.com\n")
        .unwrap();
    assert_eq!(
        value,
        serde_json::json!({"PORT": 7835, "HOST": "example.com"})
    );
    let error = ENV.parse("PORT=1\nbroken\n").unwrap_err();
    assert_eq!((error.line, error.column), (Some(2), Some(1)));
}

#[cfg(feature = "schema-validation")]
#[test]
fn schema_problems_point_at_the_external_languages_spans() {
    let schema = Schema::compile(SCHEMA).unwrap();
    assert!(schema.validate(ENV, "PORT=7835\n").is_empty());
    let problems = schema.validate(ENV, "HOST=a\nPORT=high\nEXTRA=1\n");
    let mut places: Vec<(Option<usize>, Option<usize>)> = problems
        .iter()
        .map(|problem| (problem.line, problem.column))
        .collect();
    places.sort();
    // The wrong value on its value, the unknown key on its key.
    assert_eq!(places, vec![(Some(2), Some(6)), (Some(3), Some(1))]);
}

#[cfg(all(feature = "yaml", feature = "toml", feature = "json"))]
#[test]
fn an_external_language_converts_to_and_from_the_built_in_ones() {
    let env = "PORT=7835\nHOST=example.com\n";
    let yaml = ENV.convert(env, Language::YAML).unwrap();
    assert_eq!(yaml.text, "PORT: 7835\nHOST: example.com\n");
    assert!(yaml.losses.is_empty());
    let back = Language::TOML
        .convert("PORT = 9000\nHOST = \"b\"\n", ENV)
        .unwrap();
    assert_eq!(back.text, "PORT=9000\nHOST=b\n");
    // What the external language cannot express is an error, not data loss.
    assert!(
        Language::JSON
            .convert(r#"{"NESTED": {"a": 1}}"#, ENV)
            .is_err()
    );
}

#[test]
fn highlighting_is_plain_data() {
    let tokens = ENV.parse("A=1").map(|_| DOTENV.highlight("A=1")).unwrap();
    let kinds: Vec<TokenKind> = tokens.iter().map(|token| token.kind).collect();
    assert_eq!(
        kinds,
        [TokenKind::Key, TokenKind::Punctuation, TokenKind::Value]
    );
}

/// A language described only by data: `info`, `parse` and `config`.
struct Ini;

impl ConfigLanguage for Ini {
    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: "INI",
            icon: "I",
            extensions: &["ini"],
        }
    }

    fn config(&self) -> config_editor_leptos::language::LanguageConfig {
        config_editor_leptos::language::LanguageConfig {
            line_comments: &[";", "#"],
            block_comment: None,
            brackets: &[('[', ']')],
            separators: &["="],
            quotes: &["\""],
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        let mut map = Map::new();
        for line in source.lines() {
            if let Some((key, value)) = line.split_once('=') {
                map.insert(
                    key.trim().to_string(),
                    Value::String(value.trim().to_string()),
                );
            }
        }
        Ok(Value::Object(map))
    }
}

#[test]
fn a_language_described_by_config_gets_editing_without_code() {
    let ini = Ini;
    // Ctrl+/ writes the first comment marker; comments are found on lines.
    let (commented, _, _) = ini.toggle_comments("a = 1\n", 0, 0).unwrap();
    assert_eq!(commented, "; a = 1\n");
    assert_eq!(ini.comment_start("a = 1 # note"), Some(6));
    // Enter between brackets opens an indented line.
    assert_eq!(ini.newline("hosts = []", 9).text, "\n  \n");
    // Key/value behavior from the separator.
    assert!(ini.value_follows_key(" = 1"));
    assert!(!ini.value_follows_key(" == 1"));
    assert!(ini.line_has_value("debug = true"));
    assert!(!ini.line_has_value("debug ="));
    assert_eq!(ini.empty_value_slots("a = \nb = 1\n"), vec![4]);
}

/// Folding comes with the default: indentation, `config`'s brackets and
/// comment runs, for a language with no folding code.
#[cfg(feature = "folding")]
#[test]
fn a_language_without_folding_code_folds_by_its_config() {
    use config_editor_leptos::language::FoldRange;
    let ranges = Ini.fold_ranges("; a\n; b\n[web]\nhosts = [\n  \"a\",\n]\nlisten = 1");
    assert_eq!(ranges, [FoldRange::new(0, 1), FoldRange::new(3, 4)]);
    // No config at all: indentation alone.
    assert_eq!(
        Language::new(&DOTENV).fold_ranges("A=1\nB=\n  continued\n"),
        [FoldRange::new(1, 2)]
    );
}

/// A language declaring its long quote gets Ctrl+/ over a string written
/// over lines whole, without code of its own.
struct LongStrings;

impl ConfigLanguage for LongStrings {
    fn info(&self) -> config_editor_leptos::language::LanguageInfo {
        config_editor_leptos::language::LanguageInfo {
            label: "LS",
            icon: "L",
            extensions: &[],
        }
    }

    fn parse(
        &self,
        _source: &str,
    ) -> Result<serde_json::Value, config_editor_leptos::language::ParseError> {
        Ok(serde_json::json!({}))
    }

    fn config(&self) -> config_editor_leptos::language::LanguageConfig {
        config_editor_leptos::language::LanguageConfig {
            line_comments: &[";"],
            block_comment: None,
            brackets: &[],
            separators: &["="],
            quotes: &["\"\"\"", "\""],
        }
    }
}

#[test]
fn a_language_declaring_its_quotes_gets_whole_strings_commented() {
    let text = "a = 1\nb = \"\"\"x\ny\"\"\"\nc = 2\n";
    let caret = text.find("y\"").unwrap();
    let (commented, _, _) = LongStrings.toggle_comments(text, caret, caret).unwrap();
    assert_eq!(commented, "a = 1\n; b = \"\"\"x\n; y\"\"\"\nc = 2\n");
}
