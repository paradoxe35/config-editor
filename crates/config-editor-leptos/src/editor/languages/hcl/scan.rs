//! What surrounds the caret: the path, open elements, and pending names.

use super::*;

/// One `type "label" ... { ... }` block: the body it opens and where it sits.
pub(super) struct HeaderInfo {
    pub(super) parent: usize,
    pub(super) child: usize,
    pub(super) block_type: String,
    pub(super) labels: Vec<String>,
    /// Position among the same-`(type, labels)` headers of the parent body. The
    /// JSON mapping renders repeats as arrays, so the ordinal becomes an index
    /// segment (`/variable/name/1/...`).
    pub(super) ordinal: usize,
    /// Read only by schema-path span lookups.
    pub(super) type_span: (usize, usize),
    pub(super) label_spans: Vec<(usize, usize)>,
}

/// One `name = value` attribute: the body it sits in and its byte spans.
pub(super) struct AttributeInfo {
    pub(super) body: usize,
    pub(super) name: String,
    /// Key chain from the owning body-level `name = {` assignment down to this
    /// entry. Empty for attributes written directly in a body; `["acme"]` for
    /// `secret` in `clients = { acme = { secret = ... } }`. Lets completion and
    /// error spans descend into nested value maps.
    pub(super) prefix: Vec<String>,
    /// Read only by schema-path span lookups.
    pub(super) key_span: (usize, usize),
    pub(super) value_span: (usize, usize),
}

#[derive(Clone)]
pub(super) struct Snapshot {
    pub(super) body: usize,
    pub(super) in_value: bool,
    pub(super) value_key: Option<String>,
    /// Key chain from the owning body-level assignment into the map layer at
    /// the caret (`["clients", "acme"]` inside `clients = { acme = { | } }`).
    /// Completion descends through it into the map's schema.
    pub(super) value_prefix: Vec<String>,
    /// Whether the caret sits inside `{ }` of a value map rather than at a
    /// plain value position or inside an array.
    pub(super) in_map: bool,
    pub(super) value_start: usize,
    pub(super) partial: Option<Token>,
}

pub(super) struct Scan {
    pub(super) headers: Vec<HeaderInfo>,
    /// The header opening each body (by body number), so a body's path is
    /// read without a search through every header.
    pub(super) openers: Vec<Option<usize>>,
    /// Header occurrence totals keyed by `(parent body, type, labels)`.
    pub(super) totals: HashMap<(usize, String, String), usize>,
    pub(super) attributes: Vec<AttributeInfo>,
    pub(super) snapshot: Snapshot,
}

/// An attribute value being read. It ends at a newline at bracket depth zero,
/// or at a closing bracket that belongs to an enclosing body.
#[derive(Clone)]
pub(super) struct OpenValue {
    pub(super) attribute: usize,
    pub(super) depth: usize,
    pub(super) started: bool,
    /// Open `{` layers inside the value (the value's own map is the first).
    pub(super) braces: usize,
    /// Open `[` / `(` layers inside the value. Array items are not map keys,
    /// so key completion and nested key spans stay off inside them.
    pub(super) containers: usize,
    /// Key chain below the owning attribute for the innermost open map layer
    /// (`["acme"]` inside `clients = { acme = { ... } }`).
    pub(super) map_keys: Vec<String>,
    /// The `key =` just read inside the map, awaiting its `{`.
    pub(super) pending_key: Option<String>,
    /// The nested attribute whose value is still being read, so its span can
    /// grow until the next key or the brace closing its map, with the depth
    /// its entry is at (brackets inside its value, `merge(a, { b = 1 })`,
    /// close inside it).
    pub(super) nested_value: Option<(usize, usize)>,
}

impl Scan {
    /// Whether `header` repeats an earlier header with the same type and labels.
    pub(super) fn repeats(&self, header: &HeaderInfo) -> bool {
        self.totals
            .get(&(
                header.parent,
                header.block_type.clone(),
                header.labels.join("\0"),
            ))
            .is_some_and(|total| *total > 1)
    }
}

