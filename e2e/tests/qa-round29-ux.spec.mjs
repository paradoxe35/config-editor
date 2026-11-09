import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 29, UX: one rule for a popup that still shows when the pointer
// gets onto it, wherever the popup sits (flush with the text's left edge
// or indented with its key) and whatever way the pointer took (left, up and
// right through the gutter or the indentation; right past the popup's far
// side, up and left; beside it on a line it covers, then onto it). It
// keeps the pointer (M29-U1), as VS Code's hiding delay keeps a hover;
// and a line it covers stays the text's from beside it, from the gutter
// too, without a pause (L29-U2): a pointer that goes along that line onto
// its text and stops there gets that text's docs, and a click there
// places the caret. Mice move as hands do: steps of at most 8 px, 8 to
// 16 ms apart. In every language, cards above and below their line, at
// 900 × 600, through the npm package's pages (run `npm run build` in js/
// first).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const pageUrl = (options) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

test.skip(!built, 'run `npm run build` in js/ first');
test.use({ viewport: { width: 900, height: 600 } });

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
  // The editor at the page's top: a card on its first lines has no room
  // above.
  await page.addStyleTag({ content: 'body > *:not(:has(.config-editor)) { display: none !important; } body { margin: 0; }' });
  return errors;
}

// The box of the first drawn `word` (outside quotes and brackets), its
// line's middle, and the text area's left edge.
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
// (60 to 125 events a second). A point `{ jump: true }` is reached in one
// move.
async function glide(page, points) {
  for (let i = 1; i < points.length; i++) {
    const [from, to] = [points[i - 1], points[i]];
    const steps = to.jump ? 1 : Math.max(1, Math.round(Math.hypot(to.x - from.x, to.y - from.y) / 8));
    for (let step = 1; step <= steps; step++) {
      await page.mouse.move(from.x + ((to.x - from.x) * step) / steps, from.y + ((to.y - from.y) * step) / steps);
      await page.waitForTimeout(8 + (step % 3) * 4);
    }
  }
}

// What is under the pointer at `at`: the popup, else the text.
const under = (page, at) =>
  page.evaluate(
    ({ x, y }) => (document.elementFromPoint(x, y)?.closest('.config-editor__hover, .config-editor__problem-tip') ? 'popup' : 'text'),
    at,
  );

// The caret's zero-based line, and whether the text has the focus.
const caret = (page) =>
  input(page).evaluate((element) => ({
    line: element.value.slice(0, element.selectionStart).split('\n').length - 1,
    focused: document.activeElement === element,
  }));

