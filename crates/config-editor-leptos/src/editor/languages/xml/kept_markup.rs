//! The read-back check for the markup no view shows (`declarations`): a
//! save reads back as the view's value whatever start tag holds an
//! `xsi:type` or a namespace declaration, yet XML readers read the
//! element otherwise. So each element of a saved text must hold the markup
//! of the host element it is (`identity`), and an element the edit added
//! none but the declaration of its name; an element given content is no
//! longer `xsi:nil`. Each element keeps its name's prefix, and one added
//! takes its siblings' (`written_names`). A key the host element writes
//! as an attribute stays one, and a child element stays a child (QA
//! `E35-1`: views show both alike).

use super::attribute_layout::{Tag, filled, markup_attributes};
use super::identity::{self, Host, Identity};
use super::*;

/// A host element as the check reads it: its markup attributes (name,
/// value), whether it holds anything, the prefix of its name, and the
/// local names of its other attributes and of its child elements.
#[derive(Clone, Default)]
struct HostElement {
    markup: Vec<(String, String)>,
    filled: bool,
    prefix: Option<String>,
    attributes: Vec<String>,
    children: Vec<String>,
}

impl HostElement {
    /// The name an element added beside it may take: its prefix, and the
    /// declaration binding it where this one declares it.
    fn name(&self) -> Name {
        let binding = self.markup.iter().filter(|(name, _)| written_names::binds(self.prefix.as_deref(), name)).cloned().collect();
        (self.prefix.clone(), binding)
    }

    /// Whether `saved` writes a key this element writes as an attribute
    /// only as a child element, or one it writes only as a child as an
    /// attribute (an attribute and a child of one name are two keys).
    fn form_changed(&self, saved: &HostElement) -> bool {
        let moved = |name: &String, from: (&[String], &[String]), to: (&[String], &[String])| {
            !from.1.contains(name) && to.1.contains(name) && !to.0.contains(name)
        };
        let host = (self.attributes.as_slice(), self.children.as_slice());
        let written = (saved.attributes.as_slice(), saved.children.as_slice());
        self.attributes.iter().any(|name| moved(name, host, written))
            || self.children.iter().any(|name| moved(name, (host.1, host.0), (written.1, written.0)))
    }
}

/// A name without its prefix.
fn local(name: &str) -> String {
    name.rsplit(':').next().unwrap_or(name).to_string()
}

/// An element's name as the check reads it: its prefix, and the
/// declaration binding it on its own start tag.
type Name = (Option<String>, Vec<(String, String)>);

/// The elements of a host text, by their paths, the names elements added
/// to each path may take, whether any holds markup or a prefix, and
/// whether any holds an attribute.
struct HostElements {
    markup: declarations::Markup,
    elements: HashMap<Vec<String>, HostElement>,
    names: HashMap<Vec<String>, Vec<Name>>,
    children: written_names::Children,
    marked: bool,
    attributed: bool,
}

impl HostElements {
    fn of(text: &str) -> Self {
        let markup = declarations::Markup::of(text);
        let tokens = tokenize(text);
        let plan = plan_paths(text, &tokens);
        let mut read = HostElements { markup, elements: HashMap::new(), names: HashMap::new(), children: written_names::Children::default(), marked: false, attributed: false };
        for token in &tokens {
            if !matches!(token.kind, Kind::OpenTag { closing: false, .. }) {
                continue;
            }
            let Some(path) = plan.path_of(token.start) else { continue };
            let attributes: Vec<String> = Tag::of(text, token, &read.markup)
                .attributes
                .iter()
                .filter(|attribute| !attribute.markup)
                .map(|attribute| local(&text[attribute.name.clone()]))
                .collect();
            let element = HostElement {
                markup: markup_attributes(text, token, &read.markup),
                filled: filled(text, token),
                prefix: written_names::prefix(text, token).map(str::to_string),
                attributes,
                children: Vec::new(),
            };
            read.marked |= !element.markup.is_empty() || element.prefix.is_some();
            read.attributed |= !element.attributes.is_empty();
            if let (Some(parent), Some(name)) = (read.elements.get_mut(&strings(written_names::parent(path))), unindexed(path).pop()) {
                parent.children.push(name);
            }
            read.children.note(path, element.prefix.as_deref(), !element.name().1.is_empty());
            read.names.entry(unindexed(path)).or_default().push(element.name());
            read.elements.insert(strings(path), element);
        }
        read
    }

    /// The local names its elements write as attributes, or as child
    /// elements.
    fn names_written(&self, attributes: bool) -> std::collections::HashSet<&str> {
        self.elements
            .values()
            .flat_map(|element| if attributes { &element.attributes } else { &element.children })
            .map(String::as_str)
            .collect()
    }

    /// Whether `saved` may write a key in another form than this text
    /// ([`HostElement::form_changed`]): some name this text writes as an
    /// attribute is a child element there, or the other way. Without one,
    /// no element can have changed form, and no element is compared.
    fn form_may_change(&self, saved: &HostElements) -> bool {
        !self.names_written(true).is_disjoint(&saved.names_written(false))
            || !self.names_written(false).is_disjoint(&saved.names_written(true))
    }
}

/// Whether an element of `text` holds markup, a prefixed name or an
/// attribute: what no view shows.
pub(super) fn holds_markup(text: &str) -> bool {
    let read = HostElements::of(text);
    read.marked || read.attributed
}

