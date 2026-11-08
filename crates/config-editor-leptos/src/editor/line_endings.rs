//! A host file's line endings, kept: the editor works on LF text (the
//! textarea and all line math assume it), and what it hands back to the
//! host (`on_change`, `on_save`, the handle's `value`) is written with the
//! host's own endings, line by line, in every language and through Edit-as:
//! a line left as it was keeps its CRLF, LF or lone CR; an edited line keeps
//! the ending it had; a new line takes the one above's, or the text's most
//! common one. A lone CR the host's language does not read as a line break
//! (TOML, HCL) is a syntax problem, worded by the language's reader, until
//! the line is edited (an edited line takes a break the language reads).

use leptos::prelude::*;

use super::diagnostics::{Diagnostic, Severity};
use super::host_editing::SaveRequest;
use super::languages::Language;
use super::state::normalize_newlines;

/// The host's text as last seen or last handed back, with its endings.
#[derive(Clone, Copy)]
pub(super) struct LineEndings(StoredValue<Endings>);

struct Endings {
    /// The text with its line endings; empty when all of them are LF.
    reference: String,
    /// The most common ending, for new lines without one above.
    common: &'static str,
    /// The host's language, which says whether a lone CR breaks a line.
    language: Language,
    /// Whether the language reads a lone CR as a line break (no when a text
    /// with one fails to parse while the same text with LF parses).
    lone_cr_breaks: bool,
}

impl LineEndings {
    /// The line endings of `text`, a document in `language`.
    pub(super) fn of(text: &str, language: Language) -> Self {
        let endings = Endings {
            reference: String::new(),
            common: "\n",
            language,
            lone_cr_breaks: true,
        };
        let this = Self(StoredValue::new(endings));
        this.note(text);
        this
    }

    /// A new text from the host: its line endings (a text without line
    /// breaks keeps the most common ending seen before, for new lines).
    pub(super) fn note(&self, text: &str) {
        self.0.update_value(|endings| {
            if text.contains(['\n', '\r']) {
                endings.common = most_common(text);
            }
            endings.reference = if text.contains('\r') {
                text.to_string()
            } else {
                String::new()
            };
            endings.lone_cr_breaks = !has_lone_cr(text)
                || endings.language.parse(text).is_ok()
                || endings
                    .language
                    .parse(&normalize_newlines(text.to_string()))
                    .is_err();
        });
    }

    /// `text` (LF) written with the host's line endings, kept as the text
    /// last handed back.
    pub(super) fn restore(&self, text: String) -> String {
        let restored = self.preview(text);
        self.0.update_value(|endings| {
            endings.reference = if restored.contains('\r') {
                restored.clone()
            } else {
                String::new()
            };
        });
        restored
    }

    /// `text` (LF) written with the host's line endings, not kept.
    fn preview(&self, text: String) -> String {
        self.0.with_value(|endings| {
            if endings.reference.is_empty() && endings.common == "\n" {
                text
            } else {
                written(&text, endings)
            }
        })
    }

    /// The syntax problem of `text` (LF, the host's document on screen) once
    /// written with the host's endings: a lone CR its language does not read
    /// as a line break, as its reader reports it; `None` when there is none.
    pub(super) fn rejected_break(&self, text: &str) -> Option<Diagnostic> {
        let (language, lone_cr_breaks) = self
            .0
            .with_value(|endings| (endings.language, endings.lone_cr_breaks));
        if lone_cr_breaks {
            return None;
        }
        let written = self.preview(text.to_string());
        if !has_lone_cr(&written) {
            return None;
        }
        let error = language.parse(&written).err()?;
        language.parse(text).ok()?;
        // The marker goes at the end of the first line ending with one.
        let line = split_lines(&written).position(|(_, ending)| ending == "\r")?;
        let column = text.split('\n').nth(line)?.chars().count() + 1;
        Some(Diagnostic {
            line: Some(line + 1),
            column: Some(column),
            end_column: Some(column + 1),
            message: error.message,
            severity: Severity::Error,
            ..Diagnostic::default()
        })
    }

    /// The host's `on_change`, handed the text with its line endings.
    pub(super) fn on_change(self, callback: Callback<String>) -> Callback<String> {
        Callback::new(move |text: String| callback.run(self.restore(text)))
    }

