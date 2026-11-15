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

const LISTS: &[&str] = &["YAML list", "JSON list", "XML list"];

pub(super) const KNOWN: &[Known] = &[
    Known {
        finding: "PT38-3: a new list item beside an item locked by a range is refused",
        applies: |class| is(class, Check::OverRefusal, LISTS, &[Lock::Range]),
    },
    Known {
        finding: "PT38-1: a new item in an XML list of secrets is refused",
        applies: |class| is(class, Check::OverRefusal, &["XML list"], &[Lock::Secret]),
    },
    Known {
        finding: "NEW (PT38-1 in YAML/JSON): a new item above a secret list item is refused",
        applies: |class| {
            is(
                class,
                Check::OverRefusal,
                &["YAML list", "JSON list"],
                &[Lock::Secret],
            )
        },
    },
    Known {
        finding: "NEW: an XML list item locked by path refuses new items and cuts beside it",
        applies: |class| is(class, Check::OverRefusal, &["XML list"], &[Lock::Path]),
    },
    Known {
        finding: "NEW: a new attribute beside a locked XML attribute is refused",
        applies: |class| is(class, Check::OverRefusal, &["XML attribute"], &[]),
    },
    Known {
        finding: "NEW: a secret item's line pasted under a scalar glues the placeholder to it",
        applies: |class| is(class, Check::Glued, &["YAML list"], &[Lock::Secret]),
    },
    Known {
        finding: "NEW: readOnly XML list items can be cut or moved out of the list",
        applies: |class| is(class, Check::LockChanged, &["XML list"], &[Lock::Schema]),
    },
    Known {
        finding: "NEW: a secret's new value is quoted once an edit moves it off its schema path \
                  (a second element makes its parent a list, a broken line a scalar)",
        applies: |class| is(class, Check::Leak, &[], &[Lock::Secret]),
    },
    Known {
        finding: "NEW: Edit-as, a range lock refuses cutting a sibling line after lines moved",
        applies: |class| {
            class.edit_as && is(class, Check::OverRefusal, &["YAML block"], &[Lock::Range])
        },
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