/// Finalized path of `body`: the parent path plus each owning header's type and
/// labels, with an ordinal segment when the header repeats.
pub(super) fn body_path(scan: &Scan, body: usize) -> Vec<String> {
    let mut chain = Vec::new();
    let mut current = body;
    while let Some(header) = scan
        .openers
        .get(current)
        .copied()
        .flatten()
        .map(|index| &scan.headers[index])
    {
        chain.push(header);
        current = header.parent;
    }
    let mut segments = Vec::new();
    for header in chain.into_iter().rev() {
        segments.push(header.block_type.clone());
        segments.extend(header.labels.iter().cloned());
        if scan.repeats(header) {
            segments.push(header.ordinal.to_string());
        }
    }
    segments
}

/// Schema path of an attribute: its body's path, its map prefix (empty at the
/// top level), plus its key.
pub(super) fn attribute_path(scan: &Scan, attribute: &AttributeInfo) -> Vec<String> {
    let mut path = body_path(scan, attribute.body);
    path.extend(attribute.prefix.iter().cloned());
    path.push(attribute.name.clone());
    path
}

/// Index of the first non-comment token at or after `from`.
pub(super) fn next_significant(tokens: &[Token], from: usize) -> Option<usize> {
    (from..tokens.len()).find(|&next| tokens[next].kind != Kind::Comment)
}

/// Map key chain for an entry written inside the value being read: the owning
/// attribute's own prefix and name, then each `key = {` layer below it.
pub(super) fn map_prefix(attributes: &[AttributeInfo], value: &OpenValue) -> Vec<String> {
    let owner = &attributes[value.attribute];
    let mut prefix = owner.prefix.clone();
    prefix.push(owner.name.clone());
    prefix.extend(value.map_keys.iter().cloned());
    prefix
}

/// Captures the parser state at the caret: the innermost body, and the
/// attribute whose value the caret sits in, if any.
pub(super) fn take_snapshot(
    attributes: &[AttributeInfo],
    stack: &[usize],
    open: Option<&OpenValue>,
    cursor: usize,
    partial: Option<Token>,
) -> Snapshot {
    let body = stack.last().copied().unwrap_or(0);
    match open {
        Some(value) => {
            let attribute = &attributes[value.attribute];
            let in_map = value.braces >= 1 && value.containers == 0;
            let value_prefix = if in_map {
                map_prefix(attributes, value)
            } else {
                Vec::new()
            };
            Snapshot {
                body,
                in_value: true,
                value_key: Some(attribute.name.clone()),
                value_prefix,
                in_map,
                value_start: attribute.value_span.0,
                partial,
            }
        }
        None => Snapshot {
            body,
            in_value: false,
            value_key: None,
            value_prefix: Vec::new(),
            in_map: false,
            value_start: cursor,
            partial,
        },
    }
}

/// A text's hash and length, a cursor, and the scan there.
type RememberedScan = (u64, usize, usize, std::rc::Rc<Scan>);

thread_local! {
    /// The last scan: a check's problem spans, and one keystroke's
    /// suggestions and their read-only check, each read the same text at
    /// the same place.
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
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    let key = (hasher.finish(), source.len(), cursor);
    let remembered = LAST_SCAN.with(|last| {
        last.borrow()
            .as_ref()
            .filter(|(hash, len, at, _)| (*hash, *len, *at) == key)
            .map(|(.., scan)| scan.clone())
    });
    if let Some(scan) = remembered {
        return scan;
    }
    let scan = std::rc::Rc::new(scan_text(source, cursor));
    LAST_SCAN.with(|last| *last.borrow_mut() = Some((key.0, key.1, key.2, scan.clone())));
    scan
}

/// Walks the whole document, tolerating invalid syntax, and captures the
/// parser state at `cursor` (a byte offset). Block headers and attributes are
/// recognized by token shape, so a half-typed document still yields structure.
pub(super) fn scan_text(source: &str, cursor: usize) -> Scan {
    // A walk from the top: counted where it starts while a batch runs for
    // the text (outside one, the lookup asking for it counts it).
    crate::editor::paths::walked_from_top(source);
    let tokens = tokenize(source);
    let partial_index = tokens.iter().position(|token| {
        token.start < cursor
            && cursor <= token.end
            && matches!(token.kind, Kind::Ident | Kind::Str { .. })
    });
    let partial = partial_index.map(|index| tokens[index]);
    let mut record = Record::default();
    let mut snapshot: Option<Snapshot> = None;
    let mut walk = Walk::new();
    while walk.index < tokens.len() {
        let token = tokens[walk.index];
        walk.line_break(&token);
        if snapshot.is_none() && (Some(walk.index) == partial_index || token.start >= cursor) {
            snapshot = Some(take_snapshot(
                &record.attributes,
                &walk.stack,
                walk.open.as_ref(),
                cursor,
                partial,
            ));
        }
        walk.step(source, &tokens, Some(&mut record));
    }
    let snapshot = snapshot.unwrap_or_else(|| walk.last_snapshot(source, &tokens, &record.attributes, cursor, partial));
    record.into_scan(snapshot, walk.body_count)
}

