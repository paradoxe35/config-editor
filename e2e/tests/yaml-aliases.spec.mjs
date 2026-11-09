import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// A YAML anchor used by many aliases (a Compose `x-defaults: &d` merged into
// every service, a GitLab CI template every job uses) is ordinary YAML: it
// reads, checks, converts and switches views however many aliases use it,
// in the editor and in the tools. What is refused is a text whose aliases
// expand to far more than itself ("billion laughs"), with a plain message.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

async function open(page) {
  page.errors = [];
  page.on('pageerror', (error) => page.errors.push(String(error)));
  await page.goto(pathToFileURL(join(js, 'examples', 'vanilla.html')).href);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async () => {
    window.editor?.destroy();
    window.module = await import('../config-editor/index.js');
    window.tools = await import('../config-editor/tools.js');
    await window.tools.loadConfigEditor();
    window.pause = (ms = 400) => new Promise((resolve) => setTimeout(resolve, ms));
  });
}

const compose = (services) =>
  'x-defaults: &defaults\n  restart: unless-stopped\nservices:\n' +
  Array.from({ length: services }, (_, index) => `  s${index}:\n    <<: *defaults\n    image: app${index}\n`).join('');

const laughs = (levels) =>
  'l0: &l0 "lol lol lol lol lol lol lol lol"\n' +
  Array.from({ length: levels - 1 }, (_, index) => {
    const level = index + 1;
    return `l${level}: &l${level} [${Array(10).fill(`*l${level - 1}`).join(', ')}]\n`;
  }).join('');

const REFUSED = /^The aliases \(\*name\) in this text expand to far more data than the text itself/;

test.describe('YAML aliases', () => {
  test.skip(!built, 'needs `npm run build` in js/');

  for (const services of [11, 50, 500]) {
    test(`an anchor used by ${services} aliases reads, checks, converts and switches views`, async ({ page }) => {
      await open(page);
      const text = compose(services);
      const result = await page.evaluate(
        async ([text, last]) => {
          const out = { problems: null, views: {} };
          out.validate = window.tools.validate('yaml', text).map((problem) => problem.message);
          out.restart = window.tools.parse('yaml', text).services[last].restart;
          out.json = JSON.parse(window.tools.convert('yaml', text, 'json').text).services[last].restart;
          out.toml = window.tools.convert('yaml', text, 'toml').text.includes('restart = "unless-stopped"');
          const element = document.createElement('div');
          document.body.prepend(element);
          const editor = new window.module.ConfigEditor(element, {
            value: text,
            onProblems: (problems) => (out.problems = problems.map((problem) => problem.message)),
            onEditAsChange: (language) => (out.editAs = language),
          });
          await window.pause();
          for (const view of ['json', 'toml', 'hcl']) {
            editor.update({ editAs: view });
            await window.pause();
            out.views[view] = [out.editAs, editor.text.includes('unless-stopped')];
          }
          editor.update({ editAs: null });
          await window.pause();
          out.back = editor.text === text;
          editor.destroy();
          return out;
        },
        [text, `s${services - 1}`],
      );
      expect(result.validate).toEqual([]);
      expect(result.problems).toEqual([]);
      expect(result.restart).toBe('unless-stopped');
      expect(result.json).toBe('unless-stopped');
      expect(result.toml).toBe(true);
      expect(result.views).toEqual({ json: ['json', true], toml: ['toml', true], hcl: ['hcl', true] });
      expect(result.back).toBe(true);
      expect(page.errors).toEqual([]);
    });
  }

  test('a billion laughs is refused with a plain message, promptly', async ({ page }) => {
    await open(page);
    const result = await page.evaluate(async (text) => {
      const out = {};
      const started = performance.now();
      out.validate = window.tools.validate('yaml', text).map((problem) => problem.message);
      try {
        window.tools.parse('yaml', text);
        out.parse = 'parsed';
      } catch (error) {
        out.parse = String(error.message ?? error);
      }
      const element = document.createElement('div');
      document.body.prepend(element);
      const editor = new window.module.ConfigEditor(element, {
        value: text,
        onProblems: (problems) => (out.problems = problems.map((problem) => problem.message)),
      });
      await window.pause();
      out.ms = performance.now() - started;
      editor.destroy();
      return out;
    }, laughs(12));
    expect(result.validate).toHaveLength(1);
    expect(result.validate[0]).toMatch(REFUSED);
    expect(result.parse).toMatch(REFUSED);
    expect(result.problems).toHaveLength(1);
    expect(result.problems[0]).toMatch(REFUSED);
    expect(result.ms).toBeLessThan(5000);
    expect(page.errors).toEqual([]);
  });
});
