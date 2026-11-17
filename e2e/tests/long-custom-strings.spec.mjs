import { expect, test } from "@playwright/test";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

// A host can set any text through `strings`, in any language. A long hover
// label leaves the value a usable width (the value goes under it), a long
// footer text wraps onto its own row, and a long find button wraps inside
// the widget: nothing is cut, squeezed to a sliver or past its card.

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const js = join(root, "js");
const built = existsSync(
  join(js, "config-editor", "pkg", "config_editor_bg.wasm"),
);
const example = (name) => pathToFileURL(join(js, "examples", name)).href;

const schema = JSON.stringify({
  type: "object",
  properties: {
    plain: { type: "integer" },
    name: {
      type: "string",
      description: "A name.",
      default: "a-default-value",
      examples: ["first-example", "second"],
    },
  },
  additionalProperties: false,
});

async function mount(page, strings, value) {
  await page.goto(example("vanilla.html"));
  await expect(page.locator("body")).toHaveAttribute("data-ready", "true", {
    timeout: 20000,
  });
  await page.evaluate(
    async ([strings, schema, value]) => {
      const module = await import("../config-editor/index.js");
      document.getElementById("editor")?.replaceChildren();
      const host = document.createElement("div");
      document.body.prepend(host);
      host.style.cssText = "position:relative;z-index:50;background:#fff";
      window.editor = new module.ConfigEditor(host, {
        value,
        schema,
        showFooter: true,
        strings,
      });
    },
    [strings, schema, value],
  );
  await expect(page.locator(".config-editor__input").first()).toBeVisible();
}

const LABELS = {
  "several words": {
    examples: "Examples of valid values you could write here",
    defaultValue: "Default value used when nothing is written",
  },
  "one long word": {
    examples: "Beispielwerteunddasisteinsehrlangeswort",
    defaultValue: "Standardwertwennnichtangegeben",
  },
};

for (const [name, viewport] of [
  ["desktop", { width: 1100, height: 600 }],
  ["320 px", { width: 320, height: 560 }],
]) {
  test.describe(`long custom text at ${name}`, () => {
    test.skip(!built, "js packages not built: run `npm run build` in js/");
    test.use({ viewport });

    for (const [kind, strings] of Object.entries(LABELS)) {
      test(`a hover label of ${kind} leaves the value a usable width`, async ({
        page,
      }) => {
        await mount(page, strings, "name: x\n");
        const key = page
          .locator(".config-editor__key", { hasText: "name" })
          .first();
        const box = await key.boundingBox();
        await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2, {
          steps: 4,
        });
        await expect(
          page.locator(".config-editor__hover-meta").first(),
        ).toBeVisible();
        const metas = await page.evaluate(() => {
          const card = document
            .querySelector(".config-editor__hover")
            .getBoundingClientRect();
          return [
            ...document.querySelectorAll(".config-editor__hover-meta"),
          ].map((meta) => {
            const label = meta.querySelector("span").getBoundingClientRect();
            const value = meta.querySelector("code").getBoundingClientRect();
            return {
              text: meta.querySelector("span").textContent,
              valueWidth: value.width,
              labelWidth: label.width,
              cardWidth: card.width,
              past: Math.max(label.right, value.right) - card.right,
              viewportPast: card.right - innerWidth,
            };
          });
        });
        expect(metas.length).toBe(2);
        for (const meta of metas) {
          expect(meta.valueWidth, meta.text).toBeGreaterThanOrEqual(80);
          expect(meta.labelWidth, meta.text).toBeLessThanOrEqual(
            meta.cardWidth,
          );
          expect(meta.past, meta.text).toBeLessThanOrEqual(1);
          expect(meta.viewportPast, meta.text).toBeLessThanOrEqual(0);
        }
      });
    }
  });
}

