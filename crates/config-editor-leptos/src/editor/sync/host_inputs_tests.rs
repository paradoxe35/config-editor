//! Where a host's lists are read (the rules of `host_inputs`), on one
//! document's channel driven as its editor and a host drive it.

use std::sync::Arc;

use super::{AnchorError, HostUpdate, Link, List, Listed, RangesFor, Reading};
use crate::editor::edit::Change;
use crate::editor::paths::{TextPosition, TextRange as Lines};
use crate::editor::sync::Echo;
use crate::editor::text::{TextRange, TextSize};

fn no_rewrite(_: &str, _: &str, _: &[TextRange]) -> Vec<TextRange> {
    panic!("no text was rewritten")
}

/// Line `line`, columns `from..to` (one-based).
fn on(line: usize, from: usize, to: usize) -> Lines {
    Lines::new(TextPosition::new(line, from), TextPosition::new(line, to))
}

/// The user typed: `text` replaced bytes `start..end` of the document.
fn typed(link: &mut Link, start: u32, end: u32, text: &str) -> String {
    let before = link.log.document().clone();
    let change = Change::replace(
        &before,
        TextRange::new(TextSize::from(start), TextSize::from(end)),
        text,
    );
    let after: Arc<str> = Arc::from(change.apply(&before).expect("made for it"));
    link.log.edited(&change, "", &before, &after);
    after.to_string()
}

/// The user typed, and the editor sent the document: its revision.
fn typed_and_sent(link: &mut Link, start: u32, end: u32, text: &str) -> (String, u64) {
    let document = typed(link, start, end, text);
    let revision = link.sent(&std::sync::Arc::from(document.as_str()));
    (document, revision)
}

/// What the screen locks: each given range's text in the document.
fn locked(link: &Link) -> Vec<String> {
    let (ranges, _) = link.placed(&no_rewrite);
    let document = link.log.document().clone();
    ranges
        .iter()
        .map(|range| document[usize::from(range.start())..usize::from(range.end())].to_string())
        .collect()
}

fn ranges(items: Vec<Lines>) -> Option<Listed<Lines>> {
    Some(Listed::stated(
        items.clone(),
        format!("{items:?}").len() as u64,
    ))
}

fn take(link: &mut Link, update: HostUpdate, current: &str) -> super::Taken {
    link.take(update, current, &no_rewrite).expect("taken")
}

const LOADED: &str = "a: 1\nb: 2\n";

/// A document `LOADED` with `b: 2` locked, read in it.
fn loaded() -> Link {
    let mut link = Link::new(true, LOADED, None, None);
    take(
        &mut link,
        HostUpdate {
            ranges: ranges(vec![on(2, 1, 5)]),
            ..HostUpdate::default()
        },
        LOADED,
    );
    assert_eq!(locked(&link), ["b: 2"]);
    link
}

#[test]
fn the_same_ranges_with_a_new_text_of_the_hosts_own_are_read_in_it() {
    let mut link = loaded();
    // Another file, with the same list: line 2 of it.
    let other = "x: 9\ny: 8\nz: 7\n";
    let taken = take(
        &mut link,
        HostUpdate {
            value: Some(other.to_string()),
            ranges: ranges(vec![on(2, 1, 5)]),
            ..HostUpdate::default()
        },
        LOADED,
    );
    assert!(taken.shown);
    assert_eq!(locked(&link), ["y: 8"]);
}

#[test]
fn the_same_ranges_rendered_with_an_echo_are_no_news() {
    let mut link = loaded();
    // A line typed above: the lock moved with its text.
    let (first, _) = typed_and_sent(&mut link, 0, 0, "top: 0\n");
    let (second, _) = typed_and_sent(&mut link, 0, 0, "#\n");
    assert_eq!(locked(&link), ["b: 2"]);
    // The host renders its first echo late, with the same list: the
    // numbers name `a: 1`'s line there, but the list is given again.
    let taken = take(
        &mut link,
        HostUpdate {
            value: Some(first),
            ranges: ranges(vec![on(2, 1, 5)]),
            ..HostUpdate::default()
        },
        &second,
    );
    assert_ne!(taken.echo, Some(Echo::Apply));
    assert!(!taken.ranges);
    assert_eq!(locked(&link), ["b: 2"]);
}

