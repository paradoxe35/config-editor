import { expect, test } from "@playwright/test";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { pointerOn } from "./helpers.mjs";

// What a host that masks secrets as placeholders relies on, through the npm
// package's vanilla page, for two unrelated placeholder patterns (the editor
// assumes no format): a host problem with no end on a placeholder marks the
// whole token (hover, F8 and the caret's description say it), elsewhere one
// character; and nothing the editor outputs holds a token or a piece of one
// the host never sent. Build the packages first with `npm run build` in js/.
const js = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "js");
const built = existsSync(
  join(js, "config-editor", "pkg", "config_editor_bg.wasm"),
);
test.skip(!built, "js packages not built: run `npm run build` in js/");

const HOSTS = [
  {
    placeholder: "«secret [0-9a-f]{8}»",
    same: "«secret 3f2a91c0»",
    other: "«secret 0b1c2d3e»",
    note: "«secret 5e5e5e5e»",
    quote: "",
    // What a token or a piece of one looks like in this host's format.
    pieces: ["«", "»", "3f2a91c0", "0b1c2d3e", "5e5e5e5e"],
    match: /«[^»\n]*»/g,
  },
  {
    placeholder: "\\{\\{vault:[a-z0-9-]+\\}\\}",
    same: "{{vault:db-pass}}",
    other: "{{vault:api-key}}",
    note: "{{vault:old-key}}",
    quote: '"',
    pieces: ["{{", "}}", "vault:", "db-pass", "api-key", "old-key"],
    match: /\{\{[^}\n]*\}\}/g,
  },
];

const SCHEMA = JSON.stringify({
  type: "object",
  properties: {
    name: { type: "string", minLength: 3, description: "The name" },
    a: {
      type: "string",
      "x-secret": true,
      minLength: 40,
      description: "Secret a",
    },
    b: { type: "string", "x-secret": true, pattern: "^-----BEGIN" },
    c: { type: "string", writeOnly: true },
  },
});

// The host's document: the same token in `a` and `b`, another in `c`, one
// in a comment; line 6 is empty.
const documentOf = (host) =>
  `# note ${host.note}\nname: x\na: ${host.quote}${host.same}${host.quote}\nb: ${host.quote}${host.same}${host.quote}\nc: ${host.quote}${host.other}${host.quote}\n\nlast: 1\n`;