/// What the walk over a text's tokens writes down: every block header and
/// attribute, with how often each header repeats.
#[derive(Default)]
struct Record {
    headers: Vec<HeaderInfo>,
    totals: HashMap<(usize, String, String), usize>,
    attributes: Vec<AttributeInfo>,
}

impl Record {
    fn into_scan(self, snapshot: Snapshot, body_count: usize) -> Scan {
        let mut openers = vec![None; body_count];
        for (index, header) in self.headers.iter().enumerate() {
            if let Some(opener) = openers.get_mut(header.child) {
                opener.get_or_insert(index);
            }
        }
        Scan {
            headers: self.headers,
            openers,
            totals: self.totals,
            attributes: self.attributes,
            snapshot,
        }
    }
}

/// Where the walk over a text's tokens is: the token it reads next, the
/// open bodies, the value being read, and how many bodies and attributes
/// it has met.
#[derive(Clone)]
struct Walk {
    index: usize,
    body_count: usize,
    /// Open bodies, innermost last. A stray `{` repeats the current body
    /// so that its `}` pops cleanly without opening anything.
    stack: Vec<usize>,
    open: Option<OpenValue>,
    attributes: usize,
}

impl Walk {
    fn new() -> Self {
        Walk {
            index: 0,
            body_count: 1,
            stack: vec![0],
            open: None,
            attributes: 0,
        }
    }

    /// A line break at bracket depth zero before `token` ends the value
    /// being read.
    fn line_break(&mut self, token: &Token) {
        if token.nl_before
            && token.kind != Kind::Comment
            && self
                .open
                .as_ref()
                .is_some_and(|value| value.started && value.depth == 0)
        {
            self.open = None;
        }
    }

    /// The caret's place past the last token: a line break before the caret
    /// ends the value just as a following token's line break would
    /// (`listen = 7835\n|` at the end).
    fn last_snapshot(
        &self,
        source: &str,
        tokens: &[Token],
        attributes: &[AttributeInfo],
        cursor: usize,
        partial: Option<Token>,
    ) -> Snapshot {
        let last_end = tokens.last().map_or(0, |token| token.end);
        let open = self.open.as_ref().filter(|value| {
            !(source[last_end.min(cursor)..cursor.min(source.len())].contains('\n')
                && value.started
                && value.depth == 0)
        });
        take_snapshot(attributes, &self.stack, open, cursor, partial)
    }

