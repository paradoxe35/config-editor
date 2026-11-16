//! The undo history on its own: steps grouped, sealed, capped, and moved,
//! each popped by the change it recorded. Selections are bytes.

use super::*;

/// A selection as the tests write it: (anchor, caret) bytes.
type Pair = (u32, u32);

fn kept((anchor, caret): Pair) -> Kept {
    (anchor.into(), caret.into())
}

fn pair((anchor, caret): Kept) -> Pair {
    (anchor.into(), caret.into())
}

/// The byte range `start..end`.
fn range(start: u32, end: u32) -> crate::editor::text::TextRange {
    crate::editor::text::TextRange::new(start.into(), end.into())
}

/// The side of a step with the caret at byte `at`.
fn side(selection: Pair) -> Snapshot {
    Snapshot {
        selection: kept(selection),
        language: None,
        folds: None,
        view: ViewState::default(),
    }
}

/// The step `before` → `after`, made on the selection `from`, leaving `to`.
fn made(before: &str, after: &str, from: Pair, to: Pair) -> Made {
    Made {
        before: side(from),
        after: kept(to),
        record: Record::of_texts(before, after),
        between: (0, 0),
    }
}

/// Browser input `before` → `after`, the caret at byte `from` before and
/// `to` after.
fn input(
    history: &mut EditHistory,
    (before, from): (&str, u32),
    (after, to): (&str, u32),
    kind: &str,
    time: f64,
) {
    history.record_input(
        made(before, after, (from, from), (to, to)),
        after,
        (kind, time),
    );
}

/// A command `before` → `after`.
fn command(history: &mut EditHistory, (before, from): (&str, Pair), (after, to): (&str, Pair)) {
    history.record_command(made(before, after, from, to), None);
}

/// Undoes the newest step of `current`: the text it gives back and the
/// selection there.
fn undo(history: &mut EditHistory, current: &str) -> Option<(String, Pair)> {
    let next = history.next(false)?;
    let text = next.change.apply(current)?;
    history.undo_with(current, ViewState::default());
    Some((text, pair(next.to.selection)))
}

/// Redoes the newest undone step of `current`.
fn redo(history: &mut EditHistory, current: &str) -> Option<(String, Pair)> {
    let next = history.next(true)?;
    let text = next.change.apply(current)?;
    history.redo_with(current, ViewState::default());
    Some((text, pair(next.to.selection)))
}

fn at(text: &str, caret: u32) -> Option<(String, Pair)> {
    Some((text.to_string(), (caret, caret)))
}

#[test]
fn typing_pause_beyond_the_grouping_window_splits_undo_steps() {
    assert_eq!(UNDO_GROUPING_WINDOW_MS, 800.0);
    let mut history = EditHistory::default();
    input(&mut history, ("", 0), ("a", 1), "insertText", 0.0);
    input(&mut history, ("a", 1), ("ab", 2), "insertText", 100.0);
    input(&mut history, ("ab", 2), ("abc", 3), "insertText", 1100.0);

    assert_eq!(undo(&mut history, "abc"), at("ab", 2));
    assert_eq!(undo(&mut history, "ab"), at("", 0));
    assert_eq!(undo(&mut history, ""), None);
}

#[test]
fn typing_somewhere_else_is_a_separate_step() {
    let mut history = EditHistory::default();
    input(&mut history, ("ab", 1), ("axb", 2), "insertText", 0.0);
    // The caret moved to the end before the next keystroke.
    input(&mut history, ("axb", 3), ("axby", 4), "insertText", 100.0);
    assert_eq!(undo(&mut history, "axby"), at("axb", 3));
    assert_eq!(undo(&mut history, "axb"), at("ab", 1));
}

#[test]
fn an_ime_composition_is_one_step() {
    let mut history = EditHistory::default();
    let composing =
        |history: &mut EditHistory, before: &str, from: Pair, after: &str, to: u32, time: f64| {
            history.record_input(
                made(before, after, from, (to, to)),
                after,
                ("insertCompositionText", time),
            );
        };
    composing(&mut history, "name: ", (6, 6), "name: に", 9, 0.0);
    // Each update replaces the composed text (selected by the browser).
    composing(&mut history, "name: に", (6, 9), "name: にほ", 12, 2000.0);
    composing(
        &mut history,
        "name: にほ",
        (6, 12),
        "name: にほん",
        15,
        4000.0,
    );
    composing(
        &mut history,
        "name: にほん",
        (6, 15),
        "name: 日本",
        12,
        6000.0,
    );
    assert_eq!(undo(&mut history, "name: 日本"), at("name: ", 6));
}

