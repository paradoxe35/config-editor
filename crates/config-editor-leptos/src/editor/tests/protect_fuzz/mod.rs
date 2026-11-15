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
            // duplicate shadows it, it moved), or a placeholder whose text
            // and edges are kept read with text beside it (a key half typed
            // before its entry): fine while the change is held with its
            // reason, nothing reaching the host (checked below).
            Err((Check::LockChanged | Check::Glued, _))
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

/// P1: in Dot's one-line map, a `,` typed into the placeholder's key (its
/// field renamed) and an entry typed before it would leave the placeholder
/// in that entry's value with text beside it (`k: password,: '«…»'`, the
/// path read where it is written only the root): refused.
#[test]
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

/// P1: after entries, cuts and a line moved in Odd's list, a `,` typed
/// inside the placeholder's quotes is refused.
#[test]
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

/// P1 without over-refusal: a new entry typed a key at a time before the
/// placeholder's entry in Dot's one-line map passes through `passwordx:
/// password: '«…»'`, which Dot reads as the token glued into `passwordx`:
/// the placeholder and its edges are as they were, so each key is taken
/// and what reaches the host waits until it reads as the host's again.
/// Seed 778: in Dot's one-line map, a key typed before the locked one's
/// entry (`password db: 'x'; password: 'hunter2'`) leaves the field where
/// its path cannot be found: its lock is the text followed, so the next
/// keys typed are taken.
#[test]
fn a_lock_its_path_cannot_find_while_a_key_is_typed_keeps_its_text() {
    let setup = Setup {
        case: 11,
        lock: cases::Lock::Path,
        host: None,
        pairs: false,
    };
    assert_ok(
        &setup,
        &[
            Act::Entry {
                anchor: 0,
                name: 15,
                paste: false,
            },
            Act::Entry {
                anchor: 0,
                name: 16,
                paste: false,
            },
        ],
    );
}

/// Seed 342: a `:` typed before a Dot list item holding a placeholder
/// makes the parse read it at the root with text beside it, taken and held;
/// an entry typed below meanwhile, through a text that does not parse, is
/// taken too.
#[test]
fn an_entry_typed_while_a_placeholder_reads_otherwise_is_taken() {
    let setup = Setup {
        case: 12,
        lock: cases::Lock::Secret,
        host: None,
        pairs: true,
    };
    assert_ok(
        &setup,
        &[
            Act::Entry {
                anchor: 2,
                name: 17,
                paste: true,
            },
            Act::PasteCopy { line: 31383 },
            Act::Cut { line: 52736 },
            Act::Cut { line: 22254 },
            Act::Type { at: 25516, text: 4 },
            Act::Entry {
                anchor: 2,
                name: 0,
                paste: false,
            },
        ],
    );
}

/// P1, seed 6585: a YAML list item a range covers read into a plain scalar
/// once a line above it is made `<` (its list index moved by the entries
/// added): held, nothing reaching the host, though the host's text has no
/// item at that index either.
#[test]
fn a_range_covered_item_read_otherwise_is_held_whatever_its_index_in_the_hosts_text() {
    let setup = Setup {
        case: 1,
        lock: cases::Lock::Range,
        host: None,
        pairs: false,
    };
    assert_ok(
        &setup,
        &[
            Act::Entry {
                anchor: 0,
                name: 14,
                paste: false,
            },
            Act::Entry {
                anchor: 3,
                name: 17,
                paste: false,
            },
            Act::Type { at: 15657, text: 6 },
            Act::Entry {
                anchor: 3,
                name: 0,
                paste: false,
            },
        ],
    );
}

/// Seed 12922: in Dot's one-line map, a key typed before two entries of the
/// locked key (`password:  password: 'x'; password: '…'`): its path is found
/// at the first, the user's, while the parse reads the last; the lock stays
/// the text followed, so typing goes on.
#[test]
fn a_lock_found_at_a_key_typed_again_keeps_the_text_it_followed() {
    let setup = Setup {
        case: 11,
        lock: cases::Lock::Schema,
        host: None,
        pairs: false,
    };
    assert_ok(
        &setup,
        &[
            Act::Entry {
                anchor: 2,
                name: 16,
                paste: true,
            },
            Act::Entry {
                anchor: 3,
                name: 17,
                paste: true,
            },
            Act::Entry {
                anchor: 0,
                name: 16,
                paste: false,
            },
        ],
    );
}

