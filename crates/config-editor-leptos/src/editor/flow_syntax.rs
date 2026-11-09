//! Values written between brackets (`[a, b]`, `{k: v}`, or any other
//! brackets a language names), read as the language's
//! [`LanguageConfig`] describes them: its map and list brackets, what
//! separates their items, what separates a key from its value, its quotes
//! and its line comments. Nothing here knows a language's characters.

use super::language::{ConfigLanguage, LanguageConfig, SchemaPathSegment};

/// A language's syntax for values written between brackets.
#[derive(Clone, Debug)]
pub(crate) struct Flow {
    config: LanguageConfig,
    /// The first character of each quote, and whether a backslash escapes
    /// inside it.
    quotes: Vec<(char, bool)>,
}

impl Flow {
    pub(crate) fn of(language: &(impl ConfigLanguage + ?Sized)) -> Self {
        Self::from_config(language.config())
    }

    pub(crate) fn from_config(config: LanguageConfig) -> Self {
        let mut quotes: Vec<(char, bool)> = Vec::new();
        for quote in config.quotes {
            let Some(first) = quote.chars().next() else {
                continue;
            };
            if !quotes.iter().any(|(known, _)| *known == first) {
                let literal = config
                    .literal_quotes
                    .iter()
                    .any(|literal| literal.starts_with(first));
                quotes.push((first, !literal));
            }
        }
        Flow { config, quotes }
    }

