import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// The large demo config (~2000 lines, the same document in every language):
// it loads valid, language switching keeps the large sample, editing stays
// correct at the far end, and interaction latency stays within budget.
//
// Latency comes from the browser's Event Timing API (the data behind INP):
// each entry is the time from the input event to the next paint, so it covers
// our handlers, reactive updates, and rendering. Budgets are generous enough
// for CI machines and still catch accidental O(n²) work per keystroke.
const LANGUAGES = ['yaml', 'json', 'toml', 'hcl', 'xml'];
const KEYSTROKE_BUDGET_MS = 250;

async function startLatencyProbe(page) {
  await page.evaluate(() => {
    window.__latencies = [];
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        window.__latencies.push({ name: entry.name, duration: entry.duration });
      }
    }).observe({ type: 'event', durationThreshold: 16, buffered: false });
  });
}

async function worstLatency(page) {
  // Event timing entries are delivered after the next paint.
  await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => setTimeout(done, 50))));
  return page.evaluate(() => Math.max(0, ...window.__latencies.map((entry) => entry.duration)));
}

for (const language of LANGUAGES) {
  test.describe(`large ${language} sample`, () => {
    setupEditor(test, `?language=${language}&sample=large`);

    test('loads the long document valid against the shared schema', async ({ page }) => {
      const e = editor(page);
      const lines = (await e.value()).split('\n').length;
      expect(lines).toBeGreaterThan(1500);
      await expect.poll(() => e.diagnosticCount(), { timeout: 10000 }).toBe(0);
      await expect(page.locator('.config-editor__file')).toContainText(`burrow.large.${language}`);
    });

    test('typing at the end of the document stays responsive', async ({ page }) => {
      const e = editor(page);
      await e.input.evaluate((element) => {
        element.focus();
        element.setSelectionRange(element.value.length, element.value.length);
      });
      await startLatencyProbe(page);
      // A comment line is valid in every language except JSON; JSON gets a
      // keystroke run inside a string value instead.
      if (language === 'json') {
        await e.input.evaluate((element) => {
          const at = element.value.indexOf('tunnel.pngwasi.me') + 'tunnel'.length;
          element.setSelectionRange(at, at);
        });
        await page.keyboard.type('-edge', { delay: 20 });
      } else {
        const comment = language === 'xml' ? '<!-- tuned -->' : '# tuned';
        await page.keyboard.type(comment, { delay: 20 });
      }
      const worst = await worstLatency(page);
      console.log(`${language}: worst keystroke latency ${worst.toFixed(1)}ms`);
      expect(worst).toBeLessThan(KEYSTROKE_BUDGET_MS);
      await expect.poll(() => e.diagnosticCount(), { timeout: 10000 }).toBe(0);
    });

    test('the overlay, caret line, and diagnostics stay aligned to the last line', async ({ page }) => {
      const e = editor(page);
      // A wrong value near the end: its squiggle must sit on its own line.
      await e.input.evaluate((element) => {
        const lines = element.value.split('\n');
        const index = lines
          .map((line, i) => (/[:=]\s*\d+,?$/.test(line) || /<\w+>\d+<\/\w+>/.test(line) ? i : -1))
          .filter((i) => i >= 0)
          .pop();
        lines[index] = lines[index].replace(/(\d+)(,?)$/, '-1$2').replace(/>(\d+)</, '>-1<');
        element.value = lines.join('\n');
        element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertFromPaste' }));
        element.scrollTop = element.scrollHeight;
        element.dispatchEvent(new Event('scroll'));
        window.__errorLine = index + 1;
      });
      await expect.poll(() => e.diagnosticCount(), { timeout: 10000 }).toBeGreaterThan(0);
      const geometry = await page.evaluate(() => {
        const input = document.querySelector('.config-editor__input');
        const lineHeight = parseFloat(getComputedStyle(input).lineHeight);
        // Only rows near the view exist; each carries its line number.
        const rows = [...document.querySelectorAll('.config-editor__code-line')];
        const top = (row) => row.getBoundingClientRect().top;
        const lineOf = (row) => Number(row.dataset.line);
        const first = rows[0];
        const drift = Math.max(...rows.map((row) => Math.abs(top(row) - top(first) - (lineOf(row) - lineOf(first)) * lineHeight)));
        // The textarea's own line box for the error line, to compare against.
        const input_ = document.querySelector('.config-editor__input');
        const expectedTop = input_.getBoundingClientRect().top + parseFloat(getComputedStyle(input_).paddingTop)
          + (window.__errorLine - 1) * lineHeight - input_.scrollTop;
        const errorRow = document.querySelector(`.config-editor__code-line[data-line="${window.__errorLine}"]`);
        const row = errorRow.getBoundingClientRect();
        const offset = Math.abs(row.top - expectedTop);
        const marks = [...document.querySelectorAll('.config-editor__diagnostic-marker')].map((m) => m.getBoundingClientRect().top);
        const nearest = Math.min(...marks.map((top) => Math.abs(top - row.top)));
        return { lineHeight, drift, nearest, offset };
      });
      // Fractional line heights accumulate rounding differences between the
      // textarea and the overlay over thousands of lines.
      expect(Number.isInteger(geometry.lineHeight), `line height ${geometry.lineHeight}`).toBe(true);
      expect(geometry.drift).toBeLessThan(0.5);
      expect(geometry.nearest).toBeLessThan(0.5);
      // The overlay row sits exactly on the textarea's line.
      expect(geometry.offset).toBeLessThan(0.5);
    });

    test('only the rows near the view are rendered, and they match the text', async ({ page }) => {
      const e = editor(page);
      const state = await e.input.evaluate(async (element) => {
        element.scrollTop = element.scrollHeight / 2;
        element.dispatchEvent(new Event('scroll'));
        await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
        const lineHeight = parseFloat(getComputedStyle(element).lineHeight);
        const middle = Math.floor(element.scrollTop / lineHeight) + 5;
        const row = document.querySelector(`.config-editor__code-line[data-line="${middle}"]`);
        return {
          rows: document.querySelectorAll('.config-editor__code-line').length,
          rendered: row?.textContent,
          expected: element.value.split('\n')[middle - 1],
        };
      });
      expect(state.rows).toBeLessThan(200);
      expect(state.rendered).toBe(state.expected);
    });

    test('scrolling to the end renders the last line in the overlay', async ({ page }) => {
      const e = editor(page);
      const last = (await e.value()).trimEnd().split('\n').pop();
      await e.input.evaluate((element) => {
        element.focus();
        element.setSelectionRange(element.value.length, element.value.length);
        element.scrollTop = element.scrollHeight;
        element.dispatchEvent(new Event('scroll'));
      });
      await expect(page.locator('.config-editor__code-line', { hasText: last.trim() }).last()).toBeVisible();
    });
  });
}

