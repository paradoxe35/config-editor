//! Echoes of the editor's own edits, for a host that hands every `onChange`
//! text back (the `echoes: true` option): it stores each text and passes it
//! in again through `update` (`{ value }`, or the whole `documents` list),
//! possibly late and in any order (a fetch per edit). Applying such an echo
//! would roll the document back and lose what was typed since.
//!
//! Without the option every text the host gives applies (it is the host's
//! own: a loaded file, a discard, a reset), and nothing is recorded.
//!
//! With it, a text the editor sent that has not come back yet is an echo,
//! whatever its order or delay: it is skipped (it is acknowledged, and
//! forgotten). A text the editor never sent, or sent and saw come back, is
//! the host's own and applies. When the host's own text replaces the
//! document (another file, a discard), the texts sent before it belong to
//! the text it replaced: they are no echoes any more, so going back to one
//! of them (back to the first file) applies; but for an edit the editor
//! still held, sent while that update is handled, which the host did not
//! know. To go back to a text still on its way, or to reset while the
//! echoes of what was typed are still to come (a host handing every one of
//! them back late), the host calls `replaceValue`: the echoes on their way
//! are still told apart. A skipped echo leaves the host
//! holding an old text: unless an echo of the document is still to come
//! (and set it right), the host is told the document again (`onChange`). A
//! text sent before one that came back may still come (out of order) or
//! never (a host handing back only the latest of several): it stays an
//! echo, but is not counted on to come. An update's `value` is always the
//! host's word, the text it gave last included (a reset to the
//! constructor's text); a document's text listed again, as it was listed
//! last (the whole `documents` list given for another document's edit), is
//! no news, as a framework's binding passes an unchanged prop on.
//!
//! Texts are told apart as the editor reads them (see `Print`): an echo a
//! store wrote another way (the byte order mark dropped, LF for CRLF) is
//! still the echo, and one that is the document written another way keeps
//! the document's own form.
//!
//! Ranges given with a skipped echo (`readOnlyRanges`, host diagnostics)
//! are in the echoed text: they are mapped to the document (see
//! `crate::remap`).
//!
//! An echo still to come may never be seen, though: a framework renders once
//! for several changes (React, Vue and Svelte batch them), so a host storing
//! the document's echo and then a late echo of an older text in one render
//! gives only the older one. So whenever an echo leaves the host on an older
//! text, the editor checks again once a pause ([`SETTLE_MS`]) is over, and a
//! host still on an older text then hears the document again: once all the
//! echoes are in, the host holds the document. A copy of the text the host
//! holds, given again while it is on its way, may be its echo (with a later
//! text lost before it): it counts as news once.
//!
//! The framework bindings' `ValueSync` (`js/config-editor/sync.js`) follows
//! the same rule.

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap, VecDeque};

/// How long after a text came back the copies sent before it are
/// forgotten (see [`Echoes`]).
pub(crate) const FORGET_MS: f64 = 5000.0;

/// How long after an echo that left the host on an older text the editor
/// checks the host came back to the document (see [`Echoes::settle`]).
pub(crate) const SETTLE_MS: i32 = 250;

thread_local! {
    static CLOCK: Cell<u64> = const { Cell::new(0) };
}

/// How many texts every editor on the page has sent: a text sent from now
/// on is sent at this or later (an update reads it before it sends an edit
/// still held, to tell those texts apart).
pub(crate) fn clock() -> u64 {
    CLOCK.with(Cell::get)
}

fn tick() -> u64 {
    CLOCK.with(|clock| {
        let now = clock.get();
        clock.set(now + 1);
        now
    })
}

/// A short stand-in for a text, as the editor reads it (no byte order
/// mark, every line break one line feed: a host store that drops the mark
/// or writes LF hands back the same text): its length and a 64-bit FNV-1a
/// hash, both of that reading. Two prints are equal by those alone; the
/// length of the text as given (`raw`) only tells whether a print kept for
/// the document may still be its own (see `document_print`).
#[derive(Clone, Copy, Debug)]
struct Print(usize, u64, usize);

impl PartialEq for Print {
    fn eq(&self, other: &Self) -> bool {
        (self.0, self.1) == (other.0, other.1)
    }
}

impl Eq for Print {}

impl std::hash::Hash for Print {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self.0, self.1).hash(state);
    }
}

impl Print {
    fn of(text: &str) -> Self {
        work::note(work::Work::TextRead);
        let bytes = text.strip_prefix('\u{feff}').unwrap_or(text).as_bytes();
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut length = 0;
        let mut at = 0;
        while at < bytes.len() {
            let mut byte = bytes[at];
            if byte == b'\r' {
                // CRLF and a lone CR read as one line feed.
                byte = b'\n';
                if bytes.get(at + 1) == Some(&b'\n') {
                    at += 1;
                }
            }
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
            length += 1;
            at += 1;
        }
        Self(length, hash, text.len())
    }
}

/// A copy of a text sent and not handed back yet.
#[derive(Clone, Copy, Debug)]
struct Sent {
    /// When it was sent (see [`clock`]).
    serial: u64,
    /// Not counted on to come, whatever came back since: the text the
    /// host held was given again, which may have been its echo.
    counted_out: bool,
    /// The text the host held already was given again while this copy was
    /// on its way: that may have been its echo (see `take`).
    doubted: bool,
}

/// What to do with a text the host handed back.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Echo {
    /// The host's own text, or the document already: show it (nothing to
    /// do when it is the document).
    Apply,
    /// An echo of a text the editor moved past: keep the document.
    Skip,
    /// The same, an echo of the document still to come: keep it, and
    /// [`Echoes::settle`] after [`SETTLE_MS`] (the echo may have been lost
    /// in a host's render that took a later text too).
    Settle,
    /// The same, and nothing still on its way brings the host back to the
    /// document: keep it and send it to the host again (`onChange`; already
    /// recorded as sent), then settle as for [`Echo::Settle`] (the text
    /// sent again may be lost too).
    Resend,
}

