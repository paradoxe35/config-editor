# Config Editor for Leptos

A lightweight and fast, VS Code–style config editor component for [Leptos], plus a
headless API for the same parsing, validation, and conversion.

- **Languages:** YAML (default), JSON, TOML, HCL, XML, and your own through
  the `ConfigLanguage` trait.
- **One schema for all of them:** every language parses into the same data
  model, so a single JSON Schema drives diagnostics, completions, and hover
  documentation everywhere.
- **Edit as another language:** show a YAML file as TOML (or any other pair);
  edits are saved back in the file's own language, keeping its comments.
- **Editing:** schema completion with formatted inserts, auto-closing pairs
  and XML end tags, comment toggling, smart indentation, undo/redo, a diff
  gutter against a baseline (with per-change revert, a change count and a
  unified diff), read-only mode, and eleven bundled themes.
- **Large files:** only visible rows are rendered; typing stays fast in
  documents of thousands of lines.

For JavaScript and React, see [`@pngwasi/config-editor`] and
[`@pngwasi/config-editor-react`]; they wrap this crate. Every prop, handle
method and headless function here has a JS option, method or function there
(and a React prop, ref member or export), or a documented reason not to:
`e2e/tests/api-parity.spec.mjs` checks it.

<!-- toc -->

## Contents

