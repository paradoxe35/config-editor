import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Thousands of host problems stay cheap (QA 19): F8 with 3000 of them in a
// 6000-line document (each press froze 350-1050 ms), and moving through a
// document with 50 ranges of 200 lines (10,000 underlines froze 1.6-2.9 s
// per Ctrl+End). With 3000 host ranges in a 4500–6000-line document, each
// edit placed them again from scratch, splitting the text once per range
// (QA 20: a 220–340 ms task per edit, in every language); and 400 lines
// committed in one command (an IME commit, `Input.insertText`) froze 6–8 s
// in Chrome's own line-by-line insert. Timed: runs alone after the other
// specs (see the config).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const vanilla = (options = {}) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

const input = (page) => page.locator('.config-editor__input');
const footer = (page) => page.locator('.config-editor__diagnostic-status');

async function open(page, options) {
  await page.goto(vanilla(options));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await input(page).waitFor();
  await page.evaluate(() => {
    window.longest = 0;
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) window.longest = Math.max(window.longest, entry.duration);
    }).observe({ type: 'longtask' });
  });
}

/** The longest task while `work` runs and the page settles, in ms. */
async function longestTask(page, work) {
  await page.waitForTimeout(300);
  await page.evaluate(() => (window.longest = 0));
  await work();
  await page.waitForTimeout(400);
  return page.evaluate(() => window.longest);
}

const lines = (count) => Array.from({ length: count }, (_, index) => `k${index}: ${index}`).join('\n') + '\n';

test.describe('problem timing', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  test('F8 through 3000 host problems in 6000 lines stays responsive', async ({ page }) => {
    await open(page, { value: lines(6000), schema: '{}' });
    const problems = Array.from({ length: 3000 }, (_, index) => ({
      range: { startLine: 2 + ((index * 2) % 5990), startColumn: 1, endLine: 4 + ((index * 2) % 5990), endColumn: 1 },
      severity: ['error', 'warning', 'info', 'hint'][index % 4],
      message: `host ${index}`,
    }));
    await page.evaluate((list) => window.editor.update({ hostDiagnostics: list }), problems);
    await expect(footer(page)).toHaveText(/^750 errors/);
    await input(page).click();
    await page.keyboard.press('F8');
    const presses = [];
    for (let press = 0; press < 5; press += 1) presses.push(await longestTask(page, () => page.keyboard.press('F8')));
    // Was 350-1050 ms a press.
    expect(Math.min(...presses), `longest task per press: ${presses.join(', ')}`).toBeLessThan(150);
  });

  test('moving through 50 ranges of 200 lines stays responsive', async ({ page }) => {
    await open(page, { value: lines(3000), schema: '{}' });
    const problems = Array.from({ length: 50 }, (_, index) => ({
      range: { startLine: 1 + index * 7, startColumn: 1, endLine: 201 + index * 7, endColumn: 2 },
      severity: 'warning',
      message: `w${index}`,
    }));
    await page.evaluate((list) => window.editor.update({ hostDiagnostics: list }), problems);
    await expect(footer(page)).toHaveText(/^50 warnings/);
    expect(await page.locator('.config-editor__diagnostic-marker').count()).toBeLessThanOrEqual(200);
    await input(page).click();
    const moves = [];
    for (let move = 0; move < 3; move += 1) {
      moves.push(await longestTask(page, () => page.keyboard.press('Control+End')));
      moves.push(await longestTask(page, () => page.keyboard.press('Control+Home')));
    }
    // Was 1.6-2.9 s.
    expect(Math.min(...moves), `longest task per move: ${moves.join(', ')}`).toBeLessThan(200);
  });

  test('U20-N2: 3000 host ranges in a long document add little to any edit, in every language', async ({ page }) => {
    test.setTimeout(300_000);
    const failures = [];
    for (const language of ['yaml', 'json', 'toml', 'hcl', 'xml']) {
      const text = longDocument(language);
      await openLong(page, language, text, false);
      await input(page).click();
      await page.keyboard.press('Control+g');
      await page.keyboard.type(`${Math.floor(text.split('\n').length / 2)}:1`);
      await page.keyboard.press('Enter');
      await page.keyboard.press('End');
      // The longest task of each edit, the problems found again after the
      // typing pause included; the comment toggled back after.
      const settled = (work) => longestTask(page, async () => {
        await work();
        await page.waitForTimeout(1200);
      });
      const edits = async () => ({
        key: await settled(() => page.keyboard.press('x')),
        undo: await settled(() => page.keyboard.press('Control+z')),
        comment: await settled(() => page.keyboard.press('Control+/')),
        back: await settled(() => page.keyboard.press('Control+/')),
      });
      const alone = await edits();
      await addHostRanges(page, text);
      const withHost = await edits();
      console.log(`${language}: longest task per edit ${JSON.stringify(alone)} alone, ${JSON.stringify(withHost)} with 3000 host ranges`);
      // They added 150-350 ms to each edit.
      for (const edit of Object.keys(alone)) {
        if (withHost[edit] - alone[edit] >= 100) {
          failures.push(`${language} ${edit}: ${Math.round(withHost[edit])} ms, ${Math.round(alone[edit])} ms alone`);
        }
      }
    }
    expect(failures).toEqual([]);
  });

  test('U20-3: 400 lines committed in one command at the end of a long document with 3000 host ranges take a fraction of a bare textarea\'s time', async ({ page }) => {
    test.setTimeout(120_000);
    const text = longDocument('yaml');
    const chunk = text.split('\n').slice(0, 400).join('\n') + '\n';
    await open(page, {});
    // The same insert into a bare textarea holding the document.
    await page.evaluate((value) => {
      const area = document.createElement('textarea');
      area.id = 'bare';
      area.style.cssText = 'position:fixed;left:0;top:0;width:600px;height:300px';
      document.body.append(area);
      area.value = value;
      area.focus();
      area.setSelectionRange(value.length, value.length);
    }, text);
    const bareStarted = Date.now();
    await page.keyboard.insertText(chunk);
    const bare = Date.now() - bareStarted;
    expect(await page.locator('#bare').inputValue()).toBe(text + chunk);
    await openLong(page, 'yaml', text);
    await input(page).click();
    await page.keyboard.press('Control+End');
    const started = Date.now();
    await page.keyboard.insertText(chunk);
    await expect.poll(() => input(page).inputValue()).toBe(text + chunk);
    const took = Date.now() - started;
    console.log(`insertText of 400 lines with 3000 host ranges: editor ${took} ms, bare textarea ${bare} ms`);
    // Was 6-8 s, as long as the bare textarea (Chrome inserts line by line).
    expect(took, `editor ${took} ms, bare textarea ${bare} ms`).toBeLessThan(bare / 4);
    // One step: undone at once, the caret where it was.
    await page.keyboard.press('Control+z');
    await expect.poll(() => input(page).inputValue()).toBe(text);
  });
});

