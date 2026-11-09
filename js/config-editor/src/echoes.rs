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
//! the host's own and applies; to go back to a text still on its way, the
//! host calls `replaceValue`. A skipped echo leaves the host holding an old
//! text: unless an echo of the document is still to come (and set it right),
//! the host is told the document again (`onChange`). A text sent before one
//! that came back may still come (out of order) or never (a host handing
//! back only the latest of several): it stays an echo, but is not counted on
//! to come. The text the host gave last, given again (a store re-rendering
//! for another reason, the whole `documents` list listed for another
//! document's edit), is no news either: it keeps the document.
//!
//! The framework bindings' `ValueSync` (`js/config-editor/sync.js`) follows
//! the same rule.

use std::collections::VecDeque;

/// How many sent texts are remembered at most: a host that hands back only
/// some of them (a batching framework) leaves the others here.
const MAX_PENDING: usize = 256;

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

/// A text sent and not handed back yet.
#[derive(Clone, Copy, Debug)]
struct Sent {
    print: Print,
    /// No text sent after it has come back yet: the host is expected to
    /// hand it back after what it holds now.
    to_come: bool,
}

/// What to do with a text the host handed back.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Echo {
    /// The host's own text, or the document already: show it (nothing to
    /// do when it is the document).
    Apply,
    /// An echo of a text the editor moved past: keep the document.
    Skip,
    /// The same, and nothing still on its way brings the host back to the
    /// document: keep it and send it to the host again (`onChange`; already
    /// recorded as sent).
    Resend,
}

/// The texts one document sent that the host has not handed back yet.
#[derive(Clone, Default, Debug)]
pub(crate) struct Echoes {
    /// Whether the host hands every text back (`echoes: true`); without it
    /// nothing is recorded and every text applies.
    on: bool,
    /// The texts sent and not handed back yet, oldest first.
    pending: VecDeque<Sent>,
    /// The text the host gave last.
    last_given: Option<Print>,
}

impl Echoes {
    /// For a host that hands every text back (`on`), or not.
    pub(crate) fn new(on: bool) -> Self {
        Self {
            on,
            pending: VecDeque::new(),
            last_given: None,
        }
    }

    /// The host gave `text` as the document is built (the constructor's
    /// `value`, a document first listed).
    pub(crate) fn given(&mut self, text: &str) {
        if self.on {
            self.last_given = Some(Print::of(text));
        }
    }

    /// The editor sent `text` to the host (its `onChange`).
    pub(crate) fn sent(&mut self, text: &str) {
        if !self.on {
            return;
        }
        self.pending.push_back(Sent {
            print: Print::of(text),
            to_come: true,
        });
        if self.pending.len() > MAX_PENDING {
            self.pending.pop_front();
        }
    }

    /// The host handed `value` back, the document being `current`.
    pub(crate) fn received(&mut self, value: &str, current: &str) -> Echo {
        if !self.on {
            return Echo::Apply;
        }
        let print = Print::of(value);
        if self.last_given.replace(print) == Some(print) {
            // Given again: a render, or the echo of a copy sent since. That
            // copy stays an echo, but is not counted on to come.
            if let Some(copy) = self
                .pending
                .iter_mut()
                .rev()
                .find(|sent| sent.to_come && sent.print == print)
            {
                copy.to_come = false;
            }
        } else {
            // The newest copy (a typo fixed sends the same text twice): a
            // host handing back only the latest text hands back that one.
            let Some(at) = self.pending.iter().rposition(|sent| sent.print == print) else {
                return Echo::Apply;
            };
            self.pending.remove(at);
            for sent in self.pending.range_mut(..at) {
                sent.to_come = false;
            }
        }
        if value == current {
            return Echo::Apply;
        }
        let document = Print::of(current);
        if self
            .pending
            .iter()
            .any(|sent| sent.to_come && sent.print == document)
        {
            return Echo::Skip;
        }
        self.sent(current);
        Echo::Resend
    }
}

#[cfg(test)]
mod tests {
    use super::{Echo, Echoes, MAX_PENDING};

    /// Without the option every text applies and nothing is kept.
    #[test]
    fn without_the_option_every_text_applies() {
        let mut echoes = Echoes::new(false);
        echoes.sent("a");
        echoes.sent("ab");
        assert_eq!(echoes.received("a", "ab"), Echo::Apply);
        assert!(echoes.pending.is_empty());
    }

    /// Typing `abc` with every echo one key late: each one but the newest is
    /// stale and skipped (the newest still to come), the newest applies (the
    /// document already).
    #[test]
    fn a_late_echo_is_skipped_and_the_newest_applies() {
        let mut echoes = Echoes::new(true);
        echoes.sent("a");
        echoes.sent("ab");
        assert_eq!(echoes.received("a", "ab"), Echo::Skip);
        echoes.sent("abc");
        assert_eq!(echoes.received("ab", "abc"), Echo::Skip);
        assert_eq!(echoes.received("abc", "abc"), Echo::Apply);
        assert!(echoes.pending.is_empty());
    }

