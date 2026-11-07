//! Under the editor: the host's controls (Read-only, a server change, a host
//! edit) and the saved value, what `on_change` sent the host, shown on
//! demand. A button shows it, not a `<details>` (see `settings_panel`).

use leptos::prelude::*;

use crate::settings_panel::toggle;

/// What the host holds and saw, for the panel and the e2e tests.
#[derive(Clone, Copy)]
pub struct HostState {
    pub read_only: RwSignal<bool>,
    /// The text `on_change` last sent.
    pub saved: RwSignal<String>,
    /// The host's baseline (the server's version).
    pub baseline: RwSignal<String>,
    pub dirty: RwSignal<bool>,
    pub saves: RwSignal<usize>,
    pub conflicts: RwSignal<Vec<String>>,
}

/// The server's version after another client changed it: the first number
/// goes up by one (`listen: 7835` becomes `7836`), else a line is added.
pub fn changed_on_server(text: &str) -> String {
    let Some(start) = text.find(|c: char| c.is_ascii_digit()) else {
        return format!("{text}\n");
    };
    let end = text[start..]
        .find(|c: char| !c.is_ascii_digit())
        .map_or(text.len(), |length| start + length);
    let number: u64 = text[start..end].parse().unwrap_or(0);
    format!("{}{}{}", &text[..start], number + 1, &text[end..])
}

fn set_read_only(host: HostState, event: &leptos::ev::Event) {
    host.read_only.set(event_target_checked(event));
}

/// Another client saved: the server's version moves on.
fn server_change(host: HostState) {
    host.baseline.update(|text| *text = changed_on_server(text));
}

/// The host rewrites the value itself.
fn host_edit(host: HostState) {
    host.saved.update(|text| *text = changed_on_server(text));
}

#[component]
pub fn OutputPanel(host: HostState, language: String) -> impl IntoView {
    let open = RwSignal::new(false);
    view! {
        <section class="demo__output">
            <div class="demo__summary">
                <button
                    class="demo__output-toggle"
                    type="button"
                    aria-expanded=move || open.get().to_string()
                    aria-controls="demo-output-text"
                    on:click=move |_| toggle(open)
                >
                    {format!("Saved value ({language}) — what on_change sends the host")}
                </button>
                <span class="demo__host">
                    <label class="demo__check">
                        <input
                            type="checkbox"
                            class="demo__read-only"
                            prop:checked=move || host.read_only.get()
                            on:change=move |event| set_read_only(host, &event)
                        />
                        "Read-only"
                    </label>
                    <button class="demo__button demo__server-change" type="button" on:click=move |_| server_change(host)>
                        "Server change"
                    </button>
                    <button class="demo__button demo__host-edit" type="button" on:click=move |_| host_edit(host)>
                        "Host edit"
                    </button>
                    <span
                        class="demo__host-status"
                        title="Unsaved changes"
                        data-dirty=move || host.dirty.get().to_string()
                        data-saves=move || host.saves.get().to_string()
                        data-conflicts=move || host.conflicts.get().join(",")
                    >
                        {move || if host.dirty.get() { "●" } else { "" }}
                    </span>
                </span>
            </div>
            <pre id="demo-output-text" class="demo__output-text" hidden=move || !open.get()>
                {move || host.saved.get()}
            </pre>
        </section>
    }
}
