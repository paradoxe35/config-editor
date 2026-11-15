//! Broken saves of open findings (or of classes this test found, not
//! fixed yet), allowed by name so the standing test guards every other
//! kind. The first entry that applies names a failure; the shrinker keeps
//! a failure under its entry. Each fix removes its entry; the test then
//! fails on what the fix left.

use serde_json::Value;

use super::{Broke, Broken, Case, Op, Put};
use crate::editor::languages::Language;

/// An open finding's broken saves.
struct Known {
    /// The finding (or the class id this test gave it).
    finding: &'static str,
    applies: fn(&Case, &Broke) -> bool,
}

const KNOWN: &[Known] = &[
    Known {
        finding: "FZ38-1 edits in the host's language undone by the next save of a view",
        applies: |_, broke| {
            broke.edited_natively && kinds(broke, &[Broken::Form, Broken::Comment, Broken::Meaning])
        },
    },
    Known {
        finding: "E38-1 a key pasted over a selected line in a JSON view loses its form",
        applies: |case, broke| {
            kinds(broke, &[Broken::Form])
                && (case.view == Language::JSON
                    || steps(case, broke, |op| *op == Op::Switch { to: 1 }))
                && steps(case, broke, |op| {
                    matches!(
                        op,
                        Op::Paste {
                            how: Put::OverLine | Put::OverBlank,
                            ..
                        }
                    )
                })
        },
    },
    Known {
        finding: "FZ38-3 a block or element emptied in a view is dropped from the host (or the whole document is)",
        applies: |case, broke| {
            (kinds(broke, &[Broken::Meaning])
                && (broke.failure.saved.trim().is_empty()
                    || case.host.parse(&broke.failure.saved).is_ok_and(|value| {
                        value.is_null() || value.as_object().is_some_and(|map| map.is_empty())
                    })))
                || (case.host == Language::XML
                    && kinds(broke, &[Broken::Meaning, Broken::SilentWait])
                    && view_value(broke).is_some_and(|value| {
                        value
                            .as_object()
                            .is_some_and(|map| map.values().all(Value::is_null))
                    }))
                || (kinds(broke, &[Broken::Meaning])
                    && view_value(broke).is_some_and(|value| holds_empty_map(&value)))
        },
    },
    Known {
        finding: "FZ38-4 a value whose block, table or element changes (renamed, its header deleted, or a line moved across a header) is rewritten",
        applies: |case, broke| {
            (kinds(broke, &[Broken::Form]) && broke.failure.new_parent)
                || (kinds(broke, &[Broken::Form]) && renamed(case, broke))
        },
    },
    Known {
        finding: "FZ38-13 a comment typed right after a value in a view is saved into the value",
        applies: |case, broke| {
            kinds(broke, &[Broken::Meaning])
                && steps(case, broke, |op| matches!(op, Op::Type { text: "#", .. }))
        },
    },
    Known {
        finding: "FZ38-6 an undo after a save rewrites untouched entries (spellings, one-line or block)",
        applies: |case, broke| {
            kinds(broke, &[Broken::Form]) && steps(case, broke, |op| *op == Op::Undo)
        },
    },
    Known {
        finding: "FZ38-14 (E38-1 in the TOML and XML views) a key pasted over a selected line loses its form",
        applies: |case, broke| {
            kinds(broke, &[Broken::Form])
                && steps(case, broke, |op| {
                    matches!(
                        op,
                        Op::Paste {
                            how: Put::OverLine | Put::OverBlank,
                            ..
                        }
                    )
                })
        },
    },
    Known {
        finding: "FZ38-8 an edit to a list item rewrites the item's other values",
        applies: |_, broke| {
            kinds(broke, &[Broken::Form])
                && !broke.failure.moved
                && broke
                    .failure
                    .pointer
                    .split('/')
                    .any(|segment| segment.parse::<usize>().is_ok())
        },
    },
    Known {
        finding: "FZ38-9 (E38-5 and wider) a value or list item moved or copied in a view loses its form, or takes another item's tag",
        applies: |_, broke| kinds(broke, &[Broken::Form]) && broke.failure.moved,
    },
    Known {
        finding: "SV38-19 a label typed onto an HCL block (`spec x{`) saves a YAML host's map as a list",
        applies: |case, broke| {
            case.host == Language::YAML
                && broke.failure.view == Some(Language::HCL)
                && kinds(broke, &[Broken::Meaning])
                && broke.failure.view_text.lines().any(|line| {
                    let head = line.trim().trim_end_matches('{');
                    line.trim_end().ends_with('{') && head.split_whitespace().count() >= 2
                })
        },
    },
];

/// The finding allowing `broke` in `case`, if one does.
pub(super) fn allowed(case: &Case, broke: &Broke) -> Option<&'static str> {
    KNOWN
        .iter()
        .find(|known| (known.applies)(case, broke))
        .map(|known| known.finding)
}

fn kinds(broke: &Broke, kinds: &[Broken]) -> bool {
    kinds.contains(&broke.failure.broken)
}

/// Whether a step up to the one that broke is `wanted`.
fn steps(case: &Case, broke: &Broke, wanted: fn(&Op) -> bool) -> bool {
    case.ops[..(broke.step + 1).min(case.ops.len())]
        .iter()
        .any(wanted)
}

/// The document the view showed when the save broke.
fn view_value(broke: &Broke) -> Option<Value> {
    broke.failure.view?.parse(&broke.failure.view_text).ok()
}

/// Whether the saved host has a key the session did not start with, in a
/// map it started with (a key renamed, or a table header renamed so its
/// keys went to a new one).
fn renamed(case: &Case, broke: &Broke) -> bool {
    let (Ok(before), Ok(after)) = (
        case.host.parse(&case.text),
        case.host.parse(&broke.failure.saved),
    ) else {
        return false;
    };
    fn walk(before: &Value, after: &Value) -> bool {
        match (before, after) {
            (Value::Object(old), Value::Object(new)) => {
                new.keys().any(|key| !old.contains_key(key))
                    || old
                        .iter()
                        .any(|(key, child)| new.get(key).is_some_and(|other| walk(child, other)))
            }
            (Value::Array(old), Value::Array(new)) => {
                old.iter().zip(new).any(|(child, other)| walk(child, other))
            }
            _ => false,
        }
    }
    walk(&before, &after)
}

fn holds_empty_map(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.is_empty() || map.values().any(holds_empty_map),
        Value::Array(items) => items.iter().any(holds_empty_map),
        _ => false,
    }
}
