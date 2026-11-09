//! Enter, comments, auto-closing, lexical state, and syntax colors.

use super::*;

/// Enter inside an element indents one level deeper; otherwise it keeps the
/// current indentation.
pub(super) fn newline_with_indent(source: &str, cursor: usize) -> String {
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let indent: String = source[line_start..byte]
        .chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .collect();
    let before = source[..byte].trim_end_matches([' ', '\t']);
    if before.ends_with('>') && !before.ends_with("/>") && is_open_tag_tail(source, byte) {
        format!("\n{indent}{}", indent_unit(source))
    } else {
        format!("\n{indent}")
    }
}

/// `</name>` when a `>` typed at `cursor` would finish the start tag `<name …`,
/// unless that end tag already follows.
pub(super) fn end_tag_for(source: &str, cursor: usize) -> Option<String> {
    let byte = utf16_to_byte_index(source, cursor);
    let before = &source[..byte];
    // Not inside a comment, CDATA section, or processing instruction.
    let unclosed = |open: &str, close: &str| {
        before.rfind(open).is_some_and(|at| !before[at..].contains(close))
    };
    if unclosed("<!--", "-->") || unclosed("<![CDATA[", "]]>") || unclosed("<?", "?>") {
        return None;
    }
    // The start tag: the last `<` outside quoted attribute values.
    let mut tag_start = None;
    let mut quote: Option<char> = None;
    for (index, character) in before.char_indices() {
        match quote {
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if character == '<' => tag_start = Some(index),
            None if character == '>' => tag_start = None,
            None if (character == '"' || character == '\'') && tag_start.is_some() => quote = Some(character),
            None => {}
        }
    }
    // A `>` typed inside an attribute value is text.
    if quote.is_some() {
        return None;
    }
    let tag_start = tag_start?;
    let tag = &before[tag_start + 1..];
    if tag.starts_with(['/', '?', '!']) || tag.trim_end().ends_with('/') {
        return None;
    }
    let name: String = tag
        .chars()
        .take_while(|character| !character.is_whitespace() && *character != '/')
        .collect();
    if name.is_empty() {
        return None;
    }
    let end_tag = format!("</{name}>");
    (!source[byte..].trim_start().starts_with(&end_tag)).then_some(end_tag)
}

/// Whether the element the end tag at the start of `after` closes was opened
/// on `line` (`<mode>balance|</mode>`): then the end tag keeps the line's
/// indentation.
pub(super) fn opens_on_line(line: &str, after: &str) -> bool {
    let name: String = after[2..]
        .chars()
        .take_while(|character| !matches!(character, '>' | ' ' | '\t' | '\n'))
        .collect();
    !name.is_empty()
        && line.match_indices(&format!("<{name}")).any(|(at, _)| {
            line[at + 1 + name.len()..].starts_with(['>', ' ', '\t', '/'])
        })
}

/// Whether the `>` before the caret closes an opening tag (not a closing,
/// self-closing, or declaration tag, and not a comment).
pub(super) fn is_open_tag_tail(source: &str, byte: usize) -> bool {
    let before = &source[..byte];
    let Some(tag_start) = before.rfind('<') else {
        return false;
    };
    let tag = &before[tag_start..];
    // `<?xml …?>`, `<!-- … -->`, and `<!DOCTYPE …>` open no element.
    !(tag.starts_with("</")
        || tag.starts_with("<?")
        || tag.starts_with("<!")
        || tag.trim_end().ends_with('/'))
}

