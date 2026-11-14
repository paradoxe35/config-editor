//! Counts of costly work, for the performance tests.
//!
//! A test that guards against work repeated per item (a read of the whole
//! document for each problem, a path lookup for each line) counts that work
//! instead of timing it: a count is the same on any machine under any load.
//! The code doing the work calls [`note`] at the one place it starts; in a
//! build without tests that is nothing at all.

/// A kind of costly work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Work {
    /// A read of a whole document for the spans of its keys or values
    /// (a language's span lookup, or one read shared by many).
    DocumentRead,
    /// One key's or value's span looked up alone while a batch runs for
    /// the text, outside what it read at once (the language answers from
    /// its readings of the text, counted as [`Work::DocumentRead`] where
    /// they are made).
    SpanLookup,
    /// The path at a place of a text: a read of the text from the top.
    PathLookup,
    /// A whole text split into its lines.
    LineSplit,
    /// A scan of a whole document recording where every key and value is
    /// written (with its path), where only the caret's place was wanted.
    #[cfg_attr(
        not(any(feature = "json", feature = "xml")),
        allow(dead_code, reason = "only the JSON and XML scanners note it")
    )]
    SpanScan,
    /// One element's document path built (XML), where only the caret's
    /// was wanted.
    #[cfg_attr(
        not(feature = "xml"),
        allow(dead_code, reason = "only the XML path plan notes it")
    )]
    ElementPath,
    /// A problem's marker compared with another's (or with the furthest
    /// end of a group of them), looking for the ones it overlaps.
    MarkerComparison,
    /// A pass over the host's secret placeholders (each compared with a
    /// value or a path), where one lookup among them was wanted; or one
    /// lookup of the text at a place among them (one note per length of
    /// their texts).
    #[cfg_attr(
        not(any(feature = "schema-validation", feature = "protect")),
        allow(dead_code, reason = "only schema validation and protection note it")
    )]
    TokenScan,
    /// The lines of a whole text's multi-line string bodies read
    /// (`ConfigLanguage::string_body_lines`).
    StringBodies,
    /// What the caret is on, for the suggestions
    /// (`ConfigLanguage::completion_context`): a read of the text from the
    /// top.
    CompletionContext,
    /// Whether the caret is in a string or a comment
    /// (`ConfigLanguage::lexical_state`): a read of the text before it.
    LexicalState,
    /// The code layer's constructs over several lines (comments, strings,
    /// structure) read off a whole text.
    Spanning,
    /// A whole text's fold ranges read.
    #[cfg_attr(
        not(feature = "folding"),
        allow(dead_code, reason = "only folding notes it")
    )]
    FoldRanges,
    /// A whole text diffed against the baseline, line by line.
    #[cfg_attr(
        not(feature = "diff"),
        allow(dead_code, reason = "only the diff notes it")
    )]
    LineDiff,
    /// A text's indent unit read off its first lines.
    IndentUnit,
    /// One entry of a container written between brackets passed, walking
    /// it for what a path names inside it.
    FlowEntry,
    /// A text holding an object of many entries parsed whole (HCL's
    /// parser compares each key with every key before it).
    #[cfg_attr(not(feature = "hcl"), allow(dead_code, reason = "only HCL notes it"))]
    UngroupedParse,
    /// A whole text's statements over lines (containers between brackets)
    /// read (`inline::statements`).
    Statements,
    /// A byte of a text read to tell a place in it as UTF-16 units (one
    /// note per byte, see [`note_many`]).
    #[cfg_attr(
        not(feature = "protect"),
        allow(dead_code, reason = "only the lock guard notes it")
    )]
    OffsetBytes,
    /// A text read from its top to find one line or one place in it (where
    /// line `n` starts, the line and column of a byte), outside a batch
    /// that splits its lines once.
    LineSeek,
    /// A whole text scanned for where it writes its keys (every place a
    /// rename writes the new key: a TOML key's headers and dotted keys, an
    /// XML element's tags, an HCL block's type or label).
    #[cfg_attr(
        not(any(feature = "toml", feature = "hcl", feature = "xml")),
        allow(dead_code, reason = "only the TOML, HCL and XML key scans note it")
    )]
    KeyScan,
    /// A byte of a whole text parsed (one note per byte, see [`note_many`]):
    /// reading a document is the costliest thing a keystroke can start.
    ParsedBytes,
    /// A whole text read for the constructs written over its lines
    /// (brackets, strings, elements), to widen a selection over them
    /// (`ConfigLanguage::whole_constructs`).
    ConstructScan,
    /// An expression's pointer looked for among another text's (HCL: a
    /// save's check that expressions stay expressions).
    #[cfg_attr(
        not(feature = "hcl"),
        allow(dead_code, reason = "only HCL's expression check notes it")
    )]
    ExpressionLookup,
    /// A key of a map visited while merging an Edit-as edit into the
    /// host's value (`bridge::merge`).
    MergeKeyVisit,
    /// Two list items compared while matching a list's items to the ones
    /// they were (`codec::moves`).
    ItemComparison,
    /// The docs of a place looked up (`completion::key_hover`): its key or
    /// value read off the whole document.
    HoverLookup,
    /// Another tab's file name read for a tab's label (`documents::tab_names`).
    TabNameVisit,
    /// A secret placeholder's place looked up among the places it is held
    /// (`protect::tokens`).
    #[cfg_attr(
        not(feature = "protect"),
        allow(dead_code, reason = "only the secret placeholders note it")
    )]
    PlaceLookup,
    /// A byte of a row's text built into a new view of the code layer (one
    /// note per byte, see [`note_many`]): a long row is drawn in pieces, and
    /// a keystroke builds again only the pieces it changed.
    RowText,
    /// A suggestion of the open list built for the page
    /// (`completion::window`).
    SuggestionRow,
    /// A character read on its own to place a UTF-16 offset in a text
    /// (`keyboard::utf16_to_byte_index`): ASCII is passed a block at a
    /// time.
    CharacterStep,
    /// A byte of a whole document's value read the way a schema expects
    /// (`completion::conform_arrays`: lone values where only a list is
    /// allowed made lists; one note per byte of its text, see
    /// [`note_many`]), as a host that repeats keys for lists reads a text.
    ConformedBytes,
    /// A schema's branches resolved (`completion::branches_for`): its
    /// references, combinators and conditionals walked.
    BranchWalk,
    /// A byte of a text passed from its top to find where a UTF-16 offset
    /// is (`keyboard::utf16_to_byte_index`, outside a batch that splits
    /// its lines once; one note per byte).
    OffsetSeek,
    /// The host's read-only ranges followed from one host text into
    /// another (`protect::host_ranges`: a line diff of the two).
    #[cfg_attr(
        not(feature = "protect"),
        allow(dead_code, reason = "only protection notes it")
    )]
    RangesFollowed,
    /// A line of an edited region weighed against one lock (whether it
    /// would join it), in the lock check of a text that does not parse.
    #[cfg_attr(
        not(feature = "protect"),
        allow(dead_code, reason = "only the lock check notes it")
    )]
    LockLine,
}

