//! Where each key and value is written. One walk of the lines finds the
//! spans of the paths asked for (and stops once each is found); while a
//! batch runs for the text (an editor's kept spans, a batch of problems)
//! the walk runs once for every path instead, and each lookup, found or
//! not, is one look in what it found.

use super::*;
use crate::editor::paths::{batch_cached, in_batch};
use crate::editor::work_count::{Work, note};

/// A span: line, first and end character columns.
type At = (usize, usize, usize);

/// Where a walk puts what it finds.
trait Sink {
    /// Whether every path wanted is found (the walk can stop).
    fn done(&self) -> bool;
    /// Whether `path` is wanted and not found yet.
    fn wants(&self, path: &[String]) -> bool;
    /// Whether a path strictly below `path` is wanted.
    fn below(&self, path: &[String]) -> bool;
    /// `path`'s span, if `span` gives one and none was found before.
    fn set(&mut self, path: &[String], span: impl Fn() -> Option<At>);
}

/// The paths a batch of lookups wants, indexed: where each is, which are
/// below a path; and the spans found for them, each kept from the first
/// line it is found on.
struct Wanted {
    /// Each path's positions in the list asked for.
    exact: HashMap<Vec<String>, Vec<usize>>,
    /// Every path some wanted one is strictly below.
    above: HashSet<Vec<String>>,
    spans: Vec<Option<At>>,
    missing: usize,
}

impl Wanted {
    fn new(paths: &[String]) -> Self {
        let mut exact: HashMap<Vec<String>, Vec<usize>> = HashMap::new();
        let mut above = HashSet::new();
        for (at, path) in paths.iter().enumerate() {
            let segments = pointer_segments(path);
            for end in 0..segments.len() {
                above.insert(segments[..end].to_vec());
            }
            exact.entry(segments).or_default().push(at);
        }
        Self { exact, above, spans: vec![None; paths.len()], missing: paths.len() }
    }
}

impl Sink for Wanted {
    fn done(&self) -> bool {
        self.missing == 0
    }

    fn wants(&self, path: &[String]) -> bool {
        self.exact.get(path).is_some_and(|at| at.iter().any(|at| self.spans[*at].is_none()))
    }

    fn below(&self, path: &[String]) -> bool {
        self.above.contains(path)
    }

    fn set(&mut self, path: &[String], span: impl Fn() -> Option<At>) {
        let Some(at) = self.exact.get(path) else {
            return;
        };
        for at in at {
            if self.spans[*at].is_none() {
                self.spans[*at] = span();
                if self.spans[*at].is_some() {
                    self.missing -= 1;
                }
            }
        }
    }
}

/// Every path's span, the first found for each.
#[derive(Default)]
struct Every(HashMap<Vec<String>, At>);

impl Sink for Every {
    fn done(&self) -> bool {
        false
    }

    fn wants(&self, path: &[String]) -> bool {
        !self.0.contains_key(path)
    }

    fn below(&self, _: &[String]) -> bool {
        true
    }

    fn set(&mut self, path: &[String], span: impl Fn() -> Option<At>) {
        if !self.0.contains_key(path)
            && let Some(span) = span()
        {
            self.0.insert(path.to_vec(), span);
        }
    }
}

/// Every key's span and every value's of one text, read once while a batch
/// runs for it.
struct KeyIndex(HashMap<Vec<String>, At>);
struct ValueIndex(HashMap<Vec<String>, At>);

pub(super) fn toml_key_span(source: &str, path: &str) -> Option<At> {
    toml_key_spans(source, &[path.to_string()]).pop().flatten()
}

/// [`toml_key_span`] for each of `paths`, reading the document once: each
/// line's key looked up among the paths wanted, and the reading stops once
/// each is found.
pub(super) fn toml_key_spans(source: &str, paths: &[String]) -> Vec<Option<At>> {
    if in_batch(source) {
        let index = batch_cached(source, || {
            let mut every = Every::default();
            walk_keys(source, &mut every);
            KeyIndex(every.0)
        });
        return paths.iter().map(|path| index.0.get(&pointer_segments(path)).copied()).collect();
    }
    let mut wanted = Wanted::new(paths);
    walk_keys(source, &mut wanted);
    wanted.spans
}

