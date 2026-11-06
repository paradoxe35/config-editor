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
pub(super) fn fold_dotted_tables(rendered: &str, dotted: &HashSet<Dotted>) -> String {
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
    let mut index = 0;
    while index < sections.len() {
        let Some((_, segments, false)) = sections[index].0.clone() else {
            index += 1;
            continue;
        };
        // The shortest dotted prefix that covers this table: its keys go to
        // that prefix's table, written under the whole remaining path.
        let target = (0..segments.len()).find_map(|split| {
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
                    serde_json::to_string(segment).unwrap_or_else(|_| format!("\"{segment}\""))
                }
            })
            .collect::<Vec<_>>()
            .join(".");
        let mut lines: Vec<String> = body
            .into_iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| format!("{quoted}.{}", line.trim_start()))
            .collect();
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
        target_body.extend(lines);
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
