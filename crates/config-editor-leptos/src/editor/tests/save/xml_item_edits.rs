//! Which item is which, as the user's edits of an Edit-as view make it:
//! items of an XML host holding markup no view shows (`xsi:type`, a
//! namespace declaration, a prefixed name) keep it through the lines the
//! user deletes, cuts and pastes, even where their data reads alike (QA
//! `E32-3`: deleting the first of two items with the same data kept its
//! `xsi:type` on the second) and across a save between the cut and the
//! paste (QA `E32-2`: an item cut and pasted lost its `xsi:type`, saved
//! as one removed and one added). Each session is saved as the editor
//! saves it: the edits since the last save tell the save the items.

use crate::editor::Language;
use crate::editor::bridge::Bridge;
use crate::editor::bridge::item_trail::Trail;
use crate::editor::save_matrix::report;

const XSI: &str = "xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"";

/// What tells the items apart besides their data.
#[derive(Clone, Copy, Debug)]
pub(in crate::editor::tests) enum Kind {
    Type,
    DefaultNamespace,
    /// `<a:item xmlns:a="urn:N">`.
    Prefix,
}

impl Kind {
    pub(in crate::editor::tests) fn host(self, sizes: &[u32]) -> String {
        let mut text = format!("<root {XSI}>\n");
        for (item, size) in sizes.iter().enumerate() {
            let (open, close) = match self {
                Kind::Type => (
                    format!("<item xsi:type=\"T{item}\">"),
                    "</item>".to_string(),
                ),
                Kind::DefaultNamespace => (
                    format!("<item xmlns=\"urn:d{item}\">"),
                    "</item>".to_string(),
                ),
                Kind::Prefix => (
                    format!("<a:item xmlns:a=\"urn:p{item}\">"),
                    "</a:item>".to_string(),
                ),
            };
            text.push_str(&format!("  {open}\n    <size>{size}</size>\n  {close}\n"));
        }
        text + "</root>\n"
    }

    /// The markup of the items of `saved`, as the host's item each is.
    pub(in crate::editor::tests) fn order(self, saved: &str) -> Vec<usize> {
        let marker = match self {
            Kind::Type => "xsi:type=\"T",
            Kind::DefaultNamespace => "xmlns=\"urn:d",
            Kind::Prefix => "xmlns:a=\"urn:p",
        };
        saved
            .match_indices(marker)
            .map(|(at, _)| {
                let digits = &saved[at + marker.len()..];
                digits[..digits.find('"').expect("closed")]
                    .parse()
                    .expect("a number")
            })
            .collect()
    }
}

/// A step of a session: the view's next text and the caret it leaves (in
/// UTF-16 units), or a typing pause, which saves.
enum Step {
    Edit(String, usize),
    Pause,
}

/// The host saved after `steps` in the `edit` view of `host`.
fn session(
    edit: Language,
    host: &str,
    steps: &dyn Fn(&str) -> Vec<Step>,
) -> Result<String, String> {
    let bridge = Bridge::new(Language::XML, edit, None).ok_or("same language")?;
    let mut view = bridge
        .to_edit(host)
        .map_err(|error| error.to_string())?
        .text;
    let mut trail = Trail::starting(&view);
    let mut saved = host.to_string();
    let mut steps = steps(&view);
    steps.push(Step::Pause);
    for step in steps {
        match step {
            Step::Edit(next, caret) => {
                trail.note(&view, &next, caret);
                view = next;
            }
            Step::Pause => {
                let (result, next) = bridge.to_host_traced(&view, &saved, host, Some(&trail));
                saved = result.map_err(|error| format!("{error:?}"))?;
                trail = next;
            }
        }
    }
    Ok(saved)
}

/// The line ranges (from 0, end excluded) of the list items of a view
/// whose items each start on a line of their own: a YAML `- ` line, or a
/// JSON `{` line through its `}`.
pub(in crate::editor::tests) fn item_lines(edit: Language, view: &str) -> Vec<(usize, usize)> {
    let lines: Vec<&str> = view.lines().collect();
    let starts: Vec<usize> = (0..lines.len())
        .filter(|at| {
            if edit == Language::JSON {
                lines[*at].trim() == "{"
                    && *at > 0
                    && lines[*at - 1].trim_end().ends_with(['[', ','])
            } else {
                lines[*at].trim_start().starts_with("- ")
            }
        })
        .collect();
    starts
        .iter()
        .map(|start| {
            let indent = lines[*start].len() - lines[*start].trim_start().len();
            let end = (start + 1..lines.len())
                .find(|at| {
                    let line = lines[*at];
                    let depth = line.len() - line.trim_start().len();
                    if edit == Language::JSON {
                        depth == indent && line.trim_start().starts_with('}')
                    } else {
                        depth <= indent
                    }
                })
                .map_or(lines.len(), |end| {
                    if edit == Language::JSON { end + 1 } else { end }
                });
            (*start, end)
        })
        .collect()
}

