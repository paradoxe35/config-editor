import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// The suggestion list renders the rows in its view and a screen's worth
// more each way, not every candidate (D5.perf-3: 1,200 keys in scope built
// 1,200 options on every open and filter), and stays a listbox a screen
// reader can count (D12.a11y-1): each option says its place among all of
// them, the highlighted one is always in the page, and a filter that
// leaves nothing says so.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const VANILLA = pathToFileURL(join(js, 'examples', 'vanilla.html')).href;

/** A schema with `count` root keys, `k0001` up, each with a description. */
const schema = (count, described = true) =>
  JSON.stringify({
    type: 'object',
    properties: Object.fromEntries(
      Array.from({ length: count }, (_, index) => [
        `k${String(index + 1).padStart(4, '0')}`,
        described ? { type: 'string', description: `Key number ${index + 1}` } : { type: 'string' },
      ]),
    ),
  });

/** Mounts an editor with `count` root keys to suggest and opens the list. */
async function openWith(page, count, described = true) {
  await page.goto(VANILLA);
  await page.waitForFunction(() => document.body.dataset.ready === 'true');
  await page.evaluate(async (schema) => {
    const CE = await import(new URL('../config-editor/index.js', location.href).href);
    const host = document.createElement('div');
    host.id = 'listed';
    host.style.height = '300px';
    document.body.prepend(host);
    window.listed = new CE.ConfigEditor(host, { language: 'yaml', value: '', schema });
  }, schema(count, described));
  const input = page.locator('#listed .config-editor__input');
  await input.click();
  await page.keyboard.press('Control+Space');
  await expect(page.locator('#listed .config-editor__completions')).toBeVisible();
  return input;
}

const list = (page) => page.locator('#listed .config-editor__completions');
const options = (page) => page.locator('#listed .config-editor__completion');

/** What a screen reader is told of the list: each rendered option's place and count, and the highlighted one. */
const readList = (page) =>
  page.evaluate(() => {
    const host = document.getElementById('listed');
    const input = host.querySelector('.config-editor__input');
    const box = host.querySelector('.config-editor__completions').getBoundingClientRect();
    const active = input.getAttribute('aria-activedescendant');
    const element = active && document.getElementById(active);
    const rect = element?.getBoundingClientRect();
    return {
      options: [...host.querySelectorAll('.config-editor__completion')].map((option) => ({
        id: option.id,
        setsize: option.getAttribute('aria-setsize'),
        posinset: option.getAttribute('aria-posinset'),
        label: option.querySelector('.config-editor__completion-key').textContent,
      })),
      active: element
        ? {
            selected: element.getAttribute('aria-selected'),
            posinset: element.getAttribute('aria-posinset'),
            label: element.querySelector('.config-editor__completion-key').textContent,
            inView: rect.top >= box.top - 1 && rect.bottom <= box.bottom + 1,
          }
        : null,
    };
  });

/** The rows `row_window` renders at the top of a long list of rows `row` high in a 320 px popup. */
const windowAtTop = (row) => {
  const inView = Math.ceil(320 / row);
  return inView + 2 * Math.min(40, Math.max(10, inView));
};