/// The texts one document sent that the host has not handed back yet.
///
/// Every one is kept, however many wait (a busy page, a slow host):
/// forgetting one would take its echo for the host's own text and roll the
/// document back. A copy is forgotten when its echo comes, when the host's
/// own text replaces the document (the copies sent before belong to the
/// text it replaced), or [`FORGET_MS`] after a text sent later came back:
/// a host handing back only the latest of several never hands back the
/// others, and one handing them back out of order does so long before
/// (its delays differ by less). So what is kept is bounded by what is sent
/// in that time. An echo costs one read of the text handed back
/// (for its print) and a lookup by it: never a pass over the copies
/// waiting, nor another over the document.
#[derive(Default, Debug)]
pub(crate) struct Echoes {
    /// Whether the host hands every text back (`echoes: true`); without it
    /// nothing is recorded and every text applies.
    on: bool,
    /// The copies sent and not handed back yet, by when they were sent.
    order: BTreeMap<u64, Print>,
    /// The same, by text: each text's copies, oldest first.
    copies: HashMap<Print, VecDeque<Sent>>,
    /// When the newest of the texts that came back was sent: a copy sent
    /// before it may still come (out of order) or never (a host handing
    /// back only the latest of several), so it is not counted on to come.
    came_back: Option<u64>,
    /// When (ms, see [`Echoes::at`]) each text that came back, newer than
    /// any before it, came, and when it was sent: oldest first.
    returns: VecDeque<(f64, u64)>,
    /// The time now (see [`Echoes::at`]).
    now: f64,
    /// The text the host gave last.
    last_given: Option<Print>,
    /// The text the host gave last in a list or an update, which
    /// `replaceValue` leaves as it was.
    last_listed: Option<Print>,
    /// The document's print, while known (the text sent, given or shown
    /// last).
    document: Option<Print>,
}

impl Echoes {
    /// For a host that hands every text back (`on`), or not.
    pub(crate) fn new(on: bool) -> Self {
        Self {
            on,
            ..Self::default()
        }
    }

    /// The time is `now` (ms, any clock that never goes back): the copies
    /// sent before a text that came back [`FORGET_MS`] ago or more are
    /// forgotten.
    pub(crate) fn at(&mut self, now: f64) {
        self.now = now;
        while let Some(&(came, serial)) = self.returns.front() {
            if came + FORGET_MS > now {
                break;
            }
            self.returns.pop_front();
            self.forget_before(serial);
        }
    }

    /// The host gave `text` as the document is built (the constructor's
    /// `value`, a document first listed).
    pub(crate) fn given(&mut self, text: &str) {
        if self.on {
            let print = Print::of(text);
            self.last_listed = Some(print);
            self.last_given = Some(print);
            self.document = Some(print);
        }
    }

    /// The host gave `text` of its own as the document (`replaceValue`).
    /// Its list is no news until it changes: a list the host gives again as
    /// it gave it last, the text it replaced in it (a framework's render
    /// before the host took the new text), keeps the document (QA 30,
    /// F30-2: React's `setValue` was rolled back by the render it caused).
    pub(crate) fn replaced(&mut self, text: &str) {
        if self.on {
            let print = Print::of(text);
            self.last_given = Some(print);
            self.document = Some(print);
        }
    }

    /// The editor sent `text`, the document, to the host (its `onChange`).
    pub(crate) fn sent(&mut self, text: &str) {
        if self.on {
            self.send(Print::of(text));
        }
    }

    /// The document changed and the host was not told (no `onChange`).
    pub(crate) fn unsent(&mut self) {
        self.document = None;
    }

    fn send(&mut self, print: Print) {
        let serial = tick();
        self.order.insert(serial, print);
        self.copies.entry(print).or_default().push_back(Sent {
            serial,
            counted_out: false,
            doubted: false,
        });
        self.document = Some(print);
    }

    /// The document `current`'s print: the one known (the text sent,
    /// given or shown last) when it is that long, else read.
    fn document_print(&mut self, current: &str) -> Print {
        match self.document {
            Some(print) if print.2 == current.len() => print,
            _ => {
                let print = Print::of(current);
                self.document = Some(print);
                print
            }
        }
    }

    /// Whether `value` (printed `print`) is the document `current` as the
    /// editor reads it (see [`Print`]); read again only when one of them
    /// has a byte order mark or a carriage return.
    fn same(&mut self, value: &str, print: Print, current: &str) -> bool {
        if value == current {
            return true;
        }
        let plain = |text: &str| !text.starts_with('\u{feff}') && !text.contains('\r');
        if plain(value) && plain(current) {
            return false;
        }
        print == self.document_print(current)
    }

    /// Whether a copy of `print` is counted on to come: no text sent after
    /// it has come back, and it is not counted out.
    fn to_come(&self, print: Print) -> bool {
        let came_back = self.came_back;
        self.copies.get(&print).is_some_and(|copies| {
            copies
                .iter()
                .rev()
                .inspect(|_| work::note(work::Work::CopyVisited))
                .take_while(|sent| came_back.is_none_or(|back| sent.serial > back))
                .any(|sent| !sent.counted_out)
        })
    }

    /// Forgets the copies sent before `serial` (the host's own text
    /// replaced the text they belong to): each one once.
    fn forget_before(&mut self, serial: u64) {
        while let Some(entry) = self.order.first_entry() {
            if *entry.key() >= serial {
                break;
            }
            let print = entry.remove();
            // Forgotten oldest first: the text's oldest copy.
            if let Some(copies) = self.copies.get_mut(&print) {
                copies.pop_front();
                if copies.is_empty() {
                    self.copies.remove(&print);
                }
            }
        }
    }

    /// The host handed back `print`'s newest copy.
    fn came(&mut self, print: Print) {
        let Some(copies) = self.copies.get_mut(&print) else {
            return;
        };
        let Some(sent) = copies.pop_back() else {
            return;
        };
        if copies.is_empty() {
            self.copies.remove(&print);
        }
        self.order.remove(&sent.serial);
        if self.came_back.is_none_or(|back| sent.serial > back) {
            self.came_back = Some(sent.serial);
            self.returns.push_back((self.now, sent.serial));
        }
    }

    /// The host gave `value` in an update (`update({ value })`), the
    /// document being `current`; the texts sent from `flushed` on (see
    /// [`clock`]) were sent while the update was handled (an edit still
    /// held, sent first). A text the editor never sent is news, whatever
    /// the host gave before (the constructor's text again: a discard).
    pub(crate) fn received(&mut self, value: &str, current: &str, flushed: u64) -> Echo {
        self.take(value, current, false, flushed)
    }

    /// The host listed `value` for the document (the whole `documents`
    /// list), as [`Echoes::received`], but the text it listed last, listed
    /// again, is no news (the list given for another document's edit), as
    /// a framework's binding passes an unchanged prop on.
    pub(crate) fn listed(&mut self, value: &str, current: &str, flushed: u64) -> Echo {
        self.take(value, current, true, flushed)
    }

