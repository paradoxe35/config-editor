import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { DOCUMENTS_FRAMEWORK_PAGES } from './helpers.mjs';

// Locks given again while echoes are on their way keep their field.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

const js = join(root, 'js');

const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

function listen(page) {
  page.errors = [];
  page.on('console', (message) => {
    if (message.type() === 'error') page.errors.push(message.text());
  });
  page.on('pageerror', (error) => page.errors.push(String(error)));
}

async function open(page) {
  listen(page);
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async () => {
    window.editor?.destroy();
    window.documentsView?.destroy();
    window.module = await import('../config-editor/index.js');
    window.tools = await import('../config-editor/tools.js');
  });
}

const input = (page) => page.locator('[data-host] .config-editor__input:visible');

async function caretAt(page, marker, offset = marker.length) {
  await input(page).evaluate(
    (element, [marker, offset]) => {
      const at = element.value.indexOf(marker);
      if (at < 0) throw new Error(`no ${marker} in ${element.value}`);
      element.focus();
      element.setSelectionRange(at + offset, at + offset);
    },
    [marker, offset],
  );
}

// `free`: the field on the line above the locked one, which stays free.
const SAMPLES = {
  yaml: {
    fileName: 'm.yaml',
    text: 'name: a\nport: 80\nhost: h\n',
    path: '/host',
    locked: 'host: h',
    free: 'port: 8',
    above: ['name: a', ['\n', 'x: 1']],
    before: ['port: 80', ['0']],
    below: ['host: h', ['\n', 'z: 9']],
  },
  json: {
    fileName: 'm.json',
    text: '{\n  "name": "a",\n  "port": 80,\n  "host": "h"\n}\n',
    path: '/host',
    locked: '"host": "h"',
    free: '"port": 8',
    above: ['"name": "a",', ['\n', '"x": 1,']],
    before: ['"port": 80', ['0']],
    below: ['"host": "h"', [',', '\n', '"z": 9']],
  },
  toml: {
    fileName: 'm.toml',
    text: 'name = "a"\nport = 80\nhost = "h"\n',
    path: '/host',
    locked: 'host = "h"',
    free: 'port = 8',
    above: ['name = "a"', ['\n', 'x = 1']],
    before: ['port = 80', ['0']],
    below: ['host = "h"', ['\n', 'z = 9']],
  },
  hcl: {
    fileName: 'm.tf',
    text: 'name = "a"\nport = 80\nhost = "h"\n',
    path: '/host',
    locked: 'host = "h"',
    free: 'port = 8',
    above: ['name = "a"', ['\n', 'x = 1']],
    before: ['port = 80', ['0']],
    below: ['host = "h"', ['\n', 'z = 9']],
  },
  xml: {
    fileName: 'm.xml',
    text: '<c>\n  <name>a</name>\n  <port>80</port>\n  <host>h</host>\n</c>\n',
    path: '/c/host',
    locked: '<host>h</host>',
    free: '<port>8',
    above: ['<name>a</name>', ['\n', '<x>1</x>']],
    before: ['<port>80', ['0']],
    below: ['<host>h</host>', ['\n', 'z']],
  },
};

/**
 * Mounts `language`'s document, locked on its field by a range in
 * `style`, with `echoes: true` and a host (`kind`) handing every text
 * back in `order`: `late` (in order, 300 ms late), `reversed` (held, then
 * newest first on `flushEchoes()`) or `random` (0-200 ms late).
 */
