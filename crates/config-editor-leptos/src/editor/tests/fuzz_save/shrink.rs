//! A failing session made as small as it still fails the same way (the
//! same promise broken, under the same allowance or none): steps dropped,
//! then the document's lines, then the steps made simpler; printed as a
//! regression test to paste.

use super::{Broke, Case, Op, known, run};

/// How many sessions the shrinker runs at most for one failure.
const BUDGET: usize = 600;

struct Shrinker<'a> {
    original: &'a Broke,
    allowed: Option<&'static str>,
    runs: usize,
}

impl Shrinker<'_> {
    /// Whether `case` fails as the original did; its failure then.
    fn fails(&mut self, case: &Case) -> Option<Broke> {
        if self.runs >= BUDGET {
            return None;
        }
        self.runs += 1;
        match run(case) {
            Err(broke)
                if broke.failure.broken == self.original.failure.broken
                    && spelling(&broke.failure.was) == spelling(&self.original.failure.was)
                    && known::allowed(case, &broke) == self.allowed =>
            {
                Some(broke)
            }
            _ => None,
        }
    }
}

/// A form's value as written (past its key): a shrunk failure loses the
/// same spelling, not another one the shorter session happens to lose.
fn spelling(form: &str) -> &str {
    form.split_once(['=', ':', '>'])
        .map_or(form, |(_, value)| value)
        .trim()
}

/// `case` shrunk, and how it fails then.
pub(super) fn shrink(case: &Case, broke: &Broke) -> (Case, Broke) {
    let mut shrinker = Shrinker {
        original: broke,
        allowed: known::allowed(case, broke),
        runs: 0,
    };
    let mut best = case.clone();
    let mut failure = broke.clone();
    // The steps after the one that broke are no part of it.
    let mut cut = best.clone();
    cut.ops.truncate(broke.step + 1);
    if let Some(found) = shrinker.fails(&cut) {
        best = cut;
        failure = found;
    }
    loop {
        let before = (best.ops.len(), best.text.len());
        // Steps dropped, in halves, then quarters, … then one at a time.
        let mut chunk = best.ops.len().div_ceil(2).max(1);
        while chunk >= 1 {
            let mut at = 0;
            while at < best.ops.len() {
                let mut tried = best.clone();
                let end = (at + chunk).min(tried.ops.len());
                tried.ops.drain(at..end);
                if let Some(found) = shrinker.fails(&tried) {
                    best = tried;
                    failure = found;
                } else {
                    at += chunk;
                }
            }
            if chunk == 1 {
                break;
            }
            chunk /= 2;
        }
        // The document's lines dropped, a block at a time, then one.
        for size in [8, 4, 2, 1] {
            let mut at = 0;
            loop {
                let lines: Vec<&str> = best.text.split_inclusive('\n').collect();
                if at >= lines.len() {
                    break;
                }
                let end = (at + size).min(lines.len());
                let text: String = lines[..at].concat() + &lines[end..].concat();
                let tried = Case {
                    text,
                    ..best.clone()
                };
                if !tried.text.trim().is_empty()
                    && best.host.parse(&tried.text).is_ok()
                    && let Some(found) = shrinker.fails(&tried)
                {
                    best = tried;
                    failure = found;
                } else {
                    at += size;
                }
            }
        }
        // Each step made simpler.
        for at in 0..best.ops.len() {
            let simpler = match best.ops[at] {
                Op::Type { line, column, text } if text != "x" => Some(Op::Type {
                    line,
                    column,
                    text: "x",
                }),
                Op::Backspace {
                    line,
                    column,
                    count,
                } if count > 1 => Some(Op::Backspace {
                    line,
                    column,
                    count: 1,
                }),
                _ => None,
            };
            if let Some(simpler) = simpler {
                let mut tried = best.clone();
                tried.ops[at] = simpler;
                if let Some(found) = shrinker.fails(&tried) {
                    best = tried;
                    failure = found;
                }
            }
        }
        if (best.ops.len(), best.text.len()) == before || shrinker.runs >= BUDGET {
            break;
        }
    }
    (best, failure)
}

/// `case` as a regression test to paste beside this module.
pub(super) fn repro(case: &Case, broke: &Broke) -> String {
    let ops: Vec<String> = case
        .ops
        .iter()
        .map(|op| {
            let written = format!("Op::{op:?}");
            match op {
                Op::Cut { .. } | Op::Copy { .. } => written.replace("how: ", "how: Take::"),
                Op::Paste { .. } => written.replace("how: ", "how: Put::"),
                _ => written,
            }
        })
        .collect();
    // A session under a schema replays with its seed.
    let (replay, seed) = match case.schema {
        Some(seed) => ("replay_typed", format!("\n        {seed},")),
        None => ("replay", String::new()),
    };
    format!(
        "// step {}: {:?} `{}`\n// {}\n// --- {} view:\n// {}\n// --- saved host:\n// {}\n#[test]\nfn fuzz_save_repro() {{\n    {replay}(\n        Language::{},\n        Language::{},{seed}\n        {:?},\n        &[{}],\n    );\n}}",
        broke.step,
        broke.failure.broken,
        broke.failure.was,
        broke.failure.detail.replace('\n', "\n// "),
        broke.failure.view.map_or("", |view| view.label()),
        broke.failure.view_text.replace('\n', "\n// "),
        broke.failure.saved.replace('\n', "\n// "),
        case.host.label(),
        case.view.label(),
        case.text,
        ops.iter()
            .map(|op| format!("\n            {op},"))
            .collect::<String>()
            + if ops.is_empty() { "" } else { "\n        " },
    )
}

/// [`shrink`], then [`repro`].
pub(super) fn shrunk_repro(case: &Case, broke: &Broke) -> String {
    let (case, broke) = shrink(case, broke);
    repro(&case, &broke)
}
