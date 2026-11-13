import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// The `strings` setting on the vanilla example page: every built-in text
// set to a marked value (`«key»`, with the placeholders of its default)
// reaches the screen, the accessible names and the announcements as the
// user goes through the editor with real keys and mouse; texts change live
// (the editor is not mounted again); counts pick their singular or plural
// text; a partial `strings` keeps English for the rest, and an unknown key
// or placeholder is warned about. Run against the built packages
// (`npm run build` in js/).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const vanilla = pathToFileURL(join(js, 'examples', 'vanilla.html')).href;

test.skip(!built, 'js packages not built: run `npm run build` in js/');

// Texts this walk does not reach, each checked by the crate's unit tests
// instead (`cargo test -p config-editor-leptos strings`, `edit_as`,
// `status_bar`, `protect`, `diagnostics`).
const ELSEWHERE = {
  noSuggestions: 'said only for a list the user asked for (Ctrl+Space or moving in it): completion-window.spec',
  infosOne: 'info-level problems come from hosts; counts are unit tested',
  infosOther: 'as infosOne',
  problemCode: 'a host problem with a code: a11y tests and host-diagnostics.spec',
  editAsHostInstructions: 'an XML host holding processing instructions: edit_as::tests (the notice names them)',
  problemDocs: 'as problemCode',
  hintsOne: 'as infosOne',
  hintsOther: 'as infosOne',
  warningsOther: 'one warning is enough to show the pair is used',
  errorsOther: 'as warningsOther',
  problemsCut: 'needs more than 20 problems: status_bar tests',
  problemsMore: 'as problemsCut',
  underlinesCut: 'needs more than 200 underlines: problem_summary tests and the qa-round20-ux spec',
  foldedProblemOne: 'a fold hiding a host problem: folding tests and the qa-round20-ux spec',
  foldedProblemOther: 'as foldedProblemOne (two problems: folding tests)',
  changesOther: 'one change on this page; status_bar tests',
  changeDetailsOther: 'as changesOther',
  changedAdded: 'as changesOther',
  changedRemoved: 'as changesOther',
  linesOne: 'the page has several lines; strings tests',
  replacedOther: 'one replacement here; strings tests',
  replaceSkipped: 'needs protected matches: search tests',
  nothingReplaced: 'as replaceSkipped',
  invalidPattern: 'unit tested with count_label',
  invalidPlaceholder: 'an unusable secrets pattern: protect pattern_tests (JS throws before mounting)',
  invalidSchemaSource: 'a schema its format refuses: JS throws before mounting (schema-formats.spec checks the message); shown inline in schema_formats tests (a_formatted_source_resolves_and_its_error_names_the_format)',
  oneOf: 'needs an object oneOf before any option matches (a schema problem, whose hover takes its place): choices_matrix',
  readOnlyChanged: 'a repair of a host text that never parsed changing a read-only value: protect repair_tests',
  readOnlyUnchecked: 'more lines changed in a broken host text than can be read: protect repair rule',
  editAsTypedCommentsNone: 'a JSON host: edit_as tests',
  addedLine: 'a modified line is previewed here; diff tests',
  removedLines: 'as addedLine',
  removedLinesAtEnd: 'as addedLine',
  showAdded: 'as addedLine',
  showAddedTitle: 'as addedLine',
  showRemoved: 'as addedLine',
  showRemovedTitle: 'as addedLine',
  showRemovedAtEnd: 'as addedLine',
  revertTitle: 'the change previewed here spans two lines (revertLinesTitle): changes spec',
  problemsHintTouch: 'a touch screen: problem_summary tests and the qa-round24-ux spec',
  suggestionsHintTouch: 'a touch screen: status_bar tests and the shortcut-hints spec',
  hoverFocused: 'a second Ctrl+K Ctrl+I on a card: the qa-round24-ux spec',
  hoverLabel: 'the focused hover card: the qa-round25-a11y spec',
  problemTipLabel: 'the focused problem tip: the qa-round25-a11y spec',
  changePosition: 'Alt+F5 with a baseline: the qa-round25-a11y spec',
  walkNoProblems: 'F8 with no problems: the qa-round30-a11y spec',
  noChanges: 'Alt+F5 with no changes: the qa-round30-a11y spec',
  reverted: 'reverting a change: the qa-round30-a11y spec',
  reloaded: 'Reload in the changed-baseline notice: the qa-round30-a11y spec',
  foldedLines: 'fold commands: the qa-round25-a11y spec and folding change_tests',
  unfoldedLines: 'as foldedLines',
  foldedRanges: 'as foldedLines',
  unfoldedRanges: 'as foldedLines',
  readOnlyEditor: 'typing in a read-only editor: the qa-round25-a11y spec',
  readOnlyEditorReason: 'typing in a read-only editor with a `readOnlyReason`: the documents-screen spec, framework-cases and a11y tests',
  readOnlyTabTitle: 'the tooltip of a read-only document with a `readOnlyReason`: the documents-screen spec',
  revertLineAndRemovedTitle: 'as revertTitle',
  defaultValue: 'the hovered key has no default: schema help tests',
  warning: 'severity names: a11y tests',
  info: 'as warning',
  hint: 'as warning',
  problemSource: 'host problems: a11y tests',
  required: 'no required key on this schema',
  deprecated: 'schema help tests',
  documentation: 'schema help tests (x-docs-url)',
  openDocs: 'host problems with a docs link: host diagnostics tests',
  openDocsTouch: 'a touch screen: the hover-card-text spec',
  openDocsLabel: 'the docs links of a tip listing host problems: the qa-round26-a11y spec',
  examples: 'schema help tests',
  readOnlyText: 'read-only ranges: protect tests',
  holdsReadOnly: 'renaming a locked field’s key: protect tests',
  saveRefused: 'an Edit-as save over a read-only value: protect tests',
  saveWaits: 'a broken text in a custom language without spans: qa-round21-host spec',
  secretHint: 'the page gives its own hint',
  secretValue: 'schema messages on secret fields: protect tests',
  unknownPlaceholder: 'protect tests',
  movedPlaceholder: 'protect tests (moved_token_tests) and the qa-round15 spec',
  placeholderInComment: 'a field placeholder in a comment: protect tests (bound_tests)',
  commentPlaceholder: 'a comment placeholder in a field: protect tests (bound_tests)',
  gluedPlaceholder: 'text beside a whole placeholder: protect tests (glued_tests) and the secret-guarantees spec',
  envReference: 'reference suggestions: secret tests',
  fileReference: 'as envReference',
  otherReference: 'as envReference',
  referenceType: 'as envReference',
  closesTag: 'XML end tags: completion tests',
  endTag: 'as closesTag',
  keyNotAllowed: 'diagnostics tests',
  textNotAllowed: 'diagnostics tests',
  keyDeprecated: 'schema help tests',
  keyDeprecatedInstead: 'schema help tests',
  notOneOf: 'diagnostics tests',
  notFormat: 'a value out of its format (drafts 4 to 7): schema_message tests',
  noOptionMatches: 'a value matching no oneOf/anyOf option: schema_option_messages tests',
  severalOptionsMatch: 'as noOptionMatches',
  optionNeeds: 'as noOptionMatches',
  optionRefuses: 'as noOptionMatches',
  optionProblem: 'as noOptionMatches',
  optionNumber: 'as noOptionMatches',
  invalidSchema: 'a schema that does not compile: diagnostics tests',
  hostCannotKeep: 'edit_as and bridge tests',
  editCannotShow: 'edit_as and bridge tests',
  notSaved: 'edit_as and bridge tests',
  noNull: 'edit_as tests',
  noNonFinite: 'non_finite_matrix tests',
  cannotSwitch: 'edit_as tests',
  switchRefused: 'edit_as switch_tests',
  mixedContent: 'save_xml_tests (XML mixed content refuses to convert)',
  tabList: 'the documents view: the last test here',
  noDocuments: 'as tabList',
  closeTab: 'as tabList',
  closeTabTitle: 'as tabList',
  addDocument: 'as tabList',
  unsaved: 'as tabList',
  readOnlyTab: 'as tabList',
  protectedNote: 'the caret note: a11y tests',
  problem: 'as protectedNote',
};

