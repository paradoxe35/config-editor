//! Which of a document's errors to show: inside `anyOf`/`oneOf`, the errors
//! of the option the value was meant for (see [`specific_errors`]), each
//! error read once however deep the options go.
//!
//! The explaining validator explains each value once by each option (see
//! [`super::explanation`]); where a chain of options reaches an option and
//! value explained before, the option's errors are an [`unexplained`]
//! error. [`Explanations`] finds the first explanation for each, so every
//! chain reads as explained, as if the validator had explained it again.
//!
//! [`unexplained`]: super::explanation::unexplained

use std::cell::RefCell;
use std::collections::HashMap;

use jsonschema::error::{ValidationError, ValidationErrorKind};

use super::{explanation, wording};

/// The errors of each option and value the explaining validator explained
/// only once, for each place it did not explain them again.
#[derive(Default)]
pub(in crate::editor) struct Explanations<'e> {
    /// By the address of an option's errors that are an unexplained
    /// error: the option's errors where it explained the value.
    first: HashMap<usize, &'e [ValidationError<'e>]>,
}

/// An option (its schema path) and the value it was asked about (its
/// instance path).
type Asking = (String, String);

impl<'e> Explanations<'e> {
    /// The first explanations in `trees`, each the errors of one
    /// validation (the same option and value in two validations are two).
    pub(in crate::editor) fn new(
        trees: impl IntoIterator<Item = &'e [ValidationError<'e>]>,
    ) -> Self {
        let mut first = HashMap::new();
        for tree in trees {
            let mut explained = HashMap::new();
            let mut again = Vec::new();
            read(tree, &mut explained, &mut again);
            for (address, asking) in again {
                if let Some(errors) = explained.get(&asking) {
                    first.insert(address, *errors);
                }
            }
        }
        Self { first }
    }

    /// `branch` (an option's errors), or where it is an unexplained error,
    /// the option's errors where it explained the value.
    pub(in crate::editor) fn branch<'x>(
        &'x self,
        branch: &'x [ValidationError<'x>],
    ) -> &'x [ValidationError<'x>]
    where
        'e: 'x,
    {
        self.first
            .get(&(branch.as_ptr() as usize))
            .map_or(branch, |errors| *errors)
    }

    /// The options' errors of a combinator's `error` (each explained, see
    /// [`Self::branch`]), or none for another error.
    pub(in crate::editor) fn branches<'x>(
        &'x self,
        error: &'x ValidationError<'x>,
    ) -> Option<Vec<&'x [ValidationError<'x>]>>
    where
        'e: 'x,
    {
        match error.kind() {
            ValidationErrorKind::AnyOf { context }
            | ValidationErrorKind::OneOfNotValid { context } => Some(
                context
                    .iter()
                    .map(|branch| self.branch(branch.as_slice()))
                    .collect(),
            ),
            _ => None,
        }
    }
}

/// Records each option's errors in `errors` (in the order validated) by
/// the option and value, the first time; and each unexplained one, by its
/// address, in `again`.
fn read<'e>(
    errors: &'e [ValidationError<'e>],
    explained: &mut HashMap<Asking, &'e [ValidationError<'e>]>,
    again: &mut Vec<(usize, Asking)>,
) {
    for error in errors {
        let (ValidationErrorKind::AnyOf { context }
        | ValidationErrorKind::OneOfNotValid { context }) = error.kind()
        else {
            continue;
        };
        for (index, branch) in context.iter().enumerate() {
            let asking = (
                format!("{}/{index}", error.schema_path().as_str()),
                error.instance_path().as_str().to_string(),
            );
            if branch.iter().any(explanation::unexplained) {
                again.push((branch.as_ptr() as usize, asking));
            } else {
                explained.entry(asking).or_insert(branch.as_slice());
                read(branch, explained, again);
            }
        }
    }
}

/// Whether `branch` (an option's errors) wants another type for the value
/// at `path` (`null` for an object; `1.5` for an option of text and a
/// list of words, whatever else it says): the value was not meant for it.
pub(super) fn wrong_type(branch: &[ValidationError<'_>], path: &str) -> bool {
    branch.iter().any(|nested| {
        matches!(nested.kind(), ValidationErrorKind::Type { .. })
            && nested.instance_path().as_str() == path
    })
}

/// Whether the value of a combinator's `error` is text and an option
/// (one of `branches`) refuses it only for its type, wanting a number or
/// a boolean (options chosen by type: a number, or an expression string):
/// the value may be meant for either.
pub(super) fn text_for_typed_option(
    error: &ValidationError<'_>,
    branches: &[&[ValidationError<'_>]],
) -> bool {
    let path = error.instance_path().as_str();
    error.instance().is_string()
        && branches.iter().any(|branch| {
            wrong_type(branch, path)
                && branch.iter().any(|nested| {
                    wording::listed_types(&nested.to_string()).is_some_and(|types| {
                        types.iter().any(|name| {
                            matches!(name.as_str(), "\"number\"" | "\"integer\"" | "\"boolean\"")
                        })
                    })
                })
        })
}

/// What is shown for an error: itself, the errors shown for each error of
/// one of its options, or one error deeper in it.
#[derive(Clone, Copy)]
enum Shown<'x> {
    Itself,
    Option(usize),
    Deeper(&'x ValidationError<'x>),
}

/// What is shown for an error, how deep the deepest error shown is (0
/// for none), and the last error shown that deep.
#[derive(Clone, Copy)]
struct Pick<'x> {
    shown: Shown<'x>,
    deepest: usize,
    last_deepest: Option<&'x ValidationError<'x>>,
}

impl<'x> Pick<'x> {
    /// `error` itself shown.
    fn itself(error: &'x ValidationError<'x>) -> Self {
        Pick {
            shown: Shown::Itself,
            deepest: depth(error),
            last_deepest: Some(error),
        }
    }
}

/// How deep in the document `error`'s value is.
fn depth(error: &ValidationError<'_>) -> usize {
    error.instance_path().as_str().matches('/').count()
}

/// The errors to show for `error`: inside `anyOf`/`oneOf`, every error of the
/// branch the value was meant for. A branch that fails only because
/// the value has another type (`null` for an object) is skipped, so a typo in
/// an object reports the typo rather than "not valid under any of the schemas".
/// When several options remain and none reaches deeper into the value than
/// the others (the value got further into it), the error itself is shown:
/// it is worded as the options, each with what it misses. An option whose
/// first explanation is not among the errors (see [`Explanations`]) is
/// meant only when no other is. Each error inside is read once.
pub(in crate::editor) fn specific_errors<'x>(
    explanations: &'x Explanations<'_>,
    error: &'x ValidationError<'x>,
) -> Vec<&'x ValidationError<'x>> {
    let picker = Picker {
        explanations,
        picked: RefCell::new(HashMap::new()),
    };
    picker.pick(error);
    let mut shown = Vec::new();
    picker.shown(error, &mut shown);
    shown
}

