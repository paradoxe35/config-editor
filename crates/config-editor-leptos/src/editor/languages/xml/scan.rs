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
    /// Where the start tag of the element holding the caret starts.
    pub(super) holder: Option<usize>,
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

/// Forgets the last scan.
pub(super) fn forget_scan() {
    let _ = LAST_SCAN.try_with(|last| last.borrow_mut().take());
}

#[cfg(test)]
pub(super) fn scan_kept() -> bool {
    LAST_SCAN.with(|last| last.borrow().is_some())
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
    crate::editor::paths::walked_from_top(source);
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

    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        let is_partial = partial_index == Some(index);
        if scan.snapshot.is_none() && (is_partial || token.start >= cursor) {
            let partial = partial_index.map(|index| tokens[index]);
            scan.snapshot = Some(capture_snapshot(source, partial, &frames, &plan, cursor));
        }
        match token.kind {
            Kind::OpenTag { .. } => {
                if spans {
                    record_tag_keys(source, &token, is_partial, &plan, &mut scan.key_spans);
                }
                let name = token_name(source, &token);
                // A bare `<` (or `< >`) opens nothing.
                if !name.is_empty() && !opens_nothing(source, &token) {
                    open_frame(&mut frames, &name, &token);
                }
            }
            Kind::EndTag => close_frame(&mut frames, &token_name(source, &token)),
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
        let partial = partial_index.map(|index| tokens[index]);
        scan.snapshot = Some(capture_snapshot(source, partial, &frames, &plan, cursor));
    }
    scan
}

/// Records the key spans of the start tag `tag`: its attributes' names
/// (unless it is the config root's, or `partial`, the tag being typed) and
/// its name. Its path is looked up only for its spans: the caret's alone
/// needs no other.
pub(super) fn record_tag_keys(
    source: &str,
    tag: &Token,
    partial: bool,
    plan: &PathPlan,
    key_spans: &mut Vec<(Vec<String>, usize, usize)>,
) {
    let name = token_name(source, tag);
    // A bare `<` (or `< >`) opens nothing; its attributes, if any, still
    // belong to no element.
    if name.is_empty() {
        return;
    }
    let attributes = if plan.root == Some(tag.start) {
        Vec::new()
    } else {
        tokenize_tag_inside(source, tag)
    };
    let path = if attributes.is_empty() {
        &[][..]
    } else {
        plan.path_of(tag.start).unwrap_or_default()
    };
    for inner in attributes {
        if let Kind::Text { quoted: false } = inner.kind
            && is_attr_name(source, tag, inner)
            && !partial
            && !plan.markup.holds(&source[inner.start..inner.end])
        {
            key_spans.push((
                attribute_key(path, &plan.attribute_key(tag.start, &source[inner.start..inner.end])),
                inner.start,
                inner.end,
            ));
        }
    }
    if let Some(path) = plan.path_of(tag.start).filter(|path| !path.is_empty()) {
        key_spans.push((
            strings(path),
            local_name_start(source, tag),
            local_name_start(source, tag) + name_len(source, tag),
        ));
    }
}

/// Key spans (see [`Scan::key_spans`]): some, or the whole scan's.
pub(super) enum KeySpans {
    Some(Vec<(Vec<String>, usize, usize)>),
    All(std::rc::Rc<Scan>),
}

impl std::ops::Deref for KeySpans {
    type Target = [(Vec<String>, usize, usize)];

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Some(spans) => spans,
            Self::All(scan) => &scan.key_spans,
        }
    }
}

/// The key spans [`analyze`] records at byte `cursor`, at least all whose
/// path starts with `path`: read from the element with that path among
/// those starting at `starts` (see [`PathPlan::subtree`]) where they lie
/// only there, not off the whole document.
pub(super) fn key_spans_under(
    source: &str,
    cursor: usize,
    path: &[String],
    starts: impl IntoIterator<Item = usize>,
) -> KeySpans {
    some_key_spans(source, cursor, path, starts)
        .map_or_else(|| KeySpans::All(analyze(source, cursor)), KeySpans::Some)
}

