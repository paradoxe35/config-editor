import { expect, test } from '@playwright/test';
import { editor, pointAt, scrollToLine, setupEditor } from './helpers.mjs';

setupEditor(test);

const LARGE = `# Burrow server configuration
listen: 7835
public_address: tunnel.example.com
insecure_plaintext: false
heartbeat:
  interval_secs: 15
limits:
  handshake_timeout_secs: 10
  max_handshakes: 64
  max_connections_per_client: 32
  max_pending: 128
pool:
  min_idle: 2
  max_idle: 8
clients:
  paradoxe:
    secret: alpha-secret
    max_connections: 16
  laptop:
    secret: beta-secret
  nas:
    secret: gamma-secret
  ci-runner:
    secret: delta-secret
visitors:
  ops:
    secret: ops-secret
services:
  web-ui:
    target: 8080
    protocol: tcp
    mode: failover
  dns:
    target: 53
    protocol: udp
  backup:
    client: nas
    target: 22
    protocol: tcp
  api:
    target: 9000
    protocol: tcp
    clients:
      - paradoxe
      - name: laptop
        weight: 2
    mode: balance
web:
  http: 80
  https: 443
tcp:
  nodelay: true
`;

test.describe('large documents', () => {
  test('a 60-line document loads silently and reports its diagnostics', async ({ page }) => {
    const e = editor(page);
    await e.paste(LARGE);
    expect(await e.isOpen()).toBe(false);
    await expect(page.locator('.yaml-editor__gutter > div')).toHaveCount(LARGE.split('\n').length);
    await e.screenshot('large-document');
    expect(await e.diagnosticCount(), 'a valid document has no diagnostics').toBe(0);
  });

  test('a long document keeps the suggestion list inside the viewport near the bottom', async ({ page }) => {
    const e = editor(page);
    await e.paste(LARGE);
    await e.input.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.isOpen()).toBe(true);
    const box = await e.popup.boundingBox();
    const viewport = page.viewportSize();
    expect(box.y).toBeGreaterThanOrEqual(0);
    expect(box.y + box.height).toBeLessThanOrEqual(viewport.height + 1);
    await e.screenshot('suggestions-near-bottom');
  });
});

test.describe('scale', () => {
  test('300 client entries stay responsive while typing, suggesting and hovering', async ({ page }) => {
    const e = editor(page);
    const entries = Array.from({ length: 300 }, (_, i) => `  client-${i}:\n    secret: secret-${i}\n    max_connections: ${i % 50}`);
    const doc = `clients:\n${entries.join('\n')}\n`;
    await e.paste(doc);
    await expect(page.locator('.yaml-editor__gutter > div')).toHaveCount(doc.split('\n').length);
    await e.input.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Enter');
    const started = Date.now();
    await page.keyboard.type('  client-new:', { delay: 0 });
    await page.keyboard.press('Enter');
    await page.keyboard.type('sec');
    await expect.poll(() => e.keys(), { timeout: 5000 }).toContain('secret');
    const typing = Date.now() - started;
    console.log(`scale: typed and suggested in ${typing} ms for ${doc.split('\n').length} lines`);
    expect(typing, 'typing 20 characters in a 900-line document stays under 8 s').toBeLessThan(8000);
    await e.screenshot('scale-300-clients');
  });
});

test.describe('schema-driven completion', () => {
  test('enum values are offered and filtered as the user types', async ({ page }) => {
    const e = editor(page);
    await e.paste('services:\n  dns:\n    target: 53\n    protocol: ');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toEqual(['tcp', 'udp']);
    await page.keyboard.type('u');
    await expect.poll(() => e.keys()).toEqual(['udp']);
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('services:\n  dns:\n    target: 53\n    protocol: udp');
    expect(await e.isOpen()).toBe(false);
  });

  test('initials match enum values such as least_connections', async ({ page }) => {
    const e = editor(page);
    await e.paste('services:\n  api:\n    mode: ');
    await page.keyboard.type('lc');
    await expect.poll(() => e.keys()).toContain('least_connections');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('services:\n  api:\n    mode: least_connections');
  });

  test('keys already present in a mapping are not suggested again', async ({ page }) => {
    const e = editor(page);
    await e.paste('clients:\n  paradoxe:\n    secret: x\n    ');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toEqual(['max_connections']);
  });

  test('a partially typed key is replaced, not appended to', async ({ page }) => {
    const e = editor(page);
    await e.paste('services:\n  api:\n    ');
    await page.keyboard.type('prot');
    await expect.poll(() => e.keys()).toContain('protocol');
    await e.clickKey('protocol');
    await expect(e.input).toHaveValue('services:\n  api:\n    protocol: ');
  });

  test('a service entry with required target suggests the target key', async ({ page }) => {
    const e = editor(page);
    await e.paste('services:\n  web-ui:\n    ');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toEqual(expect.arrayContaining(['target', 'protocol', 'mode']));
    expect((await e.keys())[0], 'required target ranks first').toBe('target');
    await e.screenshot('service-keys');
  });

  test('a dynamic client name gets its own key suggestions', async ({ page }) => {
    const e = editor(page);
    await e.paste('clients:\n  ci-runner:\n    ');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toEqual(expect.arrayContaining(['secret', 'max_connections']));
  });

  test('array items suggest client references', async ({ page }) => {
    const e = editor(page);
    await e.paste('services:\n  api:\n    clients:\n      - ');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.isOpen()).toBe(true);
    await e.screenshot('array-item-suggestions');
  });

  test('a document with no matching schema path offers nothing', async ({ page }) => {
    const e = editor(page);
    await e.paste('unknown_section:\n  ');
    await page.keyboard.press('Control+Space');
    await page.waitForTimeout(150);
    expect(await e.isOpen()).toBe(false);
  });
});

