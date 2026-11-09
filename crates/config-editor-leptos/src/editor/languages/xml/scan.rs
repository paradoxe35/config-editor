//! What surrounds the caret: the path, open elements, and pending names.

use super::*;

/// One step of an element path: a child element or one repeated sibling.
#[derive(Clone, Debug)]
pub(super) enum Seg {
    Key(String),
    Index(usize),
}

/// An open element: its tag name and where its start tag starts (its
/// document path is looked up when needed; the wrapper root's is empty, so
/// matching its end tag needs the name).
#[derive(Clone, Debug)]
pub(super) struct Frame {
    pub(super) name: String,
    pub(super) start: usize,
}

impl Frame {
    /// Its document path in `plan`.
    pub(super) fn path<'a>(&self, plan: &'a PathPlan) -> &'a [Seg] {
        plan.path_of(self.start).unwrap_or_default()
    }
}

#[derive(Clone, Debug)]
pub(super) struct Snapshot {
    pub(super) path: Vec<Seg>,
    pub(super) partial: Option<Token>,
    /// Whether the caret sits inside a tag (`<...>`) rather than in text.
    pub(super) in_tag: bool,
    pub(super) open_tag: Option<Token>,
    /// Document path the open tag will get (empty for the wrapper root).
    pub(super) open_tag_path: Vec<Seg>,
    pub(super) inner: Vec<Token>,
    /// Set at `name = |`.
    pub(super) pending_attr: Option<String>,
    /// The caret is in the config root's start tag, which holds only metadata.
    pub(super) in_root_tag: bool,
    /// The open tag's attributes a child element shares a name with, read
    /// `@name`.
    pub(super) spelled_apart: Vec<String>,
}

#[derive(Default)]
pub(super) struct Scan {
    pub(super) snapshot: Option<Snapshot>,
    /// Byte spans of element and attribute names, keyed by document path.
    /// Repeated siblings address their array index.
    pub(super) key_spans: Vec<(Vec<String>, usize, usize)>,
    /// Byte spans of element text and attribute values, keyed the same way.
    pub(super) value_spans: Vec<(Vec<String>, usize, usize)>,
}

/// Marks the transparent config root; never part of the parsed value.
pub(super) const CONFIG_ROOT_ATTRIBUTE: &str = "config-root";

pub(super) fn marks_config_root(source: &str, token: &Token) -> bool {
    let inner = tokenize_tag_inside(source, token);
    inner.windows(3).any(|window| {
        matches!(window[0].kind, Kind::Text { quoted: false })
            && &source[window[0].start..window[0].end] == CONFIG_ROOT_ATTRIBUTE
            && matches!(window[1].kind, Kind::Equals)
            && matches!(window[2].kind, Kind::Text { quoted: true })
            && source[window[2].start..window[2].end].trim_matches(['"', '\'']) == "true"
    })
}

pub(super) fn strings(path: &[Seg]) -> Vec<String> {
    path.iter()
        .map(|segment| match segment {
            Seg::Key(key) => key.clone(),
            Seg::Index(index) => index.to_string(),
        })
        .collect()
}

/// A text's hash and length, a cursor, whether the spans were recorded,
/// and the scan there.
type RememberedScan = (u64, usize, usize, bool, std::rc::Rc<Scan>);

thread_local! {
    /// The last scan: one keystroke's suggestions, the check of their
    /// read-only fields and the keys already written each read the same
    /// text at the same place.
    static LAST_SCAN: std::cell::RefCell<Option<RememberedScan>> = const { std::cell::RefCell::new(None) };
}

/// [`scan_text`] of `source` at `cursor`, remembered for the last text and
/// cursor.
pub(super) fn analyze(source: &str, cursor: usize) -> std::rc::Rc<Scan> {
    remembered(source, cursor, true)
}

/// [`analyze`] for the caret's place alone: no spans of names and values
/// (the caret's path and its suggestions' context, on every keystroke:
/// recording every element's spans took most of one in a long document).
pub(super) fn analyze_caret(source: &str, cursor: usize) -> std::rc::Rc<Scan> {
    remembered(source, cursor, false)
}

