// Generated from crates/config-editor-leptos/src/settings.rs by js/config-editor/tests/settings.rs; do not edit. Regenerate with `UPDATE_GENERATED=1 cargo test -p config-editor-js --test settings`.
// Generated from crates/config-editor-leptos/src/strings.rs by js/config-editor/tests/settings.rs; do not edit. Regenerate with `UPDATE_GENERATED=1 cargo test -p config-editor-js --test settings`.

/**
 * How an editor looks and which of its features are on: everything a host sets once and may switch at any time, apart from the document itself.
 *
 * Every key is optional: one left out takes its default.
 */
export interface EditorSettings {
  /**
   * The bundled color theme.
   *
   * @default "vscode-dark"
   */
  theme?: ThemeName;
  /**
   * Colors overriding the theme's, by token (`bg`, `key`, `string`, `selection`, …), for a custom palette.
   *
   * @default {}
   */
  colors?: ThemeColorMap;
  /**
   * Inject the preset stylesheet. Off, serve it as a file (`preset.css` in the crate and the npm package, as a strict Content-Security-Policy needs) or style the `config-editor__*` markup yourself (the preset is `PRESET_CSS`, `presetCss()` in JavaScript).
   *
   * @default true
   */
  styled?: boolean;
  /**
   * Show the built-in toolbar: the file name, its language and the schema's state.
   *
   * @default false
   */
  showToolbar?: boolean;
  /**
   * Show the built-in status footer: the suggestions hint, problem and change counts, and the line count.
   *
   * @default false
   */
  showFooter?: boolean;
  /**
   * Show syntax and schema problems inline, in the footer and on hover (`F8` moves between them).
   *
   * @default true
   */
  showDiagnostics?: boolean;
  /**
   * Show line changes against the baseline in the gutter (`Alt+F5` / `Alt+F3` move between them). Off, the change summary stays at zero. In Rust, needs the `diff` Cargo feature.
   *
   * @default true
   */
  showDiff?: boolean;
  /**
   * Schema completions (`Ctrl+Space`).
   *
   * @default true
   */
  completions?: boolean;
  /**
   * Open suggestions while typing and after accepting a key. Off, only `Ctrl+Space` opens them.
   *
   * @default true
   */
  autoTriggerCompletions?: boolean;
  /**
   * Show the schema's title, description, type, default and examples when hovering a key, at the caret with `Ctrl+K Ctrl+I` (read to screen readers), or on a tap on a touch screen.
   *
   * @default true
   */
  hoverDocs?: boolean;
  /**
   * Syntax colors.
   *
   * @default true
   */
  highlighting?: boolean;
  /**
   * Auto-close quotes and brackets, wrap selections, and delete empty pairs on Backspace.
   *
   * @default true
   */
  autoClosing?: boolean;
  /**
   * Find and replace (`Ctrl+F`, `Ctrl+H`, `F3`). Off, those keys are left to the browser. In Rust, needs the `search` Cargo feature.
   *
   * @default true
   */
  search?: boolean;
  /**
   * Go to line (`Ctrl+G`). Off, the key is left to the browser. In Rust, needs the `search` Cargo feature.
   *
   * @default true
   */
  goToLine?: boolean;
  /**
   * Folding by structure: chevrons in the gutter, `Ctrl+Shift+[` and `Ctrl+Shift+]` at the caret, `Ctrl+K Ctrl+0` and `Ctrl+K Ctrl+J` for everything (`Cmd` on macOS). View only: the text stays whole. Switching it off opens every fold. In Rust, needs the `folding` Cargo feature.
   *
   * @default true
   */
  folding?: boolean;
  /**
   * Whether `Ctrl+S` / `Cmd+S` saves, when a save callback is set. Off, the key is left to the browser and only the handle's `save` saves.
   *
   * @default true
   */
  saveShortcut?: boolean;
  /**
   * Every built-in text (hints, tooltips, notices and their buttons, footer counts, accessible names and announcements), English by default: give only the texts to change. See `EditorStrings`.
   *
   * @default English texts
   */
  strings?: EditorStrings;
}

/**
 * Every built-in text of the editor and its documents' tabs, in English by default; change any to reword or translate the editor.
 *
 * Every key is optional: one left out takes its default.
 */