    /// The host's `on_save`, handed the text with its line endings.
    pub(super) fn on_save(self, callback: Callback<SaveRequest>) -> Callback<SaveRequest> {
        Callback::new(move |request: SaveRequest| {
            callback.run(SaveRequest {
                text: self.restore(request.text),
                ..request
            });
        })
    }
}

/// `text` (LF) with each line's ending from `endings.reference`: the lines
/// before and after the edited stretch match the reference's and keep their
/// endings; an edited line keeps the ending of the line it replaces; a new
/// line takes the one above's (or the most common one).
fn written(text: &str, endings: &Endings) -> String {
    let reference: Vec<(&str, &str)> = if endings.reference.is_empty() {
        Vec::new()
    } else {
        split_lines(&endings.reference).collect()
    };
    let lines: Vec<&str> = text.split('\n').collect();
    let (count, old) = (lines.len(), reference.len());
    let prefix = lines
        .iter()
        .zip(&reference)
        .take_while(|(line, (old, _))| *line == old)
        .count();
    let suffix = lines
        .iter()
        .rev()
        .zip(reference.iter().rev())
        .take(count.min(old) - prefix)
        .take_while(|(line, (old, _))| *line == old)
        .count();
    // A lone CR the language does not read is never written anew.
    let readable = |ending: &'static str| endings.lone_cr_breaks || ending != "\r";
    let common = if readable(endings.common) {
        endings.common
    } else {
        "\n"
    };
    let mut out = String::with_capacity(text.len() + count);
    let mut above = common;
    for (index, line) in lines.iter().enumerate() {
        out.push_str(line);
        if index + 1 == count {
            break;
        }
        let ending = if index < prefix {
            reference[index].1
        } else if index >= count - suffix {
            reference[index + old - count].1
        } else if index < old - suffix {
            Some(reference[index].1)
                .filter(|ending| readable(ending))
                .unwrap_or("")
        } else {
            ""
        };
        let ending = if ending.is_empty() { above } else { ending };
        out.push_str(ending);
        above = ending;
    }
    out
}

/// The lines of `text` with what ends each: `"\r\n"`, `"\n"`, `"\r"`, or
/// `""` for the last.
fn split_lines(text: &str) -> impl Iterator<Item = (&str, &'static str)> {
    let mut rest = Some(text);
    std::iter::from_fn(move || {
        let current = rest?;
        let Some(at) = current.find(['\r', '\n']) else {
            rest = None;
            return Some((current, ""));
        };
        let ending = if current[at..].starts_with("\r\n") {
            "\r\n"
        } else if current.as_bytes()[at] == b'\r' {
            "\r"
        } else {
            "\n"
        };
        rest = Some(&current[at + ending.len()..]);
        Some((&current[..at], ending))
    })
}

/// Whether `text` has a CR not followed by LF.
fn has_lone_cr(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes
        .iter()
        .enumerate()
        .any(|(at, byte)| *byte == b'\r' && bytes.get(at + 1) != Some(&b'\n'))
}

/// The ending most of `text`'s lines have (LF on a tie, and without breaks).
fn most_common(text: &str) -> &'static str {
    let (mut crlf, mut lf, mut cr) = (0, 0, 0);
    for (_, ending) in split_lines(text) {
        match ending {
            "\r\n" => crlf += 1,
            "\n" => lf += 1,
            "\r" => cr += 1,
            _ => {}
        }
    }
    if crlf > lf && crlf >= cr {
        "\r\n"
    } else if cr > lf && cr > crlf {
        "\r"
    } else {
        "\n"
    }
}

#[cfg(all(test, feature = "yaml"))]
mod tests {
    use super::*;

    /// `text` (LF) written against a host text `host` in YAML.
    fn write(host: &str, text: &str) -> String {
        let endings = LineEndings::of(host, Language::YAML);
        endings.restore(text.to_string())
    }

