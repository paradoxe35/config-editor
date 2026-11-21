import { expect, test } from '@playwright/test';
import { BUILT, PAGES, installProbe, pageUrl } from './pages.mjs';
import { giveLock, host, settle } from './perform.mjs';

// The browser fuzz plays a host: the ranges it gives are worked out on the
// text it lists them with, on every page, even when a late echo leaves the
// host's text older than the editor's (`?manual`: the test hands the edits
// back, newest first). A range worked out on the editor's text instead
// runs past the host's end, which the editor rightly refuses.

const TYPED = 'é日😀';
const LATE = ['react', 'vue', 'svelte', 'documents-react', 'documents-vue', 'documents-svelte'];

async function open(browser, name, query) {
  const context = await browser.newContext({ viewport: { width: 1100, height: 620 } });
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', (error) => errors.push(`pageerror: ${error}`));
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text().split('\n')[0]));
  await page.goto(pageUrl(name, query));
  await page.waitForFunction(() => document.body.dataset.ready === 'true', null, { timeout: 20000 });
  await page.locator('.config-editor__input').first().waitFor({ timeout: 20000 });
  await page.evaluate(installProbe, PAGES[name]);
  await page.waitForFunction(() => !!window.__fz.editor(), null, { timeout: 10000 });
  const locks = [];
  const run = { page, kind: PAGES[name].kind, locked: false, setLock: (id, slot, lock) => locks.push(lock) };
  return { context, page, errors, run, locks };
}

// Types `TYPED`; on a late page hands the newest edit back first, then,
// once the host has it, the older ones: the host ends on an older text.
async function typeAndLeaveHostBehind(page, late) {
  await page.evaluate(() => window.__fz.root()?.querySelector('.config-editor__input')?.focus());
  await page.keyboard.type(TYPED);
  if (!late) return;
  const held = await page.evaluate(() => window.heldEchoes());
  expect(held).toBeGreaterThan(1);
  await page.evaluate((last) => window.releaseEchoes([last]), held - 1);
  expect(await settle(page)).toBe(true);
  await page.evaluate((count) => window.releaseEchoes([...Array(count).keys()]), held - 1);
  const { value, text } = await page.evaluate(() => ({ value: window.__fz.editor().value, text: window.__fz.host() }));
  expect(text.length).toBeLessThan(value.length);
}

test.describe('the fuzz host gives ranges in its own text', () => {
  test.skip(!BUILT, 'js packages not built: run `npm run build` in js/');

  for (const name of Object.keys(PAGES)) {
    const late = LATE.includes(name);
    test(`a lock on ${name}${late ? ' behind a late echo' : ''}`, async ({ browser }) => {
      const { context, page, errors, run, locks } = await open(browser, name, late ? '?manual' : '');
      await typeAndLeaveHostBehind(page, late);
      // Near the end: worked out on the editor's longer text, it would run
      // past the host's.
      await giveLock(run, { at: 0.95, length: 30, line: false });
      await page.waitForTimeout(150);
      expect(errors).toEqual([]);
      expect(run.locked).toBe(true);
      const { value, text } = await page.evaluate(() => ({ value: window.__fz.editor().value, text: window.__fz.rangesText() }));
      const [lock] = locks;
      expect(lock.range.end).toBeLessThanOrEqual(text.length);
      expect(value).toContain(lock.text);
      await context.close();
    });
  }

  for (const name of Object.keys(PAGES)) {
    const late = LATE.includes(name);
    test(`a reset with ranges on ${name}${late ? ' before its echo' : ''}`, async ({ browser }) => {
      const { context, page, errors, run } = await open(browser, name, late ? '?manual' : '');
      const text = `listen: 7835\npublic_address: ${TYPED}.example.com\nextra: a value past the old end\n`;
      const ranges = [{ start: text.length - 12, end: text.length }];
      const call = run.kind === 'single' ? 'singleValue' : 'docValue';
      await host(page, call, { text, how: 'setValue', ranges });
      await page.waitForTimeout(150);
      if (late) await page.evaluate(() => window.releaseEchoes([...Array(window.heldEchoes()).keys()]));
      await page.waitForTimeout(150);
      expect(errors).toEqual([]);
      expect(await page.evaluate(() => window.__fz.editor().value)).toBe(text);
      await context.close();
    });
  }
});
