//! The problem tip: the messages of the problems starting where the
//! pointer (or the focus) is on an underline, or where F8 or a tap went.
//! Each problem gets its own lines (its severity, message, who reported it
//! and its code, its docs link), the most serious first. It stays inside
//! the editor and the window: above its line when it fits there, else
//! below, else on the roomier side, no taller than the room there and
//! scrolled (F8 scrolls to the problem it went to), so every message can
//! be reached.

use leptos::html::Div;
use leptos::prelude::*;

use super::super::super::a11y::severity_name;
use super::super::super::completion::visual_column;
use super::super::super::diagnostics::{Diagnostic, DiagnosticOrigin, Severity};
use super::super::super::metrics::read_editor_metrics;
use super::super::super::positioning::{FALLBACK_VIEWPORT_HEIGHT, FALLBACK_VIEWPORT_WIDTH};
use super::super::super::problems::TipAnchor;
use super::super::super::state::EditorState;
use crate::EditorStrings;
use crate::reactive::later;
use wasm_bindgen::JsCast;

/// Widest the tip gets, in pixels.
const TIP_MAX_WIDTH: f64 = 360.0;
/// Narrowest the tip gets, in pixels, however narrow the editor.
const TIP_MIN_WIDTH: f64 = 120.0;
/// Least height given to the tip when neither side has room, in pixels.
const TIP_MIN_HEIGHT: f64 = 48.0;
/// Gap between the line and the tip, in pixels.
const TIP_GAP: f64 = 4.0;
/// Space kept between the tip and the editor's (or window's) edges.
const TIP_MARGIN: f64 = 4.0;

/// What the tip shows.
#[derive(Clone, Debug, PartialEq)]
struct TipContent {
    anchor: TipAnchor,
    entries: Vec<Entry>,
}

/// One problem in the tip.
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    severity: Severity,
    severity_name: String,
    message: String,
    /// Who reported it and its code ("burrow · E_PORT"), for a host's.
    detail: Option<String>,
    docs_url: Option<String>,
    /// The problem F8 went to last (scrolled into view).
    current: bool,
}

#[component]
pub(crate) fn ProblemTip(state: EditorState) -> impl IntoView {
    let tip_ref = NodeRef::<Div>::new();
    let content = Memo::new(move |_| tip_content(state));
    // The tip's full height, measured once its entries are drawn; it
    // decides the side and the height (see `place`).
    let natural = RwSignal::new(0.0_f64);
    let placement = Memo::new(move |_| {
        let anchor = content.with(|content| content.as_ref().map(|content| content.anchor))?;
        placement(state, anchor, natural.get())
    });
    let visible = Memo::new(move |_| placement.with(Option::is_some));
    Effect::new(move |_| {
        content.track();
        if visible.get() {
            // Once its entries and its showing are on the page (before it
            // is painted).
            later::next_frame(state, move |_| measure(tip_ref, natural, true));
        }
    });
    // It takes the pointer only to be reached: with a docs link, or taller
    // than its room (to scroll). Otherwise clicks go through to the text
    // under it, as when there is no tip.
    let interactive = move || {
        let docs = content.with(|content| {
            content
                .as_ref()
                .is_some_and(|content| content.entries.iter().any(|entry| entry.docs_url.is_some()))
        });
        let scrolls =
            placement.with(|placed| placed.is_some_and(|placed| natural.get() > placed.max_height));
        docs || scrolls
    };
    view! {
        <div
            class="config-editor__problem-tip"
            class:config-editor__problem-tip--interactive=interactive
            role="tooltip"
            node_ref=tip_ref
            hidden=move || placement.with(Option::is_none)
            style=move || placement.get().map(|placed| placed.style()).unwrap_or_default()
            on:mouseenter=move |_| state.problems().tip_entered.set_value(true)
            on:mouseleave=move |_| leave_tip(state)
            // Scrolling it or clicking a link keeps the caret.
            on:mousedown=move |event| event.prevent_default()
        >
            {move || content.get().map(|content| entries_view(state, content.entries))}
        </div>
    }
}

