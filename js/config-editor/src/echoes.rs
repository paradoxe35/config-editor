//! Late echoes of the editor's own edits: a host that stores each
//! `onChange` text and hands it back through `update` (`{ value }`, or the
//! whole `documents` list) may do it late, after the user typed on. Such an
//! echo is a text the editor already moved past: applying it would roll the
//! document back and lose what was typed since (QA 27: `abcdefghij` typed
//! at 40 ms a key against a 60 ms echo became `xabdfhj`). A text the editor
//! never sent is the host's own (a reset, a loaded file) and wins.
//!
//! The rule the framework bindings' `ValueSync` (`js/config-editor/sync.js`)
//! follows, for the vanilla classes: echoes come back in order, so a sent
//! text with newer ones still on their way is stale; a host text supersedes
//! everything still on its way, and when the host takes one of those late
//! anyway it is told the document again.
//!
//! Unlike a framework's bound value, a vanilla host need not hand texts
//! back at all (`onChange: save`); such a host setting a text the editor
//! sent earlier (a revert to what it saved) means it. So a sent text counts
//! as an echo once the host is seen echoing: it handed back the oldest text
//! on its way, or a text within a second of its sending.

use std::collections::VecDeque;

/// How long a sent text may take to come back (in milliseconds).
const ECHO_WINDOW_MS: f64 = 10_000.0;
/// How many sent texts are remembered at most.
const MAX_PENDING: usize = 1000;
/// A sent text handed back this soon (in milliseconds) is an echo.
const QUICK_ECHO_MS: f64 = 1000.0;

/// A short stand-in for a text: its length and a 64-bit FNV-1a hash.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Print(usize, u64);

impl Print {
    fn of(text: &str) -> Self {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in text.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        Self(text.len(), hash)
    }
}

/// A text sent to the host, and when (milliseconds).
#[derive(Clone, Copy, Debug)]
struct Sent {
    print: Print,
    at: f64,
}

/// What to do with a text the host handed back.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Echo {
    /// The host's own text, or the newest echo: show it (nothing to do when
    /// it is the document already).
    Apply,
    /// A late echo of a text the editor moved past: keep the document.
    Skip,
    /// A late echo the host took after its own text replaced it: keep the
    /// document and send it to the host again (`onChange`).
    Resend,
}

/// The texts one document sent that the host has not handed back yet.
#[derive(Clone, Default, Debug)]
pub(crate) struct Echoes {
    pending: VecDeque<Sent>,
    /// Texts still on their way when a host text replaced the document.
    superseded: VecDeque<Sent>,
    /// Whether the host hands the texts back (see the module's docs).
    echoing: bool,
}

impl Echoes {
    /// The editor sent `text` to the host (its `onChange`) at `now`.
    pub(crate) fn sent(&mut self, text: &str, now: f64) {
        // A host that never hands texts back leaves them here: they go once
        // they could no longer come.
        while self
            .pending
            .front()
            .is_some_and(|sent| now - sent.at >= ECHO_WINDOW_MS)
        {
            self.pending.pop_front();
        }
        self.pending.push_back(Sent {
            print: Print::of(text),
            at: now,
        });
        if self.pending.len() > MAX_PENDING {
            self.pending.pop_front();
        }
    }

    /// The host handed `value` back at `now`, the document being `current`.
    pub(crate) fn received(&mut self, value: &str, current: &str, now: f64) -> Echo {
        let fresh = |sent: &Sent| now - sent.at < ECHO_WINDOW_MS;
        self.pending.retain(fresh);
        self.superseded.retain(fresh);
        let print = Print::of(value);
        if let Some(at) = self.echo_in(&self.superseded, print, now) {
            self.echoing = true;
            self.superseded.drain(..=at);
            if self.superseded.is_empty() && current != value {
                self.sent(current, now);
                return Echo::Resend;
            }
            return Echo::Skip;
        }
        // The oldest copy: a typo fixed sends the same text twice.
        match self.echo_in(&self.pending, print, now) {
            Some(at) => {
                self.echoing = true;
                self.pending.drain(..=at);
                if !self.pending.is_empty() {
                    return Echo::Skip;
                }
            }
            None => self.supersede(),
        }
        Echo::Apply
    }

    /// The host replaced the document with a text of its own, whatever it
    /// is (`replaceValue`): what is still on its way is superseded.
    pub(crate) fn replaced(&mut self) {
        self.echoing = true;
        self.supersede();
    }

    /// Where in `sent` the text `print` is an echo: its oldest copy, while
    /// the host is seen echoing, or it is the oldest text there or was sent
    /// a moment ago (see the module's docs).
    fn echo_in(&self, sent: &VecDeque<Sent>, print: Print, now: f64) -> Option<usize> {
        let at = sent.iter().position(|sent| sent.print == print)?;
        (self.echoing || at == 0 || now - sent[at].at < QUICK_ECHO_MS).then_some(at)
    }