export interface EditorStrings {
  /**
   * The footer's suggestions hint; `{key}` is where the chord goes.
   *
   * @default "{key} for suggestions"
   */
  suggestionsHint?: string;
  /**
   * `suggestions_hint` on a touch screen, which has no `suggestions_key`; shown only while suggestions open as you type.
   *
   * @default "suggestions appear as you type"
   */
  suggestionsHintTouch?: string;
  /**
   * The chord that opens suggestions, as the footer draws it: keys between `+`.
   *
   * @default "Ctrl+Space"
   */
  suggestionsKey?: string;
  /**
   * The footer's problem count when there are none.
   *
   * @default "No errors"
   */
  noProblems?: string;
  /**
   * One error, in the footer's problem count.
   *
   * @default "{count} error"
   */
  errorsOne?: string;
  /**
   * Several errors, in the footer's problem count.
   *
   * @default "{count} errors"
   */
  errorsOther?: string;
  /**
   * One warning, in the footer's problem count.
   *
   * @default "{count} warning"
   */
  warningsOne?: string;
  /**
   * Several warnings, in the footer's problem count.
   *
   * @default "{count} warnings"
   */
  warningsOther?: string;
  /**
   * One info, in the footer's problem count.
   *
   * @default "{count} info"
   */
  infosOne?: string;
  /**
   * Several infos, in the footer's problem count.
   *
   * @default "{count} info"
   */
  infosOther?: string;
  /**
   * One hint, in the footer's problem count.
   *
   * @default "{count} hint"
   */
  hintsOne?: string;
  /**
   * Several hints, in the footer's problem count.
   *
   * @default "{count} hints"
   */
  hintsOther?: string;
  /**
   * After the problem count when every problem has its place.
   *
   * @default "hover or focus underlined text"
   */
  problemsHint?: string;
  /**
   * `problems_hint` on a touch screen, which has no hover.
   *
   * @default "tap underlined text"
   */
  problemsHintTouch?: string;
  /**
   * After the problem count when the editor's own problems were cut; `{count}` is how many of them are shown (the host's are all shown).
   *
   * @default "only the editor's first {count} are shown"
   */
  problemsCut?: string;
  /**
   * The problem that says the list was cut, after `{count}` problems.
   *
   * @default "Only the first {count} problems are shown; more follow."
   */
  problemsMore?: string;
  /**
   * After the problem count when the problems have more underlines than are drawn; `{count}` is how many are (the most serious first).
   *
   * @default "only {count} underlines are drawn, the most serious first"
   */
  underlinesCut?: string;
  /**
   * The footer's change count against the baseline, for one change.
   *
   * @default "{count} change"
   */
  changesOne?: string;
  /**
   * The footer's change count, for several changes.
   *
   * @default "{count} changes"
   */
  changesOther?: string;
  /**
   * The change count's tooltip when one line changed; `{lines}` lists the added, modified and removed counts.
   *
   * @default "{lines} line · Alt+F5 for the next"
   */
  changeDetailsOne?: string;
  /**
   * The change count's tooltip when several lines changed.
   *
   * @default "{lines} lines · Alt+F5 for the next"
   */
  changeDetailsOther?: string;
  /**
   * Added lines, in the change count's tooltip.
   *
   * @default "{count} added"
   */
  changedAdded?: string;
  /**
   * Modified lines, in the change count's tooltip.
   *
   * @default "{count} modified"
   */
  changedModified?: string;
  /**
   * Removed lines, in the change count's tooltip.
   *
   * @default "{count} removed"
   */
  changedRemoved?: string;
  /**
   * The footer's line count, for one line.
   *
   * @default "{count} line"
   */
  linesOne?: string;
  /**
   * The footer's line count, for several lines.
   *
   * @default "{count} lines"
   */
  linesOther?: string;
  /**
   * The footer's indicator while Tab moves the focus.
   *
   * @default "Tab moves focus"
   */
  tabFocus?: string;
  /**
   * Its tooltip; clicking it turns the mode off.
   *
   * @default "Tab moves focus (Ctrl+M to indent with Tab)"
   */
  tabFocusTitle?: string;
  /**
   * Announced when `Ctrl+M` makes Tab move the focus.
   *
   * @default "Tab moves focus: Tab and Shift+Tab now leave the editor. Ctrl+M to indent with Tab again."
   */
  tabFocusOn?: string;
  /**
   * Announced when `Ctrl+M` makes Tab indent again.
   *
   * @default "Tab indents: Tab and Shift+Tab indent again. Ctrl+M to move focus with Tab."
   */
  tabFocusOff?: string;
  /**
   * The toolbar's schema state.
   *
   * @default "Schema connected"
   */
  schemaConnected?: string;
  /**
   * The Edit-as notice when the host's file has comments, which the edited language does not show but saving keeps.
   *
   * @default "The {host} file's comments and formatting are not shown while editing as {edit}; saving keeps them."
   */
  editAsHostComments?: string;
  /**
   * The same when the host's file has processing instructions too (XML `<?…?>`), which the edited language does not show either.
   *
   * @default "The {host} file's comments, processing instructions and formatting are not shown while editing as {edit}; saving keeps them."
   */
  editAsHostInstructions?: string;
  /**
   * The Edit-as notice when comments are typed in the edited language, which saving drops, and the host's language has comments.
   *
   * @default "Comments typed in {edit} are not saved: the {host} file keeps only its own comments."
   */
  editAsTypedComments?: string;
  /**
   * The same when the host's language has no comments (JSON).
   *
   * @default "Comments typed in {edit} are not saved: {host} has no comments."
   */
  editAsTypedCommentsNone?: string;
  /**
   * The Edit-as notice's close button.
   *
   * @default "Dismiss"
   */
  editAsDismiss?: string;
  /**
   * The notice shown when the host's baseline changes under unsaved edits.
   *
   * @default "This file changed outside the editor."
   */
  baselineChanged?: string;
  /**
   * What the notice's choices do, after its message.
   *
   * @default "Reload it, losing your changes, or keep your text and compare it with the new version."
   */
  baselineChangedDetail?: string;
  /**
   * The notice's button that takes the new baseline as the text.
   *
   * @default "Reload"
   */
  reload?: string;
  /**
   * Announced after Reload took the changed baseline.
   *
   * @default "Reloaded"
   */
  reloaded?: string;
  /**
   * Its tooltip.
   *
   * @default "Replace your text with the new version ({cmd}+Z brings yours back)"
   */
  reloadTitle?: string;
  /**
   * The notice's button that keeps the text against the new baseline.
   *
   * @default "Keep mine"
   */
  keepMine?: string;
  /**
   * Its tooltip.
   *
   * @default "Keep your text; the changes are now shown against the new version (Escape)"
   */
  keepMineTitle?: string;
  /**
   * How to reach the notice's choices from the keyboard, shown on it (and announced with it); not on touch screens.
   *
   * @default "In the editor, F6 moves to the choices and back."
   */
  baselineChangedKeys?: string;
  /**
   * The find widget's accessible name.
   *
   * @default "Find and replace"
   */
  findWidget?: string;
  /**
   * The button showing the replace row.
   *
   * @default "Toggle replace"
   */
  toggleReplace?: string;
  /**
   * Its tooltip.
   *
   * @default "Toggle replace ({cmd}+H)"
   */
  toggleReplaceTitle?: string;
  /**
   * The query field's accessible name and placeholder.
   *
   * @default "Find"
   */
  find?: string;
  /**
   * The match-case toggle.
   *
   * @default "Match case"
   */
  matchCase?: string;
  /**
   * Its tooltip.
   *
   * @default "Match case (Alt+C)"
   */
  matchCaseTitle?: string;
  /**
   * The whole-word toggle.
   *
   * @default "Match whole word"
   */
  wholeWord?: string;
  /**
   * Its tooltip.
   *
   * @default "Match whole word (Alt+W)"
   */
  wholeWordTitle?: string;
  /**
   * The regular-expression toggle.
   *
   * @default "Use regular expression"
   */
  useRegex?: string;
  /**
   * Its tooltip.
   *
   * @default "Use regular expression (Alt+R)"
   */
  useRegexTitle?: string;
  /**
   * The previous-match button.
   *
   * @default "Previous match"
   */
  previousMatch?: string;
  /**
   * Its tooltip.
   *
   * @default "Previous match (Shift+Enter)"
   */
  previousMatchTitle?: string;
  /**
   * The next-match button.
   *
   * @default "Next match"
   */
  nextMatch?: string;
  /**
   * Its tooltip.
   *
   * @default "Next match (Enter)"
   */
  nextMatchTitle?: string;
  /**
   * The find widget's close button.
   *
   * @default "Close"
   */
  close?: string;
  /**
   * Its tooltip.
   *
   * @default "Close (Escape)"
   */
  closeTitle?: string;
  /**
   * The replacement field's accessible name and placeholder, and the button replacing the current match.
   *
   * @default "Replace"
   */
  replace?: string;
  /**
   * The replace button's tooltip.
   *
   * @default "Replace (Enter)"
   */
  replaceTitle?: string;
  /**
   * The replace-all button.
   *
   * @default "All"
   */
  replaceAll?: string;
  /**
   * Its accessible name.
   *
   * @default "Replace all"
   */
  replaceAllLabel?: string;
  /**
   * Its tooltip.
   *
   * @default "Replace all ({cmd}+Alt+Enter)"
   */
  replaceAllTitle?: string;
  /**
   * The match count: match `{current}` of `{total}`.
   *
   * @default "{current} of {total}"
   */
  matchPosition?: string;
  /**
   * The match count when nothing matches.
   *
   * @default "No results"
   */
  noResults?: string;
  /**
   * The match count when the regular expression is invalid.
   *
   * @default "Invalid pattern"
   */
  invalidPattern?: string;
  /**
   * After Replace All, for one match.
   *
   * @default "Replaced {count} occurrence"
   */
  replacedOne?: string;
  /**
   * After Replace All, for several.
   *
   * @default "Replaced {count} occurrences"
   */
  replacedOther?: string;
  /**
   * Added when Replace All left matches in protected text alone.
   *
   * @default "; skipped {count} in read-only text or secret placeholders"
   */
  replaceSkipped?: string;
  /**
   * After Replace All when every match is in read-only text or a secret placeholder.
   *
   * @default "Every match is in read-only text or secret placeholders; nothing was replaced"
   */
  nothingReplaced?: string;
  /**
   * The go-to-line field's accessible name.
   *
   * @default "Go to line"
   */
  goToLine?: string;
  /**
   * Its placeholder.
   *
   * @default "line or line:column"
   */
  goToLinePlaceholder?: string;
  /**
   * Its hint: the range of lines.
   *
   * @default "Line 1–{lines}, or line:column"
   */
  goToLineHint?: string;
  /**
   * Its hint when the text is no position.
   *
   * @default "Type a line number between 1 and {lines}"
   */
  goToLineInvalid?: string;
  /**
   * The change preview's title for an added line.
   *
   * @default "Added line"
   */
  addedLine?: string;
  /**
   * For a modified line.
   *
   * @default "Modified line"
   */
  modifiedLine?: string;
  /**
   * For removed lines.
   *
   * @default "Removed lines"
   */
  removedLines?: string;
  /**
   * For lines removed after the last one.
   *
   * @default "Removed lines at end of document"
   */
  removedLinesAtEnd?: string;
  /**
   * The gutter marker of an added line.
   *
   * @default "Show added line {line}"
   */
  showAdded?: string;
  /**
   * Its tooltip.
   *
   * @default "Show added line"
   */
  showAddedTitle?: string;
  /**
   * The gutter marker of a modified line.
   *
   * @default "Show before and after for modified line {line}"
   */
  showModified?: string;
  /**
   * Its tooltip.
   *
   * @default "Show before and after"
   */
  showModifiedTitle?: string;
  /**
   * The gutter marker of removed lines.
   *
   * @default "Show removed lines before line {line}"
   */
  showRemoved?: string;
  /**
   * Its tooltip, and that of lines removed at the end.
   *
   * @default "Show removed lines"
   */
  showRemovedTitle?: string;
  /**
   * The gutter marker of lines removed after the last one.
   *
   * @default "Show removed lines at end of document"
   */
  showRemovedAtEnd?: string;
  /**
   * The change preview's heading: its `{title}` and `{line}`.
   *
   * @default "{title} · line {line}"
   */
  previewHeading?: string;
  /**
   * The change preview's accessible name.
   *
   * @default "{title}, line {line}"
   */
  previewLabel?: string;
  /**
   * Announced on `Alt+F5` / `Shift+Alt+F5`: the change's `{title}` (as the preview's), its `{line}`, and its `{position}` of `{total}`.
   *
   * @default "{title}, line {line}: change {position} of {total}"
   */
  changePosition?: string;
  /**
   * Announced on `Alt+F5` / `Alt+F3` when nothing differs from the baseline.
   *
   * @default "No changes"
   */
  noChanges?: string;
  /**
   * Announced when a change is reverted.
   *
   * @default "Reverted"
   */
  reverted?: string;
  /**
   * The preview's revert button.
   *
   * @default "Revert"
   */
  revert?: string;
  /**
   * Its tooltip, for a one-line change.
   *
   * @default "Revert this change (R)"
   */
  revertTitle?: string;
  /**
   * Its tooltip, for a change over lines `{first}` to `{last}`.
   *
   * @default "Revert this change (lines {first}–{last}) (R)"
   */
  revertLinesTitle?: string;
  /**
   * Its tooltip, for a changed line with removed lines.
   *
   * @default "Revert this change (line {line} and the removed lines) (R)"
   */
  revertLineAndRemovedTitle?: string;
  /**
   * The preview's close button.
   *
   * @default "Close change preview"
   */
  closePreview?: string;
  /**
   * The preview's column of the baseline's lines.
   *
   * @default "Before"
   */
  before?: string;
  /**
   * The preview's column of the current lines.
   *
   * @default "Current"
   */
  current?: string;
  /**
   * The chevron of an open range.
   *
   * @default "Fold line {line}"
   */
  fold?: string;
  /**
   * The chevron or chip of a closed fold.
   *
   * @default "Unfold lines {first}–{last}"
   */
  unfold?: string;
  /**
   * A closed fold's line number when it hides changes or problems.
   *
   * @default "Folded lines have changes"
   */
  foldedChanges?: string;
  /**
   * After a closed fold's chip label when it hides one problem; `{severity}` names it.
   *
   * @default "hides a problem: {severity}"
   */
  foldedProblemOne?: string;
  /**
   * After a closed fold's chip label when it hides `{count}` problems; `{severity}` names the most serious of them.
   *
   * @default "hides {count} problems, the most serious: {severity}"
   */
  foldedProblemOther?: string;
  /**
   * Announced when the range of lines `{first}` to `{last}` folds.
   *
   * @default "Folded lines {first}–{last}"
   */
  foldedLines?: string;
  /**
   * Announced when the fold of lines `{first}` to `{last}` opens.
   *
   * @default "Unfolded lines {first}–{last}"
   */
  unfoldedLines?: string;
  /**
   * Announced when `{count}` ranges (two or more) fold at once.
   *
   * @default "Folded {count} ranges"
   */
  foldedRanges?: string;
  /**
   * Announced when `{count}` folds (two or more) open at once.
   *
   * @default "Unfolded {count} ranges"
   */
  unfoldedRanges?: string;
  /**
   * Announced when a fold key (`Ctrl+Shift+[`, `Ctrl+K Ctrl+0`) finds nothing to fold.
   *
   * @default "Nothing to fold"
   */
  nothingToFold?: string;
  /**
   * Announced when an unfold key (`Ctrl+Shift+]`, `Ctrl+K Ctrl+J`) finds nothing to open.
   *
   * @default "Nothing to unfold"
   */
  nothingToUnfold?: string;
  /**
   * The hover card's mark of a required key.
   *
   * @default "required"
   */
  required?: string;
  /**
   * The hover card's mark of a deprecated key.
   *
   * @default "deprecated"
   */
  deprecated?: string;
  /**
   * The hover card's default row.
   *
   * @default "Default"
   */
  defaultValue?: string;
  /**
   * The hover card's examples row.
   *
   * @default "Examples"
   */
  examples?: string;
  /**
   * The hover card's row listing the titles of the options (`oneOf`, `anyOf`) a field's object may take, while none matches it yet.
   *
   * @default "One of"
   */
  oneOf?: string;
  /**
   * The hover card's link to the schema's documentation.
   *
   * @default "Documentation"
   */
  documentation?: string;
  /**
   * Announced when a second `Ctrl+K Ctrl+I` moves the focus into the hover card, to scroll it.
   *
   * @default "In the docs: arrows, Page Up, Page Down, Home and End scroll them. Escape returns to the text."
   */
  hoverFocused?: string;
  /**
   * The focused hover card's accessible name: the `{key}` it documents.
   *
   * @default "About {key}"
   */
  hoverLabel?: string;
  /**
   * The problem tip's accessible name while it has the focus (a second `Ctrl+K Ctrl+I`).
   *
   * @default "Problems here"
   */
  problemTipLabel?: string;
  /**
   * The link under a host problem's message.
   *
   * @default "Open docs ↗ (Alt+Enter)"
   */
  openDocs?: string;
  /**
   * `open_docs` on a touch screen, which has no keys to name.
   *
   * @default "Open docs ↗"
   */
  openDocsTouch?: string;
  /**
   * A problem's docs link as a screen reader names it, told apart from the others in a tip: its `{link}` text (`open_docs`) and its problem's `{message}`.
   *
   * @default "{link}: {message}"
   */
  openDocsLabel?: string;
  /**
   * An error, as a screen reader hears it.
   *
   * @default "Error"
   */
  error?: string;
  /**
   * A warning, as a screen reader hears it.
   *
   * @default "Warning"
   */
  warning?: string;
  /**
   * An info, as a screen reader hears it.
   *
   * @default "Info"
   */
  info?: string;
  /**
   * A hint, as a screen reader hears it.
   *
   * @default "Hint"
   */
  hint?: string;
  /**
   * Who reported a host problem: the `{source}` of the problem texts below (empty for the editor's own problems).
   *
   * @default " from {source}"
   */
  problemSource?: string;
  /**
   * A problem at the caret, for screen readers.
   *
   * @default "{severity}{source}: {message}"
   */
  problem?: string;
  /**
   * Announced on `F8`: problem `{position}` of `{total}`.
   *
   * @default "{severity} {position} of {total}{source}: {message}"
   */
  problemPosition?: string;
  /**
   * Announced on `F8` when there are no problems.
   *
   * @default "No problems"
   */
  walkNoProblems?: string;
  /**
   * After a problem's text for screen readers: its `{code}` (from the host).
   *
   * @default " Code {code}."
   */
  problemCode?: string;
  /**
   * After a problem's text (or the hover card's docs) for screen readers: its documentation's `{url}`, and the key that opens it.
   *
   * @default " Documentation: {url} (Alt+Enter opens it)."
   */
  problemDocs?: string;
  /**
   * An underlined problem's accessible name.
   *
   * @default "{language} {severity}{source} on line {line}: {message}"
   */
  problemMarker?: string;
  /**
   * What protected text at the caret is, for screen readers: its `{label}` and `{message}`.
   *
   * @default "{label}: {message}"
   */
  protectedNote?: string;
  /**
   * The text area.
   *
   * @default "{language} source"
   */
  sourceLabel?: string;
  /**
   * The suggestions list.
   *
   * @default "{language} suggestions"
   */
  suggestionsLabel?: string;
  /**
   * The layer of underlined problems.
   *
   * @default "Inline {language} diagnostics"
   */
  problemsLabel?: string;
  /**
   * The editor's frame.
   *
   * @default "{language} editor"
   */
  editorLabel?: string;
  /**
   * Said, and shown at the caret, when typing in a read-only editor.
   *
   * @default "Cannot edit in a read-only editor"
   */
  readOnlyEditor?: string;
  /**
   * The same, for an editor the host said why it is read-only (`read_only_reason`): `{reason}`.
   *
   * @default "Cannot edit in a read-only editor: {reason}"
   */
  readOnlyEditorReason?: string;
  /**
   * Why a field is read-only when neither the schema nor the host says.
   *
   * @default "This field is read-only"
   */
  readOnlyField?: string;
  /**
   * Why text in a read-only range the host gave cannot change.
   *
   * @default "This text is read-only"
   */
  readOnlyText?: string;
  /**
   * Why renaming or moving the key above a read-only field is refused.
   *
   * @default "Holds a read-only field: {reason}"
   */
  holdsReadOnly?: string;
  /**
   * A save refused because it would change a read-only value.
   *
   * @default "{pointer} is read-only and this change would alter it"
   */
  saveRefused?: string;
  /**
   * A save waiting for the text to be valid: the language cannot say where the read-only field at `{pointer}` is written, so its value is checked only in a text that parses.
   *
   * @default "Not saved: {pointer} is read-only, and it can be checked only once the text is valid"
   */
  saveWaits?: string;
  /**
   * An edit refused because it makes a text that did not parse parse with the read-only field at `{pointer}` reading otherwise than in the host's text, and why the field is read-only (`{reason}`).
   *
   * @default "This edit makes the text valid, but {pointer} would no longer read as the host gave it ({reason}): undo the change to it first"
   */
  readOnlyChanged?: string;
  /**
   * An edit refused because it makes a text that did not parse parse after more lines that may hold the read-only field at `{pointer}` were deleted from the host's text than can be checked through, and why the field is read-only (`{reason}`).
   *
   * @default "Too many lines were deleted while the text was invalid to check {pointer} ({reason}): undo deleting the lines that held it first"
   */
  readOnlyUnchecked?: string;
  /**
   * The gutter lock's label, before the reason.
   *
   * @default "Read-only"
   */
  readOnlyLabel?: string;
  /**
   * The gutter key's label, before the hint.
   *
   * @default "Secret placeholder"
   */
  secretLabel?: string;
  /**
   * The hover text of a secret placeholder, unless the host gives one.
   *
   * @default "Keeps the current secret unless you replace it"
   */
  secretHint?: string;
  /**
   * What a schema problem says instead of a secret field's value.
   *
   * @default "the secret value"
   */
  secretValue?: string;
  /**
   * A placeholder the host did not give, in a secret field.
   *
   * @default "{pointer}: unknown secret placeholder; it stands for no current secret, so write the new value instead"
   */
  unknownPlaceholder?: string;
  /**
   * One of the host's placeholders outside the field it stands in.
   *
   * @default "{pointer}: this placeholder stands for the secret of {owner}, not this field's; here it is plain text"
   */
  movedPlaceholder?: string;
  /**
   * One of the host's field placeholders written in a comment.
   *
   * @default "This placeholder stands for the secret of {owner}; in a comment it is plain text"
   */
  placeholderInComment?: string;
  /**
   * A placeholder the host wrote in a comment, in a field.
   *
   * @default "{pointer}: this placeholder stands for a secret in a comment, not this field's; here it is plain text"
   */
  commentPlaceholder?: string;
  /**
   * Text written beside a placeholder the host gave as a field's whole value: the edit refused, the field flagged, a save waiting.
   *
   * @default "{pointer}: a secret placeholder stands for the whole secret, so nothing can be written beside it: replace it whole to write a new value"
   */
  gluedPlaceholder?: string;
  /**
   * The detail of the `${…}` reference suggestion.
   *
   * @default "Read the secret from an environment variable"
   */
  envReference?: string;
  /**
   * The detail of the `file:` reference suggestion.
   *
   * @default "Read the secret from a file"
   */
  fileReference?: string;
  /**
   * The detail of another reference suggestion.
   *
   * @default "A reference starting with {prefix}"
   */
  otherReference?: string;
  /**
   * The type column of a reference suggestion.
   *
   * @default "reference"
   */
  referenceType?: string;
  /**
   * The detail of the end tag XML suggests.
   *
   * @default "Closes <{name}>"
   */
  closesTag?: string;
  /**
   * Its type column.
   *
   * @default "end tag"
   */
  endTag?: string;
  /**
   * Said when Ctrl+Space finds nothing to offer, and when typing empties a list opened with Ctrl+Space or moved in; a list that opened by itself closes silently.
   *
   * @default "No suggestions"
   */
  noSuggestions?: string;
  /**
   * A key the schema does not allow (`{key}`); its object's pointer goes before it (`/web: `), as before every schema problem but at the root.
   *
   * @default "\"{key}\" is not allowed here"
   */
  keyNotAllowed?: string;
  /**
   * Text in an element the schema wants elements in (XML); the element's pointer goes before it.
   *
   * @default "text is not allowed here, only elements"
   */
  textNotAllowed?: string;
  /**
   * A deprecated key.
   *
   * @default "{pointer}: \"{key}\" is deprecated"
   */
  keyDeprecated?: string;
  /**
   * A deprecated key whose schema says what to use instead.
   *
   * @default "{pointer}: \"{key}\" is deprecated. {message}"
   */
  keyDeprecatedInstead?: string;
  /**
   * A value not among the allowed ones.
   *
   * @default "{value} is not one of {allowed}"
   */
  notOneOf?: string;
  /**
   * A value of none of the accepted types.
   *
   * @default "{value} is not of type {types}"
   */
  wrongType?: string;
  /**
   * A value out of its `format` (checked up to draft-07): `{format}` is its name, quoted.
   *
   * @default "{value} is not a valid {format}"
   */
  notFormat?: string;
  /**
   * A list (`{value}`) with no item its `contains` schema allows.
   *
   * @default "{value} has no item its \"contains\" schema allows"
   */
  noItemContained?: string;
  /**
   * A list (`{value}`) with fewer items its `contains` schema allows than its `minContains` (`{limit}`).
   *
   * @default "{value} has fewer than {limit} items its \"contains\" schema allows"
   */
  tooFewContained?: string;
  /**
   * A list (`{value}`) with more items its `contains` schema allows than its `maxContains` (`{limit}`).
   *
   * @default "{value} has more than {limit} items its \"contains\" schema allows"
   */
  tooManyContained?: string;
  /**
   * A value its `not` schema (`{schema}`, as JSON) matches.
   *
   * @default "{value} must not match {schema}"
   */
  mustNotMatch?: string;
  /**
   * An object holding all of the keys its `not` schema keeps apart (`not: {required: [...]}`): the first (`{key}`) and the others (`{others}`), each quoted, joined by `, `.
   *
   * @default "{key} can't be used together with {others}"
   */
  keysUsedTogether?: string;
  /**
   * A value that matches none of a schema's options (`{keyword}`: `oneOf` or `anyOf`), with why each fails (`{reasons}`, each worded by `option_needs`, `option_refuses` or `option_problem`, joined by `; `).
   *
   * @default "matches none of the {keyword} options: {reasons}"
   */
  noOptionMatches?: string;
  /**
   * A value that matches more than one of a `oneOf`'s options (`{options}`, their names joined by `, `).
   *
   * @default "matches more than one of the oneOf options ({options}); only one may match"
   */
  severalOptionsMatch?: string;
  /**
   * An option (`{option}`) that needs a key the value does not have.
   *
   * @default "{option} needs \"{key}\""
   */
  optionNeeds?: string;
  /**
   * An option (`{option}`) that does not allow a key the value has.
   *
   * @default "{option} does not allow \"{key}\""
   */
  optionRefuses?: string;
  /**
   * An option (`{option}`) and another problem it has with the value.
   *
   * @default "{option}: {problem}"
   */
  optionProblem?: string;
  /**
   * The name of an option with no title or description (`{number}`, from 1).
   *
   * @default "option {number}"
   */
  optionNumber?: string;
  /**
   * A schema that does not compile.
   *
   * @default "Invalid JSON Schema: {error}"
   */
  invalidSchema?: string;
  /**
   * A schema whose text is empty, or only spaces and line breaks.
   *
   * @default "Invalid JSON Schema: the schema text is empty"
   */
  emptySchema?: string;
  /**
   * A secret placeholder pattern the editor cannot use (it does not compile, or matches empty text): no token is taken for a placeholder.
   *
   * @default "secrets.placeholder: {error}"
   */
  invalidPlaceholder?: string;
  /**
   * A host's problem with a field the editor cannot find in the text, listed with the field's path.
   *
   * @default "{path}: {message}"
   */
  problemAtPath?: string;
  /**
   * A value the host's language reads back differently (Edit-as).
   *
   * @default "{pointer}: {host} cannot keep this value exactly; it reads back differently"
   */
  hostCannotKeep?: string;
  /**
   * A value the edited language cannot show exactly (Edit-as).
   *
   * @default "{pointer}: {edit} cannot show this value exactly; editing it here may change it"
   */
  editCannotShow?: string;
  /**
   * An edit the host's language cannot save (Edit-as).
   *
   * @default "Not saved as {host}: {error}"
   */
  notSaved?: string;
  /**
   * A null the host's language cannot write (Edit-as).
   *
   * @default "{host} has no null value (at {pointer})"
   */
  noNull?: string;
  /**
   * An infinity or NaN the host's language has no number for (Edit-as).
   *
   * @default "{host} has no infinity or NaN (at {pointer})"
   */
  noNonFinite?: string;
  /**
   * An Edit-as switch waiting until the document converts.
   *
   * @default "Cannot switch to {edit} yet: {error}"
   */
  cannotSwitch?: string;
  /**
   * An Edit-as switch refused: the host's language or the other one cannot write documents.
   *
   * @default "Cannot edit as {edit}: {error}"
   */
  switchRefused?: string;
  /**
   * XML text beside child elements, which no other language can show (Edit-as).
   *
   * @default "{pointer}: text beside child elements cannot be shown in another language"
   */
  mixedContent?: string;
  /**
   * The tab list's accessible name, unless the host gives one.
   *
   * @default "Open files"
   */
  tabList?: string;
  /**
   * Shown in place of an editor while no document is open.
   *
   * @default "No open files"
   */
  noDocuments?: string;
  /**
   * A tab's close button.
   *
   * @default "Close {name}"
   */
  closeTab?: string;
  /**
   * Its tooltip.
   *
   * @default "Close (Delete)"
   */
  closeTabTitle?: string;
  /**
   * The "+" button after the tabs (with `on_add`): its accessible name and tooltip.
   *
   * @default "New file"
   */
  addDocument?: string;
  /**
   * The dirty dot's tooltip, and what a tab's accessible name adds for it.
   *
   * @default "unsaved changes"
   */
  unsaved?: string;
  /**
   * What a read-only document's tab adds to its accessible name.
   *
   * @default "read-only"
   */
  readOnlyTab?: string;
  /**
   * The tooltip of a read-only document's tab with a reason (`Document::read_only_reason`): its file `{name}` and the `{reason}`.
   *
   * @default "{name}\nRead-only: {reason}"
   */
  readOnlyTabTitle?: string;
}

