//! `?documents`: several files in one view (`ConfigDocuments`): a main
//! config and two drop-ins in other languages, one of them read-only.
//! `?documents=many` opens ten large (~2000-line) documents instead, for
//! memory measurements.
//!
//! The host keeps the list, saves (the baseline becomes the saved text),
//! and asks before closing a document with unsaved edits. What it saw is
//! in the status line's `data-*` attributes, for the e2e tests.

use config_editor_leptos::{
    CloseRequest, ConfigDocuments, Document, DocumentsHandle, EditorSettings, Language, SaveRequest,
};
use leptos::prelude::*;

use crate::settings_panel::SettingsPanel;
use crate::theme_picker::ThemePicker;

const SCHEMA: &str = include_str!("../schema.json");
const MAIN: &str = "# Burrow server configuration\nlisten: 7835\npublic_address: tunnel.example.com\ninsecure_plaintext: false\nlimits:\n  handshake_timeout_secs: 5\n  max_handshakes: 4096\n  max_pending: 512\nheartbeat:\n  interval_secs: 15\n";
const LIMITS: &str =
    "# Drop-in: connection limits\n[limits]\nmax_handshakes = 2048\nmax_pending = 512\n";
const CLIENTS: &str = "{\n  \"clients\": {\n    \"acme\": {\n      \"secret\": \"c2VjcmV0LWZvci1hY21l\"\n    }\n  }\n}\n";

fn initial_documents(many: bool) -> Vec<Document> {
    if many {
        return many_documents();
    }
    vec![
        Document::new("main", "burrow.yaml", MAIN).schema(SCHEMA),
        Document::new("limits", "conf.d/limits.toml", LIMITS).schema(SCHEMA),
        Document::new("clients", "conf.d/clients.json", CLIENTS)
            .schema(SCHEMA)
            .read_only(true),
    ]
}

/// Ten ~2000-line documents, two per language, sharing one schema.
fn many_documents() -> Vec<Document> {
    let large = [
        (
            "yaml",
            include_str!("../crates/config-editor-leptos/tests/fixtures/samples/burrow.large.yaml"),
        ),
        (
            "json",
            include_str!("../crates/config-editor-leptos/tests/fixtures/samples/burrow.large.json"),
        ),
        (
            "toml",
            include_str!("../crates/config-editor-leptos/tests/fixtures/samples/burrow.large.toml"),
        ),
        (
            "hcl",
            include_str!("../crates/config-editor-leptos/tests/fixtures/samples/burrow.large.hcl"),
        ),
        (
            "xml",
            include_str!("../crates/config-editor-leptos/tests/fixtures/samples/burrow.large.xml"),
        ),
    ];
    (0..10)
        .map(|index| {
            let (extension, text) = large[index % large.len()];
            Document::new(
                format!("doc{index}"),
                format!("burrow-{index}.{extension}"),
                text,
            )
            .schema(SCHEMA)
        })
        .collect()
}

fn with_document(documents: RwSignal<Vec<Document>>, id: &str, change: impl FnOnce(&mut Document)) {
    documents.update(|list| {
        if let Some(document) = list.iter_mut().find(|document| document.id == id) {
            change(document);
        }
    });
}

/// Every document's text, as the host holds it.
fn values(documents: RwSignal<Vec<Document>>) -> String {
    documents.with(|list| {
        list.iter()
            .map(|document| {
                let language = Language::label(document.resolved_language());
                format!(
                    "--- {} ({language})\n{}",
                    document.file_name, document.value
                )
            })
            .collect()
    })
}

