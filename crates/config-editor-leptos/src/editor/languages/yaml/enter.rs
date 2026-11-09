//! What Enter writes in YAML: a flow collection opened on the line goes on
//! one level deeper, an empty `- ` item ends its list at the parent's
//! column, and the rest follows the line's indentation and what it opens
//! (`newline_with_indent`).

use super::super::super::keyboard::utf16_to_byte_index;
use super::super::super::language::{Newline, bracket_newline};
use super::editing::{code_before_comment, newline_with_indent};

pub(super) fn newline(source: &str, cursor: usize) -> Newline {
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let line = &source[line_start..byte];
    let before = line.trim_end_matches([' ', '\t']);
    let after = source[byte..].trim_start_matches([' ', '\t']);
    let pairs = &[('{', '}'), ('[', ']')];
    // Between flow brackets (`[|]`) the collection opens across lines; in
    // one opened earlier on the line (`[a,| b]`) the new line goes one
    // level deeper, as YAML wants flow content indented past its key.
    if (before.ends_with(['[', '{']) && after.starts_with([']', '}']))
        || flow_open_at_end(code_before_comment(line))
    {
        let config = super::super::super::language::ConfigLanguage::config(&super::YamlBehavior);
        return bracket_newline(source, cursor, pairs, &config, code_before_comment);
    }
    // Enter on an empty `- ` item ends the list: the dash goes, and the
    // caret goes to the column of what holds the list, where its next key
    // is written.
    if line.trim() == "-" && after.split('\n').next().is_some_and(|rest| rest.trim().is_empty()) {
        let dash = line.len() - line.trim_start().len();
        let column = owner_column(&source[..line_start], dash).min(dash);
        return Newline {
            text: " ".repeat(column),
            caret: column,
            replaces: 0,
            replaces_before: line.len(),
        };
    }
    Newline::new(newline_with_indent(source, cursor))
}

/// Whether a flow collection opened in `code` (a line's text before the
/// caret, without its comment) is still open at its end, the end not
/// inside a quoted scalar. A bracket opens one only where a node starts
/// (`a: [`, `- {`, `[[`), not inside a plain scalar (`a: b[1`).
fn flow_open_at_end(code: &str) -> bool {
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut previous: Option<char> = None;
    let mut characters = code.chars().peekable();
    while let Some(character) = characters.next() {
        match quote {
            Some('"') if character == '\\' => {
                characters.next();
                continue;
            }
            Some('\'') if character == '\'' && characters.peek() == Some(&'\'') => {
                characters.next();
                continue;
            }
            Some(open) if character == open => quote = None,
            Some(_) => continue,
            None => {
                let node_starts = previous.is_none_or(|previous| matches!(previous, '[' | '{' | ',' | ':' | '-' | '?'));
                match character {
                    '"' | '\'' if node_starts => quote = Some(character),
                    '[' | '{' if depth > 0 || node_starts => depth += 1,
                    ']' | '}' if depth > 0 => depth -= 1,
                    _ => {}
                }
            }
        }
        if !character.is_whitespace() {
            previous = Some(character);
        }
    }
    depth > 0 && quote.is_none()
}

/// The column where the node holding a list whose dash is at column `dash`
/// writes its next key, from the lines `above` it: the key's column on the
/// first line less indented (past its own dashes, `- a:` holds keys at
/// 2), or `dash` for a list written at its key's indentation.
fn owner_column(above: &str, dash: usize) -> usize {
    for line in above.lines().rev() {
        let content = line.trim_start_matches([' ', '\t']);
        if content.is_empty() || content.starts_with('#') {
            continue;
        }
        let indent = line.len() - content.len();
        let dashed = content == "-" || content.starts_with("- ") || content.starts_with("-\t");
        if indent < dash {
            // Past the item's dashes: `- - a:` holds keys at 4.
            let mut rest = content;
            while rest == "-" || rest.starts_with("- ") || rest.starts_with("-\t") {
                rest = rest[1..].trim_start_matches([' ', '\t']);
            }
            return line.len() - rest.len();
        }
        if indent == dash && !dashed {
            return dash;
        }
    }
    dash
}

#[cfg(test)]
mod tests {
    use super::{flow_open_at_end, owner_column};

    #[test]
    fn flows_open_where_a_node_starts() {
        for (code, open) in [
            ("l: [a,", true),
            ("l: [a, b]", false),
            ("- {a: 1,", true),
            ("l: [[a], [b,", true),
            ("a: b[1", false),
            ("l: [\"a, [\",", true),
            ("l: [\"a,", false),
            ("l: ['it''s [',", true),
            ("[a, {b: c,", true),
        ] {
            assert_eq!(flow_open_at_end(code), open, "{code}");
        }
    }

    #[test]
    fn a_list_ends_at_the_column_of_what_holds_it() {
        for (above, dash, column) in [
            ("l:\n  - a\n", 2, 0),
            ("l:\n- a\n", 0, 0),
            ("- a:\n    - x\n", 4, 2),
            ("- a:\n  - x\n", 2, 2),
            ("m:\n  l:\n    - x\n    # c\n\n    - y\n", 4, 2),
            ("- - a:\n      - x\n", 6, 4),
            ("- a\n", 0, 0),
        ] {
            assert_eq!(owner_column(above, dash), column, "{above:?}");
        }
    }
}
