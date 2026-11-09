//! Switching to another language with host problems by range: the fields
//! the ranges start in are read from the host's text in one pass however
//! many ranges there are, in every language (each range starting on a line
//! without a key or value of its own read the text from the top: 5000
//! ranges took 24 s to switch from XML), and each is the field `path_at`
//! alone reads there.

use super::super::Language;
use super::super::paths::{TextPosition, path_at};
use super::super::work_count::{Work, counted};
use super::batch_tests::{LANGUAGES, document, on_every_line};
use super::targets::Target;
use super::tests::report;
use super::{DiagnosticTarget, HostTargets};

/// The targets of a range problem on every line of `items` items written
/// in `host`, and the walks of its text from the top finding them took.
fn found(host: Language, items: usize) -> (String, Vec<Target>, usize) {
    let text = host.serialize(&document(items)).expect("the host's text");
    let problems = on_every_line(&text);
    let (found, walks) = counted(Work::PathLookup, || {
        HostTargets::new(host.behavior(), &text, &problems)
    });
    (text, found.targets().to_vec(), walks)
}

#[test]
fn the_fields_of_ranges_are_read_in_one_pass_in_every_language() {
    let mut failures = Vec::new();
    for host in LANGUAGES {
        let (_, few, few_walks) = found(host, 10);
        let (_, many, many_walks) = found(host, 100);
        let fields = |targets: &[Target]| {
            targets
                .iter()
                .filter(|target| matches!(target, Target::Field { .. }))
                .count()
        };
        if few_walks != many_walks || fields(&many) <= fields(&few) {
            failures.push(format!(
                "{host}: {few_walks} walks from the top for {} fields, {many_walks} for {}",
                fields(&few),
                fields(&many)
            ));
        }
    }
    report(failures);
}

/// Each range's field is the one `path_at` alone reads where it starts
/// (in its line's indentation: at the line's first character).
#[test]
fn the_fields_of_ranges_read_in_one_pass_are_the_ones_read_alone() {
    let mut failures = Vec::new();
    for host in LANGUAGES {
        let (text, targets, _) = found(host, 12);
        let behavior = host.behavior();
        for (problem, target) in on_every_line(&text).iter().zip(targets) {
            let DiagnosticTarget::Range(range) = problem.target else {
                continue;
            };
            let line = text.split('\n').nth(range.start.line - 1).unwrap_or("");
            let indent = line.len() - line.trim_start().len();
            let column = if indent < line.len() { indent + 1 } else { 1 };
            let at = TextPosition::new(range.start.line, column).to_offset(&text);
            let alone = path_at(behavior, &text, at)
                .filter(|path| !path.is_root())
                .map(|path| path.to_pointer());
            let read = match &target {
                Target::Field { pointer, .. } => Some(pointer.clone()),
                _ => None,
            };
            if read != alone {
                failures.push(format!(
                    "{host} line {} ({line:?}): read {read:?}, alone {alone:?}",
                    range.start.line
                ));
            }
        }
    }
    report(failures);
}