pub fn view(many: bool) -> impl IntoView {
    let documents = RwSignal::new(initial_documents(many));
    let tabs = DocumentsHandle::new();
    let saves = RwSignal::new(Vec::<String>::new());
    let selects = RwSignal::new(Vec::<String>::new());
    let dirty = RwSignal::new(Vec::<String>::new());
    // A dirty document the user asked to close, waiting for "Discard".
    let asking = RwSignal::new(None::<String>);
    let added = RwSignal::new(0usize);
    let settings = RwSignal::new(EditorSettings {
        show_footer: true,
        ..EditorSettings::default()
    });

    let changed = move |(id, text): (String, String)| {
        with_document(documents, &id, |document| document.value = text);
    };
    let saved = move |(id, request): (String, SaveRequest)| {
        saves.update(|all| all.push(format!("{id}:{}", request.language.label())));
        with_document(documents, &id, |document| {
            document.baseline = Some(request.text)
        });
    };
    let dirty_changed = move |(id, now): (String, bool)| {
        dirty.update(|all| {
            all.retain(|known| *known != id);
            if now {
                all.push(id);
            }
        });
    };
    let close = move |id: &str| documents.update(|list| list.retain(|document| document.id != id));
    let close_requested = move |request: CloseRequest| {
        if request.dirty {
            asking.set(Some(request.id));
        } else {
            close(&request.id);
        }
    };
    let discard = move |_| {
        if let Some(id) = asking.get() {
            close(&id);
        }
        asking.set(None);
    };
    let keep = move |_| asking.set(None);
    let add = move |_| {
        added.update(|count| *count += 1);
        let count = added.get_untracked();
        let document = Document::new(
            format!("extra{count}"),
            format!("conf.d/extra-{count}.hcl"),
            "# Another drop-in\nlisten = 7835\n",
        )
        .schema(SCHEMA);
        documents.update(|list| list.push(document));
    };
    let server_change = move |_| {
        with_document(documents, "main", |document| {
            let base = document
                .baseline
                .clone()
                .unwrap_or_else(|| MAIN.to_string());
            document.baseline = Some(base.replace("7835", "7836"));
        });
    };
    let toggle_edit_as = move |_| {
        with_document(documents, "main", |document| {
            document.edit_as = match document.edit_as {
                Some(_) => None,
                None => Some(Language::JSON),
            };
        });
    };
    let toggle_read_only = move |_| {
        with_document(documents, "limits", |document| {
            document.read_only = !document.read_only
        });
    };
    let rename = move |_| {
        with_document(documents, "limits", |document| {
            document.file_name = "conf.d/10-limits.toml".to_string();
        });
    };
    let save_all = move |_| {
        tabs.save_all();
    };
    let selected = move |id: String| selects.update(|all| all.push(id));
    let theme = move || settings.with(|settings| settings.theme);

    // The page takes the editor's theme (its variables) for its own controls.
    view! {
        <div
            class="config-editor-theme demo demo-documents"
            data-theme=move || theme().css_family()
            data-color-theme=move || theme().as_str()
        >
            <div class="demo__documents-actions">
                <ThemePicker settings=settings />
                <SettingsPanel settings=settings />
                <button class="demo__button demo__save-all" type="button" on:click=save_all>"Save all"</button>
                <button class="demo__button demo__add" type="button" on:click=add>"Add drop-in"</button>
                <button class="demo__button demo__server-change" type="button" on:click=server_change>"Server change (main)"</button>
                <button class="demo__button demo__toggle-read-only" type="button" on:click=toggle_read_only>"Toggle read-only (limits)"</button>
                <button class="demo__button demo__edit-as" type="button" on:click=toggle_edit_as>"Edit main as JSON"</button>
                <button class="demo__button demo__rename" type="button" on:click=rename>"Rename limits"</button>
            </div>
            <Show when=move || asking.get().is_some()>
                <div class="demo__ask" role="alertdialog" aria-label="Unsaved changes">
                    {move || format!("Discard unsaved changes to {}?", asking.get().unwrap_or_default())}
                    <button class="demo__button demo__discard" type="button" on:click=discard>"Discard"</button>
                    <button class="demo__button demo__keep" type="button" on:click=keep>"Cancel"</button>
                </div>
            </Show>
            <ConfigDocuments
                documents=documents
                handle=tabs
                on_change=changed
                on_save=saved
                on_dirty_change=dirty_changed
                on_close=close_requested
                on_select=selected
                settings=settings
            />
            <p
                class="demo__documents-status"
                data-saves=move || saves.get().join(",")
                data-selects=move || selects.get().join(",")
                data-dirty=move || dirty.get().join(",")
                data-open=move || documents.with(|list| list.iter().map(|document| document.id.clone()).collect::<Vec<_>>().join(","))
                data-active=move || tabs.active().unwrap_or_default()
            >
                {move || format!("Active: {}", tabs.active().unwrap_or_default())}
            </p>
            {(!many).then(|| view! { <pre class="demo__documents-values">{move || values(documents)}</pre> })}
        </div>
    }
}