/// Measures the tip's full height into `natural`; then (once placed by it,
/// the frame after when it changed, `again`) scrolls the problem F8 went to
/// into view.
fn measure(tip_ref: NodeRef<Div>, natural: RwSignal<f64>, again: bool) {
    let Some(tip) = tip_ref.get_untracked() else {
        return;
    };
    if tip.offset_height() == 0 {
        return;
    }
    // The borders, besides the scrolled content.
    let measured = f64::from(tip.scroll_height() + tip.offset_height() - tip.client_height());
    if natural.try_get_untracked() != Some(measured) && again {
        natural.set(measured);
        later::next_frame(natural, move |natural| measure(tip_ref, natural, false));
        return;
    }
    if let Ok(Some(current)) = tip.query_selector(".config-editor__problem-tip-entry--current")
        && let Ok(current) = current.dyn_into::<web_sys::HtmlElement>()
    {
        let top = current.offset_top();
        let bottom = top + current.offset_height();
        if top < tip.scroll_top() || bottom > tip.scroll_top() + tip.client_height() {
            tip.set_scroll_top(top - 4);
        }
    }
}

/// The pointer left the tip: it closes a moment later, as when it leaves
/// an underline.
fn leave_tip(state: EditorState) {
    state.problems().tip_entered.set_value(false);
    state.leave_problem();
}

/// The tip's entries (built once per content: no reactive reads inside).
fn entries_view(state: EditorState, entries: Vec<Entry>) -> impl IntoView {
    let open_docs = state.strings_untracked(|strings| strings.open_docs.clone());
    entries
        .into_iter()
        .map(|entry| {
            let docs = entry.docs_url.clone();
            let link = entry.docs_url.map(|url| {
                view! {
                    <a
                        class="config-editor__problem-tip-link"
                        href=url
                        target="_blank"
                        rel="noopener noreferrer"
                    >
                        {open_docs.clone()}
                    </a>
                }
            });
            let has_detail = entry.detail.is_some() || link.is_some();
            let separator = (entry.detail.is_some() && link.is_some()).then_some(" · ");
            view! {
                <div
                    class=format!(
                        "config-editor__problem-tip-entry config-editor__problem-tip-entry--{}",
                        entry.severity.as_str(),
                    )
                    class:config-editor__problem-tip-entry--current=entry.current
                >
                    <p
                        class="config-editor__problem-tip-message"
                        class:config-editor__problem-tip-message--docs=docs.is_some()
                        on:click=move |_| open(docs.as_deref())
                    >
                        <span class="config-editor__problem-tip-severity">{entry.severity_name}</span>
                        " "
                        {entry.message}
                    </p>
                    {has_detail.then(|| view! {
                        <p class="config-editor__problem-tip-detail">{entry.detail}{separator}{link}</p>
                    })}
                </div>
            }
        })
        .collect_view()
}

/// A click on a message with docs opens them, as its link does.
fn open(url: Option<&str>) {
    if let (Some(url), Some(window)) = (url, web_sys::window()) {
        let _ = window.open_with_url_and_target_and_features(url, "_blank", "noopener");
    }
}

/// What the tip shows now: the hovered underline's problems, else those
/// where F8 (or a tap) went.
fn tip_content(state: EditorState) -> Option<TipContent> {
    let problems = state.problems();
    let anchor = match problems.hovered.get() {
        Some(anchor) => anchor,
        None => {
            let (line, column) = problems.current.get()?;
            let before: String = state.doc().text.with_untracked(|text| {
                text.split('\n')
                    .nth(line - 1)
                    .unwrap_or("")
                    .chars()
                    .take(column.saturating_sub(1))
                    .collect()
            });
            TipAnchor {
                place: (line, column),
                line,
                column: visual_column(&before),
            }
        }
    };
    let walked = problems.walked.get().map(|(problem, _)| problem);
    let entries = state.config().settings.strings.with(|strings| {
        problems
            .diagnostics
            .with(|found| entries_at(strings, found, anchor.place, walked.as_ref()))
    });
    (!entries.is_empty()).then_some(TipContent { anchor, entries })
}

