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
use cases::{BUILT_IN, CASES, LOCKS, value};
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
        match safe(setup, view, protected, &view_original, &text) {
            // The locked text kept, the parse reading it otherwise (a
            // duplicate shadows it, it moved): fine while the change is held
            // with its reason, nothing reaching the host (checked below).
            Err((Check::LockChanged, _))
                if state.changes_held() && state.save_waits().is_some() => {}
            result => {
                result.map_err(|(check, detail)| at((check, format!("on screen: {detail}"))))?;
            }
        }
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

/// A setup on a case in the built-in languages, edited as another
/// language some of the time.
fn draw_setup(rng: &mut Rng) -> Setup {
    let case = rng.below(BUILT_IN);
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

/// A setup on a case in a custom language, shown in its own (custom
/// languages have no Edit-as).
fn draw_custom_setup(rng: &mut Rng) -> Setup {
    Setup {
        case: BUILT_IN + rng.below(CASES.len() - BUILT_IN),
        lock: LOCKS[rng.below(LOCKS.len())],
        host: None,
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

fn explore(
    cases: u64,
    steps: usize,
    first: u64,
    draw_setup: fn(&mut Rng) -> Setup,
) -> BTreeMap<Class, Found> {
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
fn assert_ok(setup: &Setup, acts: &[Act]) {
    if let Err(Failure { class, detail }) = replay(setup, acts) {
        panic!("{class:?}: {detail}");
    }
}

#[test]
fn random_edits_keep_every_protection_and_take_ordinary_edits() {
    explore_and_report(400, draw_setup);
}

#[test]
fn random_edits_keep_every_protection_and_take_ordinary_edits_in_custom_languages() {
    explore_and_report(2000, draw_custom_setup);
}

/// Runs `default` cases (or `PROTECT_FUZZ_CASES`) drawn by `draw_setup`
/// and fails listing every class no known finding explains.
fn explore_and_report(default: u64, draw_setup: fn(&mut Rng) -> Setup) {
    let cases = env_or("PROTECT_FUZZ_CASES", default);
    let steps = env_or("PROTECT_FUZZ_STEPS", 12) as usize;
    let first = env_or("PROTECT_FUZZ_SEED", 1);
    let started = std::time::Instant::now();
    let found = explore(cases, steps, first, draw_setup);
    eprintln!(
        "protect fuzz: {cases} runs of {steps} steps, {} class(es), {:.1}s",
        found.len(),
        started.elapsed().as_secs_f64()
    );
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

/// Seed 8733: a line break typed after a list item's `- ` puts its
/// placeholder on a line of its own (the text then broken); the empty
/// item's line cut makes the list one scalar running over the lines, the
/// token in it with other text: refused.
#[test]
fn a_list_made_one_scalar_by_a_cut_never_glues_a_placeholder() {
    let setup = Setup {
        case: 1,
        lock: cases::Lock::Secret,
        host: None,
        pairs: true,
    };
    assert_ok(
        &setup,
        &[
            Act::ReplaceAll { pair: 5 },
            Act::Entry {
                anchor: 2,
                name: 4,
                paste: false,
            },
            Act::Type { at: 39857, text: 3 },
            Act::Entry {
                anchor: 2,
                name: 6,
                paste: false,
            },
            Act::Cut { line: 40188 },
        ],
    );
}

/// Seed 7633: a placeholder's list item moved down twice (Alt+Down), under
/// `port: 1`, would read `1 - «…»`: refused (while a Replace All's
/// rewritten lines cost no path lookup each, see `link_costs`).
#[test]
fn a_secret_item_moved_under_a_scalar_never_glues_its_placeholder() {
    let setup = Setup {
        case: 1,
        lock: cases::Lock::Secret,
        host: None,
        pairs: true,
    };
    assert_ok(
        &setup,
        &[
            Act::Line {
                line: 24986,
                command: 1,
            },
            Act::Line {
                line: 44715,
                command: 1,
            },
        ],
    );
}

/// GP38-1: in Dot's one-line map, a `,` typed into the placeholder's key
/// and an entry typed before it leave the placeholder in that entry's
/// value with text beside it, and the host hears it.
#[test]
#[ignore = "GP38-1: open"]
fn a_placeholder_in_a_declared_one_line_map_never_glues_into_the_entry_before() {
    let setup = Setup {
        case: 11,
        lock: cases::Lock::Secret,
        host: None,
        pairs: false,
    };
    assert_ok(
        &setup,
        &[
            Act::Type {
                at: 38498,
                text: 10,
            },
            Act::Entry {
                anchor: 0,
                name: 8,
                paste: false,
            },
        ],
    );
}

/// GP38-2: after entries, cuts and a line moved in Odd's list, a `,` typed
/// inside the placeholder's quotes is taken.
#[test]
#[ignore = "GP38-2: open"]
fn a_placeholder_in_a_custom_list_item_never_takes_text_inside_its_quotes() {
    let setup = Setup {
        case: 10,
        lock: cases::Lock::Secret,
        host: None,
        pairs: true,
    };
    assert_ok(
        &setup,
        &[
            Act::Entry {
                anchor: 0,
                name: 0,
                paste: true,
            },
            Act::Entry {
                anchor: 3,
                name: 4,
                paste: true,
            },
            Act::Entry {
                anchor: 0,
                name: 0,
                paste: false,
            },
            Act::Cut { line: 42428 },
            Act::Cut { line: 49829 },
            Act::Line {
                line: 50594,
                command: 2,
            },
            Act::Type {
                at: 19495,
                text: 10,
            },
        ],
    );
}

/// GP38-3: a new entry typed before the placeholder's entry in Dot's
/// one-line map is refused at its closing `;`.
#[test]
#[ignore = "GP38-3: open"]
fn a_new_entry_typed_before_a_secret_in_a_declared_one_line_map_is_taken() {
    let setup = Setup {
        case: 11,
        lock: cases::Lock::Secret,
        host: None,
        pairs: false,
    };
    assert_ok(
        &setup,
        &[Act::Entry {
            anchor: 2,
            name: 5,
            paste: false,
        }],
    );
}

/// GP38-1: a `:` typed into a DotBare key (`pass:word: '«…»'`) after a line
/// moved makes the placeholder part of `pass`'s value with text beside it.
#[test]
#[ignore = "GP38-1: open"]
fn a_placeholder_in_a_declared_language_never_glues_when_a_colon_splits_its_key() {
    let setup = Setup {
        case: 13,
        lock: cases::Lock::Secret,
        host: None,
        pairs: true,
    };
    assert_ok(
        &setup,
        &[
            Act::Line {
                line: 28794,
                command: 2,
            },
            Act::Type { at: 58344, text: 4 },
        ],
    );
}

/// GP38-2: in Odd, a key renamed (`pa1ssword`), its line copied and an
/// entry added, a `-` typed after `:=` glues the placeholder.
#[test]
#[ignore = "GP38-2: open"]
fn a_placeholder_in_a_custom_block_never_glues_after_its_key_is_renamed() {
    let setup = Setup {
        case: 9,
        lock: cases::Lock::Secret,
        host: None,
        pairs: true,
    };
    assert_ok(
        &setup,
        &[
            Act::Type { at: 40974, text: 1 },
            Act::Line {
                line: 21613,
                command: 3,
            },
            Act::Entry {
                anchor: 2,
                name: 2,
                paste: false,
            },
            Act::Type {
                at: 57200,
                text: 11,
            },
        ],
    );
}

/// GP38-3: cutting a duplicate of Dot's one-line map line (holding no
/// locked value) is refused.
#[test]
#[ignore = "GP38-3: open"]
fn a_duplicate_one_line_map_line_holding_no_locked_value_can_be_cut() {
    let setup = Setup {
        case: 11,
        lock: cases::Lock::Path,
        host: None,
        pairs: false,
    };
    assert_ok(
        &setup,
        &[
            Act::Line {
                line: 3694,
                command: 2,
            },
            Act::Type { at: 32739, text: 5 },
            Act::Entry {
                anchor: 0,
                name: 13,
                paste: false,
            },
            Act::NewValue,
            Act::Cut { line: 36204 },
        ],
    );
}

/// Seed 3395: in a YAML view, a key typed at a list item's line start
/// makes the list one scalar running over the items below; the lines still
/// read the placeholder's item path, the document holds the scalar with
/// it and other text: refused.
#[test]
fn a_list_made_one_scalar_by_a_typed_key_never_glues_a_placeholder() {
    let setup = Setup {
        case: 1,
        lock: cases::Lock::Secret,
        host: Some(Language::HCL),
        pairs: true,
    };
    assert_ok(
        &setup,
        &[
            Act::Entry {
                anchor: 3,
                name: 12,
                paste: false,
            },
            Act::Entry {
                anchor: 3,
                name: 1,
                paste: true,
            },
            Act::Pause,
            Act::Type { at: 21715, text: 6 },
        ],
    );
}
