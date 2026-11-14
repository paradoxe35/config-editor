//! The read-back check for the markup no view shows (`declarations`): a
//! save reads back as the view's value whatever start tag holds an
//! `xsi:type` or a namespace declaration, yet XML readers read the
//! element otherwise. So each element of a saved text must hold the markup
//! of the host element it is (`identity`), and an element the edit added
//! none but the declaration of its name; an element given content is no
//! longer `xsi:nil`. Each element keeps its name's prefix, and one added
//! takes its siblings' (`written_names`).

use super::attribute_layout::{filled, markup_attributes};
use super::identity::{self, Host, Identity};
use super::*;

/// A host element as the check reads it: its markup attributes (name,
/// value), whether it holds anything, and the prefix of its name.
#[derive(Clone, Default)]
struct HostElement {
    markup: Vec<(String, String)>,
    filled: bool,
    prefix: Option<String>,
}

impl HostElement {
    /// The name an element added beside it may take: its prefix, and the
    /// declaration binding it where this one declares it.
    fn name(&self) -> Name {
        let binding = self.markup.iter().filter(|(name, _)| written_names::binds(self.prefix.as_deref(), name)).cloned().collect();
        (self.prefix.clone(), binding)
    }
}

/// An element's name as the check reads it: its prefix, and the
/// declaration binding it on its own start tag.
type Name = (Option<String>, Vec<(String, String)>);

/// The elements of a host text, by their paths, the names elements added
/// to each path may take, and whether any holds markup or a prefix.
struct HostElements {
    markup: declarations::Markup,
    elements: HashMap<Vec<String>, HostElement>,
    names: HashMap<Vec<String>, Vec<Name>>,
    children: written_names::Children,
    marked: bool,
}

impl HostElements {
    fn of(text: &str) -> Self {
        let markup = declarations::Markup::of(text);
        let tokens = tokenize(text);
        let plan = plan_paths(text, &tokens);
        let mut read = HostElements { markup, elements: HashMap::new(), names: HashMap::new(), children: written_names::Children::default(), marked: false };
        for token in &tokens {
            if !matches!(token.kind, Kind::OpenTag { closing: false, .. }) {
                continue;
            }
            let Some(path) = plan.path_of(token.start) else { continue };
            let element = HostElement {
                markup: markup_attributes(text, token, &read.markup),
                filled: filled(text, token),
                prefix: written_names::prefix(text, token).map(str::to_string),
            };
            read.marked |= !element.markup.is_empty() || element.prefix.is_some();
            read.children.note(path, element.prefix.as_deref(), !element.name().1.is_empty());
            read.names.entry(unindexed(path)).or_default().push(element.name());
            read.elements.insert(strings(path), element);
        }
        read
    }
}

/// Whether an element of `text` holds markup or a prefixed name.
pub(super) fn holds_markup(text: &str) -> bool {
    HostElements::of(text).marked
}