/// Every problem starting at `place` (all of them, drawn or past the
/// underline cap), the most serious first; `walked` is the one F8 went to.
fn entries_at(
    strings: &EditorStrings,
    problems: &[Diagnostic],
    place: (usize, usize),
    walked: Option<&Diagnostic>,
) -> Vec<Entry> {
    let mut here: Vec<&Diagnostic> = problems
        .iter()
        .filter(|problem| problem.line.zip(problem.column) == Some(place))
        .collect();
    here.sort_by_key(|problem| problem.severity as u8);
    here.into_iter()
        .map(|problem| Entry {
            severity: problem.severity,
            severity_name: severity_name(strings, problem.severity).to_string(),
            message: problem.message.clone(),
            detail: reported_by(problem),
            docs_url: problem.docs_url.clone(),
            current: walked == Some(problem),
        })
        .collect()
}

/// "burrow · E_PORT": who reported a host's problem, and its code.
fn reported_by(problem: &Diagnostic) -> Option<String> {
    if problem.origin != DiagnosticOrigin::Host {
        return None;
    }
    let parts: Vec<&str> = [problem.source.as_deref(), problem.code.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// Where the tip goes, in window pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Placement {
    left: f64,
    top: f64,
    width: f64,
    max_height: f64,
    above: bool,
}

impl Placement {
    fn style(self) -> String {
        format!(
            "left:{:.1}px;top:{:.1}px;max-width:{:.1}px;max-height:{:.1}px",
            self.left, self.top, self.width, self.max_height
        )
    }
}

/// The visible box the tip stays in (the editor's text area, inside the
/// window), in window pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Area {
    top: f64,
    bottom: f64,
    left: f64,
    right: f64,
}

/// The placement for the tip pointing at `anchor` (tracks the scroll and
/// the layout), or `None` while its line is scrolled out of view.
fn placement(state: EditorState, anchor: TipAnchor, natural: f64) -> Option<Placement> {
    let scroll_x = state.view().scroll_x.get();
    let scroll_y = state.view().scroll_y.get();
    state.view().layout_revision.track();
    let row = state
        .view()
        .projection
        .with(|projection| projection.row_of_line(anchor.line - 1));
    let element = state.view().textarea_ref.get()?;
    let rect = element.get_bounding_client_rect();
    let metrics = read_editor_metrics(&element);
    let window = web_sys::window()?;
    let size = |value: Result<wasm_bindgen::JsValue, _>, fallback: f64| {
        value
            .ok()
            .and_then(|value| value.as_f64())
            .unwrap_or(fallback)
    };
    let viewport_width = size(window.inner_width(), FALLBACK_VIEWPORT_WIDTH);
    let viewport_height = size(window.inner_height(), FALLBACK_VIEWPORT_HEIGHT);
    let area = Area {
        top: rect.top().max(0.0),
        bottom: rect.bottom().min(viewport_height),
        left: rect.left().max(0.0),
        right: rect.right().min(viewport_width),
    };
    let line_top = rect.top() + metrics.padding_top + row as f64 * metrics.line_height - scroll_y;
    if line_top + metrics.line_height <= area.top || line_top >= area.bottom {
        return None;
    }
    let x = rect.left() + metrics.padding_left + anchor.column * metrics.char_width - scroll_x;
    Some(place(area, line_top, metrics.line_height, x, natural))
}

/// The tip for a line at `line_top` (`line_height` tall), starting at `x`,
/// `natural` pixels tall in full: above the line when it fits there, else
/// below when it fits there, else on the roomier side, at most as tall as
/// that side's room (it scrolls); shifted left to stay inside `area`.
fn place(area: Area, line_top: f64, line_height: f64, x: f64, natural: f64) -> Placement {
    let width = TIP_MAX_WIDTH
        .min(area.right - area.left - 2.0 * TIP_MARGIN)
        .max(TIP_MIN_WIDTH);
    let left = x
        .min(area.right - TIP_MARGIN - width)
        .max(area.left + TIP_MARGIN);
    let room_above = line_top - TIP_GAP - (area.top + TIP_MARGIN);
    let room_below = area.bottom - TIP_MARGIN - (line_top + line_height + TIP_GAP);
    let above = natural <= room_above || (natural > room_below && room_above > room_below);
    let room = if above { room_above } else { room_below }.max(TIP_MIN_HEIGHT);
    let top = if above {
        line_top - TIP_GAP - natural.min(room)
    } else {
        line_top + line_height + TIP_GAP
    };
    Placement {
        left,
        top,
        width,
        max_height: room,
        above,
    }
}