const input = (page) => page.locator('.config-editor__input');

async function open(page, options = {}) {
  const warnings = [];
  page.on('pageerror', (error) => warnings.push(String(error)));
  page.on('console', (message) => ['error', 'warning'].includes(message.type()) && warnings.push(message.text()));
  await page.goto(`${vanilla}?options=${encodeURIComponent(JSON.stringify(options))}`);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await input(page).waitFor();
  return warnings;
}

/** Every text key with its default, as the editor reports them. */
const defaults = (page) => page.evaluate(() => window.editor.settings.strings);

/** `«key»` with the placeholders of the default, so filled texts stay marked. */
function marked(texts) {
  return Object.fromEntries(
    Object.entries(texts).map(([key, text]) => {
      const placeholders = text.match(/\{[a-z_]+\}/g) ?? [];
      return [key, `«${key}»${placeholders.join('')}`];
    }),
  );
}

/** The marked keys on screen, in accessible names and tooltips, and said. */
const seenKeys = (page) =>
  page.evaluate(() => {
    const parts = [document.querySelector('#editor').innerText];
    for (const element of document.querySelectorAll('#editor *')) {
      for (const name of ['title', 'aria-label', 'placeholder']) {
        const value = element.getAttribute(name);
        if (value) parts.push(value);
      }
      if (element.matches('.config-editor__sr-only')) parts.push(element.textContent);
      if (element.dataset.message) parts.push(element.dataset.message);
    }
    return [...parts.join('\n').matchAll(/«(\w+)»/g)].map(([, key]) => key);
  });