test.describe('switching languages', () => {
  setupEditor(test, '?language=yaml&sample=large');

  test('the language picker reloads the same large config in another language', async ({ page }) => {
    for (const language of ['json', 'toml', 'hcl', 'xml', 'yaml']) {
      await page.locator('.demo__language-select').selectOption(language);
      await page.waitForURL((url) => url.search.includes(`language=${language}`));
      await page.locator('.config-editor__input').waitFor();
      expect(page.url()).toContain('sample=large');
      await expect(page.locator('.config-editor__file')).toContainText(`burrow.large.${language}`);
      await expect.poll(() => editor(page).diagnosticCount(), { timeout: 10000 }).toBe(0);
    }
  });

  test('the sample picker switches between the short and the large config', async ({ page }) => {
    await page.locator('.demo__sample-select').selectOption('small');
    await page.waitForURL((url) => url.search.includes('sample=small'));
    await page.locator('.config-editor__input').waitFor();
    expect((await editor(page).value()).split('\n').length).toBeLessThan(20);
    expect(page.url()).toContain('language=yaml');
  });
});

test.describe('completion in a large document', () => {
  setupEditor(test, '?language=yaml&sample=large');

  test('completion at the far end of the document is fast and correct', async ({ page }) => {
    const e = editor(page);
    // Completion context is computed from the caret through the whole
    // document above it, so the last line is its worst case.
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(element.value.length, element.value.length);
    });
    await startLatencyProbe(page);
    await page.keyboard.type('pool:\n');
    await page.keyboard.type('max_');
    await expect.poll(() => e.keys()).toContain('max_idle');
    await page.keyboard.press('Enter');
    await expect.poll(() => e.value()).toContain('pool:\n  max_idle: 64');
    const worst = await worstLatency(page);
    console.log(`yaml completion: worst latency ${worst.toFixed(1)}ms`);
    expect(worst).toBeLessThan(KEYSTROKE_BUDGET_MS * 2);
  });
});

