//! The built-in footer: the suggestions hint, problem counts and line count.

use leptos::prelude::*;

use crate::reactive::Kept;

use super::super::a11y::toggle_tab_focus;
use super::super::changes::ChangeStatus;
use super::super::state::{EditorState, line_count};
use super::problem_summary::ProblemCount;
use crate::EditorStrings;
use crate::strings::counted;

#[component]
pub(crate) fn StatusBar(state: EditorState) -> impl IntoView {
    // Follows `read_only`, which switches without remounting.
    // A touch screen has no key to name: it is told that suggestions come as
    // you type, and told nothing when they do not.
    let touch = super::super::touch::touch_screen();
    let offers_completions = move || {
        let settings = state.config().settings;
        settings.completions.get()
            && !state.config().read_only.get()
            && (!touch || settings.auto_trigger.get())
    };
    let line_total = move || {
        let count = state.doc().text.with(|text| line_count(text));
        state.strings(|strings| counted(&strings.lines_one, &strings.lines_other, count))
    };
    view! {
        <div class="config-editor__status">
            <span class="config-editor__hint">
                {move || offers_completions().then(|| view! { <SuggestionsHint state=state /> })}
            </span>
            <span class="config-editor__status-meta">
                <TabFocusStatus state=state />
                <Kept when=state.config().settings.diagnostics view=move || view! { <ProblemCount state=state /> } />
                <Kept when=state.config().settings.diff view=move || view! { <ChangeStatus state=state /> } />
                <span class="config-editor__line-count">{line_total}</span>
            </span>
        </div>
    }
}

/// "Ctrl + Space for suggestions": the `suggestionsHint` text with the
/// `suggestionsKey` chord drawn as keys where it says `{key}`.
#[component]
fn SuggestionsHint(state: EditorState) -> impl IntoView {
    let strings = state.strings_signal();
    let touch = super::super::touch::touch_screen();
    let pieces = move || strings.with(|strings| hint_pieces(strings, touch));
    move || {
        pieces()
            .into_iter()
            .map(|piece| match piece {
                HintPiece::Key(key) => view! { <kbd>{key}</kbd> }.into_any(),
                HintPiece::Text(text) => view! { <span>{text}</span> }.into_any(),
            })
            .collect_view()
    }
}

/// A piece of the suggestions hint.
#[derive(Debug, PartialEq)]
enum HintPiece {
    Key(String),
    Text(String),
}

/// The hint as text and keys: "Ctrl", "+", "Space", "for suggestions"; on a
/// `touch` screen, only its own text.
fn hint_pieces(strings: &EditorStrings, touch: bool) -> Vec<HintPiece> {
    if touch {
        return vec![HintPiece::Text(strings.suggestions_hint_touch.clone())];
    }
    let hint = &strings.suggestions_hint;
    let (before, after) = hint.split_once("{key}").unwrap_or(("", hint.as_str()));
    let mut pieces = Vec::new();
    let text = |text: &str| {
        let text = text.trim();
        (!text.is_empty()).then(|| HintPiece::Text(text.to_string()))
    };
    pieces.extend(text(before));
    for (index, key) in strings.suggestions_key.split('+').enumerate() {
        if index > 0 {
            pieces.push(HintPiece::Text("+".to_string()));
        }
        pieces.push(HintPiece::Key(key.trim().to_string()));
    }
    pieces.extend(text(after));
    pieces
}

/// "Tab moves focus" while `Ctrl+M` is on; clicking it turns it off.
#[component]
fn TabFocusStatus(state: EditorState) -> impl IntoView {
    let turn_off = move |_| toggle_tab_focus(state);
    move || {
        state.a11y().tab_moves_focus.get().then(|| {
            view! {
                <button
                    type="button"
                    class="config-editor__tab-focus-status"
                    title=state.text(|strings| &strings.tab_focus_title)
                    on:click=turn_off
                >
                    {state.text(|strings| &strings.tab_focus)}
                </button>
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{HintPiece, hint_pieces};
    use crate::EditorStrings;

    #[test]
    fn the_hint_draws_its_chord_as_keys_where_it_says() {
        let text = |text: &str| HintPiece::Text(text.to_string());
        let key = |key: &str| HintPiece::Key(key.to_string());
        assert_eq!(
            hint_pieces(&EditorStrings::default(), false),
            [
                key("Ctrl"),
                text("+"),
                key("Space"),
                text("for suggestions")
            ]
        );
        let german = EditorStrings {
            suggestions_hint: "Vorschläge mit {key}".into(),
            suggestions_key: "Strg+Leertaste".into(),
            ..EditorStrings::default()
        };
        assert_eq!(
            hint_pieces(&german, false),
            [
                text("Vorschläge mit"),
                key("Strg"),
                text("+"),
                key("Leertaste")
            ]
        );
    }

    #[test]
    fn a_touch_screens_hint_names_no_key() {
        assert_eq!(
            hint_pieces(&EditorStrings::default(), true),
            [HintPiece::Text(
                "suggestions appear as you type".to_string()
            )]
        );
    }
}