impl Work {
    #[cfg(test)]
    const fn index(self) -> usize {
        match self {
            Self::DocumentRead => 0,
            Self::PathLookup => 1,
            Self::LineSplit => 2,
            Self::SpanScan => 3,
            Self::ElementPath => 4,
            Self::MarkerComparison => 5,
            Self::TokenScan => 6,
            Self::StringBodies => 7,
            Self::CompletionContext => 8,
            Self::Spanning => 9,
            Self::FoldRanges => 10,
            Self::LineDiff => 11,
            Self::LexicalState => 12,
            Self::IndentUnit => 13,
            Self::SpanLookup => 14,
            Self::FlowEntry => 15,
            Self::UngroupedParse => 16,
            Self::Statements => 17,
            Self::OffsetBytes => 18,
            Self::LineSeek => 19,
            Self::KeyScan => 20,
            Self::ParsedBytes => 21,
            Self::ConstructScan => 22,
            Self::ExpressionLookup => 23,
            Self::MergeKeyVisit => 24,
            Self::ItemComparison => 25,
            Self::HoverLookup => 26,
            Self::TabNameVisit => 27,
            Self::PlaceLookup => 28,
            Self::RowText => 29,
            Self::SuggestionRow => 30,
            Self::CharacterStep => 31,
            Self::ConformedBytes => 32,
            Self::BranchWalk => 33,
            Self::OffsetSeek => 34,
            Self::RangesFollowed => 35,
            Self::LockLine => 36,
        }
    }