pub(super) fn toml_value_span(source: &str, path: &str) -> Option<At> {
    toml_value_spans(source, &[path.to_string()]).pop().flatten()
}

/// [`toml_value_span`] for each of `paths`, reading the document once: a
/// line is looked at only while it is in a table some path wanted lies
/// under, and the reading stops once each is found.
pub(super) fn toml_value_spans(source: &str, paths: &[String]) -> Vec<Option<At>> {
    if in_batch(source) {
        let index = batch_cached(source, || {
            let mut every = Every::default();
            walk_values(source, &mut every);
            ValueIndex(every.0)
        });
        return paths.iter().map(|path| index.0.get(&pointer_segments(path)).copied()).collect();
    }
    let mut wanted = Wanted::new(paths);
    walk_values(source, &mut wanted);
    wanted.spans
}

fn walk_keys(source: &str, sink: &mut impl Sink) {
    note(Work::DocumentRead);
    let lines: Vec<&str> = source.lines().collect();
    let tables = table_paths(&lines);
    // The containers written over lines, read once when one is met.
    let mut statements = None;
    let mut offset = 0;
    for (index, line) in lines.iter().enumerate() {
        if sink.done() {
            break;
        }
        let line_offset = offset;
        offset += line.len() + 1;
        let (code, _) = split_toml_comment(line);
        if let Some((key, start, end)) = key_assignment(code) {
            let table = path_strings(&tables[index]);
            let mut full = table.clone();
            full.extend(split_key_path(&key).unwrap_or_else(|| vec![key.clone()]));
            if sink.wants(&full) {
                // A dotted key (`websocket.bogus`) names its last segment.
                let start = start + last_segment_start(&code[start..end]);
                sink.set(&full, || Some(span_between(source, line_offset + start, line_offset + end)));
            }
            // A dotted key below the path (`bogus.deep` for `/limits/bogus`)
            // names it with one of its segments.
            for length in table.len() + 1..full.len() {
                if sink.wants(&full[..length]) {
                    sink.set(&full[..length], || {
                        let (from, to) = segment_range(&code[start..end], length - table.len() - 1)?;
                        Some(span_between(source, line_offset + start + from, line_offset + start + to))
                    });
                }
            }
            // `key = { nested = … }` keeps every nested key on this line.
            if sink.below(&full) {
                for entry in inline_entries_on(code, &full) {
                    if sink.wants(&entry.path) {
                        sink.set(&entry.path, || Some(span_between(source, line_offset + entry.key.0, line_offset + entry.key.1)));
                    }
                }
                let end = container_end(source, code, line_offset, &mut statements);
                for (path, key) in listed_keys(source, line_offset, end) {
                    let mut entry = full.clone();
                    entry.extend(path);
                    if sink.wants(&entry) {
                        sink.set(&entry, || Some(span_between(source, line_offset + key.start, line_offset + key.end)));
                    }
                }
            }
        }
        if let Some((segments, is_array)) = parse_header(code)
            && !segments.is_empty()
        {
            let header_start = line.find('[').unwrap_or(0);
            let header = || Some(span_between(source, line_offset + header_start, line_offset + line.len()));
            let full = &tables[index + 1];
            // A `[table]` header names the table itself.
            let table = path_strings(full);
            if sink.wants(&table) {
                sink.set(&table, header);
            }
            // And what it is below: the array of an `[[array]]` header, by
            // the header; a table it opens implicitly (`a` and `a.b` of
            // `[a.b.c]`), by its segment.
            let inner_start = header_start + if is_array { 2 } else { 1 };
            let closing = if is_array { "]]" } else { "]" };
            let inner = code.trim_end().strip_suffix(closing).and_then(|code| code.get(inner_start..)).unwrap_or("");
            let mut keys = 0;
            for cut in 1..full.len() {
                let SchemaPathSegment::Key(_) = full[cut - 1] else {
                    continue;
                };
                keys += 1;
                let above = &table[..cut];
                if !sink.wants(above) {
                    continue;
                }
                if is_array && cut + 1 == full.len() {
                    sink.set(above, header);
                } else {
                    sink.set(above, || {
                        let (from, to) = segment_range(inner, keys - 1)?;
                        Some(span_between(source, line_offset + inner_start + from, line_offset + inner_start + to))
                    });
                }
            }
        }
    }
}

