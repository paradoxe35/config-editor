//! Multi-line strings (`"""…"""`, `'''…'''`) written in place of the host's:
//! a basic one is spliced, its unchanged start and end kept as written (its
//! escapes and line-ending backslashes, `"""\` … `\`), and only the change
//! escaped; a carriage return or other control character, which TOML
//! readers drop or refuse raw, is always escaped, so a literal string that
//! gains one becomes a basic string.

/// `body` in a multi-line basic string: `\`, `"""` and control characters
/// other than a tab or line break escaped (`\r`, `\u0001`).
pub(super) fn escape_basic_multiline(body: &str) -> String {
    let mut out = String::with_capacity(body.len() + 8);
    for character in body.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '\r' => out.push_str("\\r"),
            '\n' | '\t' => out.push(character),
            control if control.is_control() => out.push_str(&format!("\\u{:04X}", control as u32)),
            other => out.push(other),
        }
    }
    out.replace("\"\"\"", "\"\"\\\"")
}

/// Whether a literal string (`'''…'''`) can hold `body`: no control
/// character but a tab or line break, which it has no escape for.
pub(super) fn literal_can_hold(body: &str) -> bool {
    !body.chars().any(|character| character.is_control() && !matches!(character, '\t' | '\n'))
}

/// `body` as a new multi-line basic string (its first line break after the
/// opening quotes, which readers trim).
pub(super) fn basic_multiline(body: &str) -> String {
    format!("\"\"\"\n{}\"\"\"", escape_basic_multiline(body))
}

/// One piece of a written multi-line basic string: the character it reads
/// as (none for the line break after the opening quotes and a line-ending
/// backslash with the whitespace it trims) and how it is written.
struct Unit<'a> {
    read: Option<char>,
    written: &'a str,
}

/// The body of `"""…"""` (without its quotes) read one piece at a time;
/// `None` for an escape this reader does not know.
fn units(body: &str) -> Option<Vec<Unit<'_>>> {
    let mut out = Vec::new();
    let mut at = 0;
    if body.starts_with('\n') {
        out.push(Unit { read: None, written: &body[..1] });
        at = 1;
    }
    while at < body.len() {
        let rest = &body[at..];
        let Some(after) = rest.strip_prefix('\\') else {
            let read = rest.chars().next()?;
            out.push(Unit { read: Some(read), written: &rest[..read.len_utf8()] });
            at += read.len_utf8();
            continue;
        };
        // A line-ending backslash trims the whitespace and line breaks after
        // it.
        let blank = after.len() - after.trim_start_matches([' ', '\t']).len();
        if after[blank..].starts_with('\n') {
            let trimmed = after.len() - after.trim_start_matches([' ', '\t', '\n']).len();
            out.push(Unit { read: None, written: &rest[..1 + trimmed] });
            at += 1 + trimmed;
            continue;
        }
        let (read, length) = match after.chars().next()? {
            'b' => ('\u{8}', 2),
            't' => ('\t', 2),
            'n' => ('\n', 2),
            'f' => ('\u{c}', 2),
            'r' => ('\r', 2),
            'e' => ('\u{1b}', 2),
            '"' => ('"', 2),
            '\\' => ('\\', 2),
            letter @ ('u' | 'U' | 'x') => {
                let digits = match letter {
                    'u' => 4,
                    'U' => 8,
                    _ => 2,
                };
                let code = u32::from_str_radix(after.get(1..1 + digits)?, 16).ok()?;
                (char::from_u32(code)?, 2 + digits)
            }
            _ => return None,
        };
        out.push(Unit { read: Some(read), written: rest.get(..length)? });
        at += length;
    }
    Some(out)
}

/// `text` written over `written` (a whole `"""…"""` string): the unchanged
/// start and end as the host wrote them, the changed middle escaped. A
/// line-ending backslash inside the change goes with it (the text stays
/// exact; its line breaks there are the new text's). `None` when `written`
/// is not a multi-line basic string this reader knows.
pub(super) fn spliced_basic_multiline(text: &str, written: &str) -> Option<String> {
    let body = written.strip_prefix("\"\"\"")?.strip_suffix("\"\"\"")?;
    let units = units(body)?;
    let chars: Vec<usize> = units.iter().enumerate().filter(|(_, unit)| unit.read.is_some()).map(|(index, _)| index).collect();
    let new: Vec<char> = text.chars().collect();
    let read = |index: usize| units[chars[index]].read;
    let prefix = (0..chars.len().min(new.len())).take_while(|&index| read(index) == Some(new[index])).count();
    let room = (chars.len() - prefix).min(new.len() - prefix);
    let suffix = (0..room).take_while(|&back| read(chars.len() - 1 - back) == Some(new[new.len() - 1 - back])).count();
    // The kept start ends with its last character, the kept end starts with
    // its first: every kept backslash is still followed by what it was.
    // Before the first character, the line break after the opening quotes
    // and line-ending backslashes stay while the new text starts with no
    // whitespace for them to trim.
    let lead = chars.first().copied().unwrap_or(units.len());
    let keep_start = match prefix {
        0 if new.first().is_some_and(|first| !first.is_whitespace()) => lead,
        0 => 0,
        _ => chars[prefix - 1] + 1,
    };
    let keep_end = if suffix == 0 { units.len() } else { chars[chars.len() - suffix] };
    let mut middle = escape_basic_multiline(&new[prefix..new.len() - suffix].iter().collect::<String>());
    // Readers trim a line break right after the opening quotes.
    if keep_start == 0 && middle.starts_with('\n') {
        middle.replace_range(..1, "\\n");
    }
    // Nor may the change make three quotes with the kept text around it.
    let before: String = units[..keep_start].iter().map(|unit| unit.written).collect();
    let after: String = units[keep_end..].iter().map(|unit| unit.written).collect();
    let before_end: String = before.chars().rev().take(2).collect::<Vec<_>>().into_iter().rev().collect();
    let after_start: String = after.chars().take(2).collect();
    if format!("{before_end}{middle}{after_start}").contains("\"\"\"") {
        return None;
    }
    Some(format!("\"\"\"{before}{middle}{after}\"\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_is_spliced_and_the_written_form_kept() {
        let written = "\"\"\"\\\n  one \\\n  two\"\"\"";
        assert_eq!(spliced_basic_multiline("one twoX", written).as_deref(), Some("\"\"\"\\\n  one \\\n  twoX\"\"\""));
        assert_eq!(spliced_basic_multiline("Xone two", written).as_deref(), Some("\"\"\"\\\n  Xone \\\n  two\"\"\""));
        assert_eq!(spliced_basic_multiline("one\r\ntwo", written).as_deref(), Some("\"\"\"\\\n  one\\r\ntwo\"\"\""));
        assert_eq!(spliced_basic_multiline("\nx", "\"\"\"\na\"\"\"").as_deref(), Some("\"\"\"\\nx\"\"\""));
        assert_eq!(spliced_basic_multiline("a\\é", "\"\"\"\na\\u00e9\"\"\"").as_deref(), Some("\"\"\"\na\\\\\\u00e9\"\"\""));
        assert_eq!(escape_basic_multiline("a\r\nb\u{1}\"\"\""), "a\\r\nb\\u0001\"\"\\\"");
        assert!(!literal_can_hold("a\r\n"));
    }
}
