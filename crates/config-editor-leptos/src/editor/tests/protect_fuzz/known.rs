//! Classes of failure an open finding explains, each naming it (NEW: found
//! by this test, not in qa38-findings.md yet): the standing test lets
//! them through; the finding's fix removes the entry.

use super::Class;
use super::cases::Lock;
use super::checks::Check;

pub(super) struct Known {
    pub(super) finding: &'static str,
    pub(super) applies: fn(&Class) -> bool,
}

pub(super) const KNOWN: &[Known] = &[
    Known {
        finding: "SPANS-4 seeds 5227, 503169 (P1, MEDIUM): in an Edit-as JSON or YAML \
                  view that does not parse, a new value pasted over a list item a host \
                  range covers is taken (the guard reads a covered field's value as the \
                  text past its key, and an item's key span is the item itself)",
        applies: |class| {
            class.edit_as
                && is(
                    class,
                    Check::LockChanged,
                    &["JSON list", "YAML list"],
                    &[Lock::Range],
                )
        },
    },
    Known {
        finding: "GP38-3 (LOW): in Dot's one-line map, a new entry typed a key at a time \
                  before the secret's passes through `passwordx: password: '«…»'`, which \
                  Dot's parser reads as the token glued into `passwordx`: refused at that \
                  `:` (taking it needs the change event held while a placeholder is glued)",
        applies: |class| {
            is(class, Check::OverRefusal, &["Dot map"], &[Lock::Secret]) && class.step == "Entry"
        },
    },
    Known {
        finding: "GP38-3 (LOW): in a declared language, a line holding a locked field copied \
                  makes two alike lines; the host range follows one copy and the field's \
                  lock the other (an entry typed into the range's copy is refused), or the \
                  key holding a list stays on the first `keys:` line while the parse reads \
                  the items under the copy (cutting the first is refused)",
        applies: |class| {
            class.case.starts_with("Dot")
                && class.check == Check::OverRefusal
                && matches!(
                    (class.lock, class.step.as_str()),
                    (Lock::Range, "Entry") | (Lock::Path | Lock::Schema, "Cut")
                )
        },
    },
    Known {
        finding: "NEW (LOW, deferred): a secret's new value is quoted once an edit moves it \
                  off its schema path",
        applies: |class| is(class, Check::Leak, &[], &[Lock::Secret]),
    },
];

/// The finding excusing `class`, if one does.
pub(super) fn excused(class: &Class) -> Option<&'static str> {
    KNOWN
        .iter()
        .find(|known| (known.applies)(class))
        .map(|known| known.finding)
}

/// Whether `class` is `check` in one of `cases` (any when empty) with one
/// of `locks` (any when empty).
fn is(class: &Class, check: Check, cases: &[&str], locks: &[Lock]) -> bool {
    class.check == check
        && (cases.is_empty() || cases.contains(&class.case))
        && (locks.is_empty() || locks.contains(&class.lock))
}