test.describe('update.remote', () => {
  test('typing remote: and accepting true keeps the value on the same line', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('update:');
    await page.keyboard.press('Enter');
    await page.keyboard.type('remote:');
    await expect.poll(() => e.keys()).toEqual(expect.arrayContaining(['true', 'false']));
    await e.clickKey('true');
    await expect(e.input).toHaveValue('update:\n  remote: true');
    expect(await e.isOpen()).toBe(false);
  });

  test('keyboard acceptance of remote and true stays inline', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('update:');
    await page.keyboard.press('Enter');
    await page.keyboard.type('remote');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('update:\n  remote: ');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('update:\n  remote: true');
  });

});

test.describe('required expansion', () => {
  test('metrics expands listen and the example value stays on the listen line', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('met');
    await e.clickKey('metrics');
    await expect(e.input).toHaveValue('metrics:\n  listen: \n');
    await expect.poll(() => e.keys()).toContain('0.0.0.0:7835');
    await e.clickKey('0.0.0.0:7835');
    await expect(e.input).toHaveValue('metrics:\n  listen: "0.0.0.0:7835"\n');
  });
});

test.describe('diagnostics lines', () => {
  test('a nested key error lands on its own line, not the first key with the same name', async ({ page }) => {
    const e = editor(page);
    await e.paste(`listen: 7835
public_address: tunnel.example.com
insecure_plaintext: false
heartbeat:
  interval_secs: 15
limits:
  max_handshakes: 64
pool:
  min_idle: 2
websocket:
  listen: 0
`);
    const marker = page.locator('.yaml-editor__diagnostic-marker').first();
    await expect(marker).toHaveCount(1);
    await expect(marker).toHaveAttribute('aria-label', /line 11/);
    await expect(page.locator('.yaml-editor__line-number-error')).toHaveText(['11']);
    await e.screenshot('diagnostic-nested-line');
  });
});

test.describe('gutter and caret', () => {
  test('clicking a line number moves the caret to that line', async ({ page }) => {
    const e = editor(page);
    await e.paste('alpha: 1\n\nbeta: 2\ngamma: 3\n');
    await page.locator('.yaml-editor__gutter > div').nth(2).click();
    const state = await e.input.evaluate((element) => {
      const position = element.selectionStart;
      const before = element.value.slice(0, position);
      return { position, line: before.split('\n').length, column: position - (before.lastIndexOf('\n') + 1) };
    });
    expect(state).toEqual({ position: 10, line: 3, column: 0 });
  });

  test('the caret color follows every theme', async ({ page }) => {
    const e = editor(page);
    const select = page.locator('.yaml-editor__demo-theme-select');
    const caret = () => e.input.evaluate((element) => getComputedStyle(element).caretColor);
    await select.selectOption('vscode-dark');
    expect(await caret()).toBe('rgb(174, 175, 173)');
    await select.selectOption('github-light');
    expect(await caret()).toBe('rgb(31, 35, 40)');
    await select.selectOption('dracula');
    expect(await caret()).toBe('rgb(248, 248, 242)');
  });
});

