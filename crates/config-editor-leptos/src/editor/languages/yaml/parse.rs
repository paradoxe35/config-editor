//! Parsing to a JSON value, and the source spans of keys and values.

use super::*;

pub(super) fn yaml_parse(source: &str) -> Result<Value, ParseError> {
    let parser_config =
        ParserConfig::serde_yaml_compat().duplicate_key_policy(DuplicateKeyPolicy::Error);
    noyalib::from_str_with_config(source, &parser_config).map_err(|error| {
        let line_count = source.split('\n').count().max(1);
        let location = error.location();
        let line = location
            .as_ref()
            .map(|location| location.line().min(line_count));
        let column = location.as_ref().map(|location| location.column().max(1));
        let mut message = error.to_string();
        if let Some(location) = location
            && location.line() > line_count
        {
            message = message.replace(
                &format!("at line {} column {}", location.line(), location.column()),
                "at end of input",
            );
        }
        ParseError {
            message,
            line,
            column,
        }
    })
}

/// Maps every mapping key and sequence item in the document to its source line.
/// Paths are the same JSON-pointer segments the schema validator reports, so a
/// nested `listen` is never confused with a root `listen`.
pub(super) fn yaml_path_lines(source: &str) -> Vec<(Vec<String>, usize, usize, usize)> {
    let mut entries = Vec::new();
    let mut stack: Vec<(usize, Vec<String>)> = Vec::new();
    let mut counts: std::collections::HashMap<(Vec<String>, usize), usize> =
        std::collections::HashMap::new();
    for (index, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - trimmed.len();
        let mut rest = trimmed;
        let mut content_indent = indent;

        let mut array_item_path = None;
        if rest == "-" || rest.starts_with("- ") {
            while stack.last().is_some_and(|(level, _)| *level >= indent) {
                stack.pop();
            }
            let parent = stack
                .last()
                .map(|(_, path)| path.clone())
                .unwrap_or_default();
            let count = counts.entry((parent.clone(), indent)).or_default();
            let mut item_path = parent;
            item_path.push(count.to_string());
            *count += 1;
            let after_dash = rest.strip_prefix('-').unwrap_or_default();
            let content = after_dash.trim_start();
            content_indent = indent + 1 + after_dash.len() - content.len();
            array_item_path = Some(item_path.clone());
            stack.push((indent, item_path));
            rest = content;
        }

        if rest.is_empty() {
            if let Some(path) = array_item_path {
                let column = line[..content_indent].chars().count() + 1;
                entries.push((path, index + 1, column, column + 1));
            }
            continue;
        }
        let Some(colon) = rest.find(':') else {
            if let Some(path) = array_item_path {
                let column = line[..content_indent].chars().count() + 1;
                let end_column = column + rest.chars().count();
                entries.push((path, index + 1, column, end_column));
            }
            continue;
        };
        let key_raw = &rest[..colon];
        let key = key_raw.trim().trim_matches(['\'', '"']);
        if key.is_empty() {
            continue;
        }
        while stack
            .last()
            .is_some_and(|(level, _)| *level >= content_indent)
        {
            stack.pop();
        }
        let mut path = stack
            .last()
            .map(|(_, path)| path.clone())
            .unwrap_or_default();
        path.push(key.to_string());
        let column = line[..content_indent].chars().count() + 1;
        let end_column = column + key_raw.trim_end().chars().count();
        entries.push((path.clone(), index + 1, column, end_column));
        let value = rest[colon + 1..].trim();
        if value.is_empty() || value.starts_with('#') {
            stack.push((content_indent, path));
        }
    }
    entries
}

/// Span of a key written inside an inline `{ ... }` mapping. The key is found
/// on its parent mapping's line as a whole word after the parent's colon.
pub(super) fn flow_key_span(source: &str, wanted: &[String]) -> Option<(usize, usize, usize)> {
    let (key, parent) = wanted.split_last()?;
    if parent.is_empty() || key.is_empty() {
        return None;
    }
    let (_, parent_line, _, _) = yaml_path_lines(source)
        .into_iter()
        .find(|(candidate, _, _, _)| candidate == parent)?;
    let text = source.lines().nth(parent_line.checked_sub(1)?)?;
    let value_begin = text.find(':')? + 1;
    let is_word = |character: char| character.is_alphanumeric() || matches!(character, '_' | '-');
    let mut search = value_begin;
    while let Some(relative) = text[search..].find(key.as_str()) {
        let start = search + relative;
        let end = start + key.len();
        let whole_word = !text[..start].chars().next_back().is_some_and(is_word)
            && !text[end..].chars().next().is_some_and(is_word);
        if whole_word {
            let column = text[..start].chars().count() + 1;
            return Some((parent_line, column, column + key.chars().count()));
        }
        search = end;
    }
    None
}