#[test]
fn ranges_worked_out_on_an_older_revision_land_on_their_text() {
    let mut link = Link::new(true, LOADED, None, None);
    let (_, first) = typed_and_sent(&mut link, 0, 0, "top: 0\n");
    let (second, _) = typed_and_sent(&mut link, 0, 0, "#\n");
    // A server check of the first revision locks its line 3, `b: 2`.
    let taken = take(
        &mut link,
        HostUpdate {
            ranges: ranges(vec![on(3, 1, 5)]),
            ranges_for: Some(RangesFor::Revision(first)),
            ..HostUpdate::default()
        },
        &second,
    );
    assert!(taken.ranges);
    assert_eq!(locked(&link), ["b: 2"]);
}

#[test]
fn an_out_of_date_revision_leaves_the_locks_and_warns() {
    let mut link = loaded();
    let (first, early) = typed_and_sent(&mut link, 0, 0, "top: 0\n");
    // More texts sent since than the log keeps, the last handed back: the
    // first can no longer be named.
    let mut second = String::new();
    let mut late = early;
    for _ in 0..super::super::host_log::VERSIONS_KEPT {
        (second, late) = typed_and_sent(&mut link, 0, 0, "#\n");
    }
    take(
        &mut link,
        HostUpdate {
            value: Some(second.clone()),
            value_revision: Some(late),
            ..HostUpdate::default()
        },
        &second,
    );
    let _ = first;
    let taken = take(
        &mut link,
        HostUpdate {
            ranges: ranges(vec![on(1, 1, 7)]),
            ranges_for: Some(RangesFor::Revision(early)),
            ..HostUpdate::default()
        },
        &second,
    );
    assert_eq!(
        taken.warnings,
        [AnchorError::OutOfDate {
            list: List::Ranges,
            revision: early
        }]
    );
    assert_eq!(locked(&link), ["b: 2"]);
}

#[test]
fn a_revision_never_sent_is_refused_and_changes_nothing() {
    let mut link = loaded();
    let update = HostUpdate {
        ranges: ranges(vec![on(1, 1, 5)]),
        ranges_for: Some(RangesFor::Revision(u64::MAX)),
        ..HostUpdate::default()
    };
    assert!(matches!(
        link.reading(List::Ranges, &update, LOADED),
        Err(AnchorError::Unknown { .. })
    ));
    assert!(link.take(update, LOADED, &no_rewrite).is_err());
    assert_eq!(locked(&link), ["b: 2"]);
    // Diagnostics naming it are refused the same way: the host's mistake
    // is said, not dropped with a warning.
    let update = HostUpdate {
        diagnostics: Some(Listed::new(Vec::new())),
        diagnostics_for: Some(RangesFor::Revision(u64::MAX)),
        ..HostUpdate::default()
    };
    assert!(matches!(
        link.take(update, LOADED, &no_rewrite),
        Err(AnchorError::Unknown {
            list: List::Diagnostics,
            ..
        })
    ));
}

#[test]
fn ranges_for_the_file_as_loaded_land_on_their_text_after_a_hide_and_show() {
    // The parent's state is an echo of an editor that is gone (revision
    // `r`): a fresh editor shows it, the locks given for the file as
    // loaded.
    let state = "top: 0\na: 1\nb: 2\n";
    let mut link = Link::new(true, state, Some(7), None);
    let update = HostUpdate {
        value: Some(state.to_string()),
        value_revision: Some(7),
        ranges: ranges(vec![on(2, 1, 5)]),
        ranges_for: Some(RangesFor::Text(Arc::from(LOADED))),
        ..HostUpdate::default()
    };
    assert_eq!(
        link.reading(List::Ranges, &update, state),
        Ok(Reading::In(Arc::from(LOADED)))
    );
    take(&mut link, update, state);
    assert_eq!(locked(&link), ["b: 2"]);
    // With the revision instead: the numbers are in the state's text.
    let mut link = Link::new(true, state, Some(7), None);
    take(
        &mut link,
        HostUpdate {
            value: Some(state.to_string()),
            value_revision: Some(7),
            ranges: ranges(vec![on(3, 1, 5)]),
            ranges_for: Some(RangesFor::Revision(7)),
            ..HostUpdate::default()
        },
        state,
    );
    assert_eq!(locked(&link), ["b: 2"]);
}