test.describe('editing behaviour', () => {
  test('Ctrl+Z undoes an accepted completion and Ctrl+Shift+Z redoes it', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('lis');
    await e.clickKey('listen');
    await e.clickKey('7835');
    await expect(e.input).toHaveValue('listen: 7835');
    await page.keyboard.press('Control+Z');
    await expect(e.input).toHaveValue('listen: ');
    await page.keyboard.press('Control+Shift+Z');
    await expect(e.input).toHaveValue('listen: 7835');
  });

  test('Tab indents selected lines and Shift+Tab outdents them', async ({ page }) => {
    const e = editor(page);
    await e.paste('a: 1\nb: 2\n');
    await e.input.click();
    await page.keyboard.press('Control+A');
    await page.keyboard.press('Tab');
    await expect(e.input).toHaveValue('  a: 1\n  b: 2\n');
    await page.keyboard.press('Shift+Tab');
    await expect(e.input).toHaveValue('a: 1\nb: 2\n');
  });

  test('Ctrl+/ comments and uncomments the current line', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: 7835');
    await e.input.click();
    await page.keyboard.press('Control+/');
    await expect(e.input).toHaveValue('# listen: 7835');
    await page.keyboard.press('Control+/');
    await expect(e.input).toHaveValue('listen: 7835');
  });

  test('quotes and brackets are paired and typing the closer steps over', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('secret: "ab');
    await expect(e.input).toHaveValue('secret: "ab"');
    await page.keyboard.type('"');
    await expect(e.input).toHaveValue('secret: "ab"');
    await page.keyboard.press('ArrowLeft');
    await page.keyboard.type('x');
    await expect(e.input).toHaveValue('secret: "abx"');
  });

  test('Enter after a sequence item continues the list with a dash', async ({ page }) => {
    const e = editor(page);
    await e.paste('clients:\n  - paradoxe');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('clients:\n  - paradoxe\n  - ');
  });

  test('paste never opens suggestions', async ({ page }) => {
    const e = editor(page);
    await e.paste('services:\n  api:\n    pro');
    await page.waitForTimeout(150);
    expect(await e.isOpen()).toBe(false);
  });
});

test.describe('diagnostics', () => {
  test('an unknown root key is reported and removing it clears the report', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: 7835\nbogus_key: 1\n');
    await expect.poll(() => e.diagnosticCount()).toBeGreaterThan(0);
    await e.screenshot('unknown-key-diagnostic');
    await e.paste('listen: 7835\n');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });

  test('hovering a diagnostic marker shows its message', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: 7835\nbogus_key: 1\n');
    const marker = page.locator('.yaml-editor__diagnostic-marker').first();
    await expect(marker).toHaveCount(1);
    await marker.hover();
    await expect(marker).toHaveAttribute('data-message', /bogus_key|bogus/);
    await e.screenshot('diagnostic-tooltip');
  });

  test('broken YAML is marked with a syntax error', async ({ page }) => {
    const e = editor(page);
    await e.paste('web:\n  http: [80\n');
    await expect.poll(() => e.diagnosticCount()).toBeGreaterThan(0);
  });
});

test.describe('diff gutter and apply', () => {
  test('a new line is marked as added and Apply changes clears the marker', async ({ page }) => {
    const e = editor(page);
    await page.waitForTimeout(150);
    const initial = await e.value();
    await e.input.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Enter');
    await page.keyboard.type('insecure_plaintext: true');
    await expect(page.locator('.yaml-editor__diff-marker-added')).toHaveCount(1);
    await e.screenshot('diff-added');
    await page.locator('.yaml-editor__action-button').click();
    await expect(page.locator('.yaml-editor__diff-marker-added')).toHaveCount(0);
    expect(initial).not.toEqual(await e.value());
  });
});

test.describe('themes', () => {
  test('every theme keeps the same editor font size', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: 7835\n');
    const select = page.locator('.yaml-editor__demo-theme-select');
    const values = await select.locator('option').evaluateAll((options) => options.map((o) => o.value));
    const sizes = new Set();
    for (const theme of values) {
      await select.selectOption(theme);
      await page.waitForTimeout(80);
      sizes.add(await e.input.evaluate((el) => getComputedStyle(el).fontSize));
    }
    expect([...sizes]).toEqual(['12px']);
  });

  test('the suggestion list is readable in light and dark themes', async ({ page }) => {
    const e = editor(page);
    const select = page.locator('.yaml-editor__demo-theme-select');
    await e.typeDoc('lis');
    await e.screenshot('theme-dark-suggestions');
    await select.selectOption('vscode-light');
    await page.waitForTimeout(80);
    await expect(e.popup).toBeVisible();
    await e.screenshot('theme-light-suggestions');
  });
});

test.describe('hover on nested and dynamic keys', () => {
  test('hovering a dynamic client key reports its schema fields', async ({ page }) => {
    const e = editor(page);
    await e.paste(LARGE);
    const card = page.locator('.yaml-editor__hover');
    const line = LARGE.split('\n').findIndex((l) => l === '    secret: alpha-secret');
    await scrollToLine(page, line);
    const p = await pointAt(page, line, 6);
    await page.mouse.move(p.x, p.y, { steps: 4 });
    await expect(card).toHaveCount(1);
    await expect(card.locator('.yaml-editor__hover-key')).toHaveText('secret');
    await e.screenshot('hover-dynamic-secret');
  });

  test('hovering a key inside a sequence item reports its schema', async ({ page }) => {
    const e = editor(page);
    await e.paste(LARGE);
    const card = page.locator('.yaml-editor__hover');
    const line = LARGE.split('\n').findIndex((l) => l === '        weight: 2');
    await scrollToLine(page, line);
    const p = await pointAt(page, line, 10);
    await page.mouse.move(p.x, p.y, { steps: 4 });
    await expect(card).toHaveCount(1);
    await expect(card.locator('.yaml-editor__hover-key')).toHaveText('weight');
  });
});
