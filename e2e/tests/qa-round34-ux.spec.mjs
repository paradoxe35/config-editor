import { expect, test } from "@playwright/test";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

// QA round 34, UX: the wheel and a finger over the line numbers scroll the
// text as over the text itself, the page only at the text's ends (UX34-1);
// clearing a find query that said "No results" takes that back from the
// live region (UX34-5). Ctrl+Enter on a folded line is in
// line-commands.spec.mjs (UX34-4). Through the npm package's page (run
// `npm run build` in js/ first).
const js = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "js");
const built = existsSync(
  join(js, "config-editor", "pkg", "config_editor_bg.wasm"),
);
const pageUrl = (options) =>
  `${pathToFileURL(join(js, "examples", "vanilla.html")).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

test.skip(!built, "run `npm run build` in js/ first");

const input = (page) => page.locator(".config-editor__input").first();
const gutter = (page) => page.locator(".config-editor__gutter").first();
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

const lines = (count) =>
  Array.from({ length: count }, (_, i) => `key_${i}: ${i}\n`).join("");
const scrolled = (page) =>
  page.evaluate(() => ({
    page: Math.round(scrollY),
    text: Math.round(document.querySelector(".config-editor__input").scrollTop),
  }));

/** A one-finger drag from `(x, y)` by `dy` pixels. */
async function drag(page, cdp, [x, y], dy) {
  await cdp.send("Input.dispatchTouchEvent", {
    type: "touchStart",
    touchPoints: [{ x, y }],
  });
  for (let step = 1; step <= 12; step += 1) {
    await cdp.send("Input.dispatchTouchEvent", {
      type: "touchMove",
      touchPoints: [{ x, y: y + (dy * step) / 12 }],
    });
    await page.waitForTimeout(16);
  }
  await cdp.send("Input.dispatchTouchEvent", {
    type: "touchEnd",
    touchPoints: [],
  });
}

test.describe("the line numbers scroll the text (UX34-1)", () => {
  for (const language of ["yaml", "json"]) {
    test(`${language}: the wheel over the gutter scrolls the text, then the page at its end`, async ({
      page,
    }) => {
      const value =
        language === "yaml"
          ? lines(200)
          : `{\n${Array.from({ length: 200 }, (_, i) => `  "k${i}": ${i}`).join(",\n")}\n}\n`;
      const errors = await open(page, { value, language });
      await page.setViewportSize({ width: 1000, height: 700 });
      const box = await gutter(page).boundingBox();
      await page.mouse.move(box.x + 12, box.y + 120);
      await page.mouse.wheel(0, 200);
      await expect.poll(async () => (await scrolled(page)).text).toBe(200);
      expect((await scrolled(page)).page, "the page stays").toBe(0);
      await page.mouse.wheel(0, -120);
      await expect.poll(async () => (await scrolled(page)).text).toBe(80);
      // At the text's end, as over the text: the page goes on.
      await input(page).evaluate(
        (element) => (element.scrollTop = element.scrollHeight),
      );
      const end = (await scrolled(page)).text;
      await page.mouse.wheel(0, 150);
      await expect.poll(async () => (await scrolled(page)).page).toBe(150);
      expect((await scrolled(page)).text).toBe(end);
      expect(errors).toEqual([]);
    });
  }

  test("a finger dragged over the gutter scrolls the text, not the page; at a short text, the page", async ({
    browser,
  }) => {
    const context = await browser.newContext({
      viewport: { width: 390, height: 700 },
      hasTouch: true,
      isMobile: true,
    });
    const page = await context.newPage();
    const errors = await open(page, { value: lines(200) });
    const cdp = await context.newCDPSession(page);
    const box = await gutter(page).boundingBox();
    const at = [box.x + Math.min(12, box.width / 2), box.y + 220];
    await drag(page, cdp, at, -180);
    await expect
      .poll(async () => (await scrolled(page)).text)
      .toBeGreaterThan(100);
    expect((await scrolled(page)).page, "the page stays").toBe(0);
    // Back down: the text scrolls back to its top, the page stays.
    await drag(page, cdp, at, 400);
    await expect.poll(async () => (await scrolled(page)).text).toBe(0);
    expect((await scrolled(page)).page).toBe(0);
    // A text with nowhere to scroll: the drag goes on to the page, as a
    // drag over the text does.
    await page.evaluate(() => window.editor.update({ value: "a: 1\nb: 2\n" }));
    await expect.poll(async () => (await scrolled(page)).text).toBe(0);
    const short = await gutter(page).boundingBox();
    await drag(page, cdp, [at[0], short.y + short.height / 2], -150);
    await expect.poll(async () => (await scrolled(page)).page).toBeGreaterThan(0);
    expect(errors).toEqual([]);
    await context.close();
  });
});

test("clearing a query that said “No results” takes it back from the live region (UX34-5)", async ({
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
