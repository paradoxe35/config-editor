import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Sixth-round findings, replayed with real keys.

async function select(page, text, from, to) {
  const e = editor(page);
  await e.paste(text);
  await e.input.evaluate(
    (element, [start, end]) => {
      element.focus();
      element.setSelectionRange(start, end);
    },
    [from, to],
  );
  return e;
}

test.describe('Tab then Shift+Tab puts lines back', () => {
  for (const [language, text, first] of [
    ['yaml', 'x:\n  a: 1\n  b: 2', '  a: 1'],
    ['json', '{\n  "a": 1,\n  "b": 2\n}', '  "a": 1,'],
  ]) {
    test.describe(language, () => {
      setupEditor(test, `?language=${language}`);

      test('a selection of indented lines', async ({ page }) => {
        const start = text.indexOf(first);
        const end = language === 'yaml' ? text.length : text.indexOf('\n}');
        const e = await select(page, text, start, end);
        await page.keyboard.press('Tab');
        await page.keyboard.press('Shift+Tab');
        expect(await e.value()).toBe(text);
      });

      test('the caret on an indented line', async ({ page }) => {
        const at = text.indexOf(first) + 2;
        const e = await select(page, text, at, at);
        await page.keyboard.press('Tab');
        await page.keyboard.press('Shift+Tab');
        expect(await e.value()).toBe(text);
      });
    });
  }
});

test.describe('Ctrl+Delete at the end of a line joins it with the next', () => {
  setupEditor(test, '?language=yaml');

  test('without deleting the next line’s first word', async ({ page }) => {
    const text = 'a: 1\n  bb: 2';
    const e = await select(page, text, 4, 4);
    await page.keyboard.press('Control+Delete');
    expect(await e.value()).toBe('a: 1bb: 2');
  });
});

test.describe('xml: a typed end tag lines up with its opening tag', () => {
  setupEditor(test, '?language=xml');

  for (const [name, text] of [
    ['after Enter on a text line', '<r>\n  <a>text\n  '],
    ['after an opening tag with attributes', '<r>\n  <a x="1">\n  '],
    ['at column 0 of a blank line', '<r>\n  <a>\n'],
    ['on a deeper line', '<r>\n  <a>\n        '],
  ]) {
    test(name, async ({ page }) => {
      const e = await select(page, text, text.length, text.length);
      await page.keyboard.type('</a>');
      const lines = (await e.value()).split('\n');
      expect(lines[lines.length - 1]).toBe('  </a>');
    });
  }
});

async function at(page, marked) {
  const position = marked.indexOf('¦');
  return select(page, marked.replace('¦', ''), position, position);
}

test.describe('completion: round 6', () => {
  test.describe('yaml', () => {
    setupEditor(test, '?language=yaml');

    test('ArrowDown after typing moves the caret and keeps the list closed', async ({ page }) => {
      const e = await at(page, 'tcp:\n  nodelay: ¦\n  keepalive_secs: 3\n');
      await page.keyboard.type('yes');
      await page.keyboard.press('ArrowDown');
      await page.waitForTimeout(200);
      expect(await e.isOpen()).toBe(false);
      expect((await e.caret()).line).toBe(3);
    });

    test('an item under a mouse that has not moved is not preselected', async ({ page }) => {
      const e = await at(page, 'public_address: ¦\n');
      const box = await e.input.boundingBox();
      // Where the list opens, below the first line.
      await page.mouse.move(box.x + 140, box.y + 110);
      await page.keyboard.press('Control+Space');
      await expect.poll(() => e.isOpen()).toBe(true);
      await page.waitForTimeout(200);
      const active = await page.locator('.config-editor__completion').evaluateAll((items) =>
        items.findIndex((item) => item.classList.contains('config-editor__completion-active')),
      );
      expect(active).toBe(0);
    });
  });

  test.describe('toml', () => {
    setupEditor(test, '?language=toml');

    test('Enter on a typed key that matches its only suggestion keeps the value', async ({ page }) => {
      const e = await at(page, '[tcp]\nnodela¦ = true\n');
      await page.keyboard.type('y');
      await expect.poll(() => e.keys()).toEqual(['nodelay']);
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe('[tcp]\nnodelay = true\n');
    });
  });

  test.describe('json', () => {
    setupEditor(test, '?language=json');

    test('typing over the closing quote closes the value list', async ({ page }) => {
      const e = await at(page, '{\n  ¦\n}');
      await page.keyboard.type('"public_a');
      await expect.poll(() => e.keys()).toContain('public_address');
      await page.keyboard.press('Enter');
      await page.keyboard.type('"');
      await page.waitForTimeout(150);
      expect(await e.isOpen()).toBe(false);
    });
  });

  test.describe('xml', () => {
    setupEditor(test, '?language=xml');

    test('a key typed in front of an attribute is inserted before it', async ({ page }) => {
      const e = await at(page, '<tcp ¦nodelay="true"/>');
      await page.keyboard.type('keep');
      await expect.poll(() => e.keys()).toContain('keepalive_secs');
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe('<tcp keepalive_secs="30" nodelay="true"/>');
    });
  });

  test.describe('hcl', () => {
    setupEditor(test, '?language=hcl');

    test('a block label gets no key suggestions', async ({ page }) => {
      const e = await at(page, 'services "¦" {\n}\n');
      await page.keyboard.press('Control+Space');
      await page.waitForTimeout(200);
      expect(await e.isOpen()).toBe(false);
    });
  });
});

test.describe('diagnostics: round 6', () => {
  setupEditor(test, '?language=yaml');

  test('F8 shows the message of the problem it moves to, until the next key', async ({ page }) => {
    const e = await at(page, '¦listen: 1\nbogus: 2\n');
    const marker = page.locator('.config-editor__diagnostic-marker').first();
    await expect(marker).toBeVisible();
    const shown = () => marker.evaluate((element) => getComputedStyle(element, '::after').display);
    expect(await shown()).toBe('none');
    await page.keyboard.press('F8');
    await expect.poll(shown).toBe('block');
    expect((await e.caret()).line).toBe(2);
    await page.keyboard.press('ArrowRight');
    await expect.poll(shown).toBe('none');
  });
});