/// P4 and P1: made for a document an editor that is gone showed (its
/// record handed on), while the host still renders an older text, the new
/// document is the newest text the gone editor sent; the locks the host
/// gives with its older text (named by its revision, or not at all) land
/// on their text in it, and the late echoes keep what is typed next.
#[test]
fn a_record_holding_the_newest_text_sent_shows_it_with_the_locks_on_their_text() {
    // What the channel's locks cover in its document (texts no edit made
    // followed by the safe diff, as a binding's record does).
    let texts = |channel: &super::HostChannel| {
        let document = channel.document();
        channel
            .placed()
            .into_iter()
            .map(|(range, _)| document[range].to_string())
            .collect::<Vec<_>>()
    };
    for revision in [true, false] {
        let gone = super::HostChannel::new(true, LOADED, None, None);
        let host = gone.sent_revision().filter(|_| revision);
        gone.edited(0..0, "top: 0\n");
        let first = gone.sent("top: 0\na: 1\nb: 2\n");
        gone.edited(4..5, "9");
        let newest = "top: 9\na: 1\nb: 2\n";
        let second = gone.sent(newest);
        let fresh = super::HostChannel::new(true, LOADED, host, gone.handoff());
        assert_eq!(
            fresh.caught_up().as_deref(),
            Some(newest),
            "revision {revision}"
        );
        assert_eq!(*fresh.document(), *newest);
        fresh
            .take_unmounted(
                HostUpdate {
                    value: Some(LOADED.to_string()),
                    value_revision: host,
                    ranges: ranges(vec![on(2, 1, 5)]),
                    ranges_for: host.map(RangesFor::Revision),
                    ..HostUpdate::default()
                },
                newest,
            )
            .expect("taken");
        assert_eq!(texts(&fresh), ["b: 2"], "revision {revision}");
        // Typed in the new editor, then the gone editor's echoes come.
        fresh.edited(newest.len()..newest.len(), "c: 3\n");
        let typed = format!("{newest}c: 3\n");
        fresh.sent(&typed);
        for (text, sent) in [("top: 0\na: 1\nb: 2\n", first), (newest, second)] {
            let taken = fresh
                .take_unmounted(
                    HostUpdate {
                        value: Some(text.to_string()),
                        value_revision: revision.then_some(sent),
                        ..HostUpdate::default()
                    },
                    &typed,
                )
                .expect("taken");
            assert!(!taken.shown, "revision {revision}: {text:?} is an echo");
        }
        assert_eq!(*fresh.document(), *typed);
        assert_eq!(texts(&fresh), ["b: 2"], "revision {revision}");
    }
}

#[test]
fn a_text_of_the_hosts_own_with_no_ranges_carries_the_locks() {
    let mut link = loaded();
    assert!(link.replaced("z: 0\na: 1\nb: 2\n", LOADED, &no_rewrite));
    assert_eq!(locked(&link), ["b: 2"]);
    // The same list given next is read in the host's text now.
    take(
        &mut link,
        HostUpdate {
            ranges: ranges(vec![on(2, 1, 5)]),
            ..HostUpdate::default()
        },
        "z: 0\na: 1\nb: 2\n",
    );
    assert_eq!(locked(&link), ["a: 1"]);
}

