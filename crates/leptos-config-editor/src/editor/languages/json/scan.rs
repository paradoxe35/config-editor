//! What surrounds the caret: the path, open elements, and pending names.

use super::*;

/// One step of a document path: an object key or an array index.
#[derive(Clone, Debug)]
pub(super) enum Seg {
    Key(String),
    Index(usize),
}

/// Where an object is in its `key: value` cycle.
#[derive(Clone, Debug)]
pub(super) enum Mode {
    /// Expecting a key (or `}`).
    Key,
    /// A key was read; expecting `:`.
    Colon(String),
    /// Expecting the value of this key.
    Value(String),
    /// A value was read; expecting `,` or the closing brace.
    After,
}

/// An open container. `slot` is the child currently being read, if any.
#[derive(Clone, Debug)]
pub(super) enum Frame {
    Object {
        id: usize,
        mode: Mode,
        slot: Option<Seg>,
    },
    Array {
        id: usize,
        index: usize,
        awaiting: bool,
        slot: Option<Seg>,
    },
}

impl Frame {
    pub(super) fn id(&self) -> usize {
        match self {
            Frame::Object { id, .. } | Frame::Array { id, .. } => *id,
        }
    }

    pub(super) fn slot(&self) -> Option<Seg> {
        match self {
            Frame::Object { slot, .. } | Frame::Array { slot, .. } => slot.clone(),
        }
    }
}

/// The parser state at the caret.
pub(super) struct Snapshot {
    /// Path of the container holding the caret.
    pub(super) path: Vec<Seg>,
    /// The innermost open container, if the caret is inside one.
    pub(super) frame: Option<Frame>,
    /// The string or word being typed under the caret, if any.
    pub(super) partial: Option<Token>,
}

#[derive(Default)]
pub(super) struct Scan {
    pub(super) snapshot: Option<Snapshot>,
    /// Completed object keys as `(owner frame id, key, start byte)`. The key
    /// being typed at the caret is excluded.
    pub(super) keys: Vec<(usize, String, usize)>,
    /// Byte spans of every key, keyed by its document path.
    pub(super) key_spans: Vec<(Vec<String>, usize, usize)>,
    /// Byte spans of every value, keyed by its document path.
    pub(super) value_spans: Vec<(Vec<String>, usize, usize)>,
}

pub(super) fn with_segment(mut path: Vec<Seg>, segment: Seg) -> Vec<Seg> {
    path.push(segment);
    path
}

pub(super) fn strings(path: &[Seg]) -> Vec<String> {
    path.iter()
        .map(|segment| match segment {
            Seg::Key(key) => key.clone(),
            Seg::Index(index) => index.to_string(),
        })
        .collect()
}

/// Path of the container that holds the innermost open frame.
pub(super) fn container_path(frames: &[Frame]) -> Vec<Seg> {
    frames.split_last().map_or_else(Vec::new, |(_, parents)| {
        parents.iter().filter_map(Frame::slot).collect()
    })
}

/// Records a value starting at `start..end` in the innermost frame and moves that
/// frame past it. Scalars and containers both come through here.
pub(super) fn take_value(frames: &mut [Frame], scan: &mut Scan, start: usize, end: usize) {
    let parent = container_path(frames);
    match frames.last_mut() {
        None => scan.value_spans.push((Vec::new(), start, end)),
        Some(Frame::Object { mode, slot, .. }) => {
            if let Mode::Value(key) = mode.clone() {
                let segment = Seg::Key(key);
                scan.value_spans.push((
                    strings(&with_segment(parent, segment.clone())),
                    start,
                    end,
                ));
                *slot = Some(segment);
                *mode = Mode::After;
            }
        }
        Some(Frame::Array {
            index,
            awaiting,
            slot,
            ..
        }) => {
            if *awaiting {
                let segment = Seg::Index(*index);
                scan.value_spans.push((
                    strings(&with_segment(parent, segment.clone())),
                    start,
                    end,
                ));
                *slot = Some(segment);
                *awaiting = false;
            }
        }
    }
}

/// Walks the whole document, tolerating invalid syntax, and captures the parser
/// state at `cursor` (a byte offset).
pub(super) fn analyze(source: &str, cursor: usize) -> Scan {
    let tokens = tokenize(source);
    let partial_index = tokens.iter().position(|token| {
        token.start < cursor
            && cursor <= token.end
            && matches!(token.kind, Kind::Str { .. } | Kind::Word)
    });
    let mut frames: Vec<Frame> = Vec::new();
    let mut next_id = 0usize;
    let mut scan = Scan::default();

    for (index, token) in tokens.iter().enumerate() {
        let is_partial = partial_index == Some(index);
        if scan.snapshot.is_none() && (is_partial || token.start >= cursor) {
            scan.snapshot = Some(Snapshot {
                path: container_path(&frames),
                frame: frames.last().cloned(),
                partial: is_partial.then_some(*token),
            });
        }
        match token.kind {
            Kind::Str { closed } => {
                let name = decode_string(&source[token.start..token.end], closed);
                let in_key_slot = matches!(
                    frames.last(),
                    Some(Frame::Object {
                        mode: Mode::Key,
                        ..
                    })
                );
                if in_key_slot {
                    let path = with_segment(container_path(&frames), Seg::Key(name.clone()));
                    scan.key_spans
                        .push((strings(&path), token.start, token.end));
                    if !is_partial && let Some(frame) = frames.last() {
                        scan.keys.push((frame.id(), name.clone(), token.start));
                    }
                    if let Some(Frame::Object { mode, .. }) = frames.last_mut() {
                        *mode = Mode::Colon(name);
                    }
                } else {
                    take_value(&mut frames, &mut scan, token.start, token.end);
                }
            }
            Kind::Word => take_value(&mut frames, &mut scan, token.start, token.end),
            Kind::Punct(character @ ('{' | '[')) => {
                take_value(&mut frames, &mut scan, token.start, token.start + 1);
                next_id += 1;
                frames.push(if character == '{' {
                    Frame::Object {
                        id: next_id,
                        mode: Mode::Key,
                        slot: None,
                    }
                } else {
                    Frame::Array {
                        id: next_id,
                        index: 0,
                        awaiting: true,
                        slot: None,
                    }
                });
            }
            Kind::Punct('}' | ']') => {
                frames.pop();
            }
            Kind::Punct(':') => {
                if let Some(Frame::Object { mode, .. }) = frames.last_mut()
                    && let Mode::Colon(name) = mode.clone()
                {
                    *mode = Mode::Value(name);
                }
            }
            Kind::Punct(',') => match frames.last_mut() {
                Some(Frame::Object { mode, slot, .. }) => {
                    *mode = Mode::Key;
                    *slot = None;
                }
                Some(Frame::Array {
                    index,
                    awaiting,
                    slot,
                    ..
                }) => {
                    *index += 1;
                    *awaiting = true;
                    *slot = None;
                }
                None => {}
            },
            Kind::Punct(_) => {}
        }
    }

    if scan.snapshot.is_none() {
        scan.snapshot = Some(Snapshot {
            path: container_path(&frames),
            frame: frames.last().cloned(),
            partial: None,
        });
    }
    scan
}

pub(super) fn utf16(text: &str) -> usize {
    text.encode_utf16().count()
}

/// One-based line and character column of a byte offset.
pub(super) fn line_column(source: &str, byte: usize) -> (usize, usize) {
    let before = &source[..byte];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    (line, before[line_start..].chars().count() + 1)
}

pub(super) fn json_text(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}