pub(super) fn yaml_span_for_path(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let wanted = crate::editor::language::pointer_segments(path);
    let key = wanted.last()?.clone();
    if key.is_empty() {
        return None;
    }

    if let Some((_, line, column, end_column)) = yaml_path_lines(source)
        .into_iter()
        .find(|(candidate, _, _, _)| *candidate == wanted)
    {
        return Some((line, column, end_column));
    }

    if let Some(span) = flow_key_span(source, &wanted) {
        return Some(span);
    }

    // Last resort for paths the scanner cannot represent: first line with this key.
    source.lines().enumerate().find_map(|(index, line)| {
        let indent = line.len() - line.trim_start().len();
        let (candidate, _) = line.trim_start().split_once(':')?;
        if candidate.trim_matches(['\'', '"']) != key {
            return None;
        }
        let candidate_offset = line[indent..].find(candidate)?;
        let column = line[..indent].chars().count()
            + line[indent..indent + candidate_offset].chars().count()
            + 1;
        Some((index + 1, column, column + candidate.chars().count()))
    })
}

pub(super) fn yaml_value_span_for_path(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let wanted = crate::editor::language::pointer_segments(path);
    if let Some(span) = sequence_item_span(source, &wanted) {
        return Some(span);
    }
    // Only block-structured entries have a value slot on their line. Inline
    // mapping keys resolve through `yaml_span_for_path`, which points at the key.
    let line_number = yaml_path_lines(source)
        .into_iter()
        .find(|(candidate, _, _, _)| *candidate == wanted)
        .map(|(_, line, _, _)| line)?;
    let line = source.lines().nth(line_number.saturating_sub(1))?;
    // The value ends before a trailing `# comment`.
    let line = line_comment_start(line).map_or(line, |start| &line[..start]);
    let (key, raw_value) = line.split_once(':')?;
    let value_start = raw_value.len() - raw_value.trim_start().len();
    let value = raw_value.trim();
    let column = key.chars().count() + raw_value[..value_start].chars().count() + 2;
    if value.is_empty() || value.starts_with('#') {
        return Some((line_number, column, column + 1));
    }
    // A block scalar (`|`, `>-`) runs through its indented lines below; the
    // span's end column counts on past this line, as a heredoc's does.
    if value.starts_with(['|', '>']) {
        let full_line = source.lines().nth(line_number - 1)?;
        let key_indent = full_line.len() - full_line.trim_start().len();
        let mut end_chars = full_line.chars().count() + 1;
        let mut text_end = full_line.chars().count();
        for next in source.lines().skip(line_number) {
            let indent = next.len() - next.trim_start().len();
            if !next.trim().is_empty() && indent <= key_indent {
                break;
            }
            end_chars += next.chars().count();
            if !next.trim().is_empty() {
                text_end = end_chars;
            }
            end_chars += 1;
        }
        return Some((line_number, column, text_end + 1));
    }

    Some((line_number, column, column + value.chars().count()))
}

/// The value of block sequence item `wanted` (`.../certificates/0`): the text
/// after its `- `, found under the parent key's line.
pub(super) fn sequence_item_span(source: &str, wanted: &[String]) -> Option<(usize, usize, usize)> {
    let (last, parent) = wanted.split_last()?;
    let index: usize = last.parse().ok()?;
    let parent_line = if parent.is_empty() {
        0
    } else {
        yaml_path_lines(source)
            .into_iter()
            .find(|(candidate, _, _, _)| candidate.as_slice() == parent)
            .map(|(_, line, _, _)| line)?
    };
    let lines: Vec<&str> = source.lines().collect();
    let parent_indent = lines
        .get(parent_line.wrapping_sub(1))
        .map_or(0, |line| line.len() - line.trim_start().len());
    let mut dash_indent = None;
    let mut seen = 0;
    for (number, line) in lines.iter().enumerate().skip(parent_line) {
        let content = line.trim_start();
        if content.is_empty() || content.starts_with('#') {
            continue;
        }
        let indent = line.len() - content.len();
        if parent_line > 0 && indent <= parent_indent && !content.starts_with('-') {
            break;
        }
        if !(content == "-" || content.starts_with("- ")) {
            continue;
        }
        match dash_indent {
            None => dash_indent = Some(indent),
            Some(level) if indent < level => break,
            Some(level) if indent > level => continue,
            Some(_) => {}
        }
        if seen == index {
            let value = content[1..].trim_start();
            let value = line_comment_start(value).map_or(value, |start| value[..start].trim_end());
            let column = line[..line.len() - content[1..].trim_start().len()].chars().count() + 1;
            return Some((number + 1, column, column + value.chars().count().max(1)));
        }
        seen += 1;
    }
    None
}

pub(super) fn first_yaml_span(source: &str) -> Option<(usize, usize, usize)> {
    source.lines().enumerate().find_map(|(index, line)| {
        let start = line.find(|character: char| !character.is_whitespace())?;
        Some((
            index + 1,
            line[..start].chars().count() + 1,
            line.chars().count() + 1,
        ))
    })
}