/** Sets the texts `strings` gives, the others back to their defaults. */
async function setStrings(page, strings) {
  await page.evaluate((strings) => window.editor.update({ strings: null }) ?? window.editor.update({ strings }), strings);
}

test('every built-in text comes from the strings setting', async ({ page }) => {
  const warnings = await open(page);
  const texts = await defaults(page);
  const seen = new Set();
  const look = async () => (await seenKeys(page)).forEach((key) => seen.add(key));
  await input(page).evaluate((element) => (element.dataset.mark = 'kept'));
  await setStrings(page, marked(texts));
  await expect(page.locator('.config-editor__toolbar')).toContainText('«schemaConnected»');
  await look();

  // A schema problem and a change: the footer counts, the gutter marker.
  await input(page).click();
  await page.keyboard.press('Control+Home');
  for (let line = 1; line < 4; line += 1) await page.keyboard.press('ArrowDown');
  await page.keyboard.press('End');
  await page.keyboard.type('x');
  await expect(page.locator('.config-editor__diagnostic-status')).toContainText('«errorsOne»', { timeout: 5000 });
  await page.waitForTimeout(400);
  await look();

  // A warning too.
  await page.evaluate(() =>
    window.editor.update({ hostDiagnostics: [{ path: '/public_address', message: 'Hmm', severity: 'warning', source: 'host' }] }),
  );
  await expect(page.locator('.config-editor__diagnostic-status')).toContainText('«warningsOne»');
  await look();

  // F8 says where the problem is.
  await page.keyboard.press('F8');
  await expect(page.locator('.config-editor__announcement')).toContainText('«problemPosition»');
  await look();

  // Find and replace, with a count and a replacement.
  await input(page).click();
  await page.keyboard.press('Control+h');
  await page.locator('.config-editor__find-input').first().waitFor();
  await look();
  await page.keyboard.type('tunnel.example');
  await expect(page.locator('.config-editor__find-count')).toContainText('«matchPosition»');
  await look();
  await page.locator('.config-editor__replace-input').fill('relay');
  await page.locator('.config-editor__replace-input').press('Control+Alt+Enter');
  await expect(page.locator('.config-editor__find-count')).toContainText('«replacedOne»');
  await look();
  await page.locator('.config-editor__find-input').first().fill('no such text');
  await expect(page.locator('.config-editor__find-count')).toContainText('«noResults»');
  await look();
  await page.keyboard.press('Escape');

  // Go to line, with a wrong position.
  await input(page).click();
  await page.keyboard.press('Control+g');
  await look();
  await page.keyboard.type('abc');
  await expect(page.locator('.config-editor__goto-hint')).toContainText('«goToLineInvalid»');
  await look();
  await page.keyboard.press('Escape');

  // The change preview of the modified line.
  await page.locator('.config-editor__diff-marker-modified').first().click();
  await expect(page.locator('.config-editor__diff-peek')).toContainText('«previewHeading»');
  await look();
  await page.keyboard.press('Escape');

  // Hover docs for a key with a default.
  const listen = page.locator('.config-editor__highlight .config-editor__code-line').filter({ hasText: 'listen' }).first();
  const box = await listen.boundingBox();
  await page.mouse.move(box.x + 20, box.y + box.height / 2);
  await page.locator('.config-editor__hover').waitFor({ timeout: 5000 }).catch(() => {});
  await look();
  await page.mouse.move(0, 0);

  // Tab moves focus, on and off.
  await input(page).click();
  await page.keyboard.press('Control+m');
  await expect(page.locator('.config-editor__announcement')).toContainText('«tabFocusOn»');
  await look();
  await page.keyboard.press('Control+m');
  await expect(page.locator('.config-editor__announcement')).toContainText('«tabFocusOff»');
  await look();

  // A read-only field, and an edit it refuses.
  await page.locator('#lock-address').check();
  await expect(page.locator('.config-editor__lock-marker')).toHaveCount(1);
  await look();
  const address = page.locator('.config-editor__highlight .config-editor__code-line').filter({ hasText: 'public_address' }).first();
  const addressBox = await address.boundingBox();
  await page.mouse.click(addressBox.x + addressBox.width - 4, addressBox.y + addressBox.height / 2);
  await page.keyboard.type('z');
  await look();
  await page.locator('#lock-address').uncheck();

  // A secret placeholder.
  await page.locator('#load-secret').click();
  await expect(page.locator('.config-editor__secret-marker')).toHaveCount(1);
  await look();

  // Folding: open chevrons, then a closed fold hiding changed lines.
  await expect(page.locator('.config-editor__fold-toggle').first()).toBeAttached();
  await look();
  await input(page).click();
  await page.keyboard.press('Control+k');
  await page.keyboard.press('Control+0');
  await expect(page.locator('.config-editor__fold-chip').first()).toBeAttached();
  await look();
  await page.keyboard.press('Control+k');
  await page.keyboard.press('Control+j');

  // The server's version changes under unsaved edits.
  await input(page).click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type('\nlisten: 1');
  await page.locator('#external-change').click();
  await expect(page.locator('.config-editor__notice')).toBeVisible();
  await look();
  await page.locator('.config-editor__notice-keep').click();

  // Editing as JSON: the notice about the file's comments; then as TOML
  // with a comment typed in it.
  await page.locator('#replace').click();
  await page.evaluate(() => window.editor.update({ value: '# the port\nlisten: 7835\n' }));
  await page.locator('#edit-as').selectOption('json');
  await expect(page.locator('.config-editor__edit-as-notice')).toContainText('«editAsHostComments»');
  await look();
  await page.locator('#edit-as').selectOption('toml');
  await input(page).click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type('\n# typed');
  await expect(page.locator('.config-editor__edit-as-notice')).toContainText('«editAsTypedComments»');
  await look();

  // Still the editor mounted at first.
  expect(await input(page).evaluate((element) => element.dataset.mark)).toBe('kept');
  const missing = Object.keys(texts).filter((key) => !seen.has(key) && !(key in ELSEWHERE));
  expect(missing, 'texts that never reached the page').toEqual([]);
  const stale = Object.keys(ELSEWHERE).filter((key) => !(key in texts));
  expect(stale, 'exemptions for texts that no longer exist').toEqual([]);
  expect(warnings.filter((warning) => !warning.includes('«'))).toEqual([]);
});

