//! INI files as an external language, on the public API only: `key = value`
//! lines grouped under `[section]` headers. `config` describes the syntax as
//! data (comments, brackets, the separator), which gives editing; the
//! completion and hover methods place the caret in the structure, which is
//! all schema completion and hover docs need.

use config_editor_leptos::language::{
    CompletionContext, ConfigLanguage, InsertContext, LanguageConfig, LanguageInfo, ParseError,
    SchemaPathSegment, object_branch, utf16_to_byte_index,
};
use serde_json::{Map, Value};
use std::collections::HashSet;

pub struct Ini;

impl Ini {
    /// The section name of a `[section]` header line.
    fn header(line: &str) -> Option<&str> {
        let (name, _) = line.trim().strip_prefix('[')?.split_once(']')?;
        Some(name.trim())
    }

    fn is_comment(line: &str) -> bool {
        let line = line.trim_start();
        line.starts_with(';') || line.starts_with('#')
    }

    /// A value as data: booleans and integers, else the text as written.
    fn scalar(text: &str) -> Value {
        match text {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => text
                .parse::<i64>()
                .map(Value::from)
                .unwrap_or_else(|_| Value::String(text.to_string())),
        }
    }
}

impl ConfigLanguage for Ini {
    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: "INI",
            icon: "I",
            extensions: &["ini"],
        }
    }

    fn config(&self) -> LanguageConfig {
        LanguageConfig {
            line_comments: &[";", "#"],
            block_comment: None,
            brackets: &[('[', ']')],
            separators: &["="],
            quotes: &["\""],
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        let mut root = Map::new();
        let mut section: Option<String> = None;
        for line in source.lines() {
            if let Some(name) = Self::header(line) {
                root.insert(name.to_string(), Value::Object(Map::new()));
                section = Some(name.to_string());
            } else if let Some((key, value)) = line.split_once('=')
                && !Self::is_comment(line)
            {
                let entries = match &section {
                    Some(name) => root[name].as_object_mut().expect("a section"),
                    None => &mut root,
                };
                entries.insert(key.trim().to_string(), Self::scalar(value.trim()));
            }
        }
        Ok(Value::Object(root))
    }

    /// Inside a section the path is the section; after `=` the caret is in
    /// that key's value; anywhere else a key is being typed.
    fn completion_context(&self, source: &str, cursor: usize) -> CompletionContext {
        let byte = utf16_to_byte_index(source, cursor);
        let line_start = source[..byte].rfind('\n').map_or(0, |at| at + 1);
        let line_end = source[byte..]
            .find('\n')
            .map_or(source.len(), |at| byte + at);
        let utf16 = |byte: usize| source[..byte].encode_utf16().count();
        let path: Vec<SchemaPathSegment> = source[..line_end]
            .lines()
            .rev()
            .find_map(Self::header)
            .map(|name| vec![SchemaPathSegment::Key(name.to_string())])
            .unwrap_or_default();
        let before = &source[line_start..byte];
        let indent = before.len() - before.trim_start().len();
        let base_indent = before[..indent].to_string();
        if before.trim_start().starts_with('[') || Self::is_comment(before) {
            // A header names its section; it holds no entry.
            return CompletionContext {
                path,
                no_properties: true,
                ..Default::default()
            };
        }
        if let Some(equals) = before.find('=') {
            let after = &before[equals + 1..];
            let start = equals + 1 + (after.len() - after.trim_start().len());
            return CompletionContext {
                path,
                typed: before[start..].to_string(),
                replace_start: utf16(line_start + start),
                replace_end: cursor,
                value_key: Some(before[..equals].trim().to_string()),
                value_needs_space: after.is_empty(),
                base_indent,
                ..Default::default()
            };
        }
        CompletionContext {
            path,
            typed: before[indent..].to_string(),
            replace_start: utf16(line_start + indent),
            replace_end: cursor,
            base_indent,
            ..Default::default()
        }
    }

    /// The keys of the section at `path`; the top level's include the
    /// section names.
    fn existing_keys(
        &self,
        source: &str,
        cursor_byte: usize,
        path: &[SchemaPathSegment],
    ) -> Option<HashSet<String>> {
        let wanted = match path {
            [] => None,
            [SchemaPathSegment::Key(name)] => Some(name.as_str()),
            _ => return None,
        };
        let caret_line = source[..cursor_byte].matches('\n').count();
        let mut section = None;
        let mut keys = HashSet::new();
        for (index, line) in source.lines().enumerate() {
            if index == caret_line || Self::is_comment(line) {
                continue;
            }
            if let Some(name) = Self::header(line) {
                if wanted.is_none() {
                    keys.insert(name.to_string());
                }
                section = Some(name);
            } else if section == wanted
                && let Some((key, _)) = line.split_once('=')
            {
                keys.insert(key.trim().to_string());
            }
        }
        Some(keys)
    }

    /// `key = ` with the caret after it; an object is a `[section]`, which
    /// only the top level holds.
    fn property_insert(
        &self,
        key: &str,
        property: &Value,
        root: &Value,
        context: &InsertContext<'_>,
    ) -> Option<(String, usize)> {
        let text = if object_branch(property, root).is_some() {
            if !context.path.is_empty() {
                return None;
            }
            format!("[{key}]\n")
        } else {
            format!("{key} = ")
        };
        let caret = text.encode_utf16().count();
        Some((text, caret))
    }

    /// Values are written bare: `mode = fast`.
    fn format_value(&self, value: &Value) -> String {
        match value {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        }
    }

    /// The key of a `key = value` line, or a header's section name.
    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        let key = match Self::header(line) {
            Some(name) => name,
            None if Self::is_comment(line) => return None,
            None => line.split_once('=')?.0.trim(),
        };
        let start = line[..line.find(key)?].chars().count();
        (start..start + key.chars().count())
            .contains(&column)
            .then(|| (key.to_string(), start))
    }
}
