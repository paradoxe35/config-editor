//! Whether a line of the edited text is a line of the reference text with
//! only closers added: brackets and end tags anywhere, and what completes
//! the line itself (the quote it left open, the `>` of a tag it left open,
//! the `:` or `=` its key lacks), once.

/// A changed line's stretch inside a field: its text, and whether it starts
/// or ends its line's data (only closers and commas beyond it), where it
/// must start or end the reference line's data too: a value cut short is
/// not a value closed.
pub(super) struct Part {
    pub(super) text: Vec<char>,
    pub(super) starts_line: bool,
    pub(super) ends_line: bool,
}

impl Part {
    /// The stretch `from..to` (in chars) of the edited text's `line`.
    pub(super) fn new(line: &str, from: usize, to: usize) -> Self {
        let chars: Vec<char> = line.chars().collect();
        let (from, to) = (from.min(chars.len()), to.min(chars.len()).max(from));
        let piece = |chars: &[char]| compact(&chars.iter().collect::<String>());
        Part {
            text: piece(&chars[from..to]),
            starts_line: without_closers(&piece(&chars[..from])).is_empty(),
            ends_line: without_closers(&piece(&chars[to..])).is_empty(),
        }
    }

    /// Whether the stretch adds nothing but closers.
    pub(super) fn only_closes(&self) -> bool {
        without_closers(&self.text).is_empty()
    }

    /// Whether the stretch is a stretch of `line` (of the reference text,
    /// compacted) with only closers added: its data is a contiguous
    /// stretch of the line's, the line's characters in that stretch are
    /// all still there in order (none removed or moved), and anything else
    /// added closes something the line left open, once. `Some(true)` when
    /// such a completer was added: a quote may land inside a YAML plain
    /// value, so the caller checks the value's strings are the line's.
    pub(super) fn closes(&self, line: &[char]) -> Option<bool> {
        let completes = completers(line);
        let skipped = |character: char| is_closer(character) || completes.contains(&character);
        let core: Vec<char> = without_closers(&self.text)
            .into_iter()
            .filter(|character| !completes.contains(character))
            .collect();
        let starts: Vec<usize> = if self.starts_line {
            vec![0]
        } else {
            (0..line.len().max(1)).collect()
        };
        starts.into_iter().find_map(|start| {
            // The stretch of `line` from `start` whose data is `core`.
            let mut data = 0;
            let mut end = start;
            let open_tag = completes.contains(&'>');
            while end < line.len() && data < core.len() {
                if let Some(length) = line_end_tag(&line[end..], open_tag) {
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
                    || without_closers(&line[end..]).is_empty()
                    || line_end_tag(&line[end..], open_tag) == Some(line.len() - end)
            };
            let stretch = &line[start.min(line.len())..end];
            let added = |character: &char| {
                count(&self.text, *character).saturating_sub(count(stretch, *character))
            };
            (data == core.len()
                && rest_closes()
                && is_subsequence(stretch, &self.text)
                && completes.iter().all(|character| added(character) <= 1)
                && [':', '='].iter().all(|separator| {
                    added(separator) == 0 || (self.starts_line && separates_key(&self.text))
                }))
            .then(|| completes.iter().any(|character| added(character) == 1))
        })
    }
}

/// `text` without whitespace and without a comma ending it (a separator
/// that moves to a closer added after it: `"b": 2,` → `"b": 2` + `},`).
pub(super) fn compact(text: &str) -> Vec<char> {
    let mut chars: Vec<char> = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    while chars.last() == Some(&',') {
        chars.pop();
    }
    chars
}

/// What may be added once to complete `line` itself: a quote it left open
/// (an odd count), the `>` of a tag it left open, and the `:` or `=` after
/// its key when it has neither outside quotes (see [`separates_key`]).
fn completers(line: &[char]) -> Vec<char> {
    let mut found: Vec<char> = ['"', '\'']
        .into_iter()
        .filter(|quote| count(line, *quote) % 2 == 1)
        .collect();
    if count(line, '<') > count(line, '>') {
        found.push('>');
    }
    let mut quoted = None;
    let separated = line.iter().any(|character| match quoted {
        Some(quote) => {
            if *character == quote {
                quoted = None;
            }
            false
        }
        None if matches!(character, '"' | '\'') => {
            quoted = Some(*character);
            false
        }
        None => matches!(character, ':' | '='),
    });
    if !separated {
        found.extend([':', '=']);
    }
    found
}

/// Whether `part` (a whole line's data) starts with a key, quoted or a
/// word, followed by `:` or `=`: where a separator added completes it.
fn separates_key(part: &[char]) -> bool {
    let key = match part.first() {
        Some(quote @ ('"' | '\'')) => part[1..]
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
    key > 0 && matches!(part.get(key), Some(':' | '='))
}

/// The length of the markup end tag starting `chars`, or of one left open
/// at the end of the line (`</web`, when `>` completes the line).
fn line_end_tag(chars: &[char], open: bool) -> Option<usize> {
    end_tag(chars).or_else(|| {
        (open
            && chars.len() > 2
            && chars.starts_with(&['<', '/'])
            && chars[2..].iter().all(|character| is_name(*character)))
        .then_some(chars.len())
    })
}

fn count(chars: &[char], wanted: char) -> usize {
    chars
        .iter()
        .filter(|character| **character == wanted)
        .count()
}

/// Whether `character` only closes: adding it adds no data.
fn is_closer(character: char) -> bool {
    matches!(character, '}' | ']' | ')')
}

/// `compacted` without closers and markup end tags (`</web>`).
fn without_closers(compacted: &[char]) -> Vec<char> {
    let mut kept = Vec::new();
    let mut at = 0;
    while at < compacted.len() {
        if let Some(length) = end_tag(&compacted[at..]) {
            at += length;
            continue;
        }
        if !is_closer(compacted[at]) {
            kept.push(compacted[at]);
        }
        at += 1;
    }
    kept
}

/// The length of the markup end tag `</name>` starting `chars`, if one does.
fn end_tag(chars: &[char]) -> Option<usize> {
    if chars.first() != Some(&'<') || chars.get(1) != Some(&'/') {
        return None;
    }
    let close = chars.iter().position(|character| *character == '>')?;
    chars[2..close]
        .iter()
        .all(|character| is_name(*character))
        .then_some(close + 1)
}

fn is_name(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '-' | '.' | ':')
}

fn is_subsequence(short: &[char], long: &[char]) -> bool {
    let mut rest = long.iter();
    short
        .iter()
        .all(|character| rest.any(|other| other == character))
}