#[test]
fn a_new_entry_typed_before_a_secret_in_a_declared_one_line_map_is_taken() {
    let setup = Setup {
        case: 11,
        lock: cases::Lock::Secret,
        host: None,
        pairs: false,
    };
    let entry = Act::Entry {
        anchor: 2,
        name: 5,
        paste: false,
    };
    assert_ok(&setup, std::slice::from_ref(&entry));
    // P4: the entry finished, the hold lets go and the host hears it.
    let mounted = mount(&setup).expect("the Dot map mounts");
    make(&setup, &mounted, &entry).expect("each key taken");
    pause(mounted.state);
    assert!(!mounted.state.changes_held(), "released");
    let last = mounted
        .changes
        .with_untracked(|changes| changes.last().cloned());
    assert_eq!(last, Some(mounted.state.doc().text.get_untracked()));
}

/// P1: a `:` typed into a DotBare key (`pass:word: '«…»'`) after a line
/// copied makes the parse read the placeholder as part of `pass`'s value
/// with text beside it; the placeholder and its edges are as they were, so
/// it is taken and held, nothing reaching the host.
#[test]
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

/// P1: in Odd, after the placeholder's key is renamed (`pa1ssword`), its
/// line copied and an entry added, a `-` typed after `:=` would glue the
/// placeholder: refused.
#[test]
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

/// Cutting a duplicate of Dot's one-line map line that the locked one
/// shadows is taken: the lock's key and the key holding it, on its value's
/// line, follow the value to the copy it went to.
#[test]
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

/// P1: in Odd's list, the placeholder's item moved to another index (so it
/// is plain text there) keeps its quotes' inside: a space typed after the
/// token, inside them, is refused (`"«…» "` is not the token).
#[test]
fn a_placeholder_moved_to_another_item_never_takes_a_space_inside_its_quotes() {
    let setup = Setup {
        case: 10,
        lock: cases::Lock::Secret,
        host: None,
        pairs: true,
    };
    assert_ok(
        &setup,
        &[
            Act::Line {
                line: 846,
                command: 1,
            },
            Act::Entry {
                anchor: 0,
                name: 7,
                paste: true,
            },
            Act::Type { at: 60621, text: 2 },
        ],
    );
}

/// P1: a DotBare list item's line made `: * '«…»'` (the text then broken)
/// and moved above its key reads as a field whose value holds the token
/// with other text: refused, though the moved line was in the text before.
#[test]
fn a_line_moved_while_broken_never_glues_a_placeholder() {
    let setup = Setup {
        case: 14,
        lock: cases::Lock::Secret,
        host: None,
        pairs: false,
    };
    assert_ok(
        &setup,
        &[
            Act::Entry {
                anchor: 2,
                name: 5,
                paste: true,
            },
            Act::Entry {
                anchor: 2,
                name: 13,
                paste: true,
            },
            Act::Type { at: 30448, text: 4 },
            Act::Line {
                line: 11167,
                command: 1,
            },
        ],
    );
}

/// P1: in a declared language, a host range over a list item locks that
/// item (its lines found by the declared item marker, or without one as
/// the deeper lines holding no separator): Replace all `e` -> `E` would
/// rename its list (`kEys`), the key holding it: refused.
#[test]
fn a_range_over_a_declared_list_item_keeps_the_key_holding_it() {
    for case in [12, 14] {
        let setup = Setup {
            case,
            lock: cases::Lock::Range,
            host: None,
            pairs: false,
        };
        assert_ok(&setup, &[Act::ReplaceAll { pair: 5 }]);
    }
}

/// Seed 4183: a key added above a range's field in a YAML view and the
/// field's line moved above its neighbour; the pause writes the JSON host
/// with a comma after it (its line written anew). The range is placed
/// again at the field, so a new value over it is still refused after the
/// next edit.
#[test]
fn a_hosts_range_keeps_its_field_through_a_conversion_rewriting_its_line() {
    let setup = Setup {
        case: 0,
        lock: cases::Lock::Range,
        host: Some(Language::JSON),
        pairs: false,
    };
    assert_ok(
        &setup,
        &[
            Act::Entry {
                anchor: 2,
                name: 3,
                paste: false,
            },
            Act::Line {
                line: 43383,
                command: 1,
            },
            Act::Pause,
            Act::Cut { line: 10433 },
            Act::NewValue,
        ],
    );
}

