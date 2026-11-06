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
        const rows = [...document.querySelectorAll('.config-editor__code-line')];
        const first = rows[0].getBoundingClientRect().top;
        const drift = Math.max(...rows.map((row, i) => Math.abs(row.getBoundingClientRect().top - first - i * lineHeight)));
        const row = rows[window.__errorLine - 1].getBoundingClientRect();
        const marks = [...document.querySelectorAll('.config-editor__diagnostic-marker')].map((m) => m.getBoundingClientRect().top);
        const nearest = Math.min(...marks.map((top) => Math.abs(top - row.top)));
        return { lineHeight, drift, nearest };
      });
      // Fractional line heights accumulate rounding differences between the
      // textarea and the overlay over thousands of lines.
      expect(Number.isInteger(geometry.lineHeight), `line height ${geometry.lineHeight}`).toBe(true);
      expect(geometry.drift).toBeLessThan(0.5);
      expect(geometry.nearest).toBeLessThan(0.5);
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
