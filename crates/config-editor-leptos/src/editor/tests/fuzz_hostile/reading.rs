//! Every reading of a hostile text, in every language and the custom and
//! declared ones: parse, the syntax and schema problems, highlighting (and
//! a long row's pieces), the constructs over lines, folds, the hover, the
//! suggestions and what they replace, Enter and the typing hooks, comment
//! toggling, the paths and the fields' places, and a conversion. None
//! panics or runs past its cap; every place they report is inside the text
//! and on a character; tokens are in order, apart and inside their line,
//! and a long row's pieces give the row back exactly once.

use super::runner::{Cap, Case, Failure, Rng, cases, run};
use super::texts::{NAMES, hostile, language, literal, shown, smaller};
use crate::TextPosition;
use crate::editor::completion::{completions, schema_hover, schema_suggestions};
use crate::editor::diagnostics::{Diagnostic, check_syntax, validate};
use crate::editor::highlight::pieces::{RowPieces, splits};
use crate::editor::highlight::spanning::{Spanning, tokens_after};
use crate::editor::highlight::{Token, runs_of};
use crate::editor::language::Span;
use crate::editor::languages::Language;
use crate::editor::line_endings::editor_text;
use crate::editor::paths::FieldPath;

#[derive(Clone, Debug)]
pub(super) struct ReadCase {
    pub(super) language: &'static str,
    pub(super) text: String,
    /// Where the places read at one caret are taken from.
    pub(super) at: u32,
}

impl Case for ReadCase {
    fn code(&self) -> String {
        format!(
            "#[test]\nfn reading_repro() {{\n    super::reading::replay(super::reading::ReadCase {{\n        language: {:?},\n        text: {}.into(),\n        at: {},\n    }});\n}}",
            self.language,
            literal(&self.text),
            self.at,
        )
    }

    fn shrinks(&self) -> Vec<Self> {
        let mut out: Vec<Self> = smaller(&self.text)
            .into_iter()
            .map(|text| Self {
                text,
                ..self.clone()
            })
            .collect();
        if self.at > 0 {
            out.push(Self {
                at: 0,
                ..self.clone()
            });
        }
        out
    }

    fn size(&self) -> usize {
        self.text.chars().count() * 2 + usize::from(self.at > 0)
    }
}

fn generate(rng: &mut Rng) -> Option<ReadCase> {
    let language = *rng.pick(&NAMES);
    let text = hostile(rng, language);
    Some(ReadCase {
        language,
        text,
        at: rng.position(),
    })
}

/// The lines of `text` as the API counts them: LF, CRLF and a lone CR
/// each end one.
fn api_lines(text: &str) -> Vec<String> {
    crate::editor::line_breaks::as_lf(text)
        .split('\n')
        .map(str::to_string)
        .collect()
}

/// UTF-16 units in `text`.
fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

/// Whether UTF-16 `offset` is on a character of `text`.
fn on_character(text: &str, offset: usize) -> bool {
    let mut at = 0;
    for character in text.chars() {
        if at >= offset {
            break;
        }
        at += character.len_utf16();
    }
    at == offset
}

