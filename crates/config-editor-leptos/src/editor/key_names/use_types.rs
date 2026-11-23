//! The public shapes of Find uses: what a host's `on_find_uses` is asked
//! ([`FindUses`]) and answers ([`Use`]), what `on_go_to_use` hears
//! ([`GoToUse`]), and what `tools::uses_at` finds ([`FoundUse`]).

use std::sync::Arc;

use crate::{FieldPath, TextRange};

/// A place using a name in a file the editor does not have open: what a
/// host answers [`FindUses`] with.
///
/// ```
/// use leptos_config_editor::{FieldPath, Use};
/// let found = Use::new("server.d/home.yaml", "/default_client")
///     .with_line(3)
///     .with_text("default_client: home");
/// assert_eq!(found.path, FieldPath::from("/default_client"));
/// assert_eq!(found.line, Some(3));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Use {
    /// The file (an id the host knows it by), shown as the group's title
    /// and handed back to `on_go_to_use`.
    pub source: String,
    /// The field using the name.
    pub path: FieldPath,
    /// Its one-based line, shown with it.
    pub line: Option<usize>,
    /// The text of that line, shown in the list (else the path).
    pub text: Option<String>,
    /// Whatever the host tells its file's versions by (a revision, a
    /// hash): kept with the place, never read.
    pub version: Option<String>,
}

impl Use {
    /// The use at `path` of the file `source`.
    pub fn new(source: impl Into<String>, path: impl Into<FieldPath>) -> Self {
        Self {
            source: source.into(),
            path: path.into(),
            line: None,
            text: None,
            version: None,
        }
    }

    /// The use, on one-based line `line`.
    #[must_use]
    pub fn with_line(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }

    /// The use, its line reading `text`.
    #[must_use]
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// The use, in the version `version` of its file.
    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }
}

/// A use [`tools::uses_at`](crate::tools::uses_at) found in a text.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FoundUse {
    /// The field using the name.
    pub path: FieldPath,
    /// Where the value (or key) is written.
    pub range: TextRange,
    /// Its one-based line.
    pub line: usize,
    /// The text of that line.
    pub text: String,
}

impl FoundUse {
    /// The [`Use`] to answer [`FindUses`] with, for the file `source`.
    pub fn into_use(self, source: impl Into<String>) -> Use {
        Use::new(source, self.path)
            .with_line(self.line)
            .with_text(self.text)
    }
}

/// The user asked for the uses of a name (`Shift+F12`, the hover card's
/// Uses link, [`EditorHandle::find_uses`](crate::EditorHandle::find_uses)):
/// what `on_find_uses` is asked, for the files the editor does not have
/// open. Answer with [`reply`](Self::reply), at once or later: the open
/// documents' uses show at once, the host's join them when they come. An
/// answer to a request the user has moved on from is dropped.
#[derive(Clone)]
#[non_exhaustive]
pub struct FindUses {
    /// The `$id` of the schema of the documents the name is a key of
    /// (`None`: the document's own schema, which has no `$id`).
    pub schema: Option<String>,
    /// The JSON pointer of the map the name is a key of.
    pub pointer: String,
    /// The name.
    pub name: String,
    reply: UsesReply,
}

impl FindUses {
    pub(crate) fn new(
        schema: Option<String>,
        pointer: String,
        name: String,
        reply: UsesReply,
    ) -> Self {
        Self {
            schema,
            pointer,
            name,
            reply,
        }
    }

    /// Answers with the uses in the files the editor does not have open
    /// ([`tools::uses_at`](crate::tools::uses_at) finds them in a file's
    /// text). A use whose `source` is an open document of a documents view
    /// is left out: its live uses are listed already.
    pub fn reply(&self, uses: Vec<Use>) {
        (self.reply.0)(HostAnswer::Uses(uses));
    }

    /// Answers that the files could not be searched (the search failed):
    /// the list then says so, with the open documents' uses, and never
    /// that nothing uses the name.
    pub fn fail(&self) {
        (self.reply.0)(HostAnswer::Failed);
    }

    /// Answers that the host searches no files beyond the open ones (a
    /// binding whose host set no handler): an empty list then says "No
    /// uses in open files".
    pub(crate) fn decline(&self) {
        (self.reply.0)(HostAnswer::Declined);
    }
}

impl std::fmt::Debug for FindUses {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FindUses")
            .field("schema", &self.schema)
            .field("pointer", &self.pointer)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// What a host answered one request with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HostAnswer {
    /// The uses in its files.
    Uses(Vec<Use>),
    /// It searches no files beyond the open ones.
    Declined,
    /// Its search failed.
    Failed,
}

/// Where a host's answer to one request goes.
#[derive(Clone)]
pub(crate) struct UsesReply(pub(crate) Arc<dyn Fn(HostAnswer) + Send + Sync>);

/// The user chose a use the host listed (a file the editor does not have
/// open): what `on_go_to_use` hears, for the host to open it there.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct GoToUse {
    /// The file, as the host's [`Use`] named it.
    pub source: String,
    /// The field using the name.
    pub path: FieldPath,
    /// The name.
    pub name: String,
}

impl GoToUse {
    pub(crate) fn new(source: String, path: FieldPath, name: String) -> Self {
        Self { source, path, name }
    }
}