fn some_key_spans(
    source: &str,
    cursor: usize,
    path: &[String],
    starts: impl IntoIterator<Item = usize>,
) -> Option<Vec<(Vec<String>, usize, usize)>> {
    let walked = walked(source);
    let read;
    let tokens: &[Token] = match &walked {
        Some(walked) => &walked.tokens,
        None => {
            read = tokenize(source);
            &read
        }
    };
    let plan = plan_paths(source, tokens);
    let (start, after) = plan.subtree(path, starts)?;
    let first = tokens.partition_point(|token| token.start < start);
    let end = after.map_or(tokens.len(), |after| tokens.partition_point(|token| token.start < after));
    let partial = holding(source, tokens, cursor);
    let mut key_spans = Vec::new();
    for (index, token) in tokens.iter().enumerate().take(end).skip(first) {
        if matches!(token.kind, Kind::OpenTag { .. }) {
            record_tag_keys(source, token, partial == Some(index), &plan, &mut key_spans);
        }
    }
    Some(key_spans)
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

/// Opens the element `name` whose start tag is `tag`.
fn open_frame(frames: &mut Vec<Frame>, name: &str, tag: &Token) {
    frames.push(Frame {
        name: name.to_string(),
        start: tag.start,
    });
}

/// Closes the element an end tag named `name` closes.
fn close_frame(frames: &mut Vec<Frame>, name: &str) {
    if frames.last().is_some_and(|frame| frame.name == name) {
        frames.pop();
    } else if let Some(position) = frames.iter().rposition(|frame| frame.name == name) {
        frames.truncate(position);
    } else {
        // A misspelled end tag closes the innermost element, as in the
        // path plan.
        frames.pop();
    }
}

/// Walks past `token` as the caret's walk does: the elements it opens or
/// closes.
fn walk_frames(source: &str, token: &Token, frames: &mut Vec<Frame>) {
    match token.kind {
        Kind::OpenTag { .. } => {
            let name = token_name(source, token);
            if !name.is_empty() && !opens_nothing(source, token) {
                open_frame(frames, &name, token);
            }
        }
        Kind::EndTag => close_frame(frames, &token_name(source, token)),
        _ => {}
    }
}

/// A document's tokens, its path plan and the caret's walk over them, read
/// once while a batch runs for the text (see `paths::paths_at`).
pub(super) struct Walked {
    pub(super) tokens: Vec<Token>,
    pub(super) plan: std::rc::Rc<PathPlan>,
    kept: crate::editor::paths::Checkpoints<Vec<Frame>>,
}

impl Walked {
    /// The start tag the end tag `tokens[end]` closes, as the path plan
    /// matches them: the innermost open element of its name, from the
    /// walk kept for the text.
    pub(super) fn opening_of(&self, source: &str, end: usize) -> Option<&Token> {
        let tokens = &self.tokens;
        let frames = self
            .kept
            .before(end, |frames, index| walk_frames(source, &tokens[index], frames));
        let name = token_name(source, &tokens[end]);
        let start = frames.iter().rev().find(|frame| frame.name == name)?.start;
        let index = tokens.partition_point(|token| token.start < start);
        tokens.get(index).filter(|token| token.start == start)
    }
}

/// The tokens and path plan of `source`, read once while a batch runs for
/// it; `None` outside one.
pub(super) fn walked(source: &str) -> Option<std::rc::Rc<Walked>> {
    if !crate::editor::paths::in_batch(source) {
        return None;
    }
    Some(crate::editor::paths::batch_cached(source, || {
        crate::editor::paths::walked_from_top(source);
        let tokens = tokenize(source);
        let plan = plan_paths(source, &tokens);
        let kept = crate::editor::paths::Checkpoints::walk(Vec::new(), tokens.len(), |frames, at| {
            walk_frames(source, &tokens[at], frames)
        });
        Walked { tokens, plan, kept }
    }))
}

/// The index of the token holding the caret at byte `cursor` (not a comment
/// or processing instruction), found by halving `tokens`.
pub(super) fn holding(source: &str, tokens: &[Token], cursor: usize) -> Option<usize> {
    let next = tokens.partition_point(|token| token.start < cursor);
    next.checked_sub(1).filter(|before| {
        let token = &tokens[*before];
        contains_caret(source, token, cursor) && !matches!(token.kind, Kind::Special | Kind::Comment)
    })
}

/// The caret's place at byte `cursor` of `source`: what [`analyze_caret`]
/// captures. While a batch runs for the text, read from the walk kept for
/// it, not a walk from the top.
pub(super) fn caret_snapshot(source: &str, cursor: usize) -> Snapshot {
    let Some(walked) = walked(source) else {
        return analyze_caret(source, cursor)
            .snapshot
            .clone()
            .unwrap_or_else(|| unreachable!("analyze always captures a snapshot"));
    };
    let tokens = &walked.tokens;
    let partial = holding(source, tokens, cursor);
    let at = partial.unwrap_or_else(|| tokens.partition_point(|token| token.start < cursor));
    let frames = walked
        .kept
        .before(at, |frames, index| walk_frames(source, &tokens[index], frames));
    capture_snapshot(source, partial.map(|index| tokens[index]), &frames, &walked.plan, cursor)
}

/// Captures the caret state: the holding element, the token under the caret,
/// and the open tag around the caret when it sits inside one.
pub(super) fn capture_snapshot(
    source: &str,
    partial: Option<Token>,
    frames: &[Frame],
    plan: &PathPlan,
    cursor: usize,
) -> Snapshot {
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
        holder: frames.last().map(|frame| frame.start),
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
            if plan.markup.holds(&name) {
                position += 1;
                continue;
            }
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
