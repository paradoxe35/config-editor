import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// A JSON Schema may be given as YAML text, wherever schema text is taken:
// the editor's `schema`, a document's schema and the headless tools.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const VANILLA = pathToFileURL(join(js, 'examples', 'vanilla.html')).href;

const YAML_SCHEMA = `# The service's settings.
type: object
properties:
  listen:
    type: integer
    description: The port to listen on.
  mode:
    enum: [on, off]
`;

async function open(page) {
  await page.goto(VANILLA);
  await page.waitForFunction(() => document.body.dataset.ready === 'true');
}

test.describe('a schema written in YAML', () => {
  test.skip(!built, 'the npm package is not built: cd js && npm run build');

  test('drives completion and problems in an editor and in documents', async ({ page }) => {
    await open(page);
    await page.evaluate(async (schema) => {
      const CE = await import(new URL('../config-editor/index.js', location.href).href);
      const host = document.createElement('div');
      host.id = 'yaml-schema';
      host.style.height = '300px';
      document.body.prepend(host);
      window.problems = [];
      window.yamlSchemaEditor = new CE.ConfigEditor(host, {
        language: 'yaml',
        value: '',
        schema,
        onProblems: (problems) => (window.problems = problems),
      });
    }, YAML_SCHEMA);
    const input = page.locator('#yaml-schema .config-editor__input');
    await input.click();
    await page.keyboard.press('Control+Space');
    const keys = page.locator('#yaml-schema .config-editor__completion-key');
    await expect(keys).toHaveText(['listen', 'mode']);
    await page.keyboard.press('Escape');
    await page.keyboard.type('mode: maybe');
    await expect
      .poll(() => page.evaluate(() => window.problems.map((problem) => problem.message)))
      .toEqual([expect.stringContaining('/mode')]);

    // A document's schema in YAML.
    const told = await page.evaluate(async (schema) => {
      const CE = await import(new URL('../config-editor/index.js', location.href).href);
      const told = [];
      new CE.ConfigDocuments(document.body.appendChild(document.createElement('div')), {
        documents: [{ id: 'main', fileName: 'main.toml', value: 'listen = "x"\n', schema }],
        onProblems: (id, problems) => told.push(...problems.map((problem) => problem.message)),
      });
      await new Promise((done) => setTimeout(done, 400));
      return told;
    }, YAML_SCHEMA);
    expect(told).toContainEqual(expect.stringContaining('/listen'));
  });

  test('is read by the headless tools', async ({ page }) => {
    await open(page);
    const result = await page.evaluate(async (schema) => {
      const tools = await import(new URL('../config-editor/tools.js', location.href).href);
      return {
        problems: tools.validate('json', '{"listen": "x"}', schema).map((problem) => problem.message),
        warnings: tools.schemaWarnings('patternProperties:\n  ^(?!x-): {}\n'),
        broken: (() => {
          try {
            tools.validate('yaml', 'a: 1\n', '{"type": }');
            return null;
          } catch (error) {
            return String(error.message ?? error);
          }
        })(),
      };
    }, YAML_SCHEMA);
    expect(result.problems).toEqual([expect.stringContaining('/listen')]);
    expect(result.warnings).toHaveLength(1);
    expect(result.broken).toContain('expected value at line 1 column 10');
  });
});
