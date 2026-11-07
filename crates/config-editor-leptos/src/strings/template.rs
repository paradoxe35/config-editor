//! Filling the placeholders of a text (`{count}`), choosing between the
//! singular and plural texts of a count, and checking a host's texts use
//! only the placeholders their defaults have.

use std::fmt::{self, Display};

use super::EditorStrings;

/// A placeholder a text uses that its default does not have, so it would
/// be shown as written: from [`EditorStrings::check`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringsProblem {
    /// The text's key, as JSON spells it (`errorsOne`).
    pub key: String,
    /// The unknown placeholder, without its braces.
    pub placeholder: String,
    /// The placeholders the text takes.
    pub allowed: Vec<String>,
}

impl Display for StringsProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "strings.{}: unknown placeholder {{{}}} ",
            self.key, self.placeholder
        )?;
        if self.allowed.is_empty() {
            write!(formatter, "(this text takes none)")
        } else {
            let names: Vec<String> = self
                .allowed
                .iter()
                .map(|name| format!("{{{name}}}"))
                .collect();
            write!(formatter, "(this text takes {})", names.join(", "))
        }
    }
}

/// The placeholder names in `template`, in order: `{name}` with a name of
/// lowercase letters and underscores. Other braces are plain text.
pub(crate) fn placeholders(template: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let name_len = after
            .find(|character: char| !(character.is_ascii_lowercase() || character == '_'))
            .unwrap_or(after.len());
        if name_len > 0 && after[name_len..].starts_with('}') {
            found.push(&after[..name_len]);
            rest = &after[name_len + 1..];
        } else {
            rest = after;
        }
    }
    found
}

/// `template` with each `{name}` of `values` replaced by its value; other
/// braces stay as they are.
pub(crate) fn fill(template: &str, values: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let value = after.find('}').and_then(|close| {
            let name = &after[..close];
            values
                .iter()
                .find(|(known, _)| *known == name)
                .map(|(_, value)| (close, value))
        });
        match value {
            Some((close, value)) => {
                out.push_str(&value.to_string());
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `one` when `count` is 1, else `other`, with `{count}` filled.
pub(crate) fn counted(one: &str, other: &str, count: usize) -> String {
    counted_as(one, other, count, &count)
}

/// The same with `{count}` written as `shown` (`20+`).
pub(crate) fn counted_as(one: &str, other: &str, count: usize, shown: &dyn Display) -> String {
    let template = if count == 1 { one } else { other };
    fill(template, &[("count", shown)])
}

impl EditorStrings {
    /// The placeholders used that their text's default does not have
    /// (`{cout}` for `{count}`): each would be shown as written. Empty when
    /// every text is fine. A text may leave out placeholders it takes.
    ///
    /// ```
    /// use config_editor_leptos::EditorStrings;
    ///
    /// let strings = EditorStrings {
    ///     errors_other: "{cout} Fehler".into(),
    ///     ..EditorStrings::default()
    /// };
    /// let problems = strings.check();
    /// assert_eq!(problems[0].key, "errorsOther");
    /// assert_eq!(
    ///     problems[0].to_string(),
    ///     "strings.errorsOther: unknown placeholder {cout} (this text takes {count})"
    /// );
    /// ```
    pub fn check(&self) -> Vec<StringsProblem> {
        let texts = |strings: &EditorStrings| match serde_json::to_value(strings) {
            Ok(serde_json::Value::Object(texts)) => texts,
            _ => serde_json::Map::new(),
        };
        let defaults = texts(&EditorStrings::default());
        let mut problems = Vec::new();
        for (key, text) in texts(self) {
            let allowed: Vec<&str> = defaults
                .get(&key)
                .and_then(serde_json::Value::as_str)
                .map(placeholders)
                .unwrap_or_default();
            for placeholder in placeholders(text.as_str().unwrap_or_default()) {
                if !allowed.contains(&placeholder) {
                    problems.push(StringsProblem {
                        key: key.clone(),
                        placeholder: placeholder.to_string(),
                        allowed: allowed.iter().map(|name| name.to_string()).collect(),
                    });
                }
            }
        }
        problems
    }
}
