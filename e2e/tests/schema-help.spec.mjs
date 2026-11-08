import { expect, test } from '@playwright/test';
import { editor, pointAt, setupEditor } from './helpers.mjs';

// Richer schema help against the schema-help fixture (`?schema=help`):
// deprecated keys, docs links, documented values, required children filled
// from examples with Tab stops, and a pattern the regex engine cannot read.

async function hover(page, line, text, offset = 1) {
  const lineText = (await editor(page).value()).split('\n')[line];
  const at = await pointAt(page, line, lineText.indexOf(text) + offset);
  await page.mouse.move(at.x, at.y, { steps: 4 });
  return page.locator('.config-editor__hover');
}

const selected = (page) =>
  page
    .locator('.config-editor__input')
    .evaluate((element) => element.value.slice(element.selectionStart, element.selectionEnd));

test.describe('schema help', () => {
  setupEditor(test, '?schema=help');

  test('deprecated keys are struck through and listed last', async ({ page }) => {
    const ed = editor(page);
    await ed.input.click();
    await page.keyboard.press('Control+Space');
    await expect.poll(ed.keys).toContain('timeout');
    const keys = await ed.keys();
    expect(keys.at(-1)).toBe('timeout');
    const row = ed.options.filter({ hasText: 'timeout' });
    await expect(row).toHaveClass(/config-editor__completion--deprecated/);
    await expect(row.locator('.config-editor__completion-deprecated')).toHaveText('deprecated');
    const decoration = await row
      .locator('.config-editor__completion-key')
      .evaluate((element) => getComputedStyle(element).textDecorationLine);
    expect(decoration).toBe('line-through');
    await expect(ed.options.filter({ hasText: 'deadline' })).not.toHaveClass(/deprecated/);
  });

  test('values show their own descriptions in the list', async ({ page }) => {
    const ed = editor(page);
    await ed.typeDoc('level: ');
    await page.keyboard.press('Control+Space');
    await expect.poll(ed.keys).toContain('trace');
    await expect(ed.options.filter({ hasText: 'trace' }).locator('.config-editor__completion-detail')).toHaveText(
      'Trace — Everything, very verbose.',
    );
  });

  test('hovering a value shows its description and a docs link that opens a new tab', async ({ page }) => {
    await editor(page).paste('mode: fast\n');
    const card = await hover(page, 0, 'fast');
    await expect(card).toContainText('Skips the checks.');
    await expect(card.locator('.config-editor__hover-value-label')).toHaveText('fast');
    const link = card.locator('.config-editor__hover-link');
    await expect(link).toHaveAttribute('href', 'https://example.com/docs/mode');
    await expect(link).toHaveAttribute('target', '_blank');
    await expect(link).toHaveAttribute('rel', /noopener/);
    // The pointer can travel to the link without the card closing.
    await page.context().route('https://example.com/**', (route) =>
      route.fulfill({ contentType: 'text/html', body: '<p>docs</p>' }),
    );
    const box = await link.boundingBox();
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2, { steps: 8 });
    await expect(link).toBeVisible();
    const [opened] = await Promise.all([page.context().waitForEvent('page'), link.click()]);
    expect(opened.url()).toBe('https://example.com/docs/mode');
    await opened.close();
  });

  test('a deprecated key in the document is warned about, with what to use instead', async ({ page }) => {
    await editor(page).paste('timeout: 3\ndeadline: 3\n');
    await expect(page.locator('.config-editor__diagnostic-marker')).toHaveCount(1);
    await expect(page.locator('.config-editor__status')).toContainText('1 warning');
    // The underlined key's hover is the warning, which says it all.
    const marker = page.locator('.config-editor__diagnostic-marker');
    await expect(marker).toHaveClass(/config-editor__diagnostic-marker-warning/);
    await expect(marker).toHaveAttribute('data-message', '/timeout: "timeout" is deprecated. Use deadline instead.');
  });

  test('a required block takes examples and Tab selects each written value', async ({ page }) => {
    const ed = editor(page);
    await ed.typeDoc('serv');
    await expect.poll(ed.keys).toContain('server');
    await page.keyboard.press('Enter');
    await expect.poll(ed.value).toBe('server:\n  host: \n  port: 8080\n  scheme: https\n');
    await page.keyboard.press('Escape');
    await page.keyboard.type('a.example');
    await page.keyboard.press('Tab');
    expect(await selected(page)).toBe('8080');
    await page.keyboard.type('9090');
    await page.keyboard.press('Tab');
    expect(await selected(page)).toBe('https');
    await page.keyboard.type('http');
    await expect
      .poll(ed.value)
      .toBe('server:\n  host: a.example\n  port: 9090\n  scheme: http\n');
  });

  test('hovering an object with oneOf options names the one it is in', async ({ page }) => {
    const ed = editor(page);
    await ed.paste('dns:\n  cloudflare:\n    zone: example.com\n');
    let card = await hover(page, 0, 'dns');
    await expect(card.locator('.config-editor__hover-choice-title')).toHaveText('Cloudflare');
    await expect(card.locator('.config-editor__hover-choice')).toContainText('Records in a Cloudflare zone.');
    await expect(card.locator('.config-editor__hover-description').first()).toHaveText(
      'Where the DNS records are managed.',
    );
    // Two options require `provider`: its value decides.
    await page.mouse.move(0, 0);
    await expect(card).toHaveCount(0);
    await ed.paste('dns:\n  provider: static\n');
    card = await hover(page, 0, 'dns');
    await expect(card.locator('.config-editor__hover-choice-title')).toHaveText('Static file');
    // (Before one decides the object is invalid, and its problem takes
    // the hover: the "One of" row is covered by the unit matrix.)
  });

  test('keys inside a oneOf object name their option, the matched one first', async ({ page }) => {
    const ed = editor(page);
    await ed.paste('dns:\n  provider: static\n  ');
    await page.keyboard.press('Control+Space');
    await expect.poll(ed.keys).toContain('path');
    expect((await ed.keys())[0]).toBe('path');
    await expect(ed.options.filter({ hasText: 'path' }).locator('.config-editor__completion-detail')).toHaveText(
      'Static file',
    );
    await expect(ed.options.filter({ hasText: 'route53' }).locator('.config-editor__completion-detail')).toHaveText(
      'Route 53',
    );
  });

  test('a pattern the regex engine cannot read reaches the host once', async ({ page }) => {
    const warnings = page.locator('.demo__schema-warnings li');
    await expect(warnings).toHaveCount(1);
    await expect(warnings).toContainText('/properties/labels/patternProperties');
  });
});
