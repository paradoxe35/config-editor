//! A map or list an edit moved or copied (`item_sources` tells which)
//! from where a host text wrote it on one line (a TOML inline table, an
//! HCL object attribute `tags = { … }`, a YAML flow map) is written on
//! one line where it lands, as it was written: the host writer would
//! write it in its own form (a `[table]`, an object over lines), another
//! form than the host's, which the line patch could not place either.

use super::super::super::item_sources::{ItemSource, ItemSources};
use super::super::super::language::ConfigLanguage;
use super::{Kept, braced_one_line, marker};
use crate::editor::paths::FieldPath;
use crate::editor::text::slice;
use serde_json::Value;

/// A map or list of the save that an edit brought from another place.
struct Brought<'a> {
    /// Its pointer in the save.
    to: &'a str,
    /// Its pointer in the host text it came from.
    from: &'a str,
    /// Which text that is: the host's last text (else the earlier one
    /// the links name).
    in_previous: bool,
}

/// `kept` (the containers kept where the host writes them) with each map
/// or list `new` holds where the edit moved or copied it to, from a
/// host text writing it on one line, behind a marker of its own written
/// back as that text.
pub(super) fn with_moved(
    language: &dyn ConfigLanguage,
    previous: &str,
    old: &Value,
    new: &Value,
    kept: Option<Kept>,
    sources: Option<&ItemSources>,
) -> Option<Kept> {
    // Pointers name the value the save writes; each is checked to hold a
    // map or list where its source does, so a step of a save made in
    // several (over the text the one before wrote) reads them too (the
    // read-back checks the rest).
    let Some(sources) = sources else {
        return kept;
    };
    // Those not where they were (there, the patch keeps them as written,
    // or rewrites them in place), parents before what they hold.
    let mut brought: Vec<Brought<'_>> = sources
        .items
        .iter()
        .filter(|(to, source)| source.pointer() != to.as_str() || old.pointer(to).is_none())
        .filter(|(to, _)| new.pointer(to).is_some_and(filled))
        .map(|(to, source)| Brought {
            to,
            from: source.pointer(),
            in_previous: matches!(source, ItemSource::Previous(_)),
        })
        .collect();
    if brought.is_empty() {
        return kept;
    }
    brought.sort_by(|left, right| left.to.cmp(right.to));
    let mut marks: Vec<(&str, String)> = Vec::new();
    for in_previous in [true, false] {
        let text = if in_previous {
            sources.previous.as_str()
        } else {
            sources.origin.as_str()
        };
        let here: Vec<&Brought<'_>> = brought
            .iter()
            .filter(|item| item.in_previous == in_previous)
            .collect();
        if here.is_empty() {
            continue;
        }
        let read = (text != previous)
            .then(|| super::super::read(language, text, None).ok())
            .flatten();
        let Some(value) = (if text == previous {
            Some(old)
        } else {
            read.as_ref()
        }) else {
            continue;
        };
        // Looked up together: one read of the text for all of them.
        let paths: Vec<FieldPath> = here.iter().map(|item| FieldPath::from(item.from)).collect();
        let spans = language.value_spans(text, &paths);
        for (item, span) in here.into_iter().zip(spans) {
            let Some(now) = new.pointer(item.to) else {
                continue;
            };
            let Some(was) = value.pointer(item.from).filter(|was| {
                was.is_object() == now.is_object() && was.is_array() == now.is_array()
            }) else {
                continue;
            };
            let Some(range) = span.and_then(|span| braced_one_line(language, text, span)) else {
                continue;
            };
            // Edited since (a copy changed inside before a pause): its
            // untouched entries as written, the others like them.
            let Some(host) = slice::slice(text, range) else {
                continue;
            };
            let written = if was == now {
                Some(host.to_string())
            } else {
                super::super::flow::restyle(language, now, host)
            };
            if let Some(written) = written {
                marks.push((item.to, written));
            }
        }
    }
    if marks.is_empty() {
        return kept;
    }
    marks.sort_by(|left, right| left.0.cmp(right.0));
    let mut kept = kept.unwrap_or_else(|| Kept {
        host: previous.to_string(),
        old: old.clone(),
        new: new.clone(),
        texts: Vec::new(),
    });
    let mut marked: Vec<&str> = Vec::new();
    for (to, text) in marks {
        // What one marked holds is written with it.
        let inside = marked.iter().any(|outer| {
            to.strip_prefix(*outer)
                .is_some_and(|rest| rest.starts_with('/'))
        });
        if inside {
            continue;
        }
        // Unless the in-place pass marked it already (a copy kept there).
        let Some(slot) = kept.new.pointer_mut(to).filter(|slot| filled(slot)) else {
            continue;
        };
        let name = marker(kept.texts.len());
        *slot = Value::String(name.clone());
        kept.texts.push((name, String::new(), text.clone(), text));
        marked.push(to);
    }
    Some(kept)
}

/// Whether `value` is a map or list with something in it.
fn filled(value: &Value) -> bool {
    match value {
        Value::Object(map) => !map.is_empty(),
        Value::Array(items) => !items.is_empty(),
        _ => false,
    }
}
