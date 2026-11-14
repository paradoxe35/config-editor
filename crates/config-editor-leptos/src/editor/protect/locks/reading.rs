//! A text read for its locks: parsed, and, when it does not parse, read
//! through a stand-in and line by line, each at most once and only when a
//! step asks for it.

use std::cell::OnceCell;

use serde_json::Value;

use super::super::super::codec;
use super::super::super::language::ConfigLanguage;
use super::super::super::language::SchemaPathSegment;
use super::super::locate::LineIndex;
use super::super::marks::{SchemaMarks, read_only_names, schema_marks};
use super::super::scan::{self, Written};
use super::{Protected, Sources, protect};

/// One text and the readings of it the steps share.
pub(super) struct Reading<'a> {
    pub(super) behavior: &'a dyn ConfigLanguage,
    pub(super) text: &'a str,
    pub(super) sources: Sources<'a>,
    pub(super) lines: LineIndex<'a>,
    /// The text parsed (with the schema's array reading), when it parses.
    pub(super) document: Option<Value>,
    /// The line the parser stopped at, when it does not parse.
    error_line: Option<usize>,
    stand_in: OnceCell<Option<StandInLocks>>,
    scanned: OnceCell<Vec<Written>>,
}

/// The locks of a stand-in (see `scan::stand_in`), its text, and the lines
/// it blanked.
pub(super) struct StandInLocks {
    pub(super) protected: Protected,
    pub(super) text: String,
    pub(super) blanked: Vec<usize>,
}

impl<'a> Reading<'a> {
    /// `text`, parsed with the sources' reading schema.
    pub(super) fn new(
        behavior: &'a dyn ConfigLanguage,
        text: &'a str,
        sources: Sources<'a>,
    ) -> Self {
        let lines = LineIndex::new(text);
        let (document, error_line) = match codec::read(behavior, text, sources.reading_schema) {
            Ok(document) => (Some(document), None),
            Err(error) => (None, error.line),
        };
        Reading {
            behavior,
            text,
            sources,
            lines,
            document,
            error_line,
            stand_in: OnceCell::new(),
            scanned: OnceCell::new(),
        }
    }

    /// What the marking schema marks in the parsed text (nothing when the
    /// text does not parse or there is no such schema).
    pub(super) fn schema_marks(&self) -> SchemaMarks {
        match (self.sources.marking_schema, &self.document) {
            (Some(schema), Some(document)) => schema_marks(document, schema),
            _ => SchemaMarks::default(),
        }
    }

    /// For a text that does not parse, the locks of the same text with the
    /// lines breaking it blanked, when that parses (found with the same
    /// marks and paths, without ranges or a text before): they fall on the
    /// same places, with the values read there. `None` for a text that
    /// parses.
    pub(super) fn stand_in(&self) -> Option<&StandInLocks> {
        self.stand_in
            .get_or_init(|| {
                if self.document.is_some() {
                    return None;
                }
                scan::stand_in(
                    self.behavior,
                    self.text,
                    self.error_line,
                    self.sources.reading_schema,
                )
                .map(|stand| {
                    let protected = protect(
                        self.behavior,
                        &stand.text,
                        Sources {
                            ranges: &[],
                            previous: None,
                            ..self.sources
                        },
                    );
                    StandInLocks {
                        protected,
                        text: stand.text,
                        blanked: stand.blanked,
                    }
                })
            })
            .as_ref()
    }

    /// Every line of the text holding code, with its path (see
    /// `scan::written`: in a large text, the lines that may hold a
    /// protected field).
    pub(super) fn written(&self) -> &[Written] {
        self.scanned.get_or_init(|| {
            scan::written(self.behavior, self.text, &self.lines, None, &self.names())
        })
    }

    /// The keys of the protected fields: the schema's read-only ones and
    /// those in the host's paths.
    pub(super) fn names(&self) -> Vec<String> {
        let mut names = self
            .sources
            .marking_schema
            .map(read_only_names)
            .unwrap_or_default();
        for path in self.sources.paths {
            for segment in path.segments() {
                if let SchemaPathSegment::Key(key) = segment
                    && !names.contains(key)
                {
                    names.push(key.clone());
                }
            }
        }
        names
    }

    /// The parsed text, taking it out of the reading.
    pub(super) fn into_document(self) -> Option<Value> {
        self.document
    }
}