/// The caret places read: the text's ends and some spread over it, each
/// on a character (UTF-16).
fn places(text: &str, at: u32) -> Vec<usize> {
    let mut starts = vec![0];
    let mut units = 0;
    for character in text.chars() {
        units += character.len_utf16();
        starts.push(units);
    }
    let count = starts.len();
    let mut out = vec![0, units];
    let mut seed = u64::from(at) | 1;
    for _ in 0..10 {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        out.push(starts[(seed >> 33) as usize % count]);
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// A failure of `what` in `name`.
fn fail(what: &str, name: &str, detail: String) -> Result<(), Failure> {
    Err(Failure::new(format!("{what} ({name})"), detail))
}

/// Why `tokens` are not in order, apart, and inside `line` on characters.
fn bad_tokens(line: &str, tokens: &[Token]) -> Option<String> {
    let mut at = 0;
    for token in tokens {
        if token.start < at
            || token.start >= token.end
            || token.end > line.len()
            || !line.is_char_boundary(token.start)
            || !line.is_char_boundary(token.end)
        {
            return Some(format!("{token:?} after byte {at} of {}", shown(line)));
        }
        at = token.end;
    }
    None
}

/// Whether a one-based `line` and `column` are a place of `lines` (a
/// column just past a line's end included).
fn in_lines(lines: &[String], line: usize, column: usize) -> bool {
    line >= 1
        && column >= 1
        && lines
            .get(line - 1)
            .is_some_and(|text| column <= text.chars().count() + 1)
}

/// Whether a marker from `start` to `end` on one line is a place of
/// `lines`: inside it, or the one character a marker at the line's end
/// covers past it (an error at the end of a line, an empty value, has a
/// marker one wide there, so it shows).
fn marker_fits(lines: &[String], line: usize, start: usize, end: usize) -> bool {
    in_lines(lines, line, start) && (in_lines(lines, line, end) || end == start + 1)
}

fn bad_diagnostic(lines: &[String], problem: &Diagnostic) -> bool {
    let (Some(line), Some(column)) = (problem.line, problem.column) else {
        return false;
    };
    let last = problem.end_line.unwrap_or(line);
    let end = problem.end_column.unwrap_or(column);
    if last == line {
        return !marker_fits(lines, line, column, end) || end < column;
    }
    !in_lines(lines, line, column) || !in_lines(lines, last, end) || last < line
}

/// A span is on one line, its end counted on from its start in
/// characters: a value written over lines (a block scalar) ends past its
/// first line, inside the text.
fn bad_span(lines: &[String], span: &Span) -> bool {
    if span.end_column < span.start_column || !in_lines(lines, span.line, span.start_column) {
        return true;
    }
    let before: usize = lines[..span.line - 1]
        .iter()
        .map(|line| line.chars().count() + 1)
        .sum();
    let total: usize = lines
        .iter()
        .map(|line| line.chars().count() + 1)
        .sum::<usize>()
        - 1;
    !marker_fits(lines, span.line, span.start_column, span.end_column)
        && before + span.end_column - 1 > total
}

/// Why byte `ranges` are not inside `text` on characters (and, when
/// `ordered`, in order).
fn bad_ranges(text: &str, ranges: &[std::ops::Range<usize>], ordered: bool) -> Option<String> {
    let mut at = 0;
    for range in ranges {
        if range.start > range.end
            || range.end > text.len()
            || !text.is_char_boundary(range.start)
            || !text.is_char_boundary(range.end)
            || (ordered && range.start < at)
        {
            return Some(format!("{range:?} in {} ({ranges:?})", shown(text)));
        }
        at = range.start;
    }
    None
}

/// JSON pointers of `value`, the first few in document order.
fn pointers(value: &serde_json::Value, prefix: String, out: &mut Vec<String>) {
    if out.len() >= 60 {
        return;
    }
    out.push(prefix.clone());
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                let key = key.replace('~', "~0").replace('/', "~1");
                pointers(value, format!("{prefix}/{key}"), out);
            }
        }
        serde_json::Value::Array(items) => {
            for (index, value) in items.iter().enumerate() {
                pointers(value, format!("{prefix}/{index}"), out);
            }
        }
        _ => {}
    }
}

