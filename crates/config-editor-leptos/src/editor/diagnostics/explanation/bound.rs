//! The most explanations one validation gives: a safety net under the
//! once-per-value rules (see `refs`), for every way of collecting errors.
//!
//! The validator collects every error before it returns the first
//! (`jsonschema`'s `iter_errors` fills a list, and an `allOf` walks every
//! member), so taking fewer of them after bounds nothing. Its work grows
//! faster than the document only by reaching a `$ref` target (or an
//! option) again: each is a keyword of ours that is asked first (see
//! `explanation`), and each explanation it allows spends one of a budget
//! linear in the document. Once spent, a target or option says nothing
//! more, and the problems say they are incomplete
//! (`problems_incomplete`).

use std::cell::Cell;

use jsonschema::Keyword;
use jsonschema::error::ValidationError;
use serde_json::Value;

use super::ASKED;

/// Explanations allowed for each value of the document.
const PER_VALUE: usize = 256;

/// Explanations allowed for any document.
const FLOOR: usize = 4096;

thread_local! {
    /// The explanations left in the validation running on this thread.
    static LEFT: Cell<usize> = const { Cell::new(usize::MAX) };
    /// Whether one was refused for the budget.
    static STOPPED: Cell<bool> = const { Cell::new(false) };
}

/// Starts the budget of a validation of `value`.
pub(super) fn start(value: &Value) {
    LEFT.with(|left| left.set(FLOOR.saturating_add(PER_VALUE.saturating_mul(values(value)))));
    STOPPED.with(|stopped| stopped.set(false));
}

/// Spends one explanation: whether one was left.
pub(super) fn spend() -> bool {
    let left = LEFT.with(Cell::get);
    if left == 0 {
        STOPPED.with(|stopped| stopped.set(true));
        return false;
    }
    LEFT.with(|cell| cell.set(left - 1));
    true
}

/// Whether the validation running (or just run) left something out.
pub(super) fn stopped() -> bool {
    STOPPED.with(Cell::get)
}

/// How many values `value` holds, itself too.
fn values(value: &Value) -> usize {
    let mut count = 0;
    let mut stack = vec![value];
    while let Some(value) = stack.pop() {
        count += 1;
        match value {
            Value::Object(members) => stack.extend(members.values()),
            Value::Array(items) => stack.extend(items),
            _ => {}
        }
    }
    count
}

/// The keyword of a `$ref`'s definition in the bounded validator (see
/// `refs::Form::Stopping`): `{"anyOf": [{STOP: n}, {"$ref": target}]}`
/// holds what the target holds, but once the budget is spent, its value
/// (only while errors are collected, see `Asked`).
pub(super) struct Stop(pub(super) u64);

impl<'i> Keyword<'i> for Stop {
    fn validate(&self, _instance: &'i Value) -> Result<(), ValidationError<'i>> {
        Ok(())
    }

    fn is_valid(&self, instance: &'i Value) -> bool {
        let asking = (self.0, std::ptr::from_ref(instance) as usize);
        if ASKED.with(Cell::take) != Some(asking) {
            return false;
        }
        !spend()
    }
}

/// The keyword of a `$ref`'s definition in an explaining validator that
/// explains a target as often as it is reached (see
/// `refs::Form::Counted`): whether the budget allows it.
pub(super) struct Spent(pub(super) u64);

impl<'i> Keyword<'i> for Spent {
    fn validate(&self, _instance: &'i Value) -> Result<(), ValidationError<'i>> {
        Ok(())
    }

    fn is_valid(&self, instance: &'i Value) -> bool {
        let asking = (self.0, std::ptr::from_ref(instance) as usize);
        if ASKED.with(Cell::take) != Some(asking) {
            return true;
        }
        spend()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{spend, start, stopped};

    #[test]
    fn the_budget_grows_with_the_document_and_says_when_it_ran_out() {
        start(&json!([1, 2]));
        let allowed = std::iter::repeat_with(spend)
            .take_while(|left| *left)
            .count();
        assert_eq!(allowed, super::FLOOR + 3 * super::PER_VALUE);
        assert!(stopped());
        start(&json!(1));
        assert!(!stopped());
    }
}
