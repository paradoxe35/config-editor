use leptos::prelude::*;
use leptos_config_editor::{ConfigEditor, ConfigEditorToolbar, EditorTheme, Language};

const YAML_SAMPLE: &str = "# Burrow server configuration\nlisten: 7835\npublic_address: tunnel.example.com\ninsecure_plaintext: false\n";
const JSON_SAMPLE: &str = "{\n  \"listen\": 7835,\n  \"public_address\": \"tunnel.example.com\",\n  \"insecure_plaintext\": false\n}\n";
const TOML_SAMPLE: &str = "# Burrow server configuration\nlisten = 7835\npublic_address = \"tunnel.example.com\"\ninsecure_plaintext = false\n";
const SCHEMA: &str = include_str!("../schema.json");

fn main() {
    leptos::mount::mount_to_body(|| {
        // `?language=json` opens the same burrow config as JSON; YAML is the default.
        let search = web_sys::window()
            .and_then(|window| window.location().search().ok())
            .unwrap_or_default();
        let has_flag = |flag: &str| {
            search
                .trim_start_matches('?')
                .split('&')
                .any(|pair| pair == flag)
        };
        let (language, sample, file_name) = if has_flag("language=json") {
            (Language::Json, JSON_SAMPLE, "burrow.json")
        } else if has_flag("language=toml") {
            (Language::Toml, TOML_SAMPLE, "burrow.toml")
        } else {
            (Language::Yaml, YAML_SAMPLE, "burrow.yaml")
        };
        let baseline = RwSignal::new(sample.to_string());
        let final_value = RwSignal::new(sample.to_string());
        let theme = RwSignal::new(EditorTheme::VsCodeDark);
        // `?readonly=1` renders the demo editor in read-only view mode.
        let read_only = web_sys::window()
            .and_then(|window| window.location().search().ok())
            .is_some_and(|search| {
                search
                    .trim_start_matches('?')
                    .split('&')
                    .any(|pair| pair == "readonly" || pair == "readonly=1")
            });
        view! {
            <div
                class="config-editor config-editor__demo"
                data-theme=move || theme.get().css_family()
                data-color-theme=move || theme.get().as_str()
            >
                <ConfigEditorToolbar class="config-editor__demo-toolbar" theme=theme>
                    <div class="config-editor__file">
                        <span class="config-editor__file-icon">{language.file_icon()}</span>
                        <span>{file_name}</span>
                    </div>
                    <label class="config-editor__demo-theme-picker">
                        "Theme"
                        <select
                            class="config-editor__demo-theme-select"
                            aria-label="Editor theme"
                            on:change=move |event| {
                                theme.set(match event_target_value(&event).as_str() {
                                    "vscode-light" => EditorTheme::VsCodeLight,
                                    "obsidian" => EditorTheme::Obsidian,
                                    "obsidian-light" => EditorTheme::ObsidianLight,
                                    "dracula" => EditorTheme::Dracula,
                                    "nord" => EditorTheme::Nord,
                                    "atom-one-dark" => EditorTheme::AtomOneDark,
                                    "github-light" => EditorTheme::GitHubLight,
                                    "github-dark" => EditorTheme::GitHubDark,
                                    "solarized-light" => EditorTheme::SolarizedLight,
                                    "vira-inspired" => EditorTheme::ViraInspired,
                                    _ => EditorTheme::VsCodeDark,
                                });
                            }
                        >
                            <option value="vscode-dark">"VS Code Dark"</option>
                            <option value="vscode-light">"VS Code Light"</option>
                            <option value="obsidian">"Obsidian Dark"</option>
                            <option value="obsidian-light">"Obsidian Light"</option>
                            <option value="dracula">"Dracula"</option>
                            <option value="nord">"Nord"</option>
                            <option value="atom-one-dark">"Atom One Dark"</option>
                            <option value="github-light">"GitHub Light"</option>
                            <option value="github-dark">"GitHub Dark"</option>
                            <option value="solarized-light">"Solarized Light"</option>
                            <option value="vira-inspired">"Vira-inspired (original)"</option>
                        </select>
                    </label>
                    <button
                        class="config-editor__action-button"
                        type="button"
                        disabled=move || baseline.get() == final_value.get()
                        on:click=move |_| baseline.set(final_value.get())
                    >
                        "Apply changes"
                    </button>
                </ConfigEditorToolbar>
                <ConfigEditor
                    language=language
                    initial_value=sample
                    baseline=baseline
                    value=final_value
                    on_change=Callback::new(move |value| final_value.set(value))
                    schema_json=SCHEMA
                    file_name=file_name
                    theme=theme
                    read_only=read_only
                    show_toolbar=false
                    show_footer=true
                />
            </div>
        }
    })
}
