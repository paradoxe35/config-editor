//! The public path and position types.

use super::super::language::SchemaPathSegment;

/// A place in a document's data: the keys and array indices leading to it
/// from the root, the same in every language (`/web/hosts/1` is the second
/// host whether the document is YAML, TOML or XML).
///
/// Build one from a JSON pointer (`"/web/hosts/1"`, `~1` for `/` and `~0` for
/// `~` in keys; the leading `/` may be left out) or from segments:
///
/// ```
/// use config_editor_leptos::FieldPath;
/// let path = FieldPath::root().key("web").key("hosts").index(1);
/// assert_eq!(path, FieldPath::from("/web/hosts/1"));
/// assert_eq!(path.to_pointer(), "/web/hosts/1");
/// assert_eq!(FieldPath::from("web/hosts"), FieldPath::from("/web/hosts"));
/// ```
///
/// A pointer cannot tell an array index from a key made of digits, so its
/// numeric segments read as indices; locating a path treats both alike.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FieldPath(Vec<SchemaPathSegment>);

impl FieldPath {
    /// The whole document.
    pub fn root() -> Self {
        FieldPath(Vec::new())
    }

    /// This path, then the key `key`.
    #[must_use]
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.0.push(SchemaPathSegment::Key(key.into()));
        self
    }

    /// This path, then the array item `index`.
    #[must_use]
    pub fn index(mut self, index: usize) -> Self {
        self.0.push(SchemaPathSegment::ArrayItem(index));
        self
    }

    /// The path read from a JSON pointer.
    pub fn from_pointer(pointer: &str) -> Self {
        let pointer = pointer.trim();
        if pointer.is_empty() || pointer == "/" {
            return FieldPath::root();
        }
        let pointer = pointer.strip_prefix('/').unwrap_or(pointer);
        FieldPath(
            pointer
                .split('/')
                .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
                .map(|segment| match segment.parse::<usize>() {
                    Ok(index) if !segment.starts_with('+') => SchemaPathSegment::ArrayItem(index),
                    _ => SchemaPathSegment::Key(segment),
                })
                .collect(),
        )
    }

    /// The path as a JSON pointer (`""` for the root).
    pub fn to_pointer(&self) -> String {
        self.0
            .iter()
            .map(|segment| match segment {
                SchemaPathSegment::Key(key) => {
                    format!("/{}", key.replace('~', "~0").replace('/', "~1"))
                }
                SchemaPathSegment::ArrayItem(index) => format!("/{index}"),
            })
            .collect()
    }

    /// The keys and indices, from the root.
    pub fn segments(&self) -> &[SchemaPathSegment] {
        &self.0
    }

    /// Whether this is the whole document.
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Display for FieldPath {
    /// The JSON pointer.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.to_pointer())
    }
}

impl From<&str> for FieldPath {
    fn from(pointer: &str) -> Self {
        FieldPath::from_pointer(pointer)
    }
}

impl From<String> for FieldPath {
    fn from(pointer: String) -> Self {
        FieldPath::from_pointer(&pointer)
    }
}

impl From<&String> for FieldPath {
    fn from(pointer: &String) -> Self {
        FieldPath::from_pointer(pointer)
    }
}

impl From<Vec<SchemaPathSegment>> for FieldPath {
    fn from(segments: Vec<SchemaPathSegment>) -> Self {
        FieldPath(segments)
    }
}

impl From<&FieldPath> for FieldPath {
    fn from(path: &FieldPath) -> Self {
        path.clone()
    }
}

impl std::str::FromStr for FieldPath {
    type Err = std::convert::Infallible;

    fn from_str(pointer: &str) -> Result<Self, Self::Err> {
        Ok(FieldPath::from_pointer(pointer))
    }
}

/// A place in the text: one-based `line`, and one-based character `column`
/// (as [`Diagnostic`](crate::Diagnostic) counts them).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextPosition {
    /// One-based line.
    pub line: usize,
    /// One-based character column.
    pub column: usize,
}

impl TextPosition {
    /// The position at `line`:`column` (both one-based).
    pub fn new(line: usize, column: usize) -> Self {
        TextPosition { line, column }
    }

    /// The position of UTF-16 offset `offset` in `text` (a JavaScript string
    /// index, as a textarea's `selectionStart`), clamped to the text.
    pub fn from_offset(text: &str, offset: usize) -> Self {
        let mut line = 1;
        let mut column = 1;
        let mut units = 0;
        for character in text.chars() {
            if units >= offset {
                break;
            }
            units += character.len_utf16();
            if character == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }
        TextPosition { line, column }
    }

    /// The UTF-16 offset of this position in `text`; a column past its line's
    /// end is the line's end, a line past the text the text's end.
    pub fn to_offset(self, text: &str) -> usize {
        super::super::commands::offset_at(text, self.line, self.column)
    }
}

/// A stretch of text from `start` up to (not including) `end`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextRange {
    /// Where the range starts.
    pub start: TextPosition,
    /// Just past where it ends.
    pub end: TextPosition,
}

impl TextRange {
    /// The range from `start` to `end`.
    pub fn new(start: TextPosition, end: TextPosition) -> Self {
        TextRange { start, end }
    }

    /// The text the range covers in `text`.
    pub fn slice(self, text: &str) -> String {
        let start = self.start.to_offset(text);
        let end = self.end.to_offset(text).max(start);
        String::from_utf16_lossy(&text.encode_utf16().collect::<Vec<_>>()[start..end])
    }
}
