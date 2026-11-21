//! The host log: texts rebuilt for every version kept, and positions of
//! any version mapped to the document onto the same characters, on random
//! edits, sends, confirmations and texts of the host's own (seeded, so a
//! failure repeats).

use std::sync::Arc;

use super::super::echoes::issue;
use super::{HostLog, Named, VERSIONS_KEPT};
use crate::editor::edit::Change;
use crate::editor::text::{TextRange, TextSize};

fn r(start: u32, end: u32) -> TextRange {
    TextRange::new(start.into(), end.into())
}

fn no_rewrite(_: &str, _: &str, _: &[TextRange]) -> Vec<TextRange> {
    panic!("no text was rewritten")
}

/// `log`'s document after `text` replaced `range` of it.
fn edit(log: &mut HostLog, range: TextRange, text: &str) {
    let before = log.document().clone();
    let change = Change::replace(&before, range, text);
    let after: Arc<str> = Arc::from(change.apply(&before).expect("made for it"));
    log.edited(&change, "", &before, &after);
}

#[test]
fn every_text_sent_is_rebuilt_from_the_document() {
    let mut log = HostLog::new(Arc::from("port: 1\n"), issue(), true);
    let own = log.host_revision();
    edit(&mut log, r(6, 7), "22");
    let first = issue();
    log.sent(first, &log.document().clone(), None);
    edit(&mut log, r(0, 0), "# top\n");
    let second = issue();
    log.sent(second, &log.document().clone(), None);
    edit(&mut log, r(12, 14), "3");
    assert_eq!(log.text_at(own).as_deref(), Some("port: 1\n"));
    assert_eq!(log.text_at(first).as_deref(), Some("port: 22\n"));
    assert_eq!(log.text_at(second).as_deref(), Some("# top\nport: 22\n"));
    assert_eq!(&**log.document(), "# top\nport: 3\n");
}

/// A save held keeps the host on the text it heard last; a host its echoes
/// left on an older text hears that text again. That copy is a version of
/// the same text, the held edits still after it: the edits since it stay
/// exact (a host change is rebased over them, a lock follows them), at any
/// number of copies and edits between them.
#[test]
fn the_host_text_sent_again_keeps_the_edits_since_it() {
    for copies in 1..4u32 {
        let mut log = HostLog::new(Arc::from("a: 1\nkey: v\n"), issue(), true);
        let heard = issue();
        log.sent(heard, &log.document().clone(), None);
        let held: Arc<str> = log.document().clone();
        let mut again = heard;
        for copy in 0..copies {
            // Held edits: a line above the lock, and a word inside the key.
            edit(&mut log, r(0, 0), "# held\n");
            let at = log.document().find("ey: v").expect("the key") as u32;
            edit(&mut log, r(at, at), "x");
            again = issue();
            log.sent(again, &held, None);
            assert_eq!(log.text_at(again).as_deref(), Some(&*held), "copy {copy}");
            assert_eq!(log.text_at(heard).as_deref(), Some(&*held), "copy {copy}");
        }
        assert_eq!(log.host_revision(), again);
        let since = log.change_since(again).expect("the held edits, exactly");
        assert_eq!(since.apply(&held).as_deref(), Some(&**log.document()));
        // `key` of the text the host holds, on the same characters.
        let mapped = log.map(again, &[r(5, 8)], &no_rewrite).expect("kept");
        let document = log.document().clone();
        let [range] = mapped[..] else {
            panic!("one range: {mapped:?}")
        };
        let locked = &document[usize::from(range.start())..usize::from(range.end())];
        assert_eq!(
            locked.replace('x', ""),
            "key",
            "{copies} copies: {document:?}"
        );
    }
}

