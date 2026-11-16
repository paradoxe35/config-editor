//! Values and items a suggestion writes as list items: one in front of
//! an item already written gets the separator before it, and in an empty
//! document under a root list of values, the values open the list.

use super::placed::Placed;
use super::*;
use crate::editor::text::text_size;

/// `options` for a new list item, written in front of an item already
/// there (see [`before_item`]): each ends at the caret, then the separator.
pub(super) fn separate_from_next_item(
    options: &mut [Completion],
    source: &str,
    cursor_byte: usize,
    (context, placed): (&CompletionContext, &Placed),
    behavior: &dyn ConfigLanguage,
) {
    if let Some((end, separator)) = before_item(source, cursor_byte, (context, placed), behavior) {
        for option in options {
            option.end = end;
            option.insert.push(separator);
        }
    }
}

/// Where a new item typed in front of an item already written (`[| {
/// a = 1 }]`, or that item on the next line) ends, and the separator it
/// needs before it: the replaced range stops at the caret, short of the
/// item. `None` when no item follows (a closer, a separator, the end).
fn before_item(
    source: &str,
    cursor_byte: usize,
    (context, placed): (&CompletionContext, &Placed),
    behavior: &dyn ConfigLanguage,
) -> Option<(TextSize, char)> {
    let config = behavior.config();
    let separator = config.item_separator?;
    // The language wrote it already (JSON before a quoted item).
    if context.entry_suffix.contains(separator) {
        return None;
    }
    let cursor_byte = cursor_byte.min(source.len());
    // Only in a list written inline: right after its opener or a separator
    // (a block list's `- ` item is no such place).
    let start = usize::from(placed.replace.start()).min(cursor_byte);
    let opens_list = |character: char| {
        super::super::flow_syntax::Flow::of(behavior)
            .list()
            .is_some_and(|(open, _)| open == character)
    };
    // What comes before, past the comment lines between (`[\n  # first\n  |`).
    let is_comment = |line: &str| {
        let line = line.trim_start();
        config
            .line_comments
            .iter()
            .any(|marker| !marker.is_empty() && line.starts_with(*marker))
    };
    let mut head = source[..start].trim_end();
    while let Some(newline) = head.rfind('\n')
        && is_comment(&head[newline + 1..])
    {
        head = head[..newline].trim_end();
    }
    let before = head.chars().next_back()?;
    if before != separator && !opens_list(before) {
        return None;
    }
    let end = usize::from(placed.replace.end()).max(cursor_byte);
    // The range up to the caret only, when the rest of it is the next
    // item's (past a blank), not the rest of the one at the caret.
    let typed_end = if source[cursor_byte..end].starts_with(char::is_whitespace) {
        cursor_byte
    } else {
        end
    };
    let mut rest = &source[typed_end..];
    loop {
        rest = rest.trim_start();
        match config
            .line_comments
            .iter()
            .find(|marker| !marker.is_empty() && rest.starts_with(**marker))
        {
            Some(_) => rest = rest.find('\n').map_or("", |at| &rest[at..]),
            None => break,
        }
    }
    let next = rest.chars().next()?;
    let closes = config.brackets.iter().any(|(_, close)| *close == next);
    (!closes && next != separator).then(|| (text_size(typed_end), separator))
}

/// In an empty document under a schema whose root is a list of values
/// (not objects): each value its items allow, written as the list's first
/// item (see `CompletionContext::root_value`). `None` elsewhere.
pub(super) fn root_values(
    (context, placed): (&CompletionContext, &Placed),
    branches: &[&Value],
    applies: &Applies<'_, '_>,
    behavior: &dyn ConfigLanguage,
) -> Option<Vec<Completion>> {
    let (open, close) = context.root_value.filter(|_| context.value_key.is_none())?;
    let root = applies.root();
    let objects = |branches: &[&Value]| branches.iter().any(|branch| describes_object(branch));
    if objects(branches) || !branches.iter().any(|branch| describes_array(branch)) {
        return None;
    }
    let items = applies.at(&[SchemaPathSegment::ArrayItem(0)]);
    if objects(&items.branches()) {
        return None;
    }
    let mut options = value_completions(
        &items,
        &context.typed,
        placed.replace,
        root,
        "",
        true,
        behavior,
    );
    for option in &mut options {
        option.insert = format!("{open}{}{close}", option.insert);
        option.cursor_offset += TextSize::of(open);
    }
    Some(options)
}