fn remembered(source: &str, cursor: usize, spans: bool) -> std::rc::Rc<Scan> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    let key = (hasher.finish(), source.len(), cursor);
    let remembered = LAST_SCAN.with(|last| {
        last.borrow()
            .as_ref()
            // A scan with its spans serves one without.
            .filter(|(hash, len, at, with_spans, _)| (*hash, *len, *at) == key && (*with_spans || !spans))
            .map(|(.., scan)| scan.clone())
    });
    if let Some(scan) = remembered {
        return scan;
    }
    let scan = std::rc::Rc::new(scan_text(source, cursor, spans));
    LAST_SCAN.with(|last| *last.borrow_mut() = Some((key.0, key.1, key.2, spans, scan.clone())));
    scan
}

/// Walks the whole document, tolerating invalid syntax, and captures the parser
/// state at `cursor` (a byte offset).
pub(super) fn scan_text(source: &str, cursor: usize, spans: bool) -> Scan {
    if spans {
        crate::editor::work_count::note(crate::editor::work_count::Work::SpanScan);
    }
    let tokens = tokenize(source);
    let plan = plan_paths(source, &tokens);
    let partial_index = tokens.iter().position(|token| {
        contains_caret(source, token, cursor)
            && !matches!(token.kind, Kind::Special | Kind::Comment)
    });
    let mut frames: Vec<Frame> = Vec::new();
    let mut scan = Scan::default();

    // Records a `<name>` start tag: its name span and the new open frame
    // (`empty` skips the frame for `<name/>`). Its path is looked up only
    // for its span: the caret's alone needs no other.
    let open_element =
        |scan: &mut Scan, frames: &mut Vec<Frame>, name: &str, tag: &Token, empty: bool| {
            if spans
                && let Some(path) = plan.path_of(tag.start).filter(|path| !path.is_empty())
            {
                scan.key_spans.push((
                    strings(path),
                    local_name_start(source, tag),
                    local_name_start(source, tag) + name_len(source, tag),
                ));
            }
            if !empty {
                frames.push(Frame {
                    name: name.to_string(),
                    start: tag.start,
                });
            }
        };

    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        let is_partial = partial_index == Some(index);
        if scan.snapshot.is_none() && (is_partial || token.start >= cursor) {
            scan.snapshot = Some(capture_snapshot(source, &tokens, &frames, &plan, cursor));
        }
        match token.kind {
            Kind::OpenTag { .. } => {
                let self_closing = opens_nothing(source, &token);
                let name = token_name(source, &token);
                // A bare `<` (or `< >`) opens nothing; its attributes, if any,
                // still belong to no element.
                if !name.is_empty() {
                    let metadata = plan.root == Some(token.start);
                    let attributes = if spans && !metadata {
                        tokenize_tag_inside(source, &token)
                    } else {
                        Vec::new()
                    };
                    let path = if attributes.is_empty() {
                        &[][..]
                    } else {
                        plan.path_of(token.start).unwrap_or_default()
                    };
                    for inner in attributes {
                        if let Kind::Text { quoted: false } = inner.kind
                            && is_attr_name(source, &token, inner)
                            && !is_partial
                        {
                            scan.key_spans.push((
                                attribute_key(path, &plan.attribute_key(token.start, &source[inner.start..inner.end])),
                                inner.start,
                                inner.end,
                            ));
                        }
                    }
                    open_element(&mut scan, &mut frames, &name, &token, self_closing);
                }
            }
            Kind::EndTag => {
                let name = token_name(source, &token);
                if frames.last().is_some_and(|frame| frame.name == name) {
                    frames.pop();
                } else if let Some(position) = frames.iter().rposition(|frame| frame.name == name) {
                    frames.truncate(position);
                } else {
                    // A misspelled end tag closes the innermost element, as
                    // in the path plan.
                    frames.pop();
                }
            }
            // A CDATA section is element text, its markers included.
            Kind::Text { quoted: false } | Kind::Special
                if spans
                    && !is_partial
                    && (token.kind != Kind::Special || source[token.start..].starts_with("<![CDATA[")) =>
            {
                let raw = &source[token.start..token.end];
                let text = raw.trim();
                if !text.is_empty()
                    && let Some(frame) = frames.last()
                    && !frame.path(&plan).is_empty()
                {
                    let path = strings(frame.path(&plan));
                    let start = token.start + (raw.len() - raw.trim_start().len());
                    let end = start + text.len();
                    // Text over several lines, or around comments and
                    // processing instructions, is one value: its span runs
                    // from its first piece to its last.
                    match scan.value_spans.last_mut() {
                        Some((last, _, last_end)) if *last == path && only_comments(&source[*last_end..start]) => {
                            *last_end = end;
                        }
                        _ => scan.value_spans.push((path, start, end)),
                    }
                }
            }
            // Quoted values are recorded by `record_attribute_values`.
            Kind::Text { .. } | Kind::Comment | Kind::Special | Kind::Equals => {}
        }
        index += 1;
    }

    if spans {
        record_attribute_values(source, &tokens, &plan, &mut scan);
    }

    if scan.snapshot.is_none() {
        scan.snapshot = Some(capture_snapshot(source, &tokens, &frames, &plan, cursor));
    }
    scan
}

