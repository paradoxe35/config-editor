//! What one update does through a document's channel: its text, its
//! ranges, and the record the next update compares with.

use config_editor_leptos::js_support::{Echo, echo_clock};
use config_editor_leptos::{TextPosition, TextRange};

use super::{Given, HostChannel, Taken};

/// A language's sample: its file name, text, the range locking its last
/// field (as `rangeOf` gives it, by lines), the text a line is typed
/// after (above the lock) and that line.
pub(super) struct Sample {
    pub(super) file: &'static str,
    pub(super) text: &'static str,
    lock: [usize; 4],
    after: &'static str,
    pub(super) line: &'static str,
}

pub(super) const SAMPLES: &[Sample] = &[
    Sample {
        file: "m.yaml",
        text: "name: a\nport: 80\nhost: h\n",
        lock: [3, 1, 3, 8],
        after: "name: a",
        line: "x: 1",
    },
    Sample {
        file: "m.json",
        text: "{\n  \"name\": \"a\",\n  \"port\": 80,\n  \"host\": \"h\"\n}\n",
        lock: [4, 3, 4, 14],
        after: "\"name\": \"a\",",
        line: "\"x\": 1,",
    },
    Sample {
        file: "m.toml",
        text: "name = \"a\"\nport = 80\nhost = \"h\"\n",
        lock: [3, 1, 3, 11],
        after: "name = \"a\"",
        line: "x = 1",
    },
    Sample {
        file: "m.tf",
        text: "name = \"a\"\nport = 80\nhost = \"h\"\n",
        lock: [3, 1, 3, 11],
        after: "name = \"a\"",
        line: "x = 1",
    },
    Sample {
        file: "m.xml",
        text: "<c>\n  <name>a</name>\n  <port>80</port>\n  <host>h</host>\n</c>\n",
        lock: [4, 3, 4, 17],
        after: "<name>a</name>",
        line: "<x>1</x>",
    },
];

impl Sample {
    /// The texts the editor sends as the line is typed: the line break,
    /// then each character.
    pub(super) fn typed(&self) -> Vec<String> {
        let at = self.text.find(self.after).expect("after") + self.after.len();
        let (head, tail) = self.text.split_at(at);
        (0..=self.line.len())
            .map(|end| format!("{head}\n{}{tail}", &self.line[..end]))
            .collect()
    }

    pub(super) fn lock(&self) -> TextRange {
        let [start_line, start_column, end_line, end_column] = self.lock;
        TextRange::new(
            TextPosition::new(start_line, start_column),
            TextPosition::new(end_line, end_column),
        )
    }
}

/// A list entry (or an update) giving `value` with `ranges` printed
/// `print`, as a documents view takes it: both steps, nothing sent
/// meanwhile.
pub(super) fn list(
    channel: &mut HostChannel,
    value: &str,
    print: &str,
    ranges: &[TextRange],
    current: &str,
) -> Taken {
    let read = channel
        .ranges::<()>(
            Some(print.to_string()),
            Some(value),
            None,
            current,
            false,
            |_, _| Ok(ranges.to_vec()),
        )
        .expect("read");
    channel.received(
        Given {
            value: Some(value),
            revision: None,
            ranges_for: None,
            ranges: Some(read),
            diagnostics: None,
            flushed: echo_clock(),
            stale: false,
        },
        current,
    )
}

fn line(number: usize) -> TextRange {
    TextRange::new(TextPosition::new(number, 1), TextPosition::new(number, 13))
}

const T: &str = "name: a\nsecret: keep\nlisten: 1\n";
const ABOVE: &str = "z: 0\nname: a\nsecret: keep\nlisten: 1\n";

/// P1: a list giving the ranges given last again, with an echo of a text
/// the editor typed past (in order, newest first or in any order), keeps
/// the ranges its editor has, where it moved them. Moved from the echo's
/// text to the document as if given in its places, a lock whose line ends
/// past the end of the line that took its place (TOML and HCL's
/// `host = "h"` under a new line, over `port = 80`) would be handed one
/// line up and free the locked value.
#[test]
fn ranges_that_stay_are_not_moved_from_an_echo() {
    for sample in SAMPLES {
        let (file, lock) = (sample.file, sample.lock());
        let texts = sample.typed();
        let current = texts.last().expect("typed");
        for echo in &texts {
            let mut channel = HostChannel::new(
                true,
                sample.text,
                Some("[3]".into()),
                sample.text,
                vec![lock],
            );
            for text in &texts {
                channel.sent(text);
            }
            let taken = list(&mut channel, echo, "[3]", &[line(1)], current);
            assert_eq!(taken.ranges, None, "{file} {echo:?}");
            assert_eq!(taken.shows(), echo == current, "{file} {echo:?}");
        }
    }
}