/// Enter between `<tag>|</tag>` opens a block: the caret lands on an indented
/// middle line and the closing tag stays on its own line below.
pub(super) fn newline_insertion(source: &str, cursor: usize) -> (String, usize) {
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let indent: String = source[line_start..byte]
        .chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .collect();
    let before = source[..byte].trim_end_matches([' ', '\t']);
    let after = source[byte..].trim_start_matches([' ', '\t']);
    if before.ends_with('>')
        && !before.ends_with("/>")
        && after.starts_with("</")
        && is_open_tag_tail(source, byte)
    {
        let unit = indent_unit(source);
        // The caret sits after the newline and one unit, on the middle line.
        let caret = 1 + indent.len() + unit.len();
        (format!("\n{indent}{unit}\n{indent}"), caret)
    } else if after.starts_with("</")
        && !before.trim_end().is_empty()
        && !opens_on_line(&source[line_start..byte], after)
    {
        // An end tag moved to its own line closes the enclosing element, one
        // level out from this line's content.
        let unit = indent_unit(source);
        let outer = indent.strip_suffix(unit.as_str()).unwrap_or("");
        let text = format!("\n{outer}");
        let caret = text.len();
        (text, caret)
    } else {
        let text = newline_with_indent(source, cursor);
        let caret = text.len();
        (text, caret)
    }
}