/// Whether `gap` holds only whitespace, comments and processing
/// instructions.
pub(super) fn only_comments(gap: &str) -> bool {
    let mut rest = gap.trim_start();
    while !rest.is_empty() {
        let Some((open, close)) = [("<!--", "-->"), ("<?", "?>")].into_iter().find(|(open, _)| rest.starts_with(open)) else {
            return false;
        };
        let Some(end) = rest[open.len()..].find(close) else {
            return false;
        };
        rest = rest[open.len() + end + close.len()..].trim_start();
    }
    true
}

/// Byte length of the tag name: `<prefix:name` counts only after the colon.
pub(super) fn name_len(source: &str, tag: &Token) -> usize {
    let name_end = match tag.kind {
        Kind::OpenTag { name_end, .. } => name_end,
        _ => tag.end,
    };
    let name_start = tag.start + 1;
    let full = &source[name_start..name_end.min(source.len())];
    full.rfind(':')
        .map_or(full.len(), |colon| full.len() - colon - 1)
}

/// Attributes are ordinary properties of their element (`name` is the
/// attribute's key, [`PathPlan::attribute_key`]).
pub(super) fn attribute_key(path: &[Seg], name: &str) -> Vec<String> {
    let mut key = strings(path);
    key.push(name.to_string());
    key
}

/// Whether `inner` is an attribute name: an unquoted word directly inside an
/// open tag that is followed by `=`.
pub(super) fn is_attr_name(source: &str, open: &Token, inner: Token) -> bool {
    let inner_tokens = tokenize_tag_inside(source, open);
    inner_tokens
        .iter()
        .position(|token| token.start == inner.start && token.end == inner.end)
        .and_then(|position| inner_tokens.get(position + 1))
        .is_some_and(|next| matches!(next.kind, Kind::Equals))
}

/// Whether `token` holds the caret. A tag holds it strictly inside its
/// `<...>`, or at its end while still unclosed; a caret right after a tag's
/// closing `>` is in the text that follows.
pub(super) fn contains_caret(source: &str, token: &Token, cursor: usize) -> bool {
    if !(token.start < cursor && cursor <= token.end) {
        return false;
    }
    match token.kind {
        Kind::OpenTag { .. } | Kind::EndTag => {
            cursor < token.end || !source[..token.end].ends_with('>')
        }
        _ => true,
    }
}

