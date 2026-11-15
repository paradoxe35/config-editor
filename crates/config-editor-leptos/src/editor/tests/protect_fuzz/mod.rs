//! Random protect edits: documents in every language with a field locked
//! by `read_only_paths`, `read_only_ranges` or the schema's `readOnly`,
//! or holding a secret placeholder (lists of secrets, XML elements and
//! attributes, HCL labelled blocks, TOML tables), shown in their own
//! language or edited as YAML or JSON, auto-closing on or off, edited at
//! random: new entries typed a key at a time or pasted (their names
//! passing through the protected ones), text typed anywhere, lines cut,
//! moved, copied and deleted, the protected line pasted again, Replace
//! all, a new value over the protected one, undo, redo, pauses and saves.
//!
//! After every step: no protected value changed on screen, in what the
//! host heard or in a save; no placeholder has text beside it; no problem
//! quotes a secret's new value; and an ordinary edit (`checks::ordinary`:
//! the text parses before and after, the protected values are as they
//! were, nothing else is lost) is taken whole, so over-refusal fails too.
//!
//! Runs are seeded: `PROTECT_FUZZ_CASES`, `PROTECT_FUZZ_STEPS` and
//! `PROTECT_FUZZ_SEED` run more locally. Each class of failure (the check,
//! the case, the lock, Edit-as or not, the kind of step) prints its
//! smallest run as a test to paste; `known.rs` lists the classes an open
//! finding explains.

mod acts;
mod cases;
mod checks;
mod known;
mod mount;
mod rng;

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use leptos::prelude::*;

use acts::{Act, draw, make, pause};
use cases::{CASES, LOCKS, value};
use checks::{Check, leaks, original, safe};
use mount::{Setup, mount};
use rng::{Rng, env_or, shrink};

use crate::editor::Language;

/// A class of failure: the check, the case, the lock, whether edited as
/// another language, the kind of step.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Class {
    pub(super) check: Check,
    pub(super) case: &'static str,
    lock: cases::Lock,
    pub(super) edit_as: bool,
    pub(super) step: String,
}

struct Failure {
    class: Class,
    detail: String,
}

fn step_kind(act: &Act) -> String {
    let name = format!("{act:?}");
    name.split([' ', '{'])
        .next()
        .unwrap_or_default()
        .to_string()
}

/// Replays `acts` from `setup`: the first failure.
fn replay(setup: &Setup, acts: &[Act]) -> Result<(), Failure> {
    let fail = |check: Check, step: String, detail: String| Failure {
        class: Class {
            check,
            case: setup.case().label,
            lock: setup.lock,
            edit_as: setup.host.is_some(),
            step,
        },
        detail,
    };
    let outcome = catch_unwind(AssertUnwindSafe(|| run(setup, acts)));
    match outcome {
        Ok(Ok(())) => Ok(()),
        Ok(Err((step, check, detail))) => Err(fail(check, step, detail)),
        Err(panic) => {
            let message = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|text| (*text).to_string()))
                .unwrap_or_default();
            Err(fail(Check::Panic, String::new(), message))
        }
    }
}

type Step = (String, Check, String);

fn run(setup: &Setup, acts: &[Act]) -> Result<(), Step> {
    let Some(mounted) = mount(setup) else {
        return Ok(());
    };
    let state = mounted.state;
    let (host, view) = (setup.host_language(), setup.view_language());
    let protected = value(setup.lock);
    let host_original = original(setup, host, protected, &mounted.host_text);
    let view_original = original(setup, view, protected, &state.doc().text.get_untracked());
    let mut heard = (0, 0);
    let mut check_all = |what: &str| -> Result<(), Step> {
        let at = |(check, detail): (Check, String)| (what.to_string(), check, detail);
        let text = state.doc().text.get_untracked();
        safe(setup, view, protected, &view_original, &text)
            .map_err(|(check, detail)| at((check, format!("on screen: {detail}"))))?;
        let changes = mounted.changes.get_untracked();
        let saves = mounted.saves.get_untracked();
        for text in changes[heard.0..].iter().chain(&saves[heard.1..]) {
            safe(setup, host, protected, &host_original, text)
                .map_err(|(check, detail)| at((check, format!("heard: {detail}"))))?;
        }
        heard = (changes.len(), saves.len());
        if setup.lock == cases::Lock::Secret {
            state.problems().diagnosed_text.set(text);
            let messages: Vec<String> = state
                .problems()
                .diagnostics
                .get_untracked()
                .into_iter()
                .map(|problem| problem.message)
                .collect();
            if let Some(message) = leaks(&messages) {
                return Err(at((Check::Leak, message)));
            }
        }
        Ok(())
    };
    for (index, act) in acts.iter().enumerate() {
        let what = step_kind(act);
        make(setup, &mounted, act)
            .map_err(|(check, detail)| (what.clone(), check, format!("step {index}: {detail}")))?;
        check_all(&what)?;
    }
    pause(state);
    state.save();
    check_all("end")
}

