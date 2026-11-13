import { expect, test } from "@playwright/test";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

// The line commands (Alt+↑/↓ move lines, Shift+Alt+↑/↓ copy them,
// Ctrl+Shift+K deletes them, Ctrl+] / Ctrl+[ indent / outdent them), as in
// VS Code: a closed fold is one unit (a folded line takes its hidden lines
// along and stays folded, a line moves past a fold whole), each is one undo
// step, a lock inside a folded region is judged as its lines are, the
// caret keeps its column, the text keeps its last newline, and lines
// holding secret placeholders are written exactly as meant. Through the
// npm package's page (run `npm run build` in js/ first).
const js = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "js");
const built = existsSync(
  join(js, "config-editor", "pkg", "config_editor_bg.wasm"),
);
const pageUrl = (options) =>
  `${pathToFileURL(join(js, "examples", "vanilla.html")).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

test.skip(!built, "run `npm run build` in js/ first");
test.use({ viewport: { width: 1000, height: 700 } });

const input = (page) => page.locator(".config-editor__input").first();
// The document (the textarea shows it with the folded lines cut out).
const saved = (page) => page.evaluate(() => window.editor.value);
// What `onChange` last told the host.
const told = (page) => page.locator("#saved").textContent();
const toggle = (page, line) =>
  page.locator(`.config-editor__fold-toggle[data-line="${line}"]`);
const isClosed = async (page, line) =>
  (await toggle(page, line).count()) > 0 &&
  (await toggle(page, line).getAttribute("class")).includes(
    "config-editor__fold-toggle-closed",
  );
const shownLines = (page) =>
  page
    .locator(".config-editor__gutter-content > div[data-line]")
    .evaluateAll((rows) => rows.map((element) => Number(element.dataset.line)));

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
  await input(page).focus();
  return errors;
}

/** Puts the caret at the end of `line` (one-based, a line on screen). */
async function caretAtEndOf(page, line) {
  await page.keyboard.press("Control+g");
  await page.keyboard.type(`${line}:1`);
  await page.keyboard.press("Enter");
  await page.keyboard.press("End");
}

/** Folds `line` (one-based) and returns the last line it hides. */
async function fold(page, line) {
  await page.keyboard.press("Control+k");
  await page.keyboard.press("Control+j");
  await caretAtEndOf(page, line);
  await page.keyboard.press("Control+Shift+BracketLeft");
  await expect.poll(() => isClosed(page, line)).toBe(true);
  const shown = await shownLines(page);
  return shown[shown.indexOf(line) + 1] - 1;
}

// One closed fold per language: its header line (one-based).
const FOLDED = {
  yaml: [
    "apiVersion: v1\nkind: Service\nmetadata:\n  name: web\n  labels:\n    app: web\nspec:\n  type: ClusterIP\n",
    3,
  ],
  json: [
    '{\n  "name": "x",\n  "scripts": {\n    "build": "tsc",\n    "test": "vitest"\n  },\n  "private": true\n}\n',
    3,
  ],
  toml: [
    '[package]\nname = "x"\n\n[dependencies]\nserde = "1"\ntokio = "1"\n\n[dev-dependencies]\ninsta = "1"\n',
    4,
  ],
  hcl: [
    'variable "a" {\n  type = string\n}\nresource "x" "y" {\n  name = "n"\n  tags = {\n    a = 1\n  }\n}\noutput "o" {\n  value = 1\n}\n',
    4,
  ],
  xml: [
    "<project>\n  <name>x</name>\n  <dependencies>\n    <dependency>\n      <id>a</id>\n    </dependency>\n  </dependencies>\n  <build/>\n</project>\n",
    3,
  ],
};
// Whether a folded construct closes on a line of its own shown below the
// fold (`}`, an end tag): the folded line goes with it, as a cut takes it.
const CLOSES = { yaml: 0, json: 1, toml: 0, hcl: 1, xml: 1 };
// The lines of the constructs right below and above the fold (`spec:` and
// its child, HCL's `output` and `variable` blocks), which the folded line
// moves past whole, not into.
const NEXT = { yaml: 2, json: 1, toml: 1, hcl: 3, xml: 1 };
const PREVIOUS = { yaml: 1, json: 1, toml: 1, hcl: 3, xml: 1 };

/**
 * What a command makes of `text` with lines `first..last` (one-based) one
 * unit: the text, and where the fold's header is then (`null`: gone).
 * `next`, `previous`: the lines of the constructs below and above it,
 * which it moves past whole.
 */
function expected(text, [first, last], at, key, [next, previous] = [1, 1]) {
  const lines = text.slice(0, -1).split("\n");
  const block = lines.slice(first - 1, last);
  const size = block.length;
  const join = (parts) => `${parts.flat().join("\n")}\n`;
  const before = lines.slice(0, first - 1);
  const after = lines.slice(last);
  if (at === "header") {
    switch (key) {
      case "Alt+ArrowDown":
        return [
          join([before, after.slice(0, next), block, after.slice(next)]),
          first + next,
        ];
      case "Alt+ArrowUp":
        return [
          join([
            before.slice(0, -previous),
            block,
            before.slice(-previous),
            after,
          ]),
          first - previous,
        ];
      case "Shift+Alt+ArrowDown":
        return [join([before, block, block, after]), first + size];
      case "Shift+Alt+ArrowUp":
        return [join([before, block, block, after]), first];
      case "Control+Shift+K":
        return [join([before, after]), null];
      case "Control+BracketRight":
        return [
          join([
            before,
            block.map((line) => (line.trim() ? `  ${line}` : line)),
            after,
          ]),
          first,
        ];
    }
  }
  // The line above moved down past the fold, the one below up past it.
  if (at === "above")
    return [
      join([before.slice(0, -1), block, before.at(-1), after]),
      first - 1,
    ];
  return [join([before, after[0], block, after.slice(1)]), first + 1];
}

const HEADER_KEYS = [
  "Alt+ArrowDown",
  "Alt+ArrowUp",
  "Shift+Alt+ArrowDown",
  "Shift+Alt+ArrowUp",
  "Control+Shift+K",
  "Control+BracketRight",
];

test.describe("a closed fold is one unit", () => {
  for (const [language, [value, header]] of Object.entries(FOLDED)) {
    test(`${language}: the line commands take a folded line with its hidden lines, keep it folded, and undo in one step`, async ({
      page,
    }) => {
      const errors = await open(page, { value, language });
      const cases = [
        ...HEADER_KEYS.map((key) => ["header", key]),
        ["above", "Alt+ArrowDown"],
        ["below", "Alt+ArrowUp"],
      ];
      for (const [at, key] of cases) {
        const last = (await fold(page, header)) + CLOSES[language];
        await caretAtEndOf(page, at === "above" ? header - 1 : header);
        if (at === "below") {
          // The line after the fold (and its closing line).
          for (let row = 0; row <= CLOSES[language]; row += 1)
            await page.keyboard.press("ArrowDown");
          await page.keyboard.press("End");
        }
        await page.keyboard.press(key);
        const [want, folded] = expected(value, [header, last], at, key, [
          NEXT[language],
          PREVIOUS[language],
        ]);
        // The document (the textarea shows it with the folded lines out).
        await expect.poll(() => saved(page), `${at} ${key}`).toBe(want);
        if (folded !== null)
          expect(
            await isClosed(page, folded),
            `${at} ${key}: still folded`,
          ).toBe(true);
        await page.keyboard.press("Control+z");
        await expect
          .poll(() => saved(page), `${at} ${key} undone in one step`)
          .toBe(value);
        // Undone, the fold is closed where it was (it used to open).
        expect(
          await isClosed(page, header),
          `${at} ${key}: folded again when undone`,
        ).toBe(true);
      }
      expect(errors).toEqual([]);
    });
  }

  // A field locked inside the folded region: the region is judged as its
  // lines are (deleting it would delete the lock; moving it keeps it).
  const LOCKED = {
    yaml: "/metadata/name",
    json: "/scripts/build",
    toml: "/dependencies/serde",
    xml: "/project/dependencies/dependency/id",
  };
  for (const [language, path] of Object.entries(LOCKED)) {
    test(`${language}: a folded region holding a lock is refused deleting and keeps the lock moved`, async ({
      page,
    }) => {
      const [value, header] = FOLDED[language];
      const errors = await open(page, {
        value,
        language,
        readOnlyPaths: [path],
      });
      const last = (await fold(page, header)) + CLOSES[language];
      await caretAtEndOf(page, header);
      await page.keyboard.press("Control+Shift+K");
      await expect(
        page.locator(".config-editor__protect-tip--refused"),
      ).toBeVisible();
      expect(await saved(page)).toBe(value);
      expect(await isClosed(page, header), "still folded").toBe(true);
      await page.keyboard.press("Alt+ArrowUp");
      await expect
        .poll(() => saved(page))
        .toBe(expected(value, [header, last], "header", "Alt+ArrowUp")[0]);
      expect(errors).toEqual([]);
    });
  }
});

test.describe("the caret and the last line", () => {
  test("Ctrl+Shift+K keeps the caret column, clamped to the line taking the place", async ({
    page,
  }) => {
    await open(page, { value: "a: 1\nbbbb: 22\nc\nddd: 4\n" });
    await caretAtEndOf(page, 2);
    await page.keyboard.press("Home");
    for (let i = 0; i < 5; i += 1) await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Control+Shift+K");
    await expect(input(page)).toHaveValue("a: 1\nc\nddd: 4\n");
    // Column 5 on `c`: its end.
    expect(
      await input(page).evaluate((element) => element.selectionStart),
    ).toBe(6);
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Control+Shift+K");
    await expect(input(page)).toHaveValue("a: 1\nc\n");
    // The last line went: the caret is on the empty line after the text.
    expect(
      await input(page).evaluate((element) => element.selectionStart),
    ).toBe(7);
  });

  test("Ctrl+] / Ctrl+[ move a caret in the leading whitespace with the indent", async ({
    page,
  }) => {
    await open(page, { value: "a:\n  b: 1\n" });
    for (const [column, indented, outdented] of [
      [0, 5, 3],
      [1, 6, 4],
      [2, 7, 5],
    ]) {
      await caretAtEndOf(page, 2);
      await page.keyboard.press("Home");
      await page.keyboard.press("Home");
      await input(page).evaluate(
        (element, at) => element.setSelectionRange(at, at),
        3 + column,
      );
      await page.keyboard.press("Control+BracketRight");
      await expect(input(page)).toHaveValue("a:\n    b: 1\n");
      expect(
        await input(page).evaluate((element) => element.selectionStart),
        `column ${column}`,
      ).toBe(indented);
      await page.keyboard.press("Control+BracketLeft");
      await expect(input(page)).toHaveValue("a:\n  b: 1\n");
      expect(
        await input(page).evaluate((element) => element.selectionStart),
        `column ${column} back`,
      ).toBe(outdented);
    }
  });

  test("on the empty line after the last newline, a copy adds one line and a move none", async ({
    page,
  }) => {
    const value = "a: 1\nb: 2\n";
    await open(page, { value });
    for (const [key, want] of [
      ["Shift+Alt+ArrowDown", "a: 1\nb: 2\n\n"],
      ["Shift+Alt+ArrowUp", "a: 1\nb: 2\n\n"],
      ["Alt+ArrowUp", value],
      ["Alt+ArrowDown", value],
      ["Control+Shift+K", value],
    ]) {
      await page.keyboard.press("Control+End");
      await page.keyboard.press(key);
      await page.waitForTimeout(100);
      await expect(input(page), key).toHaveValue(want);
      if (want !== value) {
        await page.keyboard.press("Control+z");
        await expect(input(page), `${key} undone`).toHaveValue(value);
      }
    }
  });
});

// H29-A1: two lines swapped share their last characters (`»`, a token's
// last digits); read as an edit cutting a placeholder, the swap took them
// (`key: «secret 3f2a`), and the host got it.
test.describe("placeholders", () => {
  const lines = {
    yaml: (key, value) => `  ${key}: ${value}`,
    toml: (key, value) => `${key} = "${value}"`,
    hcl: (key, value) => `  ${key} = "${value}"`,
    json: (key, value) => `    "${key}": "${value}",`,
    xml: (key, value) => `  <${key}>${value}</${key}>`,
  };
  const wrap = {
    yaml: (body) => `db:\n${body.join("\n")}\n`,
    toml: (body) => `[db]\n${body.join("\n")}\n`,
    hcl: (body) => `db {\n${body.join("\n")}\n}\n`,
    json: (body) => `{\n  "db": {\n${body.join("\n")}\n    "z": "1"\n  }\n}\n`,
    xml: (body) => `<db>\n${body.join("\n")}\n</db>\n`,
  };
  const pairs = [
    [
      ["key", "«secret 3f2a»"],
      ["pin", "«secret 0b1c»"],
    ],
    [
      ["user", "app»"],
      ["key", "«secret 3f2a»"],
    ],
  ];
  for (const language of Object.keys(lines)) {
    test(`${language}: lines holding placeholders are moved and copied exactly`, async ({
      page,
    }) => {
      for (const [first, second] of pairs) {
        const body = [lines[language](...first), lines[language](...second)];
        const value = wrap[language](body);
        const errors = await open(page, { value, language });
        const top = language === "json" ? 3 : 2;
        await caretAtEndOf(page, top);
        await page.keyboard.press("Alt+ArrowDown");
        const swapped = wrap[language]([body[1], body[0]]);
        await expect(input(page)).toHaveValue(swapped);
        await expect.poll(() => saved(page)).toBe(swapped);
        await expect.poll(() => told(page)).toBe(swapped);
        await page.keyboard.press("Alt+ArrowUp");
        await expect.poll(() => saved(page)).toBe(value);
        // The caret inside the token (as a click on it can leave it).
        const inside = value.indexOf("3f2a");
        await input(page).evaluate(
          (element, at) => element.setSelectionRange(at, at),
          inside,
        );
        await page.keyboard.press("Shift+Alt+ArrowDown");
        const row = value.slice(0, inside).split("\n").length - 1;
        const copied = value.split("\n");
        copied.splice(row, 0, copied[row]);
        await expect.poll(() => saved(page)).toBe(copied.join("\n"));
        expect(errors).toEqual([]);
      }
    });
  }
});