/// Captures the caret state: the holding element, the token under the caret,
/// and the open tag around the caret when it sits inside one.
pub(super) fn capture_snapshot(
    source: &str,
    tokens: &[Token],
    frames: &[Frame],
    plan: &PathPlan,
    cursor: usize,
) -> Snapshot {
    let partial = tokens
        .iter()
        .find(|token| {
            contains_caret(source, token, cursor)
                && !matches!(token.kind, Kind::Special | Kind::Comment)
        })
        .copied();
    let open_tag = partial.filter(|token| matches!(token.kind, Kind::OpenTag { .. }));
    let inner = open_tag.map_or_else(Vec::new, |tag| tokenize_tag_inside(source, &tag));
    let pending_attr = open_tag.and_then(|_| pending_attribute(source, &inner, cursor));
    let path = frames
        .last()
        .map_or_else(Vec::new, |frame| frame.path(plan).to_vec());
    let open_tag_path = open_tag
        .and_then(|tag| plan.path_of(tag.start))
        .map_or_else(
            || {
                let mut fallback = path.clone();
                if let Some(tag) = open_tag {
                    fallback.push(Seg::Key(token_name(source, &tag)));
                }
                fallback
            },
            <[Seg]>::to_vec,
        );
    let spelled_apart = open_tag.map_or_else(Vec::new, |tag| {
        inner
            .iter()
            .filter(|token| matches!(token.kind, Kind::Text { quoted: false }) && is_attr_name(source, &tag, **token))
            .map(|token| source[token.start..token.end].to_string())
            .filter(|name| plan.attribute_key(tag.start, name).starts_with('@'))
            .collect()
    });
    Snapshot {
        path,
        partial,
        in_tag: open_tag.is_some(),
        open_tag,
        open_tag_path,
        inner,
        pending_attr,
        in_root_tag: open_tag.is_some_and(|tag| plan.root == Some(tag.start)),
        spelled_apart,
    }
}

impl Snapshot {
    /// The key of the open tag's attribute `name`: `@name` where a child
    /// element has its name ([`PathPlan::attribute_key`]).
    pub(super) fn attribute_key(&self, name: &str) -> String {
        if self.spelled_apart.iter().any(|apart| apart == name) { format!("@{name}") } else { name.to_string() }
    }
}

/// The attribute name awaiting a value when the caret follows `name =` with
/// only blanks between (`name = |`).
pub(super) fn pending_attribute(source: &str, inner: &[Token], cursor: usize) -> Option<String> {
    let equals = inner
        .iter()
        .rposition(|token| matches!(token.kind, Kind::Equals) && token.start < cursor)?;
    if !source[inner[equals].end..cursor].trim().is_empty() {
        return None;
    }
    let name = inner.get(equals.checked_sub(1)?)?;
    matches!(name.kind, Kind::Text { quoted: false })
        .then(|| source[name.start..name.end].to_string())
}

/// Records value spans by pairing `name = "value"` in every open tag.
pub(super) fn record_attribute_values(source: &str, tokens: &[Token], plan: &PathPlan, scan: &mut Scan) {
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        if !matches!(token.kind, Kind::OpenTag { .. }) {
            index += 1;
            continue;
        }
        let Some(element_path) = plan
            .path_of(token.start)
            .filter(|_| plan.root != Some(token.start))
        else {
            index += 1;
            continue;
        };
        let element_path = element_path.to_vec();
        let inner = tokenize_tag_inside(source, &token);
        let mut position = 0;
        while position < inner.len() {
            let name_token = inner[position];
            let is_name = matches!(name_token.kind, Kind::Text { quoted: false })
                && inner
                    .get(position + 1)
                    .is_some_and(|next| matches!(next.kind, Kind::Equals));
            if !is_name {
                position += 1;
                continue;
            }
            let name = source[name_token.start..name_token.end].to_string();
            let value_token = inner
                .get(position + 2)
                .filter(|next| matches!(next.kind, Kind::Text { .. }));
            if let Some(value_token) = value_token {
                scan.value_spans.push((
                    attribute_key(&element_path, &plan.attribute_key(token.start, &name)),
                    value_token.start,
                    value_token.end,
                ));
            }
            position += 1;
        }
        index += 1;
    }
}

pub(super) fn utf16(text: &str) -> usize {
    text.encode_utf16().count()
}

/// One-based line and character column of a byte offset.
pub(super) fn line_column(source: &str, byte: usize) -> (usize, usize) {
    crate::editor::paths::line_column(source, byte)
}

pub(super) fn xml_text(value: &Value) -> String {
    match value {
        Value::String(value) => super::super::super::codec::non_finite::written(value).to_string(),
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

/// Whether a start tag holds no children: `<name/>`, or a tag still being
/// typed that the next tag cut short (`<t` then `<tls>` on the next line).
pub(super) fn opens_nothing(source: &str, tag: &Token) -> bool {
    let text = &source[..tag.end];
    text.ends_with("/>") || (!text.ends_with('>') && tag.end < source.len())
}
