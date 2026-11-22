//! Drawing runs, replaying them, and grouping what fails into classes,
//! each with its smallest run, printed as a test to paste.

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::model::{Act, Anchor, BASES, Class, Failure, Form, Outcome, ResetKind, Via};
use crate::rng::{Rng, shrink};
use crate::sim::Sim;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum View {
    Editor,
    Documents,
}

/// What a run starts from.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Setup {
    pub(crate) view: View,
    /// The document's base (`BASES`), and the second document's.
    pub(crate) base: usize,
    pub(crate) other: usize,
    pub(crate) form: Form,
    /// The host keeps `{ text, revision }` and passes `valueRevision`
    /// (else a v0.1.0 host: texts only).
    pub(crate) revisions: bool,
    /// With revisions, the host forgets the newest revision it heard when
    /// it sets a text of its own (see `HostDoc::own`).
    pub(crate) slips: bool,
}

/// Replays `acts` from `setup`, then lets every text come back.
pub(crate) fn replay(setup: Setup, acts: &[Act]) -> Outcome {
    catch_unwind(AssertUnwindSafe(|| run(setup, acts))).unwrap_or_else(|panic| {
        let message = panic
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| panic.downcast_ref::<&str>().map(|text| (*text).to_string()))
            .unwrap_or_default();
        Err(Failure::new(Class::Panic, message))
    })
}

fn run(setup: Setup, acts: &[Act]) -> Outcome {
    let at = |step: usize| {
        move |failure: Failure| Failure {
            step: acts[step].kind(),
            detail: format!("step {step} ({:?}): {}", acts[step], failure.detail),
            ..failure
        }
    };
    let bases: Vec<&str> = match setup.view {
        View::Editor => vec![BASES[setup.base]],
        View::Documents => vec![BASES[setup.base], BASES[setup.other]],
    };
    let mut sim = Sim::new(&bases, setup.form, setup.revisions, setup.slips)?;
    for (step, act) in acts.iter().enumerate() {
        let outcome = sim.step(act);
        trace(act, &sim.describe());
        outcome.map_err(at(step))?;
    }
    sim.drain()
}

/// Prints each step with `HOST_SYNC_TRACE=1` (for a repro run alone).
fn trace(act: &Act, state: &str) {
    if std::env::var_os("HOST_SYNC_TRACE").is_some() {
        eprintln!("{act:?}\n{state}");
    }
}

fn setup(rng: &mut Rng, view: View) -> Setup {
    Setup {
        view,
        base: rng.below(BASES.len()),
        other: rng.below(BASES.len()),
        form: if rng.chance(50) {
            Form::Lines
        } else {
            Form::Index
        },
        revisions: rng.chance(50),
        slips: false,
    }
}

fn anchor(rng: &mut Rng) -> Anchor {
    match rng.below(3) {
        0 => Anchor::None,
        1 => Anchor::Revision,
        _ => Anchor::Text,
    }
}

fn act(rng: &mut Rng, view: View) -> Act {
    let doc = rng.below(2) as u8;
    let same_task = rng.chance(25);
    let roll = rng.below(100);
    match roll {
        0..=24 => Act::Type {
            at: rng.small(),
            ch: ['a', 'b', ':', ' ', 'x'][rng.below(5)],
        },
        25..=31 => Act::Delete {
            at: rng.small(),
            len: rng.below(3) as u8,
        },
        32..=37 => Act::Enter { at: rng.small() },
        38..=41 => Act::MoveLine { line: rng.small() },
        42..=63 => Act::Deliver {
            doc,
            pick: if rng.chance(50) { 0 } else { rng.small() },
            dup: rng.chance(10),
            ranges: rng.chance(80),
            same_task,
            anchor: anchor(rng),
        },
        64..=66 => Act::Drop {
            doc,
            pick: rng.small(),
        },
        67..=75 => Act::Reset {
            doc,
            kind: match rng.below(5) {
                0 => ResetKind::NewVersion(rng.small()),
                1 => ResetKind::Same,
                2 => ResetKind::LastListed,
                3 => ResetKind::Original,
                _ => ResetKind::Twice(rng.small()),
            },
            same_task,
            via: match rng.below(3) {
                0 => Via::ReplaceValue,
                1 => Via::SetValue,
                _ => Via::Both,
            },
        },
        76..=78 => Act::Own {
            doc,
            edit: rng.small(),
            ranges: rng.chance(80),
            same_task,
        },
        79..=84 => Act::Ranges {
            doc,
            anchor: anchor(rng),
            worked_on: rng.small(),
            same_task,
        },
        85..=86 => Act::Remount { doc },
        87 => Act::FreshMount { doc },
        88 => Act::Hide { doc },
        89..=94 => Act::Settle,
        _ if view == View::Documents => Act::Select { doc },
        _ => Act::Settle,
    }
}

/// A class of failure found: how often, and its smallest run.
pub(crate) struct Found {
    pub(crate) hits: usize,
    pub(crate) seed: u64,
    pub(crate) setup: Setup,
    pub(crate) acts: Vec<Act>,
    pub(crate) detail: String,
}

/// Runs `cases` random runs of `steps` actions from seed `first`, and
/// groups the failures by class (each shrunk once, the first found).
/// A class of failure: its kind, and the kind of step it fails at.
pub(crate) type Key = (Class, String);

pub(crate) fn explore(view: View, cases: u64, steps: usize, first: u64) -> BTreeMap<Key, Found> {
    let mut found: BTreeMap<Key, Found> = BTreeMap::new();
    for seed in first..first + cases {
        let mut rng = Rng::new(seed);
        let setup = setup(&mut rng, view);
        let acts: Vec<Act> = (0..steps).map(|_| act(&mut rng, view)).collect();
        // Drawn last, so the runs drawn before it stay as they were.
        let setup = Setup {
            slips: setup.revisions && rng.chance(50),
            ..setup
        };
        let Err(failure) = replay(setup, &acts) else {
            continue;
        };
        let key = (failure.class, failure.step.clone());
        if let Some(known) = found.get_mut(&key) {
            known.hits += 1;
            continue;
        }
        let acts = shrink(acts, |fewer| {
            replay(setup, fewer).is_err_and(|failure| (failure.class, failure.step) == key)
        });
        let detail = replay(setup, &acts)
            .err()
            .map_or(failure.detail, |failure| failure.detail);
        found.insert(
            key,
            Found {
                hits: 1,
                seed,
                setup,
                acts,
                detail,
            },
        );
    }
    found
}

/// `found` as a test to paste into `main.rs`.
pub(crate) fn repro((class, step): &Key, found: &Found) -> String {
    let acts: Vec<String> = found
        .acts
        .iter()
        .map(|act| format!("            Act::{act:?},"))
        .collect();
    format!(
        "    // {class:?} at {step} (seed {}): {}\n    #[test]\n    fn repro_{}_{:?}() {{\n        assert_ok({:?}, &[\n{}\n        ]);\n    }}",
        found.seed,
        found.detail,
        format!("{:?}", found.setup.view).to_lowercase(),
        class,
        found.setup,
        acts.join("\n")
    )
}
