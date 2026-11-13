import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Under forced colours a secret placeholder's chip reads in every theme: its
// text takes the system's text colour on its forced background (hard round
// 27's `D27.a11y-1`: it kept the theme's string colour, 1.1:1 in dracula).

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.use({ forcedColors: 'active' });

test('a secret placeholder chip keeps 4.5:1 under forced colours in every theme', async ({ page }) => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  const found = await page.evaluate(async () => {
    const module = await import('../config-editor/index.js');
    window.editor?.destroy();
    const host = document.createElement('div');
    document.body.prepend(host);
    const parse = (color) => color.match(/[\d.]+/g).slice(0, 3).map(Number);
    const luminance = ([r, g, b]) =>
      [r, g, b].map((v) => (v /= 255) <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4).reduce((sum, v, i) => sum + v * [0.2126, 0.7152, 0.0722][i], 0);
    const ratio = (a, b) => {
      const [x, y] = [luminance(parse(a)), luminance(parse(b))].sort((p, q) => q - p);
      return (x + 0.05) / (y + 0.05);
    };
    const out = {};
    for (const { name: theme } of module.themes()) {
      const editor = new module.ConfigEditor(host, {
        language: 'yaml',
        value: 'key: «secret 3f2a»\n',
        theme,
        schema: JSON.stringify({ type: 'object', properties: { key: { type: 'string', writeOnly: true } } }),
      });
      await new Promise((resolve) => setTimeout(resolve, 100));
      const chip = host.querySelector('.config-editor__highlight .config-editor__secret');
      const style = getComputedStyle(chip);
      out[theme] = ratio(style.color, style.backgroundColor);
      editor.destroy();
    }
    return out;
  });
  expect(Object.keys(found).length).toBeGreaterThan(5);
  for (const [theme, contrast] of Object.entries(found)) expect(contrast, theme).toBeGreaterThanOrEqual(4.5);
});
