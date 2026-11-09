//! Whether a line of the edited text is a line of the reference text with
//! only closers added: closing brackets and closing markup anywhere, and
//! what completes the line itself (the quote it left open, the end of the
//! markup it left open, the key separator its key lacks), once. What is a
//! closer, a quote or a separator is the language's own.

use super::super::super::language::ConfigLanguage;

/// The characters that close, quote and separate in a language, as its
/// config and markup describe them.
pub(super) struct Syntax<'a> {
    behavior: &'a dyn ConfigLanguage,
    closers: Vec<char>,
    quotes: Vec<char>,
    /// The characters of the key separators (`:`, `=`, both of `:=`).
    separators: Vec<char>,
    item_separator: Option<char>,
    /// The characters markup is written between (XML's `<` and `>`).
    markup: Option<(char, char)>,
}

impl<'a> Syntax<'a> {
    pub(super) fn of(behavior: &'a dyn ConfigLanguage) -> Self {
        let config = behavior.config();
        let mut quotes: Vec<char> = Vec::new();
        for quote in config.quotes {
            if let Some(first) = quote.chars().next()
                && !quotes.contains(&first)
            {
                quotes.push(first);
            }
        }
        let mut separators: Vec<char> = Vec::new();
        for character in config
            .separators
            .iter()
            .flat_map(|separator| separator.chars())
        {
            if !separators.contains(&character) {
                separators.push(character);
            }
        }
        Syntax {
            behavior,
            closers: config.brackets.iter().map(|(_, close)| *close).collect(),
            quotes,
            separators,
            item_separator: config.item_separator,
            markup: behavior.markup_delimiters(),
        }
    }

    fn is_closer(&self, character: char) -> bool {
        self.closers.contains(&character)
    }

    /// The length of the closing markup starting `chars`, if one does.
    fn end_tag(&self, chars: &[char]) -> Option<usize> {
        let (open, _) = self.markup?;
        if chars.first() != Some(&open) {
            return None;
        }
        let text: String = chars.iter().collect();
        let bytes = self.behavior.closing_markup_len(&text)?;
        Some(text.get(..bytes)?.chars().count())
    }

    /// [`Self::end_tag`], or closing markup left open at the end of the
    /// line (`</web`, when the markup's closer completes the line).
    fn line_end_tag(&self, chars: &[char], open: bool) -> Option<usize> {
        self.end_tag(chars).or_else(|| {
            let (_, close) = self.markup?;
            let mut closed: Vec<char> = chars.to_vec();
            closed.push(close);
            (open && chars.len() > 2 && self.end_tag(&closed) == Some(closed.len()))
                .then_some(chars.len())
        })
    }

    /// `compacted` without closers and closing markup (`</web>`).
    fn without_closers(&self, compacted: &[char]) -> Vec<char> {
        let mut kept = Vec::new();
        let mut at = 0;
        while at < compacted.len() {
            if let Some(length) = self.end_tag(&compacted[at..]) {
                at += length;
                continue;
            }
            if !self.is_closer(compacted[at]) {
                kept.push(compacted[at]);
            }
            at += 1;
        }
        kept
    }

    /// `text` without whitespace and without an item separator ending it
    /// (one that moves to a closer added after it: `"b": 2,` → `"b": 2` +
    /// `},`).
    pub(super) fn compact(&self, text: &str) -> Vec<char> {
        let mut chars: Vec<char> = text
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        while self.item_separator.is_some() && chars.last() == self.item_separator.as_ref() {
            chars.pop();
        }
        chars
    }

    /// What may be added once to complete `line` itself: a quote it left
    /// open (an odd count), the end of markup it left open, and a key
    /// separator when it has none outside quotes (see
    /// [`Self::separates_key`]).
    fn completers(&self, line: &[char]) -> Vec<char> {
        let mut found: Vec<char> = self
            .quotes
            .iter()
            .copied()
            .filter(|quote| count(line, *quote) % 2 == 1)
            .collect();
        if let Some((open, close)) = self.markup
            && count(line, open) > count(line, close)
        {
            found.push(close);
        }
        let mut quoted = None;
        let separated = line.iter().any(|character| match quoted {
            Some(quote) => {
                if *character == quote {
                    quoted = None;
                }
                false
            }
            None if self.quotes.contains(character) => {
                quoted = Some(*character);
                false
            }
            None => self.separators.contains(character),
        });
        if !separated {
            found.extend(self.separators.iter().copied());
        }
        found
    }