    fn take(&mut self, value: &str, current: &str, listed: bool, flushed: u64) -> Echo {
        if !self.on {
            return Echo::Apply;
        }
        let print = Print::of(value);
        self.last_given = Some(print);
        let again = self.last_listed.replace(print) == Some(print);
        let same = self.same(value, print, current);
        let news = if listed && again {
            // Listed again: a render, or the echo of a copy sent since (and
            // a text sent after it may have come and gone in the same
            // render of the host's). That copy stays an echo, but is not
            // counted on to come; while the host is on an older text, it is
            // taken for news once.
            let copy = self.copies.get_mut(&print).and_then(|copies| {
                copies
                    .iter_mut()
                    .rev()
                    .inspect(|_| work::note(work::Work::CopyVisited))
                    .find(|sent| !sent.doubted)
            });
            match copy {
                Some(copy) => {
                    copy.counted_out = true;
                    copy.doubted = !same;
                    copy.doubted
                }
                None => false,
            }
        } else if self.copies.contains_key(&print) {
            // The newest copy (a typo fixed sends the same text twice): a
            // host handing back only the latest text hands back that one.
            self.came(print);
            true
        } else {
            // The host's own text. When it replaces the document, the
            // texts sent before belong to the text it replaced (another
            // file, a discarded draft): no echoes any more, so going back
            // to one of them applies. Those sent while it was handled (an
            // edit still held, sent first) were unknown to the host: still
            // echoes.
            if !same {
                self.forget_before(flushed);
                self.document = Some(print);
            }
            return Echo::Apply;
        };
        if same {
            self.document = Some(print);
            // The document written another way (the host's store dropped
            // the byte order mark or wrote LF): it keeps its own.
            return if value == current {
                Echo::Apply
            } else {
                Echo::Skip
            };
        }
        let document = self.document_print(current);
        if self.to_come(document) {
            return if news { Echo::Settle } else { Echo::Skip };
        }
        self.send(document);
        Echo::Resend
    }

    /// The pause after a [`Echo::Settle`] or [`Echo::Resend`] is over:
    /// whether the host, still on an older text, is to hear the document
    /// `current` again (already recorded as sent). The echo it waited for
    /// may still be on its way (a slow host: the host hears the same text
    /// twice), or gone (a host taking several texts in one render keeps
    /// only the last).
    pub(crate) fn settle(&mut self, current: &str) -> bool {
        let Some(given) = self.last_given.filter(|_| self.on) else {
            return false;
        };
        let document = self.document_print(current);
        if given == document {
            return false;
        }
        self.send(document);
        true
    }
}

/// The work an echo costs, counted in the tests (nothing in a build
/// without them).
mod work {
    #[derive(Clone, Copy)]
    pub(super) enum Work {
        /// A whole text read for its print.
        TextRead,
        /// A copy waiting for its echo looked at.
        CopyVisited,
    }

    #[cfg(test)]
    thread_local! {
        static COUNTS: std::cell::Cell<[usize; 2]> = const { std::cell::Cell::new([0; 2]) };
    }

    #[inline(always)]
    pub(super) fn note(work: Work) {
        #[cfg(test)]
        COUNTS.with(|counts| {
            let mut now = counts.get();
            now[work as usize] += 1;
            counts.set(now);
        });
        #[cfg(not(test))]
        let _ = work;
    }

