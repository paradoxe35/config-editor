//! What surrounds the caret: the path, open elements, and pending names.

use super::*;

/// One step of an element path: a child element or one repeated sibling.
#[derive(Clone, Debug)]
pub(super) enum Seg {
    Key(String),
    Index(usize),
}

/// An open element: its tag name and document path (the wrapper root's path is
/// empty, so matching its end tag needs the name).
#[derive(Clone, Debug)]
pub(super) struct Frame {
    pub(super) name: String,
    pub(super) path: Vec<Seg>,
}

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

/// Document paths of every element, keyed by the byte offset of its start tag.
/// The config root's path is empty, so every recorded span is already relative
/// to the schema root.
pub(super) struct PathPlan {
    pub(super) paths: Vec<(usize, Vec<Seg>)>,
    /// Start of the config root's start tag (`<any config-root="true">`), if
    /// the document has one. Its attributes are editor metadata.
    pub(super) root: Option<usize>,
}

impl PathPlan {
    pub(super) fn path_of(&self, start: usize) -> Option<&[Seg]> {
        // In document order: found by halving, not by a walk per lookup.
        self.paths
            .binary_search_by_key(&start, |(tag_start, _)| *tag_start)
            .ok()
            .map(|index| self.paths[index].1.as_slice())
    }
}

/// An element seen while planning: enough structure to find the config root
/// and to number repeated siblings.
pub(super) struct PlannedNode {
    pub(super) name: String,
    pub(super) start: usize,
    pub(super) parent: Option<usize>,
    pub(super) config_root: bool,
}

/// Computes document paths for every element before any span is recorded, so
/// repeated siblings can be numbered consistently with the arrays `build_value`
/// folds them into, and so the config root can be skipped.
/// Elements deeper than this get no path (the parser refuses past 128).
const MAX_PLANNED_DEPTH: usize = 256;

