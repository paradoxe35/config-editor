import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// QA round 18 Edit-as saves, typed in another language's view: an
// indentless YAML sequence and a quoted YAML scalar over lines are spliced
// in place with the comments kept, a renamed YAML key YAML 1.1 would read as
// a boolean is quoted, and a TOML datetime typed half-way waits and is
// saved as a datetime once it is one again.

const saved = (page) => page.locator('.demo__output-text').textContent();
const pick = (page, value) => page.locator('.demo__edit-select').selectOption(value);

// Puts the caret just after `needle` (its `offset`-th character with one).
async function caretAfter(e, needle, offset = needle.length) {
  await e.input.evaluate(
    (element, [find, at]) => {
      const start = element.value.indexOf(find);
      if (start < 0) throw new Error(`no ${find}`);
      element.focus();
      element.setSelectionRange(start + at, start + at);
    },
    [needle, offset],
  );
}

test.describe('YAML edited as JSON', () => {
  setupEditor(test, '?language=yaml');

  test('an indentless sequence and a quoted scalar over lines keep their lines', async ({ page }) => {
    const e = editor(page);
    const host = '# top\nl:\n- k: >-\n    x\n  j: 2 # jc\n- - a # ac\n  - b\nml: "l1\n  l2"  # mc\nn: 1  # nc\n';
    await e.paste(host);
    await expect.poll(() => saved(page)).toContain('n: 1');
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"b"');
    await caretAfter(e, '"x', 2);
    await page.keyboard.type('Q');
    await expect.poll(() => saved(page)).toBe(host.replace('    x\n', '    xQ\n'));
    await caretAfter(e, '"b', 2);
    await page.keyboard.type('Z');
    await expect.poll(() => saved(page)).toContain('  - bZ\nml:');
    await caretAfter(e, 'l2', 2);
    await page.keyboard.type('Y');
    await expect.poll(() => saved(page)).toBe(
      host.replace('    x\n', '    xQ\n').replace('  - b\n', '  - bZ\n').replace('ml: "l1\n  l2"', 'ml: "l1\n  l2Y"'),
    );
  });

  test('a key renamed to on is written quoted, keeping the spacing before its comment', async ({ page }) => {
    const e = editor(page);
    await e.paste('# top\nport: 1  # pc\nn: 1\n');
    await expect.poll(() => saved(page)).toContain('n: 1');
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"port"');
    await caretAfter(e, '"port', 1);
    for (let i = 0; i < 4; i++) await page.keyboard.press('Shift+ArrowRight');
    await page.keyboard.type('on');
    await expect.poll(() => saved(page)).toBe('# top\n"on": 1  # pc\nn: 1\n');
  });
});

test.describe('TOML edited as JSON', () => {
  setupEditor(test, '?language=toml');

  test('a datetime typed half-way waits, then is saved as a datetime', async ({ page }) => {
    const e = editor(page);
    const host = '# c\ndt = 1979-05-27T07:32:00Z  # dtc\nn = 1\n';
    await e.paste(host);
    await expect.poll(() => saved(page)).toContain('n = 1');
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('07:32:00Z');
    await caretAfter(e, '07:32:00', 8);
    await page.keyboard.press('Backspace');
    await expect(page.locator('.demo')).toContainText('not a TOML date or time');
    expect(await saved(page)).toBe(host);
    await page.keyboard.type('1');
    await expect.poll(() => saved(page)).toBe(host.replace('07:32:00Z', '07:32:01Z'));
  });
});