    /// Runs `run` and returns its result and how many times it did `work`.
    #[cfg(test)]
    pub(super) fn counted<T>(work: Work, run: impl FnOnce() -> T) -> (T, usize) {
        let before = COUNTS.with(std::cell::Cell::get)[work as usize];
        let result = run();
        (
            result,
            COUNTS.with(std::cell::Cell::get)[work as usize] - before,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::work::{Work, counted};
    use super::{Echo, Echoes, SETTLE_MS};

    /// No text sent while the update is handled.
    const NONE: u64 = u64::MAX;

    /// How many copies wait for their echo (each kept once, by when it
    /// was sent and by its text).
    fn waiting(echoes: &Echoes) -> usize {
        let by_text: usize = echoes
            .copies
            .values()
            .map(std::collections::VecDeque::len)
            .sum();
        assert_eq!(echoes.order.len(), by_text);
        assert!(echoes.copies.values().all(|copies| !copies.is_empty()));
        by_text
    }

    /// Without the option every text applies and nothing is kept.
    #[test]
    fn without_the_option_every_text_applies() {
        let mut echoes = Echoes::new(false);
        echoes.sent("a");
        echoes.sent("ab");
        assert_eq!(echoes.received("a", "ab", NONE), Echo::Apply);
        assert_eq!(waiting(&echoes), 0);
    }

    /// Typing `abc` with every echo one key late: each one but the newest is
    /// stale and skipped (the newest still to come), the newest applies (the
    /// document already).
    #[test]
    fn a_late_echo_is_skipped_and_the_newest_applies() {
        let mut echoes = Echoes::new(true);
        echoes.sent("a");
        echoes.sent("ab");
        assert_eq!(echoes.received("a", "ab", NONE), Echo::Settle);
        echoes.sent("abc");
        assert_eq!(echoes.received("ab", "abc", NONE), Echo::Settle);
        assert_eq!(echoes.received("abc", "abc", NONE), Echo::Apply);
        assert_eq!(waiting(&echoes), 0);
    }

    /// Echoes in any order are skipped; the host, left on an old one with
    /// nothing else to come, hears the document again.
    #[test]
    fn echoes_in_any_order_are_skipped() {
        let mut echoes = Echoes::new(true);
        for text in ["a", "ab", "abc"] {
            echoes.sent(text);
        }
        assert_eq!(echoes.received("abc", "abc", NONE), Echo::Apply);
        assert_eq!(echoes.received("a", "abc", NONE), Echo::Resend);
        // The resent document is on its way: the host comes back to it.
        assert_eq!(echoes.received("ab", "abc", NONE), Echo::Settle);
        assert_eq!(echoes.received("abc", "abc", NONE), Echo::Apply);
        assert_eq!(waiting(&echoes), 0);
    }

    /// Every order of five echoes keeps the document, and the last text the
    /// host is given (an echo or a text sent again) is the document.
    #[test]
    fn every_order_of_echoes_ends_on_the_document() {
        let texts = ["a", "ab", "abc", "abcd", "abcde"];
        let mut orders = vec![vec![]];
        for _ in 0..texts.len() {
            orders = orders
                .into_iter()
                .flat_map(|order: Vec<usize>| {
                    (0..texts.len())
                        .filter(|index| !order.contains(index))
                        .map(|index| [order.clone(), vec![index]].concat())
                        .collect::<Vec<_>>()
                })
                .collect();
        }
        for order in orders {
            let mut echoes = Echoes::new(true);
            for text in texts {
                echoes.sent(text);
            }
            let current = "abcde";
            let mut host = String::new();
            let mut resent = 0;
            for index in &order {
                host = texts[*index].to_string();
                match echoes.received(&host, current, NONE) {
                    Echo::Apply => assert_eq!(host, current, "{order:?}"),
                    Echo::Skip | Echo::Settle => {}
                    Echo::Resend => resent += 1,
                }
            }
            // The resent texts come back, in order: the host ends on them.
            for _ in 0..resent {
                host = current.to_string();
                assert_eq!(echoes.received(current, current, NONE), Echo::Apply);
            }
            assert_eq!(host, current, "{order:?}");
            assert_eq!(waiting(&echoes), 0, "{order:?}");
        }
    }

    /// A host whose store writes each text another way (the byte order
    /// mark dropped, LF for CRLF, both) hands back echoes all the same: in
    /// any order, none rolls a BOM and CRLF document back, and the document
    /// keeps its own form (QA 32: they were taken for the host's own texts
    /// and applied, losing what was typed since).
    #[test]
    fn echoes_written_another_way_are_still_echoes() {
        type Store = fn(&str) -> String;
        let stores: [(&str, Store); 3] = [
            ("no BOM", |text| {
                text.trim_start_matches('\u{feff}').to_string()
            }),
            ("LF", |text| text.replace("\r\n", "\n")),
            ("both", |text| {
                text.trim_start_matches('\u{feff}').replace("\r\n", "\n")
            }),
        ];
        let texts = [
            "\u{feff}a: 12\r\nb: 2\r\n",
            "\u{feff}a: 123\r\nb: 2\r\n",
            "\u{feff}a: 1234\r\nb: 2\r\n",
            "\u{feff}a: 12345\r\nb: 2\r\n",
        ];
        let current = texts[3];
        let orders = [
            [0, 1, 2, 3],
            [3, 2, 1, 0],
            [1, 0, 3, 2],
            [2, 3, 0, 1],
            [0, 3, 1, 2],
        ];
        for (name, store) in stores {
            for order in orders {
                let mut echoes = Echoes::new(true);
                for text in texts {
                    echoes.sent(text);
                }
                for index in order {
                    let given = store(texts[index]);
                    let echo = echoes.received(&given, current, NONE);
                    assert_ne!(echo, Echo::Apply, "{name} {order:?}: {given:?} applied");
                }
            }
        }
    }

    /// An echo written another way costs at most two reads of a text (its
    /// own, and the document's when it has a byte order mark or a carriage
    /// return), whatever the size of the document: the work per echo stays
    /// the same at 100 and at 10,000 lines.
    #[test]
    fn an_echo_written_another_way_costs_two_reads_at_any_size() {
        for lines in [100, 10_000] {
            let body: String = (0..lines).map(|line| format!("k{line}: v\r\n")).collect();
            let texts: Vec<String> = (0..8)
                .map(|typed| format!("\u{feff}a: {typed}\r\n{body}"))
                .collect();
            let current = texts.last().expect("a text").clone();
            let mut echoes = Echoes::new(true);
            for text in &texts {
                echoes.sent(text);
            }
            let stored: Vec<String> = texts
                .iter()
                .rev()
                .map(|text| text.trim_start_matches('\u{feff}').replace("\r\n", "\n"))
                .collect();
            let (_, reads) = counted(Work::TextRead, || {
                for text in &stored {
                    assert_ne!(echoes.received(text, &current, NONE), Echo::Apply);
                }
            });
            assert!(
                reads <= 2 * stored.len(),
                "{lines} lines: {reads} texts read for {} echoes",
                stored.len()
            );
        }
    }

    /// A text the editor never sent is the host's own, even with echoes on
    /// their way: it replaces the document, and the texts sent before it
    /// are the host's own from then on too (they belong to the text it
    /// replaced). Echoes of what is typed after it are told apart again.
    #[test]
    fn the_hosts_own_text_wins_over_echoes() {
        let mut echoes = Echoes::new(true);
        echoes.sent("a");
        echoes.sent("ab");
        assert_eq!(echoes.received("reset", "ab", NONE), Echo::Apply);
        assert_eq!(echoes.received("a", "reset", NONE), Echo::Apply);
        echoes.sent("ac");
        echoes.sent("acd");
        assert_eq!(echoes.received("ac", "acd", NONE), Echo::Settle);
        assert_eq!(echoes.received("acd", "acd", NONE), Echo::Apply);
    }

    /// A reset through `replaceValue` (`given`) keeps telling the echoes on
    /// their way apart: they are skipped when they come.
    #[test]
    fn echoes_on_their_way_stay_echoes_after_replace_value() {
        let mut echoes = Echoes::new(true);
        echoes.sent("a");
        echoes.sent("ab");
        echoes.given("reset");
        assert_eq!(echoes.received("a", "reset", NONE), Echo::Resend);
        assert_eq!(echoes.received("ab", "reset", NONE), Echo::Settle);
        assert_eq!(echoes.received("reset", "reset", NONE), Echo::Apply);
    }

    /// The same text sent twice (a typo typed and undone) comes back twice:
    /// the host ends on the document whichever copy each echo is taken for.
    #[test]
    fn a_text_sent_twice_comes_back_twice() {
        let mut echoes = Echoes::new(true);
        echoes.sent("a");
        echoes.sent("ax");
        echoes.sent("a");
        assert_eq!(echoes.received("a", "a", NONE), Echo::Apply);
        // Taken for the newest copy, the first one is not counted on.
        assert_eq!(echoes.received("ax", "a", NONE), Echo::Resend);
        assert_eq!(echoes.received("a", "a", NONE), Echo::Apply);
        // Given again (a render): no news.
        assert_eq!(echoes.received("a", "a", NONE), Echo::Apply);
    }

    /// A host handing back only the latest of several texts: those before it
    /// stay echoes, but are not counted on to come, so a host setting one
    /// (taken for its echo) hears the document again.
    #[test]
    fn a_host_handing_back_only_the_latest_hears_the_document_again() {
        let mut echoes = Echoes::new(true);
        echoes.sent("ax");
        echoes.sent("a");
        echoes.sent("ax");
        // Only the latest comes back; then the host sets the text between.
        assert_eq!(echoes.received("ax", "ax", NONE), Echo::Apply);
        assert_eq!(echoes.received("a", "ax", NONE), Echo::Resend);
        assert_eq!(echoes.received("ax", "ax", NONE), Echo::Apply);
        // Typed abc, the host coalesces to abc, then sets a: the document
        // stays and the host is told it again.
        for text in ["a", "ab", "abc"] {
            echoes.sent(text);
        }
        assert_eq!(echoes.received("abc", "abc", NONE), Echo::Apply);
        assert_eq!(echoes.received("a", "abc", NONE), Echo::Resend);
    }

    /// Typing with backspaces (texts sent twice) against a host that hands
    /// every text back after a random delay, or only the latest of each
    /// burst: no echo ever changes the document, and the host ends on it.
    #[test]
    fn random_echo_orders_never_roll_back_and_the_host_ends_on_the_document() {
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        let mut next = move |bound: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % bound
        };
        for round in 0..500 {
            let coalescing = round % 2 == 1;
            let mut echoes = Echoes::new(true);
            echoes.given("");
            let mut document = String::new();
            // (arrival time, text) on their way to the host, and back.
            let mut in_flight: Vec<(u64, String)> = Vec::new();
            let mut host = String::new();
            for time in 0..400u64 {
                if time < 40 {
                    if next(3) == 0 {
                        document.pop();
                    } else {
                        document.push(char::from(b'a' + next(3) as u8));
                    }
                    echoes.sent(&document);
                    if coalescing {
                        in_flight.clear();
                    }
                    in_flight.push((time + next(30), document.clone()));
                }
                let (arrived, waiting): (Vec<_>, Vec<_>) =
                    in_flight.drain(..).partition(|(at, _)| *at <= time);
                in_flight = waiting;
                for (_, text) in arrived {
                    host = text;
                    match echoes.received(&host, &document, NONE) {
                        Echo::Apply => assert_eq!(host, document, "round {round}"),
                        Echo::Skip | Echo::Settle => {}
                        Echo::Resend => in_flight.push((time + next(30), document.clone())),
                    }
                }
                // Now and then the host renders again with what it holds.
                if next(4) == 0 {
                    match echoes.listed(&host, &document, NONE) {
                        Echo::Apply => assert_eq!(host, document, "round {round}"),
                        Echo::Skip | Echo::Settle => {}
                        Echo::Resend => in_flight.push((time + next(30), document.clone())),
                    }
                }
            }
            assert!(in_flight.is_empty(), "round {round}");
            assert_eq!(host, document, "round {round}");
        }
    }

    /// The host listing its last text again (the whole list, for another
    /// document's edit), its echoes still on their way, keeps the document;
    /// a text of its own still applies.
    #[test]
    fn the_last_text_listed_again_is_no_news() {
        let mut echoes = Echoes::new(true);
        echoes.given("a");
        echoes.sent("ab");
        assert_eq!(echoes.listed("a", "ab", NONE), Echo::Skip);
        assert_eq!(echoes.listed("ab", "ab", NONE), Echo::Apply);
        echoes.sent("abc");
        // A re-render with what it holds, before the echo of `abc` comes.
        assert_eq!(echoes.listed("ab", "abc", NONE), Echo::Skip);
        assert_eq!(echoes.listed("abc", "abc", NONE), Echo::Apply);
        assert_eq!(echoes.listed("reset", "abc", NONE), Echo::Apply);
        assert_eq!(echoes.listed("reset", "reset", NONE), Echo::Apply);
    }

    /// QA 30 (F30-2): after `replaceValue`, a list as the host gave it
    /// last (React's render right after `setValue`, before the host took
    /// the new text) is no news, nor are the echoes of what was typed
    /// before, in any order: the document stays the replaced text.
    #[test]
    fn a_list_as_before_a_replace_value_keeps_the_replaced_text() {
        let typed = ["listen: 78351", "listen: 783512", "listen: 7835123"];
        for reversed in [false, true] {
            let mut echoes = Echoes::new(true);
            echoes.given("listen: 7835");
            for text in typed {
                echoes.sent(text);
            }
            echoes.replaced("listen: 1");
            let document = "listen: 1";
            assert_ne!(
                echoes.listed("listen: 7835", document, NONE),
                Echo::Apply,
                "reversed {reversed}"
            );
            let mut back: Vec<&str> = typed.to_vec();
            back.push(document);
            if reversed {
                back.reverse();
            }
            for text in back {
                let echo = echoes.listed(text, document, NONE);
                assert!(
                    text == document || echo != Echo::Apply,
                    "{text} applied (reversed {reversed})"
                );
            }
        }
    }

    /// An update giving the text the host gave last (the constructor's: a
    /// discard, a reset) while echoes are on their way is the host's own:
    /// it applies, and the echoes of the texts it replaced apply too when
    /// they come (QA 29: the reset was ignored).
    #[test]
    fn the_last_text_given_again_in_an_update_applies() {
        let mut echoes = Echoes::new(true);
        echoes.given("a");
        echoes.sent("ab");
        echoes.sent("abc");
        assert_eq!(echoes.received("a", "abc", NONE), Echo::Apply);
        assert_eq!(waiting(&echoes), 0);
        echoes.sent("aQ");
        assert_eq!(echoes.received("a", "aQ", NONE), Echo::Apply);
        // A text that came back, given again: applies too.
        echoes.sent("aR");
        assert_eq!(echoes.received("aR", "aR", NONE), Echo::Apply);
        echoes.sent("aRS");
        assert_eq!(echoes.received("aR", "aRS", NONE), Echo::Apply);
    }

    /// An edit still held when the host's text came, sent while that
    /// update is handled (`flushed`), was unknown to the host: its echo is
    /// skipped when it comes, even though it is the text replaced.
    #[test]
    fn a_text_sent_while_the_hosts_text_is_handled_stays_an_echo() {
        let mut echoes = Echoes::new(true);
        echoes.given("listen: 7835");
        let flushed = super::clock();
        echoes.sent("listen: 78359");
        assert_eq!(
            echoes.received("listen: 9000", "listen: 78359", flushed),
            Echo::Apply
        );
        assert_eq!(
            echoes.received("listen: 78359", "listen: 9000", NONE),
            Echo::Resend
        );
        assert_eq!(
            echoes.received("listen: 9000", "listen: 9000", NONE),
            Echo::Apply
        );
    }

    /// A host switching files over one editor (QA 29): a text sent for file
    /// A still on its way when file B replaces the document belongs to A;
    /// switching back to A applies it, whichever way the host gives it,
    /// and nothing tells the host B's text again.
    #[test]
    fn a_text_sent_before_the_hosts_own_one_is_no_echo_any_more() {
        for listed in [false, true] {
            let mut echoes = Echoes::new(true);
            let give = |echoes: &mut Echoes, value: &str, current: &str| {
                if listed {
                    echoes.listed(value, current, NONE)
                } else {
                    echoes.received(value, current, NONE)
                }
            };
            echoes.given("a: 1");
            echoes.sent("a: 19");
            assert_eq!(give(&mut echoes, "b: 2", "a: 19"), Echo::Apply);
            assert_eq!(give(&mut echoes, "a: 19", "b: 2"), Echo::Apply);
            assert!(!echoes.settle("a: 19"), "listed {listed}");
            // On B again, typed, switched to A before B's echo came, then
            // back.
            assert_eq!(give(&mut echoes, "b: 2", "a: 19"), Echo::Apply);
            echoes.sent("b: 27");
            assert_eq!(give(&mut echoes, "a: 19", "b: 27"), Echo::Apply);
            assert_eq!(give(&mut echoes, "b: 27", "a: 19"), Echo::Apply);
            assert_eq!(waiting(&echoes), 0, "listed {listed}");
            // Three files: typed in B, on to C and A, back to B.
            echoes.sent("b: 278");
            assert_eq!(give(&mut echoes, "c: 3", "b: 278"), Echo::Apply);
            assert_eq!(give(&mut echoes, "a: 19", "c: 3"), Echo::Apply);
            assert_eq!(give(&mut echoes, "b: 278", "a: 19"), Echo::Apply);
        }
    }

    /// The texts `0`, `1`, … `count - 1` sent, the document the last.
    fn sent_texts(count: usize) -> (Echoes, String) {
        let mut echoes = Echoes::new(true);
        echoes.given("start");
        for index in 0..count {
            echoes.sent(&index.to_string());
        }
        (echoes, (count - 1).to_string())
    }

    /// Hands every text sent back in `order` (indices), and each text sent
    /// again after them: whether one was taken for the host's own (a
    /// rollback), and how many copies wait at the end.
    fn hand_back(echoes: &mut Echoes, document: &str, order: &[usize]) -> (bool, usize) {
        let mut queue: std::collections::VecDeque<String> =
            order.iter().map(ToString::to_string).collect();
        let mut rolled_back = false;
        while let Some(text) = queue.pop_front() {
            match echoes.received(&text, document, NONE) {
                Echo::Apply => rolled_back |= text != document,
                Echo::Resend => queue.push_back(document.to_string()),
                Echo::Skip | Echo::Settle => {}
            }
        }
        (rolled_back, waiting(echoes))
    }

    /// However many texts wait for their echo (a busy page: thousands of
    /// keys typed before the first echo; QA 29: past 256 the oldest was
    /// forgotten and its echo rolled typing back), each echo is told apart,
    /// in order, backwards or shuffled, and once all are back none waits.
    #[test]
    fn every_text_waiting_is_an_echo_however_many_wait() {
        let mut next = rng(0x5851_f42d_4c95_7f2d);
        for count in [10, 257, 300, 5000] {
            let mut shuffled: Vec<usize> = (0..count).collect();
            for at in (1..count).rev() {
                shuffled.swap(at, usize::try_from(next(at as u64 + 1)).unwrap_or(0));
            }
            for order in [
                (0..count).collect::<Vec<_>>(),
                (0..count).rev().collect(),
                shuffled,
            ] {
                let (mut echoes, document) = sent_texts(count);
                let (rolled_back, left) = hand_back(&mut echoes, &document, &order);
                assert!(!rolled_back, "{count} waiting, {:?}…", &order[..3]);
                assert_eq!(left, 0, "{count} waiting");
            }
        }
    }

    /// What is kept is what waits: a copy is forgotten when its echo comes,
    /// or when the host's own text replaces the document (the copies sent
    /// before it); a host handing back only the latest of each burst
    /// leaves the others, still echoes should they come.
    #[test]
    fn the_copies_kept_are_those_waiting() {
        let (mut echoes, document) = sent_texts(1000);
        assert_eq!(echoes.received(&document, &document, NONE), Echo::Apply);
        assert_eq!(waiting(&echoes), 999);
        assert_eq!(echoes.received("500", &document, NONE), Echo::Resend);
        assert_eq!(waiting(&echoes), 999);
        // The host's own text: none of them waits any more.
        assert_eq!(echoes.received("other file", &document, NONE), Echo::Apply);
        assert_eq!(waiting(&echoes), 0);
        assert_eq!(echoes.received("10", "other file", NONE), Echo::Apply);
        // Typed on the other file, its echo not back: still an echo.
        echoes.sent("other file!");
        let flushed = super::clock();
        echoes.sent("other file!!");
        assert_eq!(
            echoes.received("reset", "other file!!", flushed),
            Echo::Apply
        );
        assert_eq!(waiting(&echoes), 1);
        assert_eq!(echoes.received("other file!!", "reset", NONE), Echo::Resend);
    }

    /// The document's print is kept from the text sent last, until the
    /// document changes without the host being told (no `onChange` then):
    /// it is read again, so the host is told a document it never had.
    #[test]
    fn a_document_the_host_was_not_told_is_read_again() {
        let mut echoes = Echoes::new(true);
        echoes.given("a");
        echoes.sent("ab");
        echoes.unsent();
        // Typed over `b` (the same length), with no `onChange`.
        let ((), reads) = counted(Work::TextRead, || {
            assert_eq!(echoes.listed("a", "ax", NONE), Echo::Resend);
        });
        assert_eq!(reads, 2);
        assert_eq!(echoes.received("ab", "ax", NONE), Echo::Settle);
        assert_eq!(echoes.received("ax", "ax", NONE), Echo::Apply);
        assert_eq!(waiting(&echoes), 0);
    }

    /// A host handing back only the latest text every 200 ms while 5000
    /// keys are typed, 30 ms a key: the copies it skipped are forgotten
    /// [`super::FORGET_MS`] after a later one came back, so what is kept
    /// stays bounded by what is sent in that time, however long the
    /// session; none of its echoes rolls back.
    #[test]
    fn a_host_handing_back_only_the_latest_leaves_a_bounded_number_kept() {
        let mut echoes = Echoes::new(true);
        echoes.given("");
        let mut document = String::new();
        let mut most = 0;
        for key in 0..5000u32 {
            let now = f64::from(key * 30);
            echoes.at(now);
            document.push(char::from(b'a' + (key % 26) as u8));
            echoes.sent(&document);
            if key % 7 == 6 {
                assert_eq!(echoes.received(&document, &document, NONE), Echo::Apply);
            }
            most = most.max(waiting(&echoes));
        }
        let window = (super::FORGET_MS / 30.0) as usize + 10;
        assert!(
            most <= window,
            "{most} copies kept at most (bound {window})"
        );
    }

    /// Echoes handed back out of order, up to a second after a later one,
    /// over a long run: each is still told apart (the copies sent before a
    /// text that came back are kept long enough), and once all are back
    /// none waits.
    #[test]
    fn echoes_out_of_order_within_the_pause_are_told_apart() {
        let mut next = rng(0x9e37_79b9_7f4a_7c15);
        let mut echoes = Echoes::new(true);
        echoes.given("");
        let mut document = String::new();
        // (when it comes back, text)
        let mut on_their_way: Vec<(u64, String)> = Vec::new();
        for time in 0..20_000u64 {
            echoes.at(time as f64);
            if time % 10 == 0 && time < 15_000 {
                document.push('x');
                echoes.sent(&document);
                on_their_way.push((time + next(1000), document.clone()));
            }
            on_their_way.sort();
            while on_their_way.first().is_some_and(|(at, _)| *at <= time) {
                let (_, text) = on_their_way.remove(0);
                match echoes.received(&text, &document, NONE) {
                    Echo::Apply => assert_eq!(text, document, "a rollback at {time}"),
                    Echo::Resend => on_their_way.push((time + next(1000), document.clone())),
                    Echo::Skip | Echo::Settle => {}
                }
            }
        }
        assert!(on_their_way.is_empty());
        assert_eq!(waiting(&echoes), 0);
    }

    /// An echo costs one read of the text handed back and a look at a copy
    /// or two of it, however many copies wait: never a pass over them, nor
    /// another read of the document (QA 29: typing slowed with echoes
    /// late on a long file).
    #[test]
    fn an_echo_costs_the_same_however_many_wait() {
        let mut costs = Vec::new();
        for count in [16, 250, 4096] {
            let (mut echoes, document) = sent_texts(count);
            // The echoes come back in pairs swapped (out of order), then
            // the host lists one it held again (a render).
            let order: Vec<usize> = (0..count).map(|index| index ^ 1).collect();
            let echoes_given = count / 2 + 1;
            let (visits, reads) = counted(Work::TextRead, || {
                counted(Work::CopyVisited, || {
                    for index in &order[..count / 2] {
                        let text = index.to_string();
                        assert_ne!(echoes.received(&text, &document, NONE), Echo::Apply);
                    }
                    let last = order[count / 2 - 1].to_string();
                    assert_ne!(echoes.listed(&last, &document, NONE), Echo::Apply);
                })
                .1
            });
            costs.push((
                count,
                visits as f64 / echoes_given as f64,
                reads as f64 / echoes_given as f64,
            ));
        }
        // Per echo: copies looked at, whole texts read.
        assert!(
            costs
                .iter()
                .all(|(_, visits, reads)| *visits <= 2.0 && *reads <= 1.0),
            "(waiting, copies looked at, texts read) per echo: {costs:?}"
        );
    }

    /// A host storing each text the editor sends after a delay and
    /// rendering a moment after a change (React, Vue and Svelte render once
    /// for several changes: of the texts stored in between, the editor is
    /// given only the last), each render giving the editor what it holds.
    struct Host {
        echoes: Echoes,
        document: String,
        /// What the host stores.
        host: String,
        /// (time, order, what): `Some(text)` stores a text, `None` renders.
        events: Vec<(u64, u64, Option<String>)>,
        order: u64,
        render_due: bool,
        /// When the pause after a `Settle` or `Resend` ends.
        settle_at: Option<u64>,
        resent: usize,
    }

    impl Host {
        fn new() -> Self {
            let mut echoes = Echoes::new(true);
            echoes.given("");
            Self {
                echoes,
                document: String::new(),
                host: String::new(),
                events: Vec::new(),
                order: 0,
                render_due: false,
                settle_at: None,
                resent: 0,
            }
        }

        fn push(&mut self, at: u64, what: Option<String>) {
            self.order += 1;
            self.events.push((at, self.order, what));
        }

        /// The host hears the document at `now` (`onChange`): it renders
        /// `lag` ms later and stores the text `delay` ms later.
        fn tell(&mut self, now: u64, (lag, delay): (u64, u64)) {
            self.render(now, lag);
            let text = self.document.clone();
            self.push(now + delay, Some(text));
        }

        fn render(&mut self, now: u64, lag: u64) {
            if !self.render_due {
                self.render_due = true;
                self.push(now + lag, None);
            }
        }

        /// The user types `key` (`None`: a backspace) at `now`.
        fn typed(&mut self, key: Option<char>, now: u64, delays: (u64, u64)) {
            match key {
                Some(key) => self.document.push(key),
                None => {
                    self.document.pop();
                }
            }
            self.echoes.sent(&self.document);
            self.tell(now, delays);
        }

        /// Runs the next event due by `until`; false when there is none.
        fn step(&mut self, until: u64, mut delays: impl FnMut() -> (u64, u64)) -> bool {
            self.events.sort_unstable();
            let event = self.events.first().map(|event| event.0);
            let Some(at) = [event, self.settle_at].into_iter().flatten().min() else {
                return false;
            };
            if at > until {
                return false;
            }
            if self.settle_at == Some(at) && event != Some(at) {
                self.settle_at = None;
                if self.echoes.settle(&self.document) {
                    self.resent += 1;
                    self.tell(at, delays());
                }
                return true;
            }
            let (_, _, what) = self.events.remove(0);
            if let Some(text) = what {
                self.host = text;
                let (lag, _) = delays();
                self.render(at, lag);
                return true;
            }
            self.render_due = false;
            self.echoes.at(at as f64);
            match self.echoes.listed(&self.host, &self.document, NONE) {
                Echo::Apply => assert_eq!(self.host, self.document, "a rollback"),
                Echo::Skip => {}
                echo => {
                    self.settle_at = Some(at + u64::from(SETTLE_MS.unsigned_abs()));
                    if echo == Echo::Resend {
                        self.resent += 1;
                        self.tell(at, delays());
                    }
                }
            }
            true
        }

        /// Types `keys` `every` ms a key, then waits for everything to come
        /// back.
        fn type_keys(
            &mut self,
            keys: &[Option<char>],
            every: u64,
            mut delays: impl FnMut() -> (u64, u64),
        ) {
            for (index, key) in keys.iter().enumerate() {
                let now = index as u64 * every;
                while self.step(now, &mut delays) {}
                self.typed(*key, now, delays());
            }
            while self.step(u64::MAX, &mut delays) {}
        }
    }

    fn rng(mut seed: u64) -> impl FnMut(u64) -> u64 {
        move |bound: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % bound
        }
    }

