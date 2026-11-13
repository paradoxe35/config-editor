import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Schema S.patterns-9: documents that share one schema text share its
// compiled schema, and its warnings go to the console once, not once per
// document shown; an editor with its own `onSchemaWarning` still hears them.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const VANILLA = pathToFileURL(join(js, 'examples', 'vanilla.html')).href;

const SCHEMA = JSON.stringify({ type: 'object', patternProperties: { '^(?!x-)[a-z]+$': { type: 'integer' } } });

test.describe('schema warnings', () => {
  test.skip(!built, 'the npm package is not built: cd js && npm run build');

  test('a schema text shared by many documents warns once, and each callback still hears it', async ({ page }) => {
    const warnings = [];
    page.on(
      'console',
      (message) => message.type() === 'warning' && message.text().includes('patternProperties') && warnings.push(message.text()),
    );
    await page.goto(VANILLA);
    await page.waitForFunction(() => document.body.dataset.ready === 'true');
    const heard = await page.evaluate(async (schema) => {
      const { ConfigDocuments, ConfigEditor } = await import(new URL('../config-editor/index.js', location.href).href);
      const documents = Array.from({ length: 8 }, (_, index) => ({
        id: `d${index}`,
        fileName: `d${index}.yaml`,
        value: `a: ${index}\n`,
        schema,
      }));
      const view = new ConfigDocuments(document.body.appendChild(document.createElement('div')), { documents });
      for (const { id } of documents) {
        view.select(id);
        await new Promise((done) => requestAnimationFrame(() => setTimeout(done, 30)));
      }
      const told = [];
      for (let index = 0; index < 2; index += 1) {
        new ConfigEditor(document.body.appendChild(document.createElement('div')), {
          value: 'a: 1\n',
          schema,
          onSchemaWarning: (message) => told.push(index),
        });
      }
      await new Promise((done) => setTimeout(done, 100));
      return told;
    }, SCHEMA);
    expect(warnings).toHaveLength(1);
    expect(heard).toEqual([0, 1]);
  });
});
