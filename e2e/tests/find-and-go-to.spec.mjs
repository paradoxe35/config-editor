import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

// The find widget and go to line: empty and no-match queries, the count, right-to-left pages, short
// windows, the query field's size.

// A YAML block scalar holding an empty line froze the
// tab (loading it, Enter in it, Edit as YAML of a string with a blank line);
// copy and cut on a folded header take the whole block; Enter on an empty
// `- ` item and inside a flow sequence; XML's `</` and an attribute quote
// before `/>`; HCL's `"${`; a find pattern matching empty text (`^`).

const saved = (page) => page.locator('.demo__output-text').textContent();

// Puts the caret at `offset` of the text on screen.
const caretAt = (e, offset) =>
  e.input.evaluate((element, at) => {
    element.focus();
    element.setSelectionRange(at, at);
  }, offset);

const input = (page) => page.locator('.config-editor__input');

const find = (page) => page.locator(".config-editor__find");

const count = (page) => page.locator(".config-editor__find-count");

const said = (page) => page.locator(".config-editor__announcement").first();

test.describe('go to line', () => {
  setupEditor(test, '?sample=large');

  test('the field has its box, and the hint lines up with the typed text', async ({ page }) => {
    const e = editor(page);
    await e.input.click();
    await page.keyboard.press('Control+g');
    const field = page.locator('.config-editor__goto-input');
    await expect(field).toBeFocused();
    const look = await field.evaluate((element) => {
      const style = getComputedStyle(element);
      const hint = document.querySelector('.config-editor__goto-hint');
      const hintStyle = getComputedStyle(hint);
      return {
        border: style.borderTopWidth,
        borderColor: style.borderTopColor,
        textLeft: element.getBoundingClientRect().left + parseFloat(style.borderLeftWidth) + parseFloat(style.paddingLeft),
        hintLeft: hint.getBoundingClientRect().left + parseFloat(hintStyle.paddingLeft),
      };
    });
    expect(look.border).toBe('1px');
    expect(look.borderColor).not.toBe('rgba(0, 0, 0, 0)');
    expect(Math.abs(look.textLeft - look.hintLeft)).toBeLessThanOrEqual(1);
  });
});