    /// Echoes in any order are skipped; the host, left on an old one with
    /// nothing else to come, hears the document again.
    #[test]
    fn echoes_in_any_order_are_skipped() {
        let mut echoes = Echoes::new(true);
        for text in ["a", "ab", "abc"] {
            echoes.sent(text);
        }
        assert_eq!(echoes.received("abc", "abc"), Echo::Apply);
        assert_eq!(echoes.received("a", "abc"), Echo::Resend);
        // The resent document is on its way: the host comes back to it.
        assert_eq!(echoes.received("ab", "abc"), Echo::Skip);
        assert_eq!(echoes.received("abc", "abc"), Echo::Apply);
        assert!(echoes.pending.is_empty());
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
                match echoes.received(&host, current) {
                    Echo::Apply => assert_eq!(host, current, "{order:?}"),
                    Echo::Skip => {}
                    Echo::Resend => resent += 1,
                }
            }
            // The resent texts come back, in order: the host ends on them.
            for _ in 0..resent {
                host = current.to_string();
                assert_eq!(echoes.received(current, current), Echo::Apply);
            }
            assert_eq!(host, current, "{order:?}");
            assert!(echoes.pending.is_empty(), "{order:?}");
        }
    }

    /// A text the editor never sent is the host's own, even with echoes on
    /// their way; those echoes are skipped afterwards, and the host, left on
    /// the last of them, hears the document again.
    #[test]
    fn the_hosts_own_text_wins_over_echoes() {
        let mut echoes = Echoes::new(true);
        echoes.sent("a");
        echoes.sent("ab");
        assert_eq!(echoes.received("reset", "ab"), Echo::Apply);
        assert_eq!(echoes.received("ab", "reset"), Echo::Resend);
        assert_eq!(echoes.received("a", "reset"), Echo::Skip);
        assert_eq!(echoes.received("reset", "reset"), Echo::Apply);
        // Every echo came back: a text sent earlier is the host's own now.
        assert_eq!(echoes.received("a", "reset"), Echo::Apply);
    }

    /// The same text sent twice (a typo typed and undone) comes back twice:
    /// the host ends on the document whichever copy each echo is taken for.
    #[test]
    fn a_text_sent_twice_comes_back_twice() {
        let mut echoes = Echoes::new(true);
        echoes.sent("a");
        echoes.sent("ax");
        echoes.sent("a");
        assert_eq!(echoes.received("a", "a"), Echo::Apply);
        // Taken for the newest copy, the first one is not counted on.
        assert_eq!(echoes.received("ax", "a"), Echo::Resend);
        assert_eq!(echoes.received("a", "a"), Echo::Apply);
        // Given again (a render): no news.
        assert_eq!(echoes.received("a", "a"), Echo::Apply);
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
        assert_eq!(echoes.received("ax", "ax"), Echo::Apply);
        assert_eq!(echoes.received("a", "ax"), Echo::Resend);
        assert_eq!(echoes.received("ax", "ax"), Echo::Apply);
        // Typed abc, the host coalesces to abc, then sets a: the document
        // stays and the host is told it again.
        for text in ["a", "ab", "abc"] {
            echoes.sent(text);
        }
        assert_eq!(echoes.received("abc", "abc"), Echo::Apply);
        assert_eq!(echoes.received("a", "abc"), Echo::Resend);
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
                    match echoes.received(&host, &document) {
                        Echo::Apply => assert_eq!(host, document, "round {round}"),
                        Echo::Skip => {}
                        Echo::Resend => in_flight.push((time + next(30), document.clone())),
                    }
                }
                // Now and then the host renders again with what it holds.
                if next(4) == 0 {
                    match echoes.received(&host, &document) {
                        Echo::Apply => assert_eq!(host, document, "round {round}"),
                        Echo::Skip => {}
                        Echo::Resend => in_flight.push((time + next(30), document.clone())),
                    }
                }
            }
            assert!(in_flight.is_empty(), "round {round}");
            assert_eq!(host, document, "round {round}");
        }
    }

    /// The host giving its last text again, its echoes still on their way,
    /// keeps the document; a text of its own still applies.
    #[test]
    fn the_last_text_given_again_is_no_news() {
        let mut echoes = Echoes::new(true);
        echoes.given("a");
        echoes.sent("ab");
        assert_eq!(echoes.received("a", "ab"), Echo::Skip);
        assert_eq!(echoes.received("ab", "ab"), Echo::Apply);
        echoes.sent("abc");
        // A re-render with what it holds, before the echo of `abc` comes.
        assert_eq!(echoes.received("ab", "abc"), Echo::Skip);
        assert_eq!(echoes.received("abc", "abc"), Echo::Apply);
        assert_eq!(echoes.received("reset", "abc"), Echo::Apply);
        assert_eq!(echoes.received("reset", "reset"), Echo::Apply);
    }

    /// A host handing back only some texts leaves the others here, never
    /// more than the bound.
    #[test]
    fn the_texts_kept_are_bounded() {
        let mut echoes = Echoes::new(true);
        for index in 0..5000 {
            echoes.sent(&index.to_string());
        }
        assert_eq!(echoes.pending.len(), MAX_PENDING);
        assert_eq!(echoes.received("host", "4999"), Echo::Apply);
        assert_eq!(echoes.received("0", "4999"), Echo::Apply);
        assert_eq!(echoes.received("4998", "4999"), Echo::Skip);
    }
}