    /// Typing `# abcdefghij`, a backspace and `XY` 25 ms a key against a
    /// host storing each text 0-200 ms late and rendering 0-20 ms after a
    /// change (the documents pages' parents): the editor never rolls back
    /// and the host ends on the document.
    #[test]
    fn a_host_rendering_once_for_several_late_texts_ends_on_the_document() {
        let mut keys: Vec<Option<char>> = "# abcdefghij".chars().map(Some).collect();
        keys.push(None);
        keys.extend("XY".chars().map(Some));
        let mut next = rng(0x9e37_79b9_7f4a_7c15);
        for round in 0..20_000 {
            let lag = 1 + round % 21;
            let mut host = Host::new();
            host.type_keys(&keys, 25, || (next(lag), next(200)));
            assert_eq!(host.document, "# abcdefghiXY");
            assert_eq!(host.host, host.document, "round {round}");
        }
    }

    /// A step of a scripted run: the user types the `n`th text, or the
    /// host stores the `n`th text sent.
    #[derive(Clone, Copy, Debug)]
    enum Step {
        Type(usize),
        Store(usize),
    }

    /// Every order of `texts.len()` keys and their texts coming back, each
    /// after it is sent.
    fn orders(count: usize) -> Vec<Vec<Step>> {
        fn grow(
            count: usize,
            typed: usize,
            stored: &mut Vec<bool>,
            order: &mut Vec<Step>,
            out: &mut Vec<Vec<Step>>,
        ) {
            if order.len() == 2 * count {
                out.push(order.clone());
                return;
            }
            if typed < count {
                order.push(Step::Type(typed));
                grow(count, typed + 1, stored, order, out);
                order.pop();
            }
            for index in 0..typed {
                if !stored[index] {
                    stored[index] = true;
                    order.push(Step::Store(index));
                    grow(count, typed, stored, order, out);
                    order.pop();
                    stored[index] = false;
                }
            }
        }
        let mut out = Vec::new();
        grow(count, 0, &mut vec![false; count], &mut Vec::new(), &mut out);
        out
    }

