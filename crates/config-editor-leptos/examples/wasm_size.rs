//! A small host, for measuring what the editor adds to an app's wasm
//! (`scripts/wasm-sizes.sh`): one editor with a schema, the toolbar and
//! the footer, its language picked at run time from every built-in language
//! of the build (so each language feature counts, as in a host that offers
//! them all). Code a host does not use (`ConfigDocuments`, the handle's
//! methods, the headless functions) is left out by the linker.

use config_editor_leptos::{ConfigEditor, EditorSettings, Language};
use leptos::prelude::*;

fn main() {
    leptos::mount::mount_to_body(|| {
        // Opaque to the optimizer: any language may be picked.
        let index = std::hint::black_box(0);
        let language = Language::ALL.get(index).copied().unwrap_or_default();
        let text = RwSignal::new("listen: 7835\n".to_string());
        let settings = EditorSettings {
            show_footer: true,
            show_toolbar: true,
            ..EditorSettings::default()
        };
        view! {
            <ConfigEditor
                language=language
                value=text
                on_change=move |t| text.set(t)
                schema=r#"{"properties":{"listen":{"type":"integer"}}}"#
                settings=settings
            />
        }
    });
}