    /// Whether `part` (a whole line's data) starts with a key, quoted or a
    /// word, followed by a key separator: where a separator added
    /// completes it.
    fn separates_key(&self, part: &[char]) -> bool {
        let key = match part.first() {
            Some(quote) if self.quotes.contains(quote) => part[1..]
                .iter()
                .position(|character| character == quote)
                .map_or(part.len(), |end| end + 2),
            _ => part
                .iter()
                .take_while(|character| {
                    character.is_alphanumeric() || matches!(character, '_' | '-' | '.')
                })
                .count(),
        };
        key > 0
            && part
                .get(key)
                .is_some_and(|next| self.separators.contains(next))
    }
}

/// A changed line's stretch inside a field: its text, and whether it starts
/// or ends its line's data (only closers and separators beyond it), where
/// it must start or end the reference line's data too: a value cut short
/// is not a value closed.
pub(super) struct Part {
    pub(super) text: Vec<char>,
    pub(super) starts_line: bool,
    pub(super) ends_line: bool,
}

impl Part {
    /// The stretch `from..to` (in chars) of the edited text's `line`.
    pub(super) fn new(syntax: &Syntax<'_>, line: &str, from: usize, to: usize) -> Self {
        let chars: Vec<char> = line.chars().collect();
        let (from, to) = (from.min(chars.len()), to.min(chars.len()).max(from));
        let piece = |chars: &[char]| syntax.compact(&chars.iter().collect::<String>());
        Part {
            text: piece(&chars[from..to]),
            starts_line: syntax.without_closers(&piece(&chars[..from])).is_empty(),
            ends_line: syntax.without_closers(&piece(&chars[to..])).is_empty(),
        }
    }

    /// Whether the stretch adds nothing but closers.
    pub(super) fn only_closes(&self, syntax: &Syntax<'_>) -> bool {
        syntax.without_closers(&self.text).is_empty()
    }

    /// Whether the stretch is a stretch of `line` (of the reference text,
    /// compacted) with only closers added: its data is a contiguous
    /// stretch of the line's, the line's characters in that stretch are
    /// all still there in order (none removed or moved), and anything else
    /// added closes something the line left open, once. `Some(true)` when
    /// such a completer was added: a quote may land inside a YAML plain
    /// value, so the caller checks the value's strings are the line's.
    pub(super) fn closes(&self, syntax: &Syntax<'_>, line: &[char]) -> Option<bool> {
        let completes = syntax.completers(line);
        let skipped =
            |character: char| syntax.is_closer(character) || completes.contains(&character);
        let core: Vec<char> = syntax
            .without_closers(&self.text)
            .into_iter()
            .filter(|character| !completes.contains(character))
            .collect();
        let starts: Vec<usize> = if self.starts_line {
            vec![0]
        } else {
            (0..line.len().max(1)).collect()
        };
        let open_tag = syntax
            .markup
            .is_some_and(|(_, close)| completes.contains(&close));
        starts.into_iter().find_map(|start| {
            // The stretch of `line` from `start` whose data is `core`.
            let mut data = 0;
            let mut end = start;
            while end < line.len() && data < core.len() {
                if let Some(length) = syntax.line_end_tag(&line[end..], open_tag) {
                    end += length;
                    continue;
                }
                if skipped(line[end]) {
                    end += 1;
                    continue;
                }
                if line[end] != core[data] {
                    break;
                }
                data += 1;
                end += 1;
            }
            let rest_closes = || {
                !self.ends_line
                    || syntax.without_closers(&line[end..]).is_empty()
                    || syntax.line_end_tag(&line[end..], open_tag) == Some(line.len() - end)
            };
            let stretch = &line[start.min(line.len())..end];
            let added = |character: &char| {
                count(&self.text, *character).saturating_sub(count(stretch, *character))
            };
            (data == core.len()
                && rest_closes()
                && is_subsequence(stretch, &self.text)
                && completes.iter().all(|character| added(character) <= 1)
                && syntax.separators.iter().all(|separator| {
                    added(separator) == 0 || (self.starts_line && syntax.separates_key(&self.text))
                }))
            .then(|| completes.iter().any(|character| added(character) == 1))
        })
    }
}

fn count(chars: &[char], wanted: char) -> usize {
    chars
        .iter()
        .filter(|character| **character == wanted)
        .count()
}

fn is_subsequence(short: &[char], long: &[char]) -> bool {
    let mut rest = long.iter();
    short
        .iter()
        .all(|character| rest.any(|other| other == character))
}