    /// The map brackets and the list brackets, those the language has.
    fn pairs(&self) -> impl Iterator<Item = (char, char)> + '_ {
        self.config
            .map_brackets
            .into_iter()
            .chain(self.config.list_brackets)
    }

    /// The closer of `open` when it opens a map or a list.
    pub(crate) fn closer_of(&self, open: char) -> Option<char> {
        self.pairs()
            .find_map(|(opener, close)| (opener == open).then_some(close))
    }

    /// The opener of what `close` closes, a map or a list.
    pub(crate) fn opener_of(&self, close: char) -> Option<char> {
        self.pairs()
            .find_map(|(open, closer)| (closer == close).then_some(open))
    }

    pub(crate) fn map(&self) -> Option<(char, char)> {
        self.config.map_brackets
    }

    pub(crate) fn list(&self) -> Option<(char, char)> {
        self.config.list_brackets
    }

    /// The text after the item marker `code` starts with (YAML's `- `),
    /// with the spacing after it; `None` when it starts no item.
    pub(crate) fn after_item_marker<'a>(&self, code: &'a str) -> Option<&'a str> {
        let rest = code.strip_prefix(self.config.item_marker?)?;
        (rest.is_empty() || rest.starts_with([' ', '\t'])).then_some(rest)
    }

    /// The item marker, if the language writes one.
    pub(crate) fn item_marker(&self) -> Option<&'static str> {
        self.config.item_marker
    }

    /// The separator between items, if the language has one.
    pub(crate) fn separator(&self) -> Option<char> {
        self.config.item_separator
    }

    pub(crate) fn is_separator(&self, character: char) -> bool {
        self.config.item_separator == Some(character)
    }

    fn is_opener(&self, character: char) -> bool {
        self.closer_of(character).is_some()
    }

    fn is_closer(&self, character: char) -> bool {
        self.opener_of(character).is_some()
    }

    /// Whether a quote opens with `character`, and whether a backslash
    /// escapes in it.
    pub(crate) fn quote(&self, character: char) -> Option<bool> {
        self.quotes
            .iter()
            .find_map(|(quote, escapes)| (*quote == character).then_some(*escapes))
    }

    /// Whether `text` starts with one of the language's quotes.
    pub(crate) fn starts_quoted(&self, text: &str) -> bool {
        text.chars()
            .next()
            .is_some_and(|first| self.quote(first).is_some())
    }

    /// `text` trimmed, without the quotes around it if it is quoted.
    pub(crate) fn strip_quotes(&self, text: &str) -> String {
        let trimmed = text.trim();
        if let Some(first) = trimmed.chars().next()
            && self.quote(first).is_some()
            && trimmed.len() >= 2 * first.len_utf8()
            && trimmed.ends_with(first)
        {
            return trimmed[first.len_utf8()..trimmed.len() - first.len_utf8()].to_string();
        }
        trimmed.to_string()
    }

    /// The byte just past the string quoted at `start`, or the end.
    pub(crate) fn quote_end(&self, code: &str, start: usize) -> usize {
        self.quote_close(code, start).unwrap_or(code.len())
    }

    /// The byte just past the string quoted at `start`; `None` when it is
    /// not closed.
    fn quote_close(&self, code: &str, start: usize) -> Option<usize> {
        let open = code[start..].chars().next()?;
        let escapes = self.quote(open).unwrap_or(false);
        let mut escaped = false;
        for (offset, character) in code[start + open.len_utf8()..].char_indices() {
            if escaped {
                escaped = false;
            } else if escapes && character == '\\' {
                escaped = true;
            } else if character == open {
                return Some(start + open.len_utf8() + offset + character.len_utf8());
            }
        }
        None
    }

    /// Where a line comment starts in `code` at `at`, if one does.
    fn comment_at(&self, code: &str, at: usize) -> bool {
        let before = code[..at].chars().next_back();
        before.is_none_or(char::is_whitespace)
            && self
                .config
                .line_comments
                .iter()
                .any(|marker| code[at..].starts_with(marker))
    }

    /// The byte offset of the bracket closing the map or list `text`
    /// starts with, outside strings.
    pub(crate) fn matching_close(&self, text: &str) -> Option<usize> {
        let open = text.chars().next()?;
        let close = self.closer_of(open)?;
        let mut depth = 0usize;
        let mut at = 0;
        while let Some(character) = text[at..].chars().next() {
            if self.quote(character).is_some() {
                at = self.quote_end(text, at);
                continue;
            }
            if character == open {
                depth += 1;
            } else if character == close {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            at += character.len_utf8();
        }
        None
    }

    /// The items of a container's inside, split at its top-level
    /// separators, each with the spacing around it. `None` when a string or
    /// bracket is open.
    pub(crate) fn split_items<'a>(&self, inner: &'a str) -> Option<Vec<&'a str>> {
        if inner.trim().is_empty() {
            return Some(Vec::new());
        }
        let mut items = Vec::new();
        let (mut depth, mut start, mut at) = (0usize, 0, 0);
        while let Some(character) = inner[at..].chars().next() {
            if self.quote(character).is_some() {
                at = self.quote_close(inner, at)?;
                continue;
            }
            if self.is_opener(character) {
                depth += 1;
            } else if self.is_closer(character) {
                depth = depth.checked_sub(1)?;
            } else if self.is_separator(character) && depth == 0 {
                items.push(&inner[start..at]);
                start = at + character.len_utf8();
            }
            at += character.len_utf8();
        }
        if depth != 0 {
            return None;
        }
        let last = &inner[start..];
        // A trailing separator (`[a, b, ]`) ends the list without another item.
        if !last.trim().is_empty() {
            items.push(last);
        }
        Some(items)
    }

    /// Top-level separators in `code`, outside brackets and quotes.
    pub(crate) fn top_level_separators(&self, code: &str) -> usize {
        let (mut depth, mut count, mut at) = (0i32, 0, 0);
        while let Some(character) = code[at..].chars().next() {
            if self.quote(character).is_some() {
                at = self.quote_end(code, at);
                continue;
            }
            if self.is_opener(character) {
                depth += 1;
            } else if self.is_closer(character) {
                depth -= 1;
            } else if self.is_separator(character) && depth == 0 {
                count += 1;
            }
            at += character.len_utf8();
        }
        count
    }

    /// How many of `pairs` `text` opens and leaves open, outside quotes.
    pub(crate) fn unclosed(&self, text: &str, pairs: &[(char, char)]) -> usize {
        let (mut depth, mut at) = (0usize, 0);
        while let Some(character) = text[at..].chars().next() {
            if self.quote(character).is_some() {
                at = self.quote_end(text, at);
                continue;
            }
            if pairs.iter().any(|(open, _)| *open == character) {
                depth += 1;
            } else if pairs.iter().any(|(_, close)| *close == character) {
                depth = depth.saturating_sub(1);
            }
            at += character.len_utf8();
        }
        depth
    }

    /// Whether `code` closes every bracket it opens.
    pub(crate) fn balanced(&self, code: &str) -> bool {
        let (mut depth, mut at) = (0i32, 0);
        while let Some(character) = code[at..].chars().next() {
            if self.quote(character).is_some() {
                at = self.quote_end(code, at);
                continue;
            }
            if self.is_opener(character) {
                depth += 1;
            } else if self.is_closer(character) {
                depth -= 1;
            }
            at += character.len_utf8();
        }
        depth == 0
    }

    /// The first key separator in `code` outside quotes: its byte offset
    /// and its length.
    pub(crate) fn key_separator(&self, code: &str) -> Option<(usize, usize)> {
        let mut at = 0;
        while let Some(character) = code[at..].chars().next() {
            if self.quote(character).is_some() {
                at = self.quote_end(code, at);
                continue;
            }
            if let Some(separator) = self
                .config
                .separators
                .iter()
                .find(|separator| code[at..].starts_with(**separator))
            {
                return Some((at, separator.len()));
            }
            at += character.len_utf8();
        }
        None
    }

    /// An entry `key: value`: its key (without quotes), the text up to its
    /// value, and its value.
    pub(crate) fn entry_parts<'a>(&self, item: &'a str) -> Option<(String, &'a str, &'a str)> {
        let (at, length) = self.key_separator(item)?;
        let key = self.strip_quotes(&item[..at]);
        let after = &item[at + length..];
        let value_start = at + length + (after.len() - after.trim_start().len());
        Some((key, &item[..value_start], &item[value_start..]))
    }

    /// `head` (the text of an entry up to its value) without its key: the
    /// separator and the spacing around it.
    pub(crate) fn separator_of<'a>(&self, head: &'a str) -> &'a str {
        match self.key_separator(head) {
            Some((at, _)) => {
                let key_end = head[..at].trim_end().len();
                &head[key_end..]
            }
            None => "",
        }
    }

    fn skip_space(&self, code: &str, mut at: usize, newlines: bool) -> usize {
        while let Some(character) = code[at..].chars().next() {
            match character {
                ' ' | '\t' | '\r' => at += 1,
                '\n' if newlines => at += 1,
                _ if self.comment_at(code, at) => {
                    at = code[at..].find('\n').map_or(code.len(), |end| at + end);
                }
                _ => break,
            }
        }
        at
    }

    /// Where the value starting at `at` ends: before the separator, closer,
    /// or line break that follows it outside brackets and quotes.
    fn value_end(&self, code: &str, at: usize) -> usize {
        let mut depth = 0usize;
        let mut index = at;
        let mut end = at;
        while let Some(character) = code[index..].chars().next() {
            if self.quote(character).is_some() {
                index = self.quote_end(code, index);
                end = index;
                continue;
            }
            if self.is_opener(character) {
                depth += 1;
            } else if self.is_closer(character) {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            } else if depth == 0 && (self.is_separator(character) || character == '\n') {
                break;
            } else if matches!(character, ' ' | '\t' | '\r') {
                index += 1;
                continue;
            }
            index += character.len_utf8();
            end = index;
        }
        end
    }

    /// The byte range in `code` of what `path` names inside the container
    /// `code` starts with (spanning lines if needed): the value, or with
    /// `key` the key.
    pub(crate) fn path_span(
        &self,
        code: &str,
        path: &[SchemaPathSegment],
        key: bool,
    ) -> Option<(usize, usize)> {
        let start = code.len() - code.trim_start().len();
        self.span_from(code, start, path, key)
    }

    fn span_from(
        &self,
        code: &str,
        at: usize,
        path: &[SchemaPathSegment],
        key: bool,
    ) -> Option<(usize, usize)> {
        let at = self.skip_space(code, at, true);
        let Some((segment, rest)) = path.split_first() else {
            return Some((at, self.value_end(code, at)));
        };
        let open = code[at..].chars().next()?;
        let is = |pair: Option<(char, char)>| pair.is_some_and(|(opener, _)| opener == open);
        match segment {
            SchemaPathSegment::ArrayItem(index) if is(self.list()) => {
                let mut position = at + open.len_utf8();
                for _ in 0..*index {
                    position = self.skip_space(code, position, true);
                    position = self.value_end(code, position);
                    position = self.skip_space(code, position, true);
                    let next = code[position..].chars().next();
                    if !next.is_some_and(|next| self.is_separator(next)) {
                        return None;
                    }
                    position += next?.len_utf8();
                }
                let position = self.skip_space(code, position, true);
                match code[position..].chars().next() {
                    None => return None,
                    Some(next) if self.is_closer(next) => return None,
                    Some(_) => {}
                }
                self.span_from(code, position, rest, key)
            }
            SchemaPathSegment::Key(wanted) if is(self.map()) => {
                let mut position = at + open.len_utf8();
                loop {
                    position = self.skip_space(code, position, true);
                    while let Some(next) = code[position..].chars().next()
                        && self.is_separator(next)
                    {
                        position = self.skip_space(code, position + next.len_utf8(), true);
                    }
                    let key_start = position;
                    let first = code[position..].chars().next()?;
                    if self.is_closer(first) {
                        return None;
                    }
                    let key_end = if self.quote(first).is_some() {
                        self.quote_end(code, position)
                    } else {
                        let rest_code = &code[position..];
                        position
                            + rest_code
                                .char_indices()
                                .find(|(offset, character)| {
                                    character.is_whitespace()
                                        || self.is_separator(*character)
                                        || self.is_closer(*character)
                                        || self.starts_key_separator(&rest_code[*offset..])
                                })
                                .map_or(rest_code.len(), |(offset, _)| offset)
                    };
                    let separator = self.skip_space(code, key_end, false);
                    let length = self.key_separator_len(&code[separator..])?;
                    let value = separator + length;
                    if self.strip_quotes(&code[key_start..key_end]) == *wanted {
                        if rest.is_empty() && key {
                            return Some((key_start, key_end));
                        }
                        return self.span_from(code, value, rest, key);
                    }
                    position = self.value_end(code, self.skip_space(code, value, true));
                }
            }
            _ => None,
        }
    }

    fn starts_key_separator(&self, text: &str) -> bool {
        self.key_separator_len(text).is_some()
    }

    /// The length of the key separator `text` starts with.
    fn key_separator_len(&self, text: &str) -> Option<usize> {
        self.config
            .separators
            .iter()
            .find(|separator| !separator.is_empty() && text.starts_with(**separator))
            .map(|separator| separator.len())
    }
}

