import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

// P5: a letter typed in an Edit-as view of a large XML config reaches the
// host in time that grows with the document, not faster. The host is the
// large XML sample with its services (most of it) K times over, each copy
// of a service named apart, shown as JSON; one letter is typed into the
// first `tunnel.example.com`, and the time is from the key to the save
// holding it. Timed: runs alone after the other specs (see the config).

// Four times the services may take at most this many times as long: about
// 2.4 here (the typing pause is in both), 6.5 when the save read each
// element's lines again for each of its lines.
const RATIO = 4;

const here = dirname(fileURLToPath(import.meta.url));
const sample = readFileSync(
  join(here, '..', '..', 'crates', 'config-editor-leptos', 'tests', 'fixtures', 'samples', 'tunnel.large.xml'),
  'utf8',
);

/** The sample with each service `copies` times, the copies named apart (`web-1`). */
function host(copies) {
  const lines = sample.split('\n');
  const open = lines.indexOf('  <services>');
  const close = lines.indexOf('  </services>');
  const services = lines.slice(open + 1, close);
  const copy = (at) => services.map((line) => (at > 0 ? line.replace(/^( {4}<\/?)([\w.-]+)/, `$1$2-${at}`) : line));
  const all = Array.from({ length: copies }, (_, at) => copy(at)).flat();
  return [...lines.slice(0, open + 1), ...all, ...lines.slice(close)].join('\n');
}

/** Milliseconds from a letter typed to the host's save, in a JSON view of `copies` copies of the services. */
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
    const output = document.querySelector('.demo__output-text');
    new MutationObserver(() => {
      if (output.textContent.includes('tunnela.example.com')) window.savedAt ||= performance.now();
    }).observe(output, { childList: true, characterData: true, subtree: true });
  });
  const start = await page.evaluate(() => performance.now());
  await page.keyboard.type('a');
  await page.waitForFunction(() => window.savedAt > 0, null, { timeout: 90000 });
  const took = await page.evaluate((from) => window.savedAt - from, start);
  const saved = await page.locator('.demo__output-text').textContent();
  expect(saved).toContain('<public_address>tunnela.example.com</public_address>');
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