    /// Every kind, for a test counting them all ([`counted_all`]).
    #[cfg(test)]
    pub(crate) const ALL: [Self; KINDS] = [
        Self::DocumentRead,
        Self::PathLookup,
        Self::LineSplit,
        Self::SpanScan,
        Self::ElementPath,
        Self::MarkerComparison,
        Self::TokenScan,
        Self::StringBodies,
        Self::CompletionContext,
        Self::Spanning,
        Self::FoldRanges,
        Self::LineDiff,
        Self::LexicalState,
        Self::IndentUnit,
        Self::SpanLookup,
        Self::FlowEntry,
        Self::UngroupedParse,
        Self::Statements,
        Self::OffsetBytes,
        Self::LineSeek,
        Self::KeyScan,
        Self::ParsedBytes,
        Self::ConstructScan,
        Self::ExpressionLookup,
        Self::MergeKeyVisit,
        Self::ItemComparison,
        Self::HoverLookup,
        Self::TabNameVisit,
        Self::PlaceLookup,
        Self::RowText,
        Self::SuggestionRow,
        Self::CharacterStep,
        Self::ConformedBytes,
        Self::BranchWalk,
        Self::OffsetSeek,
        Self::RangesFollowed,
        Self::LockLine,
    ];
}

/// How many kinds of work there are.
#[cfg(test)]
const KINDS: usize = 37;

#[cfg(test)]
thread_local! {
    static COUNTS: std::cell::Cell<[usize; KINDS]> = const { std::cell::Cell::new([0; KINDS]) };
}

/// `work` is starting (counted on this thread in tests).
#[inline(always)]
pub(crate) fn note(work: Work) {
    #[cfg(test)]
    COUNTS.with(|counts| {
        let mut now = counts.get();
        now[work.index()] += 1;
        counts.set(now);
    });
    #[cfg(not(test))]
    let _ = work;
}

/// `count` units of `work` are starting.
#[inline(always)]
pub(crate) fn note_many(work: Work, count: usize) {
    #[cfg(test)]
    COUNTS.with(|counts| {
        let mut now = counts.get();
        now[work.index()] += count;
        counts.set(now);
    });
    #[cfg(not(test))]
    let _ = (work, count);
}

/// Runs `run` and returns its result and how many times it started `work`.
#[cfg(test)]
pub(crate) fn counted<T>(work: Work, run: impl FnOnce() -> T) -> (T, usize) {
    let before = COUNTS.with(std::cell::Cell::get)[work.index()];
    let result = run();
    let after = COUNTS.with(std::cell::Cell::get)[work.index()];
    (result, after - before)
}

/// Runs `run` and returns its result and how many times it started each
/// kind of work, by [`Work::index`] (see [`Work::ALL`]).
#[cfg(test)]
pub(crate) fn counted_all<T>(run: impl FnOnce() -> T) -> (T, [usize; KINDS]) {
    let before = COUNTS.with(std::cell::Cell::get);
    let result = run();
    let after = COUNTS.with(std::cell::Cell::get);
    (
        result,
        std::array::from_fn(|kind| after[kind] - before[kind]),
    )
}

#[cfg(test)]
mod tests {
    use super::{KINDS, Work, counted, counted_all, note};

    #[test]
    fn counts_only_the_work_asked_inside_the_run() {
        note(Work::PathLookup);
        let ((), reads) = counted(Work::DocumentRead, || {
            note(Work::DocumentRead);
            note(Work::PathLookup);
            note(Work::DocumentRead);
        });
        assert_eq!(reads, 2);
        let ((), lookups) = counted(Work::PathLookup, || note(Work::PathLookup));
        assert_eq!(lookups, 1);
    }

    #[test]
    fn counting_all_kinds_lists_each_by_its_place() {
        for (place, work) in Work::ALL.into_iter().enumerate() {
            assert_eq!(work.index(), place, "{work:?}");
            let ((), counts) = counted_all(|| note(work));
            let expected: [usize; KINDS] = std::array::from_fn(|kind| usize::from(kind == place));
            assert_eq!(counts, expected, "{work:?}");
        }
    }
}