- [Install](#install)
- [Quick start](#quick-start)
- [`<ConfigEditor>` props](#configeditor-props)
  - [Settings](#settings)
  - [Editing in another language](#editing-in-another-language)
  - [Built-in texts](#built-in-texts)
  - [Changes against the baseline](#changes-against-the-baseline)
  - [Saving and unsaved edits](#saving-and-unsaved-edits)
  - [Embedding](#embedding)
- [Several documents](#several-documents)
- [Paths and positions](#paths-and-positions)
- [Host diagnostics](#host-diagnostics)
- [Read-only fields](#read-only-fields)
- [Secrets](#secrets)
- [Schema help](#schema-help)
- [Keyboard](#keyboard)
  - [Keyboard and screen-reader access](#keyboard-and-screen-reader-access)
  - [Find and replace](#find-and-replace)
  - [Folding](#folding)
  - [Large documents](#large-documents)
- [Headless API](#headless-api)
- [XML mapping](#xml-mapping)
- [Custom languages](#custom-languages)
  - [Completion and hover](#completion-and-hover)
  - [Describing syntax as data](#describing-syntax-as-data)
  - [Optional refinements](#optional-refinements)
- [Themes and colors](#themes-and-colors)
- [Strict Content-Security-Policy](#strict-content-security-policy)
- [Cargo features](#cargo-features)
  - [Without a feature](#without-a-feature)
  - [Size](#size)
- [License](#license)
<!-- /toc -->

## Install

```sh
cargo add config-editor-leptos
```

Requires Leptos 0.8 with client-side rendering (`csr`), a Rust 1.88+
toolchain, and the `wasm32-unknown-unknown` target for the browser build.
The headless API also works natively (servers, CLIs, tests).

With schema validation on (the default), the wasm uses WebAssembly SIMD
(Chrome/Edge 91+, Firefox 89+, Safari 16.4+): `bytecount`, a dependency of
`jsonschema`, compiles SIMD code into every wasm build
([upstream issue](https://github.com/llogiq/bytecount/issues/94)). To load in
browsers without SIMD too, add this to your workspace's `Cargo.toml` (the npm
packages are built with it):

```toml
[patch.crates-io]
bytecount = { git = "https://github.com/CharlesNg35/bytecount", rev = "f9dd655e3cadaba5d8941f5fe0c7ac2b026c5019" }
```

## Quick start

```rust
use leptos::prelude::*;
use config_editor_leptos::{ConfigEditor, EditorSettings, Language};

#[component]
fn Config() -> impl IntoView {
    let saved = RwSignal::new("listen: 7835\n".to_string());
    view! {
        <ConfigEditor
            language=Language::YAML
            value=saved
            on_change=move |text| saved.set(text)
            schema=include_str!("schema.json")
            settings=EditorSettings { show_toolbar: true, show_footer: true, ..Default::default() }
        />
    }
}
```

The editor injects its stylesheet ([`PRESET_CSS`]) by default; under a
strict Content-Security-Policy, serve it as a file instead (see
[Strict Content-Security-Policy](#strict-content-security-policy)).

## `<ConfigEditor>` props

| Prop                   | Type                                       | Default               | Purpose                                                                                                                                                                                                                                                                                                                                                                                        |
| ---------------------- | ------------------------------------------ | --------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `language`             | `Language`                                 | `Language::YAML`      | Language of the document the host stores: `initial_value`, `value`, `baseline` and `on_change` are always in it.                                                                                                                                                                                                                                                                               |
| `edit_as`              | `Language`, `Option<Language>` or a signal | unset                 | Show and edit the document in another language. Changing it converts the open document in place, unsaved edits included.                                                                                                                                                                                                                                                                       |
| `initial_value`        | `String`                                   | `""`                  | Starting text, also the diff baseline (uncontrolled).                                                                                                                                                                                                                                                                                                                                          |
| `value`                | `Signal<String>`                           | —                     | Controlled text. Updates flow in; the caret stays on the same text and the scroll position is kept.                                                                                                                                                                                                                                                                                            |
| `on_change`            | `Fn(String)`                               | —                     | Called with the document after every edit (after each typing pause while editing as another language).                                                                                                                                                                                                                                                                                         |
| `on_edit_as_change`    | `Fn(Language)`                             | —                     | Called with the language on screen when an `edit_as` switch completes, and when undo or redo steps back across one. A switch waits while the document cannot convert yet (TOML has no `null`): until then the host's language stays on screen, edited and sent to `on_change` as it is, with the reason shown on the value in the way. Label the language shown from this, not from `edit_as`. |
| `on_schema_warning`    | `Fn(String)`                               | console warning       | Called once per thing in `schema` the editor cannot use, such as a `patternProperties` regex its engine cannot compile.                                                                                                                                                                                                                                                                        |
| `baseline`             | `Signal<String>`                           | the document at mount | Text the diff gutter and the dirty flag compare against; set it to the saved text after saving. See [Saving](#saving-and-unsaved-edits).                                                                                                                                                                                                                                                       |
| `schema`               | `&str`, `String` or `Schema`               | —                     | JSON Schema (draft 2020-12 and earlier) for diagnostics, completion and hover. Pass a compiled `Schema` to share one across editors.                                                                                                                                                                                                                                                           |
| `read_only`            | `bool` or a signal                         | `false`               | View mode: navigation, selection, highlighting, diagnostics, diffs and hover keep working. Switches live, completions included.                                                                                                                                                                                                                                                                |
| `file_name`            | `String`                                   | `untitled.<ext>`      | Name shown in the toolbar.                                                                                                                                                                                                                                                                                                                                                                     |
| `class`                | `String`                                   | —                     | Extra class on the editor root.                                                                                                                                                                                                                                                                                                                                                                |
| `settings`             | `EditorSettings` or a signal               | defaults              | The theme, colors and every on/off switch (toolbar, footer, diagnostics, diff, completions, search, folding, …); see [Settings](#settings). Every setting switches live.                                                                                                                                                                                                                       |
| `on_change_summary`    | `Fn(ChangeSummary)`                        | —                     | Called with the lines added, removed and modified against `baseline`, and the number of changes, whenever that changes.                                                                                                                                                                                                                                                                        |
| `handle`               | `EditorHandle`                             | —                     | Find fields, follow the caret and jump to a field from the host; see [Paths and positions](#paths-and-positions).                                                                                                                                                                                                                                                                              |
| `on_path_change`       | `Fn(FieldPath)`                            | —                     | Called with the path of what the caret is on each time it changes (and once on mount), for a breadcrumb.                                                                                                                                                                                                                                                                                       |
| `host_diagnostics`     | `Signal<Vec<HostDiagnostic>>`              | —                     | Problems the host found (a server's validation), by path or range, shown with the editor's own; see [Host diagnostics](#host-diagnostics).                                                                                                                                                                                                                                                     |
| `read_only_paths`      | `Signal<Vec<FieldPath>>`                   | —                     | Fields the user cannot change, each a whole subtree, on top of the schema's `readOnly` ones; see [Read-only fields](#read-only-fields).                                                                                                                                                                                                                                                        |
| `read_only_ranges`     | `Signal<Vec<TextRange>>`                   | —                     | Stretches of the host's text the user cannot change; they follow the text through edits.                                                                                                                                                                                                                                                                                                       |
| `secrets`              | `SecretOptions`                            | defaults              | How secret placeholders are matched, described and completed; see [Secrets](#secrets).                                                                                                                                                                                                                                                                                                         |
| `on_save`              | `Fn(SaveRequest)`                          | —                     | Called on `Ctrl+S` / `Cmd+S` and `EditorHandle::save` with the document in `language`. Nothing saves by itself; without it `Ctrl+S` is the browser's.                                                                                                                                                                                                                                          |
| `on_dirty_change`      | `Fn(bool)`                                 | —                     | Called with whether the text differs from `baseline`: once on mount, then each time that changes.                                                                                                                                                                                                                                                                                              |
| `on_baseline_conflict` | `Fn(BaselineConflict)`                     | —                     | Called when a new `baseline` arrives under unsaved edits (the notice opens) and when that is settled.                                                                                                                                                                                                                                                                                          |

While editing as another language, edits reach `on_change` after each typing
pause. Call `config_editor_leptos::flush_pending_edits()` before reading the
document from code (a Save button) to send an edit still waiting first.

To build your own chrome, turn the built-in toolbar or footer off and compose
`ConfigEditorToolbar` / `ConfigEditorFooter`, which take the same `theme`
and `colors` so they match the editor.

### Settings

`settings` takes an [`EditorSettings`]: the theme, the colors and every
on/off switch, one struct with a default for each field. Name only what you
change, and pass a signal to switch any of them live: the caret, the undo
history and the folds stay.

```rust
use config_editor_leptos::{ConfigEditor, EditorSettings, EditorTheme};

let settings = RwSignal::new(EditorSettings {
    theme: EditorTheme::Nord,
    show_footer: true,
    ..EditorSettings::default()
});
view! {
    <button on:click=move |_| settings.update(|settings| settings.folding = !settings.folding)>
        "Toggle folding"
    </button>
    <ConfigEditor settings=settings initial_value="listen: 7835\n" />
}
```

The same struct is what the npm package reads from its options (camelCase
keys, `{ showFooter: true }`), and its TypeScript type is generated from it.

<!-- settings:start -->

<!-- Generated from crates/config-editor-leptos/src/settings.rs by js/config-editor/tests/settings.rs; do not edit. Regenerate with `UPDATE_GENERATED=1 cargo test -p config-editor-js --test settings`. -->

| Field                      | Default                   |                                                                                                                                                                                                                                                                                    |
| -------------------------- | ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `theme`                    | `EditorTheme::VsCodeDark` | The bundled color theme.                                                                                                                                                                                                                                                           |
| `colors`                   | none                      | Colors overriding the theme's, by token (`bg`, `key`, `string`, `selection`, …), for a custom palette.                                                                                                                                                                             |
| `styled`                   | `true`                    | Inject the preset stylesheet. Off, serve it as a file (`preset.css` in the crate and the npm package, as a strict Content-Security-Policy needs) or style the `config-editor__*` markup yourself (the preset is `PRESET_CSS`, `presetCss()` in JavaScript).                        |
| `show_toolbar`             | `false`                   | Show the built-in toolbar: the file name, its language and the schema's state.                                                                                                                                                                                                     |
| `show_footer`              | `false`                   | Show the built-in status footer: the suggestions hint, problem and change counts, and the line count.                                                                                                                                                                              |
| `show_diagnostics`         | `true`                    | Show syntax and schema problems inline, in the footer and on hover (`F8` moves between them).                                                                                                                                                                                      |
| `show_diff`                | `true`                    | Show line changes against the baseline in the gutter (`Alt+F5` / `Alt+F3` move between them). Off, the change summary stays at zero. In Rust, needs the `diff` Cargo feature.                                                                                                      |
| `completions`              | `true`                    | Schema completions (`Ctrl+Space`).                                                                                                                                                                                                                                                 |
| `auto_trigger_completions` | `true`                    | Open suggestions while typing and after accepting a key. Off, only `Ctrl+Space` opens them.                                                                                                                                                                                        |
| `hover_docs`               | `true`                    | Show the schema's title, description, type, default and examples when hovering a key, at the caret with `Ctrl+K Ctrl+I` (read to screen readers), or on a tap on a touch screen.                                                                                                   |
| `highlighting`             | `true`                    | Syntax colors.                                                                                                                                                                                                                                                                     |
| `auto_closing`             | `true`                    | Auto-close quotes and brackets, wrap selections, and delete empty pairs on Backspace.                                                                                                                                                                                              |
| `search`                   | `true`                    | Find and replace (`Ctrl+F`, `Ctrl+H`, `F3`). Off, those keys are left to the browser. In Rust, needs the `search` Cargo feature.                                                                                                                                                   |
| `go_to_line`               | `true`                    | Go to line (`Ctrl+G`). Off, the key is left to the browser. In Rust, needs the `search` Cargo feature.                                                                                                                                                                             |
| `folding`                  | `true`                    | Folding by structure: chevrons in the gutter, `Ctrl+Shift+[` and `Ctrl+Shift+]` at the caret, `Ctrl+K Ctrl+0` and `Ctrl+K Ctrl+J` for everything (`Cmd` on macOS). View only: the text stays whole. Switching it off opens every fold. In Rust, needs the `folding` Cargo feature. |
| `save_shortcut`            | `true`                    | Whether `Ctrl+S` / `Cmd+S` saves, when a save callback is set. Off, the key is left to the browser and only the handle's `save` saves.                                                                                                                                             |
| `strings`                  | English (see below)       | Every built-in text (hints, tooltips, notices and their buttons, footer counts, accessible names and announcements), English by default: give only the texts to change. See `EditorStrings`.                                                                                       |

<!-- settings:end -->

### Editing in another language

```rust
<ConfigEditor
    language=Language::XML   // what the host stores and receives
    edit_as=Language::HCL    // what the user edits
    value=xml
    on_change=move |text| xml.set(text)
/>
```

<details>
<summary>What a save keeps, what waits, undo and the comments notice (all conversion rules)</summary>

- Only what the user changed is written back: untouched values keep their
  exact host spelling, and the host's comments and formatting survive. An
  edited value keeps its form: a YAML block scalar stays a block with its
  header (literal or folded, its indentation indicator such as `|2-` and its
  chomping; folded text keeps its line breaks where the words around them
  did, and text folding cannot hold is written literally), TOML long strings
  and HCL heredocs stay so, XML text keeps its CDATA sections and entity or
  character references (`&#233;`) around the change, YAML anchors stay on their items
  (an alias or merge is written out only where the edit parts it from its
  anchor, numbers as the host wrote them), and a JSON string keeps its
  `\uXXXX` escapes. A quoted string emptied and typed again keeps the quotes
  it had when the session began, and a heredoc whose text gains a line equal
  to its delimiter takes another one (`EOT_1`).
- A number typed in a spelling of its own (`2.50`, `2e3`, `+6`, `0x51`, a
  TOML time's `.999990`) is saved so where the host reads it as the same
  value. In a view that cannot tell types apart (XML text, HCL numbers), the
  host's type decides: text that reads as a number or boolean where the host
  holds one is that (`False`, `007`), and a whole number typed over a float
  stays a float.
- Items moved in a list, with others added or removed in the same save, take
  their comment lines along (scalars, one-line tables, YAML block items,
  repeated XML elements over lines); where they cannot, the save waits. The
  comments right above a list's first item are the item's at the top of the
  file too: a file's header is the comments a blank line ends. A list the
  host writes as repeated elements, HCL blocks or TOML `[[tables]]`, emptied,
  is removed with their comments.
- YAML tags (`!Sub 'x-${y}'`, `!GetAtt [a, b]`, `!include`, `!secret`,
  `!reference [.a, b]`) are valid YAML: the data (what the schema validates
  and other languages show) is the value under the tag, a tagged scalar as
  its text, and the tag stays in the host's text through every edit of that
  value or others. An edit the tag cannot follow (a tagged map replaced by a
  number) waits, naming the tag and the value. Several YAML documents in
  one text (`---` between them) are one per file here: the text shows the
  problem at the second `---`, and Edit as is refused with that reason.
- XML element text is the value exactly as an XML reader reports it, the
  whitespace and line breaks around it included (a number, boolean or `null`
  reads without them): `<s>  a  </s>` shows as `"  a  "` in other languages,
  and a YAML `|` block keeps its final line break through the XML view. A
  string an XML host would read back as another value (`"true"`, `"null"`,
  `"12"`, or `""`, an empty element) is not saved there: the save waits with
  the reason.
- XML mixed content (text beside child elements, `<p>a <b>b</b> c</p>`) has
  no form in other languages: such a document is not shown in another
  language, and the switch waits with the reason on the element
  (`mixed_content` of the `strings` setting); text typed beside child elements
  in an XML view is not saved. `Language::convert` refuses it too
  (`ConvertError::MixedContent`). So is one key in two namespaces
  (`<c:item>` beside `<item>`), which other languages would merge, and a
  character XML 1.0 cannot hold (a control character, U+FFFE): the XML
  reader and writer refuse it, even as a character reference.
- A save that cannot write what the edit means waits, with the reason on
  the value: an edit inside nested or empty lists that the XML view shows
  flat and cannot map back, a TOML date or time typed half-way (it is saved
  as a datetime once it is one again), a key of an HCL block renamed to
  something that is no identifier (the block would have to become an
  attribute). A renamed key is written as the host writes keys there (YAML
  quotes `on` and `1_000`, which YAML 1.1 reads as other types).
- Switching the language is an undo step: `Ctrl+Z` goes back to the view and
  text before the switch (`on_edit_as_change` hears it), `Ctrl+Y` returns,
  and the caret stays on the field it was on.
- Invalid text is never sent to the host.
- Values the host cannot carry exactly (for example an empty list in XML)
  are flagged as warnings on the value.
- With a schema, a single XML element or HCL block where the schema expects a
  list reads as a one-item list.
- A notice under the toolbar (`role="status"`, dismissible) says what the
  edited language does not show or keep: the host file's comments (an XML
  file's processing instructions too) and formatting are not shown there and
  a save keeps them; comments typed there are never saved (the data a
  conversion carries has none), whatever the pair of languages. It shows only
  when the host's text has comments or the edited text does; its texts are
  `editAsHostComments` (`editAsHostInstructions` when it names processing
  instructions), `editAsTypedComments` and `editAsTypedCommentsNone` of the
  `strings` setting.

</details>

### Built-in texts

Every text the editor writes itself (hints, tooltips, notices and their
buttons, footer counts, accessible names and announcements, the messages of
the problems it words) is a field of `EditorStrings`, the `strings` setting,
in English by default. Give only the texts to change:

```rust
let settings = EditorSettings {
    strings: EditorStrings {
        no_problems: "Keine Fehler".into(),
        errors_one: "{count} Fehler".into(),
        errors_other: "{count} Fehler".into(),
        find: "Suchen".into(),
        ..EditorStrings::default()
    },
    ..EditorSettings::default()
};
```

Texts change live, like the other settings. Placeholders are written in
braces (`{count}`, `{line}`); a text takes the placeholders of its default
(`EditorStrings::check` lists any other, and the editor warns about them in
the console). Counts come in pairs, `…_one` for 1 and `…_other` for any other
count. Texts the host or the schema gives (read-only reasons, a secret hint,
descriptions) and what parsers and the schema validator report are shown as
they come.

<!-- strings:start -->

<!-- Generated from crates/config-editor-leptos/src/strings.rs by js/config-editor/tests/settings.rs; do not edit. Regenerate with `UPDATE_GENERATED=1 cargo test -p config-editor-js --test settings`. -->

<details>
<summary>Every text and its default (164 texts)</summary>

| Field                           | Default                                                                                                                              |                                                                                                                                                                                                                                                |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `suggestions_hint`              | `"{key} for suggestions"`                                                                                                            | The footer's suggestions hint; `{key}` is where the chord goes.                                                                                                                                                                                |
| `suggestions_key`               | `"Ctrl+Space"`                                                                                                                       | The chord that opens suggestions, as the footer draws it: keys between `+`.                                                                                                                                                                    |
| `no_problems`                   | `"No errors"`                                                                                                                        | The footer's problem count when there are none.                                                                                                                                                                                                |
| `errors_one`                    | `"{count} error"`                                                                                                                    | One error, in the footer's problem count.                                                                                                                                                                                                      |
| `errors_other`                  | `"{count} errors"`                                                                                                                   | Several errors, in the footer's problem count.                                                                                                                                                                                                 |
| `warnings_one`                  | `"{count} warning"`                                                                                                                  | One warning, in the footer's problem count.                                                                                                                                                                                                    |
| `warnings_other`                | `"{count} warnings"`                                                                                                                 | Several warnings, in the footer's problem count.                                                                                                                                                                                               |
| `infos_one`                     | `"{count} info"`                                                                                                                     | One info, in the footer's problem count.                                                                                                                                                                                                       |
| `infos_other`                   | `"{count} info"`                                                                                                                     | Several infos, in the footer's problem count.                                                                                                                                                                                                  |
| `hints_one`                     | `"{count} hint"`                                                                                                                     | One hint, in the footer's problem count.                                                                                                                                                                                                       |
| `hints_other`                   | `"{count} hints"`                                                                                                                    | Several hints, in the footer's problem count.                                                                                                                                                                                                  |
| `problems_hint`                 | `"hover or focus underlined text"`                                                                                                   | After the problem count when every problem has its place.                                                                                                                                                                                      |
| `problems_cut`                  | `"only the editor's first {count} are shown"`                                                                                        | After the problem count when the editor's own problems were cut; `{count}` is how many of them are shown (the host's are all shown).                                                                                                           |
| `problems_more`                 | `"Only the first {count} problems are shown; more follow."`                                                                          | The problem that says the list was cut, after `{count}` problems.                                                                                                                                                                              |
| `underlines_cut`                | `"only {count} underlines are drawn, the most serious first"`                                                                        | After the problem count when the problems have more underlines than are drawn; `{count}` is how many are (the most serious first).                                                                                                             |
| `changes_one`                   | `"{count} change"`                                                                                                                   | The footer's change count against the baseline, for one change.                                                                                                                                                                                |
| `changes_other`                 | `"{count} changes"`                                                                                                                  | The footer's change count, for several changes.                                                                                                                                                                                                |
| `change_details_one`            | `"{lines} line · Alt+F5 for the next"`                                                                                               | The change count's tooltip when one line changed; `{lines}` lists the added, modified and removed counts.                                                                                                                                      |
| `change_details_other`          | `"{lines} lines · Alt+F5 for the next"`                                                                                              | The change count's tooltip when several lines changed.                                                                                                                                                                                         |
| `changed_added`                 | `"{count} added"`                                                                                                                    | Added lines, in the change count's tooltip.                                                                                                                                                                                                    |
| `changed_modified`              | `"{count} modified"`                                                                                                                 | Modified lines, in the change count's tooltip.                                                                                                                                                                                                 |
| `changed_removed`               | `"{count} removed"`                                                                                                                  | Removed lines, in the change count's tooltip.                                                                                                                                                                                                  |
| `lines_one`                     | `"{count} line"`                                                                                                                     | The footer's line count, for one line.                                                                                                                                                                                                         |
| `lines_other`                   | `"{count} lines"`                                                                                                                    | The footer's line count, for several lines.                                                                                                                                                                                                    |
| `tab_focus`                     | `"Tab moves focus"`                                                                                                                  | The footer's indicator while Tab moves the focus.                                                                                                                                                                                              |
| `tab_focus_title`               | `"Tab moves focus (Ctrl+M to indent with Tab)"`                                                                                      | Its tooltip; clicking it turns the mode off.                                                                                                                                                                                                   |
| `tab_focus_on`                  | `"Tab moves focus: Tab and Shift+Tab now leave the editor. Ctrl+M to indent with Tab again."`                                        | Announced when `Ctrl+M` makes Tab move the focus.                                                                                                                                                                                              |
| `tab_focus_off`                 | `"Tab indents: Tab and Shift+Tab indent again. Ctrl+M to move focus with Tab."`                                                      | Announced when `Ctrl+M` makes Tab indent again.                                                                                                                                                                                                |
| `schema_connected`              | `"Schema connected"`                                                                                                                 | The toolbar's schema state.                                                                                                                                                                                                                    |
| `edit_as_host_comments`         | `"The {host} file's comments and formatting are not shown while editing as {edit}; saving keeps them."`                              | The Edit-as notice when the host's file has comments, which the edited language does not show but saving keeps.                                                                                                                                |
| `edit_as_host_instructions`     | `"The {host} file's comments, processing instructions and formatting are not shown while editing as {edit}; saving keeps them."`     | The same when the host's file has processing instructions too (XML `<?…?>`), which the edited language does not show either.                                                                                                                   |
| `edit_as_typed_comments`        | `"Comments typed in {edit} are not saved: the {host} file keeps only its own comments."`                                             | The Edit-as notice when comments are typed in the edited language, which saving drops, and the host's language has comments.                                                                                                                   |
| `edit_as_typed_comments_none`   | `"Comments typed in {edit} are not saved: {host} has no comments."`                                                                  | The same when the host's language has no comments (JSON).                                                                                                                                                                                      |
| `edit_as_dismiss`               | `"Dismiss"`                                                                                                                          | The Edit-as notice's close button.                                                                                                                                                                                                             |
| `baseline_changed`              | `"The file changed on the server."`                                                                                                  | The notice shown when the host's baseline changes under unsaved edits.                                                                                                                                                                         |
| `baseline_changed_detail`       | `"Reload it, losing your changes, or keep your text and compare it with the new version."`                                           | What the notice's choices do, after its message.                                                                                                                                                                                               |
| `reload`                        | `"Reload"`                                                                                                                           | The notice's button that takes the new baseline as the text.                                                                                                                                                                                   |
| `reload_title`                  | `"Replace your text with the new version (Ctrl+Z brings yours back)"`                                                                | Its tooltip.                                                                                                                                                                                                                                   |
| `keep_mine`                     | `"Keep mine"`                                                                                                                        | The notice's button that keeps the text against the new baseline.                                                                                                                                                                              |
| `keep_mine_title`               | `"Keep your text; the changes are now shown against the new version (Escape)"`                                                       | Its tooltip.                                                                                                                                                                                                                                   |
| `baseline_changed_keys`         | `"F6 moves to the choices and back."`                                                                                                | How to reach the notice's choices from the keyboard, shown on it (and announced with it).                                                                                                                                                      |
| `find_widget`                   | `"Find and replace"`                                                                                                                 | The find widget's accessible name.                                                                                                                                                                                                             |
| `toggle_replace`                | `"Toggle replace"`                                                                                                                   | The button showing the replace row.                                                                                                                                                                                                            |
| `toggle_replace_title`          | `"Toggle replace (Ctrl+H)"`                                                                                                          | Its tooltip.                                                                                                                                                                                                                                   |
| `find`                          | `"Find"`                                                                                                                             | The query field's accessible name and placeholder.                                                                                                                                                                                             |
| `match_case`                    | `"Match case"`                                                                                                                       | The match-case toggle.                                                                                                                                                                                                                         |
| `match_case_title`              | `"Match case (Alt+C)"`                                                                                                               | Its tooltip.                                                                                                                                                                                                                                   |
| `whole_word`                    | `"Match whole word"`                                                                                                                 | The whole-word toggle.                                                                                                                                                                                                                         |
| `whole_word_title`              | `"Match whole word (Alt+W)"`                                                                                                         | Its tooltip.                                                                                                                                                                                                                                   |
| `use_regex`                     | `"Use regular expression"`                                                                                                           | The regular-expression toggle.                                                                                                                                                                                                                 |
| `use_regex_title`               | `"Use regular expression (Alt+R)"`                                                                                                   | Its tooltip.                                                                                                                                                                                                                                   |
| `previous_match`                | `"Previous match"`                                                                                                                   | The previous-match button.                                                                                                                                                                                                                     |
| `previous_match_title`          | `"Previous match (Shift+Enter)"`                                                                                                     | Its tooltip.                                                                                                                                                                                                                                   |
| `next_match`                    | `"Next match"`                                                                                                                       | The next-match button.                                                                                                                                                                                                                         |
| `next_match_title`              | `"Next match (Enter)"`                                                                                                               | Its tooltip.                                                                                                                                                                                                                                   |
| `close`                         | `"Close"`                                                                                                                            | The find widget's close button.                                                                                                                                                                                                                |
| `close_title`                   | `"Close (Escape)"`                                                                                                                   | Its tooltip.                                                                                                                                                                                                                                   |
| `replace`                       | `"Replace"`                                                                                                                          | The replacement field's accessible name and placeholder, and the button replacing the current match.                                                                                                                                           |
| `replace_title`                 | `"Replace (Enter)"`                                                                                                                  | The replace button's tooltip.                                                                                                                                                                                                                  |
| `replace_all`                   | `"All"`                                                                                                                              | The replace-all button.                                                                                                                                                                                                                        |
| `replace_all_label`             | `"Replace all"`                                                                                                                      | Its accessible name.                                                                                                                                                                                                                           |
| `replace_all_title`             | `"Replace all (Ctrl+Alt+Enter)"`                                                                                                     | Its tooltip.                                                                                                                                                                                                                                   |
| `match_position`                | `"{current} of {total}"`                                                                                                             | The match count: match `{current}` of `{total}`.                                                                                                                                                                                               |
| `no_results`                    | `"No results"`                                                                                                                       | The match count when nothing matches.                                                                                                                                                                                                          |
| `invalid_pattern`               | `"Invalid pattern"`                                                                                                                  | The match count when the regular expression is invalid.                                                                                                                                                                                        |
| `replaced_one`                  | `"Replaced {count} occurrence"`                                                                                                      | After Replace All, for one match.                                                                                                                                                                                                              |
| `replaced_other`                | `"Replaced {count} occurrences"`                                                                                                     | After Replace All, for several.                                                                                                                                                                                                                |
| `replace_skipped`               | `"; skipped {count} in read-only text or secret placeholders"`                                                                       | Added when Replace All left matches in protected text alone.                                                                                                                                                                                   |
| `nothing_replaced`              | `"Every match is in read-only text or secret placeholders; nothing was replaced"`                                                    | After Replace All when every match is in read-only text or a secret placeholder.                                                                                                                                                               |
| `go_to_line`                    | `"Go to line"`                                                                                                                       | The go-to-line field's accessible name.                                                                                                                                                                                                        |
| `go_to_line_placeholder`        | `"line or line:column"`                                                                                                              | Its placeholder.                                                                                                                                                                                                                               |
| `go_to_line_hint`               | `"Line 1–{lines}, or line:column"`                                                                                                   | Its hint: the range of lines.                                                                                                                                                                                                                  |
| `go_to_line_invalid`            | `"Type a line number between 1 and {lines}"`                                                                                         | Its hint when the text is no position.                                                                                                                                                                                                         |
| `added_line`                    | `"Added line"`                                                                                                                       | The change preview's title for an added line.                                                                                                                                                                                                  |
| `modified_line`                 | `"Modified line"`                                                                                                                    | For a modified line.                                                                                                                                                                                                                           |
| `removed_lines`                 | `"Removed lines"`                                                                                                                    | For removed lines.                                                                                                                                                                                                                             |
| `removed_lines_at_end`          | `"Removed lines at end of document"`                                                                                                 | For lines removed after the last one.                                                                                                                                                                                                          |
| `show_added`                    | `"Show added line {line}"`                                                                                                           | The gutter marker of an added line.                                                                                                                                                                                                            |
| `show_added_title`              | `"Show added line"`                                                                                                                  | Its tooltip.                                                                                                                                                                                                                                   |
| `show_modified`                 | `"Show before and after for modified line {line}"`                                                                                   | The gutter marker of a modified line.                                                                                                                                                                                                          |
| `show_modified_title`           | `"Show before and after"`                                                                                                            | Its tooltip.                                                                                                                                                                                                                                   |
| `show_removed`                  | `"Show removed lines before line {line}"`                                                                                            | The gutter marker of removed lines.                                                                                                                                                                                                            |
| `show_removed_title`            | `"Show removed lines"`                                                                                                               | Its tooltip, and that of lines removed at the end.                                                                                                                                                                                             |
| `show_removed_at_end`           | `"Show removed lines at end of document"`                                                                                            | The gutter marker of lines removed after the last one.                                                                                                                                                                                         |
| `preview_heading`               | `"{title} · line {line}"`                                                                                                            | The change preview's heading: its `{title}` and `{line}`.                                                                                                                                                                                      |
| `preview_label`                 | `"{title}, line {line}"`                                                                                                             | The change preview's accessible name.                                                                                                                                                                                                          |
| `revert`                        | `"Revert"`                                                                                                                           | The preview's revert button.                                                                                                                                                                                                                   |
| `revert_title`                  | `"Revert this change (R)"`                                                                                                           | Its tooltip, for a one-line change.                                                                                                                                                                                                            |
| `revert_lines_title`            | `"Revert this change (lines {first}–{last}) (R)"`                                                                                    | Its tooltip, for a change over lines `{first}` to `{last}`.                                                                                                                                                                                    |
| `revert_line_and_removed_title` | `"Revert this change (line {line} and the removed lines) (R)"`                                                                       | Its tooltip, for a changed line with removed lines.                                                                                                                                                                                            |
| `close_preview`                 | `"Close change preview"`                                                                                                             | The preview's close button.                                                                                                                                                                                                                    |
| `before`                        | `"Before"`                                                                                                                           | The preview's column of the baseline's lines.                                                                                                                                                                                                  |
| `current`                       | `"Current"`                                                                                                                          | The preview's column of the current lines.                                                                                                                                                                                                     |
| `fold`                          | `"Fold line {line}"`                                                                                                                 | The chevron of an open range.                                                                                                                                                                                                                  |
| `unfold`                        | `"Unfold lines {first}–{last}"`                                                                                                      | The chevron or chip of a closed fold.                                                                                                                                                                                                          |
| `folded_changes`                | `"Folded lines have changes"`                                                                                                        | A closed fold's line number when it hides changes or problems.                                                                                                                                                                                 |
| `folded_problem_one`            | `"hides a problem: {severity}"`                                                                                                      | After a closed fold's chip label when it hides one problem; `{severity}` names it.                                                                                                                                                             |
| `folded_problem_other`          | `"hides {count} problems, the most serious: {severity}"`                                                                             | After a closed fold's chip label when it hides `{count}` problems; `{severity}` names the most serious of them.                                                                                                                                |
| `required`                      | `"required"`                                                                                                                         | The hover card's mark of a required key.                                                                                                                                                                                                       |
| `deprecated`                    | `"deprecated"`                                                                                                                       | The hover card's mark of a deprecated key.                                                                                                                                                                                                     |
| `default_value`                 | `"Default"`                                                                                                                          | The hover card's default row.                                                                                                                                                                                                                  |
| `examples`                      | `"Examples"`                                                                                                                         | The hover card's examples row.                                                                                                                                                                                                                 |
| `one_of`                        | `"One of"`                                                                                                                           | The hover card's row listing the titles of the options (`oneOf`, `anyOf`) a field's object may take, while none matches it yet.                                                                                                                |
| `documentation`                 | `"Documentation"`                                                                                                                    | The hover card's link to the schema's documentation.                                                                                                                                                                                           |
| `open_docs`                     | `"Open docs ↗ (Alt+Enter)"`                                                                                                          | The link under a host problem's message.                                                                                                                                                                                                       |
| `error`                         | `"Error"`                                                                                                                            | An error, as a screen reader hears it.                                                                                                                                                                                                         |
| `warning`                       | `"Warning"`                                                                                                                          | A warning, as a screen reader hears it.                                                                                                                                                                                                        |
| `info`                          | `"Info"`                                                                                                                             | An info, as a screen reader hears it.                                                                                                                                                                                                          |
| `hint`                          | `"Hint"`                                                                                                                             | A hint, as a screen reader hears it.                                                                                                                                                                                                           |
| `problem_source`                | `" from {source}"`                                                                                                                   | Who reported a host problem: the `{source}` of the problem texts below (empty for the editor's own problems).                                                                                                                                  |
| `problem`                       | `"{severity}{source}: {message}"`                                                                                                    | A problem at the caret, for screen readers.                                                                                                                                                                                                    |
| `problem_position`              | `"{severity} {position} of {total}{source}: {message}"`                                                                              | Announced on `F8`: problem `{position}` of `{total}`.                                                                                                                                                                                          |
| `problem_code`                  | `" Code {code}."`                                                                                                                    | After a problem's text for screen readers: its `{code}` (from the host).                                                                                                                                                                       |
| `problem_docs`                  | `" Documentation: {url} (Alt+Enter opens it)."`                                                                                      | After a problem's text (or the hover card's docs) for screen readers: its documentation's `{url}`, and the key that opens it.                                                                                                                  |
| `problem_marker`                | `"{language} {severity}{source} on line {line}: {message}"`                                                                          | An underlined problem's accessible name.                                                                                                                                                                                                       |
| `protected_note`                | `"{label}: {message}"`                                                                                                               | What protected text at the caret is, for screen readers: its `{label}` and `{message}`.                                                                                                                                                        |
| `source_label`                  | `"{language} source"`                                                                                                                | The text area.                                                                                                                                                                                                                                 |
| `suggestions_label`             | `"{language} suggestions"`                                                                                                           | The suggestions list.                                                                                                                                                                                                                          |
| `problems_label`                | `"Inline {language} diagnostics"`                                                                                                    | The layer of underlined problems.                                                                                                                                                                                                              |
| `editor_label`                  | `"{language} editor"`                                                                                                                | The editor's frame.                                                                                                                                                                                                                            |
| `read_only_field`               | `"This field is read-only"`                                                                                                          | Why a field is read-only when neither the schema nor the host says.                                                                                                                                                                            |
| `read_only_text`                | `"This text is read-only"`                                                                                                           | Why text in a read-only range the host gave cannot change.                                                                                                                                                                                     |
| `holds_read_only`               | `"Holds a read-only field: {reason}"`                                                                                                | Why renaming or moving the key above a read-only field is refused.                                                                                                                                                                             |
| `save_refused`                  | `"{pointer} is read-only and this change would alter it"`                                                                            | A save refused because it would change a read-only value.                                                                                                                                                                                      |
| `save_waits`                    | `"Not saved: {pointer} is read-only, and it can be checked only once the text is valid"`                                             | A save waiting for the text to be valid: the language cannot say where the read-only field at `{pointer}` is written, so its value is checked only in a text that parses.                                                                      |
| `read_only_changed`             | `"This edit makes the text valid, but {pointer} would no longer read as the host gave it ({reason}): undo the change to it first"`   | An edit refused because it makes a text that did not parse parse with the read-only field at `{pointer}` reading otherwise than in the host's text, and why the field is read-only (`{reason}`).                                               |
| `read_only_unchecked`           | `"Too many lines were deleted while the text was invalid to check {pointer} ({reason}): undo deleting the lines that held it first"` | An edit refused because it makes a text that did not parse parse after more lines that may hold the read-only field at `{pointer}` were deleted from the host's text than can be checked through, and why the field is read-only (`{reason}`). |
| `read_only_label`               | `"Read-only"`                                                                                                                        | The gutter lock's label, before the reason.                                                                                                                                                                                                    |
| `secret_label`                  | `"Secret placeholder"`                                                                                                               | The gutter key's label, before the hint.                                                                                                                                                                                                       |
| `secret_hint`                   | `"Keeps the current secret unless you replace it"`                                                                                   | The hover text of a secret placeholder, unless the host gives one.                                                                                                                                                                             |
| `secret_value`                  | `"the secret value"`                                                                                                                 | What a schema problem says instead of a secret field's value.                                                                                                                                                                                  |
| `unknown_placeholder`           | `"{pointer}: unknown secret placeholder; it stands for no current secret, so write the new value instead"`                           | A placeholder the host did not give, in a secret field.                                                                                                                                                                                        |
| `moved_placeholder`             | `"{pointer}: this placeholder stands for the secret of {owner}, not this field's; here it is plain text"`                            | One of the host's placeholders outside the field it stands in.                                                                                                                                                                                 |
| `env_reference`                 | `"Read the secret from an environment variable"`                                                                                     | The detail of the `${…}` reference suggestion.                                                                                                                                                                                                 |
| `file_reference`                | `"Read the secret from a file"`                                                                                                      | The detail of the `file:` reference suggestion.                                                                                                                                                                                                |
| `other_reference`               | `"A reference starting with {prefix}"`                                                                                               | The detail of another reference suggestion.                                                                                                                                                                                                    |
| `reference_type`                | `"reference"`                                                                                                                        | The type column of a reference suggestion.                                                                                                                                                                                                     |
| `closes_tag`                    | `"Closes <{name}>"`                                                                                                                  | The detail of the end tag XML suggests.                                                                                                                                                                                                        |
| `end_tag`                       | `"end tag"`                                                                                                                          | Its type column.                                                                                                                                                                                                                               |
| `key_not_allowed`               | `"\"{key}\" is not allowed here"`                                                                                                    | A key the schema does not allow (`{key}`); its object's pointer goes before it (`/web: `), as before every schema problem but at the root.                                                                                                     |
| `text_not_allowed`              | `"text is not allowed here, only elements"`                                                                                          | Text in an element the schema wants elements in (XML); the element's pointer goes before it.                                                                                                                                                   |
| `key_deprecated`                | `"{pointer}: \"{key}\" is deprecated"`                                                                                               | A deprecated key.                                                                                                                                                                                                                              |
| `key_deprecated_instead`        | `"{pointer}: \"{key}\" is deprecated. {message}"`                                                                                    | A deprecated key whose schema says what to use instead.                                                                                                                                                                                        |
| `not_one_of`                    | `"{value} is not one of {allowed}"`                                                                                                  | A value not among the allowed ones.                                                                                                                                                                                                            |
| `wrong_type`                    | `"{value} is not of type {types}"`                                                                                                   | A value of none of the accepted types.                                                                                                                                                                                                         |
| `invalid_schema`                | `"Invalid JSON Schema: {error}"`                                                                                                     | A schema that does not compile.                                                                                                                                                                                                                |
| `invalid_placeholder`           | `"secrets.placeholder: {error}"`                                                                                                     | A secret placeholder pattern the editor cannot use (it does not compile, or matches empty text): no token is taken for a placeholder.                                                                                                          |
| `problem_at_path`               | `"{path}: {message}"`                                                                                                                | A host's problem with a field the editor cannot find in the text, listed with the field's path.                                                                                                                                                |
| `host_cannot_keep`              | `"{pointer}: {host} cannot keep this value exactly; it reads back differently"`                                                      | A value the host's language reads back differently (Edit-as).                                                                                                                                                                                  |
| `edit_cannot_show`              | `"{pointer}: {edit} cannot show this value exactly; editing it here may change it"`                                                  | A value the edited language cannot show exactly (Edit-as).                                                                                                                                                                                     |
| `not_saved`                     | `"Not saved as {host}: {error}"`                                                                                                     | An edit the host's language cannot save (Edit-as).                                                                                                                                                                                             |
| `no_null`                       | `"{host} has no null value (at {pointer})"`                                                                                          | A null the host's language cannot write (Edit-as).                                                                                                                                                                                             |
| `no_non_finite`                 | `"{host} has no infinity or NaN (at {pointer})"`                                                                                     | An infinity or NaN the host's language has no number for (Edit-as).                                                                                                                                                                            |
| `cannot_edit_as`                | `"Cannot edit this {host} as {edit}: {error}"`                                                                                       | A host document that does not convert for editing.                                                                                                                                                                                             |
| `cannot_switch`                 | `"Cannot switch to {edit} yet: {error}"`                                                                                             | An Edit-as switch waiting until the document converts.                                                                                                                                                                                         |
| `switch_refused`                | `"Cannot edit as {edit}: {error}"`                                                                                                   | An Edit-as switch refused: the host's language or the other one cannot write documents.                                                                                                                                                        |
| `mixed_content`                 | `"{pointer}: text beside child elements cannot be shown in another language"`                                                        | XML text beside child elements, which no other language can show (Edit-as).                                                                                                                                                                    |
| `tab_list`                      | `"Open files"`                                                                                                                       | The tab list's accessible name, unless the host gives one.                                                                                                                                                                                     |
| `no_documents`                  | `"No open files"`                                                                                                                    | Shown in place of an editor while no document is open.                                                                                                                                                                                         |
| `close_tab`                     | `"Close {name}"`                                                                                                                     | A tab's close button.                                                                                                                                                                                                                          |
| `close_tab_title`               | `"Close (Delete)"`                                                                                                                   | Its tooltip.                                                                                                                                                                                                                                   |
| `add_document`                  | `"New file"`                                                                                                                         | The "+" button after the tabs (with `on_add`): its accessible name and tooltip.                                                                                                                                                                |
| `unsaved`                       | `"unsaved changes"`                                                                                                                  | The dirty dot's tooltip, and what a tab's accessible name adds for it.                                                                                                                                                                         |
| `read_only_tab`                 | `"read-only"`                                                                                                                        | What a read-only document's tab adds to its accessible name.                                                                                                                                                                                   |

</details>

<!-- strings:end -->

### Changes against the baseline

The gutter marks added lines (green bar), modified lines (blue bar) and
removed lines (a red mark between the lines around them). Clicking a marker
opens a preview of the lines before and now, with **Revert**: it restores
the baseline lines of that whole change as one edit (one undo step, sent
through `on_change`, converted back while editing as another language). The
footer shows the number of changes; clicking it moves to the next one.

```rust
use config_editor_leptos::{unified_diff, ChangeSummary, ConfigEditor, Language};

let saved = RwSignal::new(load());
let text = RwSignal::new(saved.get_untracked());
let changes = RwSignal::new(ChangeSummary::default());
view! {
    <ConfigEditor
        language=Language::YAML
        value=text
        baseline=saved
        on_change=move |next| text.set(next)
        on_change_summary=move |summary| changes.set(summary)
    />
    <button disabled=move || changes.get().is_empty()>
        {move || format!("Review {} changes", changes.get().hunks)}
    </button>
    // For a review dialog: `diff -u` output, 3 lines of context.
    <pre>{move || unified_diff(&saved.get(), &text.get(), 3)}</pre>
}
```

The change summary is read live from `on_change_summary`, or computed for
any two texts with the free `ChangeSummary::between(baseline, current)`;
`EditorHandle` has no method for it, by design.

### Saving and unsaved edits

Nothing is saved by itself. `Ctrl+S` / `Cmd+S` in the editor (the browser's
save dialog prevented) and `EditorHandle::save` call `on_save` with a
`SaveRequest { text, language }`: the document in the host's `language`.
While editing as another language, an edit still waiting out the typing pause
is converted first (it reaches `on_change` before `on_save`); when it cannot
be written in `language` (invalid, or a `null` for TOML), nothing is sent,
`save` returns `false`, and the caret goes to the problem with its message
shown.

<details>
<summary>Line endings (CRLF, LF and CR, mixed in one text)</summary>

Line endings are kept: the editor works on LF text, and when the host's text
ends its lines with CRLF (`value`, `initial_value`, or a new `value` later),
`on_change` and `on_save` receive the text with CRLF line endings, in every
language and while editing as another one. Each line keeps its own: a text
mixing CRLF, LF and old Mac CR endings is sent with each untouched line
ending as it did, an edited line keeps its ending, and a new line takes the
ending of the line above (the text's most common one when there is none).
A lone CR that the language does not read as a line break (TOML, HCL) is
shown as the syntax error its reader gives, until that line is edited (an
edited line then takes a break the language reads). Dirty state and the
diff compare line endings as LF. Every position the API gives or takes
(`Language::range_of`, `path_at`, `fold_ranges`, `check_syntax`,
`Schema::validate`, `TextPosition::from_offset` and `to_offset`) counts
LF, CRLF and a lone CR each as one line break, as the editor does.

</details>

The text is dirty while it differs from `baseline` as shown (the document at
mount when no `baseline` is given). `on_dirty_change` reports it once on
mount and on every change; `EditorHandle::is_dirty` reads it.

```rust
use config_editor_leptos::{BaselineConflict, ConfigEditor, EditorHandle, SaveRequest};

let saved = RwSignal::new(load());
let text = RwSignal::new(saved.get_untracked());
let dirty = RwSignal::new(false);
let editor = EditorHandle::new();
view! {
    <button disabled=move || !dirty.get() on:click=move |_| { editor.save(); }>"Save"</button>
    <ConfigEditor
        handle=editor
        value=text
        on_change=move |next| text.set(next)
        baseline=saved
        on_save=move |request: SaveRequest| {
            store(&request.text);
            saved.set(request.text); // nothing unsaved any more
        }
        on_dirty_change=move |now| dirty.set(now)
        on_baseline_conflict=move |event: BaselineConflict| log(event)
    />
}
// Another client saved: `saved.set(from_server)` moves the baseline on.
```

When `baseline` changes while the user has unsaved edits, a notice opens over
the text, "The file changed on the server.", instead of mixing the two:

<details>
<summary>Reload, Keep mine, the focus, and when a new baseline is taken silently</summary>

- **Reload** makes the new baseline the text: one undo step (`Ctrl+Z` brings
  the user's text back), sent through `on_change` (exactly the new baseline,
  also while editing as another language).
- **Keep mine** keeps the text and diffs it against the new baseline.
  `Escape` in the notice chooses it, as the choice that loses nothing.
- The notice is `role="alert"`, so screen readers announce it, and it never
  takes the focus: it opens while the user may be typing, and a key in
  flight would land on its buttons. `F6` moves the focus to Keep mine and
  back to the text (the notice says so); closing it returns the focus to
  the text.
- It closes by itself (`BaselineConflict::Settled`) when the text comes to
  equal the new baseline, or the host sets the old one back.

Without unsaved edits a new baseline is taken silently. It is also taken
silently when it already is the text, or when it is a text the editor sent
the host (`on_change`, `on_save`): a save coming back while the user typed
on is not a change from elsewhere. Colors: the notice uses `raised`,
`raised-border`, `raised-text`, `raised-muted`, `warning`, `control-bg` and
`control-text`.

</details>

### Embedding

- **Fixed at mount:** `language`, `schema` and `secrets` are read when the
  editor mounts. To change one, mount the editor again (key it on the
  schema, for example) and pass the current value, so the text is kept; the
  undo history starts over.
- **Which text to send:** `EditorHandle::value()` is the document in your
  language, pending edits applied, with your text's line endings (CRLF when
  it had them): what `on_change` and `on_save` get. `EditorHandle::text()`
  is what is on screen (LF), which while editing as another language is
  that language. Send `value()` (or what `on_change` gave you) to a server.
  `EditorHandle::unified_diff(context)` is the `diff -u` of the baseline
  against `value()`, in your language, its lines ending with LF as the JS
  `unifiedDiff()` writes them (`diff` feature).
- **Saving:** `save()` and `Ctrl+S` apply pending edits first. A Save
  button of your own that reads your own copy of the value calls
  `flush_pending_edits()` first.
- **Read-only fields and secrets need the `protect` feature** (on by
  default). Without it the props compile, do nothing, and log a console
  warning; the schema's `readOnly`, `writeOnly` and `x-secret` marks are
  ignored.
- **Size:** the editor sizes itself, not by its parent's height: its text
  area grows with the text from `--config-editor-min-height` (240px, less
  in a window too short for it) to `--config-editor-max-height` (80% of the
  window, less in a short one), then scrolls; the toolbar, tab bar and
  footer come on top. `--config-editor-height` (a length, such as `400px`)
  fixes the text area's height. To fill a container of a set height, give
  the editor `height: 100%` (`.config-editor { height: 100%; }`): the text
  area takes what the toolbar and footer leave. The custom properties go
  on `.config-editor` or any element above it. `ConfigDocuments` sizes
  itself the same way (the tab strip, then the shown editor); to fill a
  container of a set height, give the view `height: 100%`
  (`.config-editor-documents { height: 100%; }`): the strip keeps its
  height and the shown editor takes the rest.
- **Problems:** at most 20 of each source (the schema's, the secret
  placeholders') are listed and underlined, and the footer says how many
  were cut ("20+ errors · only the editor's first 20 are shown"); a problem with no
  place (the whole document's) stays in the footer and `F8` reaches it.

```rust
let editor = EditorHandle::new();
let send = move |_| {
    if let Some(document) = editor.value() {
        upload(document); // in the host's language, also while editing as another
    }
};
```

## Several documents

`<ConfigDocuments>` shows several files in one view, a main config and its
drop-ins, as VS Code-style tabs over one editor per document. Each
`Document` has its own language (from its file name unless given), text,
baseline, read-only state, `edit_as` language, schema and, optionally,
settings, host problems and protected fields; documents with
the same schema JSON share one compiled `Schema` (pass a compiled `Schema`
to share it with editors elsewhere).

```rust
use config_editor_leptos::{CloseRequest, ConfigDocuments, Document, DocumentsHandle, SaveRequest};

const SCHEMA: &str = include_str!("schema.json");
let documents = RwSignal::new(vec![
    // The main config stays open: no close button.
    Document::new("main", "burrow.yaml", load("burrow.yaml")).schema(SCHEMA).closable(false),
    Document::new("limits", "conf.d/limits.toml", load("conf.d/limits.toml")).schema(SCHEMA),
    Document::new("clients", "conf.d/clients.json", load("conf.d/clients.json"))
        .schema(SCHEMA)
        .read_only(true),
]);
let edit = move |id: &str, change: &dyn Fn(&mut Document)| {
    documents.update(|list| list.iter_mut().filter(|d| d.id == id).for_each(change))
};
let tabs = DocumentsHandle::new();
view! {
    <button on:click=move |_| { tabs.save_all(); }>"Save all"</button>
    <ConfigDocuments
        documents=documents
        handle=tabs
        on_change=move |(id, text): (String, String)| edit(&id, &|d| d.value = text.clone())
        on_save=move |(id, request): (String, SaveRequest)| {
            store(&id, &request.text);
            edit(&id, &|d| d.baseline = Some(request.text.clone()));
        }
        on_close=move |request: CloseRequest| {
            if !request.dirty || confirm_discard(&request.id) {
                documents.update(|list| list.retain(|d| d.id != request.id));
            }
        }
        // The "+" after the tabs: the host decides what a new file is.
        on_add=move |()| documents.update(|list| list.push(new_drop_in(list)))
    />
}
```

- **The host owns the list.** It adds, removes, renames and reorders
  documents by changing `documents`. A document keeps its editor while its
  `id` stays listed: a new `value`, `baseline`, `file_name`, `read_only` or
  `edit_as` updates it in place; a new `language` or schema mounts a new
  editor for it.
- **The view reports intents.** `on_select(id)` when a document becomes the
  active one (a tab, a key, `DocumentsHandle::select`, or the active one
  closing: the next tab takes its place, else the previous one).
  `on_close(CloseRequest { id, dirty })` when the user asks to close one
  (its close button, a middle click, `Delete` on its tab): nothing closes by
  itself, and `dirty` tells the host to ask first. Only closable documents
  ask: `Document::closable(false)` (switches live) keeps one open, with no
  close button, and a middle click or `Delete` on its tab does nothing.
  Without `on_close`, no tab has a close button.
- **Adding is the host's too.** With `on_add`, a "+" button follows the last
  tab (outside the tab list, after it in the `Tab` order; named and titled
  by the `add_document` text, "New file"). It only calls `on_add`: the host
  decides what a new document is and lists it, and the document it adds in
  answer is selected, its editor focused without scrolling the page. In
  answer: the first one added within 5 seconds of the click, before the
  focus moves to something outside the view (a dialog of the host's: select
  the document with `DocumentsHandle::select` then), before another tab is
  picked and before the user clicks or types anywhere else in the view (in
  a document's editor). One added later (a server's) is listed, not selected. When the tabs
  overflow, the "+" stays at the end of the strip, visible while they
  scroll. No key adds a document (`Ctrl+T` and `Ctrl+N` belong to the
  browser). Leave `on_add` unset for a view that only shows files.
- **Per-document callbacks carry the id:** `on_change((id, text))`,
  `on_save((id, SaveRequest))` (`Ctrl+S` / `Cmd+S` saves the document being
  edited), `on_dirty_change((id, dirty))` (when a document is added, then on
  every change). Each tab shows a dot while its document is dirty.
- **Switching keeps each document's state.** An editor mounts the first time
  its document is shown and then stays, hidden (`visibility: hidden`,
  `inert`) while another is active, so its caret, selection, scroll
  position, undo history, folds, find widget and Edit-as language are where
  they were. Mounting lazily keeps the start cheap; keeping editors avoids
  re-parsing on every switch. Closing a document gives its editor's memory
  back (all but under 1 KB). Measured on the release build with ten ~2000-line
  documents: about 1.5 to 1.9 MB of wasm memory per editor once shown (21 MB
  with all ten, against 8 MB with one shown and 26 MB for ten separate
  `ConfigEditor`s), about 1,000 DOM nodes per editor; 50 switches between
  shown documents add nothing (a switch, from the click to the panel shown
  as a test driver sees it, took 49 ms at the median).
- **`DocumentsHandle`:** `select(id)` (shows it and focuses its editor),
  `show(id)` (shows it, leaving the focus where it is: a host following its
  own state of which file is open), `active()`, `editor(id)` / `active_editor()` (the document's
  `EditorHandle`, attached once it has been shown), `is_dirty(id)`,
  `save(id)`, `save_all()` (the dirty documents; returns the ids sent),
  `focus()`.
- **Settings:** `settings` takes the same [`EditorSettings`](#settings) as
  `ConfigEditor`, for every editor of the view and for the tabs (`theme`,
  `colors` with the `Tab*` tokens; `styled` injects the preset stylesheet
  once for the whole view). A document's own `Document::settings` replace
  them for its editor. Both switch live. Also `class`, `label` (the tab
  list's accessible name, default "Open files"), `initial_active`.
- **Per document, as on `ConfigEditor`:** `host_diagnostics`,
  `read_only_paths` and `read_only_ranges` (updated in place when the list
  changes) and `secrets` (read when its editor mounts).

```rust
use config_editor_leptos::{Document, EditorSettings, FieldPath, HostDiagnostic, Severity};

let view_settings = EditorSettings { show_footer: true, ..EditorSettings::default() };
let drop_in = Document::new("tls", "conf.d/tls.toml", tls_text)
    // This one without folding; the rest as the view's.
    .settings(EditorSettings { folding: false, ..view_settings.clone() })
    .read_only_paths(vec![FieldPath::from("/cert")])
    .host_diagnostics(vec![HostDiagnostic::at_path("/cert", Severity::Error, "certificate expired")]);
// <ConfigDocuments documents=documents settings=view_settings />
```

Keyboard: the tab strip is a WAI-ARIA `tablist` (each tab a `tab` with its
`tabpanel`; one tab in the `Tab` order). `←` / `→` select the previous /
next tab (wrapping), `Home` / `End` the first / last, `Enter` moves into the
editor, `Delete` asks to close (a closable document's tab only); `Tab`
moves on to the "+" when there is one. From anywhere in the view, the editor
included, `Alt+PageDown` / `Alt+PageUp` switch to the next / previous
document, the focus going along. Browsers keep `Ctrl+Tab` and
`Ctrl+PageDown` for their own tabs; `Alt+PageDown` is free in browsers and
in the editor's keys, and is not an AltGr combination (AltGr is `Ctrl+Alt`
on Windows layouts). The strip scrolls sideways when the tabs overflow (a
mouse wheel scrolls it too) and keeps the active tab in view, at phone
widths included. Colors: the `Tab*` tokens, see
[Themes and colors](#themes-and-colors).

## Paths and positions

A [`FieldPath`] names a field the same way in every language: a JSON pointer
(`"/web/hosts/1"`) or segments (`FieldPath::root().key("web").key("hosts").index(1)`).
An [`EditorHandle`] passed as `handle` maps paths to the text on screen and
back, and jumps to fields:

```rust
use config_editor_leptos::{ConfigEditor, EditorHandle, FieldPath};

let editor = EditorHandle::new();
let crumb = RwSignal::new(FieldPath::root());
view! {
    <nav>{move || crumb.get().to_pointer()}</nav>
    <button on:click=move |_| { editor.reveal("/web/listen"); }>"Go to web.listen"</button>
    <ConfigEditor handle=editor on_path_change=move |path| crumb.set(path) value=text />
}
```

| `EditorHandle` method         | Description                                                                                                                                                                                                                                                                                                    |
| ----------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `range_of(path)`              | `Option<TextRange>`: the key and its value when they share a line (`listen: 7835`, `<listen>7835</listen>`), the key alone when the value is a block below it, the value alone for a list item, the dash or header of an item holding a block (`- name: web`, a repeated HCL block, a TOML `[[array]]` entry). |
| `path_at(position)`           | `Option<FieldPath>` of what a `TextPosition` is on: the key under or just before it, else the value or container holding it (the root at the top level).                                                                                                                                                       |
| `current_path()`              | The caret's path. Reactive: read it in a closure to follow the caret (as `on_path_change` does). Computed only while something reads it.                                                                                                                                                                       |
| `reveal(path)`                | Scrolls to the field, selects `range_of(path)` and focuses the editor; `false` when there is no such field.                                                                                                                                                                                                    |
| `reveal_range(range)`         | Selects any `TextRange` of the text on screen (a comment, a blank line, part of a value), scrolls it into view inside the editor and focuses the editor without moving the page, as `reveal` does; clamped to the text's end; `false` before the editor mounts and for an empty document.                      |
| `fold(path)` / `unfold(path)` | Fold the block the field's key opens / open the folds around the field; `false` when there was nothing to do.                                                                                                                                                                                                  |
| `fold_all()` / `unfold_all()` | Fold every block / open every fold.                                                                                                                                                                                                                                                                            |
| `folded_lines()`              | The closed folds as one-based `(first, last)` line pairs. Reactive.                                                                                                                                                                                                                                            |
| `focus()`, `text()`           | Focus the editor; the text on screen.                                                                                                                                                                                                                                                                          |

Positions (`TextPosition`: one-based `line` and character `column`) refer to
the text on screen, which is the `edit_as` language while editing as another
one; paths are the same in every language. Before the editor mounts the
handle answers `None`. The same lookups work headless:
`Language::YAML.range_of(text, "/web/http")` and
`Language::YAML.path_at(text, TextPosition::new(2, 5))`, and for many paths
at once `Language::YAML.ranges_of(text, paths)` (the text read once for
all of them). A language of your
own gets them from its `key_span`, `value_span`, `hover_key` and
`completion_context`.

## Host diagnostics

When a server validates the document, pass its problems as
`host_diagnostics`. Each points at a field by path (preferred: it follows the
field through edits and `edit_as`) or at a range of the host's text, or at
the whole document:

```rust
use config_editor_leptos::{ConfigEditor, EditorSettings, HostDiagnostic, Severity};

let problems = RwSignal::new(Vec::new());
// After the server answered:
problems.set(vec![
    HostDiagnostic::at_path("/web/listen", Severity::Error, "port 80 is taken")
        .with_code("E_PORT")
        .with_source("burrow")
        .with_docs_url("https://example.com/docs/ports"),
    HostDiagnostic::document(Severity::Warning, "restart needed to apply"),
]);
let settings = EditorSettings { show_footer: true, ..EditorSettings::default() };
view! { <ConfigEditor value=text host_diagnostics=problems settings=settings /> }
```

<details>
<summary>How they show: placement, ranges, severities, duplicates, origin</summary>

- They are marked where the field is written, as schema problems are (the
  value, or the key of a block), next to the editor's own problems: `F8` /
  `Shift+F8` stop at them, the footer counts them, and the hover message
  adds who reported them, the code, and "Open docs ↗" (a click on the
  message opens `docs_url` when it is an `http:` or `https:` URL (another
  is not linked), as `Alt+Enter` does with the caret
  on the problem; screen readers hear the code and the link with the
  problem, in the caret's description and on `F8`). What cannot be placed (a missing field, the
  whole document) shows in the footer, a field's problem after its path
  (`/web/listen: port 80 is taken`).
- A range is underlined on every line it covers (up to 200; the
  `Diagnostic` has an `end_line`), and the footer, `F8` and screen readers
  name it once, at its start. While editing as another language it marks
  the field the range starts in (its key when it starts on the key), as a
  path would; a range starting in no field shows in the footer.
- At most 200 underlines are drawn in all, the most serious problems
  first (errors, then warnings, info and hints; each problem's start
  before the rest of its lines); the footer says when some were left out,
  and `F8` still reaches every problem.
- Severities `Error`, `Warning`, `Info` (a blue wavy underline) and `Hint`
  (a dotted one), colored by `--config-editor-error`, `-warning`, `-info`
  and `-hint` (`ThemeColor::Info`, `ThemeColor::Hint`).
- Each problem shows once: where the host marks the same text as one of the
  editor's own with the same message (with or without its JSON pointer),
  only the host's shows; a different message shows beside it. A new list replaces the previous one,
  and only the markers update.
- Every `Diagnostic` says where it came from (`origin`:
  `DiagnosticOrigin::Editor` or `Host`) and carries `code`, `source` and
  `docs_url`.

</details>

## Read-only fields

A field is read-only when the schema says `readOnly: true` (its reason, if
any, in `x-readonly-reason`), when the host names it in `read_only_paths`
(a whole subtree each), or when it lies in one of the host's
`read_only_ranges`. Both props may be signals that change at runtime; the
whole-document `read_only` stays as it is.

```rust
use config_editor_leptos::{ConfigEditor, FieldPath, TextPosition, TextRange};

// {"properties": {"id": {"type": "string", "readOnly": true,
//                        "x-readonly-reason": "Assigned by the server"}}}
let locked = RwSignal::new(vec![FieldPath::from("/limits")]);
let first_line = RwSignal::new(vec![TextRange::new(
    TextPosition::new(1, 1),
    TextPosition::new(2, 1),
)]);
view! {
    <ConfigEditor
        value=text
        schema=SCHEMA
        read_only_paths=locked
        read_only_ranges=first_line
    />
}
```

<details>
<summary>How locks behave: refused edits, formatting, syntax errors, suggestions, Edit-as, folding</summary>

- A lock shows in the gutter on each line the field covers, its value is
  muted (`--config-editor-locked`, `ThemeColor::Locked`), and hovering the
  value or the lock shows the reason.
- Every way of changing the text is checked the same way, after it is made
  and before it is applied: typing, Backspace and Delete, paste, cut, drop,
  completions, Replace, `Ctrl+/`, Revert, undo and redo. An edit that would
  change a read-only value, delete its key (also by deleting a selection or
  a parent block that holds it), rename a key holding it, or add to a
  read-only block is refused whole: the text stays as it was, and a short
  tooltip at the caret says why. Typing refused part-way leaves nothing
  half-written: what was typed of that entry on the line goes too (`<i` of
  a refused `<id>`), and the keys typed there after it are refused, until
  the caret moves. Replace All replaces the other matches and reports how
  many it skipped.
- Formatting around and inside a locked field stays free: blank lines,
  trailing spaces, a comment after the value, punctuation next to it (`,`
  in JSON), a space after `:` or `=`, and re-indenting where indentation is
  not structure (JSON, TOML, HCL, XML). In YAML, where the indent says
  which block a key belongs to, changing a locked line's indent is refused.
  A host's read-only range is text: nothing in it changes.
- A syntax error anywhere keeps every lock, both ways: a read-only value
  never changes in anything sent to you (`on_change`, a save, `value()`),
  and an edit that leaves every read-only value as it was is never
  refused. While the document does not parse, the locks found in it keep
  their text, but for whitespace between its tokens (the formatting above;
  a space inside a string or a YAML plain value, or a YAML locked line's
  indent, is refused). The edit that makes it parse again is the one that
  decides: each read-only field must read as in your text (the value it
  had there, or, when your text never parsed, the same lines with only
  closers such as `}`, `]` or an end tag added, or the quote, `>` or `:`
  its own line left out). Closing a block left open, removing a stray
  line, fixing a quote, or closing the read-only line itself goes through;
  so does a repair when a read-only path is not in the document at all,
  and in a text of any size. A repair that would change a read-only value
  (deleting its line included) is refused with a message naming the field
  (the `read_only_changed` text). This holds
  whatever order the text and the locks come in (a `value` that does not
  parse, at mount or later; paths named while the text does not parse).
- Suggestions leave read-only keys and values out. Inside a read-only
  region (browsing it with `Ctrl+Space`) they are listed, with the reason as
  their detail.
- While editing as another language, the same fields are locked there,
  found by path in that language (a host range locks the fields it covers).
  An Edit-as save that would still change a read-only value, by any route,
  is refused like other unsafe saves: it is not sent, and an error says
  which field.
  `Ctrl+S` (`on_save`) is refused the same way.
- The host's own text is never held back: a new `value`, and a baseline
  taken with "Reload" from the changed-on-server notice, may change
  read-only values (and bring their own secret placeholders). A refused
  edit changes nothing, so it leaves the dirty flag as it was.
- With folding, an edit that removes or changes a fold holding a locked
  value is refused like any other, and the fold stays closed. A folded
  block holding a read-only line or a secret placeholder shows its lock or
  key on the fold's row.

</details>

## Secrets

Fields the schema marks `writeOnly: true` or `x-secret: true` are secrets.
The host never sends their values: it writes a placeholder token instead
(`«secret 3f2a»` by default), and the editor keeps it as it is unless the
user replaces it.

```rust
use config_editor_leptos::{ConfigEditor, SecretOptions};

// tls:
//   key: «secret 3f2a»
let secrets = SecretOptions::default()
    .with_placeholder(r"«secret [0-9a-f]+»") // the default
    .with_matcher(|token| token.len() < 40)  // optional: which matches are real
    .with_hint("Keeps the current secret unless you replace it")
    .with_reference_prefixes(["${", "file:"]);
view! { <ConfigEditor value=text schema=SCHEMA secrets=secrets /> }
```

<details>
<summary>How placeholders behave: where they count, atomic editing, chips, references, validation, Edit-as</summary>

- A placeholder is real only in the field the host gave it in: a match of
  the pattern (and of the host's check) found in that field of the host's
  own `value`, at most as often as there. Anywhere else it is plain text,
  so it can never stand for another field's secret: one of the host's
  tokens pasted into another field, or left under a renamed key, is flagged
  (it stands for the secret of the field it came from), and so is a token
  the host did not give in a secret field (it stands for no current
  secret). Like the schema's problems, at most 20 are flagged; past that, a
  note says more follow (the footer counts "20+ warnings"). Put back into
  its own field (cut and pasted back, the key's name restored), it is real
  again, since the field then holds what the host gave. The schema check
  accepts a placeholder only in its own field.
- A placeholder is atomic: the caret never lands inside it (clicks, arrows,
  `Home`/`End` and selections snap to its edges; `←`/`→` step over it),
  `Backspace` or `Delete` next to it removes it whole, an edit or a
  selection cutting into it replaces it whole, and copying it copies the
  token as it is. Undo brings it back as a placeholder.
- It is drawn as a chip (`--config-editor-secret-chip`,
  `--config-editor-secret-chip-border`: `ThemeColor::SecretChip`,
  `SecretChipBorder`), with a key in the gutter; hovering it shows the hint.
- In a secret field's value, suggestions offer references instead of
  values: `${VAR}` (the caret inside the braces) and `file:`; HCL writes
  `${` escaped as `$${`, so the text survives conversion.
- The schema check takes an untouched placeholder as the current secret:
  no `pattern`, `format`, length or type problem is reported on it, and
  messages about a secret field never quote what was typed there.
- Edit-as conversions and saves keep each placeholder byte-for-byte, in
  every language pair and back. Real secret values never reach the editor;
  it logs nothing about field values.

</details>

## Schema help

Completions and hover read these keywords, with or without the
`schema-validation` feature:

| Keyword                                      | Shown                                                                                                                                                                                        |
| -------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `description`, `type`, `default`, `examples` | Suggestion detail and hover. A key is marked `required` when its parent lists it.                                                                                                            |
| `deprecated: true`                           | The key is struck through and listed last, its hover says so, and a key written in the document gets a warning on the key, struck through (`Diagnostic::deprecated`).                        |
| `deprecationMessage`                         | What to use instead, in the hover and the warning.                                                                                                                                           |
| `x-docs-url`                                 | A "Documentation" link in the hover, and an "Open docs" link (and `Alt+Enter`) on the field's problems, opening in a new tab (`http(s)` addresses only).                                     |
| `x-enum-descriptions` (and `x-enum-titles`)  | Arrays parallel to `enum`: each value's own line in the suggestion list and in the hover of that value in the document.                                                                      |
| `title` / `description` of a `const` branch  | The same for `oneOf`/`anyOf` lists of documented constants.                                                                                                                                  |
| `title` / `description` of an object branch  | Which `oneOf`/`anyOf` option the object is in, in the hover of its key ("One of" all titles until one matches); each key suggested inside names its option, the matched option's keys first. |

```json
{
  "mode": {
    "description": "How requests are routed.",
    "x-docs-url": "https://example.com/docs/mode",
    "enum": ["fast", "safe"],
    "x-enum-descriptions": ["Skips the checks.", "Checks everything."]
  },
  "level": {
    "oneOf": [
      { "const": "info", "title": "Info", "description": "Normal messages." },
      { "const": "trace", "title": "Trace", "description": "Everything." }
    ]
  },
  "timeout": {
    "type": "integer",
    "deprecated": true,
    "deprecationMessage": "Use deadline instead."
  }
}
```

<details>
<summary>Option matching, required children on accept, patternProperties</summary>

An object option is matched cheaply and exactly, never by validating:
the one whose `required` keys are all present (`dns: {cloudflare: …}`),
else the one whose `const` properties the object has
(`provider: static`), else none.

Accepting an object key writes its required children. The caret goes to
the first one (left empty unless it has a `default`); each other takes its
`default`, else its first `examples` entry, else a typed placeholder. `Tab`
then walks the block like a snippet, selecting each written value (typing
replaces it) and stopping at each empty one.

`patternProperties` regexes are compiled once per pattern, when the schema
is built. One the editor's engine cannot compile (look-around, and Unicode
classes without `schema-validation`) is reported through
`on_schema_warning` and [`Schema::warnings`] instead of silently matching
nothing. `pattern` itself is only checked by the validator, which rejects a
schema whose regex it cannot compile.

</details>

## Keyboard

| Keys                                         | Action                                                                                                                     |
| -------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `Ctrl+Space`                                 | Open suggestions (with schema examples)                                                                                    |
| `↑` `↓` `PageUp` `PageDown`, `Enter` / `Tab` | Move through suggestions, accept                                                                                           |
| `Tab` after accepting a block                | Jump to the next empty value, or select the next value the block wrote                                                     |
| `Tab` / `Shift+Tab`                          | Indent / outdent the line or selected lines                                                                                |
| `Ctrl+M`                                     | Toggle Tab moves focus: `Tab` / `Shift+Tab` leave the editor instead of indenting                                          |
| `Escape`, then `Tab` / `Shift+Tab`           | Leave the editor once (when the `Escape` had nothing to close)                                                             |
| `Enter`                                      | New line with the right indentation; steps out of a just-typed string or element                                           |
| `Ctrl+Enter` / `Ctrl+Shift+Enter`            | Open a line below / above                                                                                                  |
| `Ctrl+/`                                     | Toggle comments                                                                                                            |
| `Ctrl+Z`, `Ctrl+Shift+Z` / `Ctrl+Y`          | Undo, redo: the last 500 steps, fewer in a large text (they keep up to 64 MB of earlier texts)                             |
| `F8` / `Shift+F8`                            | Next / previous problem                                                                                                    |
| `Alt+Enter` on a problem with a docs link    | Open its documentation in a new tab (else the hover card's Documentation link)                                             |
| `Ctrl+K Ctrl+I`                              | Show the schema docs of the key at the caret, read to screen readers (`Escape`); in `Ctrl+M` mode, `Tab` reaches its links |
| `Alt+F5` / `Shift+Alt+F5`                    | Next / previous change against the baseline                                                                                |
| `Alt+F3` / `Shift+Alt+F3`                    | Next / previous change, opening its preview with the focus on Revert                                                       |
| `Enter` or `R` in a change preview           | Revert this change (`Escape` closes the preview and returns to the text)                                                   |
| `Ctrl+G`                                     | Go to line: type `line` or `line:column`, `Enter` to go, `Escape` to cancel                                                |
| `Ctrl+S`                                     | Save through `on_save` (left to the browser without it)                                                                    |
| `F6` while the baseline notice is open       | Move the focus to its Keep mine button, and back to the text                                                               |
| `Escape` in the baseline notice              | Keep mine: keep the text, diff it against the new baseline                                                                 |
| `Ctrl+Shift+[` / `Ctrl+Shift+]`              | Fold the innermost block around the caret (again: the one around it) / unfold                                              |
| `Ctrl+K Ctrl+0` / `Ctrl+K Ctrl+J`            | Fold everything / unfold everything                                                                                        |
| `Home`                                       | First non-blank character, then the line start                                                                             |
| `Ctrl+C` / `Ctrl+X` with nothing selected    | Copy / cut the whole line (pasting it inserts above)                                                                       |
| `Ctrl+F` / `Ctrl+H`                          | Find / find and replace (the selection, on one line, becomes the query)                                                    |
| `Enter` / `Shift+Enter`, `F3` / `Shift+F3`   | Next / previous match                                                                                                      |
| `Alt+C` / `Alt+W` / `Alt+R` in find          | Match case / whole word / regular expression                                                                               |
| `Enter` / `Ctrl+Alt+Enter` in replace        | Replace the current match / all matches (one undo step)                                                                    |
| `Escape`                                     | Close popups and the find widget, then collapse the selection                                                              |
| `Alt+PageDown` / `Alt+PageUp`                | Next / previous document in a `ConfigDocuments` view                                                                       |

`Cmd` works in place of `Ctrl` on macOS.

### Keyboard and screen-reader access

`Tab` indents while editing, so it would trap the focus. Two ways out, as in
VS Code: `Ctrl+M` switches `Tab` and `Shift+Tab` to moving the focus to the
next or previous element of the page (announced, and shown in the footer as
"Tab moves focus" until `Ctrl+M` again or a click on it), and right after an
`Escape` that had nothing to close (no suggestions, find widget, hover card
or preview), one `Tab` or `Shift+Tab` leaves. While the suggestion list is
open, `Tab` still accepts. A read-only editor never indents, so there `Tab`
always moves the focus. In a [`ConfigDocuments`](#several-documents) view,
the other documents' editors are inert: `Tab` out of the editor goes on to
its footer and what follows the view, `Shift+Tab` back towards the tabs.

<details>
<summary>Screen-reader descriptions and announcements, touch screens</summary>

The textarea is described (`aria-describedby`) by what is at the caret: the
message of each problem there, the editor's or the host's ("Warning from
burrow: port in use"), why a read-only value is read-only, a secret
placeholder's hint. It follows the caret a moment after it stops. `F8` and
`Shift+F8` announce the problem they move to through a polite live region
("Error 2 of 5: …"). The suggestion list is a `listbox`, hover help a
`tooltip`, the change preview a `dialog`, and the footer's counts
`aria-live`.

A click, double-click, drag or tap on underlined text reaches the text (the
caret lands where it is clicked), and the tip lists every problem covering
the place. On a touch screen a tap places the caret, a tap on underlined or
read-only text shows its message (there is no hover), and the selection a long press
or the selection handles make is followed like one made with the keyboard.

</details>

```rust
// Nothing to set up: the keys and descriptions are always there. The
// footer's indicator is part of the built-in footer (`show_footer`).
view! { <ConfigEditor initial_value=text /> }
```

### Find and replace

`Ctrl+F` opens a find widget over the text; `Ctrl+H` adds the replace row
(never in a read-only editor). Options match case, whole words, or a regular
expression (the `regex` engine with `schema-validation`, else `regex-lite`,
whose case folding is ASCII-only); an invalid pattern is shown, not thrown.
A regular expression's replacement reads as in VS Code: `$1`–`$99` insert
groups (`$1_host` is group 1, then `_host`), `$<name>` or `${name}` a named
group, `$&` or `$0` the whole match, `$$` a `$`, and `\n` and `\t` a line
break and a tab. Replacing goes through the normal edit
path: undo, `on_change` and Edit-as all see it, and replace all is one undo
step. Up to 10,000 matches are counted and stepped through, and only those
in view are drawn, so large documents stay fast.

### Folding

Each line that starts a block gets a chevron in the gutter, shown while the
pointer is over the gutter (always on touch screens and on folded lines),
as in VS Code. A folded line shows `…` after its text; clicking it or the
chevron opens the fold. What folds comes from the language
([`fold_ranges`](#custom-languages)): YAML mappings, sequences and block
scalars by indentation; JSON objects and arrays; TOML tables and arrays of
tables (header to the next header) and multi-line arrays, inline tables and
strings; HCL blocks, objects, lists, calls and heredocs; XML elements,
comments and CDATA over several lines; runs of line comments and multi-line
block comments everywhere. A closing bracket or tag on its own line stays
visible.

<details>
<summary>How folds behave through edits, caret moves, problems and large documents</summary>

Folding only changes the view. `value`, `on_change`, undo and saving see
the whole text, and copying or cutting a selection across a fold takes the
hidden lines. Hidden lines are never edited by accident: an edit that
touches a fold opens it first (typing at the end of a folded line,
Backspace at the start of the line after it, Delete at its end, a paste or
replace over a selection reaching into it), then applies to the whole text.
Editing the folded line before its end keeps the fold. Whatever moves the
caret into a fold opens it: find matches, go to line, `F8`, `Alt+F5`,
`EditorHandle::reveal`, a click on a problem, a completion's jump, undo.

A folded block that hides an error marks its line number as an error, and
one that hides changes gets a dotted change bar. Folds move with the lines
around them through edits and controlled `value` updates; one whose
structure is gone (its closing bracket deleted) disappears. The line ranges
are computed again after each edit, in one pass over the text, and the rows
stay virtualized: a 10,000-line document folds and scrolls to its end like
an unfolded one.

</details>

```rust
let editor = EditorHandle::new();
view! {
    <button on:click=move |_| editor.fold_all()>"Collapse all"</button>
    <button on:click=move |_| { editor.unfold("/web/listen"); }>"Show web.listen"</button>
    <ConfigEditor handle=editor value=text />
}
```

### Large documents

Only the rows in view are rendered, and typing, scrolling, find, folding
and selecting stay within their budgets on 6,000–8,000-line documents
(`e2e/tests/large-files.spec.mjs`). One thing on the host's page can slow
selecting down: in Chrome, while the page holds a closed `<details>` or an
element with `hidden="until-found"` anywhere, selecting many lines of a
textarea takes time quadratic in the lines selected (`Ctrl+A` over 8,000
lines takes seconds). A disclosure button (`aria-expanded` and `hidden`)
in their place keeps it instant; the demo pages use one.

## Headless API

The same engine without the component:

```rust
use config_editor_leptos::{check_syntax, Language, Schema};

let schema = Schema::compile(include_str!("schema.json"))?;
// At most 20; past that, a last one without a position says more follow
// (its `code` is "config-editor/more-problems").
let problems = schema.validate(Language::TOML, &text); // Vec<Diagnostic>
let syntax_only = check_syntax(Language::JSON, &text);

let value = Language::YAML.parse(&text)?;              // serde_json::Value
let yaml = Language::YAML.serialize(&value)?;          // only if it reads back the same
let hcl = Language::YAML.convert(&text, Language::HCL)?;
assert!(hcl.losses.is_empty());
let xml = schema.convert(Language::YAML, &text, Language::XML)?; // schema-aware

let language: Language = "yml".parse()?;               // names and extensions
let language = Language::from_path("infra/main.tf");   // Some(Language::HCL)
```

<details>
<summary>Serialize round trips, infinities and NaN</summary>

- `serialize` writes only a document that reads back as exactly the value
  given: it fails (`ConvertError::Unsupported`, naming the value) where
  `convert` would report a loss, such as an XML empty, nested or one-item
  list, or an infinity in JSON. It is the same check, so the two agree.
- Infinities and NaN (YAML `.inf`, `-.inf`, `.nan`; TOML `inf`, `-inf`,
  `nan`) stay numbers. A `serde_json::Value` cannot hold them, so the data
  model holds each as a reserved string (a Unicode noncharacter, U+FDD0,
  then `inf`, `-inf` or `nan`), which `float_value` makes and `float_of`
  reads (`float_of(&value["rate"]) == Some(f64::INFINITY)`). YAML and TOML
  write them back as numbers; JSON, HCL and XML have none, so a conversion
  reports them as losses and an Edit-as save into one of them waits with a
  problem (an Edit-as view in one of them shows the text `inf` and keeps the
  host's number unless the user edits it). Schema validation reads them as
  numbers beyond every bound (NaN beyond both `minimum` and `maximum`),
  never integers or multiples of anything. A YAML plain number too large
  for a float (`1e400`) is an infinity. A custom language says it writes them with
  `ConfigLanguage::writes_non_finite`. A real string spelled that way stays
  a string: parsers hold it with one more U+FDD0 (`string_value` makes it,
  `string_of` reads it back), and every writer takes that one off; build a
  string with `string_value` when its text may start with U+FDD0 (a custom
  language's parser does it for each string it reads).

</details>

| Item                                       | Description                                                                                                                                                                                                                                                                                       |
| ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Language`                                 | A language handle: `YAML`, `JSON`, `TOML`, `HCL`, `XML`, `ALL`, `new(&'static dyn ConfigLanguage)`. Methods: `parse`, `serialize`, `convert`, `label`, `extensions`, `file_icon`, `default_file_name`, `from_path`; `FromStr` accepts names and extensions.                                       |
| `Schema`                                   | A compiled JSON Schema: `compile`, `validate(language, text)`, `convert(from, text, to)`, `warnings()`. Cheap to clone and share.                                                                                                                                                                 |
| `SchemaSource`                             | What the `schema` prop takes: JSON text or a `Schema`.                                                                                                                                                                                                                                            |
| `FieldPath`, `TextPosition`, `TextRange`   | Paths (JSON pointers or segments) and one-based places in the text; `Language::range_of(text, path)` and `Language::path_at(text, position)` map between them.                                                                                                                                    |
| `language::FoldRange`                      | A range that folds (0-based `start` stays visible, lines through `end` hide); `Language::fold_ranges(text)` lists them.                                                                                                                                                                           |
| `check_syntax(language, text)`             | Syntax problems only.                                                                                                                                                                                                                                                                             |
| `unified_diff(baseline, current, context)` | The `diff -u` of two texts (`--- baseline` / `+++ current` headers, `@@` hunks with `context` lines around changes, `\ No newline at end of file`); empty when they have the same lines. Line endings are compared as LF.                                                                         |
| `ChangeSummary`                            | `added`, `removed`, `modified` lines and `hunks` (separate changes), as the diff gutter counts them; `ChangeSummary::between(baseline, current)`, `is_empty()`.                                                                                                                                   |
| `Diagnostic`                               | `line`, `column`, `end_column` (one-based, `None` when unplaced; `end_line` when the marker covers several lines), `message`, `severity` (`Severity::Error`, `Warning`, `Info`, `Hint`), `origin`, `deprecated` (a deprecated key's warning), and for host problems `code`, `source`, `docs_url`. |
| `HostDiagnostic`, `DiagnosticTarget`       | A problem the host found, by path, range or for the whole document; see [Host diagnostics](#host-diagnostics).                                                                                                                                                                                    |
| `Conversion`                               | `text`, and `losses`: JSON pointers of values the target reads back differently.                                                                                                                                                                                                                  |
| `ConvertError`                             | `Parse(ParseError)` or `Unsupported(message)` when the target cannot express the document.                                                                                                                                                                                                        |
| `ParseError`                               | `message`, `line`, `column`.                                                                                                                                                                                                                                                                      |
| `EditorTheme`, `ThemeColor`, `ThemeColors` | Themes and color overrides (below).                                                                                                                                                                                                                                                               |
| `PRESET_CSS`                               | The stylesheet the editor injects.                                                                                                                                                                                                                                                                |

## XML mapping

Child elements and attributes are object properties, element text is a typed
scalar (`<listen>7835</listen>` is the number 7835), and repeated siblings are
an array. An attribute sharing its name with a child element is the property
`@name` (`<r a="1"><a>2</a></r>` is `{"r": {"@a": 1, "a": 2}}`), so the two
stay apart. A top-level element marked `config-root="true"` is a transparent
wrapper; without it, every top-level element is a document property.

## Custom languages

Every built-in language implements the public `ConfigLanguage` trait, and
yours can too. Only `info` and `parse` are required; each further method adds
a capability (diagnostic positions, completion, hover, highlighting, editing,
conversion):

```rust
use config_editor_leptos::Language;
use config_editor_leptos::language::{ConfigLanguage, LanguageInfo, ParseError};

struct DotEnv;

impl ConfigLanguage for DotEnv {
    fn info(&self) -> LanguageInfo {
        LanguageInfo { label: "ENV", icon: "E", extensions: &["env"] }
    }
    fn parse(&self, source: &str) -> Result<serde_json::Value, ParseError> {
        todo!("KEY=value lines into a JSON object")
    }
}

static DOTENV: DotEnv = DotEnv;
const ENV: Language = Language::new(&DOTENV);
// <ConfigEditor language=ENV .../>, ENV.convert(text, Language::YAML), …
```

| Method                                                                                              | Adds                                                            |
| --------------------------------------------------------------------------------------------------- | --------------------------------------------------------------- |
| `info`, `parse` (required)                                                                          | Name, icon, extensions; syntax errors and the data model        |
| `serialize`                                                                                         | Conversion into the language (Edit-as, either way)              |
| `key_span`, `value_span`                                                                            | Positions for schema diagnostics and read-only fields           |
| `completion_context`, `existing_keys`, `property_insert`, `format_value`                            | Schema completion; see [below](#completion-and-hover)           |
| `hover_key`                                                                                         | Hover docs                                                      |
| `highlight`, `comments`, `multiline_strings`                                                        | Syntax colors, comments and strings over several lines included |
| `newline`, `toggle_comments`, `auto_close`, `complete_on_type`, `reindent_on_type`, `lexical_state` | Editing behavior                                                |

Read-only fields hold in a language without spans too: the editor then
compares the field's value on every edit whose text parses, and refuses one
that changes it (with spans the field is also muted, locked in the gutter,
and refused as it is typed on). While the text does not parse, an edit is
tried on the last text that parsed, line by line: one that changes a
read-only value there (typed into it while another line is broken, the
line holding it replaced or broken) is refused, and a save waits until the
text parses again, saying which field it cannot check (`save_waits` of
the `strings` setting). A language without `serialize` is never
edited as another language, nor another as it: the switch is refused with
the reason.

### Completion and hover

The editor's completion and hover read the schema; your language only says
where the caret is and how an entry is written. For an INI file
(`[section]` headers, `key = value` lines):

| Method               | Returns                                                                                                                                                                    |
| -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `completion_context` | The caret's place: `path` (`[Key("server")]` under `[server]`), the `typed` word and the UTF-16 range it replaces, and `value_key` after `key =`, where values are offered |
| `existing_keys`      | The keys already in the mapping at `path`, which are not offered again                                                                                                     |
| `property_insert`    | The text an accepted key writes and the caret in it: `("port = ", 7)`, or `[server]` and a line break for an object                                                        |
| `format_value`       | A schema value (`enum`, `boolean`, `default`) as the document writes it: `fast` rather than `"fast"`                                                                       |
| `hover_key`          | The key under a column of a line and the column it starts at; hover shows its schema docs                                                                                  |

A flat language (one `KEY=value` entry per line, no nesting) needs none of
that code: its `config().separators` give all five, through
`LanguageConfig::flat_completion_context`, `flat_existing_keys`,
`flat_property_insert`, `flat_format_value` and `flat_hover_key` (the
crate's `tests/flat_language_completion.rs`). This is what a JavaScript
language registered with only `separators` gets.

`Schema::suggestions` and `Schema::hover` give what the editor would show,
so a language checks its completion and hover in plain tests, no browser
needed. The crate's `tests/ini/mod.rs` is the whole INI language, and
`tests/custom_language_completion.rs` its tests.

```rust
use config_editor_leptos::{Schema, TextPosition};

let schema = Schema::compile(r#"{"properties": {"server": {"properties": {
    "port": {"type": "integer", "description": "Port to listen on"}}}}}"#)?;
let text = "[server]\npo";
let offered = schema.suggestions(INI, text, 11); // the caret: a UTF-16 offset
assert_eq!(offered[0].label(), "port");
assert_eq!(offered[0].accept(text).0, "[server]\nport = ");
let docs = schema.hover(INI, "[server]\nport = 80\n", TextPosition::new(2, 1));
assert_eq!(docs.unwrap().description.as_deref(), Some("Port to listen on"));
```

### Describing syntax as data

Simple syntax doesn't need code: describe it with `config`, as VS Code's
`language-configuration.json` does, and comment toggling, Enter between
brackets, and the key/value behavior below follow from it.

<details>
<summary>The LanguageConfig fields, with an example</summary>

```rust
use config_editor_leptos::language::LanguageConfig;

fn config(&self) -> LanguageConfig {
    LanguageConfig {
        line_comments: &[";", "#"],     // Ctrl+/ writes the first
        block_comment: None,            // e.g. Some(("/*", "*/")): Ctrl+/ unwraps it
        brackets: &[('[', ']')],        // Enter opens them across lines
        separators: &["="],             // key = value
        quotes: &["\""],                // strings, longest first
    }
}
```

</details>

### Optional refinements

Small hooks that polish editing for some kinds of languages. Each defaults to
what `config` describes (or to nothing), so skip the ones that don't fit, and
override one only when your syntax needs more than `config` says: HCL
overrides `value_follows_key` for its `name { … }` blocks, XML overrides them
all.

<details>
<summary>The hooks by kind of language, and their helpers</summary>

| Kind of language                        | Method                                        | Adds                                                                                                        | Helper                                                                                                             |
| --------------------------------------- | --------------------------------------------- | ----------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| Key/value (`key: value`, `key = value`) | `value_follows_key`                           | Completing a key that already has a value renames just the key                                              | —                                                                                                                  |
|                                         | `line_has_value`                              | A space typed after a complete value ends the entry                                                         | `value_after_separator`                                                                                            |
|                                         | `empty_value_slots`                           | `Tab` walks the empty values of an accepted block                                                           | `empty_slots_at_line_ends`                                                                                         |
| Markup (`<k>…</k>`)                     | `closing_markup_len`                          | Enter after a typed value steps past the end tag the editor wrote                                           | —                                                                                                                  |
| With trailing comments                  | `comment_start`                               | Edit-as keeps a line's comment when the line is renamed or moves                                            | `comment_start_outside_quotes`                                                                                     |
| With values over lines (`[` … `]`)      | `multiline_constructs`, `highlight_continued` | Each line of an array, a flow collection or a start tag written over several lines is colored as its inside | `MultilineConstruct::structure`, `MultilineConstruct::colored`                                                     |
| Any                                     | `fold_ranges`                                 | What folds; the default folds by indentation, `config`'s brackets and comments, and multi-line strings      | `indent_fold_ranges`, `bracket_fold_ranges`, `comment_fold_ranges`, `string_body_fold_ranges`, `merge_fold_ranges` |

`config_editor_leptos::language` also exports the other helpers the built-in
languages use (tokens, schema lookups, inline-container scanning, indent
detection). `tests/custom_language.rs` is a complete example.

</details>

## Themes and colors

Eleven themes ship as `EditorTheme` (all of them in `EditorTheme::ALL`;
their slugs parse with `FromStr`, as the JS package names them).

<details>
<summary>The eleven themes</summary>

| `EditorTheme`    | Slug              | Name                       | Look  |
| ---------------- | ----------------- | -------------------------- | ----- |
| `VsCodeDark`     | `vscode-dark`     | VS Code Dark (the default) | dark  |
| `VsCodeLight`    | `vscode-light`    | VS Code Light              | light |
| `Obsidian`       | `obsidian`        | Obsidian Dark              | dark  |
| `ObsidianLight`  | `obsidian-light`  | Obsidian Light             | light |
| `Dracula`        | `dracula`         | Dracula                    | dark  |
| `Nord`           | `nord`            | Nord                       | dark  |
| `AtomOneDark`    | `atom-one-dark`   | Atom One Dark              | dark  |
| `GitHubLight`    | `github-light`    | GitHub Light               | light |
| `GitHubDark`     | `github-dark`     | GitHub Dark                | dark  |
| `SolarizedLight` | `solarized-light` | Solarized Light            | light |
| `ViraInspired`   | `vira-inspired`   | Vira-inspired (original)   | dark  |

</details>

Override any color for a brand palette; every token is a `ThemeColor`:

```rust
use config_editor_leptos::{EditorSettings, EditorTheme, ThemeColor, ThemeColors};

let brand = ThemeColors::new()
    .set(ThemeColor::Background, "#101418")
    .set(ThemeColor::Key, "#ffcc66")
    .set(ThemeColor::Selection, "rgb(255 204 102 / 25%)");
let settings = EditorSettings { theme: EditorTheme::Nord, colors: brand, ..EditorSettings::default() };
// <ConfigEditor settings=settings .../>
```

<details>
<summary>Every color token: diff, footer counts, find, folding, locks, secrets, tabs, and theming your own elements</summary>

The same tokens are CSS variables (`--config-editor-key`, …), so a stylesheet
can set them too. The diff colors are `Added`, `Modified` and `Removed`
(`--config-editor-added`, `--config-editor-modified`,
`--config-editor-removed`, which follows `Error` unless set), the line
backgrounds `AddedLine` and `ModifiedLine`, the preview's `DiffRemoved*` and
`DiffCurrent*`, and the footer's change count `StatusChanges`
(`--config-editor-status-changes`, which follows `Modified` unless set),
and its problem counts `StatusError`, `StatusWarning`, `StatusInfo` and
`StatusHint`, each in its own color.
The find widget uses `SearchMatch`, `SearchMatchCurrent` and
`SearchMatchCurrentBorder` (`--config-editor-search-match`, …) for matches,
and `InputBackground`, `InputBorder` and `ToggleActive` for its fields and
option buttons. Folding uses `FoldToggle` for the gutter chevrons (following
the line numbers' color unless set) and `FoldChip` / `FoldChipText` for the
`…` after a folded line. Read-only values and the gutter lock use `Locked`
(`--config-editor-locked`, which follows `Muted` unless set), secret
placeholders `SecretChip` and `SecretChipBorder`.

The tabs of `ConfigDocuments` use `TabBar` (the strip, following `Panel`),
`TabBackground` / `TabActiveBackground` (following `Panel` / `Background`),
`TabText` / `TabActiveText` (following `Muted` / `Text`), `TabBorder`
(following `Border`), `TabActiveBorder` (the line over the active tab,
following `Accent`) and `TabDirty` (the unsaved-changes dot, following
`Text`): `--config-editor-tab-bar`, `--config-editor-tab-bg`,
`--config-editor-tab-active-bg`, `--config-editor-tab-text`,
`--config-editor-tab-active-text`, `--config-editor-tab-border`,
`--config-editor-tab-active-border`, `--config-editor-tab-dirty`. The view's
root carries the theme too (`data-color-theme`), so they follow the theme.

A page's own element can take a theme's variables for its controls without
being styled as an editor: give it the class `config-editor-theme` with
`data-theme` (the family, `EditorTheme::css_family`) and
`data-color-theme` (`EditorTheme::as_str`), and read `var(--config-editor-bg)`
and the rest inside it (the demo's page does). The `config-editor` class is
the editor's own: on a wrapper it would also style the wrapper as one.

</details>

## Strict Content-Security-Policy

The editor runs under a policy without `'unsafe-inline'`:

```text
Content-Security-Policy: default-src 'self'; style-src 'self'; script-src 'self' 'wasm-unsafe-eval'
```

- Set `styled: false` (`EditorSettings { styled: false, .. }`, and the same
  on `ConfigDocuments`): the injected `<style>` is inline, so the policy
  refuses it.
- Serve the preset as a file instead: the crate ships it as `preset.css`,
  one file with the base styles, every theme and the tabs of
  `ConfigDocuments` (byte for byte [`PRESET_CSS`]; copy it from the crate's
  folder, or write `PRESET_CSS` to a file in your build), and link it:
  `<link rel="stylesheet" href="/preset.css">`. Themes and `colors` then
  switch as they do with the injected stylesheet.
- `'wasm-unsafe-eval'` lets the page compile WebAssembly; nothing else in
  the editor evaluates code.

Everything else holds as it is: the editor sets positions, sizes and
`colors` through the CSSOM (`style.setProperty`), which the policy allows,
never as `style` attributes, and its icons are inline SVG, not `data:`
images. An end-to-end test runs every feature under this policy.

## Cargo features

All are on by default; turn off what you don't use to shrink the build.

| Feature             | Enables                                                                                                                                                                                                                                           |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `schema-validation` | Live JSON Schema diagnostics (`jsonschema`, the heaviest dependency). Completions, hover docs and schema-shaped reading work without it, from the schema alone; leave it out when documents are validated on a server. Syntax is always checked.  |
| `yaml`              | `Language::YAML` (`noyalib`)                                                                                                                                                                                                                      |
| `json`              | `Language::JSON`                                                                                                                                                                                                                                  |
| `toml`              | `Language::TOML` (`toml`)                                                                                                                                                                                                                         |
| `hcl`               | `Language::HCL` (`hcl-rs`)                                                                                                                                                                                                                        |
| `xml`               | `Language::XML` (`quick-xml`)                                                                                                                                                                                                                     |
| `search`            | [Find and replace](#find-and-replace) (`Ctrl+F`, `Ctrl+H`) and go to line (`Ctrl+G`).                                                                                                                                                             |
| `diff`              | [Changes against the baseline](#changes-against-the-baseline): the gutter markers and their preview, revert, `Alt+F5`, the change count (`on_change_summary`) and `unified_diff`. Without it the baseline still drives the dirty flag and saving. |
| `folding`           | [Folding](#folding): the chevrons, the folding keys and the handle's folding methods.                                                                                                                                                             |
| `protect`           | [Read-only fields](#read-only-fields) and [secrets](#secrets): the schema's `readOnly`, `writeOnly` and `x-secret`, `read_only_paths`, `read_only_ranges` and `secrets`. The whole-document `read_only` works without it.                         |

```toml
# YAML with live schema diagnostics:
config-editor-leptos = { version = "0.1", default-features = false, features = ["schema-validation", "yaml"] }
# YAML and TOML with schema completions and hover, validated on a server:
config-editor-leptos = { version = "0.1", default-features = false, features = ["yaml", "toml"] }
# The smallest editor: YAML, nothing optional:
config-editor-leptos = { version = "0.1", default-features = false, features = ["yaml"] }
```

With no language feature, use your own languages through `Language::new`.

### Without a feature

<details>
<summary>Which props, settings and hooks stay, and what exists only with its feature</summary>

What a feature adds to the API goes with it, with two exceptions that keep
a host's code the same in every build:

- The `<ConfigEditor>` props and the [`EditorSettings`](#settings) fields
  stay, and do nothing: `folding: true` folds nothing without `folding`,
  `on_change_summary` is never called without `diff`, and
  `read_only_paths`, `read_only_ranges` and `secrets` lock nothing without
  `protect` (the browser console says so once when one is given). Each
  one's documentation names its feature. So do the `Document` builder's
  methods that feed them.
- The [`ConfigLanguage`](#custom-languages) hooks and building blocks
  stay (`fold_ranges`, `FoldRange`, …), so a language of your own builds
  either way.

Everything else exists only with its feature, so code that needs one says
so at compile time: the handle's `fold`, `unfold`, `fold_all`, `unfold_all`
and `folded_lines` (`folding`), `unified_diff` and `ChangeSummary::between`
(`diff`), `Language::fold_ranges` (`folding`). The keys of a feature that is
out (`Ctrl+F`, `Ctrl+G`, `Alt+F5`, `Ctrl+Shift+[`) stay the browser's.

The npm packages (`@pngwasi/config-editor`, `@pngwasi/config-editor-react`)
are built with every feature.

</details>

### Size

The wasm a small host ships, by features: one `<ConfigEditor>` with a schema,
the toolbar and the footer, its language any built-in one of the build
(`examples/wasm_size.rs`), built for release, through wasm-bindgen, gzip -9.
What a host does not use (`ConfigDocuments`, the handle's methods, the
headless functions) is left out by the linker, feature or not.

<details>
<summary>The wasm size of each build, and how to measure it again</summary>

<!-- wasm-sizes:start -->

| Build                                                           | wasm, gzip -9 |
| --------------------------------------------------------------- | ------------- |
| Everything (the default features)                               | 1815 KB       |
| YAML + JSON, every optional feature, no `schema-validation`     | 721 KB        |
| YAML + JSON, every optional feature, with `schema-validation`   | 1417 KB       |
| Minimal: YAML only, no optional feature, no `schema-validation` | 579 KB        |
| Saved by leaving out `schema-validation` (from everything)      | -700 KB       |
| Saved by leaving out `search` (from everything)                 | -34 KB        |
| Saved by leaving out `folding` (from everything)                | -30 KB        |
| Saved by leaving out `diff` (from everything)                   | -25 KB        |
| Saved by leaving out `protect` (from everything)                | -53 KB        |

Measured 2026-11-07 with rustc 1.99.1, without wasm-opt (not installed), by `scripts/wasm-sizes.sh`.

<!-- wasm-sizes:end -->

To measure again (needs the `wasm32-unknown-unknown` target and the
wasm-bindgen CLI; uses `wasm-opt` when installed):
`scripts/wasm-sizes.sh` prints the table, and `scripts/wasm-sizes.sh --write`
puts it here. Sizes move with the toolchain and the dependencies.

</details>

## License

MIT. See [LICENSE](LICENSE).

[Leptos]: https://leptos.dev
[`Schema::warnings`]: https://docs.rs/config-editor-leptos/latest/config_editor_leptos/struct.Schema.html#method.warnings
[`PRESET_CSS`]: https://docs.rs/config-editor-leptos/latest/config_editor_leptos/constant.PRESET_CSS.html
[`@pngwasi/config-editor`]: https://www.npmjs.com/package/@pngwasi/config-editor
[`@pngwasi/config-editor-react`]: https://www.npmjs.com/package/@pngwasi/config-editor-react
