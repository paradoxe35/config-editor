//! What a listed document's ranges become as its text is taken.

use config_editor_leptos::{Document, TextPosition, TextRange};

use super::take;
use crate::given_ranges::Again;

/// A language's sample: its file name, text, the range locking its last
/// field (as `rangeOf` gives it, by lines), the text a line is typed
/// after (above the lock) and that line.
struct Sample {
    file: &'static str,
    text: &'static str,
    lock: [usize; 4],
    after: &'static str,
    line: &'static str,
}

const SAMPLES: &[Sample] = &[
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
    fn typed(&self) -> Vec<String> {
        let at = self.text.find(self.after).expect("after") + self.after.len();
        let (head, tail) = self.text.split_at(at);
        (0..=self.line.len())
            .map(|end| format!("{head}\n{}{tail}", &self.line[..end]))
            .collect()
    }

    fn lock(&self) -> TextRange {
        let [start_line, start_column, end_line, end_column] = self.lock;
        TextRange::new(
            TextPosition::new(start_line, start_column),
            TextPosition::new(end_line, end_column),
        )
    }
}

/// QA 34 (round 33's FW33-1 and HO33-2 back): a list giving the ranges
/// given last again, with an echo of a text the editor typed past (in
/// order, newest first or in any order), keeps the ranges its editor was
/// given, so its editor keeps them where it moved them. They were moved
/// from the echo's text to the document as if given in its places: a lock
/// whose line ends past the end of the line that took its place (TOML and
/// HCL's `host = "h"` under a new line, over `port = 80`) came back
/// changed, was given again one line up and freed the locked value.
#[test]
fn ranges_that_stay_are_not_moved_from_an_echo() {
    for sample in SAMPLES {
        let (file, lock) = (sample.file, sample.lock());
        let texts = sample.typed();
        let current = texts.last().expect("typed");
        for echo in &texts {
            let applies = echo == current;
            let mut document = Document::new("m", file, echo.clone()).read_only_ranges(vec![lock]);
            let kept = take(&mut document, current, Some(Again::Stayed), applies);
            assert_eq!(kept, None, "{file} {echo:?}");
            assert_eq!(document.read_only_ranges, vec![lock], "{file} {echo:?}");
            assert_eq!(document.value, *current, "{file} {echo:?}");
        }
    }
}

/// Ranges read in a listed text that turns out to be an echo give way to
/// the ones its editor was given (kept for the next list); in a text that
/// replaces the document, the ones read stay.
#[test]
fn ranges_read_in_an_echo_give_way_to_the_ones_given() {
    let read = TextRange::new(TextPosition::new(1, 1), TextPosition::new(1, 2));
    for sample in SAMPLES {
        let (file, lock) = (sample.file, sample.lock());
        let texts = sample.typed();
        let current = texts.last().expect("typed");
        for echo in &texts[..texts.len() - 1] {
            let mut document = Document::new("m", file, echo.clone()).read_only_ranges(vec![read]);
            let kept = take(&mut document, current, Some(Again::Read(vec![lock])), false);
            assert_eq!(kept, Some(vec![lock]), "{file} {echo:?}");
            assert_eq!(document.read_only_ranges, vec![lock], "{file} {echo:?}");
            assert_eq!(document.value, *current, "{file} {echo:?}");
        }
        // The host's own text: the ranges read in it.
        let mut document =
            Document::new("m", file, sample.text.to_string()).read_only_ranges(vec![read]);
        let kept = take(&mut document, current, Some(Again::Read(vec![lock])), true);
        assert_eq!(kept, None, "{file}");
        assert_eq!(document.read_only_ranges, vec![read], "{file}");
        assert_eq!(document.value, sample.text, "{file}");
    }
}