    /// Reads the token at `index` (and the ones it takes along: an
    /// attribute's `=`, a header's names), writing down what it finds in
    /// `record` when there is one.
    fn step(&mut self, source: &str, tokens: &[Token], mut record: Option<&mut Record>) {
        let index = self.index;
        let token = tokens[index];
        if token.kind == Kind::Comment {
            self.index += 1;
            return;
        }
        // Inside a value only brackets matter, plus the map layering that key
        // completion and nested key spans need. A closing bracket at depth
        // zero belongs to an enclosing body, so it ends the value and is read
        // below.
        let mut closes_value = false;
        if let Some(value) = self.open.as_mut() {
            match token.kind {
                Kind::Punct('{') => {
                    value.depth += 1;
                    value.braces += 1;
                    // An inner `key = {` layer: entries below it belong to the
                    // key chain; the value's own brace has no pending key.
                    if let Some(key) = value.pending_key.take() {
                        value.map_keys.push(key);
                    }
                }
                Kind::Punct('[' | '(') => {
                    value.depth += 1;
                    value.containers += 1;
                }
                Kind::Punct('}' | ']' | ')') if value.depth > 0 => {
                    value.depth -= 1;
                    value.pending_key = None;
                    if value.nested_value.is_some_and(|(_, depth)| value.depth < depth) {
                        value.nested_value = None;
                    }
                    match token.kind {
                        Kind::Punct('}') if value.braces > 0 => {
                            value.braces -= 1;
                            // Still inside the value's map: the inner layer
                            // closed, so its key leaves the chain.
                            if value.braces >= 1 {
                                value.map_keys.pop();
                            }
                        }
                        _ => value.containers = value.containers.saturating_sub(1),
                    }
                }
                Kind::Punct('}') | Kind::Punct(']') | Kind::Punct(')') => closes_value = true,
                _ => {}
            }
            if !closes_value {
                value.started = true;
                if let Some(record) = record.as_deref_mut() {
                    record.attributes[value.attribute].value_span.1 = token.end;
                }
                // A `key =` written inside a map braces records the nested
                // attribute, so its schema path and spans resolve there. The
                // `=` is skipped so the next token (its value or `{`) is read
                // with the key pending.
                if value.braces >= 1
                    && value.containers == 0
                    && matches!(token.kind, Kind::Ident | Kind::Str { .. })
                    && let Some(equals) = next_significant(tokens, index + 1)
                        .filter(|next| tokens[*next].kind == Kind::Punct('='))
                    && let Some(name) = token_name(source, &token)
                {
                    if let Some(record) = record.as_deref_mut() {
                        let value_start = next_significant(tokens, equals + 1)
                            .map_or(source.len(), |next| tokens[next].start);
                        let prefix = map_prefix(&record.attributes, value);
                        record.attributes.push(AttributeInfo {
                            body: self.stack.last().copied().unwrap_or(0),
                            name: name.clone(),
                            prefix,
                            key_span: (token.start, token.end),
                            value_span: (value_start, value_start),
                        });
                    }
                    self.attributes += 1;
                    value.nested_value = Some((self.attributes - 1, value.depth));
                    value.pending_key = Some(name);
                    self.index = equals + 1;
                    return;
                }
                // A comma between entries ends the entry before it.
                if token.kind == Kind::Punct(',') && value.containers == 0 {
                    value.nested_value = None;
                    value.pending_key = None;
                    self.index += 1;
                    return;
                }
                if let Some((nested, _)) = value.nested_value {
                    if let Some(record) = record.as_deref_mut() {
                        record.attributes[nested].value_span.1 = token.end;
                    }
                } else {
                    value.pending_key = None;
                }
                self.index += 1;
                return;
            }
            self.open = None;
        }
        let body = self.stack.last().copied().unwrap_or(0);
        match token.kind {
            Kind::Punct('{') => self.stack.push(body),
            Kind::Punct('}') => {
                if self.stack.len() > 1 {
                    self.stack.pop();
                }
            }
            Kind::Ident | Kind::Str { .. } => {
                let equals = next_significant(tokens, index + 1)
                    .filter(|next| tokens[*next].kind == Kind::Punct('='));
                if let (Some(equals), Some(name)) = (equals, token_name(source, &token)) {
                    if let Some(record) = record.as_deref_mut() {
                        let value_start = next_significant(tokens, equals + 1)
                            .map_or(source.len(), |next| tokens[next].start);
                        record.attributes.push(AttributeInfo {
                            body,
                            name,
                            prefix: Vec::new(),
                            key_span: (token.start, token.end),
                            value_span: (value_start, value_start),
                        });
                    }
                    self.attributes += 1;
                    self.open = Some(OpenValue {
                        attribute: self.attributes - 1,
                        depth: 0,
                        started: false,
                        braces: 0,
                        containers: 0,
                        map_keys: Vec::new(),
                        pending_key: None,
                        nested_value: None,
                    });
                    self.index = equals + 1;
                    return;
                } else if let Some(brace) = classify_header(tokens, index) {
                    let names: Vec<&Token> = tokens[index..brace]
                        .iter()
                        .filter(|name_token| token_name(source, name_token).is_some())
                        .collect();
                    if !names.is_empty() {
                        if let Some(record) = record.as_mut() {
                            let mut names: Vec<(String, (usize, usize))> = names
                                .iter()
                                .filter_map(|name_token| {
                                    token_name(source, name_token)
                                        .map(|name| (name, (name_token.start, name_token.end)))
                                })
                                .collect();
                            let (block_type, type_span) = names.remove(0);
                            let labels: Vec<String> =
                                names.iter().map(|(name, _)| name.clone()).collect();
                            let label_spans: Vec<(usize, usize)> =
                                names.iter().map(|(_, span)| *span).collect();
                            let count = record
                                .totals
                                .entry((body, block_type.clone(), labels.join("\0")))
                                .or_insert(0);
                            let ordinal = *count;
                            *count += 1;
                            record.headers.push(HeaderInfo {
                                parent: body,
                                child: self.body_count,
                                block_type,
                                labels,
                                ordinal,
                                type_span,
                                label_spans,
                            });
                        }
                        self.stack.push(self.body_count);
                        self.body_count += 1;
                    }
                    self.index = brace + 1;
                    return;
                }
            }
            _ => {}
        }
        self.index += 1;
    }
}