/// The pointer of the first element of `patched` (a save's text for
/// `previous`) holding other markup or another name's prefix than the
/// host element it is, or, for one the edit added, than its siblings'.
/// Which host element each is, the check reads from the edit
/// (`item_sources`) and the saved text, not from what the writer chose:
/// a writer pairing items otherwise is caught.
pub(super) fn changed_markup(language: &XmlBehavior, previous: &str, patched: &str) -> Option<String> {
    let host = HostElements::of(previous);
    // A view writes no markup (`declarations::first_markup_key`), no
    // prefix and no attribute, but what a save pastes back from the
    // session's first text holds that text's.
    // Its attributes are then all a save can change: only an attribute
    // or a child taking the other form (no parse, no element compared
    // while none does).
    let pasted_back = || crate::editor::item_sources::told(previous, |_| true).is_some_and(|sources| sources.holds_origin_items());
    let attributes_only = !host.marked && !pasted_back();
    if attributes_only && !host.attributed {
        return None;
    }
    let saved_elements = HostElements::of(patched);
    if attributes_only && !host.form_may_change(&saved_elements) {
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
    let mut open: written_names::Open<Option<&str>> = written_names::Open::default();
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
        let before = open.before().map(|prefix| prefix.map(str::to_string));
        let written_prefix = written_names::prefix(patched, token);
        let saved = markup_attributes(patched, token, &saved_markup);
        open.child(written_prefix, saved.iter().any(|(name, _)| written_names::binds(written_prefix, name)));
        if !patched[..token.end].ends_with("/>") {
            open.push(written_prefix);
        }
        let Some(path) = plan.path_of(token.start) else { continue };
        let prefix = written_prefix.map(str::to_string);
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
                let form_kept = saved_elements.elements.get(&strings(path)).is_none_or(|saved| !own.form_changed(saved));
                saved == wanted && prefix == own.prefix && form_kept
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
                        let wanted = host.children.new_name_prefix(parent.as_ref().map(|(path, _)| path.as_slice()), before.as_ref().map(Option::as_deref), in_parent);
                        name == (wanted, Vec::new())
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

    /// A key the host writes as an attribute saved as a child element (or
    /// the other way) reads back the same in every view, yet is another
    /// document: caught, on any element, in any namespace (QA `E35-1`).
    #[test]
    fn an_attribute_written_as_a_child_or_a_child_as_an_attribute_is_caught() {
        for (host, saved, caught) in [
            ("<r>\n  <s pkg=\"x\"/>\n</r>\n", "<r>\n  <s>\n    <pkg>x</pkg>\n  </s>\n</r>\n", Some("/r/s")),
            ("<r v=\"1\">\n  <s><pkg>x</pkg></s>\n</r>\n", "<r v=\"1\">\n  <s pkg=\"x\"/>\n</r>\n", Some("/r/s")),
            ("<r xmlns:c=\"urn:c\">\n  <c:s c:pkg=\"x\"/>\n</r>\n", "<r xmlns:c=\"urn:c\">\n  <c:s><c:pkg>x</c:pkg></c:s>\n</r>\n", Some("/r/s")),
            ("<r>\n  <s xmlns=\"urn:s\" pkg=\"x\"/>\n</r>\n", "<r>\n  <s xmlns=\"urn:s\"><pkg>x</pkg></s>\n</r>\n", Some("/r/s")),
            // An attribute and a child of one name are two keys.
            ("<r a=\"1\">\n  <a>2</a>\n</r>\n", "<r a=\"1\">\n  <a>3</a>\n</r>\n", None),
            ("<r a=\"1\">\n  <a>2</a>\n</r>\n", "<r>\n  <a>2</a>\n</r>\n", None),
            // Kept, removed or added in its own form: nothing changed form.
            ("<r>\n  <s pkg=\"x\"/>\n</r>\n", "<r>\n  <s pkg=\"y\" q=\"1\"/>\n</r>\n", None),
            ("<r>\n  <s pkg=\"x\"/>\n</r>\n", "<r>\n  <s><q>1</q></s>\n</r>\n", None),
        ] {
            assert_eq!(changed_markup(&XmlBehavior, host, saved).as_deref(), caught, "{host} -> {saved}");
        }
    }

    /// A host whose elements hold attributes but no markup or prefix (the
    /// sample's `config-root`): a save that keeps every key's form is told
    /// apart without parsing either text, at any size.
    #[test]
    fn a_host_with_attributes_only_is_checked_without_a_parse() {
        use crate::editor::work_count::{Work, counted};
        let host = |items: usize, port: usize| {
            let mut text = String::from("<r root=\"true\">\n");
            for item in 0..items {
                text.push_str(&format!("  <s name=\"s{item}\"><port>{}</port></s>\n", item + port));
            }
            text + "</r>\n"
        };
        let sample = include_str!("../../../../tests/fixtures/samples/tunnel.large.xml");
        let edited = sample.replacen("0.0.0.0:7835", "0.0.0.0:7836", 1);
        assert_ne!(sample, edited);
        for (previous, patched) in [(host(10, 0), host(10, 1)), (host(2000, 0), host(2000, 1)), (sample.to_string(), edited)] {
            let (changed, parsed) = counted(Work::ParsedBytes, || changed_markup(&XmlBehavior, &previous, &patched));
            assert_eq!(changed, None);
            assert_eq!(parsed, 0, "{} bytes", previous.len());
        }
        // A key that did change form is still caught.
        let moved = host(10, 0).replacen("<s name=\"s3\"><port>3</port></s>", "<s><name>s3</name><port>3</port></s>", 1);
        assert_eq!(changed_markup(&XmlBehavior, &host(10, 0), &moved).as_deref(), Some("/r/s/3"));
    }
}