    /// A host text replaced the document: what is still on its way is
    /// superseded.
    fn supersede(&mut self) {
        self.superseded.extend(self.pending.drain(..));
        while self.superseded.len() > MAX_PENDING {
            self.superseded.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Echo, Echoes};

    /// Typing `abc` with every echo one key late: each one but the newest is
    /// stale, the newest applies (the document already).
    #[test]
    fn a_late_echo_is_skipped_and_the_newest_applies() {
        let mut echoes = Echoes::default();
        echoes.sent("a", 0.0);
        echoes.sent("ab", 40.0);
        assert_eq!(echoes.received("a", "ab", 60.0), Echo::Skip);
        echoes.sent("abc", 80.0);
        assert_eq!(echoes.received("ab", "abc", 100.0), Echo::Skip);
        assert_eq!(echoes.received("abc", "abc", 140.0), Echo::Apply);
    }

    /// A text the editor never sent is the host's own, even with echoes on
    /// their way; those echoes are skipped afterwards, and the host taking
    /// the last of them is told the document again.
    #[test]
    fn the_hosts_own_text_wins_over_echoes() {
        let mut echoes = Echoes::default();
        echoes.sent("", 0.0);
        assert_eq!(echoes.received("", "", 0.0), Echo::Apply);
        echoes.sent("a", 0.0);
        echoes.sent("ab", 10.0);
        assert_eq!(echoes.received("reset", "ab", 20.0), Echo::Apply);
        assert_eq!(echoes.received("a", "reset", 30.0), Echo::Skip);
        assert_eq!(echoes.received("ab", "reset", 40.0), Echo::Resend);
        assert_eq!(echoes.received("reset", "reset", 50.0), Echo::Apply);
        // Nothing is on its way now: a text sent long ago is the host's.
        assert_eq!(echoes.received("a", "reset", 60.0), Echo::Apply);
    }

    /// The same text sent twice (a typo typed and undone) comes back twice:
    /// the first copy is stale while the second is on its way.
    #[test]
    fn a_text_sent_twice_matches_its_oldest_copy() {
        let mut echoes = Echoes::default();
        echoes.sent("a", 0.0);
        echoes.sent("ax", 1.0);
        echoes.sent("a", 2.0);
        assert_eq!(echoes.received("a", "a", 3.0), Echo::Skip);
        assert_eq!(echoes.received("ax", "a", 4.0), Echo::Skip);
        assert_eq!(echoes.received("a", "a", 5.0), Echo::Apply);
    }

    /// A host that does not hand texts back, setting one the editor sent a
    /// while ago (a revert to what it saved), means it.
    #[test]
    fn a_host_that_does_not_echo_sets_an_earlier_text() {
        let mut echoes = Echoes::default();
        echoes.sent("a", 0.0);
        echoes.sent("ab", 10.0);
        echoes.sent("abc", 2000.0);
        echoes.sent("abcd", 2010.0);
        assert_eq!(echoes.received("ab", "abcd", 3000.0), Echo::Apply);
        // Set again later, a text it replaced is its own too.
        assert_eq!(echoes.received("abc", "ab", 3100.0), Echo::Apply);
    }

    /// A slow host is seen echoing by its first, in-order echo; then any
    /// text it hands back is one.
    #[test]
    fn a_slow_host_handing_back_the_oldest_text_is_echoing() {
        let mut echoes = Echoes::default();
        for (index, text) in ["a", "ab", "abc", "abcd"].iter().enumerate() {
            echoes.sent(text, index as f64 * 1000.0);
        }
        assert_eq!(echoes.received("a", "abcd", 3500.0), Echo::Skip);
        // Coalesced: one skipped.
        assert_eq!(echoes.received("abc", "abcd", 6000.0), Echo::Skip);
        assert_eq!(echoes.received("abcd", "abcd", 7000.0), Echo::Apply);
    }

    /// A text the host replaces the document with is its own, whatever it
    /// is; echoes on their way then are skipped.
    #[test]
    fn a_replaced_document_supersedes_the_echoes_on_their_way() {
        let mut echoes = Echoes::default();
        echoes.sent("a", 0.0);
        echoes.sent("ab", 10.0);
        echoes.replaced();
        assert_eq!(echoes.received("a", "a", 20.0), Echo::Skip);
        assert_eq!(echoes.received("ab", "a", 30.0), Echo::Resend);
    }

    /// Echoes older than the window never come: a host text after it is
    /// the host's, and nothing grows without bound.
    #[test]
    fn old_and_many_texts_are_forgotten() {
        let mut echoes = Echoes::default();
        echoes.sent("a", 0.0);
        echoes.sent("ab", 1.0);
        assert_eq!(echoes.received("a", "ab", 20_000.0), Echo::Apply);
        echoes.sent("abc", 20_000.0);
        assert_eq!(echoes.received("abc", "abc", 20_001.0), Echo::Apply);
        for index in 0..5000 {
            echoes.sent(&index.to_string(), 30_000.0);
        }
        assert_eq!(echoes.pending.len(), 1000);
        assert_eq!(echoes.received("host", "4999", 30_001.0), Echo::Apply);
        assert_eq!(echoes.superseded.len(), 1000);
    }
}
