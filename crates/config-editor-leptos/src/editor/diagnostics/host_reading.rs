//! A host's schema problems in its Edit-as view, where the view's own
//! reading cannot see them. A value the view cannot show exactly keeps
//! its host text, and with it its host problem: a JSON host's `"8"` under
//! `type: number` reads as `8` in an XML view, which finds nothing wrong.
//! So while the view cannot show the host exactly, the host's text is
//! checked too (once each time it changes), and each problem only the
//! host's reading has is placed by its value in the view.

use super::{Diagnostic, MORE_PROBLEMS_CODE, capped, incomplete, shift_diagnostics};
use crate::EditorStrings;
use crate::editor::language::ConfigLanguage;
use crate::editor::schema::SchemaBundle;

/// A schema problem's value (its JSON pointer) and keyword: two readings
/// finding the same one have the same problem.
pub(in crate::editor) type ProblemKey = (String, String);

/// The host's schema problems, placed in its view's text `on` (the one
/// the host's text was saved from, or shown as).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::editor) struct HostProblems {
    on: String,
    found: Vec<(ProblemKey, Diagnostic)>,
    complete: bool,
}

/// The schema problems of `host` (the host's text, read by
/// `host_behavior`), placed where `edit` finds their values in `view`:
/// `None` when the host's text does not read (the view says why when it
/// matters) or nothing is checked.
#[cfg_attr(
    not(feature = "schema-validation"),
    expect(unused_variables, reason = "nothing is checked without the feature")
)]
pub(in crate::editor) fn host_problems(
    host: &str,
    host_behavior: &dyn ConfigLanguage,
    view: &str,
    edit: &dyn ConfigLanguage,
    schema: &SchemaBundle,
    accepted: &[(String, String)],
    strings: &EditorStrings,
) -> Option<HostProblems> {
    #[cfg(feature = "schema-validation")]
    {
        use crate::editor::work_count::{Work, note};
        note(Work::HostChecked);
        let (host, _) = crate::editor::byte_order_mark::split(host);
        if host.trim().is_empty() {
            return None;
        }
        let document =
            crate::editor::codec::read(host_behavior, host, Some(&schema.schema)).ok()?;
        let reading = super::schema_problems::Reading {
            document: &document,
            behavior: host_behavior,
        };
        let (found, complete) =
            super::schema_problems::found(reading, view, edit, schema, accepted, strings);
        Some(HostProblems {
            on: view.to_string(),
            found,
            complete,
        })
    }
    #[cfg(not(feature = "schema-validation"))]
    None
}

/// `own` (the view's problems of the text `checked`, its schema problems
/// named by `keys`) with each of `host`'s it does not have, moved to where
/// their lines are in `checked`; capped as one list, which says it is
/// incomplete when either check stopped early.
pub(in crate::editor) fn with_host_problems(
    own: &[Diagnostic],
    keys: &[ProblemKey],
    host: &HostProblems,
    checked: &str,
    strings: &EditorStrings,
) -> Vec<Diagnostic> {
    let extra: Vec<Diagnostic> = host
        .found
        .iter()
        .filter(|(key, problem)| {
            !keys.contains(key)
                && !own.iter().any(|known| {
                    (known.line, known.column, &known.message)
                        == (problem.line, problem.column, &problem.message)
                })
        })
        .map(|(_, problem)| problem.clone())
        .collect();
    if extra.is_empty() && host.complete {
        return own.to_vec();
    }
    let extra = if host.on == checked {
        extra
    } else {
        shift_diagnostics(&host.on, checked, &extra)
    };
    let mut all = own.to_vec();
    // A list already cut says more follow.
    if !own
        .iter()
        .any(|problem| problem.code.as_deref() == Some(MORE_PROBLEMS_CODE))
    {
        all.extend(extra);
        all = capped(all, strings);
    }
    if host.complete {
        all
    } else {
        incomplete(all, strings)
    }
}
