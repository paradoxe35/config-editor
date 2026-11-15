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
        finding: "GP38-1 (P1, MEDIUM): in a declared language (Dot, DotBare: fields found \
                  on their lines), a placeholder an edit moves off its field (a `:` typed \
                  into its key, its list item made a key, a one-line map's entry run into \
                  the one before, a copy of its line) is not held: text typed beside it \
                  is taken, and the host hears it",
        applies: |class| {
            class.case.starts_with("Dot") && is(class, Check::Glued, &[], &[Lock::Secret])
        },
    },
    Known {
        finding: "GP38-2 (P1, MEDIUM): in a custom language on the trait (Odd), a placeholder \
                  an edit moves off its field (its key renamed, its item's lines cut and \
                  moved) is not held: a character typed beside it, inside its quotes or \
                  after `:=`, is taken",
        applies: |class| {
            class.case.starts_with("Odd") && is(class, Check::Glued, &[], &[Lock::Secret])
        },
    },
    Known {
        finding: "GP38-3 (LOW): in a declared language (Dot, DotBare), ordinary edits near \
                  a protected field are refused: a new one-line-map entry typed before it \
                  (refused at its closing `;`, a secret glued meanwhile: the cause of \
                  GP38-1), a duplicate line shadowed by the protected one cut, a Replace \
                  all touching only other entries of its one-line map",
        applies: |class| {
            class.case.starts_with("Dot")
                && matches!(class.check, Check::OverRefusal | Check::Differs)
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
