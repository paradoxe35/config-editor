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
   * The chrome's fonts, spacing, borders and radius, by token (`tab-padding`, `tab-font-size`, `toolbar-padding`, `footer-font-size`, …), over the preset's: the documents view's tabs, the toolbar and the footer. Their colors are `colors` tokens (`tab-bg`, `toolbar-bg`, `footer-text`, …). See "Theming the chrome" in the README.
   *
   * @default {}
   */
  chrome?: ThemeChromeMap;
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
   * Show syntax and schema problems inline, in the footer and on hover (`F8` moves between them). Off, why an Edit-as switch or a save waits still shows, as the answer to what the user asked for.
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
   * How long, in milliseconds, the pointer rests on a key, a problem's underline or protected text before its hover card, problem tip or lock tip shows; a move of more than a few pixels starts the wait again. While one shows, or just after one closed, another shows after 100 ms (or this, if shorter). `Ctrl+K Ctrl+I`, `F8` and taps show them at once.
   *
   * @default 300
   */
  hoverDelay?: number;
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
   * Sticky scroll, as in VS Code: the lines opening the blocks around the top line stay at the top while the text scrolls; a click goes back to one. Works with `folding` off too. In Rust, needs the `folding` Cargo feature.
   *
   * @default true
   */
  stickyScroll?: boolean;
  /**
   * The most lines sticky scroll shows, 1 to 20 (others are taken as the nearest), and never more than fit in a quarter of the text area (at least one).
   *
   * @default 5
   */
  stickyScrollMaxLines?: number;
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
   * The problem that says the schema check stopped early, so more problems may follow than are listed (a schema whose explanation would grow faster than the document).
   *
   * @default "The schema check stopped early; more problems may follow."
   */
  problemsIncomplete?: string;
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
   * For one removed line.
   *
   * @default "Removed line"
   */
  removedLine?: string;
  /**
   * For several removed lines.
   *
   * @default "Removed lines"
   */
  removedLines?: string;
  /**
   * For one line removed after the last one.
   *
   * @default "Removed line at end of document"
   */
  removedLineAtEnd?: string;
  /**
   * For several lines removed after the last one.
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
   * Added to `hover_focused` when the focused card or problem tip holds links (Definition, Uses, its docs): how to reach them.
   *
   * @default "Tab reaches its links."
   */
  hoverLinks?: string;
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
   * Why renaming or moving the key above a read-only field is refused, when nothing says why the field is read-only.
   *
   * @default "Holds a read-only field"
   */
  holdsReadOnly?: string;
  /**
   * The same, for a field the schema says why is read-only (`x-readonly-reason`): `{reason}`.
   *
   * @default "Holds a read-only field: {reason}"
   */
  holdsReadOnlyReason?: string;
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
   * A save waiting because the read-only field at `{pointer}` no longer reads as the host gave it, held where the editor cannot tell why; `{reason}` is the `save_held_reason` text when the schema says why the field is read-only (`x-readonly-reason`), else empty. The texts after it name each cause a hold records, with the same placeholders.
   *
   * @default "Not saved: the read-only field {pointer} no longer reads as the host gave it{reason}; put it back as it was to save"
   */
  saveHeld?: string;
  /**
   * The same, because its key or a block holding it is written twice, and the other copy is read in its place.
   *
   * @default "Not saved: the read-only field {pointer} is written twice, and the other copy is read in its place{reason}; remove the copy to save"
   */
  saveHeldDuplicate?: string;
  /**
   * The same, because a key on its path was renamed (by an edit or an undo).
   *
   * @default "Not saved: a key holding the read-only field {pointer} was renamed{reason}; give the key its old name back to save"
   */
  saveHeldRenamed?: string;
  /**
   * The same, because its text now sits under another parent.
   *
   * @default "Not saved: the read-only field {pointer} moved under another parent{reason}; move it back to save"
   */
  saveHeldMoved?: string;
  /**
   * The same, for a list item whose place in its list now holds another item (items were added, removed or moved before it).
   *
   * @default "Not saved: the read-only field {pointer} is now another item of its list, because items were added, removed or moved before it{reason}; put the items back in order to save"
   */
  saveHeldListItem?: string;
  /**
   * The same, because its line is commented out.
   *
   * @default "Not saved: the read-only field {pointer} is commented out{reason}; remove the comment mark to save"
   */
  saveHeldCommented?: string;
  /**
   * The same, because the text no longer has it.
   *
   * @default "Not saved: the read-only field {pointer} is no longer in the text{reason}; write it back as the host gave it to save"
   */
  saveHeldRemoved?: string;
  /**
   * The same, because text was added into it or beside it (a key in its block, a line taken into its value).
   *
   * @default "Not saved: the read-only field {pointer} now reads text that was added into it or beside it{reason}; remove that text to save"
   */
  saveHeldAdded?: string;
  /**
   * The same, in a language that cannot say where the field is written: its value reads otherwise.
   *
   * @default "Not saved: the read-only field {pointer} now reads another value than the host gave{reason}; put its value back as it was to save"
   */
  saveHeldSpanless?: string;
  /**
   * The same, for a field in a read-only range the host gave: the range's text no longer reads as that field.
   *
   * @default "Not saved: the read-only text the host gave no longer reads as {pointer}{reason}; put that text back where it was to save"
   */
  saveHeldRange?: string;
  /**
   * The same, for a field the host made read-only after it was changed: the text reads it otherwise than the host's text does.
   *
   * @default "Not saved: {pointer} was changed before the host made it read-only{reason}; put it back as the host gave it to save"
   */
  saveHeldBeforeLock?: string;
  /**
   * What a `save_held…` text says of why the field is read-only, when the schema says (`x-readonly-reason`): its `{reason}`.
   *
   * @default " ({reason})"
   */
  saveHeldReason?: string;
  /**
   * A save waiting because the secret field at `{pointer}` now reads text beside a placeholder the host gave as its whole value (an edit beside it, or an undo, made the parse read them together).
   *
   * @default "Not saved: {pointer} now reads other text beside its secret placeholder; remove that text so the placeholder stands alone to save"
   */
  saveHeldPlaceholder?: string;
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
   * What a schema problem writes in place of each secret inside a value it quotes (a list or map holding one).
   *
   * @default "…"
   */
  secretInside?: string;
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
   * One of another document's placeholders (in a documents view), `{source}` its file name.
   *
   * @default "This placeholder stands for a secret of {source}, not of this document; here it is plain text"
   */
  foreignPlaceholder?: string;
  /**
   * One of the host's placeholders written in its own field more often than the host gave it there: a copy of it.
   *
   * @default "{pointer}: this placeholder is in this field already; another copy of it is plain text"
   */
  extraPlaceholder?: string;
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
   * The source of a suggested name (`x-config-editor-keys`) that is a key of the document itself.
   *
   * @default "this document"
   */
  keyNameThisDocument?: string;
  /**
   * The detail of a suggested name with both a source and a detail of its own.
   *
   * @default "{source} · {detail}"
   */
  keyNameDetail?: string;
  /**
   * The hover card's line on a value that is a known name: where it is defined.
   *
   * @default "defined in {source}"
   */
  keyNameDefinedIn?: string;
  /**
   * Said and shown at the caret for a moment when Ctrl+Space finds nothing to offer; said when typing empties a list opened with Ctrl+Space or moved in (a list that opened by itself closes silently).
   *
   * @default "No suggestions"
   */
  noSuggestions?: string;
  /**
   * Said and shown at the caret for a moment when F12 finds no definition of the word at the caret (where the schema names keys).
   *
   * @default "No definition for '{name}'"
   */
  noDefinitionFor?: string;
  /**
   * `no_definition_for` when there is no word at the caret.
   *
   * @default "No definition here"
   */
  noDefinitionHere?: string;
  /**
   * Said and shown at the caret for a moment when Shift+F12 finds nothing to list the uses of (where the schema names keys).
   *
   * @default "No uses of '{name}'"
   */
  noUsesOf?: string;
  /**
   * `no_uses_of` when there is no word at the caret.
   *
   * @default "No uses here"
   */
  noUsesHere?: string;
  /**
   * The hover card's link on a known name to its definition (shown on a use, not on the definition itself).
   *
   * @default "Definition"
   */
  definitionLink?: string;
  /**
   * The accessible name and tooltip of the hover card's Definition link.
   *
   * @default "Go to definition (F12)"
   */
  definitionLinkLabel?: string;
  /**
   * The hover card's link on a known name (a use or its definition) to the list of its uses.
   *
   * @default "Uses"
   */
  usesLink?: string;
  /**
   * The accessible name and tooltip of the hover card's Uses link.
   *
   * @default "Find uses (Shift+F12)"
   */
  usesLinkLabel?: string;
  /**
   * The title and accessible name of the list of a name's uses (Shift+F12).
   *
   * @default "Uses of {name}"
   */
  usesTitle?: string;
  /**
   * Said when the list of uses opens or grows, for one use.
   *
   * @default "{count} use"
   */
  usesOne?: string;
  /**
   * Said when the list of uses opens or grows, for several uses.
   *
   * @default "{count} uses"
   */
  usesOther?: string;
  /**
   * The list of uses when nothing uses the name.
   *
   * @default "No uses"
   */
  noUses?: string;
  /**
   * The list of uses when no open document uses the name and the host does not search its other files (no `onFindUses`).
   *
   * @default "No uses in open files"
   */
  noUsesInOpenFiles?: string;
  /**
   * The list of uses while the host searches its other files.
   *
   * @default "Searching other files…"
   */
  usesSearching?: string;
  /**
   * The list of uses when the host's search of its other files failed (`onFindUses` threw, rejected or answered no list of uses): shown under the open documents' uses, or alone.
   *
   * @default "The other files could not be searched"
   */
  usesSearchFailed?: string;
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
   * A list with no item its `contains` schema allows.
   *
   * @default "no item of the list matches its \"contains\" schema"
   */
  noItemContained?: string;
  /**
   * A list with fewer items its `contains` schema allows than its `minContains` (`{limit}`).
   *
   * @default "fewer than {limit} items of the list match its \"contains\" schema"
   */
  tooFewContained?: string;
  /**
   * A list with more items its `contains` schema allows than its `maxContains` (`{limit}`).
   *
   * @default "more than {limit} items of the list match its \"contains\" schema"
   */
  tooManyContained?: string;
  /**
   * A list with fewer items (`{count}`) than its `minItems` (`{limit}`).
   *
   * @default "the number of items must be at least {limit}, and is {count}"
   */
  tooFewItems?: string;
  /**
   * A list with more items (`{count}`) than its `maxItems` (`{limit}`).
   *
   * @default "the number of items must be at most {limit}, and is {count}"
   */
  tooManyItems?: string;
  /**
   * An object with fewer keys (`{count}`) than its `minProperties` (`{limit}`).
   *
   * @default "the number of keys must be at least {limit}, and is {count}"
   */
  tooFewKeys?: string;
  /**
   * An object with more keys (`{count}`) than its `maxProperties` (`{limit}`).
   *
   * @default "the number of keys must be at most {limit}, and is {count}"
   */
  tooManyKeys?: string;
  /**
   * A list holding an item (`{item}`, as JSON) more than once where its `uniqueItems` wants each item different.
   *
   * @default "the list holds {item} more than once: each item must differ"
   */
  repeatedItem?: string;
  /**
   * A value its `not` schema (`{schema}`, as JSON) matches, where no plainer words below say why.
   *
   * @default "{value} is not allowed here"
   */
  mustNotMatch?: string;
  /**
   * A text matching the pattern its `not` schema refuses (`not: {pattern: …}`): the pattern (`{pattern}`), quoted as JSON.
   *
   * @default "{value} must not match the pattern {pattern}"
   */
  mustNotMatchPattern?: string;
  /**
   * A value of a type its `not` schema refuses (`not: {type: …}`): the types (`{types}`), each quoted, joined by `, `.
   *
   * @default "{value} must not be of type {types}"
   */
  mustNotBeType?: string;
  /**
   * A value its `not` schema refuses among others by name (`not: {enum: […]}` of two or more values): the refused values (`{values}`), as JSON, joined by `, `. One refused value (`not: {const: …}`) is told by `must_not_match`.
   *
   * @default "{value} is one of the values not allowed here: {values}"
   */
  mustNotBe?: string;
  /**
   * An object holding keys its `not` schema refuses one by one (`not: {anyOf: [{required: [a]}, {required: [b]}]}`): the ones it holds (`{keys}`), each quoted, joined by `, `; the object is `{value}`.
   *
   * @default "{keys} must not be set here"
   */
  mustNotHave?: string;
  /**
   * An object holding both keys its `not` schema keeps apart (`not: {required: [a, b]}`, or a key's dependent schema's `not`): the first (`{key}`) and the other (`{other}`), each quoted.
   *
   * @default "{key} can't be used together with {other}"
   */
  keysUsedTogether?: string;
  /**
   * An object holding all of three or more keys its `not` schema keeps apart (`not: {required: [a, b, c]}`): the keys (`{keys}`), each quoted, joined by `, `.
   *
   * @default "{keys} can't all be used together"
   */
  keysAllUsedTogether?: string;
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
   * A schema its format cannot read (`{format}`: the format's label, such as `XSD`).
   *
   * @default "Invalid {format}: {error}"
   */
  invalidSchemaSource?: string;
  /**
   * A schema whose text is empty, or only spaces and line breaks.
   *
   * @default "Invalid JSON Schema: the schema text is empty"
   */
  emptySchema?: string;
  /**
   * A schema format's check that could not run (a JavaScript check that threw, or gave something else than problems): the format's `{format}`, the check's name (`{check}`) and why (`{reason}`).
   *
   * @default "{format}'s check \"{check}\" did not run: {reason}"
   */
  schemaCheckFailed?: string;
  /**
   * An XSD check (cargo feature `xsd`): an attribute, or another language's key, named `{name}`, that its element neither declares nor allows by a wildcard.
   *
   * @default "\"{name}\" is neither declared here nor allowed by a wildcard"
   */
  xsdNotDeclared?: string;
  /**
   * An XSD check: an attribute named `{name}` in an XML text, where its element declares a child element of that name and no such attribute.
   *
   * @default "\"{name}\" is a child element here, not an attribute"
   */
  xsdElementAsAttribute?: string;
  /**
   * An XSD check: child element `{name}` where its element's content model allows no more.
   *
   * @default "<{name}> is not expected here: no more elements are allowed"
   */
  xsdNoMoreElements?: string;
  /**
   * An XSD check: child element `{name}` where its element's content model wants another (`{expected}`, worded as `xsd_one_of`).
   *
   * @default "<{name}> is not expected here; expected {expected}"
   */
  xsdElementNotExpected?: string;
  /**
   * An XSD check: the child elements end before their element's content model does; it wants `{expected}` (worded as `xsd_one_of`).
   *
   * @default "an element is missing at the end: expected {expected}"
   */
  xsdElementMissing?: string;
  /**
   * What an XSD check expects when it is one of several: the others (`{names}`, joined by `, `) and the `{last}`; one alone is named as it is.
   *
   * @default "one of {names} or {last}"
   */
  xsdOneOf?: string;
  /**
   * What an XSD check expects where a wildcard (`xs:any`) allows an element of another name.
   *
   * @default "another element"
   */
  xsdAnotherElement?: string;
  /**
   * An XSD check: an element an `xs:key` (`{name}`) selects has no value for its fields (`{fields}`, joined by `, `).
   *
   * @default "the xs:key \"{name}\" needs a value for {fields}"
   */
  xsdKeyNeedsValue?: string;
  /**
   * An XSD check: a value (`{value}`, the fields' values joined by `, `) an `xs:key` or `xs:unique` (`{kind}`: `key` or `unique`, named `{name}`) has at `{first}` already.
   *
   * @default "\"{value}\" is already the value of the xs:{kind} \"{name}\" at {first}"
   */
  xsdValueRepeated?: string;
  /**
   * An XSD check: an `xs:keyref` (`{name}`) value (`{value}`) that no element of its `xs:key` (`{key}`) has.
   *
   * @default "\"{value}\" is no value of the xs:key \"{key}\" (xs:keyref \"{name}\")"
   */
  xsdKeyrefUnknown?: string;
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
   * @default "{pointer}: {host} cannot keep this value exactly, as it reads back differently: change it, or edit it as {host}, to save"
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
   * @default "{host} has no null value: give it a value or remove it to save (at {pointer})"
   */
  noNull?: string;
  /**
   * An infinity or NaN the host's language has no number for (Edit-as).
   *
   * @default "{host} has no infinity or NaN: write a finite number or remove it to save (at {pointer})"
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
  | "jump-flash"
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
  | "tab-dirty"
  | "tab-close-bg"
  | "tab-close-hover-bg"
  | "tab-add-bg"
  | "tab-add-text"
  | "tab-add-hover-bg"
  | "tab-add-hover-text"
  | "toolbar-bg"
  | "toolbar-border"
  | "footer-bg"
  | "footer-border"
  | "footer-text";

/** Color overrides: a CSS color per token, e.g. `{ bg: "#101418", key: "#7dd3fc" }`. */
export type ThemeColorMap = Partial<Record<ThemeColorToken, string>>;

/** A token `chrome` accepts: a font, spacing, border or radius of the tabs, the toolbar or the footer (from `ChromeToken` in the crate). */
export type ThemeChromeToken =
  | "tab-font-family"
  | "tab-font-size"
  | "tab-font-weight"
  | "tab-font-style"
  | "tab-active-font-weight"
  | "tab-readonly-font-style"
  | "tab-padding"
  | "tab-gap"
  | "tab-border-width"
  | "tab-active-border-width"
  | "tab-radius"
  | "tab-bar-radius"
  | "tab-close-radius"
  | "tab-close-font-size"
  | "toolbar-font-family"
  | "toolbar-font-size"
  | "toolbar-font-weight"
  | "toolbar-font-style"
  | "toolbar-padding"
  | "toolbar-gap"
  | "toolbar-border-width"
  | "footer-font-family"
  | "footer-font-size"
  | "footer-font-weight"
  | "footer-font-style"
  | "footer-padding"
  | "footer-gap"
  | "footer-border-width";

/** Chrome overrides: a CSS value per token, e.g. `{ "tab-padding": "0 12px", "footer-font-size": "12px" }`. */
export type ThemeChromeMap = Partial<Record<ThemeChromeToken, string>>;

/** A built-in language name (from `Language::ALL` in the crate; its file extensions name it too, `"yml"`, `"tf"`) or one from `registerLanguage`. */
export type LanguageName =
  | "yaml"
  | "json"
  | "toml"
  | "hcl"
  | "xml"
  | (string & {});

/** A bundled theme slug (from `EditorTheme::ALL` in the crate). See `themes()`. */
export type ThemeName =
  | "vscode-dark"
  | "vscode-light"
  | "obsidian"
  | "obsidian-light"
  | "dracula"
  | "nord"
  | "atom-one-dark"
  | "github-light"
  | "github-dark"
  | "solarized-light"
  | "vira-inspired"
  | (string & {});
