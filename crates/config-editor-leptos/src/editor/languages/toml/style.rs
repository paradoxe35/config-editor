//! Writing in the host's style: tables the host wrote as dotted keys
//! (`clients.acme.secret = "a"`) are written as dotted keys again, so an
//! Edit-as save can patch the host line by line and keep its comments.

use super::*;
use std::collections::HashSet;

/// A section of rendered TOML: its header (line, segments, array of tables),
/// none for the top level, and its body lines.
type Section = (Option<(String, Vec<String>, bool)>, Vec<String>);

/// A table written as dotted keys: in `table` (a header's segments, empty for
/// the top level), the keys under `prefix`.
type Dotted = (Vec<String>, Vec<String>);

/// The dotted-key tables of `previous`. Arrays of tables are left alone.
pub(super) fn dotted_tables(previous: &str) -> HashSet<Dotted> {
    let mut found = HashSet::new();
    let mut table: Option<Vec<String>> = Some(Vec::new());
    for line in previous.lines() {
        let (code, _) = split_toml_comment(line);
        if let Some((segments, is_array)) = parse_header(code) {
            table = (!is_array).then_some(segments);
            continue;
        }
        let (Some(table), Some((key, _, _))) = (&table, key_assignment(code)) else {
            continue;
        };
        if let Some(mut segments) = split_key_path(&key)
            && segments.len() > 1
        {
            segments.pop();
            // Every prefix is dotted too: `a.b.c = 1` writes `a` and `a.b`.
            for end in 1..=segments.len() {
                found.insert((table.clone(), segments[..end].to_vec()));
            }
        }
    }
    found
}

