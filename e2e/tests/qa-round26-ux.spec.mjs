import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 26, UX: the hover card and the problem tip take the pointer
// (their text can be selected and copied) without taking the lines they
// cover. As in VS Code, a card goes above its line when the window has
// room for it there; one below its line lets a pointer that came onto it
// another way than from its key through to the text (docs, clicks); a
// wheel over a popup that cannot scroll that way scrolls the text, or the
// page at the text's ends, and closes it; Escape with the pointer on it
// and the focus nowhere closes it. In every language, at 900 × 500,
// through the npm package's pages (run `npm run build` in js/ first).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const pageUrl = (options) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

test.skip(!built, 'run `npm run build` in js/ first');
test.use({ viewport: { width: 900, height: 500 } });

const card = (page) => page.locator('.config-editor__hover');
const input = (page) => page.locator('.config-editor__input');

async function open(page, options) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await page.goto(pageUrl(options));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await input(page).waitFor();
  return errors;
}

// The middle of the first character of the `nth` drawn `word`, and its
// line's top and bottom.
const wordAt = (page, word, nth = 0) =>
  page.evaluate(
    ([word, nth]) => {
      const rows = [...document.querySelectorAll('.config-editor__highlight .config-editor__code-line')];
      const found = [];
      for (const row of rows) {
        const text = row.textContent;
        for (let at = text.indexOf(word); at >= 0; at = text.indexOf(word, at + 1)) found.push([row, at]);
      }
      const [row, at] = found[nth];
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
    [word, nth],
  );

// The caret's line, and whether the text has the focus.
const caret = (page) =>
  input(page).evaluate((element) => {
    const before = element.value.slice(0, element.selectionStart).split('\n');
    return { line: before.length, focused: document.activeElement === element };
  });

const scrolled = (page) =>
  page.evaluate(() => ({ text: document.querySelector('.config-editor__input').scrollTop, page: scrollY }));

// Each language's document: `port` on line 1, `host` on line 2, then
// `more` lines of `key_number_<i>`.
const DOCS = {
  yaml: (more) => `port: 3\nhost: x\n${keys(more, (i) => `key_number_${i}: ${i}`)}`,
  json: (more) =>
    `{ "port": 3,\n  "host": "x",\n${keys(more, (i) => `  "key_number_${i}": ${i},`)}  "end": 0 }\n`,
  toml: (more) => `port = 3\nhost = "x"\n${keys(more, (i) => `key_number_${i} = ${i}`)}`,
  hcl: (more) => `port = 3\nhost = "x"\n${keys(more, (i) => `key_number_${i} = ${i}`)}`,
  xml: (more) => `<port>3</port>\n<host>x</host>\n${keys(more, (i) => `<key_number_${i}>${i}</key_number_${i}>`)}`,
};
const keys = (count, line) => Array.from({ length: count }, (_, i) => `${line(i)}\n`).join('');

const schemaWith = (portDescription) =>
  JSON.stringify({
    type: 'object',
    properties: {
      port: { type: 'integer', description: portDescription },
      host: { type: 'string', description: 'Host docs' },
      key_number_3: { type: 'integer', description: 'Docs for three' },
    },
  });
// Too tall to fit above line 1: the card goes below and covers lines.
const TALL = Array.from({ length: 12 }, (_, i) => `Port docs, line ${i + 1}`).join('\n');

async function hover(page, word, nth = 0) {
  await page.mouse.move(2, 498);
  await expect(card(page)).toBeHidden();
  const at = await wordAt(page, word, nth);
  await page.mouse.move(at.x, at.y);
  await expect(card(page)).toBeVisible();
  return at;
}

for (const [language, doc] of Object.entries(DOCS)) {
  test.describe(language, () => {
    test('M26-U1: a card that fits above its line goes there, leaving the next line to the pointer', async ({ page }) => {
      const errors = await open(page, { value: doc(4), language, schema: schemaWith('Port docs') });
      const port = await hover(page, 'port');
      const box = await card(page).boundingBox();
      expect(box.y + box.height).toBeLessThanOrEqual(port.top);
      // Straight down onto `host`: its docs, and a click there places the
      // caret in the focused text.
      const host = await wordAt(page, 'host');
      await page.mouse.move(host.x, host.y, { steps: 5 });
      await expect(card(page)).toContainText('Host docs');
      await page.mouse.click(host.x, host.y);
      expect(await caret(page)).toMatchObject({ line: 2, focused: true });
      expect(errors).toEqual([]);
    });

    test('M26-U1: a card below its line lets a pointer from elsewhere through to the lines it covers', async ({ page }) => {
      const errors = await open(page, { value: doc(30), language, schema: schemaWith(TALL) });
      const port = await hover(page, 'port');
      const box = await card(page).boundingBox();
      expect(box.y).toBeGreaterThan(port.bottom);
      // Up off the text, over to beside the card, down to line 2, then
      // onto `host` under the card (still the port's): the text answers,
      // not the card.
      const host = await wordAt(page, 'host');
      const beside = box.x + box.width + 30;
      const around = async (y) => {
        await page.mouse.move(port.x, 3);
        await page.mouse.move(beside, 3);
        await page.mouse.move(beside, y, { steps: 2 });
        await expect(card(page)).toContainText('Port docs');
      };
      await around(host.y);
      await page.mouse.move(host.x, host.y, { steps: 4 });
      await expect(card(page)).toContainText('Host docs');
      await expect(card(page)).not.toContainText('Port docs');
      await page.mouse.click(host.x, host.y);
      expect(await caret(page)).toMatchObject({ line: 2, focused: true });
      // A wheel there scrolls the text.
      await hover(page, 'port');
      await around(host.y + 40);
      await page.mouse.move(host.x + 20, host.y + 40, { steps: 4 });
      await page.mouse.wheel(0, 200);
      await expect.poll(async () => (await scrolled(page)).text).toBeGreaterThan(0);
      await expect(card(page)).toBeHidden();
      expect(errors).toEqual([]);
    });

    test('M26-U1: the wheel over a card that cannot scroll scrolls the text and closes it', async ({ page }) => {
      const errors = await open(page, { value: doc(300), language, schema: schemaWith('Port docs') });
      await hover(page, 'key_number_3');
      const box = await card(page).boundingBox();
      await page.mouse.move(box.x + 20, box.y + box.height / 2, { steps: 3 });
      await page.mouse.wheel(0, 300);
      await expect.poll(async () => (await scrolled(page)).text).toBeGreaterThan(0);
      await expect(card(page)).toBeHidden();
      expect(errors).toEqual([]);
    });

    test('M26-U1: the wheel over a card at the text’s end scrolls the page and closes it', async ({ page }) => {
      const errors = await open(page, { value: doc(0), language, schema: schemaWith('Port docs') });
      await page.addStyleTag({ content: 'body { padding-bottom: 2000px; }' });
      await hover(page, 'port');
      const box = await card(page).boundingBox();
      await page.mouse.move(box.x + 30, box.y + box.height / 2, { steps: 3 });
      await page.mouse.wheel(0, 150);
      await expect.poll(async () => (await scrolled(page)).page).toBe(150);
      await expect(card(page)).toBeHidden();
      expect(errors).toEqual([]);
    });

    test('M26-U1: Escape with the pointer on the card and nothing focused closes it', async ({ page }) => {
      const errors = await open(page, { value: doc(4), language, schema: schemaWith('Port docs') });
      await hover(page, 'key_number_3');
      const box = await card(page).boundingBox();
      await page.mouse.move(box.x + 20, box.y + box.height / 2, { steps: 3 });
      expect(await page.evaluate(() => document.activeElement === document.body)).toBe(true);
      await page.keyboard.press('Escape');
      await expect(card(page)).toBeHidden();
      expect(errors).toEqual([]);
    });

    test('M26-U1: a problem tip below its line lets a pointer from elsewhere through, and the wheel scrolls the text', async ({ page }) => {
      const problems = Array.from({ length: 14 }, (_, i) => ({
        range: { startLine: 1, startColumn: 1, endColumn: 30 },
        severity: 'warning',
        message: `Checked by the host, number ${i + 1}`,
        docsUrl: 'https://example.com/port',
      }));
      const errors = await open(page, { value: doc(30), language, hostDiagnostics: problems });
      const port = await wordAt(page, 'port');
      await page.mouse.move(port.x, port.y);
      const tip = page.locator('.config-editor__problem-tip');
      await expect(tip).toBeVisible();
      const box = await tip.boundingBox();
      expect(box.y).toBeGreaterThan(port.bottom);
      // (Line 1's problem reaches past its end: the whole line is what
      // the tip is for. Far enough past the tip's side that going down
      // does not head for it.)
      const host = await wordAt(page, 'host');
      const beside = box.x + box.width + 200;
      await page.mouse.move(beside, port.y, { steps: 4 });
      await page.mouse.move(beside, host.y, { steps: 2 });
      await page.mouse.move(host.x, host.y, { steps: 4 });
      await page.mouse.click(host.x, host.y);
      expect(await caret(page)).toMatchObject({ line: 2, focused: true });
      await page.mouse.move(port.x, port.y);
      await expect(tip).toBeVisible();
      const again = await tip.boundingBox();
      await page.mouse.move(again.x + 20, again.y + 10, { steps: 3 });
      // The tip scrolls first, to its end; then the text.
      await page.mouse.wheel(0, 5000);
      await expect.poll(() => tip.evaluate((tip) => tip.scrollTop + tip.clientHeight >= tip.scrollHeight - 1)).toBe(true);
      await page.mouse.wheel(0, 300);
      await expect.poll(async () => (await scrolled(page)).text).toBeGreaterThan(0);
      expect(errors).toEqual([]);
    });

    test('L26-U1: back on the key within the moment, the card stays', async ({ page }) => {
      const errors = await open(page, { value: doc(4), language, schema: schemaWith('Port docs') });
      const port = await hover(page, 'key_number_3');
      // Off the text (onto the page above it) and back at once.
      for (const away of [{ x: 600, y: port.y }, { x: port.x, y: 5 }]) {
        await page.mouse.move(away.x, away.y);
        await page.waitForTimeout(150);
        await page.mouse.move(port.x, port.y);
        await page.waitForTimeout(1000);
        await expect(card(page)).toBeVisible();
      }
      expect(errors).toEqual([]);
    });

    test('L26-U6: Ctrl+A in the focused card selects its text', async ({ page }) => {
      const errors = await open(page, { value: doc(4), language, schema: schemaWith('Port docs and more words') });
      await hover(page, 'port');
      const description = card(page).locator('.config-editor__hover-description');
      const box = await description.boundingBox();
      await page.mouse.move(box.x + 5, box.y + box.height / 2, { steps: 3 });
      await page.mouse.click(box.x + 5, box.y + box.height / 2);
      await page.keyboard.press('ControlOrMeta+A');
      const selected = await page.evaluate(() => String(getSelection()));
      expect(selected).toContain('Port docs and more words');
      expect(selected).not.toContain('config-editor');
      await expect(card(page)).toBeVisible();
      expect(errors).toEqual([]);
    });
  });
}

// L26-U4: at phone width (a touch screen's wider tab buttons) tabs sharing
// a file's name in different folders read alike ("e…limits.toml"): each
// keeps the letters that tell it apart whole, then an ellipsis, then the
// whole file's name.
for (const [width, touch] of [[360, true], [600, false], [1200, false]]) {
  test.describe(`L26-U4 at ${width} px`, () => {
    test.use({ viewport: { width, height: 400 }, hasTouch: touch, isMobile: touch });
    test('tabs sharing a file name never read alike', async ({ page }) => {
      await page.goto(pathToFileURL(join(js, 'examples', 'documents.html')).href);
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await page.evaluate(() =>
        window.documentsView.update({
          documents: [
            { id: 'a', fileName: 'clusters/eu-west-1/production/limits.toml', value: 'a = 1\n', closable: false },
            { id: 'b', fileName: 'clusters/us-east-1/production/limits.toml', value: 'a = 1\n', readOnly: true },
            { id: 'c', fileName: 'prod/limits.toml', value: 'a = 1\n' },
            { id: 'd', fileName: 'staging/eu/limits.toml', value: 'a = 1\n' },
          ],
        }),
      );
      await expect(page.locator('[role="tab"]')).toHaveCount(4);
      const read = await page.evaluate(() =>
        [...document.querySelectorAll('[role="tab"]')].map((tab) => {
          const name = tab.querySelector('.config-editor-documents__name').getBoundingClientRect();
          const inside = (part) => {
            if (!part) return true;
            const box = part.getBoundingClientRect();
            return box.left >= name.left - 0.5 && box.right <= name.right + 0.5;
          };
          const kept = tab.querySelector('.config-editor-documents__name-kept');
          const start = tab.querySelector('.config-editor-documents__name-start');
          const end = tab.querySelector('.config-editor-documents__name-end');
          const cut = start.scrollWidth > start.clientWidth + 0.5;
          return {
            reads: `${kept?.textContent ?? ''}${cut ? '…' : start.textContent}${end.textContent}`,
            whole: inside(kept) && inside(end),
          };
        }),
      );
      expect(read.every(({ whole }) => whole)).toBe(true);
      expect(new Set(read.map(({ reads }) => reads)).size).toBe(4);
    });
  });
}

// L26-U3: the notice that the text changed outside the editor said how to
// reach its choices with F6 on a touch screen too, where they are tapped
// (as the footer's hint says "tap" there).
for (const touch of [true, false]) {
  test.describe(`L26-U3 ${touch ? 'touch' : 'mouse'}`, () => {
    test.use(touch ? { viewport: { width: 360, height: 700 }, hasTouch: true, isMobile: true } : {});
    test('the changed-outside notice names F6 only with a keyboard', async ({ page }) => {
      const errors = await open(page, { value: 'listen: 7835\n' });
      await input(page).focus();
      await page.keyboard.press('Control+End');
      await page.keyboard.type('a: 1\n');
      await page.locator('#external-change').click();
      const notice = page.locator('.config-editor__notice');
      await expect(notice).toBeVisible();
      if (touch) await expect(notice).not.toContainText('F6');
      else await expect(notice).toContainText('F6 moves to the choices and back.');
      expect(errors).toEqual([]);
    });
  });
}

// L26-U2: a JSON item's `{` showed the item's docs on its left half only:
// coming back onto it at once (another part of it) hid the card. Every
// part of each language's item marker shows them, however reached.
test.describe('L26-U2: the whole item marker shows the item docs', () => {
  const schema = JSON.stringify({
    type: 'object',
    properties: {
      svc: {
        type: 'array',
        description: 'Services list',
        items: { type: 'object', title: 'Service', description: 'One service entry', properties: { name: { type: 'string' } } },
      },
    },
  });
  const ITEMS = {
    yaml: ['svc:\n  - name: a\n  - name: b\n', '-'],
    json: ['{\n  "svc": [\n    { "name": "a" },\n    { "name": "b" }\n  ]\n}\n', '{ "'],
    toml: ['[[svc]]\nname = "a"\n\n[[svc]]\nname = "b"\n', 'svc'],
    hcl: ['svc {\n  name = "a"\n}\nsvc {\n  name = "b"\n}\n', 'svc'],
    xml: ['<svc>\n  <name>a</name>\n</svc>\n<svc>\n  <name>b</name>\n</svc>\n', 'svc'],
  };
  for (const [language, [value, marker]] of Object.entries(ITEMS)) {
    test(language, async ({ page }) => {
      const errors = await open(page, { value, language, schema });
      const at = await wordAt(page, marker);
      const width = await page.evaluate(() => {
        const row = document.querySelector('.config-editor__highlight .config-editor__code-line');
        const range = document.createRange();
        range.selectNodeContents(row);
        return range.getBoundingClientRect().width / Math.max(1, row.textContent.length);
      });
      for (const part of [-0.35, 0.35, -0.35, 0.35]) {
        // Away (off the text), and back at once onto another part of it.
        await page.mouse.move(2, 498);
        await page.waitForTimeout(150);
        await page.mouse.move(at.x + part * width, at.y);
        await page.waitForTimeout(900);
        await expect(card(page).locator('.config-editor__hover-item')).toContainText('One service entry');
      }
      expect(errors).toEqual([]);
    });
  }
});