test.describe('the suggestion list', () => {
  test.skip(!built, 'the npm package is not built: cd js && npm run build');

  test('rows are 28 px without descriptions and 44 px with, each list its own window', async ({ page }) => {
    for (const [described, row] of [[false, 28], [true, 44]]) {
      await openWith(page, 1200, described);
      const rows = await options(page).evaluateAll((elements) => elements.map((element) => element.getBoundingClientRect().height));
      expect(rows.length, `${row}px rows`).toBe(windowAtTop(row));
      expect(new Set(rows), `${row}px rows`).toEqual(new Set([row]));
      const height = await list(page).evaluate((element) => element.scrollHeight);
      expect(height).toBeGreaterThanOrEqual(1200 * row);
      expect(height).toBeLessThanOrEqual(1200 * row + 4);
      // Scrolled: the option at that place is rendered, the window follows.
      await list(page).evaluate((element, top) => {
        element.scrollTop = top;
        element.dispatchEvent(new Event('scroll'));
      }, 600 * row);
      await expect(page.locator('#listed .config-editor__completion[aria-posinset="601"]')).toBeVisible();
      expect(await options(page).count(), `${row}px rows scrolled`).toBeLessThanOrEqual(windowAtTop(row) + 1);
      // PageDown moves a popup's worth of whole rows, whatever their height.
      await openWith(page, 1200, described);
      await page.keyboard.press('PageDown');
      const { active } = await readList(page);
      expect(active.posinset, `${row}px page`).toBe(String(Math.floor(320 / row) + 1));
      expect(active.inView).toBe(true);
    }
  });

  test('renders as many options for 1,200 keys as for 50 (D5.perf-3)', async ({ page }) => {
    const rendered = {};
    for (const count of [50, 1200]) {
      await openWith(page, count);
      rendered[count] = await options(page).count();
    }
    expect(rendered[1200], JSON.stringify(rendered)).toBe(rendered[50]);
    expect(rendered[1200]).toBeLessThanOrEqual(40);
  });

  test('each option says its place among all of them (D12.a11y-1)', async ({ page }) => {
    await openWith(page, 1200);
    const { options: shown, active } = await readList(page);
    expect(shown.length).toBeGreaterThan(0);
    for (const option of shown) {
      expect(option.setsize, option.id).toBe('1200');
      const index = Number(option.id.split('-').pop());
      expect(option.posinset, option.id).toBe(String(index + 1));
      expect(option.label).toBe(`k${String(index + 1).padStart(4, '0')}`);
    }
    expect(active).toEqual({ selected: 'true', posinset: '1', label: 'k0001', inView: true });
  });

  test('the highlighted option is in the page and in view however far it moves (D12.a11y-1)', async ({ page }) => {
    await openWith(page, 1200);
    // Paging down far: the list scrolls with it.
    for (let step = 0; step < 30; step++) await page.keyboard.press('PageDown');
    let { active } = await readList(page);
    expect(active).toEqual({ selected: 'true', posinset: '211', label: 'k0211', inView: true });
    // Up from the first wraps to the last.
    for (let step = 0; step < 31; step++) await page.keyboard.press('PageUp');
    await page.keyboard.press('ArrowUp');
    ({ active } = await readList(page));
    expect(active).toEqual({ selected: 'true', posinset: '1200', label: 'k1200', inView: true });
    // Scrolled back to the top with the pointer: the highlighted option
    // stays in the page for the screen reader, and the rows in view show.
    await list(page).evaluate((element) => {
      element.scrollTop = 0;
      element.dispatchEvent(new Event('scroll'));
    });
    await expect(page.locator('#listed .config-editor__completion[aria-posinset="1"]')).toBeVisible();
    ({ active } = await readList(page));
    expect(active).toMatchObject({ selected: 'true', posinset: '1200', label: 'k1200', inView: false });
    // Enter accepts it.
    await page.keyboard.press('Enter');
    await expect(page.locator('#listed .config-editor__input')).toHaveValue(/^k1200:/);
  });

  test('scrolling the list shows the options under it, at their places', async ({ page }) => {
    await openWith(page, 1200);
    const height = await list(page).evaluate((element) => element.scrollHeight);
    const row = await options(page).first().evaluate((element) => element.getBoundingClientRect().height);
    expect(height).toBeGreaterThanOrEqual(1200 * row);
    await list(page).evaluate((element) => {
      element.scrollTop = 600 * 44;
      element.dispatchEvent(new Event('scroll'));
    });
    const option = page.locator('#listed .config-editor__completion[aria-posinset="601"]');
    await expect(option).toBeVisible();
    const [box, at] = await Promise.all([list(page).boundingBox(), option.boundingBox()]);
    expect(Math.round(at.y - box.y)).toBeLessThanOrEqual(2);
    // A click on it accepts it.
    await option.click();
    await expect(page.locator('#listed .config-editor__input')).toHaveValue(/^k0601:/);
  });

  test('a filter that leaves nothing says so (D12.a11y-1)', async ({ page }) => {
    await openWith(page, 50);
    await page.keyboard.type('k00');
    await expect(options(page).first()).toBeVisible();
    await page.keyboard.type('zz');
    await expect(options(page)).toHaveCount(0);
    await expect(page.locator('#listed .config-editor__announcement')).toHaveText(/^No suggestions/);
    // Backspace back to a match brings the list back (D13.a11y-1).
    await page.keyboard.press('Backspace');
    await page.keyboard.press('Backspace');
    await expect(options(page).first()).toBeVisible();
  });
});
