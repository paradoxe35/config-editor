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
        finding: "SPANS-4: a field a host range covers reads otherwise, not held, in a few \
                  shapes (Edit-as views, a new value or a line moved in TOML or XML)",
        applies: |class| is(class, Check::LockChanged, &[], &[Lock::Range]),
    },
    Known {
        finding: "SPANS-8: a readOnly YAML list item moved out of its list still keeps its \
                  text (cutting it is refused)",
        applies: |class| is(class, Check::OverRefusal, &["YAML list"], &[Lock::Schema]),
    },
    Known {
        finding: "SPANS-9 (P1): in a YAML view, a character typed at a list item's line \
                  start that makes the list one scalar passes, gluing a placeholder \
                  below (1 run in 10000)",
        applies: |class| class.edit_as && is(class, Check::Glued, &["YAML list"], &[Lock::Secret]),
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