#[test]
fn positions_of_an_older_text_land_on_the_same_characters() {
    let mut log = HostLog::new(Arc::from("a: 1\nb: 2\n"), issue(), true);
    let given = log.host_revision();
    // A line above, then the value beside the range changed.
    edit(&mut log, r(0, 0), "z: 0\n");
    log.sent(issue(), &log.document().clone(), None);
    edit(&mut log, r(8, 9), "11");
    // `b: 2` of the text given.
    let mapped = log.map(given, &[r(5, 9)], &no_rewrite);
    assert_eq!(mapped, Some(vec![r(11, 15)]));
    assert_eq!(&log.document()[11..15], "b: 2");
}

/// A late check of an older text names it: a revision handed back keeps
/// the ones before it while the log holds no more than `VERSIONS_KEPT`.
#[test]
fn a_confirmed_revision_keeps_the_ones_before_it_within_the_window() {
    let mut log = HostLog::new(Arc::from(""), issue(), true);
    let own = log.host_revision();
    let mut sent = Vec::new();
    for at in 0..VERSIONS_KEPT as u32 * 2 {
        edit(&mut log, r(at, at), "a");
        let revision = issue();
        log.sent(revision, &log.document().clone(), None);
        sent.push(revision);
        log.confirmed(revision);
        if at < 10 {
            assert_eq!(log.named(own), Named::Kept, "after {at}");
        }
    }
    assert_eq!(log.revisions().len(), VERSIONS_KEPT);
    assert_eq!(log.named(own), Named::OutOfDate);
    let oldest = sent[sent.len() - VERSIONS_KEPT];
    assert_eq!(log.named(oldest), Named::Kept);
    assert_eq!(log.kept_after(*sent.last().expect("sent")), Some(oldest));
    assert_eq!(
        log.text_at(oldest).map(|text| text.len()),
        Some(VERSIONS_KEPT + 1)
    );
}

#[test]
fn a_confirmed_revision_names_the_texts_around_it() {
    let other = issue();
    let mut log = HostLog::new(Arc::from("x"), issue(), true);
    let own = log.host_revision();
    edit(&mut log, r(1, 1), "y");
    let first = issue();
    log.sent(first, &log.document().clone(), None);
    edit(&mut log, r(2, 2), "z");
    let second = issue();
    log.sent(second, &log.document().clone(), None);
    log.confirmed(first);
    assert_eq!(log.named(own), Named::Kept);
    assert_eq!(log.text_at(own).as_deref(), Some("x"));
    assert_eq!(log.named(first), Named::Kept);
    assert_eq!(log.named(second), Named::Kept);
    assert_eq!(log.text_at(first).as_deref(), Some("xy"));
    assert_eq!(log.named(u64::MAX), Named::Unknown);
    // Issued before this log began (by the editor a hide and show
    // replaced): it names no text here, a list naming it is out of date.
    assert_eq!(log.named(other), Named::OutOfDate);
}

#[test]
fn a_text_of_the_hosts_own_starts_a_new_log() {
    let mut log = HostLog::new(Arc::from("x"), issue(), true);
    edit(&mut log, r(1, 1), "y");
    let sent = issue();
    log.sent(sent, &log.document().clone(), None);
    let own = issue();
    log.own(Arc::from("other"), own);
    assert_eq!(log.named(sent), Named::OutOfDate);
    assert_eq!(log.revisions(), vec![own]);
    assert_eq!(log.text_at(own).as_deref(), Some("other"));
}

#[test]
fn a_host_handing_nothing_back_names_a_bounded_number_of_texts() {
    let mut log = HostLog::new(Arc::from(""), issue(), false);
    for at in 0..VERSIONS_KEPT * 3 {
        let at = u32::try_from(at).expect("small");
        edit(&mut log, r(at, at), "k");
        log.sent(issue(), &log.document().clone(), None);
    }
    assert_eq!(log.revisions().len(), VERSIONS_KEPT);
    assert!(log.step_count() <= VERSIONS_KEPT);
}

