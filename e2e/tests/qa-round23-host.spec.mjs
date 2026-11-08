import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 23 (host API): list items holding a block (YAML `- name: web`,
// a second HCL block, a TOML `[[array]]` and its entries) are found by
// path in every language, and a host problem by such a path is marked on
// the item; a nested `undefined` in `colors` and `strings` leaves the
// entry as it was, as a top-level `undefined` leaves a setting.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

async function open(page) {
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async () => {
    window.module = await import('../config-editor/index.js');
    window.tools = await import('../config-editor/tools.js');
  });
}

test.describe('QA round 23: host API', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('list items holding a block are found by path, and pathAt gives them back', async ({ page }) => {
    await open(page);
    const found = await page.evaluate(() => {
      const cases = [
        ['yaml', 'l:\n  - n: 1\n  - n: 2\n', ['/l/0', '/l/1']],
        ['yaml', 'spec:\n  containers:\n    - name: web\n    - name: side\n', ['/spec/containers/1']],
        ['hcl', 'b {\n  x = 1\n}\nb {\n  x = 2\n}\n', ['/b/1']],
        ['hcl', 'd "l" {\n  y = 1\n}\nd "l" {\n  y = 2\n}\n', ['/d/l/1']],
        ['toml', '[[l]]\nn = 1\n\n[[l]]\nn = 2\n', ['/l', '/l/1']],
        ['toml', '[t]\n[[t.l]]\nn = 1\n', ['/t/l']],
      ];
      const out = [];
      for (const [language, text, paths] of cases) {
        for (const ending of ['\n', '\r\n']) {
          const written = text.replaceAll('\n', ending);
          for (const path of paths) {
            const range = window.tools.rangeOf(language, written, path);
            const back = range && window.tools.pathAt(language, written, range.start)?.pointer;
            out.push(`${language} ${JSON.stringify(ending)} ${path}: ${range ? `line ${range.startLine}` : 'none'} -> ${back}`);
          }
        }
      }
      return out;
    });
    const lines = { '/l/0': 2, '/l/1': 3 };
    expect(found).toEqual([
      ...['"\\n"', '"\\r\\n"'].flatMap((ending) => ['/l/0', '/l/1'].map((path) => `yaml ${ending} ${path}: line ${lines[path]} -> ${path}`)),
      ...['"\\n"', '"\\r\\n"'].map((ending) => `yaml ${ending} /spec/containers/1: line 4 -> /spec/containers/1`),
      ...['"\\n"', '"\\r\\n"'].map((ending) => `hcl ${ending} /b/1: line 4 -> /b/1`),
      ...['"\\n"', '"\\r\\n"'].map((ending) => `hcl ${ending} /d/l/1: line 4 -> /d/l/1`),
      ...['"\\n"', '"\\r\\n"'].flatMap((ending) => [`toml ${ending} /l: line 1 -> /l/0`, `toml ${ending} /l/1: line 4 -> /l/1`]),
      ...['"\\n"', '"\\r\\n"'].map((ending) => `toml ${ending} /t/l: line 2 -> /t/l/0`),
    ]);
  });

  test('a host problem at a list item holding a block is marked on its line, and reveal goes there', async ({ page }) => {
    await open(page);
    const result = await page.evaluate(async () => {
      const host = document.createElement('div');
      host.style.height = '300px';
      document.body.prepend(host);
      const value = 'l:\n  - n: 1\n  - n: 2\n';
      const editor = new window.module.ConfigEditor(host, {
        value,
        language: 'yaml',
        hostDiagnostics: [{ path: '/l/1', severity: 'error', message: 'second item' }],
      });
      await new Promise((done) => requestAnimationFrame(() => setTimeout(done, 50)));
      // The line of the gutter row level with each host marker.
      const rows = [...host.querySelectorAll('.config-editor__gutter-content > div[data-line]')];
      const marked = [...host.querySelectorAll('.config-editor__diagnostic-marker-host')].map((marker) => {
        const top = marker.getBoundingClientRect().top;
        const distance = (row) => Math.abs(row.getBoundingClientRect().top - top);
        return rows.reduce((best, row) => (distance(row) < distance(best) ? row : best)).dataset.line;
      });
      const revealed = editor.reveal('/l/1');
      const input = host.querySelector('.config-editor__input');
      const caretLine = input.value.slice(0, input.selectionStart).split('\n').length;
      editor.destroy();
      host.remove();
      return { marked, revealed, caretLine };
    });
    expect(result).toEqual({ marked: ['3'], revealed: true, caretLine: 3 });
  });

  test('the editor sizes itself as the READMEs say: height 100% fills its element, --config-editor-height fixes the text area', async ({ page }) => {
    await open(page);
    const heights = await page.evaluate(async () => {
      const host = document.createElement('div');
      host.id = 'sized';
      document.body.prepend(host);
      const style = document.createElement('style');
      document.head.append(style);
      const big = `${Array.from({ length: 200 }, (_, index) => `k${index}: ${index}`).join('\n')}\n`;
      const out = {};
      for (const [name, css] of [
        ['fill', '#sized { height: 300px; } #sized .config-editor { height: 100%; }'],
        ['fixed', '#sized .config-editor { --config-editor-height: 400px; }'],
      ]) {
        style.textContent = css;
        out[name] = [];
        for (const value of ['a: 1\n', big]) {
          const editor = new window.module.ConfigEditor(host, { value, showToolbar: true, showFooter: true });
          await new Promise((done) => requestAnimationFrame(() => setTimeout(done, 100)));
          const of = (selector) => Math.round(host.querySelector(selector).getBoundingClientRect().height);
          out[name].push(name === 'fill' ? of('.config-editor') : of('.config-editor__surface'));
          editor.destroy();
        }
      }
      style.remove();
      host.remove();
      return out;
    });
    expect(heights).toEqual({ fill: [300, 300], fixed: [400, 400] });
  });

  test('a nested undefined leaves a color token or a text as it was; null resets it', async ({ page }) => {
    await open(page);
    const result = await page.evaluate(() => {
      const host = document.createElement('div');
      document.body.prepend(host);
      const editor = new window.module.ConfigEditor(host, {
        value: 'a: 1\n',
        colors: { key: '#ff0000' },
        strings: { noProblems: 'Keine Fehler' },
      });
      const out = [];
      editor.update({ colors: { key: undefined }, strings: { noProblems: undefined } });
      out.push([editor.settings.colors, editor.settings.strings.noProblems]);
      editor.update({ colors: { key: null }, strings: { noProblems: null } });
      out.push([editor.settings.colors, editor.settings.strings.noProblems]);
      editor.destroy();
      // A document's own settings over the view's, alike.
      const view = new window.module.ConfigDocuments(host, {
        documents: [{ id: 'a', fileName: 'a.yaml', value: 'a: 1\n', settings: { colors: { key: undefined } } }],
        colors: { key: '#00ff00' },
      });
      out.push(view.editor('a')?.settings.colors ?? null);
      view.destroy();
      host.remove();
      return out;
    });
    expect(result[0]).toEqual([{ key: '#ff0000' }, 'Keine Fehler']);
    expect(result[1][0]).toEqual({});
    expect(result[1][1]).not.toBe('Keine Fehler');
    expect(result[2]).toEqual({ key: '#00ff00' });
  });
});
