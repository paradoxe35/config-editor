//! `?remount`: a host that remounts the editor in place, as many Leptos
//! apps do: a language picker over `move || view! { <ConfigEditor … /> }`,
//! so each pick rebuilds the editor's view over the elements of the one
//! before. One of the languages, `Lines`, reads documents but cannot write
//! them (no `serialize`), so Edit-as from it is refused; an Edit-as picker
//! beside it sets `edit_as`. The editors mounted so far are counted in the
//! status line's `data-mounts`, for the e2e tests.

use leptos::prelude::*;
use leptos_config_editor::language::{ConfigLanguage, LanguageConfig, LanguageInfo, ParseError};
use leptos_config_editor::{ConfigEditor, EditorSettings, Language};
use serde_json::{Map, Value};

/// `key: value` lines that the editor reads but cannot write.
struct Lines;

impl ConfigLanguage for Lines {
    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: "Lines",
            icon: "L",
            extensions: &["lines"],
        }
    }

    fn config(&self) -> LanguageConfig {
        LanguageConfig {
            separators: &[":"],
            ..LanguageConfig::default()
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        let mut map = Map::new();
        for (index, line) in source.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once(':') else {
                return Err(ParseError {
                    message: "expected key: value".to_string(),
                    line: Some(index + 1),
                    column: None,
                    quotes: None,
                });
            };
            map.insert(key.trim().to_string(), Value::from(value.trim()));
        }
        Ok(Value::Object(map))
    }
}

const LINES: Language = Language::new(&Lines);

/// The languages to pick from, with their samples.
fn natives() -> [(Language, &'static str); 2] {
    [
        (LINES, "name: edge\nlisten: 7835\n"),
        (
            Language::YAML,
            "name: edge\nlisten: 7835\nlimits:\n  max: 10\n",
        ),
    ]
}

pub(crate) fn view() -> impl IntoView {
    let native = RwSignal::new(0usize);
    let text = RwSignal::new(natives()[0].1.to_string());
    let edit_as = RwSignal::new(None::<Language>);
    let mounts = RwSignal::new(0usize);
    let settings = RwSignal::new(EditorSettings {
        show_toolbar: true,
        show_footer: true,
        ..EditorSettings::default()
    });
    let pick_native = move |index: usize| {
        native.set(index);
        text.set(natives()[index].1.to_string());
    };
    view! {
        <div class="demo">
            <label>
                "Language "
                <select
                    class="demo__native-select"
                    on:change=move |event| {
                        if let Ok(index) = event_target_value(&event).parse() {
                            pick_native(index);
                        }
                    }
                >
                    {natives()
                        .into_iter()
                        .enumerate()
                        .map(|(index, (language, _))| {
                            view! { <option value=index.to_string()>{language.label()}</option> }
                        })
                        .collect_view()}
                </select>
            </label>
            <label>
                " Edit as "
                <select
                    class="demo__edit-as-select"
                    on:change=move |event| edit_as.set(event_target_value(&event).parse().ok())
                >
                    <option value="">"Native"</option>
                    <option value="JSON">"JSON"</option>
                    <option value="YAML">"YAML"</option>
                </select>
            </label>
            <p class="demo__remount-status" data-mounts=move || mounts.get().to_string()></p>
            {move || {
                let (language, _) = natives()[native.get()];
                mounts.update(|mounts| *mounts += 1);
                view! {
                    <ConfigEditor
                        language=language
                        edit_as=edit_as
                        value=text
                        on_change=move |updated| text.set(updated)
                        settings=settings
                    />
                }
            }}
        </div>
    }
}
