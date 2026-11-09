import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 28, UX: a hover card that still shows keeps a pointer that gets
// onto it after a detour (into the indentation or the gutter, past its
// key's end, onto the next line), while the lines it covers stay the
// text's for a pointer that comes onto them from beside it. Mice move as
// hands do: small steps 8 to 16 ms apart. In every language, at 900 × 600,
// through the npm package's pages (run `npm run build` in js/ first).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const pageUrl = (options) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

test.skip(!built, 'run `npm run build` in js/ first');
test.use({ viewport: { width: 900, height: 600 } });

const card = (page) => page.locator('.config-editor__hover');
const input = (page) => page.locator('.config-editor__input');

async function open(page, options) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await page.goto(pageUrl(options));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await input(page).waitFor();
  // The editor at the page's top: a card on its first lines has no room
  // above.
  await page.addStyleTag({ content: 'body > *:not(:has(.config-editor)) { display: none !important; } body { margin: 0; }' });
  return errors;
}

// The box of the first drawn `word` (in quotes or not), and its line's
// middle.
const wordBox = (page, word) =>
  page.evaluate((word) => {
    const rows = [...document.querySelectorAll('.config-editor__highlight .config-editor__code-line')];
    const row = rows.find((row) => new RegExp(`\\b${word}\\b`).test(row.textContent));
    const at = row.textContent.search(new RegExp(`\\b${word}\\b`));
    const walker = document.createTreeWalker(row, NodeFilter.SHOW_TEXT);
    let left = at;
    let node;
    while ((node = walker.nextNode())) {
      if (left < node.length) break;
      left -= node.length;
    }
    const range = document.createRange();
    range.setStart(node, left);
    range.setEnd(node, left + word.length);
    const box = range.getBoundingClientRect();
    const text = document.querySelector('.config-editor__input').getBoundingClientRect();
    return { left: box.left, right: box.right, y: (box.top + box.bottom) / 2, top: box.top, bottom: box.bottom, textLeft: text.left };
  }, word);

// A mouse's way through `points`: steps of at most 8 px, 8 to 16 ms apart
// (60 to 125 events a second).
async function glide(page, points) {
  for (let i = 1; i < points.length; i++) {
    const [from, to] = [points[i - 1], points[i]];
    const steps = Math.max(1, Math.round(Math.hypot(to.x - from.x, to.y - from.y) / 8));
    for (let step = 1; step <= steps; step++) {
      await page.mouse.move(from.x + ((to.x - from.x) * step) / steps, from.y + ((to.y - from.y) * step) / steps);
      await page.waitForTimeout(8 + (step % 3) * 4);
    }
  }
}

// What is under the pointer at `at`: the card, else the text.
const under = (page, at) =>
  page.evaluate(({ x, y }) => (document.elementFromPoint(x, y)?.closest('.config-editor__hover') ? 'card' : 'text'), at);

