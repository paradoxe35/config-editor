//! A small host, for measuring what the editor adds to an app's wasm
//! (`scripts/wasm-sizes.sh`): one editor with a schema, the toolbar and
//! the footer, its language picked at run time from every built-in language
//! of the build (so each language feature counts, as in a host that offers
//! them all), and its schema's format likewise from every built-in format
//! (so `xsd` counts). Code a host does not use (`ConfigDocuments`, the handle's
//! methods, the headless functions) is left out by the linker.

use config_editor_leptos::{ConfigEditor, EditorSettings, Language, SchemaFormat, SchemaSource};
use leptos::prelude::*;

fn main() {
    leptos::mount::mount_to_body(|| {
        // Opaque to the optimizer: any language may be picked.
        let index = std::hint::black_box(0);
        let language = Language::ALL.get(index).copied().unwrap_or_default();
        let format = SchemaFormat::ALL.get(index).copied().unwrap_or_default();
        let schema =
            SchemaSource::formatted(format, r#"{"properties":{"listen":{"type":"integer"}}}"#);
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
                schema=schema
                settings=settings
            />
        }
    });
}
