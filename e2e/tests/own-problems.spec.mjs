import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// `onProblems`: the editor's own problems (syntax, schema, secret
// placeholders; `origin: "editor"`), never the host's `hostDiagnostics`,
// once on mount and then each time they change, and never while they stay
// the same; per document in `ConfigDocuments`. Real keys in the vanilla
// page (the framework components: `framework-cases.mjs`).

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = pathToFileURL(join(js, 'examples', 'vanilla.html')).href;

test.skip(!built, 'js packages not built: run `npm run build` in js/');

const SCHEMA = JSON.stringify({ type: 'object', properties: { port: { type: 'integer' } } });

async function open(page) {
  await page.goto(example);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(() => {
    window.editor?.destroy();
    document.querySelectorAll('.config-editor').forEach((element) => element.remove());
  });
}

const input = (page) => page.locator('.config-editor__input');
const heard = (page) => page.evaluate(() => window.heard);
const summary = (problems) => problems.map(({ origin, severity, message }) => `${origin} ${severity} ${message}`);

// Puts the caret at the first `marker` in the text, plus `delta`.
async function caretAt(page, marker, delta = 0) {
  await input(page).evaluate(
    (element, [text, shift]) => {
      const at = element.value.indexOf(text) + shift;
      element.focus();
      element.setSelectionRange(at, at);
      element.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    },
    [marker, delta],
  );
}

test("ConfigEditor: a syntax error and a schema problem come and go; the host's never", async ({ page }) => {
  await open(page);
  await page.evaluate(async (schema) => {
    const { ConfigEditor } = await import('../config-editor/index.js');
    const host = document.createElement('div');
    document.body.prepend(host);
    window.heard = [];
    window.qa = new ConfigEditor(host, {
      language: 'yaml',
      value: 'port: 80\nname: web\n',
      schema,
      hostDiagnostics: [{ path: '/name', severity: 'error', message: 'taken on the server' }],
      onProblems: (problems) => window.heard.push(problems),
    });
  }, SCHEMA);
  // Once on mount: none of the editor's own (the host's is shown, not heard).
  await expect.poll(async () => (await heard(page)).length).toBe(1);
  expect((await heard(page))[0]).toEqual([]);
  await expect(page.locator('.config-editor__diagnostic-marker[aria-label*="taken on the server"]')).toHaveCount(1);

  // A schema problem.
  await caretAt(page, '80', 2);
  await page.keyboard.type('x');
  await expect.poll(async () => (await heard(page)).length).toBe(2);
  const schema = (await heard(page))[1];
  expect(schema).toHaveLength(1);
  expect(schema[0]).toMatchObject({ origin: 'editor', severity: 'error', line: 1 });
  expect(schema[0].message).toMatch(/^\/port/);

  // Fixed, then a syntax error, then fixed again.
  await page.keyboard.press('Backspace');
  await expect.poll(async () => (await heard(page)).length).toBe(3);
  expect((await heard(page))[2]).toEqual([]);
  await caretAt(page, 'web', 3);
  await page.keyboard.type(': [');
  await expect.poll(async () => (await heard(page)).length).toBe(4);
  const syntax = (await heard(page))[3];
  expect(summary(syntax).length).toBeGreaterThan(0);
  expect(syntax.every((problem) => problem.origin === 'editor')).toBe(true);
  for (let i = 0; i < 3; i += 1) await page.keyboard.press('Backspace');
  await expect.poll(async () => (await heard(page)).length).toBe(5);
  expect((await heard(page))[4]).toEqual([]);

  // Nothing changes: no event (typing a comment, a new host list).
  await page.keyboard.press('Control+End');
  await page.keyboard.type('# a note');
  await page.evaluate(() => window.qa.update({ hostDiagnostics: [{ path: '/port', severity: 'warning', message: 'busy' }] }));
  await page.waitForTimeout(1200);
  expect((await heard(page)).length).toBe(5);
  await page.evaluate(() => window.qa.destroy());
});

test('ConfigDocuments: each document reports its own, once its editor mounts', async ({ page }) => {
  await open(page);
  await page.evaluate(async () => {
    const { ConfigDocuments } = await import('../config-editor/index.js');
    const host = document.createElement('div');
    document.body.prepend(host);
    window.heard = [];
    window.qa = new ConfigDocuments(host, {
      documents: [
        { id: 'ok', fileName: 'ok.yaml', value: 'a: 1\n' },
        { id: 'broken', fileName: 'broken.json', value: '{"a": }\n' },
      ],
      onProblems: (id, problems) => window.heard.push([id, problems.map((problem) => problem.origin)]),
    });
  });
  await expect.poll(() => heard(page)).toEqual([['ok', []]]);
  await page.locator('[role="tab"]', { hasText: 'broken.json' }).click();
  await expect.poll(async () => (await heard(page)).length).toBe(2);
  const [id, origins] = (await heard(page))[1];
  expect(id).toBe('broken');
  expect(origins.length).toBeGreaterThan(0);
  expect(origins.every((origin) => origin === 'editor')).toBe(true);
  // Back to the first: its editor stays mounted and nothing changed.
  await page.locator('[role="tab"]', { hasText: 'ok.yaml' }).click();
  await page.waitForTimeout(800);
  expect((await heard(page)).length).toBe(2);
  await page.evaluate(() => window.qa.destroy());
});
