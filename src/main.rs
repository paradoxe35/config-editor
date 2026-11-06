use leptos::prelude::*;
use leptos_config_editor::{ConfigEditor, ConfigEditorToolbar, EditorTheme, Language};

const YAML_SAMPLE: &str = "# Burrow server configuration\nlisten: 7835\npublic_address: tunnel.example.com\ninsecure_plaintext: false\n";
const JSON_SAMPLE: &str = "{\n  \"listen\": 7835,\n  \"public_address\": \"tunnel.example.com\",\n  \"insecure_plaintext\": false\n}\n";
const TOML_SAMPLE: &str = "# Burrow server configuration\nlisten = 7835\npublic_address = \"tunnel.example.com\"\ninsecure_plaintext = false\n";
const HCL_SAMPLE: &str = "# Burrow server configuration\nlisten = 7835\npublic_address = \"tunnel.example.com\"\ninsecure_plaintext = false\n";
const XML_SAMPLE: &str = "<listen>7835</listen>\n<public_address>tunnel.example.com</public_address>\n<insecure_plaintext>false</insecure_plaintext>\n<clients>\n  <acme>\n    <secret>c2VjcmV0LWZvci1hY21l</secret>\n  </acme>\n</clients>\n";
const SCHEMA: &str = include_str!("../schema.json");

// The large demo config (~2000 lines), the same document in every language;
// regenerate with `python3 samples/generate.py`.
const YAML_LARGE: &str = include_str!("../samples/burrow.large.yaml");
const JSON_LARGE: &str = include_str!("../samples/burrow.large.json");
const TOML_LARGE: &str = include_str!("../samples/burrow.large.toml");
const HCL_LARGE: &str = include_str!("../samples/burrow.large.hcl");
const XML_LARGE: &str = include_str!("../samples/burrow.large.xml");

/// Demo languages in toolbar order: URL value, label, language, small and
/// large samples.
const LANGUAGES: [(&str, &str, Language, &str, &str); 5] = [
    ("yaml", "YAML", Language::Yaml, YAML_SAMPLE, YAML_LARGE),
    ("json", "JSON", Language::Json, JSON_SAMPLE, JSON_LARGE),
    ("toml", "TOML", Language::Toml, TOML_SAMPLE, TOML_LARGE),
    ("hcl", "HCL", Language::Hcl, HCL_SAMPLE, HCL_LARGE),
    ("xml", "XML", Language::Xml, XML_SAMPLE, XML_LARGE),
];

/// The page's query flags (`language=json`, `sample=large`, `readonly`).
fn query_flags() -> Vec<String> {
    web_sys::window()
        .and_then(|window| window.location().search().ok())
        .unwrap_or_default()
        .trim_start_matches('?')
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(str::to_string)
        .collect()
}

/// The value of `name=value` in the query, if present.
fn query_value(flags: &[String], name: &str) -> Option<String> {
    flags
        .iter()
        .find_map(|pair| pair.strip_prefix(&format!("{name}=")).map(str::to_string))
}

/// Reloads the demo with `name=value` replacing that flag, keeping the others.
fn navigate_with(name: &str, value: &str) {
    let mut flags: Vec<String> = query_flags()
        .into_iter()
        .filter(|pair| !pair.starts_with(&format!("{name}=")))
        .collect();
    flags.push(format!("{name}={value}"));
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_search(&flags.join("&"));
    }
}

fn main() {
    leptos::mount::mount_to_body(|| {
        // `?language=json` opens the same burrow config as JSON (YAML is the
        // default); `?sample=large` loads the ~2000-line config instead of the
        // short one. The toolbar selects rewrite these flags.
        let flags = query_flags();
        let language_key = query_value(&flags, "language").unwrap_or_default();
        let (language_key, _, language, small, large) = LANGUAGES
            .iter()
            .copied()
            .find(|(key, ..)| *key == language_key)
            .unwrap_or(LANGUAGES[0]);
        let large_sample = query_value(&flags, "sample").as_deref() == Some("large");
        let sample = if large_sample { large } else { small };
        let file_name = format!(
            "burrow{}.{language_key}",
            if large_sample { ".large" } else { "" }
        );
        let toolbar_file_name = file_name.clone();
        let schema = SCHEMA;
        let baseline = RwSignal::new(sample.to_string());
        let final_value = RwSignal::new(sample.to_string());
        let theme = RwSignal::new(EditorTheme::VsCodeDark);
        // `?readonly=1` renders the demo editor in read-only view mode.
        let read_only = flags
            .iter()
            .any(|pair| pair == "readonly" || pair == "readonly=1");
        view! {
            <div
                class="config-editor config-editor__demo"
                data-theme=move || theme.get().css_family()
                data-color-theme=move || theme.get().as_str()
            >
                <ConfigEditorToolbar class="config-editor__demo-toolbar" theme=theme>
                    <div class="config-editor__file">
                        <span class="config-editor__file-icon">{language.file_icon()}</span>
                        <span>{toolbar_file_name}</span>
                    </div>
                    <label class="config-editor__demo-picker">
                        "Language"
                        <select
                            class="config-editor__demo-select config-editor__demo-language-select"
                            aria-label="Config language"
                            on:change=move |event| navigate_with("language", &event_target_value(&event))
                        >
                            {LANGUAGES
                                .iter()
                                .map(|(key, label, ..)| {
                                    view! {
                                        <option value=*key selected=*key == language_key>
                                            {*label}
                                        </option>
                                    }
                                })
                                .collect_view()}
                        </select>
                    </label>
                    <label class="config-editor__demo-picker">
                        "Sample"
                        <select
                            class="config-editor__demo-select config-editor__demo-sample-select"
                            aria-label="Sample config"
                            on:change=move |event| navigate_with("sample", &event_target_value(&event))
                        >
                            <option value="small" selected=!large_sample>"Short"</option>
                            <option value="large" selected=large_sample>"Large (~2000 lines)"</option>
                        </select>
                    </label>
                    <label class="config-editor__demo-picker">
                        "Theme"
                        <select
                            class="config-editor__demo-select config-editor__demo-theme-select"
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
                    schema_json=schema
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