const page = (options) =>
  `${pathToFileURL(join(js, "examples", "vanilla.html")).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

const input = (browserPage) => browserPage.locator(".config-editor__input");
const markers = (browserPage) =>
  browserPage.locator(".config-editor__diagnostic-marker");
const setHost = (browserPage, list) =>
  browserPage.evaluate(
    (problems) => window.editor.update({ hostDiagnostics: problems }),
    list,
  );

async function open(browserPage, host, value = documentOf(host)) {
  const consoleMessages = [];
  browserPage.on("console", (message) => consoleMessages.push(message.text()));
  browserPage.on("pageerror", (error) => consoleMessages.push(String(error)));
  await browserPage.goto(
    page({
      value,
      schema: SCHEMA,
      secrets: { placeholder: host.placeholder },
      showDiff: true,
    }),
  );
  await expect(browserPage.locator("body")).toHaveAttribute(
    "data-ready",
    "true",
    { timeout: 20000 },
  );
  await input(browserPage).waitFor();
  // Every text the page's onChange shows.
  await browserPage.evaluate(() => {
    window.changes = [];
    const saved = document.getElementById("saved");
    new MutationObserver(() => window.changes.push(saved.textContent)).observe(
      saved,
      {
        childList: true,
        characterData: true,
        subtree: true,
      },
    );
  });
  return consoleMessages;
}

// One-based column of `token` on one-based `line` of `text`.
const columnOf = (text, line, token) =>
  text.split("\n")[line - 1].indexOf(token) + 1;

for (const host of HOSTS) {
  test.describe(`placeholders matching ${host.placeholder}`, () => {
    test("a host problem with no end marks the whole placeholder it is on, else one character", async ({
      page: browserPage,
    }) => {
      const errors = await open(browserPage, host);
      const text = documentOf(host);
      const column = columnOf(text, 5, host.other);
      await setHost(browserPage, [
        {
          range: { startLine: 5, startColumn: column + 3 },
          message: "the secret was rotated",
          code: "E_ROTATED",
        },
        {
          range: { startLine: 2, startColumn: 8 },
          severity: "warning",
          message: "at the line end",
        },
        {
          range: { startLine: 6, startColumn: 1 },
          severity: "info",
          message: "on the empty line",
        },
      ]);
      const hostMarkers = browserPage.locator(
        ".config-editor__diagnostic-marker-host",
      );
      await expect(hostMarkers).toHaveCount(3);
      const rotated = browserPage.locator(
        '.config-editor__diagnostic-marker-host[data-message="the secret was rotated"]',
      );
      await expect(rotated).toHaveText(host.other);
      // The point markers: one character wide, there.
      for (const message of ["at the line end", "on the empty line"]) {
        const marker = browserPage.locator(`[data-message="${message}"]`);
        await expect(marker).toHaveText("");
        const box = await marker.boundingBox();
        expect(box.width, message).toBeGreaterThan(2);
        const cell = await input(browserPage).evaluate((element) =>
          parseFloat(getComputedStyle(element).fontSize),
        );
        expect(box.width, message).toBeLessThan(cell * 1.5);
      }
      // Hover says it over the whole token, at its far end too.
      await pointerOn(browserPage, rotated, { at: 0.9 });
      await expect(
        browserPage.locator(".config-editor__problem-tip"),
      ).toContainText("the secret was rotated");
      // The caret at the token's end hears it.
      await input(browserPage).click();
      await browserPage.keyboard.press("Control+Home");
      for (let step = 0; step < 4; step++)
        await browserPage.keyboard.press("ArrowDown");
      await browserPage.keyboard.press("End");
      // Inside the closing quote, where it is quoted.
      if (host.quote) await browserPage.keyboard.press("ArrowLeft");
      const note = browserPage.locator(
        `#${await input(browserPage).getAttribute("aria-describedby")}`,
      );
      await expect(note).toContainText("the secret was rotated");
      await expect(note).toContainText("E_ROTATED");
      // F8 reaches it and announces it, the caret on the token's start.
      await browserPage.keyboard.press("Control+Home");
      const announcement = browserPage.locator(".config-editor__announcement");
      let heard = false;
      for (let step = 0; step < 8 && !heard; step++) {
        await browserPage.keyboard.press("F8");
        heard = /the secret was rotated/.test(await announcement.textContent());
      }
      expect(heard).toBe(true);
      const caret = await input(browserPage).evaluate(
        (element) => element.selectionStart,
      );
      expect(caret).toBe(text.indexOf(host.other));
      expect(errors.filter((message) => /error|panic/i.test(message))).toEqual(
        [],
      );
    });

    test("nothing the editor outputs holds a token or a piece of one the host never sent", async ({
      page: browserPage,
      context,
    }) => {
      await context.grantPermissions(["clipboard-read", "clipboard-write"]);
      const consoleMessages = await open(browserPage, host);
      const text = documentOf(host);
      const outputs = [];
      // Every text, label and description the editor shows or speaks.
      const collect = async (step) => {
        const read = await browserPage.evaluate(() => {
          const root = document.querySelector(".config-editor");
          const found = [root.innerText];
          for (const element of document.querySelectorAll(
            '.config-editor, .config-editor *, [class*="config-editor__"]',
          )) {
            for (const name of [
              "aria-label",
              "aria-description",
              "title",
              "data-message",
              "data-detail",
              "placeholder",
            ]) {
              const value = element.getAttribute(name);
              if (value) found.push(value);
            }
            if (
              /announcement|tip|hover|peek|note|status/.test(element.className)
            )
              found.push(element.textContent);
          }
          const described = document
            .querySelector(".config-editor__input")
            ?.getAttribute("aria-describedby");
          if (described)
            found.push(document.getElementById(described)?.textContent ?? "");
          return found;
        });
        outputs.push(...read.map((value) => ({ step, value })));
      };
      await setHost(browserPage, [
        { path: "/c", severity: "warning", message: "check c" },
        {
          range: { startLine: 3, startColumn: columnOf(text, 3, host.same) },
          message: "a is old",
        },
      ]);
      await expect(markers(browserPage).first()).toBeVisible();
      await collect("loaded");
      // Hover a placeholder, a problem and a key.
      const chip = browserPage
        .locator(".config-editor__code-line .config-editor__secret")
        .first();
      await pointerOn(browserPage, chip, { at: 0.5 });
      await expect(
        browserPage.locator(".config-editor__protect-tip"),
      ).toBeVisible();
      await collect("hover placeholder");
      await pointerOn(browserPage, markers(browserPage).last(), { at: 0.5 });
      await collect("hover problem");
      // F8 through every problem.
      await input(browserPage).click();
      await browserPage.keyboard.press("Control+Home");
      for (let step = 0; step < 8; step++) {
        await browserPage.keyboard.press("F8");
        await collect(`F8 ${step}`);
      }
      await browserPage.keyboard.press("Escape");
      // An edit (onChange, the diff gutter and its preview).
      await browserPage.keyboard.press("Control+Home");
      await browserPage.keyboard.press("ArrowDown");
      await browserPage.keyboard.press("End");
      await browserPage.keyboard.type("yz");
      // The token of `a` written into `c` (flagged there), and Ctrl+A copied.
      const shown = await input(browserPage).inputValue();
      const c = shown.indexOf(host.other);
      await input(browserPage).evaluate(
        (element, [from, to]) => element.setSelectionRange(from, to),
        [c, c + host.other.length],
      );
      await browserPage.keyboard.insertText(host.same);
      await expect(
        browserPage
          .locator(".config-editor__diagnostic-marker-warning")
          .first(),
      ).toBeVisible();
      await browserPage.waitForTimeout(400);
      await collect("moved");
      const modified = browserPage
        .locator(".config-editor__diff-marker-modified")
        .first();
      await modified.click();
      await expect(
        browserPage.locator(".config-editor__diff-peek"),
      ).toBeVisible();
      await collect("diff preview");
      await browserPage.keyboard.press("Escape");
      await input(browserPage).click();
      await browserPage.keyboard.press("Control+A");
      await browserPage.keyboard.press("Control+C");
      outputs.push({
        step: "copy",
        value: await browserPage.evaluate(() => navigator.clipboard.readText()),
      });
      outputs.push({
        step: "unifiedDiff",
        value: await browserPage.evaluate(() => window.editor.unifiedDiff()),
      });
      // Edit as JSON and back.
      await browserPage.locator("#edit-as").selectOption("json");
      await expect(input(browserPage)).toHaveValue(/"name"/);
      await browserPage.waitForTimeout(400);
      await collect("edit as JSON");
      await browserPage.locator("#edit-as").selectOption("");
      await browserPage.waitForTimeout(400);
      await collect("back");
      for (const value of await browserPage.evaluate(() => window.changes))
        outputs.push({ step: "onChange", value });
      outputs.push(
        ...consoleMessages.map((value) => ({ step: "console", value })),
      );

      // What the host sent: its three tokens, as written. Each token found
      // in an output is one of them; with them taken out, no piece of one
      // is left (no hex recombined, no token cut or made up).
      const sent = [host.same, host.other, host.note];
      const leaks = [];
      for (const { step, value } of outputs) {
        for (const token of value.match(host.match) ?? []) {
          if (!sent.includes(token))
            leaks.push(
              `${step}: ${token} in ${JSON.stringify(value).slice(0, 200)}`,
            );
        }
        const rest = sent.reduce(
          (left, token) => left.split(token).join(" "),
          value,
        );
        for (const piece of host.pieces) {
          if (rest.includes(piece))
            leaks.push(
              `${step}: "${piece}" in ${JSON.stringify(value).slice(0, 200)}`,
            );
        }
      }
      expect(leaks).toEqual([]);
      // The outputs were read: the placeholders, a problem, the copy.
      expect(
        outputs.some(
          ({ step, value }) => step === "copy" && value.includes(host.same),
        ),
      ).toBe(true);
      expect(
        outputs.some(({ value }) =>
          value.includes("stands for the secret of /a"),
        ),
      ).toBe(true);
      expect(outputs.some(({ step }) => step === "onChange")).toBe(true);
    });

    test("onChange hands back the text on screen byte for byte; the host text sent again is no change", async ({
      page: browserPage,
    }) => {
      await open(browserPage, host);
      const text = documentOf(host).replace(/\n/g, "\r\n");
      await browserPage.evaluate(
        (value) => window.editor.update({ value }),
        text,
      );
      await browserPage.evaluate(
        (value) => window.editor.update({ value }),
        text,
      );
      await browserPage.waitForTimeout(300);
      expect(await browserPage.evaluate(() => window.changes)).toEqual([]);
      await input(browserPage).click();
      await browserPage.keyboard.press("Control+Home");
      await browserPage.keyboard.press("ArrowDown");
      await browserPage.keyboard.press("End");
      await browserPage.keyboard.type("y");
      await expect
        .poll(() => browserPage.evaluate(() => window.changes.at(-1)))
        .toBe(text.replace("name: x", "name: xy"));
    });

    // A placeholder the host gave as a field's whole value stands for the
    // whole secret: keys typed beside it are refused, in the host's language
    // and in an Edit-as view, and the host never hears it glued to text.
    for (const view of [null, "json", "toml"]) {
      test(`typing beside a whole placeholder is refused${view ? ` editing as ${view}` : ""}`, async ({
        page: browserPage,
      }) => {
        await open(browserPage, host);
        if (view) {
          await browserPage.evaluate(
            (editAs) => window.editor.update({ editAs }),
            view,
          );
          await expect(input(browserPage)).toHaveValue(
            new RegExp(view === "json" ? "^\\{" : "name = "),
          );
        }
        const shown = await input(browserPage).inputValue();
        for (const [where, keys] of [
          ["after", "Z"],
          ["before", "Z"],
          ["after", " x"],
        ]) {
          await input(browserPage).evaluate(
            (element, [token, after]) => {
              const at =
                element.value.indexOf(token) + (after ? token.length : 0);
              element.focus();
              element.setSelectionRange(at, at);
              element.dispatchEvent(new Event("select", { bubbles: true }));
            },
            [host.same, where === "after"],
          );
          await browserPage.keyboard.type(keys);
          await expect(
            browserPage.locator(".config-editor__protect-tip"),
          ).toContainText(
            "/a: a secret placeholder stands for the whole secret",
          );
          const value = await input(browserPage).inputValue();
          expect(
            value.includes(`${host.same}Z`) ||
              value.includes(`Z${host.same}`) ||
              value.includes(`${host.same} x`),
          ).toBe(false);
        }
        await browserPage.waitForTimeout(600);
        // Only a space a YAML plain value ends before may stay.
        const trimmed = (text) => text.replace(/ +\n/g, "\n");
        expect(trimmed(await input(browserPage).inputValue())).toBe(
          trimmed(shown),
        );
        const changes = await browserPage.evaluate(() => window.changes);
        expect(
          changes.filter((change) =>
            /Z«|»Z|» x|Z\{\{|\}\}Z|\}\} x/.test(change),
          ),
        ).toEqual([]);
      });
    }

    // A token alone in a block scalar is `a`'s whole value too, whatever the
    // chomping (`|` keeps a line break after it): untouched, the schema
    // check takes it as the current secret; keys typed at either edge are
    // refused, and the host never hears it glued to text.
    for (const header of ["|", ">", "|-", "|+"]) {
      test(`a placeholder alone in a ${header} block is its field's whole value`, async ({
        page: browserPage,
      }) => {
        const quoted = `a: ${host.quote}${host.same}${host.quote}\n`;
        const value = documentOf(host).replace(
          quoted,
          `a: ${header}\n  ${host.same}\n`,
        );
        await open(browserPage, host, value);
        // A chip in `a` and in `b`.
        await expect(
          browserPage
            .locator(".config-editor__code-line .config-editor__secret")
            .filter({ hasText: host.same }),
        ).toHaveCount(2);
        // The other fields' problems show; none is `a`'s.
        await expect(
          browserPage.locator('[data-message^="/name"]'),
        ).toHaveCount(1);
        await expect(browserPage.locator('[data-message^="/a"]')).toHaveCount(
          0,
        );
        for (const after of [true, false]) {
          await input(browserPage).evaluate(
            (element, [token, after]) => {
              const at =
                element.value.indexOf(token) + (after ? token.length : 0);
              element.focus();
              element.setSelectionRange(at, at);
              element.dispatchEvent(new Event("select", { bubbles: true }));
            },
            [host.same, after],
          );
          await browserPage.keyboard.type("Z");
          await expect(
            browserPage.locator(".config-editor__protect-tip"),
          ).toContainText(
            "/a: a secret placeholder stands for the whole secret",
          );
          await expect(input(browserPage)).toHaveValue(value);
        }
        await browserPage.waitForTimeout(600);
        await expect(browserPage.locator('[data-message^="/a"]')).toHaveCount(
          0,
        );
        const changes = await browserPage.evaluate(() => window.changes);
        expect(
          changes.filter((change) => /Z«|»Z|Z\{\{|\}\}Z/.test(change)),
        ).toEqual([]);
      });
    }

    // A host `value` that does not parse: its own characters say the token
    // is `a`'s whole value, so a key typed beside it while broken is
    // refused as once it parses; repaired by editing, the text holds it
    // alone, unflagged, and saves.
    test("text typed beside a whole placeholder of a broken host is refused", async ({
      page: browserPage,
    }) => {
      const broken = documentOf(host).replace("last: 1", "last: [");
      await open(browserPage, host, broken);
      const caretAt = (needle, offset) =>
        input(browserPage).evaluate(
          (element, [needle, offset]) => {
            const at = element.value.indexOf(needle) + offset;
            element.focus();
            element.setSelectionRange(at, at + (offset < 0 ? -offset : 0));
            element.dispatchEvent(new Event("select", { bubbles: true }));
          },
          [needle, offset],
        );
      const glued = "/a: a secret placeholder stands for the whole secret";
      await caretAt(
        `a: ${host.quote}${host.same}`,
        `a: ${host.quote}${host.same}`.length,
      );
      await browserPage.keyboard.type("Y");
      await expect(
        browserPage.locator(".config-editor__protect-tip"),
      ).toContainText(glued);
      await expect(input(browserPage)).toHaveValue(broken);
      // Repaired by typing over the `[`.
      await input(browserPage).evaluate((element) => {
        const at = element.value.indexOf("last: [") + 6;
        element.focus();
        element.setSelectionRange(at, at + 1);
        element.dispatchEvent(new Event("select", { bubbles: true }));
      });
      await browserPage.keyboard.type("1");
      await expect(input(browserPage)).toHaveValue(/last: 1\n$/);
      await browserPage.waitForTimeout(600);
      await expect(
        browserPage.locator(`[data-message^="${glued}"]`),
      ).toHaveCount(0);
      expect(await browserPage.evaluate(() => window.editor.save())).toBe(true);
      const changes = await browserPage.evaluate(() => window.changes);
      expect(changes.filter((change) => /»Y|\}\}Y/.test(change))).toEqual([]);
    });
  });
}