pub(in crate::editor::tests) fn line_offset(text: &str, line: usize) -> usize {
    text.split_inclusive('\n').take(line).map(str::len).sum()
}

fn units(text: &str, byte: usize) -> usize {
    text[..byte].encode_utf16().count()
}

/// The view without item `item`'s lines, the caret left where they were.
fn deleted(edit: Language, view: &str, item: usize) -> Step {
    let (next, caret) = without_item(edit, view, item);
    Step::Edit(next, caret)
}

/// The view (YAML or JSON) without item `item`'s lines, and the caret
/// left where they were.
pub(in crate::editor::tests) fn without_item(
    edit: Language,
    view: &str,
    item: usize,
) -> (String, usize) {
    let (start, end) = item_lines(edit, view)[item];
    let (from, to) = (line_offset(view, start), line_offset(view, end));
    let next = format!("{}{}", &view[..from], &view[to..]);
    let caret = units(&next, from);
    (next, caret)
}

/// Item `item`'s lines cut and pasted before item `before`'s: the cut, and
/// the paste (an item before the last, so a JSON view's commas stay).
fn moved(edit: Language, view: &str, item: usize, before: usize, pause: bool) -> Vec<Step> {
    let (start, end) = item_lines(edit, view)[item];
    let (from, to) = (line_offset(view, start), line_offset(view, end));
    let lines = view[from..to].to_string();
    let cut = format!("{}{}", &view[..from], &view[to..]);
    let at = line_offset(&cut, item_lines(edit, &cut)[before].0);
    let pasted = format!("{}{lines}{}", &cut[..at], &cut[at..]);
    let mut steps = vec![Step::Edit(cut.clone(), units(&cut, from))];
    if pause {
        steps.push(Step::Pause);
    }
    steps.push(Step::Edit(
        pasted,
        units(&cut, at) + lines.encode_utf16().count(),
    ));
    steps
}

#[test]
fn items_keep_their_markup_through_the_lines_deleted_cut_and_pasted() {
    let mut failures = Vec::new();
    // A label, the items' sizes, the edit, and each saved item's host item.
    type Operation = (
        &'static str,
        &'static [u32],
        fn(Language, &str) -> Vec<Step>,
        &'static [usize],
    );
    let operations: [Operation; 6] = [
        (
            "same data, delete the first",
            &[1, 1],
            |edit, view| vec![deleted(edit, view, 0)],
            &[1],
        ),
        (
            "same data, delete the second",
            &[1, 1, 1],
            |edit, view| vec![deleted(edit, view, 1)],
            &[0, 2],
        ),
        (
            "cut and paste",
            &[1, 2, 3],
            |edit, view| moved(edit, view, 1, 0, false),
            &[1, 0, 2],
        ),
        (
            "cut, save, paste",
            &[1, 2, 3],
            |edit, view| moved(edit, view, 1, 0, true),
            &[1, 0, 2],
        ),
        (
            "same data, cut and paste",
            &[1, 1, 1],
            |edit, view| moved(edit, view, 1, 0, false),
            &[1, 0, 2],
        ),
        (
            "same data, cut, save, paste",
            &[1, 1, 1],
            |edit, view| moved(edit, view, 1, 0, true),
            &[1, 0, 2],
        ),
    ];
    for kind in [Kind::Type, Kind::DefaultNamespace, Kind::Prefix] {
        for (label, sizes, operation, wanted) in &operations {
            let host = kind.host(sizes);
            for edit in [Language::YAML, Language::JSON] {
                let case = format!("{kind:?}, {label}, via {edit}");
                match session(edit, &host, &|view| operation(edit, view)) {
                    Ok(saved) if kind.order(&saved) == *wanted => {}
                    Ok(saved) => failures.push(format!(
                        "{case}: items {:?}, wanted {wanted:?}:\n{saved}",
                        kind.order(&saved)
                    )),
                    Err(error) => failures.push(format!("{case}: {error}")),
                }
            }
        }
    }
    report(failures);
}
