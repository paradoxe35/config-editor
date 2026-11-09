//! The odd language's reader: the value of a document and where each key
//! and value of it is written.
//!
//! ```text
//! ; a comment
//! name := web            ; after a value too
//! ports := (80 | 443)
//! tls := <cert := a.pem | on := yes>
//! hosts :=
//!   @ alpha
//!   @
//!     name := beta
//! ```

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::editor::language::{ParseError, Span};

/// A document read: its value and the spans of its keys and values.
pub(super) struct Read {
    pub value: Value,
    pub keys: HashMap<String, Span>,
    pub values: HashMap<String, Span>,
}

/// One line with code: its number (one-based), its indent, its code
/// without the comment, and the text of the whole line.
struct Line<'a> {
    number: usize,
    indent: usize,
    code: &'a str,
    text: &'a str,
}

/// Where the line's comment starts: a `;` outside a `"…"` string, at the
/// line start or after whitespace.
pub(super) fn comment_start(line: &str) -> Option<usize> {
    let mut quoted = false;
    let mut escaped = false;
    let mut previous = ' ';
    for (index, character) in line.char_indices() {
        if quoted {
            match character {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => quoted = false,
                _ => {}
            }
        } else if character == '"' {
            quoted = true;
        } else if character == ';' && previous.is_whitespace() {
            return Some(index);
        }
        previous = character;
    }
    None
}

fn error(message: impl Into<String>, line: usize, column: usize) -> ParseError {
    ParseError {
        message: message.into(),
        line: Some(line),
        column: Some(column),
    }
}

fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

pub(super) fn read(source: &str) -> Result<Read, ParseError> {
    let lines: Vec<Line<'_>> = source
        .split('\n')
        .enumerate()
        .filter_map(|(index, text)| {
            let text = text.strip_suffix('\r').unwrap_or(text);
            let code = text[..comment_start(text).unwrap_or(text.len())].trim_end();
            let indent = code.len() - code.trim_start().len();
            (!code.trim().is_empty()).then_some(Line {
                number: index + 1,
                indent,
                code,
                text,
            })
        })
        .collect();
    let mut reader = Reader {
        lines,
        at: 0,
        keys: HashMap::new(),
        values: HashMap::new(),
    };
    let value = if reader.lines.is_empty() {
        Value::Object(Map::new())
    } else {
        if reader.lines[0].indent != 0 {
            let line = &reader.lines[0];
            return Err(error("the document starts indented", line.number, 1));
        }
        let value = reader.block(0, "")?;
        if let Some(line) = reader.lines.get(reader.at) {
            return Err(error("unexpected indent", line.number, line.indent + 1));
        }
        value
    };
    if !value.is_object() {
        return Err(error("a document is a map of `key := value` entries", 1, 1));
    }
    Ok(Read {
        value,
        keys: reader.keys,
        values: reader.values,
    })
}

struct Reader<'a> {
    lines: Vec<Line<'a>>,
    at: usize,
    keys: HashMap<String, Span>,
    values: HashMap<String, Span>,
}

impl Reader<'_> {
    fn span(&self, line: usize, from: usize, to: usize) -> Span {
        let text = self.lines[line].text;
        Span {
            line: self.lines[line].number,
            start_column: text[..from].chars().count() + 1,
            end_column: text[..to].chars().count() + 1,
        }
    }

    /// The block (map or list) whose lines start at `indent`.
    fn block(&mut self, indent: usize, pointer: &str) -> Result<Value, ParseError> {
        let list = is_item(self.lines[self.at].code.trim_start());
        let mut map = Map::new();
        let mut items = Vec::new();
        while let Some(line) = self.lines.get(self.at) {
            if line.indent < indent {
                break;
            }
            let (number, code) = (line.number, line.code);
            if line.indent > indent {
                return Err(error("unexpected indent", number, line.indent + 1));
            }
            let rest = &code[indent..];
            if is_item(rest) != list {
                return Err(error("items and entries mixed", number, indent + 1));
            }
            let (child, value_from) = if list {
                (format!("{pointer}/{}", items.len()), indent + 1)
            } else {
                let (key, key_end) = key_of(rest)
                    .ok_or_else(|| error("expected `key := value`", number, indent + 1))?;
                let after = rest[key_end..].trim_start();
                let Some(after) = after.strip_prefix(":=") else {
                    return Err(error("expected `:=`", number, indent + key_end + 1));
                };
                let child = format!("{pointer}/{}", escape(&key));
                let span = self.span(self.at, indent, indent + key_end);
                self.keys.insert(child.clone(), span);
                if map.contains_key(&key) {
                    return Err(error(format!("`{key}` twice"), number, indent + 1));
                }
                map.insert(key, Value::Null);
                (child, code.len() - after.len())
            };
            let value_text = &code[value_from..];
            let value_start = value_from + (value_text.len() - value_text.trim_start().len());
            let value = if value_text.trim().is_empty() {
                // The value is the block below.
                let line = self.at;
                self.at += 1;
                match self.lines.get(self.at) {
                    Some(next) if next.indent > indent => {
                        let deeper = next.indent;
                        let first = self.at;
                        let first_code = self.lines[first].code;
                        let span = self.span(first, deeper, first_code.len());
                        self.values.insert(child.clone(), span);
                        self.block(deeper, &child)?
                    }
                    _ => return Err(error("a value is missing", self.lines[line].number, 1)),
                }
            } else {
                let line = self.at;
                let mut inline = Inline {
                    text: code,
                    at: value_start,
                    found: Vec::new(),
                };
                let value = inline.value(&child).map_err(|problem| ParseError {
                    line: Some(number),
                    ..problem
                })?;
                if inline.at < code.len() {
                    return Err(error("unexpected text", number, inline.at + 1));
                }
                for (pointer, from, to) in inline.found {
                    let span = self.span(line, from, to);
                    self.values.insert(pointer, span);
                }
                self.at += 1;
                value
            };
            if list {
                items.push(value);
            } else if let Some(last) = map.values_mut().last() {
                *last = value;
            }
        }
        Ok(if list {
            Value::Array(items)
        } else {
            Value::Object(map)
        })
    }
}

