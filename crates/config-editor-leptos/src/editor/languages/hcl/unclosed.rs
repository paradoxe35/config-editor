//! What a cut-off HCL text leaves open: a string, a heredoc, a block
//! comment, a `${`, a bracket. The parser only says "unexpected token"
//! for these; this scan finds the opener to name and point at.

/// A construct opened and never closed, at a byte offset.
#[derive(Debug, PartialEq)]
pub(super) enum Open {
    String,
    Heredoc(String),
    Comment,
    Interpolation,
    Bracket(char),
}

/// The first construct of `source` left open at its end (the outermost
/// bracket), or a string a line break ends.
pub(super) fn first_open(source: &str) -> Option<(usize, Open)> {
    // Openers not yet closed; a `"` here is a string, a `$` a `${`.
    let mut stack: Vec<(usize, char)> = Vec::new();
    let chars: Vec<(usize, char)> = source.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (at, c) = chars[i];
        let in_string = stack.last().is_some_and(|(_, top)| *top == '"');
        let next = chars.get(i + 1).map(|(_, c)| *c);
        if in_string {
            match c {
                '\\' => i += 1,
                '"' => {
                    stack.pop();
                }
                '\n' => {
                    let (start, _) = *stack.last()?;
                    return Some((start, Open::String));
                }
                '$' | '%' if next == Some('{') => {
                    // `$${` and `%%{` are escapes.
                    let escaped = i > 0 && chars[i - 1].1 == c;
                    if escaped {
                        i += 1;
                    } else {
                        stack.push((at, '$'));
                        i += 1;
                    }
                }
                _ => {}
            }
        } else {
            match c {
                '"' => stack.push((at, '"')),
                '#' => i = line_end(&chars, i),
                '/' if next == Some('/') => i = line_end(&chars, i),
                '/' if next == Some('*') => {
                    let rest = &source[at + 2..];
                    match rest.find("*/") {
                        Some(end) => {
                            let target = at + 2 + end + 2;
                            i = chars.iter().position(|(offset, _)| *offset >= target).unwrap_or(chars.len());
                            continue;
                        }
                        None => return Some((at, Open::Comment)),
                    }
                }
                '<' if next == Some('<') => {
                    if let Some((word, body)) = heredoc_word(&source[at..]) {
                        match heredoc_end(&source[at + body..], &word) {
                            Some(end) => {
                                let target = at + body + end;
                                i = chars.iter().position(|(offset, _)| *offset >= target).unwrap_or(chars.len());
                                continue;
                            }
                            None => return Some((at, Open::Heredoc(word))),
                        }
                    }
                }
                '{' | '[' | '(' => stack.push((at, c)),
                '}' | ']' | ')' => {
                    // A `}` ends a `${` the same way it ends a `{`.
                    stack.pop();
                }
                _ => {}
            }
        }
        i += 1;
    }
    let (at, top) = stack.first().copied()?;
    // The outermost opener, unless a string or `${` inside it is the cut.
    let inner = stack.iter().rev().find(|(_, c)| *c == '$').or_else(|| stack.iter().rev().find(|(_, c)| *c == '"')).copied();
    let (at, top) = inner.unwrap_or((at, top));
    Some((
        at,
        match top {
            '"' => Open::String,
            '$' => Open::Interpolation,
            bracket => Open::Bracket(bracket),
        },
    ))
}

/// The index of the line break ending the line `i` is on (the loop's `+ 1`
/// steps past it).
fn line_end(chars: &[(usize, char)], i: usize) -> usize {
    chars[i..].iter().position(|(_, c)| *c == '\n').map_or(chars.len(), |n| i + n)
}

/// A heredoc's end word and the length of its opening line, for text
/// starting at `<<`.
fn heredoc_word(text: &str) -> Option<(String, usize)> {
    let rest = text.strip_prefix("<<")?;
    let rest = rest.strip_prefix('-').unwrap_or(rest);
    let line = rest.lines().next()?;
    let word = line.trim_end();
    let valid = !word.is_empty() && word.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-');
    // Without a line break after the word, nothing follows to read.
    if !valid || !rest.contains('\n') {
        return None;
    }
    let consumed = text.len() - rest.len() + line.len();
    Some((word.to_string(), consumed))
}

/// Where in `text` (after a heredoc's opening line) its terminator line
/// ends it: the offset just past `word` there (see
/// [`super::heredoc::terminator_end`]).
fn heredoc_end(text: &str, word: &str) -> Option<usize> {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if let Some(end) = super::heredoc::terminator_end(line, word) {
            return Some(offset + end);
        }
        offset += line.len();
    }
    None
}
