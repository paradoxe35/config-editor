import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 27, UX: the hover card and the problem tip are reached the way
// a mouse goes there, above their line or below it: many small moves at
// 60 to 125 events a second, across the few pixels between the popup and
// its line (which belong to the next line) and, from a long value's end,
// slanting across the popup's side. Then its docs link opens, its text is
// selected, a long one scrolls. In every language, at 900 × 500, through
// the npm package's pages (run `npm run build` in js/ first).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const pageUrl = (options, name = 'vanilla') =>
  `${pathToFileURL(join(js, 'examples', `${name}.html`)).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

test.skip(!built, 'run `npm run build` in js/ first');
test.use({ viewport: { width: 900, height: 500 } });

const card = (page) => page.locator('.config-editor__hover');
const tip = (page) => page.locator('.config-editor__problem-tip');
const input = (page) => page.locator('.config-editor__input');

async function open(page, options) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await page.goto(pageUrl(options));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await input(page).waitFor();
  // The editor at the page's top: a card on its first line has no room
  // above.
  await page.addStyleTag({ content: 'body > *:not(:has(.config-editor)) { display: none !important; } body { margin: 0; }' });
  return errors;
}

// The middle of character `offset` of the first drawn `word` (its last
// with a negative one), and its line's top and bottom.
const wordAt = (page, word, offset = 0) =>
  page.evaluate(
    ([word, offset]) => {
      const rows = [...document.querySelectorAll('.config-editor__highlight .config-editor__code-line')];
      const row = rows.find((row) => row.textContent.includes(word));
      const at = row.textContent.indexOf(word) + (offset < 0 ? word.length + offset : offset);
      const walker = document.createTreeWalker(row, NodeFilter.SHOW_TEXT);
      let left = at;
      let node;
      while ((node = walker.nextNode())) {
        if (left < node.length) break;
        left -= node.length;
      }
      const range = document.createRange();
      range.setStart(node, left);
      range.setEnd(node, left + 1);
      const box = range.getBoundingClientRect();
      return { x: box.left + box.width / 2, y: box.top + box.height / 2, top: box.top, bottom: box.bottom };
    },
    [word, offset],
  );

const middle = async (locator) => {
  const box = await locator.boundingBox();
  return { x: box.x + box.width / 2, y: box.y + box.height / 2, box };
};

// A mouse's way from `from` to `to`: `steps` moves 8 to 16 ms apart (60
// to 125 events a second), as a hand moves it.
async function glide(page, from, to, steps) {
  for (let step = 1; step <= steps; step++) {
    const at = step / steps;
    await page.mouse.move(from.x + (to.x - from.x) * at, from.y + (to.y - from.y) * at);
    await page.waitForTimeout(8 + (step % 3) * 4);
  }
}

// Each language's document: `lines` (key, value) pairs, one a line.
const LANGUAGES = {
  yaml: (lines) => lines.map(([key, value]) => `${key}: ${value}\n`).join(''),
  json: (lines) => `{ ${lines.map(([key, value]) => `"${key}": ${value}`).join(',\n  ')} }\n`,
  toml: (lines) => lines.map(([key, value]) => `${key} = ${value}\n`).join(''),
  hcl: (lines) => lines.map(([key, value]) => `${key} = ${value}\n`).join(''),
  xml: (lines) => lines.map(([key, value]) => `<${key}>${String(value).replace(/"/g, '')}</${key}>\n`).join(''),
};
const fill = (count) => Array.from({ length: count }, (_, i) => [`fill_${i}`, i]);
const LONG = `"${'long value '.repeat(6)}end"`;

const DOCS_URL = 'https://example.com/port';
const schemaWith = (description) =>
  JSON.stringify({
    type: 'object',
    properties: {
      port: { type: 'integer', description, 'x-docs-url': DOCS_URL },
      host: { type: 'string', description: 'Host docs' },
    },
  });
const WORDS = 'Port docs, with enough words to select some of them';
// Too tall for the window on either side: it scrolls.
const TALL = Array.from({ length: 40 }, (_, i) => `Port docs, line ${i + 1}`).join('\n');

// Away from the text, then onto `at`: the popup shows.
async function hoverAt(page, at, popup) {
  await page.mouse.move(890, 495);
  await expect(popup).toBeHidden();
  await page.mouse.move(at.x, at.y);
  await expect(popup).toBeVisible();
}

const SIDES = {
  // `port` on line 8: the card fits above it.
  above: (doc, value = 3) => doc([...fill(7), ['port', value], ['host', '"x"']]),
  // `port` on line 1: no room above, the card goes below.
  below: (doc, value = 3) => doc([['port', value], ['host', '"x"'], ...fill(7)]),
};

const STEPS = [15, 25, 40];

for (const [language, doc] of Object.entries(LANGUAGES)) {
  test.describe(language, () => {
    for (const [side, value] of Object.entries(SIDES)) {
      test(`M27-U1: a mouse reaches a card ${side} its line and opens its docs link`, async ({ page, context }) => {
        const errors = await open(page, { value: value(doc), language, schema: schemaWith(WORDS) });
        const port = await wordAt(page, 'port', 1);
        for (const steps of STEPS) {
          await hoverAt(page, port, card(page));
          const box = await card(page).boundingBox();
          if (side === 'above') expect(box.y + box.height).toBeLessThanOrEqual(port.top);
          else expect(box.y).toBeGreaterThanOrEqual(port.bottom);
          const link = await middle(card(page).locator('.config-editor__hover-link'));
          await glide(page, port, link, steps);
          await page.waitForTimeout(500);
          await expect(card(page), `${steps} steps`).toBeVisible();
          const opened = context.waitForEvent('page');
          await page.mouse.click(link.x, link.y);
          const docs = await opened;
          expect(docs.url()).toContain('example.com/port');
          await docs.close();
        }
        expect(errors).toEqual([]);
      });

      test(`M27-U1: a mouse reaches a card ${side} its line and selects its text`, async ({ page }) => {
        const errors = await open(page, { value: value(doc), language, schema: schemaWith(WORDS) });
        const port = await wordAt(page, 'port', 1);
        await hoverAt(page, port, card(page));
        const text = await middle(card(page).locator('.config-editor__hover-description'));
        // Along its first line.
        const from = { x: text.box.x + 2, y: text.box.y + 6 };
        await glide(page, port, from, 25);
        await page.mouse.down();
        await glide(page, from, { x: text.box.x + text.box.width - 2, y: from.y }, 20);
        await page.mouse.up();
        expect(await page.evaluate(() => String(getSelection()))).toContain('enough words');
        await expect(card(page)).toBeVisible();
        expect(errors).toEqual([]);
      });

      test(`M27-U1: a mouse reaches a long card ${side} its line and scrolls it`, async ({ page }) => {
        // Line 1 for below; for above, the last line of a short window's
        // text, with more room above than below.
        const lines = side === 'above' ? [...fill(14), ['port', 3]] : [['port', 3], ...fill(3)];
        await page.setViewportSize({ width: 900, height: 460 });
        const errors = await open(page, { value: doc(lines), language, schema: schemaWith(TALL) });
        const port = await wordAt(page, 'port', 1);
        await hoverAt(page, port, card(page));
        const box = await card(page).boundingBox();
        if (side === 'above') expect(box.y + box.height).toBeLessThanOrEqual(port.top);
        else expect(box.y).toBeGreaterThanOrEqual(port.bottom);
        expect(await card(page).evaluate((card) => card.scrollHeight > card.clientHeight)).toBe(true);
        const inside = { x: box.x + 40, y: side === 'above' ? box.y + box.height - 30 : box.y + 30 };
        await glide(page, port, inside, 30);
        await page.mouse.wheel(0, 120);
        await expect.poll(() => card(page).evaluate((card) => card.scrollTop)).toBeGreaterThan(0);
        await expect(card(page)).toBeVisible();
        expect(errors).toEqual([]);
      });

      test(`M27-U1: a mouse reaches a card ${side} a long value from the value's end`, async ({ page, context }) => {
        const errors = await open(page, { value: value(doc, LONG), language, schema: schemaWith(WORDS) });
        const end = await wordAt(page, 'end', 1);
        for (const steps of STEPS) {
          await hoverAt(page, end, card(page));
          const link = await middle(card(page).locator('.config-editor__hover-link'));
          expect(link.box.x + link.box.width).toBeLessThan(end.x);
          await glide(page, end, link, steps);
          await page.waitForTimeout(500);
          await expect(card(page), `${steps} steps`).toContainText('not of type');
          const opened = context.waitForEvent('page');
          await page.mouse.click(link.x, link.y);
          const docs = await opened;
          expect(docs.url()).toContain('example.com/port');
          await docs.close();
        }
        expect(errors).toEqual([]);
      });

      test(`M27-U1: a mouse reaches a problem tip ${side} a long underline from its end`, async ({ page, context }) => {
        const text = value(doc, LONG);
        const line = text.split('\n').findIndex((line) => line.includes('port')) + 1;
        const problem = {
          range: { startLine: line, startColumn: 1, endColumn: text.split('\n')[line - 1].length + 1 },
          severity: 'warning',
          message: 'Checked by the host',
          docsUrl: DOCS_URL,
        };
        const errors = await open(page, { value: text, language, hostDiagnostics: [problem] });
        for (const from of [await wordAt(page, 'end', 1), await wordAt(page, 'port', 1)]) {
          for (const steps of STEPS) {
            await hoverAt(page, from, tip(page));
            const box = await tip(page).boundingBox();
            if (side === 'above') expect(box.y + box.height).toBeLessThanOrEqual(from.top);
            else expect(box.y).toBeGreaterThanOrEqual(from.bottom);
            const link = await middle(tip(page).locator('.config-editor__problem-tip-link'));
            await glide(page, from, link, steps);
            await page.waitForTimeout(500);
            await expect(tip(page), `${steps} steps`).toBeVisible();
            const opened = context.waitForEvent('page');
            await page.mouse.click(link.x, link.y);
            const docs = await opened;
            expect(docs.url()).toContain('example.com/port');
            await docs.close();
          }
        }
        expect(errors).toEqual([]);
      });
    }
  });
}

// Whether the card shows all the time for `ms` (checked every frame).
const staysFor = (page, ms) =>
  page.evaluate(
    (ms) =>
      new Promise((resolve) => {
        const end = performance.now() + ms;
        const check = () => {
          if (!document.querySelector('.config-editor__hover')) return resolve(false);
          if (performance.now() > end) return resolve(true);
          requestAnimationFrame(check);
        };
        check();
      }),
    ms,
  );

// L27-U2: from the card back onto its key, the card closed and opened
// again a moment later; back on the very pixel the pointer left the key
// at, it closed for good.
for (const [language, doc] of Object.entries(LANGUAGES)) {
  test(`L27-U2 ${language}: back from the card onto its key, the card stays`, async ({ page }) => {
    const errors = await open(page, { value: SIDES.above(doc), language, schema: schemaWith(WORDS) });
    const port = await wordAt(page, 'port', 1);
    for (const back of [(at) => glide(page, at, port, 10), () => page.mouse.move(port.x, port.y)]) {
      await hoverAt(page, port, card(page));
      const text = await middle(card(page).locator('.config-editor__hover-description'));
      await glide(page, port, text, 20);
      await page.waitForTimeout(300);
      await back(text);
      expect(await staysFor(page, 1200)).toBe(true);
    }
    expect(errors).toEqual([]);
  });
}

// L27-U2: from a tab the card covers, back onto its key at once, the card
// was gone for a second and more.
test('L27-U2: from a tab under the card back onto its key, the card stays', async ({ page }) => {
  await page.goto(pathToFileURL(join(js, 'examples', 'documents.html')).href);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  const schema = JSON.stringify({ type: 'object', properties: { listen: { type: 'integer', description: `${WORDS}\n${WORDS}` } } });
  await page.evaluate((schema) => {
    window.documentsView.update({
      documents: [
        { id: 'a', fileName: 'tunnel.yaml', value: 'a: 1\nb: 2\nlisten: 7835\n', schema },
        { id: 'b', fileName: 'other.yaml', value: 'c: 3\n' },
      ],
    });
  }, schema);
  const listen = await wordAt(page, 'listen', 1);
  await hoverAt(page, listen, card(page));
  const box = await card(page).boundingBox();
  const tab = await middle(page.locator('[role="tab"]').nth(1));
  expect(box.y).toBeLessThan(tab.y);
  expect(box.x + box.width).toBeGreaterThan(tab.x);
  for (const wait of [100, 300]) {
    await glide(page, listen, tab, 4);
    await page.waitForTimeout(wait);
    await glide(page, tab, listen, 4);
    expect(await staysFor(page, 1200), `after ${wait} ms`).toBe(true);
  }
});

// L27-U3: at phone width a short folder before a shared file's name
// (prod/limits.toml) was cut by a pixel or four (the touch screen's
// wider close button took its room), and a tab that cannot be closed
// kept the close button's room empty.
for (const [width, touch] of [[360, true], [600, false], [1200, false]]) {
  test.describe(`L27-U3 at ${width} px`, () => {
    test.use({ viewport: { width, height: 400 }, hasTouch: touch, isMobile: touch });
    test('a short folder shows whole, and a tab that cannot close keeps no empty room', async ({ page }) => {
      await page.goto(pathToFileURL(join(js, 'examples', 'documents.html')).href);
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await page.evaluate(() =>
        window.documentsView.update({
          documents: [
            { id: 'a', fileName: 'clusters/eu-west-1/production/limits.toml', value: 'a = 1\n', closable: false },
            { id: 'b', fileName: 'clusters/us-east-1/production/limits.toml', value: 'a = 1\n' },
            { id: 'c', fileName: 'prod/limits.toml', value: 'a = 1\n' },
            { id: 'd', fileName: 'x.yaml', value: 'a: 1\n', closable: false },
            { id: 'e', fileName: 'y.yaml', value: 'a: 1\n', baseline: 'a: 2\n', closable: false },
          ],
        }),
      );
      await expect(page.locator('[role="tab"]')).toHaveCount(5);
      const tabs = () =>
        page.evaluate(() =>
          [...document.querySelectorAll('[role="tab"]')].map((tab) => {
            const name = tab.querySelector('.config-editor-documents__name');
            const start = name.querySelector('.config-editor-documents__name-start');
            return {
              cut: start.scrollWidth > start.clientWidth + 0.5 || name.scrollWidth > name.clientWidth + 0.5,
              room: tab.getBoundingClientRect().right - name.getBoundingClientRect().right,
            };
          }),
        );
      const [eu, , prod, x] = await tabs();
      expect(prod.cut).toBe(false);
      // The long folders still give way.
      expect(eu.cut).toBe(true);
      // Only the tab's own padding after the name.
      expect(x.room).toBeLessThanOrEqual(8);
      // Unsaved, it shows its dot.
      await expect(page.locator('[role="tab"]').nth(4).locator('.config-editor-documents__dirty-dot')).toBeVisible();
    });
  });
}
