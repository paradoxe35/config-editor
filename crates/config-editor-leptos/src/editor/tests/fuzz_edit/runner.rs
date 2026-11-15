//! The random-test runner: seeded cases, failures grouped into classes,
//! each class shrunk to its smallest case and printed as a test to paste.
//!
//! `FUZZ_EDIT_CASES` sets how many cases each property runs (default: a
//! modest count for CI), `FUZZ_EDIT_SEED` the first seed (default 1), so a
//! run of thousands is `FUZZ_EDIT_CASES=5000 cargo test fuzz_edit`.

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Once;

use super::known;

/// A small deterministic generator (splitmix64): no dependency, the same
/// cases on every run.
pub(super) struct Rng(u64);

impl Rng {
    pub(super) fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub(super) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number below `bound` (0 for a bound of 0).
    pub(super) fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.next() % bound as u64) as usize
        }
    }

    pub(super) fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    pub(super) fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    pub(super) fn position(&mut self) -> u32 {
        (self.next() >> 40) as u32
    }
}

/// Why a case failed: its class (short, the same for every case failing
/// the same way: what failed and in which languages) and what it saw.
#[derive(Clone, Debug)]
pub(super) struct Failure {
    pub(super) class: String,
    pub(super) detail: String,
}

impl Failure {
    pub(super) fn new(class: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            class: class.into(),
            detail: detail.into(),
        }
    }
}

/// A generated case: printable as a test, and smaller cases to try.
pub(super) trait Case: Clone {
    /// The case as a ready-to-paste test body.
    fn code(&self) -> String;
    /// Smaller cases, most promising first.
    fn shrinks(&self) -> Vec<Self>;
    /// A measure of size; shrinking only takes smaller cases.
    fn size(&self) -> usize;
}

/// How many cases each property runs: `FUZZ_EDIT_CASES`, else `default`.
pub(super) fn cases(default: usize) -> usize {
    std::env::var("FUZZ_EDIT_CASES")
        .ok()
        .and_then(|cases| cases.parse().ok())
        .unwrap_or(default)
}

fn first_seed() -> u64 {
    std::env::var("FUZZ_EDIT_SEED")
        .ok()
        .and_then(|seed| seed.parse().ok())
        .unwrap_or(1)
}

thread_local! {
    static QUIET: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Where the last quiet panic was (`file:line`).
    static WHERE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Panics inside a checked case are caught and become failures; their
/// messages go to the report, not to the test output.
fn quiet_panics() {
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        let default = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if QUIET.with(std::cell::Cell::get) {
                let at = info
                    .location()
                    .map(|at| format!("{}:{}", at.file(), at.line()))
                    .unwrap_or_default();
                WHERE.with(|place| *place.borrow_mut() = at);
            } else {
                default(info);
            }
        }));
    });
}

fn checked<C>(check: &dyn Fn(&C) -> Result<(), Failure>, case: &C) -> Result<(), Failure> {
    QUIET.with(|quiet| quiet.set(true));
    let result = catch_unwind(AssertUnwindSafe(|| check(case)));
    QUIET.with(|quiet| quiet.set(false));
    match result {
        Ok(result) => result,
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(ToString::to_string))
                .unwrap_or_default();
            let first = message.lines().next().unwrap_or_default();
            // The class names the panic by its place and message, without
            // the message's numbers (offsets differ) or characters.
            let mut class = String::new();
            for character in first.chars().take(60) {
                let character = if character.is_ascii_digit() {
                    '#'
                } else {
                    character
                };
                if !(character == '#' && class.ends_with('#')) {
                    class.push(character);
                }
            }
            let class = class
                .split(" it is inside")
                .next()
                .unwrap_or_default()
                .to_string();
            let at = WHERE.with(|place| place.borrow().clone());
            let at = at.rsplit("src/").next().unwrap_or_default().to_string();
            Err(Failure::new(format!("panic at {at}: {class}"), message))
        }
    }
}

/// `case` shrunk while it still fails in `class`.
fn shrink<C: Case>(
    check: &dyn Fn(&C) -> Result<(), Failure>,
    mut case: C,
    mut failure: Failure,
) -> (C, Failure) {
    let mut tries = 0;
    'outer: while tries < 2000 {
        for smaller in case.shrinks() {
            if smaller.size() >= case.size() {
                continue;
            }
            tries += 1;
            if let Err(found) = checked(check, &smaller)
                && found.class == failure.class
            {
                case = smaller;
                failure = found;
                continue 'outer;
            }
            if tries >= 2000 {
                break 'outer;
            }
        }
        break;
    }
    (case, failure)
}

struct Class<C> {
    count: usize,
    seed: u64,
    case: C,
    failure: Failure,
}

/// Runs `property` on `count` generated cases, shrinks a case of each
/// failing class, and fails listing every class no known finding excuses.
pub(super) fn run<C: Case>(
    property: &str,
    count: usize,
    generate: &dyn Fn(&mut Rng) -> Option<C>,
    check: &dyn Fn(&C) -> Result<(), Failure>,
) {
    quiet_panics();
    let first = first_seed();
    let salt = property
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
        });
    let mut classes: BTreeMap<String, Class<C>> = BTreeMap::new();
    let (mut ran, mut skipped) = (0, 0);
    let started = std::time::Instant::now();
    for index in 0..count as u64 {
        let seed = first + index;
        let mut rng = Rng::new(seed ^ salt);
        let Some(case) = generate(&mut rng) else {
            skipped += 1;
            continue;
        };
        ran += 1;
        let Err(failure) = checked(check, &case) else {
            continue;
        };
        if let Some(class) = classes.get_mut(&failure.class) {
            class.count += 1;
            continue;
        }
        let (case, failure) = shrink(check, case, failure);
        // Shrinking can move a case into another class; keep both apart.
        classes
            .entry(failure.class.clone())
            .and_modify(|class| class.count += 1)
            .or_insert(Class {
                count: 1,
                seed,
                case,
                failure,
            });
    }
    let mut report = String::new();
    let mut open = 0;
    for (name, class) in &classes {
        let excuse = known::excused(property, name);
        if excuse.is_none() {
            open += 1;
        }
        report.push_str(&format!(
            "\n=== {property}: {name} ({} case(s), first seed {}){}\n{}\n--- smallest case:\n{}\n",
            class.count,
            class.seed,
            excuse.map_or(String::new(), |finding| format!(" KNOWN: {finding}")),
            class.failure.detail,
            class.case.code(),
        ));
    }
    eprintln!(
        "{property}: {ran} case(s) run, {skipped} skipped, {} class(es), {open} open, {:.1}s",
        classes.len(),
        started.elapsed().as_secs_f64()
    );
    if !report.is_empty() {
        eprintln!("{report}");
    }
    assert!(
        open == 0,
        "{property}: {open} open failure class(es):{report}"
    );
}