test.describe('find', () => {
  setupEditor(test, '?language=yaml');

  test('a pattern matching empty text finds and replaces at each place', async ({ page }) => {
    const e = editor(page);
    await e.paste('a: 1\nb: 2\n');
    await caretAt(e, 0);
    await page.keyboard.press('Control+h');
    const widget = page.locator('.config-editor__find');
    await widget.getByRole('button', { name: 'Use regular expression' }).click();
    await widget.getByRole('textbox', { name: 'Find' }).fill('^');
    await expect(widget.locator('.config-editor__find-count')).toHaveText('1 of 3');
    await widget.getByRole('textbox', { name: 'Replace' }).fill('# ');
    await widget.getByRole('button', { name: 'Replace all' }).click();
    await expect.poll(() => saved(page)).toBe('# a: 1\n# b: 2\n# ');
  });
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name, options) =>
    `${pathToFileURL(join(js, 'examples', name)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

  const input = (page) => page.locator('.config-editor__input');

  async function open(page, options) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(example('vanilla.html', options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    return errors;
  }

  /** Waits two frames: what was put back before the next frame is drawn is back. */
  const frames = (page) => page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));

  // Sixty keys in each language, so a page key has somewhere to go.
  const DOCUMENTS = {
    yaml: Array.from({ length: 60 }, (_, i) => `k${i}: ${i}\n`).join(''),
    json: `${JSON.stringify(Object.fromEntries(Array.from({ length: 60 }, (_, i) => [`k${i}`, i])), null, 2)}\n`,
    toml: Array.from({ length: 60 }, (_, i) => `k${i} = ${i}\n`).join(''),
  };

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('a short window', () => {
    for (const [width, height] of [
      [320, 180],
      [320, 256],
      [640, 300],
      [740, 360],
    ]) {
      test(`${width}x${height}: the text stays in its surface and the find widget in the window`, async ({ browser }) => {
        const context = await browser.newContext({ viewport: { width, height } });
        const page = await context.newPage();
        const errors = await open(page, { value: DOCUMENTS.yaml.split('\n').slice(0, 15).join('\n') });
        await input(page).focus();
        await page.keyboard.press('Control+End');
        await frames(page);
        const boxes = await page.evaluate(() => {
          const box = (selector) => document.querySelector(selector).getBoundingClientRect();
          const [surface, text, gutter] = ['.config-editor__surface', '.config-editor__input', '.config-editor__gutter'].map(box);
          return { surface: [surface.top, surface.bottom], text: [text.top, text.bottom], gutter: [gutter.top, gutter.bottom] };
        });
        // The textarea within its surface, the gutter beside all of it.
        expect(boxes.text[0]).toBeGreaterThanOrEqual(boxes.surface[0] - 1);
        expect(boxes.text[1]).toBeLessThanOrEqual(boxes.surface[1] + 1);
        expect(boxes.gutter[0]).toBeLessThanOrEqual(boxes.text[0] + 1);
        expect(boxes.gutter[1]).toBeGreaterThanOrEqual(boxes.text[1] - 1);

        await page.keyboard.press('Control+h');
        const find = page.locator('.config-editor__find');
        await expect(find).toBeVisible();
        const box = await find.boundingBox();
        expect(box.y).toBeGreaterThanOrEqual(0);
        expect(box.y + box.height).toBeLessThanOrEqual(height);
        expect(errors).toEqual([]);
        await context.close();
      });
    }

    // The page scrolled so the text area runs past the window's bottom (as CI's
    // fonts left it): the widget is capped to the part in the window, not to
    // the whole text area.
    for (const [width, height] of [
      [320, 180],
      [200, 150],
    ]) {
      test(`${width}x${height}, the text area past the window's bottom: the find widget stays in the window`, async ({ browser }) => {
        const context = await browser.newContext({ viewport: { width, height } });
        const page = await context.newPage();
        const errors = await open(page, { value: DOCUMENTS.yaml.split('\n').slice(0, 15).join('\n') });
        await input(page).focus();
        await page.keyboard.press('Control+h');
        const find = page.locator('.config-editor__find');
        await expect(find).toBeVisible();
        // Up to where a row of the text area still shows above the window's bottom.
        for (const past of [10, 30, Math.round(height / 4)]) {
          // Scroll the page so the text area's bottom is `past` px below the window.
          await page.evaluate((past) => {
            const area = document.querySelector('.config-editor__surface').getBoundingClientRect();
            scrollBy(0, area.bottom - innerHeight - past);
          }, past);
          await frames(page);
          const box = await find.boundingBox();
          expect(box.y + box.height, `${past} px past`).toBeLessThanOrEqual(height + 1);
        }
        expect(errors).toEqual([]);
        await context.close();
      });
    }
  });
});

// A read-only editor has no `Ctrl+M`
//; a fold key with nothing to do says so; the find
// widget's stepping and replacing buttons say they are unavailable, and do
// nothing, while the query finds nothing; the query field never
// narrows as the window widens; the demo's own toolbar
// (`ConfigEditorToolbar`) keeps the page's look under the host shield
//.
test.describe('the find widget with no match', () => {
  setupEditor(test, '');

  test('marks stepping and replacing unavailable, and they do nothing', async ({ page }) => {
    await input(page).click();
    await page.keyboard.press('Control+h');
    const buttons = page.locator(
      '.config-editor__find-actions .config-editor__find-button:not([aria-label="Close"]), .config-editor__replace-row .config-editor__find-button',
    );
    await expect(buttons).toHaveCount(4);
    await page.keyboard.type('listen');
    for (const button of await buttons.all()) await expect(button).not.toHaveAttribute('aria-disabled', 'true');
    await page.keyboard.press('Control+a');
    await page.keyboard.type('nothing-matches-this');
    await expect(page.locator('.config-editor__find-count')).toHaveText('No results');
    for (const button of await buttons.all()) await expect(button).toHaveAttribute('aria-disabled', 'true');
    const before = await input(page).inputValue();
    await page.locator('.config-editor__replace-input').fill('x');
    for (const button of await buttons.all()) {
      await button.click({ force: true });
      await button.focus();
      await page.keyboard.press('Enter');
      // Unavailable, not gone: the focus stays.
      await expect(button).toBeFocused();
    }
    expect(await input(page).inputValue()).toBe(before);
    await expect(page.locator('.config-editor__find-count')).toHaveText('No results');
  });

  test('"All" leaves the focus on the button it disables', async ({ page }) => {
    await input(page).click();
    await page.keyboard.press('Control+h');
    await page.keyboard.type('listen');
    await page.locator('.config-editor__replace-input').fill('port');
    const all = page.locator('.config-editor__replace-row .config-editor__find-button').last();
    await all.focus();
    await page.keyboard.press('Enter');
    await expect(all).toHaveAttribute('aria-disabled', 'true');
    await expect(all).toBeFocused();
  });
});