/// Where the container that is the value on the line `code` (at
/// `line_offset` in `source`) ends: on the line, or past the line that
/// closes it (`statements`, the text's containers over lines, read the
/// first time one is asked for). `None` when the value is no container.
fn container_end(
    source: &str,
    code: &str,
    line_offset: usize,
    statements: &mut Option<Vec<(usize, usize)>>,
) -> Option<usize> {
    let (start, _) = value_span_in_line(code)?;
    if !code[start..].starts_with(['[', '{']) {
        return None;
    }
    if super::super::inline::unclosed_brackets(code) == 0 {
        return Some(line_offset + code.len());
    }
    let statements = statements.get_or_insert_with(|| super::super::inline::statements(source));
    let at = statements.partition_point(|(start, _)| *start < line_offset);
    statements
        .get(at)
        .filter(|(start, _)| *start == line_offset)
        .map(|(_, end)| *end)
}

/// The keys inside the container from `line_offset` to `end` (a list's
/// items' keys, `a = [{ b = 1 }]`, and keys on the lines after the one
/// opening it), each with its path below the line's key and its byte
/// range from `line_offset`.
fn listed_keys(
    source: &str,
    line_offset: usize,
    end: Option<usize>,
) -> Vec<(Vec<String>, super::super::inline::InlineKey)> {
    let Some(text) = end.and_then(|end| source.get(line_offset..end)) else {
        return Vec::new();
    };
    // Paths from the line's own key (the first), which the caller gives
    // in full.
    super::super::inline::inline_key_paths(text, b'=')
        .into_iter()
        .filter(|(path, _)| path.len() > 1 && matches!(path[0], SchemaPathSegment::Key(_)))
        .map(|(path, key)| (path_strings(&path[1..]), key))
        .collect()
}

fn walk_values(source: &str, sink: &mut impl Sink) {
    note(Work::DocumentRead);
    let mut tables = Tables::default();
    let mut table: Vec<String> = Vec::new();
    let mut offset = 0;
    for line in source.lines() {
        if sink.done() {
            break;
        }
        let line_offset = offset;
        offset += line.len() + 1;
        if line.trim_start().starts_with('[') {
            let (code, _) = split_toml_comment(line);
            if let Some((segments, is_array)) = parse_header(code) {
                tables.enter(&segments, is_array);
                table = path_strings(&tables.path);
                continue;
            }
        }
        if !sink.below(&table) && !sink.wants(&table) {
            continue;
        }
        let (code, _) = split_toml_comment(line);
        let Some((key, _, _)) = key_assignment(code) else {
            continue;
        };
        let mut full = table.clone();
        full.extend(split_key_path(&key).unwrap_or_else(|| vec![key.clone()]));
        if sink.wants(&full) {
            sink.set(&full, || {
                let (start, end) = value_span_in_line(code)?;
                Some(value_span_at(source, code, line_offset, start, end))
            });
        }
        // A value nested in an inline table there.
        if sink.below(&full) {
            for entry in inline_entries_on(code, &full) {
                if !sink.wants(&entry.path) {
                    continue;
                }
                let (start, end) = entry.value;
                sink.set(&entry.path, || {
                    if start >= end && !code[start..].trim_start().starts_with(['{', '[']) {
                        return None;
                    }
                    Some(value_span_at(source, code, line_offset, start, end))
                });
            }
        }
    }
}
