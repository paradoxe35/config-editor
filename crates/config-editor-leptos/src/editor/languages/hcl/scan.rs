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
    /// Header occurrence totals keyed by `(parent body, type, labels)`.
    pub(super) totals: HashMap<(usize, String, String), usize>,
    pub(super) attributes: Vec<AttributeInfo>,
    pub(super) snapshot: Snapshot,
}

/// An attribute value being read. It ends at a newline at bracket depth zero,
/// or at a closing bracket that belongs to an enclosing body.
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
    /// grow until the next key or closing brace.
    pub(super) nested_value: Option<usize>,
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
    while let Some(header) = scan.headers.iter().find(|header| header.child == current) {
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

/// Walks the whole document, tolerating invalid syntax, and captures the
/// parser state at `cursor` (a byte offset). Block headers and attributes are
/// recognized by token shape, so a half-typed document still yields structure.
pub(super) fn analyze(source: &str, cursor: usize) -> Scan {
    let tokens = tokenize(source);
    let partial_index = tokens.iter().position(|token| {
        token.start < cursor
            && cursor <= token.end
            && matches!(token.kind, Kind::Ident | Kind::Str { .. })
    });
    let partial = partial_index.map(|index| tokens[index]);
    let mut body_count = 1;
    let mut headers: Vec<HeaderInfo> = Vec::new();
    let mut totals: HashMap<(usize, String, String), usize> = HashMap::new();
    let mut attributes: Vec<AttributeInfo> = Vec::new();
    let mut snapshot: Option<Snapshot> = None;
    // Open bodies, innermost last. A stray `{` repeats the current body so that
    // its `}` pops cleanly without opening anything.
    let mut stack: Vec<usize> = vec![0];
    let mut open: Option<OpenValue> = None;

    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        // A line break at bracket depth zero ends the value being read.
        if token.nl_before
            && token.kind != Kind::Comment
            && open
                .as_ref()
                .is_some_and(|value| value.started && value.depth == 0)
        {
            open = None;
        }
        if snapshot.is_none() && (Some(index) == partial_index || token.start >= cursor) {
            snapshot = Some(take_snapshot(
                &attributes,
                &stack,
                open.as_ref(),
                cursor,
                partial,
            ));
        }
        if token.kind == Kind::Comment {
            index += 1;
            continue;
        }
        // Inside a value only brackets matter, plus the map layering that key
        // completion and nested key spans need. A closing bracket at depth
        // zero belongs to an enclosing body, so it ends the value and is read
        // below.
        let mut closes_value = false;
        if let Some(value) = open.as_mut() {
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
                    value.nested_value = None;
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
                attributes[value.attribute].value_span.1 = token.end;
                // A `key =` written inside a map braces records the nested
                // attribute, so its schema path and spans resolve there. The
                // `=` is skipped so the next token (its value or `{`) is read
                // with the key pending.
                if value.braces >= 1
                    && value.containers == 0
                    && matches!(token.kind, Kind::Ident | Kind::Str { .. })
                    && let Some(equals) = next_significant(&tokens, index + 1)
                        .filter(|next| tokens[*next].kind == Kind::Punct('='))
                    && let Some(name) = token_name(source, &token)
                {
                    let value_start = next_significant(&tokens, equals + 1)
                        .map_or(source.len(), |next| tokens[next].start);
                    attributes.push(AttributeInfo {
                        body: stack.last().copied().unwrap_or(0),
                        name: name.clone(),
                        prefix: map_prefix(&attributes, value),
                        key_span: (token.start, token.end),
                        value_span: (value_start, value_start),
                    });
                    value.nested_value = Some(attributes.len() - 1);
                    value.pending_key = Some(name);
                    index = equals;
                    index += 1;
                    continue;
                }
                // A comma between entries ends the entry before it.
                if token.kind == Kind::Punct(',') && value.containers == 0 {
                    value.nested_value = None;
                    value.pending_key = None;
                    index += 1;
                    continue;
                }
                if let Some(nested) = value.nested_value {
                    attributes[nested].value_span.1 = token.end;
                } else {
                    value.pending_key = None;
                }
                index += 1;
                continue;
            }
            open = None;
        }
        let body = stack.last().copied().unwrap_or(0);
        match token.kind {
            Kind::Punct('{') => stack.push(body),
            Kind::Punct('}') => {
                if stack.len() > 1 {
                    stack.pop();
                }
            }
            Kind::Ident | Kind::Str { .. } => {
                let equals = next_significant(&tokens, index + 1)
                    .filter(|next| tokens[*next].kind == Kind::Punct('='));
                if let (Some(equals), Some(name)) = (equals, token_name(source, &token)) {
                    let value_start = next_significant(&tokens, equals + 1)
                        .map_or(source.len(), |next| tokens[next].start);
                    attributes.push(AttributeInfo {
                        body,
                        name,
                        prefix: Vec::new(),
                        key_span: (token.start, token.end),
                        value_span: (value_start, value_start),
                    });
                    open = Some(OpenValue {
                        attribute: attributes.len() - 1,
                        depth: 0,
                        started: false,
                        braces: 0,
                        containers: 0,
                        map_keys: Vec::new(),
                        pending_key: None,
                        nested_value: None,
                    });
                    index = equals;
                } else if let Some(brace) = classify_header(&tokens, index) {
                    let mut names: Vec<(String, (usize, usize))> = tokens[index..brace]
                        .iter()
                        .filter_map(|name_token| {
                            token_name(source, name_token)
                                .map(|name| (name, (name_token.start, name_token.end)))
                        })
                        .collect();
                    if !names.is_empty() {
                        let (block_type, type_span) = names.remove(0);
                        let labels: Vec<String> =
                            names.iter().map(|(name, _)| name.clone()).collect();
                        let label_spans: Vec<(usize, usize)> =
                            names.iter().map(|(_, span)| *span).collect();
                        let count = totals
                            .entry((body, block_type.clone(), labels.join("\0")))
                            .or_insert(0);
                        let ordinal = *count;
                        *count += 1;
                        let child = body_count;
                        body_count += 1;
                        headers.push(HeaderInfo {
                            parent: body,
                            child,
                            block_type,
                            labels,
                            ordinal,
                            type_span,
                            label_spans,
                        });
                        stack.push(child);
                    }
                    index = brace;
                }
            }
            _ => {}
        }
        index += 1;
    }

    // Past the last token, a line break before the caret ends the value just
    // as a following token's line break would (`listen = 7835\n|` at the end).
    let last_end = tokens.last().map_or(0, |token| token.end);
    if snapshot.is_none()
        && source[last_end.min(cursor)..cursor.min(source.len())].contains('\n')
        && open
            .as_ref()
            .is_some_and(|value| value.started && value.depth == 0)
    {
        open = None;
    }
    let snapshot = snapshot
        .unwrap_or_else(|| take_snapshot(&attributes, &stack, open.as_ref(), cursor, partial));
    Scan {
        headers,
        totals,
        attributes,
        snapshot,
    }
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