/// Ranges worked out in an echo the user typed past are moved from it to
/// the document (the lock stays on its field); ranges given with a text
/// of the host's own are read in it, as given.
#[test]
fn ranges_given_with_an_echo_move_to_the_document() {
    for sample in SAMPLES {
        let (file, lock) = (sample.file, sample.lock());
        let texts = sample.typed();
        let current = texts.last().expect("typed");
        // The lock, one line down in each typed text.
        let below = TextRange::new(
            TextPosition::new(lock.start.line + 1, lock.start.column),
            TextPosition::new(lock.end.line + 1, lock.end.column),
        );
        for echo in &texts[..texts.len() - 1] {
            let mut channel = HostChannel::new(
                true,
                sample.text,
                Some("[old]".into()),
                sample.text,
                vec![lock],
            );
            for text in &texts {
                channel.sent(text);
            }
            let taken = list(&mut channel, echo, "[new]", &[below], current);
            assert_ne!(taken.echo, Some(Echo::Apply), "{file} {echo:?}");
            assert_eq!(taken.ranges, Some(vec![below]), "{file} {echo:?}");
        }
        // The host's own text: the ranges read in it.
        let mut channel = HostChannel::new(
            true,
            sample.text,
            Some("[old]".into()),
            sample.text,
            vec![lock],
        );
        let own = format!("{}# own\n", sample.text);
        let taken = list(&mut channel, &own, "[new]", &[line(1)], sample.text);
        assert!(taken.shows(), "{file}");
        assert_eq!(taken.ranges, Some(vec![line(1)]), "{file}");
    }
}

/// Ranges listed again stay, and the text they came with is the one the
/// next ones may be worked out on; other ranges with an echo are moved to
/// the document; a text from before a reset the host has not given back
/// yet keeps the ranges carried into the reset, and the same ones given
/// next are read anew.
#[test]
fn a_list_records_what_its_editor_took() {
    let mut channel = HostChannel::new(true, T, Some("[2]".into()), T, vec![line(2)]);
    channel.sent(ABOVE);
    channel.sent(T);
    // Given again with an echo: they stay.
    let taken = list(&mut channel, ABOVE, "[2]", &[line(9)], T);
    assert!(!taken.shows() && taken.ranges.is_none());
    // They came with the echo: the next ones may be worked out on it.
    assert_eq!(channel.locks.came_with(), [ABOVE, ABOVE]);
    // A reset to `ABOVE`: the lock carried into it; then a late `T` from
    // before it, with other ranges, is not taken.
    assert_eq!(channel.replaced(ABOVE, T), Some(vec![line(3)]));
    let version = channel.version();
    let taken = list(&mut channel, T, "[1]", &[line(1)], ABOVE);
    assert!(!taken.shows() && taken.ranges.is_none());
    assert_eq!(channel.version(), version);
    // The same ranges as before the reset, given next: read anew.
    assert!(!channel.locks.again(Some("[2]")));
}

/// Without echoes every text applies, and ranges given again with the
/// document itself stay where the editor moved them.
#[test]
fn without_echoes_every_text_applies() {
    let mut channel = HostChannel::new(false, T, Some("[2]".into()), T, vec![line(2)]);
    let taken = list(&mut channel, T, "[2]", &[line(9)], T);
    assert!(taken.shows() && taken.ranges.is_none());
    let taken = list(&mut channel, ABOVE, "[2]", &[line(3)], T);
    assert!(taken.shows());
    assert_eq!(taken.ranges, Some(vec![line(3)]));
}

/// A text of the host's own with no ranges hands the editor the given
/// ones carried into it, afresh from the ones given each time.
#[test]
fn a_text_of_its_own_alone_carries_the_ranges_given() {
    let mut channel = HostChannel::new(true, T, Some("[2]".into()), T, vec![line(2)]);
    for (text, want) in [(ABOVE, 3), (T, 2)] {
        let current = if text == ABOVE { T } else { ABOVE };
        let taken = channel.received(
            Given {
                value: Some(text),
                revision: None,
                ranges_for: None,
                ranges: None,
                diagnostics: None,
                flushed: echo_clock(),
                stale: false,
            },
            current,
        );
        assert!(taken.shows());
        assert_eq!(taken.ranges, Some(vec![line(want)]), "{text:?}");
    }
}

/// The text the host was told last, given while the update's flush sent
/// a newer one, keeps the document, with or without echoes.
#[test]
fn a_text_older_than_the_flush_keeps_the_document() {
    for echoing in [true, false] {
        let mut channel = HostChannel::new(echoing, T, None, T, Vec::new());
        let taken = channel.received(
            Given {
                value: Some(T),
                revision: None,
                ranges_for: None,
                ranges: None,
                diagnostics: None,
                flushed: echo_clock(),
                stale: true,
            },
            ABOVE,
        );
        assert_eq!(taken.echo, Some(Echo::Skip), "echoes: {echoing}");
    }
}

#[path = "listed_again_tests.rs"]
mod listed_again;
