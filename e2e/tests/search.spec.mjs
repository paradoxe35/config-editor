import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Find and replace (Ctrl+F, Ctrl+H) with real keys: the widget, match
// highlights, stepping, options, replacing through the normal edit path,
// read-only, narrow screens and a 10k-line document.

const DOC = 'port: 1\nPort: 2\nexport: 3\nport_name: x\nlast: port\n';

function search(page) {
  const widget = page.locator('.config-editor__find');
  return {
    widget,
    query: widget.getByRole('textbox', { name: 'Find' }),
    replacement: widget.getByRole('textbox', { name: 'Replace' }),
    count: widget.locator('.config-editor__find-count'),
    matches: page.locator('.config-editor__search-match'),
    current: page.locator('.config-editor__search-match-current'),
    toggle: (name) => widget.getByRole('button', { name }),
  };
}

// The selected text and where it starts.
const selection = (e) =>
  e.input.evaluate((element) => ({
    text: element.value.slice(element.selectionStart, element.selectionEnd),
    start: element.selectionStart,
  }));

const focusedLabel = (page) =>
  page.evaluate(() => document.activeElement?.getAttribute('aria-label') ?? document.activeElement?.className);

test.describe('find', () => {
  setupEditor(test);

  test('Ctrl+F opens the widget, highlights matches and steps through them', async ({ page }) => {
    const e = editor(page);
    const s = search(page);
    await e.paste(DOC);
    await expect(s.widget).toBeHidden();
    await e.input.evaluate((element) => element.setSelectionRange(0, 0));
    await page.keyboard.press('Control+f');
    await expect(s.widget).toBeVisible();
    await expect(s.query).toBeFocused();
    await page.keyboard.type('port');
    await expect(s.count).toHaveText('1 of 5');
    await expect(s.matches).toHaveCount(5);
    await expect(s.current).toHaveCount(1);
    expect(await selection(e)).toEqual({ text: 'port', start: 0 });

    await page.keyboard.press('Enter');
    await expect(s.count).toHaveText('2 of 5');
    expect(await selection(e)).toEqual({ text: 'Port', start: 8 });
    await page.keyboard.press('Shift+Enter');
    await expect(s.count).toHaveText('1 of 5');
    await page.keyboard.press('Shift+Enter');
    await expect(s.count).toHaveText('5 of 5');
    await page.keyboard.press('F3');
    await expect(s.count).toHaveText('1 of 5');
    await page.keyboard.press('Shift+F3');
    await expect(s.count).toHaveText('5 of 5');
    // The query field keeps focus while stepping.
    await expect(s.query).toBeFocused();
  });

  test('Escape closes the widget and returns focus to the text, match selected', async ({ page }) => {
    const e = editor(page);
    const s = search(page);
    await e.paste(DOC);
    await e.input.evaluate((element) => element.setSelectionRange(0, 0));
    await page.keyboard.press('Control+f');
    await page.keyboard.type('export');
    await page.keyboard.press('Escape');
    await expect(s.widget).toBeHidden();
    await expect(e.input).toBeFocused();
    await expect(s.matches).toHaveCount(0);
    expect(await selection(e)).toEqual({ text: 'export', start: 16 });
    // Typing goes to the text again, replacing the selected match.
    await page.keyboard.type('import');
    expect(await e.value()).toContain('import: 3');
  });

  test('Escape in the text closes the widget too', async ({ page }) => {
    const e = editor(page);
    const s = search(page);
    await e.paste(DOC);
    await page.keyboard.press('Control+f');
    await page.keyboard.type('port');
    await e.input.click();
    await expect(s.widget).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(s.widget).toBeHidden();
  });

  test('a one-line selection becomes the query', async ({ page }) => {
    const e = editor(page);
    const s = search(page);
    await e.paste(DOC);
    await e.input.evaluate((element) => element.setSelectionRange(16, 22));
    await page.keyboard.press('Control+f');
    await expect(s.query).toHaveValue('export');
    await expect(s.count).toHaveText('1 of 1');
    // Ctrl+F again in the widget selects the query to type over.
    await page.keyboard.press('Control+f');
    await page.keyboard.type('last');
    await expect(s.query).toHaveValue('last');
  });

  test('match case, whole word and regex toggles', async ({ page }) => {
    const e = editor(page);
    const s = search(page);
    await e.paste(DOC);
    await page.keyboard.press('Control+f');
    await page.keyboard.type('port');
    await expect(s.count).toHaveText(/of 5$/);

    await s.toggle('Match case').click();
    await expect(s.toggle('Match case')).toHaveAttribute('aria-pressed', 'true');
    await expect(s.count).toHaveText(/of 4$/);
    await s.toggle('Match whole word').click();
    await expect(s.count).toHaveText(/of 2$/);
    // Alt+C from the keyboard turns case back off.
    await s.query.focus();
    await page.keyboard.press('Alt+c');
    await expect(s.toggle('Match case')).toHaveAttribute('aria-pressed', 'false');
    await expect(s.count).toHaveText(/of 3$/);
    await page.keyboard.press('Alt+w');

    await s.toggle('Use regular expression').click();
    await s.query.fill('^\\w+: \\d$');
    await expect(s.count).toHaveText(/of 3$/);
    await s.query.fill('(port');
    await expect(s.count).toHaveText('Invalid pattern');
    await expect(s.query).toHaveAttribute('aria-invalid', 'true');
    await expect(s.matches).toHaveCount(0);
    await s.query.fill('port|last');
    await expect(s.count).toHaveText(/of 6$/);
    await expect(s.query).toHaveAttribute('aria-invalid', 'false');
  });

  test('the count is announced and the controls are labelled', async ({ page }) => {
    await editor(page).input.click();
    await page.keyboard.press('Control+f');
    const s = search(page);
    await expect(s.count).toHaveAttribute('aria-live', 'polite');
    for (const name of ['Match case', 'Match whole word', 'Use regular expression', 'Previous match', 'Next match', 'Close']) {
      await expect(s.toggle(name)).toBeVisible();
    }
    // Tab walks the widget in order.
    await expect(s.query).toBeFocused();
    await page.keyboard.press('Tab');
    expect(await focusedLabel(page)).toBe('Match case');
  });
});