async function mount(page, kind, language, style, order) {
  await page.evaluate(
    ([kind, language, style, order, sample]) => {
      window.mounted?.destroy();
      document.querySelectorAll('[data-host]').forEach((element) => element.remove());
      const element = document.createElement('div');
      element.dataset.host = '';
      element.style.height = '400px';
      document.body.prepend(element);
      const { text, path, fileName } = sample;
      const found = tools.rangeOf(language, text, path);
      const range =
        style === 'indices'
          ? { start: found.start, end: found.end }
          : { startLine: found.startLine, startColumn: found.startColumn, endLine: found.endLine, endColumn: found.endColumn };
      const held = [];
      window.flushEchoes = () => held.splice(0).reverse().forEach((deliver) => deliver());
      // An echo still on its way when the next case mounts is dropped.
      const mount = (window.mountCount = (window.mountCount ?? 0) + 1);
      const later = (handBack) => {
        const deliver = () => mount === window.mountCount && handBack();
        if (order === 'reversed') held.push(deliver);
        else setTimeout(deliver, order === 'late' ? 300 : Math.random() * 200);
      };
      window.saved = [];
      if (kind === 'documents') {
        let files = [{ id: 'm', fileName, value: text, readOnlyRanges: [range] }];
        window.mounted = new module.ConfigDocuments(element, {
          documents: files,
          echoes: true,
          onChange: (id, value) =>
            later(() => {
              files = files.map((file) => (file.id === id ? { ...file, value } : file));
              mounted.update({ documents: files });
            }),
          onSave: (id, value) => saved.push(value),
        });
        window.current = () => mounted.editor('m').value;
        window.saveNow = () => mounted.save('m');
      } else {
        // `store`: written another way (CRLF, LF back) and rendered twice.
        const store = kind === 'store';
        const crlf = (value) => value.replace(/\r?\n/g, '\r\n');
        const props = { language, value: store ? crlf(text) : text, readOnlyRanges: [range] };
        window.mounted = new module.ConfigEditor(element, {
          ...props,
          echoes: true,
          onSave: (value) => saved.push(value),
          onChange: (value) => {
            const stored = store ? value.replace(/\r\n/g, '\n') : value;
            later(() => {
              props.value = stored;
              mounted.update({ ...props });
              if (store) mounted.update({ ...props });
            });
          },
        });
        window.current = () => mounted.value.replace(/\r\n/g, '\n');
        window.saveNow = () => mounted.save();
      }
    },
    [kind, language, style, order, SAMPLES[language]],
  );
}

const wait = (page, ms) => page.evaluate((ms) => new Promise((resolve) => setTimeout(resolve, ms)), ms);

/** One case: the edit typed, the echoes handed back, then the lock, the free field and the save checked. */
async function check(page, kind, language, style, order, edit) {
  const sample = SAMPLES[language];
  const failures = [];
  const name = `${kind} ${language} ${style} ${order} ${edit}`;
  await mount(page, kind, language, style, order);
  const [marker, parts] = sample[edit];
  await caretAt(page, marker);
  for (const part of parts) {
    if (part === '\n') await page.keyboard.press('Enter');
    else await page.keyboard.type(part, { delay: 40 });
  }
  await wait(page, 60);
  if (order === 'reversed') await page.evaluate(() => flushEchoes());
  await wait(page, order === 'late' ? 700 : 400);
  const edited = await page.evaluate(() => current());
  const expected = parts.filter((part) => part !== '\n');
  if (parts[0] !== '\n') expected.push(marker + parts[0]);
  if (expected.some((part) => !edited.includes(part))) {
    failures.push(`${name}: typing lost: ${JSON.stringify(edited)}`);
  }
  if (!edited.includes(sample.locked)) failures.push(`${name}: the locked text changed: ${JSON.stringify(edited)}`);
  // The locked value refuses typing.
  await caretAt(page, sample.locked, sample.locked.lastIndexOf('h') + 1);
  await page.keyboard.type('ACKED');
  await wait(page, 60);
  const after = await page.evaluate(() => current());
  if (after !== edited) failures.push(`${name}: the locked value took typing: ${JSON.stringify(after)}`);
  // The free field above it takes typing.
  await caretAt(page, sample.free);
  await page.keyboard.type('7');
  await wait(page, 60);
  const freed = await page.evaluate(() => current());
  if (!freed.includes(`${sample.free}7`)) failures.push(`${name}: the free field refused typing: ${JSON.stringify(freed)}`);
  const saved = await page.evaluate(() => {
    saveNow();
    return saved.at(-1) ?? null;
  });
  const savedText = saved?.replace(/\r\n/g, '\n');
  if (!savedText?.includes(sample.locked) || savedText.includes('ACKED')) failures.push(`${name}: saved ${JSON.stringify(saved)}`);
  return failures;
}