fn draw_setup(rng: &mut Rng) -> Setup {
    let case = rng.below(CASES.len());
    let language = CASES[case].language;
    let others = [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
        Language::XML,
    ];
    let edit_as = matches!(language.label(), "YAML" | "JSON") && rng.chance(40);
    let host = edit_as.then(|| {
        let hosts: Vec<Language> = others
            .into_iter()
            .filter(|other| *other != language)
            .collect();
        hosts[rng.below(hosts.len())]
    });
    Setup {
        case,
        lock: LOCKS[rng.below(LOCKS.len())],
        host,
        pairs: rng.chance(50),
    }
}

struct Found {
    hits: usize,
    seed: u64,
    setup: Setup,
    acts: Vec<Act>,
    detail: String,
}

fn explore(cases: u64, steps: usize, first: u64) -> BTreeMap<Class, Found> {
    let mut found: BTreeMap<Class, Found> = BTreeMap::new();
    for seed in first..first + cases {
        let mut rng = Rng::new(seed);
        let setup = draw_setup(&mut rng);
        let acts: Vec<Act> = (0..steps).map(|_| draw(&mut rng)).collect();
        let Err(failure) = replay(&setup, &acts) else {
            continue;
        };
        if let Some(known) = found.get_mut(&failure.class) {
            known.hits += 1;
            continue;
        }
        let class = failure.class.clone();
        let acts = shrink(acts, |fewer| {
            replay(&setup, fewer).is_err_and(|failure| failure.class == class)
        });
        let detail = replay(&setup, &acts)
            .err()
            .map_or(failure.detail, |failure| failure.detail);
        found.insert(
            failure.class,
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

/// `found` as a test to paste.
fn repro(class: &Class, found: &Found) -> String {
    let setup = found.setup;
    let host = setup.host.map_or("None".to_string(), |host| {
        format!("Some(Language::{})", host.label())
    });
    let acts: Vec<String> = found
        .acts
        .iter()
        .map(|act| format!("            Act::{act:?},"))
        .collect();
    format!(
        "    // {class:?} (seed {}): {}\n    #[test]\n    fn repro() {{\n        let setup = Setup {{ case: {}, lock: Lock::{:?}, host: {host}, pairs: {} }};\n        assert_ok(&setup, &[\n{}\n        ]);\n    }}",
        found.seed,
        found.detail,
        setup.case,
        setup.lock,
        setup.pairs,
        acts.join("\n")
    )
}

/// A run that must pass (the repros pasted below).
#[allow(dead_code)]
fn assert_ok(setup: &Setup, acts: &[Act]) {
    if let Err(Failure { class, detail }) = replay(setup, acts) {
        panic!("{class:?}: {detail}");
    }
}

#[test]
fn random_edits_keep_every_protection_and_take_ordinary_edits() {
    let cases = env_or("PROTECT_FUZZ_CASES", 400);
    let steps = env_or("PROTECT_FUZZ_STEPS", 12) as usize;
    let first = env_or("PROTECT_FUZZ_SEED", 1);
    let found = explore(cases, steps, first);
    let mut new = Vec::new();
    for (class, found) in &found {
        let line = format!(
            "{class:?}: {} of {cases} runs\n{}",
            found.hits,
            repro(class, found)
        );
        match known::excused(class) {
            Some(finding) => eprintln!("known ({finding}): {line}"),
            None => new.push(line),
        }
    }
    assert!(new.is_empty(), "new classes:\n{}", new.join("\n\n"));
}
