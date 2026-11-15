import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Ctrl+/ in every language: blank lines, block comments, block scalars, mixed selections, toggling back.

async function at(page, marked) {
  const e = editor(page);
  const position = marked.indexOf('|');
  await e.paste(marked.replace('|', ''));
  await e.input.evaluate((element, offset) => {
    element.focus();
    element.setSelectionRange(offset, offset);
  }, position);
  return e;
}

test.describe('yaml: editing', () => {
  setupEditor(test, '?language=yaml');
  test('Ctrl+/ on a blank line starts a comment', async ({ page }) => {
    const e = await at(page, 'a: 1\n|');
    await page.keyboard.press('Control+/');
    expect(await e.value()).toBe('a: 1\n# ');
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  async function at(page, marked) {
    const e = editor(page);
    const [start, end] = [marked.indexOf('¦'), marked.lastIndexOf('¦') - (marked.indexOf('¦') === marked.lastIndexOf('¦') ? 0 : 1)];
    await e.paste(marked.replaceAll('¦', ''));
    await e.input.evaluate(
      (element, [from, to]) => {
        element.focus();
        element.setSelectionRange(from, to);
      },
      [start, end],
    );
    return e;
  }


  test.describe('xml', () => {
    setupEditor(test, '?language=xml');
    test('Ctrl+/ twice on lines holding a comment gives them back, valid in between', async ({ page }) => {
      const text = '<a>x</a>\n<!-- c -->\n<b/>';
      const e = await at(page, `¦${text}¦`);
      await page.keyboard.press('Control+/');
      const once = await e.value();
      expect(once.match(/<!--/g)).toHaveLength(1);
      // One comment, and no `--` inside it (XML forbids it).
      expect(once.startsWith('<!-- ') && once.endsWith(' -->')).toBe(true);
      expect(once.slice(5, -4)).not.toContain('--');
      await page.keyboard.press('Control+/');
      expect(await e.value()).toBe(text);
    });
  });

  test.describe('hcl: Ctrl+/', () => {
    setupEditor(test, '?language=hcl');

  test('Ctrl+/ on a block-commented line removes the block comment', async ({ page }) => {
      const e = await at(page, '¦/* listen = 1 */');
    await page.keyboard.press('Control+/');
    expect(await e.value()).toBe('listen = 1');
  });
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  async function at(page, marked) {
    const e = editor(page);
    const first = marked.indexOf('¦');
    const last = marked.lastIndexOf('¦');
    const [start, end] = first === last ? [first, first] : [first, last - 1];
    await e.paste(marked.replaceAll('¦', ''));
    await e.input.evaluate(
      (element, [from, to]) => {
        element.focus();
        element.setSelectionRange(from, to);
      },
      [start, end],
    );
    return e;
  }

  const selection = (e) => e.input.evaluate((element) => [element.selectionStart, element.selectionEnd]);


  test.describe('xml', () => {
    setupEditor(test, '?language=xml');
    test('Ctrl+/ twice on a selection starting mid-line gives the text back', async ({ page }) => {
      const text = '<web>\n  <path>a</path>\n  <host>b</host>\n</web>\n';
      const e = await at(page, text.replace('a</path>', '¦a</path>').replace('b</host>', 'b¦</host>'));
      await page.keyboard.press('Control+/');
      await page.keyboard.press('Control+/');
      expect(await e.value()).toBe(text);
    });
  });

  test.describe('hcl', () => {
    setupEditor(test, '?language=hcl');

    test('Ctrl+/ on a block comment keeps the caret on its text', async ({ page }) => {
      const e = await at(page, 'tcp {\n  /* nodelay¦ = true */\n}\n');
      await page.keyboard.press('Control+/');
      expect(await e.value()).toBe('tcp {\n  nodelay = true\n}\n');
      const [start, end] = await selection(e);
      expect([start, end]).toEqual(['tcp {\n  nodelay'.length, 'tcp {\n  nodelay'.length]);
    });
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  async function at(page, marked) {
    const e = editor(page);
    const first = marked.indexOf('¦');
    const last = marked.lastIndexOf('¦');
    const [start, end] = first === last ? [first, first] : [first, last - 1];
    await e.paste(marked.replaceAll('¦', ''));
    await e.input.evaluate(
      (element, [from, to]) => {
        element.focus();
        element.setSelectionRange(from, to);
      },
      [start, end],
    );
    return e;
  }


  test.describe('yaml', () => {
    setupEditor(test, '?language=yaml');
    test('Ctrl+/ in a block scalar comments the whole entry and back', async ({ page }) => {
      const text = 'listen: 1\nnote: |\n  line1\n  line2\nbind: x\n';
      const e = await at(page, text.replace('line1', 'line1¦'));
      await page.keyboard.press('Control+/');
      const once = await e.value();
      expect(once).not.toContain('  # line1');
      expect(once).toContain('# note: |');
      await page.keyboard.press('Control+/');
      expect(await e.value()).toBe(text);
    });
  });

  test.describe('hcl', () => {
    setupEditor(test, '?language=hcl');
    test('Ctrl+/ inside a block comment over lines removes it', async ({ page }) => {
      const e = await at(page, 'tcp {\n  /* nodelay = true\n  keepalive_secs¦ = 30 */\n}\n');
      await page.keyboard.press('Control+/');
      expect(await e.value()).toBe('tcp {\n  nodelay = true\n  keepalive_secs = 30\n}\n');
    });
  });
});

test.describe('Ctrl+/ over a mixed XML selection', () => {
  setupEditor(test, '?language=xml');

  test('comments whole lines, stays well-formed, and toggles back exactly', async ({ page }) => {
    const e = editor(page);
    const text = '<r>\n<a>1</a>\n<!-- <b>2</b> -->\n<c>3</c>\n<n>1</n>\n</r>\n';
    await e.paste(text);
    await e.input.evaluate((element) => {
      const start = element.value.indexOf('<a>');
      const end = element.value.indexOf('3</c>') + 1;
      element.setSelectionRange(start, end);
    });
    await page.keyboard.press('Control+/');
    const once = await e.value();
    expect(once).toContain('<c>3</c> -->');
    expect(await page.evaluate((xml) => {
      const parsed = new DOMParser().parseFromString(xml, 'application/xml');
      return parsed.getElementsByTagName('parsererror').length;
    }, once)).toBe(0);
    await page.keyboard.press('Control+/');
    expect(await e.value()).toBe(text);
  });
});