#[test]
fn a_dragged_stretch_goes_where_it_was_dropped() {
    let mut log = HostLog::new(Arc::from("one two three"), issue(), true);
    let given = log.host_revision();
    // `two ` dragged to the end.
    let before = log.document().clone();
    let delete = Change::replace(&before, r(4, 8), "");
    let middle: Arc<str> = Arc::from(delete.apply(&before).expect("made for it"));
    log.edited(&delete, "deleteByDrag", &before, &middle);
    let drop = Change::replace(&middle, r(9, 9), "two ");
    let after: Arc<str> = Arc::from(drop.apply(&middle).expect("made for it"));
    log.edited(&drop, "insertFromDrop", &middle, &after);
    assert_eq!(&*after, "one threetwo ");
    assert_eq!(
        log.map(given, &[r(4, 7)], &no_rewrite),
        Some(vec![r(9, 12)])
    );
}

#[test]
fn a_text_no_edit_made_is_followed_by_the_rewrite() {
    let mut log = HostLog::new(Arc::from("a"), issue(), true);
    let given = log.host_revision();
    log.moved_to(&Arc::from("b: a"));
    let rewrite = |from: &str, to: &str, ranges: &[TextRange]| {
        assert_eq!((from, to), ("a", "b: a"));
        ranges
            .iter()
            .map(|range| *range + TextSize::from(3))
            .collect()
    };
    assert_eq!(log.map(given, &[r(0, 1)], &rewrite), Some(vec![r(3, 4)]));
    assert_eq!(log.text_at(given).as_deref(), Some("a"));
}

/// A small seeded generator (xorshift).
struct Rng(u64);

impl Rng {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % bound.max(1) as u64).unwrap_or(0)
    }
}

fn seed() -> u64 {
    std::env::var("HOST_LOG_SEED")
        .ok()
        .and_then(|seed| seed.parse().ok())
        .unwrap_or(0x51d4_15bd_9691_2f2f)
}

/// Random cases per property: `HOST_LOG_CASES`, else `default` (the sweep
/// raises it).
fn cases(default: usize) -> usize {
    std::env::var("HOST_LOG_CASES")
        .ok()
        .and_then(|cases| cases.parse().ok())
        .unwrap_or(default)
}

const PIECES: [&str; 7] = ["a", "b", "\n", " ", "é", "😀", ""];

/// A text in the model: each character with the id it got when it was
/// written.
type Text = Vec<(char, usize)>;

fn string(text: &[(char, usize)]) -> String {
    text.iter().map(|(c, _)| *c).collect()
}

/// The byte offset of character `at` of `text`.
fn byte(text: &[(char, usize)], chars: usize) -> TextSize {
    TextSize::of(string(&text[..chars]).as_str())
}

/// The model of one session: the document, the versions kept and their
/// texts, and the next character id.
struct Model {
    document: Text,
    versions: Vec<(u64, Text)>,
    next: usize,
}

impl Model {
    fn fresh(&mut self, piece: &str) -> Text {
        piece
            .chars()
            .map(|c| {
                self.next += 1;
                (c, self.next)
            })
            .collect()
    }
}

/// One random step of a session, made on `log` and `model` alike.
fn step(rng: &mut Rng, log: &mut HostLog, model: &mut Model) {
    match rng.below(20) {
        0..=11 => {
            let len = model.document.len();
            let start = rng.below(len + 1);
            let end = (start + rng.below(4)).min(len);
            let inserted = model.fresh(PIECES[rng.below(PIECES.len())]);
            let range = TextRange::new(byte(&model.document, start), byte(&model.document, end));
            let insert = string(&inserted);
            model.document.splice(start..end, inserted);
            edit(log, range, &insert);
        }
        12..=15 => {
            let revision = issue();
            log.sent(revision, &string(&model.document), None);
            model.versions.push((revision, model.document.clone()));
        }
        16 => {
            // The host's text now sent again, the edits since held.
            let mut newest = model.versions.last().expect("never empty").1.clone();
            if string(&newest) == string(&model.document) {
                // The same text as the document: a send of it.
                newest = model.document.clone();
            }
            let revision = issue();
            log.sent(revision, &string(&newest), None);
            model.versions.push((revision, newest));
        }
        17..=18 => {
            let at = rng.below(model.versions.len());
            let revision = model.versions[at].0;
            log.confirmed(revision);
            while model.versions.len() > VERSIONS_KEPT
                && model.versions[0].0 < revision
                && model.versions[1].0 <= revision
            {
                model.versions.remove(0);
            }
        }
        _ => {
            let piece: String = (0..rng.below(6))
                .map(|_| PIECES[rng.below(PIECES.len())])
                .collect();
            let text = model.fresh(&piece);
            let revision = issue();
            log.own(Arc::from(piece.as_str()), revision);
            model.document = text.clone();
            model.versions = vec![(revision, text)];
        }
    }
}