#[test]
fn a_drag_move_is_one_step() {
    let mut history = EditHistory::default();
    input(
        &mut history,
        ("alpha: one\nb: 2", 7),
        ("alpha: \nb: 2", 7),
        "deleteByDrag",
        0.0,
    );
    input(
        &mut history,
        ("alpha: \nb: 2", 13),
        ("alpha: \nb: 2one", 16),
        "insertFromDrop",
        5.0,
    );
    assert_eq!(
        undo(&mut history, "alpha: \nb: 2one"),
        at("alpha: one\nb: 2", 7)
    );
}

#[test]
fn sealing_splits_consecutive_compositions() {
    let mut history = EditHistory::default();
    input(
        &mut history,
        ("", 0),
        ("日", 3),
        "insertCompositionText",
        0.0,
    );
    history.seal();
    input(
        &mut history,
        ("日", 3),
        ("日本", 6),
        "insertCompositionText",
        10.0,
    );
    assert_eq!(undo(&mut history, "日本"), at("日", 3));
}

#[test]
fn typing_on_past_a_closer_stepped_over_is_one_step() {
    let mut history = EditHistory::default();
    // `"` writes `""`, `k` goes in it, `"` steps over the closer, `:`.
    input(&mut history, ("{}", 1), ("{\"\"}", 2), "insertText", 0.0);
    input(
        &mut history,
        ("{\"\"}", 2),
        ("{\"k\"}", 3),
        "insertText",
        100.0,
    );
    history.stepped_over(3.into(), 4.into(), 200.0);
    input(
        &mut history,
        ("{\"k\"}", 4),
        ("{\"k\":}", 5),
        "insertText",
        300.0,
    );
    // A line break Enter typed goes on with it.
    input(
        &mut history,
        ("{\"k\":}", 5),
        ("{\"k\":\n}", 6),
        "insertLineBreak",
        400.0,
    );
    assert_eq!(undo(&mut history, "{\"k\":\n}"), at("{}", 1));
    // Past a pause, or once the caret moved, stepping over starts nothing.
    let mut history = EditHistory::default();
    input(&mut history, ("{}", 1), ("{\"\"}", 3), "insertText", 0.0);
    history.stepped_over(3.into(), 4.into(), 2000.0);
    input(
        &mut history,
        ("{\"\"}", 4),
        ("{\"\":}", 5),
        "insertText",
        2100.0,
    );
    assert_eq!(undo(&mut history, "{\"\":}"), at("{\"\"}", 4));
    let mut history = EditHistory::default();
    input(&mut history, ("{}", 1), ("{\"\"}", 3), "insertText", 0.0);
    history.stepped_over(1.into(), 2.into(), 100.0);
    input(
        &mut history,
        ("{\"\"}", 2),
        ("{\"\":}", 3),
        "insertText",
        200.0,
    );
    assert_eq!(undo(&mut history, "{\"\":}"), at("{\"\"}", 2));
}

#[test]
fn a_step_typed_back_to_its_start_is_no_step() {
    let mut history = EditHistory::default();
    command(&mut history, ("", (0, 0)), ("\n", (1, 1)));
    // `-` typed, then the Enter ending the list takes it away.
    input(&mut history, ("\n", 0), ("-\n", 1), "insertText", 0.0);
    input(
        &mut history,
        ("-\n", 1),
        ("\n", 0),
        "insertLineBreak",
        100.0,
    );
    assert_eq!(undo(&mut history, "\n"), at("", 0));
    // A drag dropped where it started.
    let mut history = EditHistory::default();
    input(&mut history, ("ab", 0), ("b", 0), "deleteByDrag", 0.0);
    input(&mut history, ("b", 0), ("ab", 1), "insertFromDrop", 5.0);
    assert_eq!(undo(&mut history, "ab"), None);
}

#[test]
fn a_paste_is_its_own_step() {
    let mut history = EditHistory::default();
    input(&mut history, ("", 0), ("hello", 5), "insertFromPaste", 0.0);
    input(
        &mut history,
        ("hello", 5),
        ("hello!", 6),
        "insertText",
        10.0,
    );
    assert_eq!(undo(&mut history, "hello!"), at("hello", 5));
}