/// Unwraps when every selected line is commented; otherwise wraps each line.
/// The line selection of `start..end` (UTF-16) widened so it holds whole
/// elements, start tags, attribute values, CDATA sections and comments.
pub(super) fn over_balanced_xml_lines(source: &str, start: usize, end: usize) -> (usize, usize) {
    use super::super::super::comment_scope::{LineState, selected_lines, selection_over, widen};
    let lines: Vec<&str> = source.split('\n').collect();
    let mut starts = Vec::with_capacity(lines.len() + 1);
    let mut lowest = Vec::with_capacity(lines.len());
    let mut depth = 0usize;
    // What is open: its closing delimiter, and whether it is a start tag.
    let mut open: Option<(&'static str, bool)> = None;
    let mut quote: Option<char> = None;
    for line in &lines {
        starts.push(LineState { depth, open: open.is_some() || quote.is_some() });
        let mut low = depth;
        let mut at = 0;
        while at < line.len() {
            let rest = &line[at..];
            let character = rest.chars().next().unwrap_or(' ');
            match (open, quote) {
                (Some(_), Some(active)) => {
                    if character == active {
                        quote = None;
                    }
                }
                (Some((_, true)), None) if matches!(character, '"' | '\'') => quote = Some(character),
                (Some(("/>", true)), None) if rest.starts_with("/>") => {
                    open = None;
                    depth = depth.saturating_sub(1);
                    at += 2;
                    continue;
                }
                (Some((closer, _)), None) if rest.starts_with(closer) => {
                    open = None;
                    at += closer.len();
                    continue;
                }
                (Some(_), None) => {}
                (None, _) if rest.starts_with("<!--") => {
                    open = Some(("-->", false));
                    at += 4;
                    continue;
                }
                (None, _) if rest.starts_with("<![CDATA[") => {
                    open = Some(("]]>", false));
                    at += 9;
                    continue;
                }
                (None, _) if rest.starts_with("</") => {
                    depth = depth.saturating_sub(1);
                    low = low.min(depth);
                    open = Some((">", false));
                    at += 2;
                    continue;
                }
                (None, _) if rest.starts_with("<?") || rest.starts_with("<!") => {
                    open = Some((">", false));
                }
                (None, _) if character == '<' => {
                    // A start tag: an element opens unless it ends `/>`.
                    depth += 1;
                    open = Some(("/>", true));
                }
                (None, _) => {}
            }
            // In a start tag, `>` ends it (the element stays open).
            if let Some(("/>", true)) = open
                && quote.is_none()
                && character == '>'
            {
                open = None;
            }
            at += character.len_utf8();
        }
        lowest.push(low);
    }
    starts.push(LineState { depth, open: open.is_some() || quote.is_some() });
    let (first, last) = selected_lines(source, start, end);
    selection_over(source, (start, end), widen(&starts, &lowest, first, last))
}

pub(super) fn toggle_xml_comments(
    source: &str,
    selection_start: usize,
    selection_end: usize,
) -> Option<(String, usize, usize)> {
    let start = utf16_to_byte_index(source, selection_start);
    let end = utf16_to_byte_index(source, selection_end);
    // Inside an existing comment spanning lines: remove that comment (a
    // comment cannot contain another).
    if let Some(open) = source[..start].rfind("<!--")
        && !source[open..start].contains("-->")
        && let Some(close) = source[end..].find("-->").map(|offset| end + offset)
        && source[open..close].contains('\n')
        // One comment from start to end, not two per-line ones.
        && !source[open + 4..close].contains("-->")
        && !source[open + 4..close].contains("<!--")
        // On lines of its own: one sharing them with elements is commented
        // line by line, or its text would become content.
        && source[source[..open].rfind('\n').map_or(0, |at| at + 1)..open].trim().is_empty()
        && source[close + 3..source[close..].find('\n').map_or(source.len(), |at| close + at)].trim().is_empty()
    {
        // Markers alone on their lines take the whole line with them.
        let line_start = |at: usize| source[..at].rfind('\n').map_or(0, |index| index + 1);
        let line_end = |at: usize| source[at..].find('\n').map_or(source.len(), |index| at + index);
        let (open, open_end) = if source[line_start(open)..open].trim().is_empty()
            && source[open + 4..line_end(open)].trim().is_empty()
        {
            (line_start(open), (line_end(open) + 1).min(source.len()))
        } else {
            (open, open + 4 + usize::from(source[open + 4..].starts_with(' ')))
        };
        let (close_start, cut_end) = if source[line_start(close)..close].trim().is_empty()
            && source[close + 3..line_end(close)].trim().is_empty()
        {
            (line_start(close), (line_end(close) + 1).min(source.len()))
        } else {
            (close - usize::from(source[..close].ends_with(' ')), close + 3)
        };
        let mut updated = String::with_capacity(source.len());
        updated.push_str(&source[..open]);
        updated.push_str(&unescape_comment_text(&source[open_end..close_start]));
        updated.push_str(&source[cut_end..]);
        // Where a position inside the comment's text lands once unwrapped.
        let map = |byte: usize| {
            let byte = byte.clamp(open_end, close_start);
            (source[..open].to_string() + &unescape_comment_text(&source[open_end..byte])).encode_utf16().count()
        };
        return Some((updated.clone(), map(start), map(end)));
    }
    // Whole elements, start tags, CDATA and comments over lines.
    let (selection_start, selection_end) = over_balanced_xml_lines(source, selection_start, selection_end);
    let (start, end) = (utf16_to_byte_index(source, selection_start), utf16_to_byte_index(source, selection_end));
    let first_line = source[..start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count();
    let newline_count = source[..end].bytes().filter(|byte| *byte == b'\n').count();
    let last_line =
        newline_count.saturating_sub(usize::from(start < end && source[..end].ends_with('\n')));

    let lines: Vec<_> = source.split_inclusive('\n').collect();
    if first_line >= lines.len() {
        return None;
    }
    let selected = &lines[first_line..=last_line.min(lines.len() - 1)];
    let uncomment = selected
        .iter()
        .filter_map(|line| {
            let body = line.trim_end_matches('\n').trim_end_matches('\r');
            let trimmed = body.trim();
            (!trimmed.is_empty()).then(|| trimmed.starts_with("<!--") && trimmed.ends_with("-->"))
        })
        .all(|commented| commented);
    // Only unwrap when every selected line is wrapped on its own line; a
    // partially commented selection gets wrapped instead.
    if !uncomment
        && selected.iter().any(|line| {
            let trimmed = line.trim_end_matches('\n').trim_end_matches('\r').trim();
            !trimmed.is_empty() && trimmed.starts_with("<!--") && trimmed.ends_with("-->")
        })
    {
        return wrap_xml_selection(source, start, end);
    }
    let mut edits: Vec<(usize, usize, usize)> = Vec::new();
    let mut offset = 0;
    let mut updated = String::with_capacity(source.len() + lines.len() * 9);

    for (line_index, line) in lines.iter().enumerate() {
        if (first_line..=last_line).contains(&line_index) {
            let body = line.trim_end_matches('\n').trim_end_matches('\r');
            if body.trim().is_empty() {
                updated.push_str(line);
            } else if uncomment {
                let trimmed = body.trim();
                let indent_len = body.len() - body.trim_start().len();
                let after_open = &trimmed[4..];
                let open = 4 + usize::from(after_open.starts_with(' '));
                let before_close = &trimmed[..trimmed.len() - 3];
                let close = 3 + usize::from(before_close.len() > open && before_close.ends_with(' '));
                let raw = &trimmed[open..trimmed.len() - close];
                let inner = unescape_comment_text(raw);
                let inner = inner.as_str();
                // The markers are separate edits, so a caret inside the line stays put.
                edits.push((offset + indent_len, open, 0));
                for (at, _) in raw.match_indices(ESCAPED_DASHES) {
                    edits.push((offset + indent_len + open + at, ESCAPED_DASHES.len(), 2));
                }
                edits.push((offset + indent_len + trimmed.len() - close, close, 0));
                updated.push_str(&line[..indent_len]);
                updated.push_str(inner);
                updated.push_str(&line[body.len()..]);
            } else {
                let indent_len = body.len() - body.trim_start().len();
                let trimmed = body.trim();
                edits.push((offset + indent_len, 0, 5));
                // Each escaped `--` grows the line (see `escape_comment_text`).
                let mut from = 0;
                while let Some(at) = trimmed[from..].find("--") {
                    edits.push((offset + indent_len + from + at, 2, ESCAPED_DASHES.len()));
                    from += at + 2;
                }
                edits.push((offset + indent_len + trimmed.len(), 0, 4));
                updated.push_str(&line[..indent_len]);
                updated.push_str("<!-- ");
                updated.push_str(&escape_comment_text(trimmed));
                updated.push_str(" -->");
                updated.push_str(&line[body.len()..]);
            }
        } else {
            updated.push_str(line);
        }
        offset += line.len();
    }

    let map_offset = |position: usize, right_biased: bool| {
        let mut delta = 0isize;
        for (edit_position, removed, inserted) in &edits {
            if position < *edit_position {
                break;
            }
            if position == *edit_position && *removed == 0 && !right_biased {
                return (*edit_position as isize + delta) as usize;
            }
            if position <= *edit_position + *removed {
                return (*edit_position as isize + delta + *inserted as isize) as usize;
            }
            delta += *inserted as isize - *removed as isize;
        }
        (position as isize + delta) as usize
    };
    let updated_start = map_offset(start, start == end).min(updated.len());
    let updated_end = map_offset(end, true).min(updated.len());
    let updated_selection_start = updated[..updated_start].encode_utf16().count();
    let updated_selection_end = updated[..updated_end].encode_utf16().count();
    Some((updated, updated_selection_start, updated_selection_end))
}

/// One `<!-- -->` pair around the selected lines (from the first one's
/// indentation to the last one's end), used when they already hold
/// commented lines that per-line wrapping would corrupt. Whole lines, so the
/// comment never starts or ends inside an element (`<c>3 --></c>`); their
/// markers are escaped (see [`escape_comment_text`]), so the result is valid
/// XML and toggling again restores them.
pub(super) fn wrap_xml_selection(source: &str, start: usize, end: usize) -> Option<(String, usize, usize)> {
    let line_start = source[..start].rfind('\n').map_or(0, |at| at + 1);
    let start = line_start + (source[line_start..].len() - source[line_start..].trim_start_matches([' ', '\t']).len());
    let end = if end > start && source[..end].ends_with('\n') { end - 1 } else { end };
    let line_end = source[end..].find('\n').map_or(source.len(), |at| end + at);
    let end = start.max(source[..line_end].trim_end().len());
    let inner = escape_comment_text(&source[start..end]);
    let mut updated = String::with_capacity(source.len() + inner.len() + 9);
    updated.push_str(&source[..start]);
    updated.push_str("<!-- ");
    updated.push_str(&inner);
    updated.push_str(" -->");
    updated.push_str(&source[end..]);
    // The selection stays on the text, inside the comment, so the next toggle
    // unwraps it.
    let selection_start = updated[..start + 5].encode_utf16().count();
    let selection_end = updated[..start + 5 + inner.len()].encode_utf16().count();
    Some((updated, selection_start, selection_end))
}

/// The escape for `--`, which a comment cannot contain: commented-out text
/// holding a comment (`<!-- c -->`) or `--` stays valid XML.
const ESCAPED_DASHES: &str = "&#45;&#45;";

/// `text` to put inside a comment the editor writes.
pub(super) fn escape_comment_text(text: &str) -> String {
    // `&` starting what the unescape reads (`&#45;`, `&amp;`) is escaped
    // too, so text already holding them comes back exactly.
    let mut escaped = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(character) = rest.chars().next() {
        if rest.starts_with("--") {
            escaped.push_str(ESCAPED_DASHES);
            rest = &rest[2..];
        } else if rest.starts_with("&#45;") || rest.starts_with("&amp;") {
            escaped.push_str("&amp;");
            rest = &rest[1..];
        } else {
            escaped.push(character);
            rest = &rest[character.len_utf8()..];
        }
    }
    escaped
}

/// The text of a comment the editor wrote, as it was before.
pub(super) fn unescape_comment_text(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(character) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix("&#45;") {
            plain.push('-');
            rest = after;
        } else if let Some(after) = rest.strip_prefix("&amp;") {
            plain.push('&');
            rest = after;
        } else {
            plain.push(character);
            rest = &rest[character.len_utf8()..];
        }
    }
    plain
}

/// The open attribute quote at the caret and whether it is in a comment:
/// the auto-pairing guard.
pub(super) fn xml_lexical_state(source: &str, cursor: usize) -> (Option<char>, bool) {
    let byte = utf16_to_byte_index(source, cursor);
    let tokens = tokenize(source);
    for token in &tokens {
        if matches!(token.kind, Kind::Comment) && token.start < byte && byte <= token.end {
            return (None, true);
        }
    }
    for token in &tokens {
        if !matches!(token.kind, Kind::OpenTag { .. }) {
            continue;
        }
        let inner = tokenize_tag_inside(source, token);
        for inner_token in &inner {
            if matches!(inner_token.kind, Kind::Text { quoted: true })
                && inner_token.start < byte
                && byte <= inner_token.end
            {
                let quote = source[inner_token.start..].chars().next();
                return (quote, false);
            }
        }
    }
    (None, false)
}


/// Colors a line: tags and their parts, comments, declarations, CDATA, and
/// element text. Whitespace between tokens and inside tags stays plain.
pub(super) fn highlight(line: &str) -> Vec<HighlightToken> {
    let mut out = Tokens::new();
    // Text words with only blanks between them: one value run, as the other
    // languages draw a value (a long text line was one span per word).
    let mut text: Option<std::ops::Range<usize>> = None;
    for token in tokenize(line) {
        let words = matches!(token.kind, Kind::Text { .. } | Kind::Equals)
            && !line[token.start..token.end].starts_with(CDATA_CLOSE);
        if words {
            match &mut text {
                Some(run) if line[run.end..token.start].trim().is_empty() => run.end = token.end,
                _ => {
                    if let Some(run) = text.replace(token.start..token.end) {
                        out.push(run.start, run.end, TokenKind::Value);
                    }
                }
            }
            continue;
        }
        if let Some(run) = text.take() {
            out.push(run.start, run.end, TokenKind::Value);
        }
        match token.kind {
            Kind::Comment => out.push(token.start, token.end, TokenKind::Comment),
            Kind::Special if line[token.start..].starts_with(CDATA_OPEN) => {
                highlight_cdata(line, token, &mut out);
            }
            Kind::Special => out.push(token.start, token.end, TokenKind::Punctuation),
            Kind::OpenTag { name_end, .. } => highlight_open_tag(line, token, name_end, &mut out),
            Kind::EndTag => highlight_end_tag(line, token, &mut out),
            // The `]]>` closing CDATA opened on a line above.
            Kind::Text { .. } if line[token.start..token.end].starts_with(CDATA_CLOSE) => {
                out.push(token.start, token.start + 3, TokenKind::Punctuation);
                out.push(token.start + 3, token.end, TokenKind::Value);
            }
            Kind::Text { .. } | Kind::Equals => {
                out.push(token.start, token.end, TokenKind::Value);
            }
        }
    }
    if let Some(run) = text {
        out.push(run.start, run.end, TokenKind::Value);
    }
    out.finish()
}

const CDATA_OPEN: &str = "<![CDATA[";
const CDATA_CLOSE: &str = "]]>";

/// `<![CDATA[text]]>`: the delimiters, and the text as a string.
fn highlight_cdata(line: &str, token: Token, out: &mut Tokens) {
    let text_start = token.start + CDATA_OPEN.len();
    let text_end = if line[..token.end].ends_with(CDATA_CLOSE) {
        token.end - CDATA_CLOSE.len()
    } else {
        token.end
    };
    out.push(token.start, text_start, TokenKind::Punctuation);
    out.push(text_start, text_end.max(text_start), TokenKind::String);
    out.push(text_end.max(text_start), token.end, TokenKind::Punctuation);
}

/// The `<!-- … -->` comments.
pub(super) fn comments(source: &str) -> Vec<std::ops::Range<usize>> {
    tokenize(source)
        .iter()
        .filter(|token| matches!(token.kind, Kind::Comment))
        .map(|token| token.start..token.end)
        .collect()
}

/// Start tags over lines (structure), the attribute values in them over
/// lines, CDATA text and other declarations over lines.
pub(super) fn multiline_constructs(source: &str) -> Vec<MultilineConstruct> {
    let spans_lines = |start: usize, end: usize| source[start..end].contains('\n');
    let mut found = Vec::new();
    for token in tokenize(source) {
        if !spans_lines(token.start, token.end) {
            continue;
        }
        match token.kind {
            Kind::OpenTag { .. } => {
                found.push(MultilineConstruct::structure(token.start..token.end));
                for part in tokenize_tag_inside(source, &token) {
                    if matches!(part.kind, Kind::Text { quoted: true }) && spans_lines(part.start, part.end) {
                        found.push(MultilineConstruct::colored(part.start..part.end, TokenKind::String));
                    }
                }
            }
            Kind::Special if source[token.start..].starts_with(CDATA_OPEN) => {
                let start = token.start + CDATA_OPEN.len();
                let end = if source[..token.end].ends_with(CDATA_CLOSE) {
                    token.end - CDATA_CLOSE.len()
                } else {
                    token.end
                };
                found.push(MultilineConstruct::colored(start..end.max(start), TokenKind::String));
            }
            Kind::Special => {
                found.push(MultilineConstruct::colored(token.start..token.end, TokenKind::Punctuation));
            }
            _ => {}
        }
    }
    found
}

/// The inside of a start tag opened on a line above: its attributes, and
/// the tag's closing `>` or `/>`.
pub(super) fn highlight_continued(text: &str) -> Vec<HighlightToken> {
    let mut out = Tokens::new();
    let inside = Token {
        start: 0,
        end: text.len(),
        kind: Kind::Text { quoted: false },
    };
    highlight_attributes(text, inside, 0, &mut out);
    out.finish()
}

/// `<name attr="value" ...>` or `<name ...` while typing. The tag-inside scan
/// does not stop at `>`, so the closing `>` usually lands in the last word
/// and is colored as a value.
pub(super) fn highlight_open_tag(line: &str, token: Token, name_end: usize, out: &mut Tokens) {
    out.push(token.start, token.start + 1, TokenKind::Punctuation);
    let name_end = name_end.min(line.len());
    out.push(token.start + 1, name_end, TokenKind::Key);
    highlight_attributes(line, token, name_end, out);
}

/// The attributes of the tag `token` (from byte `from`) and its closing `>`
/// or `/>`, which is punctuation; attribute runs stop before it.
fn highlight_attributes(line: &str, token: Token, from: usize, out: &mut Tokens) {
    let tag = &line[..token.end.min(line.len())];
    let close = if tag.ends_with("/>") {
        token.end - 2
    } else if tag.ends_with('>') {
        token.end - 1
    } else {
        token.end
    }
    .max(from);
    let inner = tokenize_tag_inside(line, &token);
    for (index, part) in inner.iter().enumerate() {
        if part.start >= close {
            break;
        }
        let kind = match part.kind {
            Kind::Text { quoted: true } => TokenKind::String,
            Kind::Equals => TokenKind::Punctuation,
            _ if inner
                .get(index + 1)
                .is_some_and(|next| matches!(next.kind, Kind::Equals)) =>
            {
                TokenKind::Key
            }
            _ => TokenKind::Value,
        };
        out.push(part.start, part.end.min(close), kind);
    }
    out.push(close, token.end, TokenKind::Punctuation);
}

/// `</name>`: the `</`, the raw name (prefix included), then the `>`.
pub(super) fn highlight_end_tag(line: &str, token: Token, out: &mut Tokens) {
    let name_start = (token.start + 2).min(token.end);
    let name_end = line[name_start..token.end]
        .find(|character: char| character.is_whitespace() || character == '>')
        .map_or(token.end, |offset| name_start + offset);
    out.push(token.start, name_start, TokenKind::Punctuation);
    out.push(name_start, name_end, TokenKind::Key);
    out.push(name_end, token.end, TokenKind::Punctuation);
}

/// The indentation of the line holding the start tag of the innermost open
/// `name` element in `before`, so a typed `</name>` lines up with it.
pub(super) fn open_element_indent(before: &str, name: &str) -> Option<String> {
    let open = open_elements(before);
    let (_, start) = open.iter().rev().find(|(open_name, _)| *open_name == name)?;
    let line_start = before[..*start].rfind('\n').map_or(0, |index| index + 1);
    Some(
        before[line_start..*start]
            .chars()
            .take_while(|character| matches!(character, ' ' | '\t'))
            .collect(),
    )
}

/// The elements still open at the end of `before`, outermost first: each
/// one's name and the byte its start tag starts at.
pub(super) fn open_elements(before: &str) -> Vec<(&str, usize)> {
    let mut open: Vec<(&str, usize)> = Vec::new();
    let mut at = 0;
    while let Some(offset) = before[at..].find('<') {
        let start = at + offset;
        let rest = &before[start..];
        let skip_to = |close: &str| rest.find(close).map(|end| start + end + close.len());
        let next = if rest.starts_with("<!--") {
            skip_to("-->")
        } else if rest.starts_with("<![CDATA[") {
            skip_to("]]>")
        } else if rest.starts_with("<?") {
            skip_to("?>")
        } else if rest.starts_with("<!") {
            skip_to(">")
        } else {
            // The tag's end, outside quoted attribute values.
            let mut quote = None;
            let end = rest.char_indices().find_map(|(index, character)| {
                match quote {
                    Some(open) if character == open => quote = None,
                    Some(_) => {}
                    None if matches!(character, '"' | '\'') => quote = Some(character),
                    None if character == '>' => return Some(index),
                    None => {}
                }
                None
            });
            match end {
                Some(end) => {
                    let tag = &rest[..=end];
                    let tag_name = tag
                        .trim_start_matches(['<', '/'])
                        .split(|character: char| character.is_whitespace() || matches!(character, '>' | '/'))
                        .next()
                        .unwrap_or("");
                    if tag.starts_with("</") {
                        if let Some(index) = open.iter().rposition(|(open_name, _)| *open_name == tag_name) {
                            open.truncate(index);
                        }
                    } else if !tag.ends_with("/>") && !tag_name.is_empty() {
                        open.push((tag_name, start));
                    }
                    Some(start + end + 1)
                }
                None => None,
            }
        };
        match next {
            Some(next) => at = next,
            None => break,
        }
    }
    open
}
