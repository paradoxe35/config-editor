//! Each element of a rewrite as the host element it is (`identity`):
//! named as it (its prefix), its start tag laid out as it and holding its
//! markup (`attribute_layout`), wherever the edit moved it, and an element
//! the edit added named and laid out as its siblings, in their namespace
//! (`written_names`), with none of their other markup.

use super::attribute_layout::{Tag, filled, laid_out};
use super::identity::{Host, Identity};
use super::*;

/// A host start tag.
struct HostTag {
    tag: Tag,
    pairs: Vec<(String, String)>,
    filled: bool,
    /// The prefix of its name (`xs` of `<xs:element>`).
    prefix: Option<String>,
}

/// The start tags of a host text, by their elements' paths.
struct HostTags<'t> {
    text: &'t str,
    markup: declarations::Markup,
    tags: Vec<HostTag>,
    by_path: HashMap<Vec<String>, usize>,
    /// Each element path's tags, by the path without places, in order.
    by_name: HashMap<Vec<String>, Vec<usize>>,
    children: written_names::Children,
}

impl<'t> HostTags<'t> {
    fn of(text: &'t str) -> Self {
        let markup = declarations::Markup::of(text);
        let tokens = tokenize(text);
        let plan = plan_paths(text, &tokens);
        let mut read = HostTags {
            text,
            markup,
            tags: Vec::new(),
            by_path: HashMap::new(),
            by_name: HashMap::new(),
            children: written_names::Children::default(),
        };
        for token in &tokens {
            if !matches!(token.kind, Kind::OpenTag { closing: false, .. }) {
                continue;
            }
            let Some(path) = plan.path_of(token.start) else { continue };
            let tag = Tag::of(text, token, &read.markup);
            let pairs = tag.pairs(text).into_iter().map(|(name, value)| (name.to_string(), value.to_string())).collect();
            read.by_path.insert(strings(path), read.tags.len());
            read.by_name.entry(unindexed(path)).or_default().push(read.tags.len());
            let prefix = written_names::prefix(text, token).map(str::to_string);
            let declares_own = tag
                .attributes
                .iter()
                .any(|attribute| attribute.markup && written_names::binds(prefix.as_deref(), &text[attribute.name.clone()]));
            read.children.note(path, prefix.as_deref(), declares_own);
            read.tags.push(HostTag { tag, pairs, filled: filled(text, token), prefix });
        }
        read
    }

    fn at(&self, path: &[String]) -> Option<&HostTag> {
        self.by_path.get(path).map(|at| &self.tags[*at])
    }

    /// The host tag of the path `name`'s tags an added tag, the
    /// `ordinal`-th written of that path, is laid out like: of those a few
    /// places around it, the one sharing the most attributes with it, then
    /// the nearest.
    fn nearest(&self, name: &[String], ordinal: usize, pairs: &[(&str, &str)]) -> Option<&HostTag> {
        let tags = self.by_name.get(name)?;
        let shared = |host: &HostTag| {
            pairs.iter().filter(|(name, value)| host.pairs.iter().any(|(other, held)| other == name && held == value)).count()
        };
        (ordinal.saturating_sub(2)..tags.len().min(ordinal + 3))
            .max_by_key(|index| (shared(&self.tags[tags[*index]]), std::cmp::Reverse(index.abs_diff(ordinal))))
            .map(|index| &self.tags[tags[index]])
    }
}

