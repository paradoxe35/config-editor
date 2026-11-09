//! The colours inside an HCL string: its text, and the template sequences
//! written into it (`"web-${var.env}"`, `"%{ if x }on%{ endif }"`), which
//! are expressions, not text (QA 28: they took the string's colour).

use super::super::super::highlight::{TokenKind, Tokens};
use super::tokens::template_end;

/// Pushes the runs of the string `line[start..end]` (or the template
/// sequence outside a string, there), at byte `offset` of the line: the
/// text as a string and each `${ … }` / `%{ … }` as an interpolation,
/// delimiters included. `$${` and `%%{` are text (HCL's escapes).
pub(super) fn push_string(out: &mut Tokens, line: &str, offset: usize, start: usize, end: usize) {
    push_runs(out, line, offset, start, end, true);
}

/// Pushes the runs of a heredoc's line `text`, as a string's: a heredoc
/// has no backslash escapes.
pub(super) fn push_heredoc_line(out: &mut Tokens, text: &str) {
    push_runs(out, text, 0, 0, text.len(), false);
}

fn push_runs(out: &mut Tokens, line: &str, offset: usize, start: usize, end: usize, escapes: bool) {
    let text = &line[..end];
    let mut run = start;
    let mut index = start;
    while index < end {
        let rest = &text[index..];
        let character = rest.chars().next().unwrap_or('\0');
        if escapes && character == '\\' {
            index += 1 + rest[1..].chars().next().map_or(0, char::len_utf8);
        } else if rest.starts_with("$${") || rest.starts_with("%%{") {
            index += 3;
        } else if rest.starts_with("${") || rest.starts_with("%{") {
            let close = template_end(text, index).min(end);
            out.push(offset + run, offset + index, TokenKind::String);
            out.push(offset + index, offset + close, TokenKind::Interpolation);
            index = close;
            run = close;
        } else {
            index += character.len_utf8();
        }
    }
    out.push(offset + run, offset + end, TokenKind::String);
}

#[cfg(test)]
mod tests {
    use super::super::super::super::highlight::{Token, TokenKind};
    use super::super::super::Language;

    /// The parts of `line` HCL colours as interpolations.
    fn interpolations(line: &str) -> Vec<&str> {
        Language::HCL
            .behavior()
            .highlight(line)
            .iter()
            .filter(|token: &&Token| token.kind == TokenKind::Interpolation)
            .map(|token| &line[token.start..token.end])
            .collect()
    }

    /// QA 28 (F3): `${…}` in a string was one string token.
    #[test]
    fn template_sequences_in_strings_are_interpolations() {
        for (line, expected) in [
            ("name = \"web-${var.env}\"", vec!["${var.env}"]),
            ("a = \"${x}-${y}\"", vec!["${x}", "${y}"]),
            ("t = \"%{ if on }yes%{ endif }\"", vec!["%{ if on }", "%{ endif }"]),
            ("n = \"${ \"}\" }!\"", vec!["${ \"}\" }"]),
            ("tags = { Name = \"x-${local.n}\" }", vec!["${local.n}"]),
            ("lit = \"$${not} %%{not}\"", vec![]),
            ("open = \"a ${var.x", vec!["${var.x"]),
        ] {
            assert_eq!(interpolations(line), expected, "{line}");
        }
    }

    /// Around the interpolations, the string keeps its colour; a key
    /// written as a string stays a key.
    #[test]
    fn the_text_around_them_stays_a_string() {
        let line = "name = \"web-${var.env}-01\"";
        let tokens = Language::HCL.behavior().highlight(line);
        let kind_of = |part: &str| {
            let at = line.find(part).unwrap();
            tokens
                .iter()
                .find(|token| token.start <= at && at < token.end)
                .map(|token| token.kind)
        };
        assert_eq!(kind_of("\"web-"), Some(TokenKind::String));
        assert_eq!(kind_of("-01\""), Some(TokenKind::String));
        assert_eq!(kind_of("${"), Some(TokenKind::Interpolation));
        let tokens = Language::HCL.behavior().highlight("\"kebab-${x}\" = 1");
        assert_eq!(tokens[0].kind, TokenKind::Key);
    }
}