#[test]
fn ranges_named_by_a_revision_the_host_confirms_past_stay_on_their_text() {
    let mut link = Link::new(true, LOADED, None, None);
    let (typed, first) = typed_and_sent(&mut link, 0, 0, "top: 0\n");
    take(
        &mut link,
        HostUpdate {
            ranges: ranges(vec![on(3, 1, 5)]),
            ranges_for: Some(RangesFor::Revision(first)),
            ..HostUpdate::default()
        },
        &typed,
    );
    let (second, late) = typed_and_sent(&mut link, 0, 0, "#\n");
    let (third, _) = typed_and_sent(&mut link, 0, 0, "#\n");
    take(
        &mut link,
        HostUpdate {
            value: Some(second),
            value_revision: Some(late),
            ..HostUpdate::default()
        },
        &third,
    );
    assert_eq!(locked(&link), ["b: 2"]);
    // Past the versions the log keeps: the ranges move on to a kept one.
    let mut last = (third, 0);
    for _ in 0..super::super::host_log::VERSIONS_KEPT + 2 {
        last = typed_and_sent(&mut link, 0, 0, "#\n");
    }
    take(
        &mut link,
        HostUpdate {
            value: Some(last.0.clone()),
            value_revision: Some(last.1),
            ..HostUpdate::default()
        },
        &last.0,
    );
    assert!(link.log.revisions().len() <= super::super::host_log::VERSIONS_KEPT);
    assert_eq!(locked(&link), ["b: 2"]);
}

/// Ranges named by the file as loaded, given again with a text of the
/// host's own (a hide and show's late echo of the replaced editor): the
/// lock moves from the loaded text into the new one, onto its field (not
/// read in the new text where the loaded one had it).
#[test]
fn ranges_named_by_the_loaded_text_follow_it_into_a_text_of_the_hosts_own() {
    let mut link = Link::new(true, LOADED, None, None);
    let given = |value: Option<&str>| HostUpdate {
        value: value.map(str::to_string),
        ranges: ranges(vec![on(2, 1, 5)]),
        ranges_for: Some(RangesFor::Text(Arc::from(LOADED))),
        ..HostUpdate::default()
    };
    take(&mut link, given(None), LOADED);
    assert_eq!(locked(&link), ["b: 2"]);
    let own = format!("x: 0\n{LOADED}");
    let taken = take(&mut link, given(Some(&own)), LOADED);
    assert!(taken.shown);
    assert_eq!(locked(&link), ["b: 2"]);
}

/// A host writing CRLF breaks: the record works in the editor's text (LF),
/// so a lock read by lines lands on its line there, not where the same
/// bytes of the CRLF text are.
#[test]
fn a_crlf_hosts_record_works_in_the_editors_line_breaks() {
    let host = "a: 1\r\nb: 2\r\nc: 3\r\n";
    let channel = super::HostChannel::new(true, host, None, None);
    assert_eq!(&*channel.document(), "a: 1\nb: 2\nc: 3\n");
    channel
        .take_unmounted(
            HostUpdate {
                ranges: ranges(vec![on(3, 1, 5)]),
                ..HostUpdate::default()
            },
            host,
        )
        .expect("taken");
    let document = channel.document();
    let placed: Vec<&str> = channel
        .placed()
        .into_iter()
        .map(|(range, _)| &document[range])
        .collect();
    assert_eq!(placed, ["c: 3"]);
}

/// A reset carries the locks of the list in force: the record names that
/// list (a binding rendering it again past the reset's end skips it), until
/// a list is given.
#[test]
fn a_reset_names_the_list_whose_locks_it_carried() {
    let mut link = loaded();
    let listed = link
        .ranges_listed
        .as_ref()
        .and_then(|(listed, _)| listed.statement);
    assert!(listed.is_some());
    assert!(link.replaced("b: 2\n", LOADED, &no_rewrite));
    assert_eq!(link.carried_list, listed);
    take(
        &mut link,
        HostUpdate {
            ranges: ranges(vec![on(1, 1, 2), on(1, 4, 5)]),
            ..HostUpdate::default()
        },
        "b: 2\n",
    );
    assert_eq!(link.carried_list, None);
}