/// P1: in a JSON view of an HCL host that does not parse (` }` typed
/// before a list item a range covers), a new value pasted over the item is
/// refused: an item's key span is the item itself, all of it the value.
#[test]
fn a_new_value_over_a_range_covered_item_in_a_broken_view_is_refused() {
    let setup = Setup {
        case: 3,
        lock: cases::Lock::Range,
        host: Some(Language::HCL),
        pairs: false,
    };
    assert_ok(
        &setup,
        &[
            Act::Type { at: 56928, text: 9 },
            Act::NewValue,
            Act::Cut { line: 65396 },
        ],
    );
}

/// Seed 12127: in a JSON view of a TOML host, the line opening the range's
/// field's block cut (the text broken) and a character typed into the
/// field's key: the field's value is followed past its key, so the field
/// stays locked and, once the text parses without it, held.
#[test]
fn a_range_covered_field_whose_key_changes_in_a_broken_view_stays_locked() {
    let setup = Setup {
        case: 2,
        lock: cases::Lock::Range,
        host: Some(Language::TOML),
        pairs: true,
    };
    assert_ok(
        &setup,
        &[
            Act::Cut { line: 57878 },
            Act::Type { at: 26170, text: 6 },
            Act::Cut { line: 28289 },
        ],
    );
}

/// Seed 4669: an XML list item a range covers copied below, the root line
/// cut, the item moved above its list and that undone: of the two alike
/// lines, the field's lock follows the one its range follows (the nearest
/// to where it was), so the field is still locked when the text parses.
#[test]
fn a_range_covered_item_and_its_range_follow_the_same_copy_of_its_line() {
    let setup = Setup {
        case: 7,
        lock: cases::Lock::Range,
        host: None,
        pairs: false,
    };
    assert_ok(
        &setup,
        &[
            Act::PasteCopy { line: 20908 },
            Act::Line {
                line: 64435,
                command: 3,
            },
            Act::Line {
                line: 41245,
                command: 1,
            },
            Act::Undo,
            Act::Line {
                line: 43085,
                command: 3,
            },
        ],
    );
}

/// Seeds 6192 and 3703: in Dot's one-line map, which reads the last of
/// alike keys, a range's line copied twice: the range follows one copy, the
/// field's lock another, the parse reads the third. An edit of the third
/// changes the value read: held, as the host's ranges are the same ones.
#[test]
fn a_range_covered_field_read_at_another_copy_of_its_line_is_held() {
    for (pairs, acts) in [
        (
            false,
            vec![
                Act::PasteCopy { line: 15525 },
                Act::Line {
                    line: 60486,
                    command: 2,
                },
                Act::Type { at: 4270, text: 1 },
            ],
        ),
        (
            true,
            vec![
                Act::Line {
                    line: 36092,
                    command: 2,
                },
                Act::PasteCopy { line: 31214 },
                Act::Entry {
                    anchor: 2,
                    name: 9,
                    paste: false,
                },
                Act::ReplaceAll { pair: 3 },
            ],
        ),
    ] {
        let setup = Setup {
            case: 11,
            lock: cases::Lock::Range,
            host: None,
            pairs,
        };
        assert_ok(&setup, &acts);
    }
}

/// The sweep's smallest case of a range following a copy of its field's
/// line: in TOML, the range's line pasted again, an entry pasted, and a
/// Replace all changing the copy the parse reads. The field is held
/// wherever the parse reads it, not only where the range went.
#[test]
fn a_range_covered_field_changed_at_the_copy_the_parse_reads_is_held() {
    let setup = Setup {
        case: 4,
        lock: cases::Lock::Range,
        host: None,
        pairs: true,
    };
    assert_ok(
        &setup,
        &[
            Act::PasteCopy { line: 7364 },
            Act::Entry {
                anchor: 2,
                name: 2,
                paste: true,
            },
            Act::ReplaceAll { pair: 5 },
        ],
    );
}
