// The gallery page docs/screenshots/README.md, written from this list by
// `node screenshots.mjs` (and `--gallery` alone): each section's screenshots
// with one line about them. A screenshot here must exist, and every
// screenshot in docs/screenshots must be here (`--check` fails otherwise).

export const GALLERY = [
  {
    section: 'Writing',
    items: [
      { title: 'Schema completion', caption: 'Keys and values from the JSON Schema, with their descriptions; accepting one writes it with the right formatting.', images: [['completion.png', 'Schema completion in YAML']] },
      { title: 'Validation as you type', caption: 'Syntax and schema problems underlined at the exact key or value; the message shows on hover and the footer counts them.', images: [['diagnostics.png', 'A schema problem on its value']] },
      { title: 'Hover docs', caption: "A key's description, type, default and examples, from the schema.", images: [['hover.png', 'Hover docs for a key']] },
      {
        title: 'Schema help',
        caption: 'Deprecated keys are struck through and listed last, and a key written in the document gets a warning with what to use instead. Documented values (`x-enum-descriptions`, `oneOf` constants) show their own description, and `x-docs-url` adds a documentation link.',
        images: [
          ['schema-help.png', 'A deprecated key in the suggestions'],
          ['deprecated.png', 'The warning on a deprecated key'],
          ['schema-docs.png', 'A documented value with a docs link'],
        ],
      },
    ],
  },
  {
    section: 'Languages',
    items: [
      { title: 'Edit as another language', caption: "A YAML file edited as TOML (any pair of YAML, JSON, TOML, HCL and XML); edits are saved back in the file's own language.", images: [['edit-as.png', 'A YAML file edited as TOML']] },
      { title: 'What Edit-as keeps', caption: "A notice under the toolbar says the file's comments and formatting are not shown in the edited language (saving keeps them) and that comments typed there are not saved; it can be dismissed, and its texts come from the `strings` setting.", images: [['edit-as-notice.png', 'The Edit-as notice for a YAML file edited as TOML']] },
    ],
  },
  {
    section: 'Reviewing changes',
    items: [
      {
        title: 'Changes against the baseline',
        caption: 'Added, modified and removed lines in the gutter, each with a before/after preview and **Revert**; `Alt+F5` moves between changes, `Alt+F3` opens one with the focus on Revert, and the footer counts them.',
        images: [
          ['diff.png', 'The preview of a modified line'],
          ['changes.png', 'Moving to a change with Alt+F3'],
        ],
      },
      { title: 'The file changed on the server', caption: 'A new baseline while there are unsaved edits asks before replacing them.', images: [['server-change.png', 'The baseline notice with Reload and Keep mine']] },
    ],
  },
  {
    section: 'Finding your way',
    items: [
      { title: 'Find and replace', caption: '`Ctrl+F` / `Ctrl+H`, with match case, whole word and regex.', images: [['find-replace.png', 'Find and replace']] },
      { title: 'Go to line', caption: '`Ctrl+G`, with `line` or `line:column`.', images: [['go-to-line.png', 'Go to line']] },
      { title: 'Folding', caption: 'Gutter chevrons and `Ctrl+K Ctrl+0` / `Ctrl+K Ctrl+J`; a folded block shows `…` and keeps the markers of what it hides.', images: [['folding.png', 'Every top-level block folded']] },
    ],
  },
  {
    section: 'Host control',
    items: [
      { title: 'Read-only fields', caption: 'Fields locked by the schema (`readOnly`) or the host: a lock in the gutter, the value muted, and a refused edit explains why.', images: [['read-only.png', 'Typing into a read-only field']] },
      { title: 'Secrets', caption: 'Placeholders the host gives (`«secret 3f2a»`) are single tokens: the caret steps over them and they are kept unless replaced.', images: [['secrets.png', 'Secret placeholders with their hint']] },
      { title: 'Host diagnostics', caption: "Problems from the host (a server's validation), by path or range, with their source, code and docs link; info and hint severities too.", images: [['host-diagnostics.png', 'A host problem with its source and code']] },
      { title: 'Several documents', caption: '`ConfigDocuments`: a main file and drop-ins as tabs, each with its own language, baseline and read-only state; the dot marks unsaved edits.', images: [['documents.png', 'Three documents as tabs']] },
    ],
  },
  {
    section: 'Accessibility',
    items: [
      { title: 'Leaving the editor with the keyboard', caption: '`Tab` indents while editing; `Ctrl+M` makes `Tab` move the focus instead (the footer says so until it is turned off), and `Escape` then `Tab` leaves once. Screen readers hear the problem at the caret and each `F8` stop.', images: [['tab-focus.png', 'Tab moves focus, shown in the footer']] },
    ],
  },
  {
    section: 'Looks',
    items: [
      {
        title: 'Themes',
        caption: 'Eleven themes, every color a CSS variable.',
        images: [
          ['light-theme.png', 'VS Code Light'],
          ['theme-dracula.png', 'Dracula'],
          ['theme-nord.png', 'Nord'],
          ['theme-github-light.png', 'GitHub Light'],
          ['theme-solarized-light.png', 'Solarized Light'],
        ],
      },
      { title: 'Phones', caption: "The editor fits a phone's width; suggestions and the footer stay usable.", images: [['phone.png', 'The editor at 390 px wide', 390]] },
    ],
  },
];

/** The gallery page's Markdown. */
export function galleryMarkdown() {
  const lines = [
    '<!-- Written by e2e/screenshots.mjs from e2e/screenshots-gallery.mjs; edit those, not this file. -->',
    '',
    '# Screenshots',
    '',
    'Every feature of the editor, as it looks in the demo (`trunk serve` at the',
    'repository root) and the JavaScript examples (`js/examples/`). They are taken',
    'by [`e2e/screenshots.mjs`](../../e2e/screenshots.mjs) with real keys and',
    'mouse; refresh them after a UI change with `cd e2e && npm run screenshots`.',
  ];
  for (const { section, items } of GALLERY) {
    lines.push('', `## ${section}`);
    for (const { title, caption, images } of items) {
      lines.push('', `### ${title}`, '', caption);
      for (const [file, alt, width] of images) {
        lines.push('', width ? `<img src="${file}" alt="${alt}" width="${width}">` : `![${alt}](${file})`);
      }
    }
  }
  return `${lines.join('\n')}\n`;
}

/** The files the gallery shows. */
export const galleryFiles = () => GALLERY.flatMap(({ items }) => items.flatMap(({ images }) => images.map(([file]) => file)));