pub(super) fn check(case: &ReadCase) -> Result<(), Failure> {
    let name = case.language;
    let language = language(name);
    let behavior = language.behavior();
    let raw = case.text.as_str();
    // The text as the editor holds it: LF breaks, no leading mark.
    let text = editor_text(raw.to_string());
    let lines: Vec<&str> = text.split('\n').collect();
    let owned_lines: Vec<String> = lines.iter().map(|line| line.to_string()).collect();
    let raw_lines = api_lines(raw);
    let bundle = crate::editor::save_properties::schema();

    // Parsing and the problems, headless and as the editor shows them.
    let parsed = behavior.parse(&text);
    if let Err(error) = &parsed
        && let (Some(line), Some(column)) = (error.line, error.column)
        && !in_lines(&owned_lines, line, column)
    {
        return fail(
            "a parse error is placed outside the text",
            name,
            format!("{error:?} in {}", shown(&text)),
        );
    }
    for problem in check_syntax(language, raw) {
        if bad_diagnostic(&raw_lines, &problem) {
            return fail(
                "a syntax problem is placed outside the text",
                name,
                format!("{problem:?} in {}", shown(raw)),
            );
        }
    }
    for problem in validate(&text, Some(&bundle), None, true, behavior) {
        if bad_diagnostic(&owned_lines, &problem) {
            return fail(
                "a schema problem is placed outside the text",
                name,
                format!("{problem:?} in {}", shown(&text)),
            );
        }
    }

    // Highlighting, one line at a time after the constructs over lines.
    let comments = behavior.comments(&text);
    if let Some(bad) = bad_ranges(&text, &comments, true) {
        return fail("a comment range is outside the text", name, bad);
    }
    if let Some(bad) = bad_ranges(&text, &behavior.multiline_strings(&text), true) {
        return fail("a multi-line string range is outside the text", name, bad);
    }
    let constructs = behavior.multiline_constructs(&text);
    let ranges: Vec<_> = constructs.iter().map(|one| one.range.clone()).collect();
    if let Some(bad) = bad_ranges(&text, &ranges, false) {
        return fail("a multi-line construct is outside the text", name, bad);
    }
    let spanning = Spanning::of(&text, &comments, &constructs);
    for span in spanning.colored.iter().chain(&spanning.structures) {
        let first = span.lines.start.checked_sub(1).and_then(|at| lines.get(at));
        let last = span.lines.end.checked_sub(1).and_then(|at| lines.get(at));
        let fits = matches!((first, last), (Some(first), Some(last))
            if span.start <= first.len() && span.end <= last.len());
        if !fits || span.lines.start > span.lines.end {
            return fail(
                "a construct over lines names lines outside the text",
                name,
                format!("{span:?} in {}", shown(&text)),
            );
        }
    }
    for (index, line) in lines.iter().enumerate() {
        let tokens = tokens_after(language, line, spanning.at(index));
        if let Some(bad) = bad_tokens(line, &tokens) {
            return fail("highlight tokens overlap or leave their line", name, bad);
        }
        for own in [behavior.highlight(line), behavior.highlight_continued(line)] {
            if let Some(bad) = bad_tokens(line, &own) {
                return fail(
                    "the language's own tokens overlap or leave their line",
                    name,
                    bad,
                );
            }
        }
        let runs = runs_of(line, &tokens);
        let covered: String = runs.iter().map(|(from, to, _)| &line[*from..*to]).collect();
        if covered != *line {
            return fail(
                "highlight runs do not give the line back",
                name,
                shown(line),
            );
        }
        if splits(line) {
            let (pieces, _) = RowPieces::next(None, line, &runs, &[]);
            let joined: String = pieces
                .pieces()
                .iter()
                .map(|piece| piece.content.text.as_str())
                .collect();
            if joined != *line {
                return fail(
                    "a long row's pieces do not give the row back",
                    name,
                    shown(line),
                );
            }
        }
        if let Some((_, start)) = behavior.hover_key(line, line.chars().count() / 2)
            && start > line.chars().count()
        {
            return fail("a hovered key starts past its line", name, shown(line));
        }
    }

    // Folds and the lines of strings over lines.
    let line_count = lines.len();
    let folds = behavior.fold_ranges(&text);
    for (index, fold) in folds.iter().enumerate() {
        let sorted = index == 0 || folds[index - 1].start < fold.start;
        if fold.start >= fold.end || fold.end >= line_count || !sorted {
            return fail(
                "a fold range is outside the text or out of order",
                name,
                format!(
                    "{fold:?} of {line_count} lines: {folds:?} in {}",
                    shown(&text)
                ),
            );
        }
    }
    for fold in language.fold_ranges(raw) {
        if fold.start >= fold.end || fold.end >= raw_lines.len() {
            return fail(
                "a fold range of the host's text is outside it",
                name,
                format!("{fold:?} of {} lines in {}", raw_lines.len(), shown(raw)),
            );
        }
    }
    for lines_of in [
        behavior.string_body_lines(&text),
        behavior.saved_string_lines(&text),
    ] {
        if lines_of
            .iter()
            .any(|range| range.start > range.end || range.end > line_count)
        {
            return fail(
                "string body lines are outside the text",
                name,
                format!("{lines_of:?} of {line_count} lines in {}", shown(&text)),
            );
        }
    }
    for slot in behavior.empty_value_slots(&text) {
        if slot > text.len() || !text.is_char_boundary(slot) {
            return fail(
                "an empty value slot is outside the text",
                name,
                format!("{slot} in {}", shown(&text)),
            );
        }
    }
    if let Some(span) = behavior.first_span(&text)
        && bad_span(&owned_lines, &span)
    {
        return fail(
            "the first field's span is outside the text",
            name,
            format!("{span:?} in {}", shown(&text)),
        );
    }

    // At some carets: the suggestions, Enter, the typing hooks, comments.
    let end = units(&text);
    for at in places(&text, case.at) {
        let context = behavior.completion_context(&text, at);
        let ranges = std::iter::once((context.replace_start, context.replace_end))
            .chain(context.paired_names.iter().copied());
        for (start, stop) in ranges {
            if start > stop
                || stop > end
                || !on_character(&text, start)
                || !on_character(&text, stop)
            {
                return fail(
                    "the completion context names a range outside the text",
                    name,
                    format!("{start}..{stop} at {at} in {}", shown(&text)),
                );
            }
        }
        let byte = crate::editor::keyboard::utf16_to_byte_index(&text, at);
        let newline = behavior.newline(&text, at);
        if newline.caret > newline.text.len()
            || !newline.text.is_char_boundary(newline.caret)
            || byte + newline.replaces > text.len()
            || !text.is_char_boundary(byte + newline.replaces)
            || newline.replaces_before > byte
            || !text.is_char_boundary(byte - newline.replaces_before)
        {
            return fail(
                "Enter writes or replaces outside the text",
                name,
                format!("{newline:?} at {at} in {}", shown(&text)),
            );
        }
        let _ = behavior.entry_started(&text, at);
        let _ = behavior.lexical_state(&text, at);
        for typed in ['>', '/', '"', ']', '}', '=', '\'', ':'] {
            let _ = behavior.auto_close(&text, at, typed);
            let _ = behavior.complete_on_type(&text, at, typed);
            let _ = behavior.reindent_on_type(&text, at, typed);
        }
        if let Some((toggled, from, to)) = behavior.toggled_text(&text, at, at)
            && (from > to || to > units(&toggled))
        {
            return fail(
                "comment toggling selects outside its text",
                name,
                format!("{from}..{to} at {at} in {}", shown(&text)),
            );
        }
        let _ = completions(&text, at, true, Some(&bundle), true, behavior);
        let suggestions = schema_suggestions(&bundle, behavior, &text, at);
        if let Some(option) = suggestions.first() {
            let (accepted, selection) = option.accept(&text);
            if selection.start > selection.end || selection.end > units(&accepted) {
                return fail(
                    "an accepted suggestion selects outside the text",
                    name,
                    format!("{selection:?} at {at} in {}", shown(&accepted)),
                );
            }
        }
        let _ = crate::editor::paths::path_at(behavior, &text, at);
        // The hover at the caret's line and column.
        let position = TextPosition::from_offset(&text, at);
        let _ = schema_hover(&bundle, behavior, &text, position);
        if let Some(line) = lines.get(position.line.saturating_sub(1)) {
            let _ = behavior.hover_path(&text, position.line - 1, position.column - 1);
            if let Some((_, start)) = behavior.hover_key(line, position.column - 1)
                && start > line.chars().count()
            {
                return fail("a hovered key starts past its line", name, shown(line));
            }
        }
        let _ = language.path_at(raw, position);
    }
    if let Some((toggled, from, to)) = behavior.toggled_text(&text, 0, end)
        && (from > to || to > units(&toggled))
    {
        return fail(
            "comment toggling selects outside its text",
            name,
            format!("{from}..{to} of all of {}", shown(&text)),
        );
    }

    // The fields' places, and a conversion of what parses.
    if let Ok(value) = &parsed {
        let mut found = Vec::new();
        pointers(value, String::new(), &mut found);
        let found_paths: Vec<FieldPath> = found.iter().map(FieldPath::from).collect();
        for (pointer, range) in found
            .iter()
            .zip(language.ranges_of(raw, found.iter().cloned()))
        {
            if let Some(range) = range
                && (!in_lines(&raw_lines, range.start.line, range.start.column)
                    || !(in_lines(&raw_lines, range.end.line, range.end.column)
                        || (range.end.line == range.start.line
                            && marker_fits(
                                &raw_lines,
                                range.start.line,
                                range.start.column,
                                range.end.column,
                            )))
                    || (range.end.line, range.end.column) < (range.start.line, range.start.column))
            {
                return fail(
                    "a field's range is outside the text",
                    name,
                    format!("{pointer:?} at {range:?} in {}", shown(raw)),
                );
            }
        }
        for spans in [
            behavior.key_spans(&text, &found_paths),
            behavior.value_spans(&text, &found_paths),
        ] {
            for (pointer, span) in found.iter().zip(spans) {
                if let Some(span) = span
                    && bad_span(&owned_lines, &span)
                {
                    return fail(
                        "a field's span is outside the text",
                        name,
                        format!("{pointer:?} at {span:?} in {}", shown(&text)),
                    );
                }
            }
        }
        let target = Language::ALL[case.at as usize % Language::ALL.len()];
        if let Ok(conversion) = language.convert(raw, target)
            && let Err(error) = target.parse(&conversion.text)
        {
            return fail(
                &format!("a conversion to {target} writes what it cannot read"),
                name,
                format!("{error} in {} from {}", shown(&conversion.text), shown(raw)),
            );
        }
    }
    Ok(())
}

/// Runs one case, failing with what it found (for a pasted repro).
#[allow(dead_code, reason = "called by pasted repros")]
pub(super) fn replay(case: ReadCase) {
    if let Err(failure) = check(&case) {
        panic!("{}\n{}", failure.class, failure.detail);
    }
}

/// Generous for a debug build on a busy machine: a case takes
/// milliseconds; the stack is what a page's main thread has, about.
const CAP: Cap = Cap::Thread {
    time: std::time::Duration::from_secs(30),
    stack: 4 << 20,
};

#[test]
fn every_reading_of_hostile_text_stays_inside_it() {
    run("reading", cases(200), CAP, generate, check);
}