/// Where the markup opening a key (see
/// [`ConfigLanguage::key_markup_len`]) starts when it ends right at byte
/// `name` of `text`: a few characters back at most.
pub(crate) fn markup_before(
    language: &(impl ConfigLanguage + ?Sized),
    text: &str,
    name: usize,
) -> Option<usize> {
    text[..name]
        .char_indices()
        .rev()
        .take(8)
        .map(|(at, _)| at)
        .find(|at| language.key_markup_len(&text[*at..]) == Some(name - at))
}

#[cfg(test)]
mod tests {
    use super::Flow;
    use crate::editor::language::LanguageConfig;
    use crate::editor::language::SchemaPathSegment::{self, ArrayItem, Key};

    fn flow(separators: &'static [&'static str]) -> Flow {
        Flow::from_config(LanguageConfig {
            separators,
            quotes: &["\"", "'"],
            map_brackets: Some(('{', '}')),
            list_brackets: Some(('[', ']')),
            item_separator: Some(','),
            ..LanguageConfig::default()
        })
    }

    fn text<'a>(flow: &Flow, code: &'a str, path: &[SchemaPathSegment], key: bool) -> &'a str {
        let (start, end) = flow.path_span(code, path, key).unwrap();
        &code[start..end]
    }

    #[test]
    fn finds_items_and_keys_inside_containers() {
        let colon = flow(&[":"]);
        let equals = flow(&["="]);
        assert_eq!(text(&colon, "[a, 5]", &[ArrayItem(1)], false), "5");
        let allow = [Key("allow".into()), ArrayItem(1)];
        assert_eq!(text(&colon, "{allow: [a, 5]}", &allow, false), "5");
        let bogus = [Key("bogus".into())];
        assert_eq!(
            text(&colon, "{hosts: [a], cert: c, bogus: 1}", &bogus, true),
            "bogus"
        );
        let key = [ArrayItem(0), Key("key".into())];
        let code = "[\n  { hosts = [\"a\"], key = \"k\" },\n]";
        assert_eq!(text(&equals, code, &key, false), "\"k\"");
        let deep = [Key("a".into()), Key("b".into()), ArrayItem(2)];
        assert_eq!(
            text(&equals, "{ a = { b = [1, \"x, y\", 3] } }", &deep, false),
            "3"
        );
        assert!(colon.path_span("[1]", &[ArrayItem(3)], false).is_none());
    }

    /// Any brackets, separators and quotes the language names, and none
    /// it does not: `(a | b)` lists, `<k := v>` maps, `,` as text.
    #[test]
    fn the_syntax_is_the_languages_own() {
        let odd = Flow::from_config(LanguageConfig {
            separators: &[":="],
            quotes: &["\""],
            map_brackets: Some(('<', '>')),
            list_brackets: Some(('(', ')')),
            item_separator: Some('|'),
            ..LanguageConfig::default()
        });
        let path = [Key("tls".into()), Key("on".into())];
        let code = "<name := \"a | b\" | tls := <cert := x,y | on := yes>>";
        assert_eq!(text(&odd, code, &path, false), "yes");
        assert_eq!(text(&odd, code, &path, true), "on");
        assert_eq!(text(&odd, "(1, 2 | 3)", &[ArrayItem(1)], false), "3");
        assert_eq!(odd.matching_close("(a | \")\" | (b))"), Some(14));
        assert_eq!(odd.split_items(" a, b | c ").unwrap(), [" a, b ", " c "]);
        assert!(odd.path_span("[a, b]", &[ArrayItem(1)], false).is_none());
    }

    #[test]
    fn a_backslash_is_text_in_literal_quotes_only() {
        let toml = Flow::from_config(LanguageConfig {
            quotes: &["\"", "'"],
            literal_quotes: &["'"],
            list_brackets: Some(('[', ']')),
            item_separator: Some(','),
            ..LanguageConfig::default()
        });
        assert_eq!(toml.matching_close(r#"['C:\', "a\"]"]"#), Some(14));
        assert_eq!(toml.split_items(r"'C:\', 'd'").unwrap().len(), 2);
    }
}
