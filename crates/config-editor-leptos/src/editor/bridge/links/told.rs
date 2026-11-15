//! What a view's links tell a save: each item of the view's text paired
//! with the links starting on its first line, and its source in the
//! host's last text or an earlier one; and links carried over a switch
//! to the items at the same pointers.

use super::super::super::item_sources::ItemSource;
use super::super::super::language::ConfigLanguage;
use super::super::super::parse_cache;
use super::super::super::work_count::{Work, note};
use super::items::items;
use super::{Link, Links, Source, content};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// What a save is told of the view's items: each one's source, by its
/// pointer in the view's value, and the host text before the last that
/// the [`ItemSource::Origin`] ones are in.
pub(crate) struct Told {
    pub(crate) items: HashMap<String, ItemSource>,
    pub(crate) earlier: Option<String>,
}

/// Whether `before` and `after` (in `language`) read as values with the
/// same keys in the same order and lists of the same lengths, all the
/// way down; `false` when either does not read.
fn same_keys(language: &dyn ConfigLanguage, before: &str, after: &str) -> bool {
    fn shape(left: &serde_json::Value, right: &serde_json::Value) -> bool {
        use serde_json::Value;
        match (left, right) {
            (Value::Object(left), Value::Object(right)) => {
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right)
                        .all(|((key, left), (other, right))| key == other && shape(left, right))
            }
            (Value::Array(left), Value::Array(right)) => {
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right)
                        .all(|(left, right)| shape(left, right))
            }
            (Value::Object(_) | Value::Array(_), _) | (_, Value::Object(_) | Value::Array(_)) => {
                false
            }
            _ => true,
        }
    }
    let (before, after) = (
        parse_cache::parsed(language, before),
        parse_cache::parsed(language, after),
    );
    match (before.as_ref(), after.as_ref()) {
        (Ok(before), Ok(after)) => shape(before, after),
        _ => false,
    }
}

impl Links {
    /// What the links tell a save of the view's text `text` (in
    /// `language`) over the host's last text `previous`: `None` when they
    /// tell nothing more than the values do (typing only, since a save
    /// whose host items hold no more than the view shows: `identity`
    /// false).
    pub(crate) fn told(
        &mut self,
        language: &dyn ConfigLanguage,
        text: &str,
        previous: &str,
        identity: bool,
    ) -> Option<Told> {
        // Typing only, its keys all where they were: each item is where
        // the patches find it by its values. Typing that renames a key or
        // a table's header moves every item under it (QA `FZ38-4`).
        // Not when the links are of another host text than the last (an
        // undo back past a save, QA `FZ38-12`): its items are no longer
        // where the last text has them.
        if let Links::Fresh { base, host, .. } = self
            && !identity
            && **host == *previous
            && same_keys(language, base, text)
        {
            return None;
        }
        let paired = self.paired(language, text)?;
        // The host text before the last most items come from.
        let mut earlier: Vec<(&Arc<str>, usize)> = Vec::new();
        for (_, source) in &paired {
            if &*source.host == previous {
                continue;
            }
            match earlier
                .iter_mut()
                .find(|(host, _)| Arc::ptr_eq(host, &source.host))
            {
                Some((_, count)) => *count += 1,
                None => earlier.push((&source.host, 1)),
            }
        }
        let earlier = earlier
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(host, _)| host.clone());
        let items = paired
            .into_iter()
            .filter_map(|(pointer, source)| {
                let told = if &*source.host == previous {
                    ItemSource::Previous(source.pointer.clone())
                } else if earlier
                    .as_ref()
                    .is_some_and(|host| Arc::ptr_eq(host, &source.host))
                {
                    ItemSource::Origin(source.pointer.clone())
                } else {
                    return None;
                };
                Some((pointer, told))
            })
            .collect();
        Some(Told {
            items,
            earlier: earlier.map(|host| host.to_string()),
        })
    }

    /// Each item of `text` (in `language`) with a link, and its source:
    /// paired with the links starting on its first line, outermost first;
    /// `None` when no item has one.
    fn paired(
        &mut self,
        language: &dyn ConfigLanguage,
        text: &str,
    ) -> Option<Vec<(String, Arc<Source>)>> {
        if !self.fits(text.len()) {
            return None;
        }
        let links = self.read(language);
        if links.is_empty() {
            return None;
        }
        let parsed = parse_cache::parsed(language, text);
        let value = parsed.as_ref().as_ref().ok()?;
        let starts: Vec<usize> = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(at, _)| at + 1))
            .collect();
        let line_of = |at: usize| starts.partition_point(|start| *start <= at) - 1;
        let mut on_line: BTreeMap<usize, Vec<&Link>> = BTreeMap::new();
        for link in links.iter() {
            on_line.entry(line_of(link.start)).or_default().push(link);
        }
        let mut items_on_line: BTreeMap<usize, Vec<(usize, String)>> = BTreeMap::new();
        for item in items(language, text, value) {
            note(Work::LinkStep);
            if let Some((start, end)) = content(text, item.lines.start, item.lines.end) {
                items_on_line
                    .entry(line_of(start))
                    .or_default()
                    .push((end, item.pointer));
            }
        }
        let depth = |pointer: &str| pointer.matches('/').count();
        let mut paired = Vec::new();
        for (line, mut items) in items_on_line {
            let Some(links) = on_line.get_mut(&line) else {
                continue;
            };
            items.sort_by(|left, right| {
                right
                    .0
                    .cmp(&left.0)
                    .then(depth(&left.1).cmp(&depth(&right.1)))
            });
            links.sort_by(|left, right| {
                right
                    .end
                    .cmp(&left.end)
                    .then(depth(&left.source.pointer).cmp(&depth(&right.source.pointer)))
            });
            paired.extend(
                items
                    .into_iter()
                    .map(|(_, pointer)| pointer)
                    .zip(links.iter().map(|link| link.source.clone())),
            );
        }
        Some(paired)
    }

    /// The links of `text` (in `language`), the text of these links
    /// (`from_text`, in `from_language`) converted: each item takes the
    /// source of the item at its pointer.
    pub(crate) fn converted(
        &mut self,
        from_language: &dyn ConfigLanguage,
        from_text: &str,
        language: &dyn ConfigLanguage,
        text: &str,
    ) -> Links {
        let sources: HashMap<String, Arc<Source>> = self
            .paired(from_language, from_text)
            .unwrap_or_default()
            .into_iter()
            .collect();
        let parsed = parse_cache::parsed(language, text);
        let Ok(value) = parsed.as_ref() else {
            return Links::default();
        };
        let mut links: Vec<Link> = items(language, text, value)
            .into_iter()
            .filter_map(|item| {
                let source = sources.get(&item.pointer)?.clone();
                let (start, end) = content(text, item.lines.start, item.lines.end)?;
                Some(Link { start, end, source })
            })
            .collect();
        links.sort_by(|left, right| left.start.cmp(&right.start).then(right.end.cmp(&left.end)));
        Links::Kept(Arc::new(links))
    }
}
