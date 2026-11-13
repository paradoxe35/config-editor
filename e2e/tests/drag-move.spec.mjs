import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// A drag inside the text is one move: the browser sends its delete and its
// drop as two edits, and when the editor refuses one (it reaches into a
// closed fold, or changes a locked field), the other is refused too. Before,
// a refused drop left the dragged line deleted (and saved), and a refused
// delete let the drop copy the text, a fold's header twice (round 8's
// `D8.editing-1`, hard round 4's drag onto a lock).

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

const input = (page) => page.locator('#qa .config-editor__input');

async function open(page) {
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', {
    timeout: 20000,
  });
  await page.evaluate(async () => {
    window.module = await import('../config-editor/index.js');
  });
}

async function mount(page, options) {
  await page.evaluate((options) => {
    window.editor?.destroy();
    window.qa?.destroy();
    let host = document.getElementById('qa');
    if (!host) {
      host = document.createElement('div');
      host.id = 'qa';
      document.body.prepend(host);
    }
    window.saved = [];
    window.qa = new window.module.ConfigEditor(host, {
      ...options,
      onSave: (text) => window.saved.push(text),
    });
  }, options);
  await input(page).waitFor();
}

/** The screen point of `line`, `column` of the textarea's shown text. */
async function point(page, line, column) {
  return input(page).evaluate(
    (element, [line, column]) => {
      const rect = element.getBoundingClientRect();
      const style = getComputedStyle(element);
      const probe = document.createElement('span');
      probe.style.cssText = `position:absolute;visibility:hidden;white-space:pre;font:${style.font}`;
      probe.textContent = '0123456789';
      document.body.append(probe);
      const char = probe.getBoundingClientRect().width / 10;
      probe.remove();
      const height = parseFloat(style.lineHeight) || 20;
      return {
        x: rect.left + parseFloat(style.paddingLeft) + column * char + char / 3,
        y: rect.top + parseFloat(style.paddingTop) + line * height + height / 2 - element.scrollTop,
      };
    },
    [line, column],
  );
}

/** Selects shown `line` from `from` to `to` (the line break with `to` past its end). */
async function select(page, line, from, to) {
  await input(page).evaluate(
    (element, [line, from, to]) => {
      const lines = element.value.split('\n');
      const start = lines.slice(0, line).reduce((sum, text) => sum + text.length + 1, 0);
      element.focus();
      element.setSelectionRange(start + from, start + Math.min(to, lines[line].length + 1));
    },
    [line, from, to],
  );
}

/** Drags the selection, grabbed at `from` ([line, column]), to `to`. */
async function drag(page, from, to) {
  const a = await point(page, ...from);
  const b = await point(page, ...to);
  await page.mouse.move(a.x, a.y);
  await page.mouse.down();
  await page.mouse.move(a.x + 4, a.y, { steps: 3 });
  await page.mouse.move(b.x, b.y, { steps: 12 });
  await page.mouse.up();
  // The input lands at the end of the task, the refusal's repair with it.
  await page.waitForTimeout(300);
}

async function saveNow(page) {
  await input(page).focus();
  await page.keyboard.press('Control+s');
  return page.evaluate(() => window.saved.at(-1) ?? null);
}

// Each text: a block folded at `fold` whose header is shown line `header`,
// and a plain line `plain` above it.
const CASES = {
  yaml: {
    text: 'a: 1\nserver:\n  host: x\n  port: 9\nb: 2\n',
    fold: '/server',
    header: 1,
    plain: 0,
  },
  json: {
    text: '{\n  "a": 1,\n  "server": {\n    "host": "x"\n  },\n  "b": 2\n}\n',
    fold: '/server',
    header: 2,
    plain: 1,
  },
  toml: {
    text: 'a = 1\nb = 2\n\n[server]\nhost = "x"\nport = 9\n',
    fold: '/server',
    header: 3,
    plain: 0,
  },
  hcl: {
    text: 'a = 1\nserver {\n  host = "x"\n}\nb = 2\n',
    fold: '/server',
    header: 1,
    plain: 0,
  },
  xml: {
    text: '<c>\n  <a>1</a>\n  <server>\n    <host>x</host>\n  </server>\n</c>\n',
    fold: '/c/server',
    header: 2,
    plain: 1,
  },
};

test.describe('a drag near a closed fold is one move', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const [language, { text, fold, header, plain }] of Object.entries(CASES)) {
    test(`${language}: a line dropped on the fold's header is not lost`, async ({ page }) => {
      await open(page);
      await mount(page, { language, value: text });
      expect(await page.evaluate((fold) => window.qa.fold(fold), fold)).toBe(true);
      await select(page, plain, 0, Infinity);
      const end = (await input(page).inputValue()).split('\n')[header].length;
      await drag(page, [plain, 1], [header, end]);
      await expect.poll(() => page.evaluate(() => window.qa.value)).toBe(text);
      expect(await saveNow(page)).toBe(text);
    });

    test(`${language}: the folded header dragged up is not copied`, async ({ page }) => {
      await open(page);
      await mount(page, { language, value: text });
      expect(await page.evaluate((fold) => window.qa.fold(fold), fold)).toBe(true);
      await select(page, header, 0, Infinity);
      await drag(page, [header, 2], [0, 0]);
      await expect.poll(() => page.evaluate(() => window.qa.value)).toBe(text);
      expect(await saveNow(page)).toBe(text);
    });
  }

  test('control: without folds the line moves, one undo step', async ({ page }) => {
    await open(page);
    const text = 'a: 1\nb: 2\nc: 3\n';
    await mount(page, { language: 'yaml', value: text });
    await select(page, 1, 0, Infinity);
    await drag(page, [1, 1], [0, 0]);
    await expect.poll(() => page.evaluate(() => window.qa.value)).toBe('b: 2\na: 1\nc: 3\n');
    await page.keyboard.press('Control+z');
    await expect.poll(() => page.evaluate(() => window.qa.value)).toBe(text);
  });
});

test.describe('a drag at a locked field is one move', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');
  const text = 'a: 1\nlisten: 7835\nb: 2\n';

  test('a line dropped on the locked value comes back, and redo does not delete it', async ({ page }) => {
    await open(page);
    await mount(page, {
      language: 'yaml',
      value: text,
      readOnlyPaths: ['/listen'],
    });
    await select(page, 2, 0, 4);
    await drag(page, [2, 1], [1, 10]);
    await expect.poll(() => page.evaluate(() => window.qa.value)).toBe(text);
    await page.keyboard.press('Control+y');
    expect(await page.evaluate(() => window.qa.value)).toBe(text);
    expect(await saveNow(page)).toBe(text);
  });

  test('the locked value dragged away is not copied', async ({ page }) => {
    await open(page);
    await mount(page, {
      language: 'yaml',
      value: text,
      readOnlyPaths: ['/listen'],
    });
    await select(page, 1, 8, 12);
    await drag(page, [1, 9], [0, 4]);
    await expect.poll(() => page.evaluate(() => window.qa.value)).toBe(text);
    expect(await saveNow(page)).toBe(text);
  });
});
