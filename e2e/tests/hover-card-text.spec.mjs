import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// The hover card's text, in every language: a long word or web address in
// a description or a deprecation message wraps inside the card and the
// schema's line breaks show; the text can be selected and copied; a
// repeated block's item (a YAML dash, a JSON item's brace) has a card with
// the item's own docs; on a touch screen a problem's docs link names no
// key and is a finger's 24px tall. Through the npm package's pages (run
// `npm run build` in js/ first).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const pageUrl = (options) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

test.skip(!built, 'run `npm run build` in js/ first');
test.use({ viewport: { width: 800, height: 450 } });

const card = (page) => page.locator('.config-editor__hover');

async function open(page, options) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await page.goto(pageUrl(options));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.locator('.config-editor__input').waitFor();
  return errors;
}

// The middle of the first character of the `nth` drawn `word`.
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
      return { x: box.left + box.width / 2, y: box.top + box.height / 2 };
    },
    [word, nth],
  );

async function hover(page, word, nth = 0) {
  // Away first, until the card shown goes.
  await page.mouse.move(2, 2);
  await expect(card(page)).toBeHidden();
  const at = await wordAt(page, word, nth);
  await page.mouse.move(at.x, at.y);
  await expect(card(page)).toBeVisible();
}

// Each language's document with a `port` and a `mode` field.
const DOCS = {
  yaml: 'port: 3\nmode: 1\n',
  json: '{\n  "port": 3,\n  "mode": 1\n}\n',
  toml: 'port = 3\nmode = 1\n',
  hcl: 'port = 3\nmode = 1\n',
  xml: '<port>3</port>\n<mode>1</mode>\n',
};
const URL = `https://docs.example.com/configuration/${'segment'.repeat(12)}#listen-address-and-port`;
const DOCUMENTED = JSON.stringify({
  type: 'object',
  properties: {
    port: { type: 'integer', description: `See ${URL} for details.\n\n- item one\n- item two` },
    mode: { type: 'integer', deprecated: true, deprecationMessage: `Use ${URL} instead` },
  },
});

// What of the card's text sticks out of it (wider than the card).
const overflowing = (page) =>
  card(page).evaluate((element) => {
    const right = element.getBoundingClientRect().right;
    return [element, ...element.querySelectorAll('*')]
      .filter((part) => part.scrollWidth > part.clientWidth + 1 || part.getBoundingClientRect().right > right + 1)
      .map((part) => part.className || part.tagName);
  });

test.describe('U25-1: long words wrap in the card and line breaks show', () => {
  for (const [language, value] of Object.entries(DOCS)) {
    test(language, async ({ page }) => {
      const errors = await open(page, { value, language, schema: DOCUMENTED });
      await hover(page, 'port');
      expect(await overflowing(page)).toEqual([]);
      const description = card(page).locator('.config-editor__hover-description');
      await expect(description).toContainText(URL.slice(0, 30));
      expect(await description.innerText()).toContain('details.\n\n- item one\n- item two');
      await hover(page, 'mode');
      await expect(card(page).locator('.config-editor__hover-deprecation')).toContainText('instead');
      expect(await overflowing(page)).toEqual([]);
      expect(errors).toEqual([]);
    });
  }
});

test.describe('U25-9: the card text can be selected and copied', () => {
  for (const [language, value] of Object.entries(DOCS)) {
    test(language, async ({ page }) => {
      const schema = JSON.stringify({
        type: 'object',
        properties: { port: { type: 'integer', description: 'Selectable words of the docs' } },
      });
      const errors = await open(page, { value, language, schema });
      await hover(page, 'port');
      const description = card(page).locator('.config-editor__hover-description');
      const box = await description.boundingBox();
      // Into the card, then a double click on its first word.
      await page.mouse.move(box.x + 20, box.y + box.height / 2, { steps: 4 });
      await page.mouse.dblclick(box.x + 20, box.y + box.height / 2);
      await expect(card(page)).toBeVisible();
      expect(await page.evaluate(() => String(getSelection()).trim())).toBe('Selectable');
      // Ctrl+C copies the selection.
      await page.evaluate(() => {
        window.copied = '';
        document.addEventListener('copy', () => (window.copied = String(getSelection())), { once: true });
      });
      await page.keyboard.press('Control+C');
      expect((await page.evaluate(() => window.copied)).trim()).toBe('Selectable');
      // The text is unchanged.
      expect(await page.locator('.config-editor__input').inputValue()).toBe(value);
      expect(errors).toEqual([]);
    });
  }
});