// Where the keys sit: at the top level (flush with the text's left edge
// where the language allows it), or nested in a `server` block. Each
// builds a document from (key, value) pairs and its schema from the
// keys' docs.
const pairsOf = (lines) => lines.map(([key, value]) => [key, value]);
const POSITIONS = {
  top: {
    yaml: (lines) => pairsOf(lines).map(([k, v]) => `${k}: ${v}`).join('\n') + '\n',
    json: (lines) => `{\n${pairsOf(lines).map(([k, v], i) => `  "${k}": ${v}${i < lines.length - 1 ? ',' : ''}`).join('\n')}\n}\n`,
    toml: (lines) => pairsOf(lines).map(([k, v]) => `${k} = ${v}`).join('\n') + '\n',
    hcl: (lines) => pairsOf(lines).map(([k, v]) => `${k} = ${v}`).join('\n') + '\n',
    xml: (lines) => `<config>\n${pairsOf(lines).map(([k, v]) => `  <${k}>${String(v).replace(/"/g, '')}</${k}>`).join('\n')}\n</config>\n`,
    schema: (language, properties) =>
      language === 'xml' ? { type: 'object', properties: { config: { type: 'object', properties } } } : { type: 'object', properties },
  },
  nested: {
    yaml: (lines) => `server:\n${pairsOf(lines).map(([k, v]) => `  ${k}: ${v}`).join('\n')}\n`,
    json: (lines) => `{\n  "server": {\n${pairsOf(lines).map(([k, v], i) => `    "${k}": ${v}${i < lines.length - 1 ? ',' : ''}`).join('\n')}\n  }\n}\n`,
    toml: (lines) => `[server]\n${pairsOf(lines).map(([k, v]) => `  ${k} = ${v}`).join('\n')}\n`,
    hcl: (lines) => `server {\n${pairsOf(lines).map(([k, v]) => `  ${k} = ${v}`).join('\n')}\n}\n`,
    xml: (lines) =>
      `<config>\n  <server>\n${pairsOf(lines).map(([k, v]) => `    <${k}>${String(v).replace(/"/g, '')}</${k}>`).join('\n')}\n  </server>\n</config>\n`,
    schema: (language, properties) => {
      const server = { server: { type: 'object', properties } };
      return language === 'xml'
        ? { type: 'object', properties: { config: { type: 'object', properties: server } } }
        : { type: 'object', properties: server };
    },
  },
};
const LANGUAGES = ['yaml', 'json', 'toml', 'hcl', 'xml'];
const fill = (from, count) => Array.from({ length: count }, (_, i) => [`fill_${from + i}`, from + i]);

// `host` on the line next to `port`, on the side of port's card: above
// (port well down, its card fits above) or below (port on the first lines,
// a tall card does not).
const SIDES = {
  above: { lines: [...fill(0, 6), ['host', '"x"'], ['port', 3], ...fill(6, 6)], docLines: 1 },
  below: { lines: [['port', 3], ['host', '"x"'], ...fill(0, 10)], docLines: 4 },
};
// (A card about 200 px wide: the way round its far side is short enough
// to take before it goes, as in VS Code, where a hover goes a moment
// after the pointer stops getting closer to it.)
const docs = (lines) => ['Port docs, read and select.', ...Array(lines - 1).fill('More port docs.')].join('\n');

// Away from the text, then onto `port`: its card shows.
async function hoverPort(page, side) {
  await page.mouse.move(890, 595);
  await expect(card(page)).toBeHidden();
  const port = await wordBox(page, 'port');
  const at = { x: port.left + 6, y: port.y };
  await page.mouse.move(at.x, at.y);
  await expect(card(page)).toContainText('Port docs');
  const box = await card(page).boundingBox();
  if (side === 'above') expect(box.y + box.height).toBeLessThanOrEqual(port.top);
  else expect(box.y).toBeGreaterThanOrEqual(port.bottom);
  return { port, at, box };
}

// The ways a hand takes from the key to its card after losing it: each
// ends on the card, on its plain lines (the card's middle), a few dozen
// pixels from the side it came across.
const DETOURS = {
  'left, along the card through the gutter, right': ({ port, box, middle }) => [
    { x: port.textLeft - 20, y: port.y },
    { x: port.textLeft - 20, y: middle },
    { x: box.x + 40, y: middle },
  ],
  'left, along the card through the indentation, right': ({ port, box, middle }) =>
    box.x - port.textLeft >= 10 && [
      { x: box.x - 6, y: port.y },
      { x: box.x - 6, y: middle },
      { x: box.x + 40, y: middle },
    ],
  'right past the card, along its far side, left': ({ port, box, middle }) => [
    { x: box.x + box.width + 25, y: port.y },
    { x: box.x + box.width + 25, y: middle },
    { x: box.x + box.width - 35, y: middle },
  ],
  'beside the card on a line it covers, then onto it at once': ({ box, middle, host }) => [
    { x: box.x + box.width + 40, y: host.y },
    { x: box.x + 60, y: middle, jump: true },
  ],
};

// The ways to `host`, the line next to port that its card covers: along
// that line from beside the card onto host's text.
const TO_HOST = {
  'through the gutter': ({ port, host }) => [
    { x: port.textLeft - 20, y: port.y },
    { x: port.textLeft - 20, y: host.y },
  ],
  'through the indentation': ({ port, box, host }) =>
    box.x - port.textLeft >= 10 && [
      { x: box.x - 6, y: port.y },
      { x: box.x - 6, y: host.y },
    ],
  'past the card’s far side': ({ port, box, host }) => [
    { x: box.x + box.width + 30, y: port.y },
    { x: box.x + box.width + 30, y: host.y },
  ],
};

for (const [position, build] of Object.entries(POSITIONS)) {
  for (const language of LANGUAGES) {
    for (const [side, { lines, docLines }] of Object.entries(SIDES)) {
      const value = build[language](lines);
      const schema = JSON.stringify(
        build.schema(language, {
          port: { type: 'integer', description: docs(docLines), 'x-docs-url': 'https://example.com/port' },
          host: { type: 'string', description: 'Host docs' },
        }),
      );
      const hostLine = value.split('\n').findIndex((text) => /\bhost\b/.test(text));

      test.describe(`${language}, ${position} keys, card ${side}`, () => {
        test('M29-U1: a card that still shows keeps a pointer that gets onto it after any detour', async ({ page }) => {
          const errors = await open(page, { value, language, schema });
          for (const [name, way] of Object.entries(DETOURS)) {
            const { port, at, box } = await hoverPort(page, side);
            const host = await wordBox(page, 'host');
            const middle = box.y + box.height / 2;
            const points = way({ port, box, middle, host });
            if (!points) continue;
            await glide(page, [at, ...points]);
            const end = points.at(-1);
            await expect(card(page), name).toContainText('Port docs');
            await expect.poll(() => under(page, end), { message: name }).toBe('popup');
            await page.waitForTimeout(700);
            await expect(card(page), name).toContainText('Port docs');
            expect(await under(page, end), name).toBe('popup');
          }
          expect(errors).toEqual([]);
        });

        test('L29-U2: the line next to the key stays the text’s under its card, from beside it', async ({ page }) => {
          const errors = await open(page, { value, language, schema });
          for (const [name, way] of Object.entries(TO_HOST)) {
            for (const click of ['after its docs show', 'at once']) {
              const { port, at, box } = await hoverPort(page, side);
              const host = await wordBox(page, 'host');
              // `host` is under the card.
              expect(host.left, name).toBeGreaterThanOrEqual(box.x);
              expect(host.y, name).toBeGreaterThan(box.y);
              expect(host.y, name).toBeLessThan(box.y + box.height);
              const points = way({ port, box, host });
              if (!points) continue;
              const onHost = { x: host.left + 4, y: host.y };
              await glide(page, [at, ...points, onHost]);
              if (click === 'after its docs show') await expect(card(page), name).toContainText('Host docs');
              await page.mouse.click(onHost.x, onHost.y);
              expect(await caret(page), `${name}, click ${click}`).toEqual({ line: hostLine, focused: true });
            }
          }
          expect(errors).toEqual([]);
        });
      });
    }
  }
}

// The problem tip with a link follows the same rule: kept after a detour
// past its far side, the line it covers reached along it from beside.
const TIP_DOCS = {
  yaml: (more) => `port: 3\nhost: x\n${more.map((i) => `fill_${i}: ${i}`).join('\n')}\n`,
  json: (more) => `{ "port": 3,\n  "host": "x",\n${more.map((i) => `  "fill_${i}": ${i},`).join('\n')}\n  "end": 0 }\n`,
  toml: (more) => `port = 3\nhost = "x"\n${more.map((i) => `fill_${i} = ${i}`).join('\n')}\n`,
  hcl: (more) => `port = 3\nhost = "x"\n${more.map((i) => `fill_${i} = ${i}`).join('\n')}\n`,
  xml: (more) => `<c>\n<port>3</port>\n<host>x</host>\n${more.map((i) => `<fill_${i}>${i}</fill_${i}>`).join('\n')}\n</c>\n`,
};
for (const [language, doc] of Object.entries(TIP_DOCS)) {
  test(`M29-U1 ${language}: a problem tip that still shows keeps a pointer that gets onto it past its far side`, async ({ page }) => {
    const value = doc(Array.from({ length: 12 }, (_, i) => i));
    const portLine = value.split('\n').findIndex((text) => /\bport\b/.test(text)) + 1;
    const problems = Array.from({ length: 4 }, (_, i) => ({
      range: { startLine: portLine, startColumn: 1, endColumn: 3 },
      severity: 'warning',
      message: `Checked by the host, number ${i + 1}`,
      docsUrl: 'https://example.com/port',
    }));
    const schema = JSON.stringify({ type: 'object', properties: { host: { type: 'string', description: 'Host docs' } } });
    const errors = await open(page, { value, language, hostDiagnostics: problems, schema: language === 'xml' ? undefined : schema });
    for (const route of ['detour', 'to host']) {
      await page.mouse.move(890, 595);
      await expect(tip(page)).toBeHidden();
      const row = await page.evaluate((line) => {
        const box = document.querySelectorAll('.config-editor__highlight .config-editor__code-line')[line - 1].getBoundingClientRect();
        return { y: (box.top + box.bottom) / 2, left: box.left };
      }, portLine);
      const at = { x: row.left + 3, y: row.y };
      await page.mouse.move(at.x, at.y);
      await expect(tip(page)).toContainText('number 1');
      const box = await tip(page).boundingBox();
      expect(box.y).toBeGreaterThan(row.y);
      const middle = box.y + box.height / 2;
      const beside = box.x + box.width + 30;
      if (route === 'detour') {
        const end = { x: box.x + box.width - 35, y: middle };
        await glide(page, [at, { x: beside, y: row.y }, { x: beside, y: middle }, end]);
        await expect.poll(() => under(page, end)).toBe('popup');
        await page.waitForTimeout(700);
        await expect(tip(page)).toContainText('number 1');
      } else {
        const host = await wordBox(page, 'host');
        const onHost = { x: host.left + 4, y: host.y };
        await glide(page, [at, { x: beside, y: row.y }, { x: beside, y: host.y }, onHost]);
        await page.mouse.click(onHost.x, onHost.y);
        expect(await caret(page)).toEqual({ line: portLine, focused: true });
      }
    }
    expect(errors).toEqual([]);
  });
}
