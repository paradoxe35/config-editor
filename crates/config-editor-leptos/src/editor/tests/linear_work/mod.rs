//! Every kind of edit costs work that grows with the document no faster
//! than the document: one standing matrix, counted (see `work_count`), so
//! a slowdown like QA 36's SP36-1 (a secret line's copies, quadratic),
//! SP36-2 (the suggestions parsing again), QA 37's SP37-1 (a large edit
//! in an Edit-as view going through the item trail) or SC37-2 (the
//! validation's error path, exponential in a recursive `anyOf`) fails here
//! without a tester.
//!
//! The matrix: native editing in every language and Edit-as for every
//! host and view, × every [`EditKind`], × every [`Shape`] it applies to.
//! Each case runs at two sizes (`SIZES`, `DEEP_SIZES`), the second twice
//! the first, and each kind of work (and their total) may grow at most
//! [`RATIO`] times. A kind with a real reason to grow faster, or an open
//! finding, gets an entry in `known.rs` (with an ignored test here for a
//! finding), never a blanket exemption.
//!
//! A new edit kind: a variant of [`EditKind`] (its edit in
//! `edits::make`, any setup in `edits::prepare`), and a test in the
//! `linear!` list below.

mod edits;
mod known;
mod shapes;

use leptos::prelude::*;

use crate::editor::languages::Language;
use crate::editor::secret_options::SecretOptions;
use crate::editor::test_state::TestEditor;
use crate::editor::work_count::{Work, counted_all};
use edits::{EditKind, make, pause, prepare, read_view};
use shapes::Shape;

/// The sizes every case runs at.
const SIZES: [usize; 2] = [12, 24];

/// The depths a [`Shape::Deep`] value runs at.
const DEEP_SIZES: [usize; 2] = [3, 6];

/// How many halves a kind of work may grow by when the document doubles:
/// 2.5 times (linear work doubles; quadratic work quadruples).
const RATIO: usize = 5;

/// Counts this small at the smaller size are compared as if this large:
/// a constant few more steps are not growth.
const FLOOR: usize = 16;

/// One case of the matrix.
#[derive(Clone, Copy, Debug)]
struct Case {
    host: Language,
    view: Option<Language>,
    kind: EditKind,
    shape: Shape,
}

impl std::fmt::Display for Case {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.view {
            Some(view) => write!(f, "{} as {view}", self.host)?,
            None => write!(f, "{}", self.host)?,
        }
        write!(f, ", {:?}, {:?}", self.kind, self.shape)
    }
}

/// The shapes `kind` is measured on.
fn shapes_of(kind: EditKind) -> &'static [Shape] {
    match kind {
        EditKind::Validate => &[
            Shape::Same,
            Shape::Distinct,
            Shape::Secrets,
            Shape::Locks,
            Shape::Deep,
        ],
        _ => &[Shape::Same, Shape::Distinct, Shape::Secrets, Shape::Locks],
    }
}

/// Every case of `kind`: native editing in each language, then each host
/// seen as each other language.
fn cases(kind: EditKind) -> Vec<Case> {
    let mut cases = Vec::new();
    for &host in Language::ALL {
        for view in std::iter::once(None).chain(Language::ALL.iter().map(|view| Some(*view))) {
            if view == Some(host) {
                continue;
            }
            for &shape in shapes_of(kind) {
                cases.push(Case {
                    host,
                    view,
                    kind,
                    shape,
                });
            }
        }
    }
    cases
}

/// The work `case` costs at `size`, by [`Work::ALL`].
fn work(
    case: &Case,
    size: usize,
    schema: &std::sync::Arc<crate::editor::diagnostics::SchemaBundle>,
) -> Vec<usize> {
    let mut text = shapes::text(case.host, case.shape, size);
    if case.shape == Shape::Secrets && case.view.is_none() {
        text = shapes::with_copies(case.host, &text, size);
    }
    let mut editor = TestEditor {
        edit_as: case.view,
        schema: Some(schema.clone()),
        on_save: Some(Callback::new(|_| {})),
        ..TestEditor::new(case.host, &text)
    };
    if case.shape == Shape::Secrets {
        editor.secrets = Some(SecretOptions::default().with_placeholder(shapes::PLACEHOLDER));
    }
    if case.shape == Shape::Locks {
        editor.read_only_ranges = shapes::locks(&text);
    }
    let opened = |editor: TestEditor| {
        let (owner, state) = editor.mount();
        read_view(state);
        pause(state);
        (owner, state)
    };
    if case.kind == EditKind::Open {
        let (_opened, counts) = counted_all(|| opened(editor));
        return counts.to_vec();
    }
    let (_owner, state) = opened(editor);
    prepare(state, case.kind);
    let last = if case.shape == Shape::Secrets {
        "token"
    } else {
        "mode"
    };
    counted_all(|| make(state, case.kind, last)).1.to_vec()
}