/// `written` (a rewrite of `previous`, from the value `identity` edits it
/// to; `origin`, the text the edit session began on, holds the items it
/// pastes back) with each element named and its start tag laid out as the
/// host writes the same element's (see the module documentation): its
/// own, wherever the edit moved it. An element the edit added is laid out
/// as the nearest of its path's, without that one's markup, and named as
/// the item it copies or that nearest one. The attributes keep the order,
/// names and values of `written`.
pub(super) fn with_layout_of(written: &str, (previous, origin): (&str, Option<&str>), identity: &Identity<'_>) -> String {
    let host = HostTags::of(previous);
    let first = origin.map(HostTags::of);
    let tokens = tokenize(written);
    let plan = plan_paths(written, &tokens);
    let mut seen: HashMap<Vec<String>, usize> = HashMap::new();
    let mut out = String::with_capacity(written.len() + previous.len() / 8);
    let mut copied = 0;
    // The prefix given to each element open in `written`, and the prefix
    // it is named with, innermost last.
    let mut open: Vec<(Option<&str>, Option<&str>)> = Vec::new();
    for token in &tokens {
        match token.kind {
            Kind::EndTag => {
                if let Some((Some(prefix), _)) = open.pop() {
                    out.push_str(&written[copied..token.start + 2]);
                    out.push_str(prefix);
                    out.push(':');
                    copied = token.start + 2;
                }
                continue;
            }
            Kind::OpenTag { closing: false, .. } => {}
            _ => continue,
        }
        let closes = written[..token.end].ends_with("/>");
        let Some(path) = plan.path_of(token.start) else {
            if !closes {
                open.push((None, written_names::prefix(written, token)));
            }
            continue;
        };
        let name = unindexed(path);
        let ordinal = {
            let count = seen.entry(name.clone()).or_default();
            *count += 1;
            *count - 1
        };
        let tag = Tag::of(written, token, &host.markup);
        let own = identity.host_path(path).and_then(|(path, side)| match side {
            Host::Previous => Some((&host, host.at(&path)?)),
            Host::Origin => first.as_ref().and_then(|first| Some((first, first.at(&path)?))),
        });
        // A name new among its siblings: in their namespace, else its
        // parent's (QA `E33-3`).
        let mut new_name = None;
        let (like, named, from) = match own {
            // An empty element given content is no longer nil.
            Some((from, own)) if !own.filled && filled(written, token) => (
                Some(own.tag.keeping(|attribute| {
                    !from.markup.nil(&from.text[attribute.name.clone()], &from.text[attribute.value.clone()])
                })),
                Some(own),
                from.text,
            ),
            Some((from, own)) => (Some(own.tag.keeping(|_| true)), Some(own), from.text),
            // An element the edit added: laid out as the nearest of its
            // path's, without that one's markup, and named as the item it
            // copies or that nearest one, in its namespace (with the
            // declaration binding its name, where that one declares it).
            None => {
                let nearest = host.nearest(&name, ordinal, &tag.pairs(written));
                let source = identity.copied(path).and_then(|path| host.at(&path)).or(nearest);
                if source.is_none() {
                    let parent = identity.host_path(written_names::parent(path)).filter(|(_, side)| *side == Host::Previous);
                    let in_parent = open.last().and_then(|(_, named)| *named);
                    new_name = host.children.new_name_prefix(parent.as_ref().map(|(path, _)| path.as_slice()), in_parent);
                }
                let like = nearest.map(|nearest| {
                    let mut like = nearest.tag.keeping(|attribute| !attribute.markup);
                    if let Some(source) = source {
                        let binding = source.tag.keeping(|attribute| {
                            attribute.markup && written_names::binds(source.prefix.as_deref(), &previous[attribute.name.clone()])
                        });
                        like.attributes.splice(0..0, binding.attributes);
                    }
                    like
                });
                (like, source, previous)
            }
        };
        let written_prefix = written_names::prefix(written, token);
        let prefix = named.and_then(|named| named.prefix.as_deref()).or(new_name).filter(|_| written_prefix.is_none());
        if !closes {
            open.push((prefix, prefix.or(written_prefix)));
        }
        out.push_str(&written[copied..token.start + 1]);
        if let Some(prefix) = prefix {
            out.push_str(prefix);
            out.push(':');
        }
        match like {
            Some(like) => {
                out.push_str(&written[token.start + 1..tag.name_end]);
                laid_out(&mut out, (written, &tag), (from, &like));
                copied = token.end;
            }
            None => copied = token.start + 1,
        }
    }
    out.push_str(&written[copied..]);
    out
}
