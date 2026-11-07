use config_editor_leptos::{ConfigEditor, EditorSettings, Language};
use leptos::prelude::*;

fn main() {
    leptos::mount::mount_to_body(|| {
        let text = RwSignal::new("listen: 7835\n".to_string());
        let settings = EditorSettings {
            show_footer: true,
            show_toolbar: true,
            ..EditorSettings::default()
        };
        view! {
            <ConfigEditor
                language=Language::YAML
                value=text
                on_change=move |t| text.set(t)
                schema=r#"{"properties":{"listen":{"type":"integer"}}}"#
                settings=settings
            />
        }
    });
}
