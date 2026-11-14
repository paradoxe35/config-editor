//! The read-back check for the markup no view shows (`declarations`): a
//! save reads back as the view's value whatever start tag holds an
//! `xsi:type` or a namespace declaration, yet XML readers read the
//! element otherwise. So each element of a saved text must hold the markup
//! of the host element it is (`identity`), and an element the edit added
//! none; an element given content is no longer `xsi:nil`.

use super::attribute_layout::{filled, markup_attributes};
use super::identity::Identity;
use super::*;

/// A host element's markup attributes (name, value), and whether it holds
/// anything.
type HostMarkup = (Vec<(String, String)>, bool);

/// The pointer of the first element of `patched` (a save's text for
/// `previous`) holding other markup than the host element it is.
pub(super) fn changed_markup(language: &XmlBehavior, previous: &str, patched: &str) -> Option<String> {
    let markup = declarations::Markup::of(previous);
    let tokens = tokenize(previous);
    let plan = plan_paths(previous, &tokens);
    // Each element's markup, and whether it holds anything.
    let mut host: HashMap<Vec<String>, HostMarkup> = HashMap::new();
    for token in &tokens {
        if !matches!(token.kind, Kind::OpenTag { closing: false, .. }) {
            continue;
        }
        let Some(path) = plan.path_of(token.start) else { continue };
        let held = markup_attributes(previous, token, &markup);
        if !held.is_empty() {
            host.insert(strings(path), (held, filled(previous, token)));
        }
    }
    // A view writes no markup (`declarations::first_markup_key`).
    if host.is_empty() {
        return None;
    }
    let old = crate::editor::parse_cache::parsed(language, previous);
    let new = crate::editor::parse_cache::parsed(language, patched);
    let (Ok(old), Ok(new)) = (old.as_ref(), new.as_ref()) else {
        return None;
    };
    let identity = Identity::of(Some(old), new);
    let saved_markup = declarations::Markup::of(patched);
    let tokens = tokenize(patched);
    let plan = plan_paths(patched, &tokens);
    for token in &tokens {
        if !matches!(token.kind, Kind::OpenTag { closing: false, .. }) {
            continue;
        }
        let Some(path) = plan.path_of(token.start) else { continue };
        let saved = markup_attributes(patched, token, &saved_markup);
        let (mut wanted, was_filled) = identity.host_path(path).and_then(|path| host.get(&path)).cloned().unwrap_or_default();
        // An empty element given content is no longer nil.
        if !was_filled && filled(patched, token) {
            wanted.retain(|(name, value)| !markup.nil(name, value));
        }
        if saved != wanted {
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