/// `rendered` (as `toml` writes it) with each `[table]` the host wrote as
/// dotted keys folded back into its parent table as `prefix.key = value`.
pub(super) fn fold_dotted_tables(rendered: &str, dotted: &HashSet<Dotted>, previous: &str) -> String {
    if dotted.is_empty() {
        return rendered.to_string();
    }
    // Sections: the top-level body, then each header with its body.
    let mut sections: Vec<Section> = vec![(None, Vec::new())];
    for line in rendered.lines() {
        match parse_header(line) {
            Some((segments, is_array)) => {
                sections.push((Some((line.to_string(), segments, is_array)), Vec::new()));
            }
            None => sections.last_mut().map_or((), |(_, body)| body.push(line.to_string())),
        }
    }
    let table_of = |header: &Option<(String, Vec<String>, bool)>| -> Option<Vec<String>> {
        match header {
            None => Some(Vec::new()),
            Some((_, segments, false)) => Some(segments.clone()),
            Some((_, _, true)) => None,
        }
    };
    // Tables the host writes under a header of their own stay that way,
    // even when a parent's other keys are dotted.
    let host_headers: HashSet<Vec<String>> = previous
        .lines()
        .filter_map(|line| parse_header(split_toml_comment(line).0))
        .filter_map(|(segments, is_array)| (!is_array).then_some(segments))
        .collect();
    let mut index = 0;
    while index < sections.len() {
        let Some((_, segments, false)) = sections[index].0.clone() else {
            index += 1;
            continue;
        };
        if host_headers.contains(&segments) {
            index += 1;
            continue;
        }
        // The deepest table holding a dotted prefix that covers this table:
        // its keys go there, written under the remaining path (never to a
        // shallower table, which would define a headed table twice).
        let target = (0..segments.len()).rev().find_map(|split| {
            let (table, rest) = segments.split_at(split);
            (1..=rest.len())
                .any(|end| dotted.contains(&(table.to_vec(), rest[..end].to_vec())))
                .then(|| (table.to_vec(), rest.to_vec()))
        });
        let Some((table, prefix)) = target else {
            index += 1;
            continue;
        };
        let (_, body) = sections.remove(index);
        let quoted = prefix
            .iter()
            .map(|segment| {
                let bare = !segment.is_empty()
                    && segment
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'));
                if bare {
                    segment.clone()
                } else {
                    super::suggestions::basic_string(segment)
                }
            })
            .collect::<Vec<_>>()
            .join(".");
        // Only lines starting a key get the prefix: the rest of a multi-line
        // string (`"""` … `"""`) is the string's own text.
        let mut lines: Vec<String> = Vec::new();
        let mut open: Option<&str> = None;
        for line in body {
            match open {
                Some(delimiter) => {
                    if line.contains(delimiter) {
                        open = None;
                    }
                    lines.push(line);
                }
                None if line.trim().is_empty() => {}
                None => {
                    open = ["\"\"\"", "'''"]
                        .into_iter()
                        .find(|delimiter| line.matches(delimiter).count() % 2 == 1);
                    lines.push(format!("{quoted}.{}", line.trim_start()));
                }
            }
        }
        // An emptied table has no keys to carry it: it is written empty.
        if lines.is_empty() {
            lines.push(format!("{quoted} = {{}}"));
        }
        let position = sections
            .iter()
            .position(|(header, _)| table_of(header).as_ref() == Some(&table));
        let at = match position {
            Some(at) => at,
            None => {
                let header = format!("[{}]", table.join("."));
                sections.insert(index, (Some((header, table.clone(), false)), Vec::new()));
                index
            }
        };
        let separate = at != 0 || sections.len() > 1;
        let target_body = &mut sections[at].1;
        while target_body.last().is_some_and(|line| line.trim().is_empty()) {
            target_body.pop();
        }
        // Where the host wrote the group among its table's keys (the writer
        // puts plain keys first): before the first key the host wrote later.
        let at_line = host_order(previous, &table, &prefix).and_then(|group| {
            target_body.iter().position(|line| {
                let (code, _) = split_toml_comment(line);
                key_assignment(code)
                    .and_then(|(key, _, _)| split_key_path(&key))
                    .and_then(|segments| host_order(previous, &table, &segments))
                    .is_some_and(|position| position > group)
            })
        });
        match at_line {
            Some(index) => {
                target_body.splice(index..index, lines);
            }
            None => target_body.extend(lines),
        }
        if separate {
            target_body.push(String::new());
        }
        // Folding may have moved sections; restart from the top.
        index = 0;
    }
    let mut out = String::new();
    for (header, body) in &sections {
        if let Some((line, _, _)) = header {
            out.push_str(line);
            out.push('\n');
        }
        for line in body {
            out.push_str(line);
            out.push('\n');
        }
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

/// The edits renaming the key at `pointer` `new_key` wherever TOML writes
/// it: in `[table]` headers and in every dotted key through it
/// (`web.http`, `web.https`). `None` when it is written nowhere like that
/// (an inline table's key), or under an array of tables.
pub(super) fn toml_key_renames(source: &str, pointer: &str, new_key: &str) -> Option<Vec<crate::editor::language::KeyEdit>> {
    let wanted = pointer_segments(pointer);
    if wanted[..wanted.len().saturating_sub(1)].iter().any(|segment| crate::editor::language::pointer_index(segment).is_some()) {
        return None;
    }
    let edits = toml_key_occurrences(source, &wanted);
    if edits.is_empty() {
        return None;
    }
    Some(
        edits
            .into_iter()
            .map(|(start, end)| {
                // A bare new key in the host's quotes.
                let quoted = source[start..end].starts_with(['"', '\'']) && !new_key.starts_with(['"', '\'']);
                (start..end, if quoted { format!("\"{new_key}\"") } else { new_key.to_string() })
            })
            .collect(),
    )
}

/// Where `source` writes each key path's last segment (keys only): in
/// `[table]` headers and in every dotted key through it, in document order.
struct KeyPlaces(std::collections::HashMap<Vec<String>, Vec<(usize, usize)>>);

/// The byte ranges where `source` writes the last segment of the key path
/// `wanted` (keys only): in `[table]` headers and in every dotted key
/// through it, in document order. The text is scanned once per batch.
pub(super) fn toml_key_occurrences(source: &str, wanted: &[String]) -> Vec<(usize, usize)> {
    let places = crate::editor::paths::batch_cached_as(crate::editor::work_count::Work::KeyScan, source, || key_places(source));
    places.0.get(wanted).cloned().unwrap_or_default()
}

fn key_places(source: &str) -> KeyPlaces {
    let mut found: std::collections::HashMap<Vec<String>, Vec<(usize, usize)>> = std::collections::HashMap::new();
    let mut table: Vec<String> = Vec::new();
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        let (code, _) = split_toml_comment(line.trim_end_matches('\n'));
        if let Some((segments, _)) = parse_header(code) {
            if let Some(open) = code.find(|character: char| character != '[' && !character.is_whitespace())
                && let Some(close) = code.rfind(|character: char| character != ']' && !character.is_whitespace())
            {
                for depth in 0..segments.len() {
                    if let Some((start, end)) = segment_range(&code[open..close + 1], depth) {
                        found.entry(segments[..=depth].to_vec()).or_default().push((offset + open + start, offset + open + end));
                    }
                }
            }
            table = segments;
        } else if let Some((key, start, end)) = key_assignment(code) {
            let mut full = table.clone();
            full.extend(split_key_path(&key).unwrap_or_else(|| vec![key.clone()]));
            for depth in table.len()..full.len() {
                if let Some((from, to)) = segment_range(&code[start..end], depth - table.len()) {
                    found.entry(full[..=depth].to_vec()).or_default().push((offset + start + from, offset + start + to));
                }
            }
        }
        offset += line.len();
    }
    KeyPlaces(found)
}

/// The line where `previous` first writes a key whose path (in `table`, a
/// header's segments, empty for the top level) starts with `path`.
fn host_order(previous: &str, table: &[String], path: &[String]) -> Option<usize> {
    let mut current: Option<Vec<String>> = Some(Vec::new());
    for (index, line) in previous.lines().enumerate() {
        let (code, _) = split_toml_comment(line);
        if let Some((segments, is_array)) = parse_header(code) {
            current = (!is_array).then_some(segments);
            continue;
        }
        if current.as_deref() == Some(table)
            && let Some((key, _, _)) = key_assignment(code)
            && split_key_path(&key).is_some_and(|segments| segments.starts_with(path))
        {
            return Some(index);
        }
    }
    None
}

/// A table header's segments, and whether it names an array of tables.
type Header = (Vec<String>, bool);

/// `rendered` with its tables in the order `previous` writes them: the
/// writer puts a table before its arrays of tables, a host may not
/// (`[[a.b]]` then `[a]`). Tables the host lacks follow the one before them.
pub(super) fn order_tables_like(rendered: &str, previous: &str) -> String {
    let host: Vec<(Vec<String>, bool)> = previous
        .lines()
        .filter_map(|line| parse_header(split_toml_comment(line).0))
        .collect();
    if host.is_empty() {
        return rendered.to_string();
    }
    let mut sections: Vec<(Option<Header>, Vec<&str>)> = vec![(None, Vec::new())];
    for line in rendered.lines() {
        match parse_header(line) {
            Some(header) => sections.push((Some(header), vec![line])),
            None => sections.last_mut().map_or((), |(_, body)| body.push(line)),
        }
    }
    let mut anchor = 0usize;
    let mut keyed: Vec<((usize, usize), Vec<&str>)> = Vec::new();
    for (index, (header, body)) in sections.into_iter().enumerate() {
        if let Some(header) = &header
            && let Some(at) = host.iter().position(|known| known == header)
        {
            anchor = at + 1;
        }
        keyed.push(((if header.is_none() { 0 } else { anchor }, index), body));
    }
    keyed.sort_by_key(|(key, _)| *key);
    let mut out: Vec<&str> = Vec::new();
    for (_, body) in keyed {
        // A table moved to the end loses the blank line it had before.
        if !out.is_empty() && body.first().is_some_and(|line| parse_header(line).is_some()) && out.last().is_some_and(|line| !line.is_empty()) {
            out.push("");
        }
        out.extend(body);
    }
    while out.last().is_some_and(|line| line.is_empty()) {
        out.pop();
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}
