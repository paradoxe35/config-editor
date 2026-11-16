import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

// The suggestion list itself: Ctrl+Space, keys with the list open, wrapping around, the wheel over it.

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

const LIST_STARTS = { yaml: '', json: '{\n  \n}\n', toml: '', hcl: '', xml: '' };

const input = (page) => page.locator('.config-editor__input');

const said = (page) => page.locator('.config-editor__announcement').first();

// [language, what to type: a key nothing in the schema starts with]
const NOTHING_TO_OFFER = [
  ['yaml', 'bindzzq'],
  ['json', '{"bindzzq'],
  ['toml', 'bindzzq'],
  ['hcl', 'bindzzq'],
  ['xml', '<bindzzq'],
];

test.describe('yaml: whole lines and selections', () => {
  setupEditor(test, '?language=yaml');
  test('Ctrl+A closes an open list', async ({ page }) => {
    const e = await at(page, 'listen: 1\npu|');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.isOpen()).toBe(true);
    await page.keyboard.press('Control+a');
    await expect.poll(() => e.isOpen()).toBe(false);
  });
});

test.describe('keys keep their meaning with the suggestion list open', () => {
  setupEditor(test, '?language=yaml');

  test('Shift+Up selects instead of moving in the list', async ({ page }) => {
    const e = await at(page, 'listen: 1\n|');
    await page.keyboard.type('pub');
    await expect.poll(() => e.isOpen()).toBe(true);
    await page.keyboard.press('Shift+ArrowUp');
    const selected = await e.input.evaluate((element) => element.selectionStart !== element.selectionEnd);
    expect(selected).toBe(true);
  });

  test('Ctrl+Enter opens a line below instead of accepting', async ({ page }) => {
    const e = await at(page, 'listen: 1\n|');
    await page.keyboard.type('pub');
    await expect.poll(() => e.isOpen()).toBe(true);
    await page.keyboard.press('Control+Enter');
    expect(await e.value()).toBe('listen: 1\npub\n');
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


  test.describe('yaml: Ctrl+Space', () => {
    setupEditor(test, '?language=yaml');

    test('Ctrl+Space with a selection opens the list at its end', async ({ page }) => {
      const e = await at(page, 'tcp:\n  ¦node¦lay: true\n');
      await page.keyboard.press('Control+Space');
      await expect.poll(() => e.isOpen()).toBe(true);
      await page.keyboard.press('Escape');
      expect(await e.value()).toBe('tcp:\n  nodelay: true\n');
    });
  });
});

for (const [language, text] of Object.entries(LIST_STARTS)) {
  test.describe(`the ${language} suggestion list`, () => {
    setupEditor(test, `?language=${language}`);

    test('wraps around at both ends', async ({ page }) => {
      const e = editor(page);
      await e.paste(text);
      if (text) {
        await page.keyboard.press('Control+Home');
        await page.keyboard.press('ArrowDown');
        await page.keyboard.press('End');
      }
      await page.keyboard.press('Control+Space');
      await expect(e.options.first()).toBeVisible();
      const count = await e.options.count();
      expect(count).toBeGreaterThan(2);
      const active = () =>
        e.options.evaluateAll((options) => options.findIndex((option) => option.classList.contains('config-editor__completion-active')));
      await expect.poll(active).toBe(0);
      await page.keyboard.press('ArrowUp');
      await expect.poll(active).toBe(count - 1);
      await page.keyboard.press('ArrowDown');
      await expect.poll(active).toBe(0);
    });
  });
}

for (const [language, typed] of NOTHING_TO_OFFER) {
  test.describe(`${language}: Ctrl+Space with nothing to offer`, () => {
    setupEditor(test, `?language=${language}&schema=help`);

    test('says "No suggestions"', async ({ page }) => {
      await input(page).click();
      await page.keyboard.press('Control+End');
      await page.keyboard.type(typed);
      await page.keyboard.press('Escape');
      await page.keyboard.press('Control+Space');
      await expect(page.locator('.config-editor__completion')).toHaveCount(0);
      await expect(said(page)).toHaveText(/^No suggestions ?$/);
    });
  });
}

test.describe('Ctrl+Space with something to offer', () => {
  setupEditor(test, '?language=yaml&schema=help');

  test('shows the list and says no "No suggestions"', async ({ page }) => {
    await input(page).click();
    await page.keyboard.press('Control+Space');
    await expect(page.locator('.config-editor__completion').first()).toBeVisible();
    await expect(said(page)).not.toHaveText(/No suggestions/);
  });
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "js");

  const built = existsSync(
    join(js, "config-editor", "pkg", "config_editor_bg.wasm"),
  );

  const pageUrl = (options) =>
    `${pathToFileURL(join(js, "examples", "vanilla.html")).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

  const input = (page) => page.locator(".config-editor__input").first();

  async function open(page, options) {
    const errors = [];
    page.on("pageerror", (error) => errors.push(String(error)));
    page.on(
      "console",
      (message) => message.type() === "error" && errors.push(message.text()),
    );
    await page.goto(pageUrl({ schema: undefined, ...options }));
    await expect(page.locator("body")).toHaveAttribute("data-ready", "true", {
      timeout: 20000,
    });
    await input(page).waitFor();
    // A page that scrolls, the editor at its top.
    await page.addStyleTag({ content: "body { padding-bottom: 3000px; }" });
    await page.evaluate(() => scrollTo(0, 0));
    return errors;
  }

  const scrolled = (page) =>
    page.evaluate(() => ({
      page: Math.round(scrollY),
      text: Math.round(document.querySelector(".config-editor__input").scrollTop),
    }));

  test.skip(!built, "run `npm run build` in js/ first");

  test.describe("the wheel over the suggestion list", () => {
    const filler = Array.from({ length: 200 }, (_, i) => `# line ${i}\n`).join(
      "",
    );
    const schemaOf = (count) =>
      JSON.stringify({
        type: "object",
        properties: Object.fromEntries(
          Array.from({ length: count }, (_, i) => [
            `prop_${i}`,
            { type: "string" },
          ]),
        ),
      });
    const list = (page) => page.locator(".config-editor__completions");
    const listTop = (page) => list(page).evaluate((element) => element.scrollTop);

    async function openList(page, count) {
      const errors = await open(page, {
        value: `\n${filler}`,
        schema: schemaOf(count),
      });
      await page.setViewportSize({ width: 1000, height: 700 });
      await input(page).focus();
      await page.keyboard.press("Control+Home");
      await page.keyboard.press("Control+Space");
      await expect(list(page)).toBeVisible();
      return errors;
    }

    test("a list with nothing to scroll passes the wheel to the text", async ({
      page,
    }) => {
      const errors = await openList(page, 1);
      const box = await list(page).boundingBox();
      await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
      await page.mouse.wheel(0, 200);
      await expect.poll(async () => (await scrolled(page)).text).toBe(200);
      await expect(list(page)).toBeVisible();
      expect(errors).toEqual([]);
    });

    test("a long list scrolls itself, then the text at its end", async ({
      page,
    }) => {
      const errors = await openList(page, 40);
      const box = await list(page).boundingBox();
      await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
      await page.mouse.wheel(0, 100);
      await expect.poll(() => listTop(page)).toBeGreaterThan(0);
      expect((await scrolled(page)).text, "the text stays").toBe(0);
      await list(page).evaluate(
        (element) => (element.scrollTop = element.scrollHeight),
      );
      const end = await listTop(page);
      await page.mouse.wheel(0, 150);
      await expect.poll(async () => (await scrolled(page)).text).toBe(150);
      expect(await listTop(page)).toBe(end);
      expect(errors).toEqual([]);
    });
  });
});