test.describe('U25-7: a list item has a card with its own docs', () => {
  const schema = JSON.stringify({
    type: 'object',
    properties: {
      svc: {
        type: 'array',
        description: 'Services list',
        items: {
          type: 'object',
          title: 'Service',
          description: 'One service entry',
          properties: { name: { type: 'string', description: 'Service name' } },
        },
      },
    },
  });
  const ITEMS = {
    yaml: ['svc:\n  - name: a\n  - name: b\n', '-', [0, 1]],
    json: ['{\n  "svc": [\n    { "name": "a" },\n    { "name": "b" }\n  ]\n}\n', '{ "', [0, 1]],
    toml: ['[[svc]]\nname = "a"\n\n[[svc]]\nname = "b"\n', 'svc', [0, 1]],
    hcl: ['svc {\n  name = "a"\n}\nsvc {\n  name = "b"\n}\n', 'svc', [0, 1]],
    // (`svc` of each opening tag, not of the closing ones.)
    xml: ['<svc>\n  <name>a</name>\n</svc>\n<svc>\n  <name>b</name>\n</svc>\n', 'svc', [0, 2]],
  };
  for (const [language, [value, marker, nths]] of Object.entries(ITEMS)) {
    test(language, async ({ page }) => {
      const errors = await open(page, { value, language, schema });
      for (const nth of nths) {
        await hover(page, marker, nth);
        await expect(card(page).locator('.config-editor__hover-key')).toHaveText('svc');
        await expect(card(page)).toContainText('Services list');
        await expect(card(page).locator('.config-editor__hover-item')).toContainText('Service');
        await expect(card(page).locator('.config-editor__hover-item')).toContainText('One service entry');
      }
      expect(errors).toEqual([]);
    });
  }
});

test.describe('U25-10: a problem docs link on a touch screen', () => {
  const schema = JSON.stringify({ type: 'object', properties: { port: { type: 'integer' } } });
  const problem = (value) => {
    const line = value.split('\n').findIndex((text) => text.includes('port')) + 1;
    const column = value.split('\n')[line - 1].indexOf('port') + 1;
    return [
      {
        range: { startLine: line, startColumn: column, endColumn: column + 4 },
        severity: 'warning',
        message: 'Port is checked by the host',
        docsUrl: 'https://example.com/port',
      },
    ];
  };
  for (const touch of [true, false]) {
    test.describe(touch ? 'touch' : 'mouse', () => {
      test.use(touch ? { hasTouch: true, isMobile: true, viewport: { width: 390, height: 600 } } : {});
      for (const [language, value] of Object.entries(DOCS)) {
        test(language, async ({ page }) => {
          const errors = await open(page, { value, language, schema, hostDiagnostics: problem(value) });
          const at = await wordAt(page, 'port');
          if (touch) await page.touchscreen.tap(at.x, at.y);
          else await page.mouse.move(at.x, at.y);
          const link = page.locator('.config-editor__problem-tip-link').filter({ visible: true });
          await expect(link).toHaveCount(1);
          if (touch) await expect(link).toHaveText('Open docs ↗');
          else await expect(link).toHaveText('Open docs ↗ (Alt+Enter)');
          expect((await link.boundingBox()).height).toBeGreaterThanOrEqual(24);
          expect(errors).toEqual([]);
        });
      }
    });
  }
});