// QA round 33 (FW33-1, HO33-2, HO33-3): `readOnlyRanges` a host gives again
// unchanged, with an echo of a text the editor sent (`echoes: true`), keep
// where the editor moved them, whatever order the echoes come back in (in
// order and late, newest first, random delays), for a ConfigDocuments host
// re-listing every document, a ConfigEditor host passing its props again,
// and one whose store writes the text another way and renders twice: the
// lock stays on its field (a free field next to it stays free), nothing
// typed is lost, and the save keeps the locked value. The framework
// documents pages too. Through the npm package (run `npm run build` in js/).
test.describe('ranges given again with echoes keep the lock', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  for (const kind of ['documents', 'editor', 'store']) {
    for (const order of ['late', 'reversed', 'random']) {
      test(`${kind}, echoes ${order}`, async ({ page }) => {
        test.setTimeout(300_000);
        await open(page);
        const failures = [];
        for (const language of (process.env.QA33_LANGUAGES ?? Object.keys(SAMPLES).join(',')).split(',')) {
          for (const style of ['lines', 'indices']) {
            for (const edit of ['above', 'before', 'below']) {
              try {
                failures.push(...(await check(page, kind, language, style, order, edit)));
              } catch (error) {
                failures.push(`${kind} ${language} ${style} ${order} ${edit}: ${String(error).split('\n')[0]}`);
              }
            }
          }
        }
        expect(failures).toEqual([]);
        expect(page.errors).toEqual([]);
      });
    }
  }
});

// The framework documents pages, their parent handing every text back
// 0-200 ms late (`?random`): the lock `updateFile` gives on the second
// line holds while two lines are typed at the top, nothing typed is lost,
// and the parent ends on the document.
test.describe('the framework documents pages keep a lock under random echoes', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  for (const name of DOCUMENTS_FRAMEWORK_PAGES) {
    test(`${name}?random`, async ({ page }) => {
      listen(page);
      await page.goto(`${example(name)}?random`);
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      const shown = page.locator('[role="tabpanel"]:not([hidden]) .config-editor__input');
      await shown.waitFor();
      await wait(page, 300);
      const locked = await page.evaluate(() => {
        window.updateFile('main', { readOnlyRanges: [{ startLine: 2, startColumn: 1, endLine: 3, endColumn: 1 }] });
        return window.currentFiles().find((file) => file.id === 'main').value.split('\n')[1];
      });
      await wait(page, 300);
      await shown.click();
      await page.keyboard.press('Control+Home');
      await page.keyboard.type('# a\n# b\n', { delay: 30 });
      await wait(page, 800);
      await shown.evaluate((element, locked) => {
        const at = element.value.indexOf(locked) + locked.length - 3;
        element.focus();
        element.setSelectionRange(at, at);
      }, locked);
      await page.keyboard.type('ZZ', { delay: 30 });
      await wait(page, 800);
      const value = await shown.inputValue();
      expect(value.startsWith('# a\n# b\n'), JSON.stringify(value)).toBe(true);
      expect(value.split('\n')[3]).toBe(locked);
      expect(value).not.toContain('ZZ');
      const host = await page.evaluate(() => window.currentFiles().find((file) => file.id === 'main').value);
      expect(host).toBe(value);
      expect(page.errors).toEqual([]);
    });
  }
});