    #[test]
    fn each_line_keeps_its_own_ending() {
        let cases = [
            // Uniform endings, as before.
            ("a\r\nb\r\n", "a\nB\n", "a\r\nB\r\n"),
            ("a\nb\n", "a\nB\n", "a\nB\n"),
            // Mixed: untouched lines keep theirs, the edited one its own.
            ("a\r\nb\nc\r\n", "A\nb\nc\n", "A\r\nb\nc\r\n"),
            ("a\r\nb\nc\r\n", "a\nB\nc\n", "a\r\nB\nc\r\n"),
            ("a\nb\r\nc\nd\n", "a\nb\nc\nD\n", "a\nb\r\nc\nD\n"),
            // Lone CR kept on its line.
            ("a\rb\nc\n", "a\nb\nC\n", "a\rb\nC\n"),
            ("a\rb\rc\r", "a\nb\nC\n", "a\rb\rC\r"),
            // New lines take the line above's ending.
            ("a\r\nb\nc\n", "a\nx\nb\nc\n", "a\r\nx\r\nb\nc\n"),
            ("a\nb\r\n", "a\nb\nc\n", "a\nb\r\nc\r\n"),
            // A last line without an ending gets one when a line follows.
            ("a\r\nb", "a\nb\nc", "a\r\nb\r\nc"),
            // Removed lines take their endings with them.
            ("a\r\nb\nc\r\n", "a\nc\n", "a\r\nc\r\n"),
            // New lines below an LF line take LF.
            ("a\r\nb\r\nc\n", "a\nb\nc\nd\ne\n", "a\r\nb\r\nc\nd\ne\n"),
        ];
        let mut failures = Vec::new();
        for (host, text, expected) in cases {
            let actual = write(host, text);
            if actual != expected {
                failures.push(format!("{host:?} + {text:?}: {actual:?}, not {expected:?}"));
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn the_most_common_ending_breaks_ties_to_lf() {
        assert_eq!(most_common("a\r\nb\r\n"), "\r\n");
        assert_eq!(most_common("a\nb\n"), "\n");
        assert_eq!(most_common("a\r\nb\nc\r\n"), "\r\n");
        assert_eq!(most_common("a\r\nb\nc\n"), "\n");
        assert_eq!(most_common("a\rb\r"), "\r");
        assert_eq!(most_common("a"), "\n");
    }
}

/// Edits in every language, in its own view and from every other, reach
/// `on_change` and `on_save` with the host's CRLF line endings (round 16
/// turned every line ending into LF).
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod editor_tests {
    use leptos::prelude::*;

    use super::super::edit::Edit;
    use super::super::host_editing::SaveRequest;
    use super::super::languages::Language;
    use super::super::sync::convert_now;
    use super::super::test_state::TestEditor;

    /// A commented document in `language`, CRLF, holding `a` = 1.
    fn sample(language: Language) -> &'static str {
        match language.label() {
            "YAML" => "# c\r\na: 1\r\nb: x\r\n",
            "JSON" => "{\r\n  \"a\": 1,\r\n  \"b\": \"x\"\r\n}\r\n",
            "TOML" => "# c\r\na = 1\r\nb = \"x\"\r\n",
            "HCL" => "# c\r\na = 1\r\nb = \"x\"\r\n",
            "XML" => "<!-- c -->\r\n<a>1</a>\r\n<b>x</b>\r\n",
            other => panic!("no sample for {other}"),
        }
    }

    #[test]
    fn edits_reach_the_host_with_its_crlf_line_endings() {
        let mut failures = Vec::new();
        for &host in Language::ALL {
            for &edit in Language::ALL {
                let text = sample(host);
                let changes = RwSignal::new(Vec::<String>::new());
                let saves = RwSignal::new(Vec::<String>::new());
                let mut editor = TestEditor::new(host, text);
                editor.edit_as = (edit != host).then_some(edit);
                editor.on_change = Some(Callback::new(move |text| {
                    changes.update(|all| all.push(text))
                }));
                editor.on_save = Some(Callback::new(move |request: SaveRequest| {
                    saves.update(|all| all.push(request.text));
                }));
                let (_owner, state) = editor.mount();
                let before = state.doc().text.get_untracked();
                let at = before.find('1').expect("the value");
                let after = format!("{}2{}", &before[..at], &before[at + 1..]);
                state.commit(Edit::new(before, (at, at + 1), after.clone(), at + 1));
                if edit != host {
                    convert_now(state, after, edit);
                }
                state.save();
                let expected = text.replacen('1', "2", 1);
                let case = format!("{host} host, edited as {edit}");
                if changes.get_untracked().last() != Some(&expected) {
                    failures.push(format!(
                        "{case}: on_change heard {:?}",
                        changes.get_untracked()
                    ));
                }
                if saves.get_untracked() != vec![expected.clone()] {
                    failures.push(format!("{case}: on_save got {:?}", saves.get_untracked()));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// A sample in `language` whose lines end with CRLF, LF and lone CR
    /// (`ending` after `a`), holding `a` = 1 and `b` = "x".
    fn mixed(language: Language, ending: &str) -> String {
        let lines: [&str; 3] = match language.label() {
            "YAML" => ["# c", "a: 1", "b: x"],
            "JSON" => ["{", "  \"a\": 1,", "  \"b\": \"x\"\n}"],
            "TOML" | "HCL" => ["# c", "a = 1", "b = \"x\""],
            "XML" => ["<r>", "<a>1</a>", "<b>x</b>\n</r>"],
            other => panic!("no sample for {other}"),
        };
        format!("{}\r\n{}{ending}{}\n", lines[0], lines[1], lines[2])
    }

    /// Each untouched line keeps its own ending (round 17 made every line
    /// the majority's), in every language, in its own view and from every
    /// other.
    #[test]
    fn untouched_lines_keep_their_own_endings() {
        let mut failures = Vec::new();
        for &host in Language::ALL {
            for &edit in Language::ALL {
                let text = mixed(host, "\n");
                let changes = RwSignal::new(Vec::<String>::new());
                let mut editor = TestEditor::new(host, &text);
                editor.edit_as = (edit != host).then_some(edit);
                editor.on_change = Some(Callback::new(move |text| {
                    changes.update(|all| all.push(text))
                }));
                let (_owner, state) = editor.mount();
                let before = state.doc().text.get_untracked();
                let at = before.find('x').expect("the value");
                let after = format!("{}w{}", &before[..at], &before[at + 1..]);
                state.commit(Edit::new(before, (at, at + 1), after.clone(), at + 1));
                if edit != host {
                    convert_now(state, after, edit);
                }
                let expected = text.replacen('x', "w", 1);
                if changes.get_untracked().last() != Some(&expected) {
                    failures.push(format!(
                        "{host} host, edited as {edit}: on_change heard {:?}",
                        changes.get_untracked()
                    ));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// A lone CR is kept on its untouched line; where the host's language
    /// does not read it as a line break (TOML, HCL), the editor shows the
    /// syntax error its reader gives, as a headless parse does, until the
    /// line is edited.
    #[test]
    fn a_lone_cr_the_language_rejects_is_a_syntax_problem() {
        let mut failures = Vec::new();
        for &host in Language::ALL {
            let text = mixed(host, "\r");
            let rejects = host.parse(&text).is_err();
            let changes = RwSignal::new(Vec::<String>::new());
            let mut editor = TestEditor::new(host, &text);
            editor.on_change = Some(Callback::new(move |text| {
                changes.update(|all| all.push(text))
            }));
            let (_owner, state) = editor.mount();
            let problems = state.problems().diagnostics.get_untracked();
            let shown = problems.iter().any(|problem| problem.line == Some(2));
            if shown != rejects || (rejects && problems.len() != 1) {
                failures.push(format!(
                    "{host}: reader rejects: {rejects}, problems {problems:?}"
                ));
            }
            // An edit elsewhere keeps the lone CR.
            let before = state.doc().text.get_untracked();
            let at = before.find('x').expect("the value");
            let after = format!("{}w{}", &before[..at], &before[at + 1..]);
            state.commit(Edit::new(before, (at, at + 1), after.clone(), at + 1));
            let expected = text.replacen('x', "w", 1);
            if changes.get_untracked().last() != Some(&expected) {
                failures.push(format!(
                    "{host}: on_change heard {:?}",
                    changes.get_untracked()
                ));
            }
            // Editing the line itself writes a break the language reads.
            let before = after;
            let at = before.find('1').expect("the value");
            let after = format!("{}2{}", &before[..at], &before[at + 1..]);
            state.commit(Edit::new(before, (at, at + 1), after.clone(), at + 1));
            let heard = changes.get_untracked().last().cloned().unwrap_or_default();
            if rejects == super::has_lone_cr(&heard) || host.parse(&heard).is_err() {
                failures.push(format!("{host}: after editing the line, heard {heard:?}"));
            }
            state.problems().diagnosed_text.set(after);
            let problems = state.problems().diagnostics.get_untracked();
            if !problems.is_empty() {
                failures.push(format!(
                    "{host}: after editing the line, problems {problems:?}"
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
