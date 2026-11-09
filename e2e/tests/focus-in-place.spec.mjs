import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// The editor focusing itself from code (the host's `focus()`, a document
// opened or switched to) leaves the host page where it is. A plain
// `element.focus()` scrolls the page to bring the element into view, and the
// text area is as tall as the document: the page jumped down and hid the
// host's own controls above the editor (one host: the settings tabs and its
// Cancel / Review changes buttons).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const page = (name) => pathToFileURL(join(js, 'examples', name)).href;
test.skip(!built, 'js packages not built: run `npm run build` in js/');

const long = Array.from({ length: 300 }, (_, index) => `key${index}: ${index}`).join('\n') + '\n';

/** A host page taller than the window whose editor starts below what is in view (the host's controls fill the window). */
async function hostPage(browserPage, name) {
  await browserPage.setViewportSize({ width: 1000, height: 700 });
  await browserPage.goto(page(name));
  await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await browserPage.addStyleTag({ content: 'body { padding-top: 900px; padding-bottom: 4000px; }' });
}

async function scrolledTo(browserPage, y) {
  await browserPage.evaluate((top) => window.scrollTo(0, top), y);
  await browserPage.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));
  return browserPage.evaluate(() => window.scrollY);
}

const settled = (browserPage) =>
  browserPage.evaluate(() => new Promise((done) => requestAnimationFrame(() => setTimeout(done, 100))));

test('the host focusing the editor leaves the page where it is', async ({ page: browserPage }) => {
  await hostPage(browserPage, 'vanilla.html');
  await browserPage.evaluate((value) => window.editor.update({ value }), long);
  await browserPage.locator('.config-editor__input').waitFor();
  const before = await scrolledTo(browserPage, 100);
  await browserPage.evaluate(() => window.editor.focus());
  await settled(browserPage);
  expect(await browserPage.evaluate(() => document.activeElement?.classList.contains('config-editor__input'))).toBe(true);
  expect(await browserPage.evaluate(() => window.scrollY)).toBe(before);
});

test('switching documents leaves the page where it is', async ({ page: browserPage }) => {
  await hostPage(browserPage, 'documents.html');
  await browserPage.evaluate((value) => window.updateFile('limits', { value: `[limits]\n${value.replace(/: /g, ' = ')}` }), long);
  const before = await scrolledTo(browserPage, 100);
  // A click on the tab (element.click(): Playwright's own click would scroll the tab into view first).
  await browserPage.getByRole('tab', { name: /limits/ }).evaluate((tab) => tab.click());
  await settled(browserPage);
  await expect(browserPage.locator('.config-editor__input:visible')).toHaveValue(/key299 = 299/);
  expect(await browserPage.evaluate(() => window.scrollY)).toBe(before);
});
