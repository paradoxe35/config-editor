import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

// P5: a letter typed in an Edit-as view of a large XML config reaches the
// host in time that grows with the document, not faster. The host is K
// copies of the large XML sample's elements, each under an element of its
// own inside the config root, shown as JSON; one letter is typed into the
// first `tunnel.example.com`, and the time is from the key to the save the
// host is sent. Timed: runs alone after the other specs (see the config).

// Four times the document may take at most this many times as long: about
// 3 here (the typing pause is in both), 9 and more when the save read
// each element's lines again for each of its lines.
const RATIO = 6;

const here = dirname(fileURLToPath(import.meta.url));
const sample = readFileSync(
  join(here, '..', '..', 'crates', 'config-editor-leptos', 'tests', 'fixtures', 'samples', 'tunnel.large.xml'),
  'utf8',
);

/** `copies` copies of the sample's elements, each under an element of its own. */
function host(copies) {
  const lines = sample.trimEnd().split('\n');
  const open = lines.findIndex((line) => line.includes('config-root="true"'));
  const head = lines.slice(0, open + 1);
  const body = lines.slice(open + 1, -1).map((line) => `  ${line}`);
  const parts = Array.from({ length: copies }, (_, at) => [`  <copy${at}>`, ...body, `  </copy${at}>`]);
  return [...head, ...parts.flat(), lines.at(-1), ''].join('\n');
}

/** Milliseconds from a letter typed to the host's save, in a JSON view of `copies` copies. */
async function typed(page, baseURL, copies) {
  await page.addInitScript((text) => {
    window.configEditorSample = text;
  }, host(copies));
  await page.goto(`${baseURL}?language=xml&sample=page&edit=json`);
  const input = page.locator('.config-editor__input');
  await input.waitFor();
  await expect.poll(() => input.inputValue(), { timeout: 30000 }).toContain('"public_address": "tunnel.example.com"');
  // The view's own pause after the switch is over before the clock starts.
  await page.waitForTimeout(1500);
  await input.evaluate((element) => {
    element.focus();
    const at = element.value.indexOf('tunnel.example.com') + 6;
    element.setSelectionRange(at, at);
  });
  await page.evaluate(() => {
    window.savedAt = 0;
    new MutationObserver(() => {
      window.savedAt ||= performance.now();
    }).observe(document.querySelector('.demo__output-text'), { childList: true, characterData: true, subtree: true });
  });
  const start = await page.evaluate(() => performance.now());
  await page.keyboard.type('a');
  await page.waitForFunction(() => window.savedAt > 0, null, { timeout: 90000 });
  const took = await page.evaluate((from) => window.savedAt - from, start);
  const saved = await page.locator('.demo__output-text').textContent();
  expect(saved).toContain('<public_address>tunnea.example.com</public_address>');
  return took;
}

test('a letter typed in an Edit-as view of a large XML config reaches the host in time growing with the config', async ({
  browser,
  baseURL,
}) => {
  test.setTimeout(240_000);
  const times = {};
  for (const copies of [1, 4]) {
    const page = await browser.newPage();
    times[copies] = await typed(page, baseURL, copies);
    await page.close();
  }
  console.log(`Letter to save: ${Math.round(times[1])} ms at one copy, ${Math.round(times[4])} ms at four`);
  expect(times[4] / times[1]).toBeLessThan(RATIO);
});