/// `count` sessions of random steps from a random text of the host's own.
fn sessions(count: usize, salt: u64, mut check: impl FnMut(&mut Rng, &mut HostLog, &Model)) {
    let mut rng = Rng(seed() ^ salt);
    for _ in 0..count {
        let mut model = Model {
            document: Vec::new(),
            versions: Vec::new(),
            next: 0,
        };
        let piece: String = (0..rng.below(8))
            .map(|_| PIECES[rng.below(PIECES.len())])
            .collect();
        model.document = model.fresh(&piece);
        let revision = issue();
        model.versions.push((revision, model.document.clone()));
        let mut log = HostLog::new(Arc::from(piece.as_str()), revision, true);
        // Past `VERSIONS_KEPT` versions at times, for the window.
        for _ in 0..rng.below(160) {
            step(&mut rng, &mut log, &mut model);
            assert_eq!(*string(&model.document), **log.document());
            assert_eq!(
                log.revisions(),
                model.versions.iter().map(|(at, _)| *at).collect::<Vec<_>>()
            );
            check(&mut rng, &mut log, &model);
        }
    }
}

#[test]
fn the_host_log_gives_back_every_text_it_keeps() {
    sessions(cases(300), 1, |_, log, model| {
        for (revision, text) in &model.versions {
            assert_eq!(
                log.text_at(*revision).as_deref(),
                Some(string(text).as_str()),
                "version {revision}"
            );
        }
    });
}

#[test]
fn the_host_log_maps_any_kept_texts_positions_onto_the_same_characters() {
    sessions(cases(300), 2, |rng, log, model| {
        let (revision, text) = &model.versions[rng.below(model.versions.len())];
        let start = rng.below(text.len() + 1);
        let end = (start + rng.below(5)).min(text.len());
        let range = TextRange::new(byte(text, start), byte(text, end));
        let mapped = log
            .map(*revision, &[range], &no_rewrite)
            .expect("a kept version maps")[0];
        let ids: Vec<usize> = text[start..end].iter().map(|(_, id)| *id).collect();
        let at: Vec<usize> = model
            .document
            .iter()
            .enumerate()
            .filter(|(_, (_, id))| ids.contains(id))
            .map(|(at, _)| at)
            .collect();
        let (Some(&first), Some(&last)) = (at.first(), at.last()) else {
            return;
        };
        let (low, high) = (
            byte(&model.document, first),
            byte(&model.document, last + 1),
        );
        // Every character of the range still in the document is covered:
        // a lock never lets one of its characters go.
        assert!(
            mapped.start() <= low && high <= mapped.end(),
            "{range:?} of {:?} mapped to {mapped:?} in {:?}",
            string(text),
            string(&model.document)
        );
        // Kept whole, with nothing written inside, it covers them alone.
        let whole = at.len() == ids.len() && last + 1 - first == ids.len();
        if whole {
            assert_eq!(
                mapped,
                TextRange::new(low, high),
                "{range:?} of {:?} in {:?}",
                string(text),
                string(&model.document)
            );
        }
    });
}