// Each language's document: `lines` (key, value) pairs in a `server`
// block, indented; an empty pair is a blank line.
const blankOr = (line) => (pair) => (pair.length ? line(...pair) : '');
const LANGUAGES = {
  yaml: (lines) => `server:\n${lines.map(blankOr((key, value) => `  ${key}: ${value}`)).join('\n')}\n`,
  json: (lines) => {
    const last = lines.findLastIndex((pair) => pair.length);
    const line = (pair, i) => (pair.length ? `    "${pair[0]}": ${pair[1]}${i < last ? ',' : ''}` : '');
    return `{\n  "server": {\n${lines.map(line).join('\n')}\n  }\n}\n`;
  },
  toml: (lines) => `[server]\n${lines.map(blankOr((key, value) => `  ${key} = ${value}`)).join('\n')}\n`,
  hcl: (lines) => `server {\n${lines.map(blankOr((key, value) => `  ${key} = ${value}`)).join('\n')}\n}\n`,
  xml: (lines) =>
    `<server>\n${lines.map(blankOr((key, value) => `  <${key}>${String(value).replace(/"/g, '')}</${key}>`)).join('\n')}\n</server>\n`,
};
const fill = (from, count) => Array.from({ length: count }, (_, i) => [`fill_${from + i}`, from + i]);

// Port's docs, `lines` lines of them (taller than the room above line 4
// with 4).
const schema = (lines) => JSON.stringify({
  type: 'object',
  properties: {
    server: {
      type: 'object',
      properties: {
        port: { type: 'integer', description: ['Port docs, long enough to read and select some words.', ...Array(lines - 1).fill('More port docs.')].join('\n'), 'x-docs-url': 'https://example.com/port' },
        host: { type: 'string', description: 'Host docs' },
      },
    },
  },
});

// A blank line beside `port` on the side away from its card (a detour
// onto another key's docs swaps the card for them).
const SIDES = {
  // `port` on line 9: its card fits above it, over `host`.
  above: (doc) => doc([...fill(0, 5), ['host', '"x"'], ['fill_5', 5], ['port', 3], [], ...fill(6, 6)]),
  // `port` on line 3: no room above, the card goes below, over `host`.
  below: (doc) => doc([[], ['port', 3], ['fill_0', 0], ['host', '"x"'], ...fill(1, 8)]),
};

// Away from the text, then onto `port`: its card shows.
async function hoverPort(page) {
  await page.mouse.move(890, 595);
  await expect(card(page)).toBeHidden();
  const port = await wordBox(page, 'port');
  const at = { x: port.left + 6, y: port.y };
  await page.mouse.move(at.x, at.y);
  await expect(card(page)).toContainText('Port docs');
  return { port, at, box: await card(page).boundingBox() };
}

// The detours a hand makes on its way from the key to its card: `[x, y]`
// of the detour's end, from the key, its card and its line. Short ones:
// a pointer away from the key and not heading for the card lets it go
// after its moment (350 ms).
const DETOURS = {
  'left into the indentation': ({ port }) => [port.left - 12, port.y - 4],
  'left across the indentation': ({ port }) => [port.textLeft + 1, port.y],
  'left into the gutter': ({ port }) => [port.textLeft - 3, port.y - 4],
  'far left into the gutter': ({ port }) => [Math.max(2, port.textLeft - 20), port.y],
  'right past the key': ({ port }) => [port.right + 40, port.y - 4],
  'away from the card onto the blank line': ({ port, side }) => [port.left + 6, port.y + (side === 'above' ? 24 : -24)],
};

for (const [language, doc] of Object.entries(LANGUAGES)) {
  test.describe(language, () => {
    for (const [side, value] of Object.entries(SIDES)) {
      test(`F1: a card ${side} its line keeps a pointer that gets onto it after a detour`, async ({ page }) => {
        const errors = await open(page, { value: value(doc), language, schema: schema(side === 'above' ? 1 : 4) });
        for (const [name, detour] of Object.entries(DETOURS)) {
          const { port, at, box } = await hoverPort(page);
          if (side === 'above') expect(box.y + box.height).toBeLessThanOrEqual(port.top);
          else expect(box.y).toBeGreaterThanOrEqual(port.bottom);
          const [x, y] = detour({ port, box, side });
          // Into the card at its key's side, a few lines from its edge.
          const target = { x: box.x + 40, y: side === 'above' ? box.y + box.height - 25 : box.y + 25 };
          await glide(page, [at, { x, y }, target]);
          expect(await under(page, target), name).toBe('card');
          await page.waitForTimeout(700);
          await expect(card(page), name).toContainText('Port docs');
          expect(await under(page, target), name).toBe('card');
        }
        expect(errors).toEqual([]);
      });

      test(`F1: lines a card ${side} its line covers stay the text's from beside it`, async ({ page }) => {
        const errors = await open(page, { value: value(doc), language, schema: schema(side === 'above' ? 1 : 4) });
        const { port, at, box } = await hoverPort(page);
        const host = await wordBox(page, 'host');
        // `host` is under the card.
        expect(host.left).toBeGreaterThanOrEqual(box.x);
        expect(host.y).toBeGreaterThan(box.y);
        expect(host.y).toBeLessThan(box.y + box.height);
        // Along the key's line past the card's side, onto host's line
        // beside the card, then onto host under it.
        const beside = box.x + box.width + 40;
        const onHost = { x: host.left + 6, y: host.y };
        await glide(page, [at, { x: beside, y: port.y }, { x: beside, y: host.y }, onHost]);
        await expect(card(page)).toContainText('Host docs');
        await page.mouse.click(onHost.x, onHost.y);
        const line = await input(page).evaluate((element) => element.value.slice(0, element.selectionStart).split('\n').length);
        expect(line).toBe(value(doc).split('\n').findIndex((text) => text.includes('host')) + 1);
        expect(errors).toEqual([]);
      });
    }
  });
}

// F4: moving along a line from a key (its card) over the items after it
// (each its problem's tip) and back, the popup swapped with an empty frame
// at each switch (12 to 16 ms). It swaps in place: no frame without a
// popup between two that have one. Logged at every frame, before the
// editor's own work of that frame.
const ITEMS = {
  yaml: 'args: ["--alpha", "--bravo", "--charlie", "--delta"]\nimage: x\n',
  json: '{ "args": ["--alpha", "--bravo", "--charlie", "--delta"],\n  "image": "x" }\n',
  toml: 'args = ["--alpha", "--bravo", "--charlie", "--delta"]\nimage = "x"\n',
  hcl: 'args = ["--alpha", "--bravo", "--charlie", "--delta"]\nimage = "x"\n',
  xml: '<args>--alpha</args>\n<args>--bravo</args>\n<image>x</image>\n',
};
const ITEMS_SCHEMA = JSON.stringify({
  type: 'object',
  properties: {
    args: { type: 'array', items: { type: 'string', maxLength: 5 }, description: 'Arguments to the entrypoint.' },
    image: { type: 'string', description: 'Image name.' },
  },
});

for (const [language, value] of Object.entries(ITEMS)) {
  test(`F4 ${language}: moving between neighbouring targets swaps the popup without an empty frame`, async ({ page }) => {
    const errors = await open(page, { value, language, schema: ITEMS_SCHEMA });
    const line = await page.evaluate(() => {
      const row = document.querySelector('.config-editor__highlight .config-editor__code-line');
      const spans = [...row.querySelectorAll('span')].filter((span) => !span.children.length && span.textContent.trim());
      // From the key (a JSON line's `{` before it has no docs).
      spans.splice(0, spans.findIndex((span) => span.textContent.includes('args')));
      const [first, last] = [spans[0].getBoundingClientRect(), spans.at(-1).getBoundingClientRect()];
      return { from: first.left + 2, to: last.right - 2, y: (first.top + first.bottom) / 2 };
    });
    await page.mouse.move(line.from + 6, line.y);
    await expect(page.locator('.config-editor__hover, .config-editor__problem-tip:not([hidden])').first()).toBeVisible();
    await page.evaluate(() => {
      window.popupFrames = [];
      const log = () => {
        const shown = [...document.querySelectorAll('.config-editor__hover, .config-editor__problem-tip')]
          .filter((popup) => popup.checkVisibility({ visibilityProperty: true, opacityProperty: true }))
          .map((popup) => popup.textContent.slice(0, 16))
          .join('+');
        if (window.popupFrames.at(-1)?.shown !== shown) window.popupFrames.push({ at: performance.now(), shown });
        requestAnimationFrame(log);
      };
      requestAnimationFrame(log);
    });
    const points = [];
    const steps = Math.round((line.to - line.from) / 6);
    for (let i = 0; i <= steps; i++) points.push({ x: line.from + ((line.to - line.from) * i) / steps, y: line.y });
    await glide(page, [...points, ...[...points].reverse()]);
    const frames = await page.evaluate(() => window.popupFrames);
    // A card and tips took turns.
    expect(new Set(frames.map((frame) => frame.shown).filter(Boolean)).size).toBeGreaterThan(1);
    const blinks = frames.filter((frame, i) => i > 0 && !frame.shown && frames[i + 1]?.shown && frames[i + 1].at - frame.at < 40);
    expect(blinks, JSON.stringify(frames)).toEqual([]);
    expect(errors).toEqual([]);
  });
}