/// The picks of one call of [`specific_errors`], by error address.
struct Picker<'x, 'e> {
    explanations: &'x Explanations<'e>,
    /// `None` while the error's pick is being made.
    picked: RefCell<HashMap<usize, Option<Pick<'x>>>>,
}

impl<'x> Picker<'x, '_> {
    /// What is shown for `error`, picked once.
    fn pick(&self, error: &'x ValidationError<'x>) -> Pick<'x> {
        let address = std::ptr::from_ref(error) as usize;
        if let Some(known) = self.picked.borrow().get(&address) {
            // An option holding itself for the same value: itself.
            return known.unwrap_or_else(|| Pick::itself(error));
        }
        self.picked.borrow_mut().insert(address, None);
        let pick = self.make(error);
        self.picked.borrow_mut().insert(address, Some(pick));
        pick
    }

    fn make(&self, error: &'x ValidationError<'x>) -> Pick<'x> {
        super::super::work_count::note(super::super::work_count::Work::ErrorVisit);
        let itself = Pick::itself(error);
        let Some(branches) = self.explanations.branches(error) else {
            return itself;
        };
        let path = error.instance_path().as_str();
        // Text where an option wants a number: the error itself, worded as
        // every option (the number one too), not the text option's alone.
        if text_for_typed_option(error, &branches) {
            return itself;
        }
        // How deep each branch's deepest error shown is, and the last
        // that deep.
        let resolved: Vec<(usize, Option<&'x ValidationError<'x>>)> = branches
            .iter()
            .map(|branch| {
                let mut deepest = (0, None);
                for nested in branch.iter() {
                    let pick = self.pick(nested);
                    if pick.last_deepest.is_some()
                        && (deepest.1.is_none() || pick.deepest >= deepest.0)
                    {
                        deepest = (pick.deepest, pick.last_deepest);
                    }
                }
                deepest
            })
            .collect();
        let explained =
            |branch: &[ValidationError<'_>]| !branch.iter().any(explanation::unexplained);
        let any_explained = branches.iter().any(|branch| explained(branch));
        let meant: Vec<(usize, usize)> = branches
            .iter()
            .enumerate()
            .filter(|(_, branch)| {
                !wrong_type(branch, path) && (explained(branch) || !any_explained)
            })
            .map(|(index, _)| (index, resolved[index].0))
            .collect();
        // The branch the value was meant for: every one of its errors is shown,
        // so a second problem is not hidden behind the first.
        if let Some(&(index, deepest)) = meant.iter().max_by_key(|(_, deepest)| *deepest) {
            let alone = meant.len() == 1
                || (deepest > itself.deepest
                    && meant.iter().filter(|(_, other)| *other == deepest).count() == 1);
            return if alone && explained(branches[index]) {
                Pick {
                    shown: Shown::Option(index),
                    deepest,
                    last_deepest: resolved[index].1,
                }
            } else {
                itself
            };
        }
        // Without one, only a deeper error says more than "wrong type".
        match resolved
            .iter()
            .filter_map(|(deepest, last)| Some((*deepest, (*last)?)))
            .reduce(|kept, next| if next.0 >= kept.0 { next } else { kept })
        {
            Some((deepest, nested)) if deepest > itself.deepest => Pick {
                shown: Shown::Deeper(nested),
                deepest,
                last_deepest: Some(nested),
            },
            _ => itself,
        }
    }

    /// Appends the errors shown for `error` (picked) to `shown`.
    fn shown(&self, error: &'x ValidationError<'x>, shown: &mut Vec<&'x ValidationError<'x>>) {
        let pick = self
            .picked
            .borrow()
            .get(&(std::ptr::from_ref(error) as usize))
            .copied()
            .flatten();
        match pick.map(|pick| pick.shown) {
            Some(Shown::Option(index)) => {
                let branches = self.explanations.branches(error).unwrap_or_default();
                for nested in branches[index] {
                    self.shown(nested, shown);
                }
            }
            Some(Shown::Deeper(nested)) => shown.push(nested),
            Some(Shown::Itself) | None => shown.push(error),
        }
    }
}