fn is_item(code: &str) -> bool {
    code == "@" || code.starts_with("@ ")
}

/// A key at the start of `text` and where it ends.
fn key_of(text: &str) -> Option<(String, usize)> {
    if text.starts_with('"') {
        let (value, end) = quoted(text, 0).ok()?;
        return Some((value, end));
    }
    let end = text
        .find(|character: char| !(character.is_alphanumeric() || matches!(character, '_' | '-')))
        .unwrap_or(text.len());
    (end > 0).then(|| (text[..end].to_string(), end))
}

/// The string quoted at `at` (its `"`) and the byte past it.
fn quoted(text: &str, at: usize) -> Result<(String, usize), usize> {
    let mut out = String::new();
    let mut characters = text[at + 1..].char_indices();
    while let Some((offset, character)) = characters.next() {
        match character {
            '"' => return Ok((out, at + 1 + offset + 1)),
            '\\' => match characters.next() {
                Some((_, 'n')) => out.push('\n'),
                Some((_, 't')) => out.push('\t'),
                Some((_, other)) => out.push(other),
                None => return Err(at),
            },
            other => out.push(other),
        }
    }
    Err(at)
}

/// A value written on one line, from `at`.
struct Inline<'a> {
    text: &'a str,
    at: usize,
    /// Each value read: its pointer and bytes.
    found: Vec<(String, usize, usize)>,
}

impl Inline<'_> {
    fn skip_space(&mut self) {
        let rest = &self.text[self.at..];
        self.at += rest.len() - rest.trim_start().len();
    }

    fn fail(&self, message: &str) -> ParseError {
        ParseError {
            message: message.to_string(),
            line: None,
            column: Some(self.at + 1),
        }
    }

    fn value(&mut self, pointer: &str) -> Result<Value, ParseError> {
        self.skip_space();
        let start = self.at;
        let rest = &self.text[self.at..];
        let value = if rest.starts_with('(') {
            self.at += 1;
            let mut items = Vec::new();
            self.skip_space();
            if !self.text[self.at..].starts_with(')') {
                loop {
                    items.push(self.value(&format!("{pointer}/{}", items.len()))?);
                    self.skip_space();
                    match self.text[self.at..].chars().next() {
                        Some('|') => self.at += 1,
                        Some(')') => break,
                        _ => return Err(self.fail("expected `|` or `)`")),
                    }
                }
            }
            self.at += 1;
            Value::Array(items)
        } else if rest.starts_with('<') {
            self.at += 1;
            let mut map = Map::new();
            self.skip_space();
            if !self.text[self.at..].starts_with('>') {
                loop {
                    self.skip_space();
                    let (key, end) =
                        key_of(&self.text[self.at..]).ok_or_else(|| self.fail("expected a key"))?;
                    self.at += end;
                    self.skip_space();
                    if !self.text[self.at..].starts_with(":=") {
                        return Err(self.fail("expected `:=`"));
                    }
                    self.at += 2;
                    let child = self.value(&format!("{pointer}/{}", escape(&key)))?;
                    map.insert(key, child);
                    self.skip_space();
                    match self.text[self.at..].chars().next() {
                        Some('|') => self.at += 1,
                        Some('>') => break,
                        _ => return Err(self.fail("expected `|` or `>`")),
                    }
                }
            }
            self.at += 1;
            Value::Object(map)
        } else if rest.starts_with('"') {
            let (text, end) =
                quoted(self.text, self.at).map_err(|_| self.fail("unclosed string"))?;
            self.at = end;
            Value::String(text)
        } else {
            let end = rest.find(['|', ')', '>']).unwrap_or(rest.len());
            let word = rest[..end].trim_end();
            if word.is_empty() {
                return Err(self.fail("a value is missing"));
            }
            self.at += word.len();
            scalar(word)
        };
        self.found.push((pointer.to_string(), start, self.at));
        Ok(value)
    }
}

/// A bare word as the value it writes.
pub(super) fn scalar(word: &str) -> Value {
    match word {
        "nil" => Value::Null,
        "yes" => Value::Bool(true),
        "no" => Value::Bool(false),
        _ => {
            if let Ok(integer) = word.parse::<i64>() {
                return Value::from(integer);
            }
            let float = word.contains('.')
                && word
                    .trim_start_matches('-')
                    .starts_with(|c: char| c.is_ascii_digit())
                && word
                    .chars()
                    .all(|c| c.is_ascii_digit() || matches!(c, '.' | '-'));
            match word.parse::<f64>() {
                Ok(number) if float => serde_json::Number::from_f64(number)
                    .map_or_else(|| Value::String(word.to_string()), Value::Number),
                _ => Value::String(word.to_string()),
            }
        }
    }
}