for (const query of ['', '?readonly']) {
  test.describe(`the query field as the window widens${query ? ', read-only' : ''}`, () => {
    setupEditor(test, query);

    for (const typed of ['nothing-matches-this', 'listen']) {
      test(`never narrows: "${typed}"`, async ({ page }) => {
        await input(page).click();
        await page.keyboard.press('Control+f');
        await page.keyboard.type(typed);
        const widths = [];
        for (let width = 280; width <= 1100; width += 10) {
          await page.setViewportSize({ width, height: 620 });
          widths.push([width, await page.locator('.config-editor__find-field .config-editor__find-input').evaluate((element) => element.getBoundingClientRect().width)]);
        }
        const narrower = widths.filter(([, field], index) => index > 0 && field < widths[index - 1][1] - 0.5);
        expect(narrower, 'window widths where the field got narrower').toEqual([]);
        // At its widest it fills its row.
        expect(widths.at(-1)[1]).toBeGreaterThan(140);
      });
    }
  });
}

// An empty find query shows and announces no
// count; in a right-to-left page the find widget stays at the end of the code lines
// side, though the editor's text stays left to right; the demo's
// unsaved-changes dot is a graphic (3:1 needed, 4.5:1 held, as it is also a glyph) against its background in every
// theme.
test.describe("the find count", () => {
  setupEditor(test, "");

  test("is empty, and says nothing, for an empty query", async ({ page }) => {
    await input(page).click();
    await page.keyboard.press("Control+f");
    await expect(find(page)).toBeVisible();
    await expect(count(page)).toHaveText("");
    await expect(said(page)).not.toHaveText(/No results/);
    await page.keyboard.type("zzzqqq");
    await expect(count(page)).toHaveText("No results");
    await page.keyboard.press("Control+a");
    await page.keyboard.press("Backspace");
    await expect(count(page)).toHaveText("");
  });
});

test.describe("the find widget in a right-to-left page", () => {
  setupEditor(test, "");

  // The editor's text is always left to right, so its widgets stay at the end
  // of the code lines (right), where they do not hide the start of a line.
  test("stays at the right and does not cover the current match", async ({
    page,
  }) => {
    await page.evaluate(() => (document.documentElement.dir = "rtl"));
    await input(page).click();
    await page.keyboard.type("alpha: 1");
    await page.keyboard.press("Control+f");
    await expect(find(page)).toBeVisible();
    await page.keyboard.type("alpha");
    await expect(
      page.locator(".config-editor__search-match-current"),
    ).toHaveCount(1);
    const { start, end, overlap } = await page.evaluate(() => {
      const wrap = document
        .querySelector(".config-editor__input-wrap")
        .getBoundingClientRect();
      const box = document
        .querySelector(".config-editor__find")
        .getBoundingClientRect();
      const match = document
        .querySelector(".config-editor__search-match-current")
        .getBoundingClientRect();
      const overlap =
        box.left < match.right &&
        match.left < box.right &&
        box.top < match.bottom &&
        match.top < box.bottom;
      return {
        start: box.left - wrap.left,
        end: wrap.right - box.right,
        overlap,
      };
    });
    expect(end).toBeLessThan(start);
    expect(overlap).toBe(false);
    expect(
      await input(page).evaluate(
        (element) => getComputedStyle(element).direction,
      ),
    ).toBe("ltr");
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

  const said = (page) => page.locator(".config-editor__announcement").first();

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

  test.skip(!built, "run `npm run build` in js/ first");

  test("clearing a query that said “No results” takes it back from the live region", async ({
    page,
  }) => {
    const errors = await open(page, { value: "a: 1\n" });
    await input(page).focus();
    await page.keyboard.press("Control+f");
    await page.keyboard.type("zzzqqq");
    await expect(said(page)).toHaveText(/No results/);
    await page.keyboard.press("Control+a");
    await page.keyboard.press("Backspace");
    await expect(page.locator(".config-editor__find-count")).toHaveText("");
    await expect(said(page)).not.toHaveText(/No results/);
    // Typed again, it says so again.
    await page.keyboard.type("zz");
    await expect(said(page)).toHaveText(/No results/);
    expect(errors).toEqual([]);
  });
});
