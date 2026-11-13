import { expect, test } from "@playwright/test";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

// A call about one thing changes only it, and a call the host makes from
// `onChange` is newer than the call whose input flush ran that `onChange`.
// Before, `replaceValue` on one document gave every other document its
// listed text again, reverting their edits (round 11's `D11.host-1`), and
// an `update()` applied the options it read before the flush over what a
// reentrant `update`/`replaceValue` had just set (round 17's `D17.host-1`).

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const js = join(root, "js");
const built = existsSync(
  join(js, "config-editor", "pkg", "config_editor_bg.wasm"),
);
const example = (name) => pathToFileURL(join(js, "examples", name)).href;

async function open(page) {
  await page.goto(example("vanilla.html"));
  await expect(page.locator("body")).toHaveAttribute("data-ready", "true", {
    timeout: 20000,
  });
  await page.evaluate(async () => {
    window.module = await import("../config-editor/index.js");
    window.editor?.destroy();
    const host = document.createElement("div");
    host.id = "qa";
    document.body.prepend(host);
    window.saved = [];
  });
}

/** Types `text` at the end of the line holding `marker` in the shown editor. */
async function typeAfter(page, marker, text) {
  const input = page.locator("#qa .config-editor__input:visible");
  await input.evaluate((element, marker) => {
    const at = element.value.indexOf(marker) + marker.length;
    element.focus();
    element.setSelectionRange(at, at);
  }, marker);
  await page.keyboard.type(text);
}

/** Makes the next input event call `call` in a 0 ms timer, inside the typing pause. */
async function onNextInput(page, call) {
  await page.evaluate((call) => {
    const input = [
      ...document.querySelectorAll("#qa .config-editor__input"),
    ].find((element) => element.offsetParent);
    input.addEventListener(
      "input",
      () => setTimeout(() => new Function(call)(), 0),
      { once: true },
    );
  }, call);
}

const pause = (page) => page.waitForTimeout(500);

test.describe("replaceValue on one document", () => {
  test.skip(!built, "js packages not built: run `npm run build` in js/");

  for (const editAs of [null, "json"]) {
    test(`keeps another document's edit${editAs ? " (edited as JSON)" : ""}`, async ({
      page,
    }) => {
      await open(page);
      await page.evaluate((editAs) => {
        window.view = new window.module.ConfigDocuments(
          document.getElementById("qa"),
          {
            documents: [
              {
                id: "main",
                fileName: "main.yaml",
                value: "a: 1\n",
                ...(editAs ? { editAs } : {}),
              },
              {
                id: "limits",
                fileName: "limits.toml",
                value: "[limits]\nmax_pending = 1\n",
              },
            ],
            onSave: (id, text) => window.saved.push([id, text]),
          },
        );
      }, editAs);
      await typeAfter(page, editAs ? '"a": 1' : "a: 1", "1");
      expect(
        await page.evaluate(() =>
          window.view.replaceValue("limits", "[limits]\nmax_pending = 800\n"),
        ),
      ).toBe(true);
      await pause(page);
      const state = await page.evaluate(() => ({
        value: window.view.editor("main").value,
        dirty: window.view.isDirty("main"),
        saved: window.view.saveAll(),
        sent: window.saved,
      }));
      expect(state.value).toBe("a: 11\n");
      expect(state.dirty).toBe(true);
      expect(state.saved).toContain("main");
      expect(state.sent).toContainEqual(["main", "a: 11\n"]);
    });
  }

  test("keeps another document's edit through the view's settings", async ({
    page,
  }) => {
    await open(page);
    await page.evaluate(() => {
      window.view = new window.module.ConfigDocuments(
        document.getElementById("qa"),
        {
          documents: [
            { id: "main", fileName: "main.yaml", value: "a: 1\n" },
            {
              id: "limits",
              fileName: "limits.yaml",
              value: "b: 1\n",
              settings: { showFooter: true },
            },
          ],
        },
      );
    });
    await typeAfter(page, "a: 1", "1");
    await page.evaluate(() => window.view.update({ theme: "vscode-light" }));
    await pause(page);
    expect(await page.evaluate(() => window.view.editor("main").value)).toBe(
      "a: 11\n",
    );
    expect(await page.evaluate(() => window.view.isDirty("main"))).toBe(true);
  });
});

// An Edit-as view holds its edit until the typing pause is over: a host
// call inside the pause applies it first, and the host hears it then.
test.describe("a call from onChange is newer than the call that flushed it", () => {
  test.skip(!built, "js packages not built: run `npm run build` in js/");

  test("documents: a re-list from onChange stays over the older list", async ({
    page,
  }) => {
    await open(page);
    await page.evaluate(() => {
      window.files = { only: "x: 1\n" };
      const list = () =>
        Object.entries(window.files).map(([id, value]) => ({
          id,
          fileName: `${id}.yaml`,
          value,
          editAs: "json",
        }));
      window.stale = list();
      window.view = new window.module.ConfigDocuments(
        document.getElementById("qa"),
        {
          documents: list(),
          onChange: (id, text) => {
            window.files[id] = text;
            window.view.update({ documents: list() });
          },
          onSave: (id, text) => window.saved.push([id, text]),
        },
      );
    });
    await onNextInput(page, "window.view.update({ documents: window.stale })");
    await typeAfter(page, '"x": 1', "3");
    await pause(page);
    expect(await page.evaluate(() => window.view.editor("only").value)).toBe(
      "x: 13\n",
    );
    expect(await page.evaluate(() => window.view.saveAll())).toEqual(["only"]);
  });

  const single = {
    "a value pushed back from onChange": "window.qa.update({ value: text })",
    "a replaceValue from onChange": "window.qa.replaceValue(text)",
  };
  for (const [name, reentrant] of Object.entries(single)) {
    test(`editor: ${name} stays over the older value`, async ({ page }) => {
      await open(page);
      await page.evaluate((reentrant) => {
        window.qa = new window.module.ConfigEditor(
          document.getElementById("qa"),
          {
            language: "yaml",
            editAs: "json",
            value: "x: 1\n",
            onChange: new Function("text", reentrant),
            onSave: (text) => window.saved.push(text),
          },
        );
      }, reentrant);
      await onNextInput(page, "window.qa.update({ value: 'x: 1\\n' })");
      await typeAfter(page, '"x": 1', "3");
      await pause(page);
      expect(await page.evaluate(() => window.qa.value)).toBe("x: 13\n");
      expect(await page.evaluate(() => window.qa.save())).toBe(true);
      expect(await page.evaluate(() => window.saved)).toEqual(["x: 13\n"]);
    });
  }

  test("editor: a lock set from onChange stays over the older locks, and the older call still applies its other keys", async ({
    page,
  }) => {
    await open(page);
    await page.evaluate(() => {
      window.qa = new window.module.ConfigEditor(
        document.getElementById("qa"),
        {
          language: "yaml",
          editAs: "json",
          value: "x: 1\n",
          onChange: (text) =>
            window.qa.update({ value: text, readOnlyPaths: ["/x"] }),
        },
      );
    });
    await onNextInput(
      page,
      "window.qa.update({ value: 'x: 1\\n', readOnlyPaths: [], baseline: 'x: 0\\n' })",
    );
    await typeAfter(page, '"x": 1', "3");
    await pause(page);
    await typeAfter(page, '"x": 13', "9");
    await pause(page);
    expect(await page.evaluate(() => window.qa.value)).toBe("x: 13\n");
    // The older call's baseline applied: the document differs from it.
    expect(await page.evaluate(() => window.qa.unifiedDiff())).toContain(
      "-x: 0",
    );
  });
});