/** A 4500–6000-line document in `language`: 1500 items of two fields, one in two the schema refuses. */
function longDocument(language) {
  const rows = Array.from({ length: 1500 }, (_, index) => ({ key: `i${index}`, n: index % 2 ? '"bad"' : String(index) }));
  if (language === 'yaml') return 'items:\n' + rows.map((row) => `  ${row.key}:\n    n: ${row.n}\n    s: "x"\n`).join('');
  if (language === 'json') return '{\n  "items": {\n' + rows.map((row) => `    "${row.key}": {\n      "n": ${row.n},\n      "s": "x"\n    }`).join(',\n') + '\n  }\n}\n';
  if (language === 'toml') return rows.map((row) => `[items.${row.key}]\nn = ${row.n}\ns = "x"\n`).join('\n');
  if (language === 'hcl') return 'items {\n' + rows.map((row) => `  ${row.key} {\n    n = ${row.n}\n    s = "x"\n  }\n`).join('') + '}\n';
  return '<items>\n' + rows.map((row) => `  <${row.key}>\n    <n>${row.n.replace(/"/g, '')}</n>\n    <s>x</s>\n  </${row.key}>\n`).join('') + '</items>\n';
}

/** Opens `text` in `language` with a schema (and, with `host`, 3000 host ranges). */
async function openLong(page, language, text, host = true) {
  const schema = JSON.stringify({
    type: 'object',
    properties: { items: { type: 'object', additionalProperties: { type: 'object', properties: { n: { type: 'integer' }, s: { type: 'string' } } } } },
  });
  await open(page, { language, value: language === 'json' ? '{}\n' : '', schema, fileName: `x.${language}` });
  await page.evaluate((value) => window.editor.update({ value }), text);
  await page.waitForTimeout(1500);
  if (host) await addHostRanges(page, text);
}

/** Gives the editor of `text` 3000 host ranges of three lines. */
async function addHostRanges(page, text) {
  const lines = text.split('\n').length;
  const problems = Array.from({ length: 3000 }, (_, index) => ({
    range: { startLine: 2 + ((index * 2) % (lines - 5)), startColumn: 1, endLine: 4 + ((index * 2) % (lines - 5)), endColumn: 1 },
    severity: ['error', 'warning', 'info', 'hint'][index % 4],
    message: `host ${index}`,
  }));
  await page.evaluate((list) => window.editor.update({ hostDiagnostics: list }), problems);
  await expect(footer(page)).toContainText('750 hints', { timeout: 20000 });
  await page.waitForTimeout(1500);
}
