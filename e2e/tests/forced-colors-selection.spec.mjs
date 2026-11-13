import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Under forced colours the editor's selection and find field stay usable in
// both system schemes: the selected code keeps 4.5:1 (hard-bar `D3.a11y-1`:
// the overlay's CanvasText over the system Highlight was 2.07:1 light,
// 2.62:1 dark) and the find field's focus shows (`D18.a11y-2`: its border was
// CanvasText focused and not).

test.use({ forcedColors: 'active' });

/** A crop's most common pixel: the colour a selection paints under its text. */
async function paintedBackground(page, buffer) {
  return page.evaluate(async (base64) => {
    const bitmap = await createImageBitmap(await (await fetch(`data:image/png;base64,${base64}`)).blob());
    const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
    const context = canvas.getContext('2d');
    context.drawImage(bitmap, 0, 0);
    const { data } = context.getImageData(0, 0, bitmap.width, bitmap.height);
    const counts = new Map();
    for (let i = 0; i < data.length; i += 4) {
      const key = `${data[i]},${data[i + 1]},${data[i + 2]}`;
      counts.set(key, (counts.get(key) || 0) + 1);
    }
    return [...counts.entries()]
      .sort((a, b) => b[1] - a[1])[0][0]
      .split(',')
      .map(Number);
  }, buffer.toString('base64'));
}

const luminance = (rgb) =>
  rgb
    .map((v) => ((v /= 255) <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4))
    .reduce((sum, v, i) => sum + v * [0.2126, 0.7152, 0.0722][i], 0);
const ratio = (a, b) => {
  const [high, low] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (high + 0.05) / (low + 0.05);
};

for (const scheme of ['light', 'dark']) {
  test.describe(`${scheme} system scheme`, () => {
    test.use({ colorScheme: scheme });
    setupEditor(test, '?sample=large');

    test(`selected code keeps 4.5:1 (D3.a11y-1, ${scheme})`, async ({ page }) => {
      const e = editor(page);
      await e.input.click();
      await page.keyboard.press('Control+Home');
      await page.keyboard.press('ArrowDown');
      await page.keyboard.press('Shift+End');
      const line = page.locator('.config-editor__code-line').nth(1);
      const box = await line.boundingBox();
      const text = await line
        .locator('span')
        .first()
        .evaluate((element) =>
          getComputedStyle(element)
            .color.match(/[\d.]+/g)
            .slice(0, 3)
            .map(Number),
        );
      const crop = await page.screenshot({ clip: { x: box.x + 3, y: box.y + 3, width: 40, height: box.height - 6 } });
      expect(ratio(text, await paintedBackground(page, crop))).toBeGreaterThanOrEqual(4.5);
    });

    test(`the focused find field differs from the blurred one (D18.a11y-2, ${scheme})`, async ({ page }) => {
      const e = editor(page);
      await e.input.click();
      await page.keyboard.press('Control+f');
      const field = page.locator('.config-editor__find-field');
      const paint = () =>
        field.evaluate((element) => {
          const style = getComputedStyle(element);
          return [style.borderTopColor, style.outlineColor, style.outlineStyle, style.outlineWidth].join(' ');
        });
      await expect(page.getByRole('textbox', { name: 'Find', exact: true })).toBeFocused();
      const focused = await paint();
      await page.getByRole('button', { name: 'Close', exact: true }).first().focus();
      expect(focused).not.toEqual(await paint());
    });
  });
}
