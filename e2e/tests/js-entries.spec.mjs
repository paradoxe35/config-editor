import { expect, test } from '@playwright/test';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// What each entry exports at runtime is exactly what its README documents:
// `@pngwasi/config-editor` (an app), `/tools` (text without an editor),
// `/binding` (a package for another framework), and each framework package
// (its components only: the helpers come from `@pngwasi/config-editor`).
// wasm-bindgen's internals (`start`, the stream adapters, `initSync`, the
// raw init as a default export) never leak. Build the packages first with
// `npm run build` in js/.
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const page = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.skip(!built, 'js packages not built: run `npm run build` in js/');

const ENTRIES = {
  main: ['ConfigDocuments', 'ConfigEditor', 'languageForFile', 'languages', 'loadConfigEditor', 'registerLanguage', 'themes'],
  tools: [
    'changeSummary',
    'convert',
    'loadConfigEditor',
    'parse',
    'pathAt',
    'presetCss',
    'rangeOf',
    'rangesOf',
    'schemaWarnings',
    'serialize',
    'unifiedDiff',
    'validate',
  ],
  binding: ['DocumentsBinding', 'EditorBinding', 'SETTING_TYPES'],
};

const FRAMEWORKS = {
  'react.html': { exports: ['ConfigDocuments', 'ConfigEditor'], readme: 'config-editor-react' },
  'vue.html': { exports: ['ConfigDocuments', 'ConfigEditor', 'configDocumentsProps', 'configEditorProps'], readme: 'config-editor-vue' },
  'svelte.html': { exports: ['ConfigDocuments', 'ConfigEditor'], readme: 'config-editor-svelte' },
};

async function ready(browserPage, name) {
  await browserPage.goto(page(name));
  await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
}

test('the main, tools and binding entries export exactly their documented names', async ({ page: browserPage }) => {
  await ready(browserPage, 'vanilla.html');
  const exported = await browserPage.evaluate(async () => {
    const names = async (path) => Object.keys(await import(path)).sort();
    return {
      main: await names('../config-editor/index.js'),
      tools: await names('../config-editor/tools.js'),
      binding: await names('../config-editor/binding.js'),
    };
  });
  expect(exported).toEqual(ENTRIES);
});

test('package.json maps each entry to its module and types', () => {
  const manifest = JSON.parse(readFileSync(join(js, 'config-editor', 'package.json'), 'utf8'));
  expect(manifest.exports['.']).toEqual({ types: './index.d.ts', default: './index.js' });
  expect(manifest.exports['./tools']).toEqual({ types: './tools.d.ts', default: './tools.js' });
  expect(manifest.exports['./binding']).toEqual({ types: './binding.d.ts', default: './binding.js' });
  // Modules the entries import; the internal ones (`sync.js`, the bindings'
  // echo check, and the generated `setting-types.js`) ship without types.
  const internal = ['sync.js', 'setting-types.js'];
  for (const file of ['index.js', 'tools.js', 'binding.js', 'load.js', ...internal]) {
    expect(manifest.files, file).toContain(file);
    const types = file.replace('.js', '.d.ts');
    if (internal.includes(file)) expect(manifest.files, file).not.toContain(types);
    else expect(manifest.files, file).toContain(types);
  }
});

test('the README documents every export of each entry', () => {
  const readme = readFileSync(join(js, 'config-editor', 'README.md'), 'utf8');
  const missing = Object.values(ENTRIES)
    .flat()
    .filter((name) => !readme.includes(`\`${name}`));
  expect(missing).toEqual([]);
  expect(readme).toContain('@pngwasi/config-editor/tools');
  expect(readme).toContain('@pngwasi/config-editor/binding');
});

for (const [name, { exports, readme }] of Object.entries(FRAMEWORKS)) {
  test(`${name}: the framework package exports its components only`, async ({ page: browserPage }) => {
    await ready(browserPage, name);
    expect(await browserPage.evaluate(() => Object.keys(window.packageApi).sort())).toEqual(exports);
    const text = readFileSync(join(js, readme, 'README.md'), 'utf8');
    expect(text).toContain('come from `@pngwasi/config-editor`');
  });
}