/// What grew too fast in `case` (each kind of work, then the total, with
/// its counts at both sizes), less what an entry of `known` excuses
/// unless `whole`.
fn grown(
    case: &Case,
    schema: &std::sync::Arc<crate::editor::diagnostics::SchemaBundle>,
    whole: bool,
) -> Vec<String> {
    let sizes = if case.shape == Shape::Deep {
        DEEP_SIZES
    } else {
        SIZES
    };
    let [small, large] = sizes.map(|size| work(case, size, schema));
    let named = Work::ALL.iter().map(|work| Some(*work)).chain([None]);
    let total = |counts: &[usize]| counts.iter().sum::<usize>();
    let pairs = small
        .iter()
        .zip(&large)
        .map(|(small, large)| (*small, *large))
        .chain([(total(&small), total(&large))]);
    named
        .zip(pairs)
        .filter(|(_, (small, large))| 2 * large > RATIO * small.max(&FLOOR))
        .filter(|(work, _)| whole || known::excused(case, *work).is_none())
        .map(|(work, (small, large))| {
            let work = work.map_or("total".to_string(), |work| format!("{work:?}"));
            format!(
                "{case}: {work} {small} at size {} then {large} at {}",
                sizes[0], sizes[1]
            )
        })
        .collect()
}

/// Every case of `kind` grows no faster than its document (less what an
/// open finding excuses).
fn assert_linear(kind: EditKind) {
    crate::editor::parse_cache::hold();
    let schema = shapes::schema();
    let failures: Vec<String> = cases(kind)
        .iter()
        .flat_map(|case| grown(case, &schema, false))
        .collect();
    crate::editor::parse_cache::release();
    assert!(
        failures.is_empty(),
        "work growing faster than the document:\n{}",
        failures.join("\n")
    );
}

/// The cases `finding` excuses grow no faster than their document.
fn assert_fixed(finding: &str) {
    crate::editor::parse_cache::hold();
    let schema = shapes::schema();
    let failures: Vec<String> = EditKind::ALL
        .into_iter()
        .flat_map(cases)
        .filter(|case| known::of(case, finding))
        .flat_map(|case| grown(&case, &schema, true))
        .collect();
    crate::editor::parse_cache::release();
    assert!(
        failures.is_empty(),
        "{finding}: work growing faster than the document:\n{}",
        failures.join("\n")
    );
}

// The open findings' cases, whole: each mark goes with the finding's
// entry in `known.rs` once its fix is in.

#[test]
#[ignore = "SP37-1: a large edit in an Edit-as view goes through the item trail (quadratic, cubic with items alike)"]
fn sp37_1_edits_in_a_view_trace_the_items_in_linear_work() {
    assert_fixed("SP37-1");
}

#[test]
#[ignore = "SC37-2: the validation's error path is exponential in a recursive anyOf"]
fn sc37_2_a_recursive_any_of_is_validated_in_linear_work() {
    assert_fixed("SC37-2");
}

macro_rules! linear {
    ($($name:ident: $kind:ident),* $(,)?) => {
        $(
            #[test]
            fn $name() {
                assert_linear(EditKind::$kind);
            }
        )*
    };
}

linear! {
    opening_grows_with_the_document: Open,
    typing_grows_with_the_document: Type,
    enter_grows_with_the_document: Enter,
    deleting_a_line_grows_with_the_document: DeleteLine,
    cut_pause_paste_grows_with_the_document: CutPausePaste,
    cut_paste_grows_with_the_document: CutPaste,
    moving_a_line_grows_with_the_document: MoveLine,
    copying_a_line_grows_with_the_document: CopyLine,
    replace_all_grows_with_the_document: ReplaceAll,
    pasting_the_whole_text_grows_with_the_document: PasteWhole,
    undo_redo_grows_with_the_document: UndoRedo,
    suggestions_grow_with_the_document: Suggest,
    hover_grows_with_the_document: Hover,
    validation_grows_with_the_document: Validate,
}
