import { expect, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { editor } from './helpers.mjs';

// Large files: the demo config generated about three times as long as the
// large sample (`samples/generate.py --scale 5`, 6000–8500 lines), in every
// language. It is handed to the demo before it starts (`?sample=page`), so
// the load is the editor mounting it, as a host would.
//
// Timings come from the browser's Event Timing API (the data behind INP):
// each interaction's time from the input event to the next paint, which
// covers our handlers, the reactive updates and rendering. Each test logs
// its numbers. The budgets leave at least 1.7× (mostly 5–10×) headroom
// over what a debug build measures on a shared 2-core box, while O(n²)
// work per keystroke still fails them. They are soft: one run reports
// every number over its budget.
const LANGUAGES = ['yaml', 'json', 'toml', 'hcl', 'xml'];
const SCALE = 5;
// Typed inside a JSON string value, where any text is valid.
const JSON_TYPED = '-edge-tuned-for-many-lines';
const BUDGET = {
  loadMs: 3000,
  keystrokeWorstMs: 400,
  keystrokeP95Ms: 250,
  jumpToEndMs: 400,
  wheelStepMs: 250,
  findMs: 1000,
  foldAllMs: 1000,
  editAsMs: 3000,
};

const here = dirname(fileURLToPath(import.meta.url));
const generated = join(here, '..', 'test-results', 'large-files');

/** The scaled sample of every language, generated once per run. */
function samples() {
  mkdirSync(generated, { recursive: true });
  execFileSync('python3', [
    join(here, '..', '..', 'samples', 'generate.py'),
    '--scale',
    String(SCALE),
    '--out',
    generated,
  ]);
  return Object.fromEntries(
    LANGUAGES.map((language) => [
      language,
      readFileSync(join(generated, `burrow.large.${language}`), 'utf8'),
    ]),
  );
}

/** Opens the demo on `text`, returning how long until it was on screen. */
async function open(page, baseURL, language, text) {
  page.errors = [];
  page.on('pageerror', (error) => page.errors.push(String(error)));
  page.on('console', (message) => {
    if (message.type() === 'error') page.errors.push(message.text());
  });
  await page.addInitScript((sample) => {
    window.configEditorSample = sample;
  }, text);
  await page.goto(`${baseURL}?language=${language}&sample=page`);
  await page.locator('.config-editor__input').waitFor();
  await page.locator('.config-editor__code-line').first().waitFor();
  // From navigation start to the first paint after the editor is mounted.
  return page.evaluate(
    () => new Promise((done) => requestAnimationFrame(() => setTimeout(() => done(performance.now()), 0))),
  );
}

/** Starts collecting Event Timing entries, grouped by interaction. */
async function startProbe(page) {
  await page.evaluate(() => {
    window.__interactions = new Map();
    window.__events = [];
    window.__probe?.disconnect();
    window.__probe = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        window.__events.push({ name: entry.name, duration: entry.duration });
        if (!entry.interactionId) continue;
        const worst = window.__interactions.get(entry.interactionId) ?? 0;
        window.__interactions.set(entry.interactionId, Math.max(worst, entry.duration));
      }
    });
    window.__probe.observe({ type: 'event', durationThreshold: 16, buffered: false });
  });
}

/** Lets pending Event Timing entries arrive (they come after the paint). */
const settle = (page) =>
  page.evaluate(() => new Promise((done) => requestAnimationFrame(() => setTimeout(done, 80))));

/**
 * The worst and the 95th percentile of `count` interactions: those under the
 * observer's 16 ms threshold are not reported and count as 16 ms.
 */
async function interactionStats(page, count) {
  await settle(page);
  const reported = await page.evaluate(() => [...window.__interactions.values()]);
  const all = [...reported, ...Array(Math.max(0, count - reported.length)).fill(16)].sort((a, b) => a - b);
  return {
    worst: all[all.length - 1] ?? 0,
    p95: all[Math.min(all.length - 1, Math.floor(all.length * 0.95))] ?? 0,
  };
}

/** The longest event of any kind since the probe started. */
async function worstEvent(page) {
  await settle(page);
  return page.evaluate(() => Math.max(16, ...window.__events.map((entry) => entry.duration)));
}

const lastLineShown = (page, lines) =>
  page.locator(`.config-editor__gutter-content > div[data-line="${lines}"]`).count();

const numbers = {};
const record = (language, name, value) => {
  numbers[language] = { ...numbers[language], [name]: Math.round(value) };
};