    /// Typing `a`, `b`, a backspace and `c` (the text `a` sent twice), the
    /// host storing the texts in every order and rendering after any of
    /// the steps (several stored texts in one render: only the last is
    /// given), then the texts sent again and the pause's check: the editor
    /// never rolls back, and the host ends on the document.
    #[test]
    fn every_order_and_grouping_of_echoes_ends_on_the_document() {
        let texts = ["a", "ab", "a", "ac"];
        let orders = orders(texts.len());
        assert_eq!(orders.len(), 105);
        let steps = 2 * texts.len();
        for order in &orders {
            for renders in 0..1u32 << (steps - 1) {
                let mut echoes = Echoes::new(true);
                echoes.given("");
                let mut document = "";
                let mut host = String::new();
                let mut again: Vec<String> = Vec::new();
                let mut settle = false;
                let render = |host: &str,
                              document: &str,
                              echoes: &mut Echoes,
                              again: &mut Vec<String>,
                              settle: &mut bool| {
                    match echoes.listed(host, document, NONE) {
                        Echo::Apply => assert_eq!(host, document, "{order:?} {renders:b}"),
                        Echo::Skip => {}
                        Echo::Settle => *settle = true,
                        Echo::Resend => {
                            *settle = true;
                            again.push(document.to_string());
                        }
                    }
                };
                for (at, step) in order.iter().enumerate() {
                    match *step {
                        Step::Type(index) => {
                            document = texts[index];
                            echoes.sent(document);
                        }
                        Step::Store(index) => host = texts[index].to_string(),
                    }
                    if at + 1 == steps || renders & (1 << at) != 0 {
                        render(&host, document, &mut echoes, &mut again, &mut settle);
                    }
                }
                loop {
                    if !again.is_empty() {
                        host = again.remove(0);
                        render(&host, document, &mut echoes, &mut again, &mut settle);
                    } else if std::mem::take(&mut settle) {
                        if echoes.settle(document) {
                            again.push(document.to_string());
                        }
                    } else {
                        break;
                    }
                }
                assert_eq!(host, document, "{order:?} {renders:b}");
            }
        }
    }

    /// Long random runs: typing and backspaces at 10-60 ms a key, the host
    /// storing each text 0-400 ms late and rendering 0-30 ms after a
    /// change: the editor never rolls back, the host ends on the document,
    /// and it hears a text again less often than once per key typed.
    #[test]
    fn long_random_runs_end_on_the_document() {
        let mut next = rng(0x2545_f491_4f6c_dd1d);
        let mut resent = 0;
        let mut typed = 0;
        for round in 0..300 {
            let keys: Vec<Option<char>> = (0..200)
                .map(|_| (next(4) != 0).then(|| char::from(b'a' + next(3) as u8)))
                .collect();
            let every = 10 + next(51);
            let (lag, delay) = (1 + next(31), 1 + next(401));
            let mut host = Host::new();
            host.type_keys(&keys, every, || (next(lag), next(delay)));
            assert_eq!(host.host, host.document, "round {round}");
            resent += host.resent;
            typed += keys.len();
        }
        assert!(resent < typed, "{resent} texts sent again for {typed} keys");
    }
}
