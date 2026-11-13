import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// The find, replace and go-to-line fields keep their own size under a
// host's rules for every `input` (a framework's
// `width: 100%; display: block; margin: 0 0 16px; box-sizing: content-box; padding; font-size`): the widget's controls
// stay inside the editor, not pushed past its edge.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const VANILLA = pathToFileURL(join(js, 'examples', 'vanilla.html')).href;

const HOST_RULES = `
  button { padding: 6px 12px; border: 1px solid #ccc; background: #eee; color: #000; border-radius: 4px; font-size: 16px; line-height: 1.5; }
  input, textarea { padding: 6px 12px; font-size: 16px; line-height: 1.5; border: 1px solid #888; border-radius: 4px; width: 100%; display: block; margin: 0 0 16px; box-sizing: content-box; background: #fff; color: #000; }
  body { line-height: 1.6; font-size: 16px; }`;

/** Opens the widget with `keys` over an editor under `hostRules`; reports which of its elements stick out of the editor and how high its inputs are. */
async function layout(page, keys, widget, hostRules, width) {
  await page.goto(VANILLA);
  await page.waitForFunction(() => document.body.dataset.ready === 'true');
  await page.evaluate(async (width) => {
    const CE = await import(new URL('../config-editor/index.js', location.href).href);
    const host = document.createElement('div');
    host.id = 'styled';
    host.style.cssText = `height:300px;width:${width}px`;
    document.body.prepend(host);
    window.styled = new CE.ConfigEditor(host, { language: 'yaml', value: 'server:\n  host: a\n' });
  }, width);
  if (hostRules) await page.addStyleTag({ content: hostRules });
  await page.locator('#styled .config-editor__input').click();
  for (const key of keys) await page.keyboard.press(key);
  const root = page.locator(`#styled ${widget}`);
  await expect(root).toBeVisible();
  return root.evaluate((element) => {
    const edge = document.getElementById('styled').getBoundingClientRect();
    const outside = [element, ...element.querySelectorAll('*')]
      .filter((child) => child.getClientRects().length)
      .filter((child) => {
        const rect = child.getBoundingClientRect();
        return rect.right > edge.right + 0.5 || rect.left < edge.left - 0.5;
      })
      .map((child) => String(child.className || child.tagName));
    const heights = [...element.querySelectorAll('input')].map((input) => input.getBoundingClientRect().height);
    return { outside, heights, box: element.getBoundingClientRect().height };
  });
}

test.describe('the find widget under a host stylesheet', () => {
  test.skip(!built, 'the npm package is not built: cd js && npm run build');

  for (const [name, keys, widget] of [
    ['find', ['Control+f'], '.config-editor__find'],
    ['replace', ['Control+h'], '.config-editor__find'],
    ['go to line', ['Control+g'], '.config-editor__goto'],
  ]) {
    for (const width of [240, 360, 600]) {
      test(`${name} stays inside an editor ${width} px wide under rules for every input`, async ({ page }) => {
        const plain = await layout(page, keys, widget, '', width);
        const styled = await layout(page, keys, widget, HOST_RULES, width);
        expect(styled.outside).toEqual([]);
        // Its fields are as high as without the host's rules.
        expect(styled.heights).toEqual(plain.heights);
        // ...and the widget as high as without them (no margins added).
        expect(styled.box).toBe(plain.box);
      });
    }
  }
});
