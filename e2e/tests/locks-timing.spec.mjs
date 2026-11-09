import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Many read-only fields stay cheap (QA 23): each lock looked its field up
// with a read of the whole document, so with 50 locks an Enter in an
// 8000-line document took 2.1 s (288 ms with one), and 2000 read-only paths
// took 100 s to mount and crashed the tab on repeated updates. Timed: runs
// alone after the other specs (see the config).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const vanilla = (options = {}) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

const input = (page) => page.locator('.config-editor__input');

async function open(page, options) {
  await page.goto(vanilla(options));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await input(page).waitFor();
  await page.evaluate(() => {
    window.longest = 0;
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) window.longest = Math.max(window.longest, entry.duration);
    }).observe({ type: 'longtask' });
  });
}

/** The longest task while `work` runs and the page settles, in ms. */
async function longestTask(page, work, settle = 400) {
  await page.waitForTimeout(300);
  await page.evaluate(() => (window.longest = 0));
  await work();
  await page.waitForTimeout(settle);
  return page.evaluate(() => window.longest);
}

/** `blocks` services of a host and a port each (four lines a service in YAML: 8000 lines for 2000). */
function services(language, blocks) {
  const range = Array.from({ length: blocks }, (_, index) => index);
  if (language === 'yaml') return range.map((i) => `svc_${i}:\n  host: h${i}.example.com # c\n  port: ${1000 + i}\n  on: true\n`).join('');
  if (language === 'json') return '{\n' + range.map((i) => `  "svc_${i}": {\n    "host": "h${i}.example.com",\n    "port": ${1000 + i}\n  }`).join(',\n') + '\n}\n';
  if (language === 'toml') return range.map((i) => `[svc_${i}]\nhost = "h${i}.example.com" # c\nport = ${1000 + i}\n`).join('\n');
  if (language === 'hcl') return range.map((i) => `svc_${i} {\n  host = "h${i}.example.com"\n  port = ${1000 + i}\n}\n`).join('');
  return '<root>\n' + range.map((i) => `  <svc_${i}>\n    <host>h${i}.example.com</host>\n    <port>${1000 + i}</port>\n  </svc_${i}>`).join('\n') + '\n</root>\n';
}

/** Read-only paths of the first `count` services' hosts. */
const hostPaths = (language, count) =>
  Array.from({ length: count }, (_, i) => `${language === 'xml' ? '/root' : ''}/svc_${i}/host`);

/** Mounts a fresh editor of `text` in `language` with `options`, in place of the last one: how long it took, in ms. */
async function mount(page, language, text, options = {}) {
  return page.evaluate(
    async ({ language, text, options }) => {
      const { ConfigEditor } = await import(new URL('../config-editor/index.js', location.href).href);
      window.locked?.destroy();
      document.getElementById('locked')?.remove();
      const host = document.createElement('div');
      host.id = 'locked';
      host.style.height = '400px';
      document.body.append(host);
      const started = performance.now();
      window.locked = new ConfigEditor(host, { language, value: text, schema: '{}', ...options });
      await new Promise((done) => requestAnimationFrame(() => setTimeout(done, 0)));
      return performance.now() - started;
    },
    { language, text, options },
  );
}

const lockedInput = (page) => page.locator('#locked .config-editor__input');

test.describe('lock timing', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  test('P23-1: Enter with 50 read-only fields in an 8000-line document costs about what it costs without them, in every language', async ({ page }) => {
    test.setTimeout(600_000);
    await open(page, {});
    const failures = [];
    for (const language of ['yaml', 'json', 'toml', 'hcl', 'xml']) {
      const text = services(language, 2000);
      // The shortest longest task of a few Enters (load only ever makes one
      // longer), without locks, then with 50 read-only paths.
      const enters = async (options) => {
        await mount(page, language, text, options);
        await lockedInput(page).click({ position: { x: 300, y: 60 } });
        const presses = [];
        for (let press = 0; press < 4; press += 1) presses.push(await longestTask(page, () => page.keyboard.press('Enter')));
        return Math.min(...presses);
      };
      const bare = await enters({});
      const locked = await enters({ readOnlyPaths: hostPaths(language, 50) });
      console.log(`${language}: longest task per Enter ${Math.round(bare)} ms, ${Math.round(locked)} ms with 50 locks`);
      // Was 2.1 s with 50 locks, 288 ms with one (YAML).
      if (locked - bare >= 150) failures.push(`${language}: ${Math.round(locked)} ms with 50 locks, ${Math.round(bare)} ms without`);
    }
    await page.evaluate(() => window.locked?.destroy());
    expect(failures).toEqual([]);
  });

  test('P23-1: 2000 read-only fields mount and update in a moment, by path and by the schema', async ({ page }) => {
    test.setTimeout(600_000);
    await open(page, {});
    const language = 'yaml';
    const text = services(language, 2000);
    const paths = hostPaths(language, 2000);
    const schema = JSON.stringify({
      type: 'object',
      properties: Object.fromEntries(
        Array.from({ length: 2000 }, (_, i) => [`svc_${i}`, { type: 'object', properties: { host: { type: 'string', readOnly: true } } }]),
      ),
    });
    const bare = await longestTask(page, () => mount(page, language, text));
    const byPaths = await longestTask(page, () => mount(page, language, text, { readOnlyPaths: paths }));
    const bySchema = await longestTask(page, () => mount(page, language, text, { schema }));
    await mount(page, language, text);
    // Updated again and again (the tab crashed).
    const updates = [];
    for (let round = 0; round < 4; round += 1) {
      updates.push(
        await longestTask(page, async () => {
          await page.evaluate((list) => window.locked.update({ readOnlyPaths: list }), round % 2 ? paths.slice(1) : paths);
          await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => setTimeout(done, 0))));
        }),
      );
    }
    await lockedInput(page).click({ position: { x: 300, y: 60 } });
    const enter = await longestTask(page, () => page.keyboard.press('Enter'));
    console.log(
      `2000 locks: mount ${Math.round(bare)} ms bare, ${Math.round(byPaths)} ms by path, ${Math.round(bySchema)} ms by schema; updates ${updates.map(Math.round).join(', ')} ms; Enter ${Math.round(enter)} ms`,
    );
    await expect(page.locator('#locked .config-editor__code-line').first()).toBeVisible();
    await page.evaluate(() => window.locked?.destroy());
    // Mounting took 100 s; each a small part of that now.
    expect(byPaths - bare, 'mount by path').toBeLessThan(1500);
    expect(bySchema - bare, 'mount by schema').toBeLessThan(1500);
    expect(Math.min(...updates), 'update').toBeLessThan(1500);
    expect(enter, 'Enter').toBeLessThan(1500);
  });
});