/// Visits of the walk between two kept states.
const KEEP_EVERY: usize = 64;

/// A text's scan and the walk over its tokens, kept every [`KEEP_EVERY`]
/// visits, read once while a batch runs for it (see `paths::paths_at`).
pub(super) struct Walked {
    tokens: Vec<Token>,
    pub(super) scan: std::rc::Rc<Scan>,
    kept: Vec<Walk>,
    last: Walk,
}

/// The scan of `source` and its walk, read once while a batch runs for
/// it; `None` outside one.
pub(super) fn walked(source: &str) -> Option<std::rc::Rc<Walked>> {
    if !crate::editor::paths::in_batch(source) {
        return None;
    }
    Some(crate::editor::paths::batch_cached(source, || {
        crate::editor::paths::walked_from_top(source);
        let tokens = tokenize(source);
        let mut record = Record::default();
        let mut walk = Walk::new();
        let mut kept = Vec::new();
        let mut visits = 0;
        while walk.index < tokens.len() {
            if visits % KEEP_EVERY == 0 {
                kept.push(walk.clone());
            }
            visits += 1;
            let token = tokens[walk.index];
            walk.line_break(&token);
            walk.step(source, &tokens, Some(&mut record));
        }
        let snapshot = walk.last_snapshot(source, &tokens, &record.attributes, 0, None);
        let scan = std::rc::Rc::new(record.into_scan(snapshot, walk.body_count));
        Walked {
            tokens,
            scan,
            kept,
            last: walk,
        }
    }))
}

/// The scan of `source` and the caret's place at byte `cursor`: what
/// [`analyze`] reads there (its scan's own snapshot is the caret's
/// outside a batch). While a batch runs for the text, the scan is read
/// once and the place from the walk kept for it.
pub(super) fn analyze_at(source: &str, cursor: usize) -> (std::rc::Rc<Scan>, Snapshot) {
    let Some(walked) = walked(source) else {
        let scan = analyze(source, cursor);
        let snapshot = scan.snapshot.clone();
        return (scan, snapshot);
    };
    let tokens = &walked.tokens;
    // The string or name holding the caret, or the first token past it.
    let next = tokens.partition_point(|token| token.start < cursor);
    let partial_index = next.checked_sub(1).filter(|before| {
        let token = &tokens[*before];
        cursor <= token.end && matches!(token.kind, Kind::Ident | Kind::Str { .. })
    });
    let partial = partial_index.map(|index| tokens[index]);
    let target = partial_index.unwrap_or(next);
    let attributes = &walked.scan.attributes;
    if target >= tokens.len() {
        let snapshot = walked.last.last_snapshot(source, tokens, attributes, cursor, partial);
        return (walked.scan.clone(), snapshot);
    }
    let from = walked.kept.partition_point(|walk| walk.index <= target);
    let mut walk = walked.kept[from.saturating_sub(1)].clone();
    let snapshot = loop {
        if walk.index >= tokens.len() {
            break walk.last_snapshot(source, tokens, attributes, cursor, partial);
        }
        walk.line_break(&tokens[walk.index]);
        if walk.index >= target {
            break take_snapshot(attributes, &walk.stack, walk.open.as_ref(), cursor, partial);
        }
        walk.step(source, tokens, None);
    };
    (walked.scan.clone(), snapshot)
}

/// Index of the `{` opening a block header whose names start at `index`, or
/// `None` when the tokens do not form one. A header is one or more names on a
/// single line followed by `{` on that same line.
pub(super) fn classify_header(tokens: &[Token], index: usize) -> Option<usize> {
    let mut names = 0;
    for (offset, token) in tokens[index..].iter().enumerate() {
        match token.kind {
            Kind::Comment => {}
            Kind::Ident | Kind::Str { .. } => {
                if names > 0 && token.nl_before {
                    return None;
                }
                names += 1;
            }
            Kind::Punct('{') => {
                return (names > 0 && !token.nl_before).then_some(index + offset);
            }
            _ => return None,
        }
    }
    None
}