test.describe('replace', () => {
  setupEditor(test);

  test('replace one steps to the next match; replace all is one undo step', async ({ page }) => {
    const e = editor(page);
    const s = search(page);
    await e.paste(DOC);
    await e.input.evaluate((element) => element.setSelectionRange(0, 0));
    await page.keyboard.press('Control+h');
    await expect(s.replacement).toBeVisible();
    await expect(s.query).toBeFocused();
    await page.keyboard.type('port');
    await s.replacement.focus();
    await page.keyboard.type('listen');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe(DOC.replace('port: 1', 'listen: 1'));
    await expect(s.count).toHaveText('1 of 4');
    expect(await selection(e)).toEqual({ text: 'Port', start: 10 });
    // The host got the edit.
    await expect(page.locator('.demo__output-text')).toHaveText(DOC.replace('port: 1', 'listen: 1'));

    await page.keyboard.press('Control+Alt+Enter');
    const all = 'listen: 1\nlisten: 2\nexlisten: 3\nlisten_name: x\nlast: listen\n';
    expect(await e.value()).toBe(all);
    await expect(s.count).toHaveText('Replaced 4 occurrences');
    await expect(page.locator('.demo__output-text')).toHaveText(all);

    await e.input.focus();
    await page.keyboard.press('Control+z');
    expect(await e.value()).toBe(DOC.replace('port: 1', 'listen: 1'));
    await page.keyboard.press('Control+z');
    expect(await e.value()).toBe(DOC);
    await page.keyboard.press('Control+Shift+z');
    await page.keyboard.press('Control+Shift+z');
    expect(await e.value()).toBe(all);
  });

  test('regex replacements expand groups', async ({ page }) => {
    const e = editor(page);
    const s = search(page);
    await e.paste('a: 1\nb: 2\n');
    await page.keyboard.press('Control+h');
    await s.toggle('Use regular expression').click();
    await s.query.fill('(\\w): (\\d)');
    await s.replacement.fill('$1 = $2');
    await s.widget.getByRole('button', { name: 'Replace all' }).click();
    expect(await e.value()).toBe('a = 1\nb = 2\n');
  });

  test('the replace toggle shows the row; Ctrl+H focuses the replacement when there is a query', async ({ page }) => {
    const e = editor(page);
    const s = search(page);
    await e.paste(DOC);
    await page.keyboard.press('Control+f');
    await expect(s.replacement).toBeHidden();
    await s.toggle('Toggle replace').click();
    await expect(s.replacement).toBeVisible();
    await expect(s.replacement).toBeFocused();
    await s.toggle('Toggle replace').click();
    await expect(s.replacement).toBeHidden();
    await s.query.fill('port');
    await page.keyboard.press('Control+h');
    await expect(s.replacement).toBeFocused();
  });
});

