import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

// P5: a Replace all in an Edit-as view of a real config reaches the host in
// time that grows with the document, not faster. The host is K copies of
// the large sample under keys of their own (JSON, shown as YAML), and every
// `hsts: true` becomes `false`; the time is from the click to the save the
// host is sent. Timed: runs alone after the other specs (see the config).

// Four times the document may take at most this many times as long: about
// 1.8 here (the typing pause is in both), 5.7 when each item's link read
// the whole host text.
const RATIO = 3.5;

const here = dirname(fileURLToPath(import.meta.url));
const sample = readFileSync(
  join(here, '..', '..', 'crates', 'config-editor-leptos', 'tests', 'fixtures', 'samples', 'tunnel.large.json'),
  'utf8',
);

/** `copies` copies of the sample, each under a key of its own. */
function host(copies) {
  const parts = Array.from({ length: copies }, (_, at) => `"copy${at}": ${sample.trim().replace(/\n/g, '\n  ')}`);
  return `{\n  ${parts.join(',\n  ')}\n}\n`;
}

/** Milliseconds from Replace all to the host's save, in a YAML view of `copies` copies. */
async function replaceAll(page, baseURL, copies) {
  await page.addInitScript((text) => {
    window.configEditorSample = text;
  }, host(copies));
  await page.goto(`${baseURL}?language=json&sample=page`);
  const input = page.locator('.config-editor__input');
  await input.waitFor();
  await page.locator('.demo__edit-select').selectOption('yaml');
  await expect.poll(() => input.inputValue(), { timeout: 30000 }).toContain('hsts: true');
  await input.focus();
  await page.keyboard.press('Control+h');
  const find = page.locator('.config-editor__find');
  await find.getByRole('textbox', { name: 'Find' }).fill('hsts: true');
  await find.getByRole('textbox', { name: 'Replace' }).fill('hsts: false');
  // The view's own pause after the switch is over before the clock starts.
  await page.waitForTimeout(1000);
  await page.evaluate(() => {
    window.savedAt = 0;
    new MutationObserver(() => {
      window.savedAt ||= performance.now();
    }).observe(document.querySelector('.demo__output-text'), { childList: true, characterData: true, subtree: true });
  });
  const start = await page.evaluate(() => performance.now());
  await find.getByRole('button', { name: 'Replace all' }).click();
  await page.waitForFunction(() => window.savedAt > 0, null, { timeout: 60000 });
  const took = await page.evaluate((from) => window.savedAt - from, start);
  const saved = await page.locator('.demo__output-text').textContent();
  expect(saved).not.toContain('"hsts": true');
  expect(saved).toContain('"hsts": false');
  return took;
}

test('a Replace all in an Edit-as view of a real config reaches the host in time growing with the config', async ({
  browser,
  baseURL,
}) => {
  test.setTimeout(180_000);
  const times = {};
  for (const copies of [1, 4]) {
    const page = await browser.newPage();
    times[copies] = await replaceAll(page, baseURL, copies);
    await page.close();
  }
  console.log(`Replace all to save: ${Math.round(times[1])} ms at one copy, ${Math.round(times[4])} ms at four`);
  expect(times[4] / times[1]).toBeLessThan(RATIO);
});
