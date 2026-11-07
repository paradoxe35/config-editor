// Generated from crates/config-editor-leptos/src/settings.rs by js/config-editor/tests/settings.rs; do not edit. Regenerate with `UPDATE_GENERATED=1 cargo test -p config-editor-js --test settings`.

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
   * Inject the preset stylesheet. Off, style the `config-editor__*` markup yourself (the preset is `PRESET_CSS`, `presetCss()` in JavaScript).
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
   * Show line changes against the baseline in the gutter (`Alt+F5` / `Alt+F3` move between them). Off, the change summary stays at zero.
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
   * Show the schema's description, type, default and examples when hovering a key.
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
   * Find and replace (`Ctrl+F`, `Ctrl+H`, `F3`). Off, those keys are left to the browser.
   *
   * @default true
   */
  search?: boolean;
  /**
   * Go to line (`Ctrl+G`). Off, the key is left to the browser.
   *
   * @default true
   */
  goToLine?: boolean;
  /**
   * Folding by structure: chevrons in the gutter, `Ctrl+Shift+[` and `Ctrl+Shift+]` at the caret, `Ctrl+K Ctrl+0` and `Ctrl+K Ctrl+J` for everything (`Cmd` on macOS). View only: the text stays whole. Switching it off opens every fold.
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
}