// The work after a pause in typing (validation, the placeholders' fields,
// the Edit-as conversion) and after a whole-document edit, as long tasks:
// QA round 16 measured 630 ms after one key in the large JSON sample (each
// placeholder looked up by path in a text that does not parse), seconds for
// Ctrl+A Tab on the sample pasted four times, and 1.7 s for an Edit-as
// switch there. Generous bounds for CI machines that still catch work done
// once per placeholder or per problem.
const LONG_TASK_BUDGET_MS = 300;

async function startLongTaskProbe(page) {
  await page.evaluate(() => {
    window.__longTasks = [];
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) window.__longTasks.push(Math.round(entry.duration));
    }).observe({ type: 'longtask' });
  });
}

async function longestTask(page, settle = 1500) {
  await page.waitForTimeout(settle);
  return page.evaluate(() => Math.max(0, ...window.__longTasks));
}

for (const language of LANGUAGES) {
  test.describe(`the typing pause in the large ${language} sample`, () => {
    setupEditor(test, `?language=${language}&sample=large`);

    test('a key that breaks the document leaves no long task at the pause', async ({ page }) => {
      const e = editor(page);
      await e.input.evaluate((element) => {
        element.focus();
        const at = element.value.indexOf('\n', element.value.length >> 1);
        element.setSelectionRange(at, at);
      });
      await page.waitForTimeout(500);
      await startLongTaskProbe(page);
      await page.keyboard.type('x{');
      const longest = await longestTask(page);
      console.log(`${language}: longest task after a key ${longest}ms`);
      expect(longest).toBeLessThan(LONG_TASK_BUDGET_MS);
    });
  });
}

