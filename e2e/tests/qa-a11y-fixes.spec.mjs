import { expect, test } from "@playwright/test";
import { editor, pointAt, setupEditor } from "./helpers.mjs";

// Accessibility fixes found by QA, as a user meets them:
//  - D1.a11y-1 / D21.a11y-1: the demo's "Unsaved changes" question is a
//    modal dialog (focus in, Tab kept inside, Escape cancels, focus back,
//    inside the window at 400% zoom);
//  - D2.a11y-1: a refused save is said through the live region;
//  - D3.ux-1: Ctrl+K Ctrl+I and a tap on a read-only key show its docs card
//    and say why it is read-only;
//  - D4.a11y-1: a user's line-height (WCAG 1.4.12) moves the painted rows
//    with the textarea's lines.

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

test.describe("the unsaved-changes question is a modal dialog (D1.a11y-1, D21.a11y-1)", () => {
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

test.describe("a refused save is announced (D2.a11y-1)", () => {
  setupEditor(test, "?language=yaml");

  test("Ctrl+S on text the host language cannot hold says why", async ({
    page,
  }) => {
    const e = editor(page);
    await page.locator(".demo__edit-select").selectOption("json");
    await expect.poll(() => e.value()).toContain("{");
    await e.paste("{");
    const announcement = page.locator(".config-editor__announcement");
    await expect(announcement).toHaveText("");
    await page.keyboard.press("Control+s");
    await expect(announcement).not.toHaveText("");
    expect(await announcement.textContent()).toMatch(/\S/);
    // Nothing was sent.
    await expect(page.locator(".demo__output-text")).not.toContainText("{");
  });
});

test.describe("a refused save in the documents view is announced (D2.a11y-1)", () => {
  setupEditor(test, "?documents");

  test("Ctrl+S in an unconvertible Edit-as view says why", async ({ page }) => {
    await page.locator(".demo__edit-as").click();
    const input = panel(page, "main").locator(".config-editor__input");
    await expect(input).toHaveValue(/^\{/);
    await input.click();
    await page.keyboard.press("Control+a");
    await page.keyboard.type("{");
    const announcement = panel(page, "main").locator(
      ".config-editor__announcement",
    );
    await page.waitForTimeout(700);
    await expect(announcement).toHaveText("");
    await page.keyboard.press("Control+s");
    await expect(announcement).not.toHaveText("");
  });
});

test.describe("a read-only key shows its docs and says why (D3.ux-1)", () => {
  test.describe("keyboard", () => {
    setupEditor(test, "?schema=protected");

    test("Ctrl+K Ctrl+I on a read-only key", async ({ page }) => {
      const e = editor(page);
      const line = (await e.value())
        .split("\n")
        .findIndex((text) => text.startsWith("id"));
      const at = await pointAt(page, line, 1);
      await page.mouse.click(at.x, at.y);
      await page.keyboard.press("Control+k");
      await page.keyboard.press("Control+i");
      const card = page.locator(".config-editor__hover");
      await expect(card).toBeVisible();
      await expect(card.locator(".config-editor__hover-key")).toHaveText("id");
      const announcement = page.locator(".config-editor__announcement");
      await expect(announcement).toContainText("Assigned by the server");
      await expect(page.locator(".config-editor__protect-tip")).toContainText(
        "Assigned by the server",
      );
    });

    test("an editable key still shows its card alone", async ({ page }) => {
      const e = editor(page);
      const line = (await e.value())
        .split("\n")
        .findIndex((text) => text.startsWith("name"));
      const at = await pointAt(page, line, 1);
      await page.mouse.click(at.x, at.y);
      await page.keyboard.press("Control+k");
      await page.keyboard.press("Control+i");
      await expect(page.locator(".config-editor__hover")).toBeVisible();
      await expect(page.locator(".config-editor__protect-tip")).toHaveCount(0);
    });
  });

  test.describe("touch", () => {
    test.use({ hasTouch: true });
    setupEditor(test, "?schema=protected");

    test("a tap on a read-only key", async ({ page }) => {
      const e = editor(page);
      const line = (await e.value())
        .split("\n")
        .findIndex((text) => text.startsWith("id"));
      const at = await pointAt(page, line, 1);
      await page.touchscreen.tap(at.x, at.y);
      await expect(page.locator(".config-editor__hover")).toBeVisible();
      await expect(page.locator(".config-editor__announcement")).toContainText(
        "Assigned by the server",
      );
    });
  });
});

test.describe("a user line-height keeps the painted rows on the textarea lines (D4.a11y-1)", () => {
  for (const lineHeight of ["1.5", "2"]) {
    test.describe(`line-height ${lineHeight} !important`, () => {
      setupEditor(test, "?sample=large");

      test("rows, gutter and find highlights follow the textarea", async ({
        page,
      }) => {
        await page.addStyleTag({
          content: `* { line-height: ${lineHeight} !important; }`,
        });
        const e = editor(page);
        await e.input.click();
        await page.keyboard.press("Control+f");
        await page.keyboard.type("listen");
        await page.waitForTimeout(500);
        const drift = await page.evaluate(() => {
          const area = document.querySelector(".config-editor__input");
          const top = area.getBoundingClientRect().top;
          const style = getComputedStyle(area);
          const pad = parseFloat(style.paddingTop);
          const lineHeight = parseFloat(style.lineHeight);
          const expected = (line) =>
            top + pad + (line - 1) * lineHeight - area.scrollTop;
          const rows = (selector, attribute) =>
            [...document.querySelectorAll(selector)].map((row) => ({
              line: Number(row.getAttribute(attribute)),
              drift:
                row.getBoundingClientRect().top -
                expected(Number(row.getAttribute(attribute))),
              height: row.getBoundingClientRect().height,
            }));
          return {
            lineHeight,
            code: rows(".config-editor__code-line", "data-line"),
            gutter: rows(".config-editor__gutter-content > div", "data-line"),
            matches: [
              ...document.querySelectorAll(".config-editor__search-match"),
            ].map((match) => {
              const rect = match.getBoundingClientRect();
              return { top: rect.top, height: rect.height };
            }),
            keys: [
              ...document.querySelectorAll(".config-editor__code-line"),
            ].map((row) => ({
              line: Number(row.getAttribute("data-line")),
              text: row.textContent,
              top: row.getBoundingClientRect().top,
            })),
          };
        });
        expect(drift.code.length).toBeGreaterThan(10);
        for (const row of [...drift.code, ...drift.gutter]) {
          expect(Math.abs(row.drift), `line ${row.line}`).toBeLessThanOrEqual(
            1,
          );
          expect(
            Math.abs(row.height - drift.lineHeight),
            `line ${row.line} height`,
          ).toBeLessThanOrEqual(1);
        }
        // A find highlight sits on its code row.
        expect(drift.matches.length).toBeGreaterThan(0);
        const row = drift.keys.find((row) => row.text.includes("listen"));
        const match = drift.matches[0];
        expect(Math.abs(match.top - row.top)).toBeLessThanOrEqual(1);
        expect(Math.abs(match.height - drift.lineHeight)).toBeLessThanOrEqual(
          1,
        );
      });

      test("a click on a row lands on its line", async ({ page }) => {
        await page.addStyleTag({
          content: `* { line-height: ${lineHeight} !important; }`,
        });
        const e = editor(page);
        await e.input.click();
        const target = await page.evaluate(() => {
          const row = document.querySelector(
            '.config-editor__code-line[data-line="12"]',
          );
          const rect = row.getBoundingClientRect();
          return { x: rect.left + 40, y: rect.top + rect.height / 2 };
        });
        await page.mouse.click(target.x, target.y);
        const line = await e.input.evaluate(
          (area) => area.value.slice(0, area.selectionStart).split("\n").length,
        );
        expect(line).toBe(12);
      });
    });
  }
});
