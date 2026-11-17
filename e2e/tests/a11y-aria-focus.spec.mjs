import { expect, test } from '@playwright/test';
import { existsSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

// Roles, names and focus: the unsaved-changes dialog, the textarea and its list, aria-keyshortcuts,
// Ctrl+M, notices that name keys, and the example pages' headings and landmarks.

const tab = (page, id) =>
  page.locator(`[role="tab"][data-document-id="${id}"]`);

const panel = (page, id) =>
  page.locator(`[role="tabpanel"][data-document-id="${id}"]`);

const status = (page) => page.locator(".demo__documents-status");

const dialog = (page) => page.locator("dialog.demo__ask");

const inDialog = (page) =>
  page.evaluate(() => !!document.activeElement?.closest("dialog"));

const focusedName = (page) =>
  page.evaluate(
    () =>
      document.activeElement?.getAttribute("role") ??
      document.activeElement?.tagName,
  );

async function dirtyMain(page) {
  await panel(page, "main").locator(".config-editor__input").click();
  await page.keyboard.press("Control+End");
  await page.keyboard.type("# unsaved");
}

const input = (page) => page.locator('.config-editor__input');

const said = (page) => page.locator('.config-editor__announcement').first();

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

const built = existsSync(join(root, 'js', 'config-editor', 'pkg', 'config_editor_bg.wasm'));

// Every example page (found on disk, so a new page is covered by itself): one
// heading, one main landmark and no control under 24x24.
const examples = join(root, 'js', 'examples');

// Accessibility fixes, as a user meets them:
//  - the demo's "Unsaved changes" question is a
//    modal dialog (focus in, Tab kept inside, Escape cancels, focus back,
//    inside the window at 400% zoom);
//  - a refused save is said through the live region;
//  - Ctrl+K Ctrl+I and a tap on a read-only key show its docs card
//    and say why it is read-only;
//  - a user's line-height (WCAG 1.4.12) moves the painted rows
//    with the textarea's lines.
test.describe("the unsaved-changes question is a modal dialog", () => {
  setupEditor(test, "?documents");

  test("from the keyboard: focus moves in, Tab stays inside, Escape cancels and focus goes back", async ({
    page,
  }) => {
    await dirtyMain(page);
    await tab(page, "main").focus();
    await page.keyboard.press("Delete");
    await expect(dialog(page)).toBeVisible();
    await expect(dialog(page)).toHaveAttribute("role", "alertdialog");
    expect(await inDialog(page)).toBe(true);
    // The page behind is out of reach: every Tab, either way, stays inside.
    for (let step = 0; step < 6; step++) {
      await page.keyboard.press("Tab");
      expect(await inDialog(page), `Tab ${step}`).toBe(true);
    }
    for (let step = 0; step < 6; step++) {
      await page.keyboard.press("Shift+Tab");
      expect(await inDialog(page), `Shift+Tab ${step}`).toBe(true);
    }
    await page.keyboard.press("Escape");
    await expect(dialog(page)).toHaveCount(0);
    await expect(status(page)).toHaveAttribute(
      "data-open",
      "main,limits,clients",
    );
    await expect(tab(page, "main")).toBeFocused();
    // Nothing typed behind it while it was open.
    await expect(
      panel(page, "main").locator(".config-editor__input"),
    ).toHaveValue(/# unsaved$/);
  });

  // It starts on the choice that loses nothing, names the
  // tab's file and describes itself.
  test("it starts on Cancel, names the tab's file and has its message as description", async ({
    page,
  }) => {
    await dirtyMain(page);
    await tab(page, "main").focus();
    await page.keyboard.press("Delete");
    await expect(dialog(page)).toBeVisible();
    await expect(page.getByRole("button", { name: "Cancel" })).toBeFocused();
    await expect(dialog(page)).toContainText("tunnel.yaml");
    const described = await dialog(page).evaluate((element) =>
      document.getElementById(element.getAttribute("aria-describedby") ?? "")?.textContent,
    );
    expect(described).toBe("Discard unsaved changes to tunnel.yaml?");
    // Enter right away keeps the document.
    await page.keyboard.press("Enter");
    await expect(dialog(page)).toHaveCount(0);
    await expect(status(page)).toHaveAttribute("data-open", "main,limits,clients");
  });

  test("Cancel and Discard from the keyboard: focus returns to the tab, or the next one", async ({
    page,
  }) => {
    await dirtyMain(page);
    await tab(page, "main").focus();
    await page.keyboard.press("Delete");
    await expect(dialog(page)).toBeVisible();
    await page.getByRole("button", { name: "Cancel" }).focus();
    await page.keyboard.press("Enter");
    await expect(dialog(page)).toHaveCount(0);
    await expect(tab(page, "main")).toBeFocused();

    await page.keyboard.press("Delete");
    await expect(dialog(page)).toBeVisible();
    await page.getByRole("button", { name: "Discard" }).focus();
    await page.keyboard.press("Enter");
    await expect(status(page)).toHaveAttribute("data-open", "limits,clients");
    expect(await focusedName(page)).not.toBe("BODY");
  });

  test("it fits the window at 400% zoom", async ({ page }) => {
    await page.setViewportSize({ width: 320, height: 225 });
    await dirtyMain(page);
    await tab(page, "main").focus();
    await page.keyboard.press("Delete");
    await expect(dialog(page)).toBeVisible();
    const box = await dialog(page).boundingBox();
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.y).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(320);
    expect(box.y + box.height).toBeLessThanOrEqual(225);
    for (const name of ["Discard", "Cancel"]) {
      await expect(page.getByRole("button", { name })).toBeInViewport({
        ratio: 1,
      });
    }
  });
});

test.describe('the suggestion list, for screen readers', () => {
  setupEditor(test);

  test('the textarea names the list and its highlighted suggestion', async ({ page }) => {
    const e = editor(page);
    const list = page.locator('.config-editor__completions');
    await e.input.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('li');
    await expect(list.locator('[role="option"]').first()).toBeVisible();
    const listId = await list.getAttribute('id');
    expect(listId).toBeTruthy();
    await expect(e.input).toHaveAttribute('aria-controls', listId);
    const active = await e.input.getAttribute('aria-activedescendant');
    expect(active).toBeTruthy();
    await expect(page.locator(`#${active}`)).toHaveAttribute('aria-selected', 'true');
    await expect(list.locator('[role="option"][aria-selected="true"]')).toHaveCount(1);
    const options = await list.locator('[role="option"]').count();
    await expect(list.locator('[role="option"][aria-selected="false"]')).toHaveCount(options - 1);
    // The accessibility tree has the selected option.
    expect(await list.ariaSnapshot()).toMatch(/- option ".+" \[selected\]/);
    if (options > 1) {
      await page.keyboard.press('ArrowDown');
      const next = await e.input.getAttribute('aria-activedescendant');
      expect(next).not.toBe(active);
      await expect(page.locator(`#${next}`)).toHaveAttribute('aria-selected', 'true');
      await expect(page.locator(`#${active}`)).toHaveAttribute('aria-selected', 'false');
    }
    await page.keyboard.press('Escape');
    await expect(list.locator('[role="option"]')).toHaveCount(0);
    await expect(e.input).not.toHaveAttribute('aria-activedescendant', /./);
    // Closed: it controls no list (the open state shows by it).
    await expect(e.input).not.toHaveAttribute('aria-controls', /./);
  });
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const vanilla = (options = {}) =>
    `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

  const input = (page) => page.locator('.config-editor__input');

  async function open(page, options) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(vanilla(options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    return errors;
  }

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('the textarea, Ctrl+K Ctrl+I, docs links and the footer', () => {
    test.use({ viewport: { width: 1000, height: 600 } });
    test('the textarea is a multi-line text box; the open list shows through aria-controls, not aria-expanded', async ({ page }) => {
      const errors = await open(page, { value: 'port: 1\n', schema: JSON.stringify({ type: 'object', properties: { port: {}, host: {} } }) });
      await expect(input(page)).toHaveAttribute('role', 'textbox');
      await expect(input(page)).toHaveAttribute('aria-multiline', 'true');
      await input(page).click();
      await page.keyboard.press('Control+End');
      await page.keyboard.press('Control+Space');
      await expect(page.locator('.config-editor__completion-active')).toBeVisible();
      await expect(input(page)).not.toHaveAttribute('aria-expanded', /./);
      await expect(input(page)).toHaveAttribute('aria-activedescendant', /./);
      expect(errors).toEqual([]);
    });
    const listenSchema = JSON.stringify({
      type: 'object',
      properties: { config: { type: 'object', properties: { listen: { type: 'integer', description: 'Port to listen on.' } } } },
    });
    const values = {
      yaml: ['config:\n  listen: 80\n', '80'],
      json: ['{\n  "config": {\n    "listen": 80\n  }\n}\n', '80'],
      toml: ['[config]\nlisten = 80\n', '80'],
      hcl: ['config {\n  listen = 80\n}\n', '80'],
      xml: ['<config>\n  <listen>80</listen>\n</config>\n', '80'],
    };
  });
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  // Closing a tab with Delete keeps the focus on
  // the tabs; the hover card's Documentation link is reached by Alt+Enter,
  // named to screen readers and reached by Tab in `Ctrl+M` mode; the editor
  // stays left to right in a right-to-left page; forced colors mark the
  // selected tab; a diff marker's Enter focuses Revert and the preview sits
  // right after the text in the Tab order; the textarea controls the list
  // only while it is open. Through the npm package's example pages (run
  // `npm run build` in js/ first).
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name, options) =>
    `${pathToFileURL(join(js, 'examples', name)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

  const input = (page) => page.locator('.config-editor__input');

  async function open(page, name, options) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(example(name, options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).first().waitFor();
    return errors;
  }

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('the open suggestion list', () => {
    test('the textarea controls the list only while it is open', async ({ page }) => {
      const errors = await open(page, 'vanilla.html', {
        value: 'port: 1\n',
        schema: JSON.stringify({ type: 'object', properties: { port: {}, host: {} } }),
      });
      await expect(input(page)).toHaveAttribute('role', 'textbox');
      await expect(input(page)).not.toHaveAttribute('aria-controls', /./);
      await input(page).click();
      await page.keyboard.press('Control+End');
      await page.keyboard.press('Control+Space');
      const list = page.locator('.config-editor__completions');
      await expect(list.locator('[role="option"]').first()).toBeVisible();
      await expect(input(page)).toHaveAttribute('aria-controls', await list.getAttribute('id'));
      await expect(input(page)).toHaveAttribute('aria-activedescendant', /./);
      await page.keyboard.press('Escape');
      await expect(list.locator('[role="option"]')).toHaveCount(0);
      await expect(input(page)).not.toHaveAttribute('aria-controls', /./);
      await expect(input(page)).not.toHaveAttribute('aria-activedescendant', /./);
      expect(errors).toEqual([]);
    });
  });
});

// A group of its own (needs the built npm packages, a 900×500 window).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const pageUrl = (options) =>
    `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

  const input = (page) => page.locator('.config-editor__input');

  async function open(page, options) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(pageUrl(options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    return errors;
  }

  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 900, height: 500 } });

  // The notice that the text changed outside the editor said how to
  // reach its choices with F6 on a touch screen too, where they are tapped
  // (as the footer's hint says "tap" there).
  for (const touch of [true, false]) {
    test.describe(`in ${touch ? 'touch' : 'mouse'}`, () => {
      test.use(touch ? { viewport: { width: 360, height: 700 }, hasTouch: true, isMobile: true } : {});
      test('the changed-outside notice names F6 only with a keyboard', async ({ page }) => {
        const errors = await open(page, { value: 'listen: 7835\n' });
        await input(page).focus();
        await page.keyboard.press('Control+End');
        await page.keyboard.type('a: 1\n');
        await page.locator('#external-change').click();
        const notice = page.locator('.config-editor__notice');
        await expect(notice).toBeVisible();
        if (touch) await expect(notice).not.toContainText('F6');
        else await expect(notice).toContainText('F6 moves to the choices and back.');
        expect(errors).toEqual([]);
      });
    });
  }
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const pageUrl = (options) =>
    `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

  const input = (page) => page.locator('.config-editor__input').first();

  async function open(page, options) {
    await page.goto(pageUrl(options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    await input(page).click();
  }

  test.skip(!built, 'run `npm run build` in js/ first');

  test('aria-keyshortcuts names F8 only while the problems are on', async ({ page }) => {
    await open(page);
    await expect(input(page)).toHaveAttribute('aria-keyshortcuts', /\bF8\b.*\bShift\+F8\b/);
    await page.evaluate(() => window.editor.update({ showDiagnostics: false }));
    await expect(input(page)).not.toHaveAttribute('aria-keyshortcuts', /F8/);
    await page.evaluate(() => window.editor.update({ showDiagnostics: true }));
    await expect(input(page)).toHaveAttribute('aria-keyshortcuts', /\bShift\+F8\b/);
  });
});

test.describe('aria-keyshortcuts', () => {
  setupEditor(test, '?language=yaml');

  test('names the line, comment, fold and move chords the README lists', async ({ page }) => {
    const named = ((await input(page).getAttribute('aria-keyshortcuts')) ?? '').split(' ');
    for (const chord of ['Control+Shift+[', 'Control+Shift+]', 'Control+/', 'Control+Shift+K', 'Control+]', 'Control+[',
      'Control+Enter', 'Control+Shift+Enter', 'Alt+ArrowUp', 'Alt+ArrowDown', 'Shift+Alt+ArrowUp', 'Control+Z', 'Alt+F3', 'F3']) {
      expect(named, chord).toContain(chord);
    }
    // Keys of the widgets and lists, not of the text, are not named.
    for (const chord of ['Alt+C', 'Control+Alt+Enter']) expect(named, chord).not.toContain(chord);
  });
});

test.describe('Ctrl+M in an editable editor', () => {
  setupEditor(test, '');

  test('switches Tab and is named', async ({ page }) => {
    await input(page).click();
    await page.keyboard.press('Control+m');
    await expect(said(page)).toHaveText(/Tab/);
    await expect(page.locator('.config-editor__tab-focus-status')).toBeVisible();
    const named = ((await input(page).getAttribute('aria-keyshortcuts')) ?? '').split(' ');
    expect(named).toContain('Control+M');
  });
});

for (const name of readdirSync(examples).filter((file) => file.endsWith('.html')).sort()) {
  test.describe(`${name}`, () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');

    test('has a heading, a main landmark and controls of 24px', async ({ page }) => {
      await page.goto(pathToFileURL(join(examples, name)).href);
      // The pages mark themselves done (`data-ready`, or `data-result` for the load test).
      await page.locator('body[data-ready], body[data-result], header button').first().waitFor({ state: 'attached' });
      await expect(page.locator('h1')).toHaveCount(1);
      await expect(page.locator('main')).toHaveCount(1);
      const small = await page.evaluate(() =>
        [...document.querySelectorAll('button, select, input:not([type="hidden"]), label, summary, [role="button"]')]
          .map((element) => [element.id || element.className || element.tagName, element.getBoundingClientRect()])
          .filter(([, box]) => box.width > 0 && (box.height < 24 || box.width < 24))
          .map(([id, box]) => `${id} ${box.width}x${box.height}`),
      );
      expect(small, 'controls under 24x24').toEqual([]);
    });
  });
}