/// The pointer of the first element of `patched` (a save's text for
/// `previous`) holding other markup or another name's prefix than the
/// host element it is, or, for one the edit added, than its siblings'.
/// Which host element each is, the check reads from the edit
/// (`item_sources`) and the saved text, not from what the writer chose:
/// a writer pairing items otherwise is caught.
pub(super) fn changed_markup(language: &XmlBehavior, previous: &str, patched: &str) -> Option<String> {
    let host = HostElements::of(previous);
    // A view writes no markup (`declarations::first_markup_key`) and no
    // prefix.
    if !host.marked {
        return None;
    }
    let old = crate::editor::parse_cache::parsed(language, previous);
    let new = crate::editor::parse_cache::parsed(language, patched);
    let (Ok(old), Ok(new)) = (old.as_ref(), new.as_ref()) else {
        return None;
    };
    let sources = crate::editor::item_sources::told(previous, |held| identity::alike(held, new));
    let origin = sources.as_ref().filter(|sources| sources.holds_origin_items()).map(|sources| sources.origin.clone());
    let first_value = origin.as_deref().map(|origin| crate::editor::parse_cache::parsed(language, origin));
    let first = origin.as_deref().map(HostElements::of);
    let identity = Identity::of(Some(old), new)
        .told(sources.as_deref(), first_value.as_ref().and_then(|first| first.as_ref().as_ref().ok()));
    let saved_markup = declarations::Markup::of(patched);
    let tokens = tokenize(patched);
    let plan = plan_paths(patched, &tokens);
    // The prefix of each element open, innermost last.
    let mut open: Vec<Option<&str>> = Vec::new();
    for token in &tokens {
        match token.kind {
            Kind::EndTag => {
                open.pop();
                continue;
            }
            Kind::OpenTag { closing: false, .. } => {}
            _ => continue,
        }
        let in_parent = open.last().copied().flatten();
        if !patched[..token.end].ends_with("/>") {
            open.push(written_names::prefix(patched, token));
        }
        let Some(path) = plan.path_of(token.start) else { continue };
        let saved = markup_attributes(patched, token, &saved_markup);
        let prefix = written_names::prefix(patched, token).map(str::to_string);
        let own = identity.host_path(path).and_then(|(path, side)| {
            let elements = match side {
                Host::Previous => &host,
                Host::Origin => first.as_ref()?,
            };
            Some((elements, elements.elements.get(&path)?))
        });
        let kept = match own {
            Some((elements, own)) => {
                let mut wanted = own.markup.clone();
                // An empty element given content is no longer nil.
                if !own.filled && filled(patched, token) {
                    wanted.retain(|(name, value)| !elements.markup.nil(name, value));
                }
                saved == wanted && prefix == own.prefix
            }
            // Added: named as one of its path's host elements, or where
            // the host has none, in its siblings' namespace, else its
            // parent's; no other markup.
            None => {
                let name = (prefix, saved);
                match host.names.get(&unindexed(path)) {
                    Some(allowed) => allowed.contains(&name),
                    None => {
                        let parent = identity.host_path(written_names::parent(path)).filter(|(_, side)| *side == Host::Previous);
                        let wanted = host.children.new_name_prefix(parent.as_ref().map(|(path, _)| path.as_slice()), in_parent);
                        name == (wanted.map(str::to_string), Vec::new())
                    }
                }
            }
        };
        if !kept {
            let segments: Vec<String> = strings(path).iter().map(|segment| segment.replace('~', "~0").replace('/', "~1")).collect();
            return Some(segments.iter().map(|segment| format!("/{segment}")).collect());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{XmlBehavior, changed_markup};

    const XSI: &str = "xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"";

    fn shapes(items: &[(&str, &str)]) -> String {
        let mut text = format!("<l {XSI}>\n");
        for (markup, size) in items {
            let space = if markup.is_empty() { "" } else { " " };
            text.push_str(&format!("  <s{space}{markup}><n>{size}</n></s>\n"));
        }
        text + "</l>\n"
    }

    #[test]
    fn markup_left_on_another_item_than_its_own_is_caught() {
        let host = shapes(&[("xsi:type=\"A\"", "1"), ("xsi:type=\"B\"", "2"), ("xsi:type=\"C\"", "3")]);
        let check = |saved: &str| changed_markup(&XmlBehavior, &host, saved);
        // The middle item removed: the third keeps its own type.
        assert_eq!(check(&shapes(&[("xsi:type=\"A\"", "1"), ("xsi:type=\"C\"", "3")])), None);
        assert_eq!(check(&shapes(&[("xsi:type=\"A\"", "1"), ("xsi:type=\"B\"", "3")])), Some("/l/s/1".into()));
        // An item added holds none; moved, each keeps its own.
        let added = [("xsi:type=\"A\"", "1"), ("", "9"), ("xsi:type=\"B\"", "2"), ("xsi:type=\"C\"", "3")];
        assert_eq!(check(&shapes(&added)), None);
        let copied = [("xsi:type=\"A\"", "1"), ("xsi:type=\"A\"", "9"), ("xsi:type=\"B\"", "2"), ("xsi:type=\"C\"", "3")];
        assert_eq!(check(&shapes(&copied)), Some("/l/s/1".into()));
        assert_eq!(check(&shapes(&[("xsi:type=\"B\"", "2"), ("xsi:type=\"A\"", "1"), ("xsi:type=\"C\"", "3")])), None);
        // Markup lost is caught too.
        assert_eq!(check(&shapes(&[("xsi:type=\"A\"", "1"), ("", "2"), ("xsi:type=\"C\"", "3")])), Some("/l/s/1".into()));
    }

    #[test]
    fn namespaces_and_nil_follow_their_items() {
        let host = "<r>\n  <i xmlns=\"urn:a\"><v>1</v></i>\n  <i xmlns=\"urn:b\"><v>2</v></i>\n</r>\n";
        let check = |saved: &str| changed_markup(&XmlBehavior, host, saved);
        assert_eq!(check("<r>\n  <i xmlns=\"urn:b\"><v>2</v></i>\n</r>\n"), None);
        assert_eq!(check("<r>\n  <i xmlns=\"urn:a\"><v>2</v></i>\n</r>\n"), Some("/r/i".into()));
        // A renamed element keeps its own.
        assert_eq!(check("<r>\n  <j xmlns=\"urn:a\"><v>1</v></j>\n  <j xmlns=\"urn:b\"><v>2</v></j>\n</r>\n"), None);
        let nil = format!("<r {XSI}>\n  <e xsi:nil=\"true\"/>\n  <e><v>2</v></e>\n</r>\n");
        let check = |saved: &str| changed_markup(&XmlBehavior, &nil, saved);
        // Given content, an element is no longer nil.
        assert_eq!(check(&format!("<r {XSI}>\n  <e><v>1</v></e>\n  <e><v>2</v></e>\n</r>\n")), None);
        let filled_nil = format!("<r {XSI}>\n  <e xsi:nil=\"true\"><v>1</v></e>\n  <e><v>2</v></e>\n</r>\n");
        assert_eq!(check(&filled_nil), Some("/r/e/0".into()));
        // Moved, it stays nil; left empty, it must.
        assert_eq!(check(&format!("<r {XSI}>\n  <e><v>2</v></e>\n  <e xsi:nil=\"true\"/>\n</r>\n")), None);
        assert_eq!(check(&format!("<r {XSI}>\n  <e/>\n  <e><v>2</v></e>\n</r>\n")), Some("/r/e/0".into()));
    }
}