test('texts change live, counts pick singular or plural, and English fills the rest', async ({ page }) => {
  await open(page);
  const footer = page.locator('.config-editor__status');
  await expect(footer).toContainText('No errors');
  await expect(footer).toContainText('5 lines');
  await input(page).evaluate((element) => (element.dataset.mark = 'kept'));
  await setStrings(page, { noProblems: 'Keine Fehler', linesOne: '{count} Zeile', linesOther: '{count} Zeilen' });
  await expect(footer).toContainText('Keine Fehler');
  await expect(footer).toContainText('5 Zeilen');
  await expect(page.locator('.config-editor__toolbar')).toContainText('Schema connected');
  await page.evaluate(() => window.editor.update({ value: 'listen: 7835' }));
  await expect(footer).toContainText('1 Zeile');
  await expect(footer).not.toContainText('1 Zeilen');
  // Back to English.
  await setStrings(page, {});
  await expect(footer).toContainText('No errors');
  await expect(footer).toContainText('1 line');
  expect(await input(page).evaluate((element) => element.dataset.mark)).toBe('kept');
});

test('an unknown text key and an unknown placeholder are warned about', async ({ page }) => {
  const warnings = await open(page, { strings: { noProblem: 'x', linesOther: '{cout} Zeilen' } });
  await expect.poll(() => warnings.join('\n')).toContain('unknown string "noProblem" is ignored (did you mean "noProblems"?)');
  await expect.poll(() => warnings.join('\n')).toContain('strings.linesOther: unknown placeholder {cout} (this text takes {count})');
  // Shown as written.
  await expect(page.locator('.config-editor__line-count')).toHaveText('{cout} Zeilen');
});