test.describe('the large sample pasted four times', () => {
  setupEditor(test, '?language=yaml&sample=large');

  test.beforeEach(async ({ page, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    const e = editor(page);
    const large = await e.value();
    await page.evaluate((text) => navigator.clipboard.writeText(text), large.endsWith('\n') ? large : `${large}\n`);
    await e.input.click();
    for (let copy = 1; copy < 4; copy += 1) {
      await page.keyboard.press('Control+End');
      await page.keyboard.press('Control+v');
    }
    expect((await e.value()).split('\n').length).toBeGreaterThan(7000);
    await page.waitForTimeout(2000);
  });

  test('Ctrl+A Tab and its undo leave no multi-second task', async ({ page }) => {
    await startLongTaskProbe(page);
    await page.keyboard.press('Control+a');
    await page.keyboard.press('Tab');
    await page.waitForTimeout(1000);
    await page.keyboard.press('Control+z');
    const longest = await longestTask(page, 2000);
    console.log(`yaml x4: longest task of Ctrl+A Tab and its undo ${longest}ms`);
    expect(longest).toBeLessThan(LONG_TASK_BUDGET_MS * 3);
  });

  test('an Edit-as switch leaves no multi-second task', async ({ page }) => {
    await startLongTaskProbe(page);
    await page.locator('.demo__edit-select').selectOption('json');
    await page.waitForTimeout(500);
    await editor(page).input.click();
    await page.keyboard.press('Control+z');
    const longest = await longestTask(page, 2000);
    console.log(`yaml x4: longest task of an Edit-as switch and its undo ${longest}ms`);
    expect(longest).toBeLessThan(LONG_TASK_BUDGET_MS * 3);
  });
});

// QA round 17: Ctrl+/ over the whole sample pasted four times (~8000 lines),
// back, and its undo took 1-2.5 s each: after the toggle, every commented
// placeholder was looked up by path (a read of the whole text each).
for (const language of ['yaml', 'xml']) {
  test.describe(`Ctrl+/ over the large ${language} sample pasted four times`, () => {
    setupEditor(test, `?language=${language}&sample=large`);

    test('commenting everything, back, and its undo leave no long task', async ({ page, context }) => {
      await context.grantPermissions(['clipboard-read', 'clipboard-write']);
      const e = editor(page);
      const large = await e.value();
      await page.evaluate((text) => navigator.clipboard.writeText(text), large.endsWith('\n') ? large : `${large}\n`);
      await e.input.click();
      for (let copy = 1; copy < 4; copy += 1) {
        await page.keyboard.press('Control+End');
        await page.keyboard.press('Control+v');
      }
      const pasted = await e.value();
      expect(pasted.split('\n').length).toBeGreaterThan(7000);
      await page.waitForTimeout(2000);
      const worst = [];
      for (const [step, keys] of [['comment', ['Control+a', 'Control+/']], ['back', ['Control+/']], ['comment', ['Control+/']], ['undo', ['Control+z']]]) {
        await startLongTaskProbe(page);
        for (const key of keys) await page.keyboard.press(key);
        worst.push(`${step} ${await longestTask(page, 2000)}`);
      }
      expect(await e.value()).toBe(pasted);
      console.log(`${language} x4: longest task of Ctrl+/ steps: ${worst.join(', ')} ms`);
      for (const entry of worst) expect(Number(entry.split(' ')[1]), entry).toBeLessThan(LONG_TASK_BUDGET_MS * 2);
    });
  });
}

// QA round 18: Ctrl+/ on a closing tag or a block's `}` comments the whole
// element or block, leaving a commented copy of the document beside a live
// one. Then Ctrl+A Ctrl+/, back and the undos took 2-3 s each on 4300 lines
// of XML and 7800 of HCL: every placeholder, written in the commented copy
// too, was looked up by path (a read of the whole text each).
for (const language of ['xml', 'hcl']) {
  test.describe(`Ctrl+/ over a large ${language} document holding a commented copy`, () => {
    setupEditor(test, `?language=${language}&sample=large`);

    test('commenting everything, back, and the undos leave no long task', async ({ page, context }) => {
      await context.grantPermissions(['clipboard-read', 'clipboard-write']);
      const e = editor(page);
      const one = (await e.value()).replace(/\n?$/, '\n');
      const big = language === 'hcl' ? [0, 1, 2, 3].map((copy) => `copy${copy} {\n${one}}\n`).join('') : one.repeat(2);
      await page.evaluate((text) => navigator.clipboard.writeText(text), big);
      await e.input.click();
      await page.keyboard.press('Control+a');
      await page.keyboard.press('Control+v');
      await expect.poll(() => e.value()).toBe(big);
      // The middle line: the first copy's closing tag, or a block's line.
      await e.input.evaluate((element) => {
        const lines = element.value.split('\n');
        const at = lines.slice(0, Math.floor(lines.length / 2) - 1).join('\n').length + 1;
        element.setSelectionRange(at, at);
      });
      await page.keyboard.press('Control+/');
      await expect.poll(() => e.value()).not.toBe(big);
      const commented = await e.value();
      await page.waitForTimeout(2000);
      const worst = [];
      for (const [step, keys] of [['all', ['Control+a', 'Control+/']], ['back', ['Control+/']], ['undo', ['Control+z']], ['undo', ['Control+z']]]) {
        await startLongTaskProbe(page);
        for (const key of keys) await page.keyboard.press(key);
        worst.push(`${step} ${await longestTask(page, 2000)}`);
      }
      expect(await e.value()).toBe(commented);
      console.log(`${language} with a commented copy: longest task of Ctrl+/ steps: ${worst.join(', ')} ms`);
      for (const entry of worst) expect(Number(entry.split(' ')[1]), entry).toBeLessThan(LONG_TASK_BUDGET_MS * 2);
    });
  });
}