#[test]
fn undo_restores_the_selection_and_redo_the_result() {
    let mut history = EditHistory::default();
    command(
        &mut history,
        ("a: 1\nb: 2", (0, 9)),
        ("  a: 1\n  b: 2", (0, 13)),
    );
    assert_eq!(
        undo(&mut history, "  a: 1\n  b: 2"),
        Some(("a: 1\nb: 2".to_string(), (0, 9)))
    );
    assert_eq!(
        redo(&mut history, "a: 1\nb: 2"),
        Some(("  a: 1\n  b: 2".to_string(), (0, 13)))
    );
}

/// A selection made backward (its caret at its start) comes back backward:
/// the history keeps (anchor, caret).
#[test]
fn undo_and_redo_restore_a_backward_selection_backward() {
    let mut history = EditHistory::default();
    command(
        &mut history,
        ("a: 1\nb: 2", (9, 0)),
        ("  a: 1\n  b: 2", (13, 0)),
    );
    assert_eq!(
        undo(&mut history, "  a: 1\n  b: 2"),
        Some(("a: 1\nb: 2".to_string(), (9, 0)))
    );
    assert_eq!(
        redo(&mut history, "a: 1\nb: 2"),
        Some(("  a: 1\n  b: 2".to_string(), (13, 0)))
    );
}

#[cfg(all(feature = "yaml", feature = "json"))]
#[test]
fn a_switch_is_a_step_undone_into_the_language_it_left() {
    use super::Language;
    let mut history = EditHistory::default();
    let yaml = Snapshot {
        language: Some(Language::YAML),
        ..side((0, 0))
    };
    history.record_switch(
        Made {
            before: yaml.clone(),
            after: kept((4, 4)),
            record: Record::of_texts("a: 1\n", "{\"a\": 1}\n"),
            between: (0, 1),
        },
        Language::JSON,
    );
    let next = history.next(false).expect("the switch");
    assert_eq!(next.to, yaml);
    assert_eq!(undo(&mut history, "{\"a\": 1}\n"), at("a: 1\n", 0));
    let next = history.next(true).expect("the switch again");
    assert_eq!(next.to.language, Some(Language::JSON));
    assert_eq!(redo(&mut history, "a: 1\n"), at("{\"a\": 1}\n", 4));
    assert_eq!(
        history.next(false).and_then(|next| next.to.language),
        Some(Language::YAML)
    );
}

#[test]
fn actions_are_separate_steps_and_clear_redo() {
    let mut history = EditHistory::default();
    input(&mut history, ("", 0), ("a", 1), "insertText", 0.0);
    command(&mut history, ("a", (1, 1)), ("a: ", (3, 3)));
    assert_eq!(undo(&mut history, "a: "), at("a", 1));
    command(&mut history, ("a", (1, 1)), ("ab", (2, 2)));
    assert_eq!(redo(&mut history, "ab"), None);
}

/// The steps kept: by count, and by the bytes their changes hold. A step
/// holds what its edit replaced, not the whole text.
#[test]
fn history_is_capped_by_steps_and_by_size_keeping_the_newest() {
    let mut history = EditHistory::default();
    let mut text = String::new();
    for step in 0..MAX_STEPS + 20 {
        let next = format!("{text}{}", step % 10);
        command(&mut history, (&text, (0, 0)), (&next, (0, 0)));
        text = next;
    }
    assert_eq!(history.undo.len(), MAX_STEPS);
    assert_eq!(
        undo(&mut history, &text).map(|(text, _)| text.len()),
        Some(MAX_STEPS + 19)
    );

    // Steps typed into a large text hold only what they typed.
    let mut history = EditHistory::default();
    let large = "x".repeat(MAX_BYTES / 10);
    let mut text = large.clone();
    for step in 0..30 {
        let next = format!("{step}{text}");
        command(&mut history, (&text, (0, 0)), (&next, (0, 0)));
        text = next;
    }
    assert_eq!(history.undo.len(), 30);

    // Steps replacing a large text whole: as many as fit, the newest.
    let mut history = EditHistory::default();
    let mut text = String::new();
    let whole = |step: u8| {
        char::from(b'a' + step)
            .to_string()
            .repeat(MAX_BYTES / 10 + 100)
    };
    for step in 0..26 {
        let next = whole(step);
        command(&mut history, (&text, (0, 0)), (&next, (0, 0)));
        text = next;
    }
    let kept: usize = history
        .undo
        .iter()
        .map(|step| step.record.held_bytes())
        .sum();
    assert!(kept <= MAX_BYTES, "{kept} bytes kept");
    assert_eq!(history.undo.len(), 9);
    assert!(undo(&mut history, &text).is_some_and(|(text, _)| text.starts_with('y')));

    // One step larger than the budget is still undoable.
    let mut history = EditHistory::default();
    let huge = "y".repeat(MAX_BYTES + 1);
    command(&mut history, (&huge, (0, 0)), ("", (0, 0)));
    assert_eq!(history.undo.len(), 1);
}

