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
        finding: "NEW (LOW): in XML that does not parse (an element written above the root), \
                  three alike `<db …/>` lines copied: the lock of the range's field follows \
                  past its line's end, so an attribute pasted into the range's line is \
                  refused (seed 7862 at 20000 cases; the same without this branch's lock \
                  placement)",
        applies: |class| {
            is(
                class,
                Check::OverRefusal,
                &["XML attribute"],
                &[Lock::Range],
            ) && class.step == "Entry"
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