/** A token `colors` accepts: one of the theme's colors (from `ThemeColor` in the crate). */
export type ThemeColorToken =
  | "bg"
  | "panel"
  | "border"
  | "text"
  | "muted"
  | "accent"
  | "error"
  | "warning"
  | "info"
  | "hint"
  | "toolbar-text"
  | "line-number"
  | "line-active"
  | "added"
  | "modified"
  | "removed"
  | "active-line"
  | "error-line"
  | "added-line"
  | "modified-line"
  | "selection"
  | "caret"
  | "comment"
  | "key"
  | "punctuation"
  | "string"
  | "literal"
  | "value"
  | "reference"
  | "interpolation"
  | "raised"
  | "raised-border"
  | "raised-text"
  | "raised-muted"
  | "raised-hover"
  | "completion-active"
  | "completion-key"
  | "completion-type"
  | "completion-type-active"
  | "completion-detail"
  | "completion-icon-border"
  | "completion-icon-text"
  | "completion-match-active"
  | "tooltip-text"
  | "control-bg"
  | "control-text"
  | "control-disabled-text"
  | "focus-ring"
  | "file-icon-bg"
  | "file-icon-text"
  | "status-error"
  | "status-warning"
  | "status-info"
  | "status-hint"
  | "status-changes"
  | "diff-focus-ring"
  | "diff-peek-divider"
  | "diff-removed-bg"
  | "diff-removed-text"
  | "diff-current-bg"
  | "diff-current-text"
  | "scrollbar-thumb"
  | "popup-shadow"
  | "peek-shadow"
  | "hover-shadow"
  | "search-match"
  | "search-match-current"
  | "search-match-current-border"
  | "input-bg"
  | "input-border"
  | "toggle-active"
  | "locked"
  | "secret-chip"
  | "secret-chip-border"
  | "fold-toggle"
  | "fold-chip"
  | "fold-chip-text"
  | "tab-bar"
  | "tab-bg"
  | "tab-active-bg"
  | "tab-text"
  | "tab-active-text"
  | "tab-border"
  | "tab-active-border"
  | "tab-dirty";

/** Color overrides: a CSS color per token, e.g. `{ bg: "#101418", key: "#7dd3fc" }`. */
export type ThemeColorMap = Partial<Record<ThemeColorToken, string>>;