/// A host that hears nothing (no `on_change`) keeps no version per text
/// sent: kept, the log grew by one version and its steps per edit, for as
/// long as the editor lived.
#[test]
fn a_host_that_hears_nothing_keeps_no_version_per_text_sent() {
    let mut link = Link::new(false, "a: 1\n", None, None);
    for _ in 0..200 {
        typed_and_sent(&mut link, 4, 4, "9");
        link.untold();
        typed_and_sent(&mut link, 4, 5, "");
        link.untold();
    }
    assert!(
        link.log.revisions().len() <= 2,
        "{:?}",
        link.log.revisions()
    );
    assert!(link.log.step_count() <= 2, "{}", link.log.step_count());
    assert_eq!(&**link.log.document(), "a: 1\n");
}

/// A text no host heard that a list in force names stays a version: its
/// positions still map to the document.
#[test]
fn a_text_no_host_heard_stays_while_a_list_names_it() {
    let mut link = Link::new(false, "a: 1\nb: 2\n", None, None);
    let (document, revision) = typed_and_sent(&mut link, 0, 0, "# top\n");
    link.untold();
    take(
        &mut link,
        HostUpdate {
            ranges: ranges(vec![on(3, 1, 5)]),
            ranges_for: Some(RangesFor::Revision(revision)),
            ..HostUpdate::default()
        },
        &document,
    );
    assert_eq!(locked(&link), ["b: 2"]);
    typed_and_sent(&mut link, 0, 0, "x");
    link.untold();
    typed_and_sent(&mut link, 0, 0, "y");
    assert_eq!(link.log.named(revision), super::Named::Kept);
    assert_eq!(locked(&link), ["b: 2"]);
}

/// A host's own new text with its ranges, naming no text (`ranges_for`,
/// `value_revision` left out).
fn own_text_with(link: &mut Link, value: &str, items: Vec<Lines>) -> super::Taken {
    take(
        link,
        HostUpdate {
            value: Some(value.to_string()),
            ranges: Some(Listed::new(items)),
            ..HostUpdate::default()
        },
        LOADED,
    )
}

const OWN: &str = "name: demo\npublic_address: x\n";

#[test]
fn ranges_cutting_a_word_of_an_unnamed_own_text_are_taken_with_a_warning() {
    let mut link = loaded();
    let lic = on(2, 4, 7);
    let taken = own_text_with(&mut link, OWN, vec![on(1, 1, 5), lic]);
    assert_eq!(taken.warnings, [AnchorError::CutsWord { range: lic }]);
    assert!(taken.warnings[0].is_warning());
    let said = taken.warnings[0].to_string();
    assert!(said.contains("2:4-2:7"), "{said}");
    assert!(
        said.contains("rangesFor") && said.contains("valueRevision"),
        "{said}"
    );
    assert!(said.contains("Hosts that echo"), "{said}");
    // Nothing else changes: read in the value, as before.
    assert_eq!(locked(&link), ["name", "lic"]);
}

#[test]
fn ranges_on_whole_words_or_naming_their_text_warn_nothing() {
    // Whole words.
    let mut link = loaded();
    let taken = own_text_with(&mut link, OWN, vec![on(2, 1, 15), on(1, 7, 11)]);
    assert_eq!(taken.warnings, []);
    // Mid word, the text named.
    let mut link = loaded();
    let taken = take(
        &mut link,
        HostUpdate {
            value: Some(OWN.to_string()),
            ranges: Some(Listed::new(vec![on(2, 4, 7)])),
            ranges_for: Some(RangesFor::Text(Arc::from(OWN))),
            ..HostUpdate::default()
        },
        LOADED,
    );
    assert_eq!(taken.warnings, []);
    assert_eq!(locked(&link), ["lic"]);
}

#[test]
fn ranges_cutting_a_word_of_an_echo_warn_nothing() {
    let mut link = loaded();
    let (sent, _) = typed_and_sent(&mut link, 3, 4, "public_address");
    let taken = take(
        &mut link,
        HostUpdate {
            value: Some(sent.clone()),
            ranges: Some(Listed::new(vec![on(1, 6, 9)])),
            ..HostUpdate::default()
        },
        &sent,
    );
    assert_eq!(taken.warnings, []);
}