test.describe("long footer and find texts at 320 px", () => {
  test.skip(!built, "js packages not built: run `npm run build` in js/");
  test.use({ viewport: { width: 320, height: 640 } });

  const strings = {
    errorsOne: "{count} fehlerhafte Stelle wurde in dieser Datei gefunden",
    errorsOther: "{count} fehlerhafte Stellen wurden in dieser Datei gefunden",
    changesOne: "{count} Änderung gegenüber der gespeicherten Fassung",
    changesOther: "{count} Änderungen gegenüber der gespeicherten Fassung",
    linesOne: "{count} Zeile insgesamt in der Datei",
    linesOther: "{count} Zeilen insgesamt in der Datei",
    problemsHintTouch:
      "Auf unterstrichenen Text tippen, um die Probleme zu lesen",
    replaceAll: "Alle ersetzen und danach speichern",
    replace: "Ersetzen durch den eingegebenen Text",
  };

  test("the footer keeps a reasonable height and every part its width", async ({
    page,
  }) => {
    await mount(page, strings, "plain: x\nbad: 1\nz: 2\n");
    await page.locator(".config-editor__input").click();
    await page.keyboard.type("# ");
    await expect(
      page.locator(".config-editor__diagnostic-status"),
    ).toContainText("fehlerhafte");
    const footer = await page.evaluate(() => {
      const status = document
        .querySelector(".config-editor__status")
        .getBoundingClientRect();
      const part = (selector) => {
        const element = document.querySelector(selector);
        if (!element) return null;
        const rect = element.getBoundingClientRect();
        return {
          width: rect.width,
          past: rect.right - status.right,
          cut: element.scrollWidth - element.clientWidth,
        };
      };
      return {
        height: status.height,
        parts: [
          ".config-editor__diagnostic-status",
          ".config-editor__change-status",
          ".config-editor__line-count",
        ].map(part),
      };
    });
    // Four short rows at most, not a column of one word per line.
    expect(footer.height).toBeLessThanOrEqual(110);
    const [problems, changes, lines] = footer.parts;
    expect(problems.width).toBeGreaterThanOrEqual(150);
    for (const part of [problems, changes, lines].filter(Boolean)) {
      expect(part.past).toBeLessThanOrEqual(1);
      expect(part.cut).toBeLessThanOrEqual(1);
    }
  });

  test("the find widget buttons wrap their text and stay inside it", async ({
    page,
  }) => {
    await mount(page, strings, "plain: x\nz: 2\n");
    await page.locator(".config-editor__input").click();
    await page.keyboard.press("Control+h");
    const widget = page.locator(".config-editor__find");
    await expect(widget).toBeVisible();
    await page.keyboard.type("z");
    const outside = await page.evaluate(() => {
      const widget = document
        .querySelector(".config-editor__find")
        .getBoundingClientRect();
      return [...document.querySelectorAll(".config-editor__find button")]
        .filter((button) => button.getBoundingClientRect().width > 0)
        .map((button) => {
          const rect = button.getBoundingClientRect();
          return {
            text: button.textContent,
            over: Math.max(rect.right - widget.right, widget.left - rect.left),
            cut: button.scrollWidth - button.clientWidth,
          };
        })
        .filter((button) => button.over > 1 || button.cut > 1);
    });
    expect(outside).toEqual([]);
    await expect(
      widget.locator(".config-editor__find-text-button", {
        hasText: strings.replaceAll,
      }),
    ).toBeVisible();
  });
  test("the find and go-to texts wrap inside their widgets", async ({
    page,
  }) => {
    const long = {
      matchPosition:
        "Treffer {current} von insgesamt {total} Treffern in der ganzen Datei",
      noResults: "Es wurden keine Ergebnisse für diese Suche gefunden",
      invalidPattern: "Das Suchmuster ist kein gültiger regulärer Ausdruck",
      replacedOne: "{count} Vorkommen wurde ersetzt und danach gespeichert",
      replacedOther:
        "{count} Vorkommen wurden ersetzt und danach wurde gespeichert",
      replaceSkipped: "; einige Treffer im geschützten Text blieben unberührt",
      goToLineHint: "Eine Zeilennummer von 1 bis {lines} eingeben und Enter",
      goToLineInvalid:
        "Keine gültige Position: Zeile oder Zeile:Spalte erwartet",
    };
    const clipped = () =>
      page.evaluate(() => {
        const out = [];
        for (const root of document.querySelectorAll(
          ".config-editor__find, .config-editor__goto",
        )) {
          if (root.getBoundingClientRect().width === 0) continue;
          const box = root.getBoundingClientRect();
          for (const element of root.querySelectorAll(
            ".config-editor__find-count, .config-editor__goto-hint",
          )) {
            const rect = element.getBoundingClientRect();
            if (rect.width === 0) continue;
            const over = Math.max(rect.right - box.right, box.left - rect.left);
            const cut = element.scrollWidth - element.clientWidth;
            if (over > 1 || cut > 1)
              out.push({ text: element.textContent, over, cut });
          }
        }
        return out;
      });
    await mount(page, long, "plain: x\nz: 2\n");
    await page.locator(".config-editor__input").click();
    await page.keyboard.press("Control+h");
    await expect(page.locator(".config-editor__find")).toBeVisible();
    await page.keyboard.type("z");
    await expect(page.locator(".config-editor__find-count")).toContainText(
      "Treffer",
    );
    expect(await clipped()).toEqual([]);
    await page.keyboard.type("qq");
    await expect(page.locator(".config-editor__find-count")).toContainText(
      "keine Ergebnisse",
    );
    expect(await clipped()).toEqual([]);
    await page.locator(".config-editor__find-input").first().fill("z");
    await page.keyboard.press("Control+Alt+Enter");
    await expect(page.locator(".config-editor__find-count")).toContainText(
      "ersetzt",
    );
    expect(await clipped()).toEqual([]);
    await page.keyboard.press("Escape");
    await page.locator(".config-editor__input").click();
    await page.keyboard.press("Control+g");
    await expect(page.locator(".config-editor__goto")).toBeVisible();
    await expect(page.locator(".config-editor__goto-hint")).toContainText(
      "Zeilennummer",
    );
    expect(await clipped()).toEqual([]);
    await page.keyboard.type("abc");
    await expect(page.locator(".config-editor__goto-hint")).toContainText(
      "Keine gültige",
    );
    expect(await clipped()).toEqual([]);
  });
});