/// [`plan_text`] of `source` (whose tokens are `tokens`), remembered for
/// the last text.
pub(super) fn plan_paths(source: &str, tokens: &[Token]) -> std::rc::Rc<PathPlan> {
    use std::hash::{Hash, Hasher};
    thread_local! {
        static LAST: std::cell::RefCell<Option<(u64, usize, std::rc::Rc<PathPlan>)>> = const { std::cell::RefCell::new(None) };
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    let key = (hasher.finish(), source.len());
    if let Some(plan) = LAST.with(|last| {
        last.borrow().as_ref().filter(|(hash, len, _)| (*hash, *len) == key).map(|(_, _, plan)| plan.clone())
    }) {
        return plan;
    }
    let plan = std::rc::Rc::new(plan_text(source, tokens));
    LAST.with(|last| *last.borrow_mut() = Some((key.0, key.1, plan.clone())));
    plan
}

pub(super) fn plan_text(source: &str, tokens: &[Token]) -> PathPlan {
    let mut nodes: Vec<PlannedNode> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    for token in tokens {
        match token.kind {
            Kind::OpenTag { .. } => {
                let name = token_name(source, token);
                if name.is_empty() {
                    continue;
                }
                let self_closing = opens_nothing(source, token);
                let id = nodes.len();
                nodes.push(PlannedNode {
                    name,
                    start: token.start,
                    parent: stack.last().copied(),
                    config_root: stack.is_empty() && marks_config_root(source, token),
                });
                if !self_closing {
                    stack.push(id);
                }
            }
            Kind::EndTag => {
                let name = token_name(source, token);
                if stack.last().is_some_and(|id| nodes[*id].name == name) {
                    stack.pop();
                } else if let Some(position) = stack.iter().rposition(|id| nodes[*id].name == name)
                {
                    stack.truncate(position);
                } else {
                    // An end tag matching no open element (`<listen>1</listn>`):
                    // a misspelled end of the innermost one, which it closes,
                    // so the elements after it are not all nested inside it.
                    stack.pop();
                }
            }
            _ => {}
        }
    }

    // Several marked roots are a parse error; the tolerant scan keeps the first.
    let wrapper = nodes.iter().position(|node| node.config_root);

    let mut multiplicity: HashMap<(Option<usize>, String), usize> = HashMap::new();
    for (id, node) in nodes.iter().enumerate() {
        if Some(id) == wrapper {
            continue;
        }
        *multiplicity
            .entry((node.parent, node.name.clone()))
            .or_insert(0) += 1;
    }
    let mut ordinal: HashMap<(Option<usize>, String), usize> = HashMap::new();
    let mut paths = vec![Vec::new(); nodes.len()];
    let mut paths_depth = vec![0usize; nodes.len()];
    for id in 0..nodes.len() {
        if Some(id) == wrapper {
            continue;
        }
        let node = &nodes[id];
        // Deeper than the parser reads (a syntax error anyway), no path:
        // copying every ancestor's path would grow with the square of depth.
        let depth = node.parent.map_or(0, |parent| paths_depth[parent] + 1);
        paths_depth[id] = depth;
        if depth > MAX_PLANNED_DEPTH {
            continue;
        }
        // Elements are created parent-first, so the parent's path is ready.
        let mut path = node
            .parent
            .map_or_else(Vec::new, |parent| paths[parent].clone());
        path.push(Seg::Key(node.name.clone()));
        let sibling = (node.parent, node.name.clone());
        let index = ordinal.entry(sibling.clone()).or_insert(0);
        let occurrence = *index;
        *index += 1;
        if multiplicity.get(&sibling).copied().unwrap_or(0) > 1 {
            path.push(Seg::Index(occurrence));
        }
        paths[id] = path;
    }
    PathPlan {
        paths: nodes.iter().map(|node| node.start).zip(paths).collect(),
        root: wrapper.map(|id| nodes[id].start),
    }
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

/// Walks the whole document, tolerating invalid syntax, and captures the parser
/// state at `cursor` (a byte offset).
pub(super) fn analyze(source: &str, cursor: usize) -> Scan {
    let tokens = tokenize(source);
    let plan = plan_paths(source, &tokens);
    let partial_index = tokens.iter().position(|token| {
        contains_caret(source, token, cursor)
            && !matches!(token.kind, Kind::Special | Kind::Comment)
    });
    let mut frames: Vec<Frame> = Vec::new();
    let mut scan = Scan::default();

    // Records a `<name>` start tag: its name span and the new open frame
    // (`empty` skips the frame for `<name/>`).
    let open_element =
        |scan: &mut Scan, frames: &mut Vec<Frame>, name: &str, tag: &Token, empty: bool| {
            let path = plan.path_of(tag.start).map_or_else(
                || {
                    let mut fallback = frames
                        .last()
                        .map_or_else(Vec::new, |frame| frame.path.clone());
                    fallback.push(Seg::Key(name.to_string()));
                    fallback
                },
                <[Seg]>::to_vec,
            );
            if !path.is_empty() {
                scan.key_spans.push((
                    strings(&path),
                    local_name_start(source, tag),
                    local_name_start(source, tag) + name_len(source, tag),
                ));
            }
            if !empty {
                frames.push(Frame {
                    name: name.to_string(),
                    path,
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
                    let path = plan
                        .path_of(token.start)
                        .map_or_else(Vec::new, <[Seg]>::to_vec);
                    let metadata = plan.root == Some(token.start);
                    for inner in tokenize_tag_inside(source, &token)
                        .into_iter()
                        .filter(|_| !metadata)
                    {
                        if let Kind::Text { quoted: false } = inner.kind
                            && is_attr_name(source, &token, inner)
                            && !is_partial
                        {
                            scan.key_spans.push((
                                attribute_key(&path, &source[inner.start..inner.end]),
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
                if !is_partial
                    && (token.kind != Kind::Special || source[token.start..].starts_with("<![CDATA[")) =>
            {
                let raw = &source[token.start..token.end];
                let text = raw.trim();
                if !text.is_empty()
                    && let Some(frame) = frames.last()
                    && !frame.path.is_empty()
                {
                    let path = strings(&frame.path);
                    let start = token.start + (raw.len() - raw.trim_start().len());
                    let end = start + text.len();
                    // Text over several lines is one value: its span runs
                    // from its first line to its last.
                    match scan.value_spans.last_mut() {
                        Some((last, _, last_end)) if *last == path && source[*last_end..start].trim().is_empty() => {
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

    record_attribute_values(source, &tokens, &plan, &mut scan);

    if scan.snapshot.is_none() {
        scan.snapshot = Some(capture_snapshot(source, &tokens, &frames, &plan, cursor));
    }
    scan
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

/// Attributes are ordinary properties of their element.
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
        .map_or_else(Vec::new, |frame| frame.path.clone());
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
    Snapshot {
        path,
        partial,
        in_tag: open_tag.is_some(),
        open_tag,
        open_tag_path,
        inner,
        pending_attr,
        in_root_tag: open_tag.is_some_and(|tag| plan.root == Some(tag.start)),
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
                    attribute_key(&element_path, &name),
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
    let before = &source[..byte];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    (line, before[line_start..].chars().count() + 1)
}

pub(super) fn xml_text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

/// Whether a start tag holds no children: `<name/>`, or a tag still being
/// typed that the next tag cut short (`<t` then `<tls>` on the next line).
fn opens_nothing(source: &str, tag: &Token) -> bool {
    let text = &source[..tag.end];
    text.ends_with("/>") || (!text.ends_with('>') && tag.end < source.len())
}
