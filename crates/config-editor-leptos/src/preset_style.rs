//! The preset stylesheet, injected once per page (or shadow root) however
//! many editors and documents views it shows.
//!
//! Each `<style>` with the preset is a stylesheet the browser matches every
//! element against, so a copy per editor made each new editor's first style
//! pass cost more for each editor already shown (with 100 editors shown,
//! the 100th mounted 3× slower than the first, its style pass 10× longer).
//! The views share one `<style>` per root instead, appended to the
//! page's `<head>` (or the shadow root they are in) while one of them
//! shows, removed with the last.
//!
//! The first view on the page also renders its own copy until it has
//! joined the shared one (an effect, once it is on the page), so it is
//! styled the moment it is built, before the host's code after it runs;
//! the views after it find the shared copy there already. Not the copy
//! for each view, even for that moment: adding or removing a stylesheet
//! makes the browser restyle the whole page, every editor on it. A view
//! that is on no page when it joins (a host's element not attached) keeps
//! its own copy.

use std::cell::RefCell;

use leptos::html::Div;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::PRESET_CSS;

/// The attribute the shared `<style>` carries.
const MARK: &str = "data-config-editor-preset";

/// A shared copy: the root it styles (a document or a shadow root), its
/// `<style>` and how many views use it.
struct Sheet {
    root: web_sys::Node,
    style: web_sys::Element,
    holders: usize,
}

thread_local! {
    static SHEETS: RefCell<Vec<Sheet>> = const { RefCell::new(Vec::new()) };
}

/// A view's use of its root's shared copy, given back when dropped.
struct Held(web_sys::Node);

impl Held {
    /// Joins (or makes) the shared copy of `element`'s root; `None` when
    /// `element` is in neither a document nor a shadow root.
    fn take(element: &web_sys::Element) -> Option<Self> {
        let root = element.get_root_node();
        let place: web_sys::Node = if let Some(document) = root.dyn_ref::<web_sys::Document>() {
            document
                .head()
                .map(Into::into)
                .or_else(|| document.document_element().map(Into::into))?
        } else if root.has_type::<web_sys::ShadowRoot>() {
            root.clone()
        } else {
            return None;
        };
        SHEETS.with(|sheets| {
            let mut sheets = sheets.borrow_mut();
            if let Some(sheet) = sheets.iter_mut().find(|sheet| sheet.root == root) {
                // A host that rewrote its head (or shadow root) dropped it.
                if !sheet.style.is_connected() {
                    place.append_child(&sheet.style).ok()?;
                }
                sheet.holders += 1;
            } else {
                let style = element.owner_document()?.create_element("style").ok()?;
                style.set_attribute(MARK, "").ok()?;
                style.set_text_content(Some(PRESET_CSS));
                place.append_child(&style).ok()?;
                sheets.push(Sheet {
                    root: root.clone(),
                    style,
                    holders: 1,
                });
            }
            Some(Self(root))
        })
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        let _ = SHEETS.try_with(|sheets| {
            let mut sheets = sheets.borrow_mut();
            let Some(at) = sheets.iter().position(|sheet| sheet.root == self.0) else {
                return;
            };
            sheets[at].holders -= 1;
            if sheets[at].holders == 0 {
                sheets.swap_remove(at).style.remove();
            }
        });
    }
}

/// Whether a shared copy is on a page already.
fn shared_yet() -> bool {
    SHEETS.with(|sheets| !sheets.borrow().is_empty())
}

/// The preset for the view whose root is `root`, while `styled`: the
/// shared copy of the root's page, and the view's own `<style>` (the
/// returned view) until it has joined it.
pub(crate) fn preset_style(root: NodeRef<Div>, styled: Memo<bool>) -> impl IntoView {
    let own = RwSignal::new(styled.get_untracked() && !shared_yet());
    let held = StoredValue::new_local(None::<Held>);
    Effect::new(move |_| {
        let wanted = styled.get();
        let joined = root
            .get()
            .filter(|_| wanted)
            .and_then(|element| Held::take(&element));
        own.set(wanted && joined.is_none());
        // Joined before the old one goes: the copy stays on the page.
        drop(held.try_update_value(|held| std::mem::replace(held, joined)));
    });
    on_cleanup(move || drop(held.try_update_value(Option::take)));
    move || own.get().then(|| view! { <style>{PRESET_CSS}</style> })
}