test.describe('replace while editing as another language', () => {
  setupEditor(test, '?language=yaml&edit=toml');

  test('replace all reaches the host in its own language', async ({ page }) => {
    const e = editor(page);
    const s = search(page);
    await e.input.click();
    await page.keyboard.press('Control+h');
    await s.query.fill('tunnel.example.com');
    await s.replacement.fill('edge.example.com');
    await page.keyboard.press('Control+Alt+Enter');
    expect(await e.value()).toContain('public_address = "edge.example.com"');
    await expect(page.locator('.demo__output-text')).toContainText('public_address: edge.example.com');
    await expect(page.locator('.demo__output-text')).toContainText('# Burrow server configuration');
  });
});

test.describe('find in a read-only editor', () => {
  setupEditor(test, '?readonly');

  test('finds but never replaces', async ({ page }) => {
    const e = editor(page);
    const s = search(page);
    const before = await e.value();
    await e.input.click();
    await page.keyboard.press('Control+h');
    await expect(s.widget).toBeVisible();
    await expect(s.replacement).toBeHidden();
    await expect(s.toggle('Toggle replace')).toBeHidden();
    await page.keyboard.type('listen');
    await expect(s.count).toHaveText('1 of 1');
    await page.keyboard.press('Control+Alt+Enter');
    expect(await e.value()).toBe(before);
  });
});

for (const width of [390, 320]) {
  test.describe(`find at ${width}px`, () => {
    test.use({ viewport: { width, height: 700 } });
    setupEditor(test);

    test('the widget fits and the page does not scroll sideways', async ({ page }) => {
      const e = editor(page);
      const s = search(page);
      await e.input.click();
      await page.keyboard.press('Control+h');
      await page.keyboard.type('listen');
      const box = await s.widget.boundingBox();
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(width);
      for (const name of ['Next match', 'Close', 'Replace all']) {
        const button = await s.widget.getByRole('button', { name }).boundingBox();
        expect(button.x + button.width, name).toBeLessThanOrEqual(width);
      }
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
      expect(overflow).toBeLessThanOrEqual(0);
      await expect(s.count).toHaveText('1 of 1');
    });
  });
}

test.describe('find in a 10k-line document', () => {
  setupEditor(test, '?sample=large');

  test('typing a query and stepping stays fast; only visible matches are drawn', async ({ page }) => {
    const e = editor(page);
    const s = search(page);
    const large = await e.value();
    const doc = [large, large, large, large, large].join('\n');
    await e.paste(doc);
    const lines = (await e.value()).split('\n').length;
    expect(lines).toBeGreaterThan(9000);
    const expected = (doc.match(/secret/gi) ?? []).length;
    expect(expected).toBeGreaterThan(100);

    await e.input.evaluate((element) => element.setSelectionRange(0, 0));
    await page.keyboard.press('Control+f');
    await page.evaluate(() => {
      window.__latencies = [];
      new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) window.__latencies.push(entry.duration);
      }).observe({ type: 'event', durationThreshold: 16, buffered: false });
    });
    await page.keyboard.type('secret', { delay: 30 });
    await expect(s.count).toHaveText(`1 of ${expected}`);
    // Far fewer boxes than matches: only the rendered rows get them.
    const drawn = await s.matches.count();
    expect(drawn).toBeGreaterThan(0);
    expect(drawn).toBeLessThan(expected);
    // Stepping back wraps to the last match, at the far end, and shows it.
    await page.keyboard.press('Shift+Enter');
    await expect(s.count).toHaveText(`${expected} of ${expected}`);
    await expect(s.current).toHaveCount(1);
    const caret = await e.caret();
    expect(caret.line).toBeGreaterThan(9000);
    await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => setTimeout(done, 50))));
    const worst = await page.evaluate(() => Math.max(0, ...window.__latencies));
    console.log(`find: ${expected} matches in ${lines} lines, worst interaction ${worst}ms`);
    expect(worst).toBeLessThan(250);
    // Typing in the text with the widget open stays correct.
    await page.keyboard.press('Escape');
    await page.keyboard.type('X');
    expect((await e.value()).split('\n').length).toBe(lines);
  });
});