test.describe('large files', () => {
  test.describe.configure({ timeout: 120_000 });
  let texts;
  test.beforeAll(() => {
    texts = samples();
  });

  test.afterAll(() => {
    console.log(`large files (ms):\n${JSON.stringify(numbers, null, 1)}`);
  });

  test.afterEach(async ({ page }) => {
    expect(page.errors, 'no page errors').toEqual([]);
  });

  for (const language of LANGUAGES) {
    test(`${language}: loads, types, scrolls, finds, folds and converts within budget`, async ({
      page,
      baseURL,
    }) => {
      const text = texts[language];
      const lines = text.split('\n').length;
      expect(lines).toBeGreaterThan(5500);
      expect(lines).toBeLessThan(10000);

      const loaded = await open(page, baseURL, language, text);
      record(language, 'lines', lines);
      record(language, 'load', loaded);
      const e = editor(page);
      expect(await e.value()).toBe(text);
      // The scaled document is valid: nothing to underline.
      await expect.poll(() => e.diagnosticCount(), { timeout: 20000 }).toBe(0);
      expect.soft(loaded, `${language} loadMs`).toBeLessThan(BUDGET.loadMs);

      // Typing in the middle of the document: every line after it moves.
      await e.input.evaluate((element) => {
        element.focus();
        const middle = element.value.indexOf('\n', element.value.length / 2);
        element.setSelectionRange(middle, middle);
      });
      await startProbe(page);
      const typed =
        language === 'json' ? '' : language === 'xml' ? '<!-- tuned for many lines -->' : '# tuned for a document of many lines';
      if (language === 'json') {
        // Inside a string value, where JSON allows any text.
        await e.input.evaluate((element) => {
          const at = element.value.indexOf('"tunnel.pngwasi.me"') + '"tunnel'.length;
          element.setSelectionRange(at, at);
        });
        await page.keyboard.type(JSON_TYPED, { delay: 25 });
      } else {
        await page.keyboard.press('Enter');
        await page.keyboard.type(typed, { delay: 25 });
      }
      const keys = language === 'json' ? JSON_TYPED.length : typed.length + 1;
      const typing = await interactionStats(page, keys);
      record(language, 'keyWorst', typing.worst);
      record(language, 'keyP95', typing.p95);
      expect.soft(typing.worst, `${language} keystrokeWorstMs`).toBeLessThan(BUDGET.keystrokeWorstMs);
      expect.soft(typing.p95, `${language} keystrokeP95Ms`).toBeLessThan(BUDGET.keystrokeP95Ms);
      await expect.poll(() => e.diagnosticCount(), { timeout: 20000 }).toBe(0);

      // Ctrl+End reaches the last line.
      const total = (await e.value()).split('\n').length;
      await startProbe(page);
      await page.keyboard.press('Control+End');
      await expect.poll(() => lastLineShown(page, total)).toBe(1);
      const jump = await worstEvent(page);
      record(language, 'ctrlEnd', jump);
      expect.soft(jump, `${language} jumpToEndMs`).toBeLessThan(BUDGET.jumpToEndMs);

      // Back to the top, then the wheel to the end: each step paints in time.
      await page.keyboard.press('Control+Home');
      await expect.poll(() => lastLineShown(page, 1)).toBe(1);
      const box = await e.input.boundingBox();
      await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
      // The frames right after each wheel step, until the end shows.
      const frames = [];
      for (let step = 0; step < 60 && (await lastLineShown(page, total)) === 0; step += 1) {
        await page.mouse.wheel(0, 4000);
        frames.push(
          await page.evaluate(async () => {
            const start = performance.now();
            await new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done)));
            return performance.now() - start;
          }),
        );
      }
      await expect.poll(() => lastLineShown(page, total)).toBe(1);
      const wheel = Math.max(...frames);
      record(language, 'wheelStep', wheel);
      expect.soft(wheel, `${language} wheelStepMs`).toBeLessThan(BUDGET.wheelStepMs);

      // Find: every match of a common word is counted and highlighted.
      await e.input.focus();
      await page.keyboard.press('Control+f');
      const field = page.getByRole('textbox', { name: 'Find' });
      await field.waitFor();
      await startProbe(page);
      const started = Date.now();
      await field.fill('target');
      await expect(page.locator('.config-editor__find-count')).toContainText(/of \d{3,}/, { timeout: 20000 });
      const find = Math.max(await worstEvent(page), Date.now() - started);
      record(language, 'findAll', find);
      expect.soft(find, `${language} findMs`).toBeLessThan(BUDGET.findMs);
      await page.keyboard.press('Escape');

      // Fold everything, then unfold everything.
      await e.input.focus();
      await startProbe(page);
      await page.keyboard.press('Control+K');
      await page.keyboard.press('Control+0');
      // Every top-level block folds (TOML's tables fold one by one).
      await expect.poll(async () => (await e.value()).split('\n').length).toBeLessThan(total * 0.6);
      await page.keyboard.press('Control+K');
      await page.keyboard.press('Control+J');
      await expect.poll(async () => (await e.value()).split('\n').length).toBe(total);
      const folds = await worstEvent(page);
      record(language, 'foldAll', folds);
      expect.soft(folds, `${language} foldAllMs`).toBeLessThan(BUDGET.foldAllMs);

      // Edit as another language and back.
      const other = language === 'json' ? 'yaml' : 'json';
      const before = await e.value();
      const switchStart = Date.now();
      await page.locator('.demo__edit-select').selectOption(other);
      await expect.poll(async () => (await e.value()) !== before, { timeout: 20000 }).toBe(true);
      await page.locator('.config-editor__code-line').first().waitFor();
      const editAs = Date.now() - switchStart;
      record(language, 'editAs', editAs);
      expect.soft(editAs, `${language} editAsMs`).toBeLessThan(BUDGET.editAsMs);
    });
  }
});