/// An undo and a redo of a step typed in two edits make exactly the change
/// the step recorded, undone and made again: of two alike lines, the one
/// typed into is the one changed back (a diff of the texts would take the
/// first). A step the host's change carried over keeps its change, rebased.
#[test]
fn undo_and_redo_make_the_change_their_step_recorded() {
    use super::super::edit::Edit;
    let typed = |before: &str, at: usize, text: &str| {
        let after = format!("{}{text}{}", &before[..at], &before[at..]);
        let caret = at + text.len();
        Edit::input(
            before.into(),
            (at, at),
            after,
            (caret, caret),
            "insertText".into(),
            0.0,
        )
    };
    let mut history = EditHistory::default();
    // `a\na\n`: an `a` typed after the second line's `a`, twice.
    let first = typed("a\na\n", 3, "a");
    let second = typed("a\naa\n", 4, "a");
    for (edit, time) in [(&first, 0.0), (&second, 100.0)] {
        let caret = u32::try_from(edit.caret()).expect("a short text");
        history.record_input(
            Made {
                before: side((caret - 1, caret - 1)),
                after: kept((caret, caret)),
                record: Record::undoing(edit).expect("its own text"),
                between: (0, 0),
            },
            &edit.text,
            ("insertText", time),
        );
    }
    assert_eq!(history.undo.len(), 1);
    let undo_change = history.next(false).expect("the undo").change;
    assert_eq!(undo_change.apply("a\naaa\n").as_deref(), Some("a\na\n"));
    assert_eq!(
        undo_change.edits().collect::<Vec<_>>(),
        vec![(range(3, 5), "")]
    );
    undo(&mut history, "a\naaa\n");
    let redo_change = history.next(true).expect("the redo").change;
    assert_eq!(
        redo_change.edits().collect::<Vec<_>>(),
        vec![(range(3, 3), "aa")]
    );
    // Carried over a host change above it: the same edit, moved down.
    redo(&mut history, "a\na\n");
    history.rebase(&Change::diffed("a\naaa\n", "b\na\naaa\n"));
    let next = history.next(false).expect("still a step");
    assert!(next.told);
    assert_eq!(
        next.change.edits().collect::<Vec<_>>(),
        vec![(range(5, 7), "")]
    );
    assert_eq!(
        next.change.apply("b\na\naaa\n").as_deref(),
        Some("b\na\na\n")
    );
}

/// The host's change carries each step on its own: a step apart from it
/// stays although a newer step lies on its other side.
#[test]
fn a_host_change_between_two_steps_keeps_both() {
    let mut history = EditHistory::default();
    // `a: 1\nb: 2\nc: 3\n`: `x` typed after `c: 3`, then after `a: 1`.
    command(
        &mut history,
        ("a: 1\nb: 2\nc: 3\n", (14, 14)),
        ("a: 1\nb: 2\nc: 3x\n", (15, 15)),
    );
    command(
        &mut history,
        ("a: 1\nb: 2\nc: 3x\n", (4, 4)),
        ("a: 1y\nb: 2\nc: 3x\n", (5, 5)),
    );
    // The host writes `b: 9` between them.
    history.rebase(&Change::diffed(
        "a: 1y\nb: 2\nc: 3x\n",
        "a: 1y\nb: 9\nc: 3x\n",
    ));
    assert_eq!(
        undo(&mut history, "a: 1y\nb: 9\nc: 3x\n"),
        at("a: 1\nb: 9\nc: 3x\n", 4)
    );
    assert_eq!(
        undo(&mut history, "a: 1\nb: 9\nc: 3x\n"),
        at("a: 1\nb: 9\nc: 3\n", 14)
    );
}