test('strings must be an object of texts', async ({ page }) => {
  await open(page);
  const error = await page.evaluate(() => {
    try {
      window.editor.update({ strings: { find: 3 } });
      return null;
    } catch (error) {
      return String(error.message ?? error);
    }
  });
  expect(error).toContain('strings.find');
  const notObject = await page.evaluate(() => {
    try {
      window.editor.update({ strings: 'Deutsch' });
      return null;
    } catch (error) {
      return String(error.message ?? error);
    }
  });
  expect(notObject).toContain('strings must be an object');
});

test('the documents view takes its tab texts from the strings setting', async ({ page }) => {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  await page.goto(pathToFileURL(join(js, 'examples', 'documents.html')).href);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.locator('[role="tab"]').first().waitFor();
  await page.evaluate(() =>
    window.documentsView.update({
      strings: {
        tabList: 'Offene Dateien',
        closeTab: '{name} schließen',
        closeTabTitle: 'Schließen (Entf)',
        addDocument: 'Neue Datei',
        unsaved: 'ungespeichert',
        readOnlyTab: 'schreibgeschützt',
        noDocuments: 'Keine Dateien',
      },
    }),
  );
  const strip = page.locator('[role="tablist"]');
  await expect(strip).toHaveAttribute('aria-label', 'Offene Dateien');
  const readOnly = page.locator('[role="tab"]', { hasText: 'clients.json' });
  await expect(readOnly).toHaveAttribute('aria-label', 'conf.d/clients.json, schreibgeschützt');
  const close = page.locator('.config-editor-documents__close').first();
  await expect(close).toHaveAttribute('title', 'Schließen (Entf)');
  await expect(close).toHaveAttribute('aria-label', /schließen$/);
  const add = page.locator('.config-editor-documents__add');
  await expect(add).toHaveAttribute('title', 'Neue Datei');
  await expect(add).toHaveAttribute('aria-label', 'Neue Datei');
  // An edit marks the tab unsaved, in the host's words.
  await page.locator('[role="tabpanel"]:not(.config-editor-documents__panel-hidden) .config-editor__input').click();
  await page.keyboard.type('x');
  await expect(page.locator('[role="tab"][aria-selected="true"]')).toHaveAttribute('aria-label', /, ungespeichert$/);
  // Every document closed: the empty view says so.
  page.on('dialog', (dialog) => dialog.accept());
  while ((await page.locator('.config-editor-documents__close').count()) > 0) {
    const before = await page.locator('[role="tab"]').count();
    await page.locator('.config-editor-documents__close').first().click();
    await expect(page.locator('[role="tab"]')).toHaveCount(before - 1);
  }
  // The main file is not closable until the host says so.
  await expect(page.locator('[role="tab"]')).toHaveCount(1);
  await page.evaluate(() => window.updateFile('main', { closable: true }));
  await page.locator('.config-editor-documents__close').click();
  await expect(page.locator('.config-editor-documents__empty')).toHaveText('Keine Dateien');
  expect(errors).toEqual([]);
});