#[cfg(test)]
mod tests {
    use super::{Area, Placement, entries_at, place};
    use crate::{Diagnostic, DiagnosticOrigin, EditorStrings, Severity};

    const AREA: Area = Area {
        top: 100.0,
        bottom: 500.0,
        left: 0.0,
        right: 800.0,
    };

    fn fits(placed: Placement, natural: f64) {
        let height = natural.min(placed.max_height);
        assert!(placed.top >= AREA.top, "{placed:?}");
        assert!(placed.top + height <= AREA.bottom, "{placed:?}");
    }

    /// QA 19: a tall tip near the bottom went off the top, its first
    /// messages out of reach. It goes where it fits, else on the roomier
    /// side no taller than the room there (it scrolls).
    #[test]
    fn the_tip_stays_inside_the_editor_above_below_or_scrolled() {
        // A short tip above its line, as before.
        let placed = place(AREA, 300.0, 20.0, 50.0, 60.0);
        assert!(placed.above);
        assert_eq!(placed.top, 300.0 - 4.0 - 60.0);
        fits(placed, 60.0);
        // Near the top: below.
        let placed = place(AREA, 110.0, 20.0, 50.0, 60.0);
        assert!(!placed.above);
        fits(placed, 60.0);
        // Taller than either side: on the roomier one, capped to its room.
        for line_top in [130.0, 300.0, 470.0] {
            let placed = place(AREA, line_top, 20.0, 50.0, 2000.0);
            fits(placed, 2000.0);
            assert!(placed.max_height < 2000.0);
            assert_eq!(placed.above, line_top > 300.0 - 10.0, "{line_top}");
        }
        // Near the right edge: shifted left to stay inside.
        let placed = place(AREA, 300.0, 20.0, 780.0, 60.0);
        assert!(placed.left + placed.width <= AREA.right);
    }

    /// QA 19: several problems at one place each keep their own source,
    /// code and docs link, the most serious first (a warning's source was
    /// shown under an error's message).
    #[test]
    fn each_problem_at_one_place_keeps_its_own_detail_most_serious_first() {
        let host =
            |severity, source: Option<&str>, code: Option<&str>, docs: Option<&str>| Diagnostic {
                line: Some(2),
                column: Some(3),
                severity,
                message: format!("{severity:?}"),
                origin: DiagnosticOrigin::Host,
                source: source.map(str::to_string),
                code: code.map(str::to_string),
                docs_url: docs.map(str::to_string),
                ..Diagnostic::default()
            };
        let own = Diagnostic {
            line: Some(2),
            column: Some(3),
            message: "schema".into(),
            ..Diagnostic::default()
        };
        let problems = [
            host(Severity::Warning, Some("lint"), Some("W1"), None),
            host(
                Severity::Hint,
                Some("host"),
                None,
                Some("https://example.com/h"),
            ),
            own.clone(),
            host(
                Severity::Error,
                Some("burrow"),
                Some("E_PORT"),
                Some("https://example.com/e"),
            ),
            Diagnostic {
                line: Some(2),
                column: Some(4),
                ..own
            },
        ];
        let entries = entries_at(
            &EditorStrings::default(),
            &problems,
            (2, 3),
            Some(&problems[0]),
        );
        let seen: Vec<_> = entries
            .iter()
            .map(|entry| {
                (
                    entry.severity_name.as_str(),
                    entry.message.as_str(),
                    entry.detail.as_deref(),
                    entry.docs_url.as_deref(),
                    entry.current,
                )
            })
            .collect();
        assert_eq!(
            seen,
            [
                ("Error", "schema", None, None, false),
                (
                    "Error",
                    "Error",
                    Some("burrow · E_PORT"),
                    Some("https://example.com/e"),
                    false
                ),
                ("Warning", "Warning", Some("lint · W1"), None, true),
                (
                    "Hint",
                    "Hint",
                    Some("host"),
                    Some("https://example.com/h"),
                    false
                ),
            ]
        );
    }
}