// A group of its own (its own helpers).
test.describe(() => {
  // QA round 35 (HO35-1, FW35-1): with `echoes: true`, the user types on
  // past texts the editor sent, and the host lists those echoes with
  // `readOnlyRanges` worked out in each (by `rangeOf`, by lines or by index),
  // more than once: the same echo twice (a double render), two echoes giving
  // the same ranges, three times, or older echoes after the newest. The ranges are moved from the echo to the
  // document once: the lock stays on its field for a ConfigDocuments view
  // as for a ConfigEditor, refuses typing, and the save keeps the locked
  // value. Through the npm package (run `npm run build` in js/).
  const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

  const js = join(root, "js");

  const built = existsSync(
    join(js, "config-editor", "pkg", "config_editor_bg.wasm"),
  );

  const example = (name) => pathToFileURL(join(js, "examples", name)).href;

  // `free`: the field on the line above the locked one, which stays free;
  // `first`: a line typed above the lock (the echoes); `then`: where the
  // user types on past them (above, right before or below the lock).
  const SAMPLES = {
    yaml: {
      fileName: "m.yaml",
      text: "name: a\nport: 80\nhost: h\n",
      locked: "host: h",
      free: "port: 8",
      first: ["name: a", "x: 1"],
      then: {
        above: ["name: a", "y: 2"],
        before: ["port: 80", "y: 2"],
        below: ["host: h", "y: 2"],
      },
    },
    json: {
      fileName: "m.json",
      text: '{\n  "name": "a",\n  "port": 80,\n  "host": "h",\n  "end": 0\n}\n',
      locked: '"host": "h"',
      free: '"port": 8',
      first: ['"name": "a",', '"x": 1,'],
      then: {
        above: ['"name": "a",', '"y": 2,'],
        before: ['"port": 80,', '"y": 2,'],
        below: ['"host": "h",', '"y": 2,'],
      },
    },
    toml: {
      fileName: "m.toml",
      text: 'name = "a"\nport = 80\nhost = "h"\n',
      locked: 'host = "h"',
      free: "port = 8",
      first: ['name = "a"', "x = 1"],
      then: {
        above: ['name = "a"', "y = 2"],
        before: ["port = 80", "y = 2"],
        below: ['host = "h"', "y = 2"],
      },
    },
    hcl: {
      fileName: "m.tf",
      text: 'name = "a"\nport = 80\nhost = "h"\n',
      locked: 'host = "h"',
      free: "port = 8",
      first: ['name = "a"', "x = 1"],
      then: {
        above: ['name = "a"', "y = 2"],
        before: ["port = 80", "y = 2"],
        below: ['host = "h"', "y = 2"],
      },
    },
    xml: {
      fileName: "m.xml",
      text: "<c>\n  <name>a</name>\n  <port>80</port>\n  <host>h</host>\n</c>\n",
      locked: "<host>h</host>",
      free: "<port>8",
      first: ["<name>a</name>", "<x>1</x>"],
      then: {
        above: ["<name>a</name>", "<y>2</y>"],
        before: ["<port>80</port>", "<y>2</y>"],
        below: ["<host>h</host>", "<y>2</y>"],
      },
    },
  };

  const PATH = { xml: "/c/host" };

  async function open(page) {
    page.errors = [];
    page.on("console", (message) => {
      if (message.type() === "error") page.errors.push(message.text());
    });
    page.on("pageerror", (error) => page.errors.push(String(error)));
    await page.goto(example("vanilla.html"));
    await expect(page.locator("body")).toHaveAttribute("data-ready", "true", {
      timeout: 20000,
    });
    await page.evaluate(async () => {
      window.editor?.destroy();
      window.documentsView?.destroy();
      window.module = await import("../config-editor/index.js");
      window.tools = await import("../config-editor/tools.js");
    });
  }

  const input = (page) =>
    page.locator("[data-host] .config-editor__input:visible");

  async function caretAfter(page, marker, offset = marker.length) {
    await input(page).evaluate(
      (element, [marker, offset]) => {
        const at = element.value.indexOf(marker);
        if (at < 0) throw new Error(`no ${marker} in ${element.value}`);
        element.focus();
        element.setSelectionRange(at + offset, at + offset);
      },
      [marker, offset],
    );
  }

  /** Mounts `language`'s document locked on its field; the host keeps every text sent and lists only by `list(text)`. */
  async function mount(page, kind, language, style) {
    await page.evaluate(
      ([kind, language, style, sample, path]) => {
        window.mounted?.destroy();
        document
          .querySelectorAll("[data-host]")
          .forEach((element) => element.remove());
        const element = document.createElement("div");
        element.dataset.host = "";
        element.style.height = "400px";
        document.body.prepend(element);
        const ranges = (text) => {
          const found = tools.rangeOf(language, text, path);
          return [
            style === "indices"
              ? { start: found.start, end: found.end }
              : {
                  startLine: found.startLine,
                  startColumn: found.startColumn,
                  endLine: found.endLine,
                  endColumn: found.endColumn,
                },
          ];
        };
        window.sent = [];
        window.saved = [];
        const { text, fileName } = sample;
        if (kind === "documents") {
          const files = (value) => [
            { id: "m", fileName, value, readOnlyRanges: ranges(value) },
          ];
          window.mounted = new module.ConfigDocuments(element, {
            documents: files(text),
            echoes: true,
            onChange: (id, value) => sent.push(value),
            onSave: (id, value) => saved.push(value),
          });
          window.list = (value) => mounted.update({ documents: files(value) });
          window.current = () => mounted.editor("m").value;
          window.saveNow = () => mounted.save("m");
        } else {
          window.mounted = new module.ConfigEditor(element, {
            language,
            value: text,
            readOnlyRanges: ranges(text),
            echoes: true,
            onChange: (value) => sent.push(value),
            onSave: (value) => saved.push(value),
          });
          window.list = (value) =>
            mounted.update({ language, value, readOnlyRanges: ranges(value) });
          window.current = () => mounted.value;
          window.saveNow = () => mounted.save();
        }
      },
      [kind, language, style, SAMPLES[language], PATH[language] ?? "/host"],
    );
  }

  const wait = (page, ms) =>
    page.evaluate((ms) => new Promise((resolve) => setTimeout(resolve, ms)), ms);

  async function typeLine(page, [after, line]) {
    await caretAfter(page, after);
    await page.keyboard.press("Enter");
    await page.keyboard.type(line, { delay: 20 });
    await wait(page, 400);
  }

  async function check(page, kind, language, style, lists, then) {
    const sample = SAMPLES[language];
    const name = `${kind} ${language} ${style} ${lists} ${then}`;
    const failures = [];
    await mount(page, kind, language, style);
    await typeLine(page, sample.first);
    await typeLine(page, [
      sample.first[1],
      sample.first[1].replace("x", "w").replace("x", "w").replace("1", "3"),
    ]);
    const echoes = await page.evaluate(() => sent.length);
    await typeLine(page, sample.then[then]);
    await page.evaluate(
      ([lists, n]) => {
        const middle = Math.floor(n / 2);
        const order = {
          sameTwice: [n - 1, n - 1],
          twoEchoes: [n - 2, n - 1],
          three: [n - 1, n - 1, n - 1],
          newestFirstMiddle: [n - 1, 0, middle],
          newestSecondMiddle: [n - 1, 1, middle],
          newestFirstSecond: [n - 1, 0, 1],
        }[lists];
        for (const at of order) list(sent[at]);
      },
      [lists, echoes],
    );
    await wait(page, 100);
    const edited = await page.evaluate(() => current());
    if (
      !edited.includes(sample.first[1]) ||
      !edited.includes(sample.then[then][1]) ||
      !edited.includes(sample.locked)
    ) {
      failures.push(`${name}: the document changed: ${JSON.stringify(edited)}`);
    }
    // The locked value refuses typing.
    await caretAfter(page, sample.locked, sample.locked.lastIndexOf("h") + 1);
    await page.keyboard.type("ACKED");
    await wait(page, 60);
    const after = await page.evaluate(() => current());
    if (after !== edited)
      failures.push(
        `${name}: the locked value took typing: ${JSON.stringify(after)}`,
      );
    // The free field above it takes typing.
    await caretAfter(page, sample.free);
    await page.keyboard.type("7");
    await wait(page, 60);
    const freed = await page.evaluate(() => current());
    if (!freed.includes(`${sample.free}7`))
      failures.push(
        `${name}: the free field refused typing: ${JSON.stringify(freed)}`,
      );
    const saved = await page.evaluate(() => {
      saveNow();
      return saved.at(-1) ?? null;
    });
    if (!saved?.includes(sample.locked) || saved.includes("ACKED"))
      failures.push(`${name}: saved ${JSON.stringify(saved)}`);
    return failures;
  }


  test.describe("ranges worked out in an echo, listed again, move once", () => {
  test.skip(!built, "run `npm run build` in js/ first");

  for (const kind of ["documents", "editor"]) {
      for (const lists of [
        "sameTwice",
        "twoEchoes",
        "three",
        "newestFirstMiddle",
      "newestSecondMiddle",
      "newestFirstSecond",
    ]) {
      test(`${kind}, ${lists}`, async ({ page }) => {
          test.setTimeout(300_000);
          await open(page);
          const failures = [];
          for (const language of Object.keys(SAMPLES)) {
            for (const style of ["lines", "indices"]) {
            for (const then of ["above", "before", "below"]) {
                try {
                  failures.push(
                    ...(await check(page, kind, language, style, lists, then)),
                  );
                } catch (error) {
                  failures.push(
                    `${kind} ${language} ${style} ${lists} ${then}: ${String(error).split("\n")[0]}`,
                  );
                }
              }
            }
          }
          expect(failures).toEqual([]);
          expect(page.errors).toEqual([]);
        });
      }
    }
  });
});
