import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

// Edit as another language keeps the host's forms: comments, quoting, numbers, dates, block styles,
// tags, merges and attributes survive edits, moves and copies made in the other view.

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

const saved = (page) => page.locator('.demo__output-text').textContent();

// --- Fourth-round findings: Edit-as saving and diagnostics ---

const savedText = (page) => page.locator('.demo__output-text').textContent();

const pick = (page, value) => page.locator('.demo__edit-select').selectOption(value);

// Puts the caret just after `needle` (its `offset`-th character with one).
async function caretAfter(e, needle, offset = needle.length) {
  await e.input.evaluate(
    (element, [find, at]) => {
      const start = element.value.indexOf(find);
      if (start < 0) throw new Error(`no ${find}`);
      element.focus();
      element.setSelectionRange(start + at, start + at);
    },
    [needle, offset],
  );
}

// Selects `needle` (or puts the caret after it with `collapse`).
async function select(e, needle, collapse = false) {
  await e.input.evaluate(
    (element, [find, end]) => {
      const start = element.value.indexOf(find);
      if (start < 0) throw new Error(`no ${find}`);
      element.focus();
      const stop = start + find.length;
      element.setSelectionRange(end ? stop : start, stop);
    },
    [needle, collapse],
  );
}

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

const js = join(root, 'js');

const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

const input = (page) => page.locator('#qa .config-editor__input');

const value = (page) => input(page).inputValue();

async function caretAt(page, marker, offset = 0, length = 0) {
  const text = await value(page);
  const at = text.indexOf(marker);
  expect(at, `${JSON.stringify(marker)} in the text`).toBeGreaterThanOrEqual(0);
  await input(page).evaluate(
    (element, [start, end]) => {
      element.focus();
      element.setSelectionRange(start, end);
    },
    [at + offset, at + offset + length],
  );
}

async function open(page) {
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async () => {
    window.module = await import('../config-editor/index.js');
    window.tools = await import('../config-editor/tools.js');
    const parse = (text) => {
      const value = {};
      text.split('\n').forEach((line, index) => {
        if (!line.trim() || line.trim().startsWith('#')) return;
        const at = line.indexOf('=');
        if (at < 0) throw { message: 'expected KEY=value', line: index + 1, column: 1 };
        value[line.slice(0, at).trim()] = line.slice(at + 1).trim();
      });
      return value;
    };
    window.envParse = parse;
    const serialize = (value) =>
      Object.entries(value)
        .map(([key, item]) => `${key}=${item}`)
        .join('\n') + '\n';
    window.module.registerLanguage({ label: 'Bare21', parse, serialize });
  });
}

/** A new editor in `#qa`, recording what reaches the host. */
async function mount(page, options) {
  await page.evaluate((options) => {
    window.editor?.destroy();
    window.qa?.destroy();
    let host = document.getElementById('qa');
    if (!host) {
      host = document.createElement('div');
      host.id = 'qa';
      document.body.prepend(host);
    }
    window.sent = [];
    window.saved = [];
    window.qa = new window.module.ConfigEditor(host, {
      showFooter: true,
      ...options,
      onChange: (text) => window.sent.push(text),
      onSave: (text) => window.saved.push(text),
    });
  }, options);
  await input(page).waitFor();
}

const TAGGED = "# stack\nres: !GetAtt [r, Arn]\nname: !Sub 'x-${y}'\nport: 1\ninc: !include\n  k: v\nkey: !secret pw\nrefs: !reference [.base, script]\n";

test.describe('edit as: structural edits keep host comments', () => {
  setupEditor(test, '?language=yaml&edit=json');

  test('adding a key while editing as JSON keeps the YAML comment', async ({ page }) => {
    const e = editor(page);
    const text = await e.value();
    await e.paste(text.replace('{\n', '{\n  "heartbeat": {"interval_secs": 5},\n'));
    await expect
      .poll(() => page.locator('.demo__output-text').textContent())
      .toContain('heartbeat');
    const saved = await page.locator('.demo__output-text').textContent();
    expect(saved).toContain('# Tunnel server configuration');
  });
});

test.describe('edit as XML keeps untouched host values', () => {
  setupEditor(test, '?language=yaml&edit=xml');

  test('changing one value leaves null and quoted values alone', async ({ page }) => {
    const e = editor(page);
    await page.locator('.demo__edit-select').selectOption('');
    await e.paste('public_address: null\ninsecure_plaintext: false\n');
    await page.locator('.demo__edit-select').selectOption('xml');
    await expect(e.input).toHaveValue(/<insecure_plaintext>false<\/insecure_plaintext>/);
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('>false<') + 1;
      element.focus();
      element.setSelectionRange(at, at + 5);
    });
    await page.keyboard.type('true');
    await expect.poll(() => saved(page)).toContain('insecure_plaintext: true');
    expect(await saved(page)).toContain('public_address: null');
  });
});

test.describe('edit as JSON keeps comments on structural edits', () => {
  setupEditor(test, '?language=yaml');

  test('a key added after a line with a trailing comment keeps every comment', async ({ page }) => {
    const e = editor(page);
    await e.paste('# Burrow server configuration\nlisten: 7835 # main port\ninsecure_plaintext: false\n');
    await page.locator('.demo__edit-select').selectOption('json');
    await expect(e.input).toHaveValue(/"listen": 7835/);
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('"listen": 7835,') + '"listen": 7835,'.length;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('Enter');
    await page.keyboard.type('"public_address": "x",');
    await expect.poll(() => saved(page)).toContain('public_address: x');
    const text = await saved(page);
    expect(text).toContain('# Burrow server configuration');
    expect(text).toContain('# main port');
  });
});

test.describe('edit a TOML host as YAML: deleting a table', () => {
  setupEditor(test, '?language=toml');

  test('removes only that table and its comment', async ({ page }) => {
    const e = editor(page);
    const host = '# head\nlisten = 7835 # port\n\n# acme client\n[clients.acme]\nsecret = "s1" # the secret\n\n# beta client\n[clients.beta]\nsecret = "s2"\n';
    await e.paste(host);
    await page.locator('.demo__edit-select').selectOption('yaml');
    await expect(e.input).toHaveValue(/beta:/);
    await e.input.evaluate((element) => {
      const start = element.value.indexOf('  beta:');
      const end = element.value.indexOf('s2', start) + 3;
      element.focus();
      element.setSelectionRange(start, end);
    });
    await page.keyboard.press('Delete');
    await expect.poll(() => savedText(page)).not.toContain('beta');
    expect(await savedText(page)).toBe('# head\nlisten = 7835 # port\n\n# acme client\n[clients.acme]\nsecret = "s1" # the secret\n');
  });
});

test.describe('edit a YAML host as JSON: adding to a flow map', () => {
  setupEditor(test, '?language=yaml');

  test('rewrites only that map, on its line', async ({ page }) => {
    const e = editor(page);
    await e.paste('# head\ntcp: {nodelay: true} # tcp\nlisten: 1 # port\n');
    await page.locator('.demo__edit-select').selectOption('json');
    await expect(e.input).toHaveValue(/"nodelay": true/);
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('"nodelay": true') + '"nodelay": true'.length;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type(', "keepalive_secs": 3');
    await expect.poll(() => savedText(page)).toContain('keepalive_secs');
    expect(await savedText(page)).toBe('# head\ntcp: {nodelay: true, keepalive_secs: 3} # tcp\nlisten: 1 # port\n');
  });
});

test.describe('a TOML date is text', () => {
  setupEditor(test, '?language=toml');

  test('no internal key leaks into messages or the YAML view', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen = 1\npublic_address = 1979-05-27\n');
    await page.locator('.demo__edit-select').selectOption('yaml');
    await expect(e.input).toHaveValue(/public_address: "?1979-05-27"?/);
    expect(await e.value()).not.toContain('toml_private');
  });
});

test.describe('a YAML host viewed as XML: values XML cannot show', () => {
  setupEditor(test, '?language=yaml');

  test('are flagged as warnings instead of silently changed', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: 1\nx:\n  - [1, 2]\n  - [3]\nallow: []\n');
    await page.locator('.demo__edit-select').selectOption('xml');
    await expect(e.input).toHaveValue(/<listen>1<\/listen>/);
    const warnings = page.locator('.config-editor__diagnostic-marker-warning');
    await expect(warnings).toHaveCount(2);
    const messages = await warnings.evaluateAll((all) => all.map((marker) => marker.dataset.message).join('\n'));
    expect(messages).toContain('/x');
    expect(messages).toContain('/allow');
    expect(messages).toContain('cannot show this value exactly');
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  const saved = (page) => page.locator('.demo__output-text').textContent();

  async function at(page, marked) {
    const e = editor(page);
    const first = marked.indexOf('¦');
    const last = marked.lastIndexOf('¦');
    const [start, end] = first === last ? [first, first] : [first, last - 1];
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


  test.describe('Edit-as next to an equal sibling', () => {
    setupEditor(test, '?language=yaml');

    test('a block-style sibling equal to a one-line container keeps its style and comment', async ({ page }) => {
      const e = editor(page);
      await e.paste('# c\nlisten: 1\ntcp: {nodelay: true}\nudp:\n  nodelay: true # u\n');
      await page.locator('.demo__edit-select').selectOption('json');
      await expect(e.input).toHaveValue(/"udp"/);
      await e.input.evaluate((element) => {
        const at = element.value.indexOf('"listen": 1,') + '"listen": 1,'.length;
        element.focus();
        element.setSelectionRange(at, at);
      });
      await page.keyboard.press('Enter');
      await page.keyboard.type('"insecure_plaintext": true,', { delay: 10 });
      await page.keyboard.press('Escape');
      await expect.poll(() => saved(page)).toContain('insecure_plaintext: true');
      expect(await saved(page)).toContain('udp:\n  nodelay: true # u');
      expect(await saved(page)).toContain('tcp: {nodelay: true}');
    });
  });
});

// QA round 17 Edit-as saves, typed in another language's view: a value
// starting with a YAML indicator is quoted, a block in a list item's map and
// a compact nested sequence are spliced in place with the comments kept, a
// carriage return is escaped, an XML comment in text survives.
test.describe('YAML edited as JSON', () => {
  setupEditor(test, '?language=yaml');

  test('a value given a leading % or a YAML 1.1 word is written quoted', async ({ page }) => {
    const e = editor(page);
    await e.paste('# c\ns: x # sc\nt: z\n');
    await expect.poll(() => saved(page)).toContain('t: z');
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"x"');
    await caretAfter(e, '"x', 1);
    await page.keyboard.type('%');
    await expect.poll(() => saved(page)).toBe('# c\ns: "%x" # sc\nt: z\n');
    await caretAfter(e, '"z', 1);
    await page.keyboard.press('Delete');
    await page.keyboard.type('yes');
    await expect.poll(() => saved(page)).toBe('# c\ns: "%x" # sc\nt: "yes"\n');
  });

  test('a block in a list item map and a compact nested list keep the comments', async ({ page }) => {
    const e = editor(page);
    const host = '# top\nm:\n  - k: |\n      a\n    # kc\n    j: 2 # jc\n  - - b # bc\n    - c\nn: 1 # nc\n';
    await e.paste(host);
    await expect.poll(() => saved(page)).toContain('n: 1');
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"c"');
    await caretAfter(e, '"a', 2);
    await page.keyboard.type('\\tq');
    await expect.poll(() => saved(page)).toBe(host.replace('  - k: |\n      a\n', '  - k: |\n      a\tq\n'));
    await caretAfter(e, '"c', 2);
    await page.keyboard.type('Q');
    await expect.poll(() => saved(page)).toContain('    - cQ\nn: 1 # nc\n');
    expect(await saved(page)).toContain('  - - b # bc\n');
  });
});

test.describe('a carriage return typed in another view', () => {
  for (const [language, host, needle, expected] of [
    ['toml', '# c\nml = """\nl1\nl2\n"""\nn = 1\n', 'l1', '# c\nml = """\nl1\\r\nl2\n"""\nn = 1\n'],
    ['hcl', '# c\nml = <<EOT\nl1\nl2\nEOT\nn = 1\n', 'l1', '# c\nml = "l1\\r\\nl2\\n"\nn = 1\n'],
  ]) {
    test.describe(language, () => {
      setupEditor(test, `?language=${language}`);
      test('is written as an escape', async ({ page }) => {
        const e = editor(page);
        await e.paste(host);
        await expect.poll(() => saved(page)).toContain('n = 1');
        await pick(page, 'json');
        await expect.poll(() => e.value()).toContain(needle);
        await caretAfter(e, needle);
        await page.keyboard.type('\\r');
        await expect.poll(() => saved(page)).toBe(expected);
      });
    });
  }
});

test.describe('XML text around a comment edited as YAML', () => {
  setupEditor(test, '?language=xml');

  test('keeps the comment and the CDATA beside text with an attribute', async ({ page }) => {
    const e = editor(page);
    const host = '<root>\n  <p k="v">alpha<!-- c -->beta</p>\n  <q k="v">t<![CDATA[x < y]]>u</q>\n</root>\n';
    await e.paste(host);
    await expect.poll(() => saved(page)).toContain('CDATA');
    await pick(page, 'yaml');
    await expect.poll(() => e.value()).toContain('alphabeta');
    await caretAfter(e, 'alphabeta');
    await page.keyboard.type('X');
    await expect.poll(() => saved(page)).toContain('<p k="v">alpha<!-- c -->betaX</p>');
    await caretAfter(e, 'tx < yu', 7);
    await page.keyboard.type('Z');
    await expect.poll(() => saved(page)).toBe(host.replace('alpha<!-- c -->beta', 'alpha<!-- c -->betaX').replace('u</q>', 'uZ</q>'));
  });
});

// QA round 18 Edit-as saves, typed in another language's view: an
// indentless YAML sequence and a quoted YAML scalar over lines are spliced
// in place with the comments kept, a renamed YAML key YAML 1.1 would read as
// a boolean is quoted, and a TOML datetime typed half-way waits and is
// saved as a datetime once it is one again.
test.describe('YAML edited as JSON', () => {
  setupEditor(test, '?language=yaml');

  test('an indentless sequence and a quoted scalar over lines keep their lines', async ({ page }) => {
    const e = editor(page);
    const host = '# top\nl:\n- k: >-\n    x\n  j: 2 # jc\n- - a # ac\n  - b\nml: "l1\n  l2"  # mc\nn: 1  # nc\n';
    await e.paste(host);
    await expect.poll(() => saved(page)).toContain('n: 1');
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"b"');
    await caretAfter(e, '"x', 2);
    await page.keyboard.type('Q');
    await expect.poll(() => saved(page)).toBe(host.replace('    x\n', '    xQ\n'));
    await caretAfter(e, '"b', 2);
    await page.keyboard.type('Z');
    await expect.poll(() => saved(page)).toContain('  - bZ\nml:');
    await caretAfter(e, 'l2', 2);
    await page.keyboard.type('Y');
    await expect.poll(() => saved(page)).toBe(
      host.replace('    x\n', '    xQ\n').replace('  - b\n', '  - bZ\n').replace('ml: "l1\n  l2"', 'ml: "l1\n  l2Y"'),
    );
  });

  test('a key renamed to on is written quoted, keeping the spacing before its comment', async ({ page }) => {
    const e = editor(page);
    await e.paste('# top\nport: 1  # pc\nn: 1\n');
    await expect.poll(() => saved(page)).toContain('n: 1');
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"port"');
    await caretAfter(e, '"port', 1);
    for (let i = 0; i < 4; i++) await page.keyboard.press('Shift+ArrowRight');
    await page.keyboard.type('on');
    await expect.poll(() => saved(page)).toBe('# top\n"on": 1  # pc\nn: 1\n');
  });
});

test.describe('TOML edited as JSON', () => {
  setupEditor(test, '?language=toml');

  test('a datetime typed half-way waits, then is saved as a datetime', async ({ page }) => {
    const e = editor(page);
    const host = '# c\ndt = 1979-05-27T07:32:00Z  # dtc\nn = 1\n';
    await e.paste(host);
    await expect.poll(() => saved(page)).toContain('n = 1');
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('07:32:00Z');
    await caretAfter(e, '07:32:00', 8);
    await page.keyboard.press('Backspace');
    await expect(page.locator('.demo')).toContainText('not a TOML date or time');
    expect(await saved(page)).toBe(host);
    await page.keyboard.type('1');
    await expect.poll(() => saved(page)).toBe(host.replace('07:32:00Z', '07:32:01Z'));
  });
});

// QA round 19 Edit-as saves, typed in another language's view: an edit
// elsewhere keeps XML's `1.0` as written, a value typed in the XML view
// keeps the host's type (an emptied element is no `{}`, a TOML datetime
// retyped there stays a datetime), and YAML list items of maps moved in
// another view take their comments along.
test.describe('XML edited as JSON', () => {
  setupEditor(test, '?language=xml');

  test('an edit elsewhere keeps 1.0 and -0.0 as written', async ({ page }) => {
    const e = editor(page);
    const host = '<!-- top -->\n<r a="2.0">\n  <name>edge</name>\n  <version>1.0</version>\n  <z>-0.0</z>\n</r>\n';
    await e.paste(host);
    await expect.poll(() => saved(page)).toContain('<z>');
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"edge"');
    await select(e, 'edge', true);
    await page.keyboard.type('Q');
    await expect.poll(() => saved(page)).toBe(host.replace('edge', 'edgeQ'));
  });
});

test.describe('YAML edited as XML', () => {
  setupEditor(test, '?language=yaml');

  test('an emptied element saves the empty string, and a typed number stays a string', async ({ page }) => {
    const e = editor(page);
    const host = '# top\nname: edge  # nc\nver: "1"\nport: 80\n';
    await e.paste(host);
    await expect.poll(() => saved(page)).toContain('port: 80');
    await pick(page, 'xml');
    await expect.poll(() => e.value()).toContain('<name>edge</name>');
    await select(e, 'edge');
    await page.keyboard.press('Delete');
    await expect.poll(() => saved(page)).toMatch(/^# top\nname: (""|'')  # nc\nver: "1"\nport: 80\n$/);
    await page.keyboard.type('9');
    await expect.poll(() => saved(page)).toMatch(/^# top\nname: ("9"|'9')  # nc\n/);
    await select(e, '<ver>1', true);
    await page.keyboard.type('2');
    await expect.poll(() => saved(page)).toContain('ver: "12"\n');
  });

  test('map items moved in the JSON view take their comments', async ({ page }) => {
    const e = editor(page);
    const host = '# top\nl:\n  - name: a  # ac\n    v: 1\n  - name: b  # bc\n    v: 2\nn: 1\n';
    await e.paste(host);
    await expect.poll(() => saved(page)).toContain('n: 1');
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"b"');
    const shown = await e.value();
    const first = '{\n      "name": "a",\n      "v": 1\n    }';
    const second = '{\n      "name": "b",\n      "v": 2\n    }';
    await select(e, `${first},\n    ${second}`);
    await page.keyboard.insertText(`${second},\n    ${first}`);
    expect(shown).toContain(first);
    await expect.poll(() => saved(page)).toBe(
      '# top\nl:\n  - name: b  # bc\n    v: 2\n  - name: a  # ac\n    v: 1\nn: 1\n',
    );
  });
});

test.describe('TOML edited as XML', () => {
  setupEditor(test, '?language=toml');

  test('a datetime emptied and retyped waits, then is saved as a datetime', async ({ page }) => {
    const e = editor(page);
    const host = '# c\ndt = 1979-05-27T07:32:00Z  # dtc\nn = 1\n';
    await e.paste(host);
    await expect.poll(() => saved(page)).toContain('n = 1');
    await pick(page, 'xml');
    await expect.poll(() => e.value()).toContain('<dt>');
    await select(e, '1979-05-27T07:32:00Z');
    await page.keyboard.press('Delete');
    await page.keyboard.type('2');
    await expect(page.locator('.demo')).toContainText('not a TOML date or time');
    expect(await saved(page)).toBe(host);
    await page.keyboard.type('024-01-02T03:04:05Z');
    await expect.poll(() => saved(page)).toBe(host.replace('1979-05-27T07:32:00Z', '2024-01-02T03:04:05Z'));
  });
});

test.describe('YAML edited in other views', () => {
  setupEditor(test, '?language=yaml');
  test('a list in a list item reversed as JSON takes its comments', async ({ page }) => {
    const e = editor(page);
    const host = 'l:\n  - name: a\n    sub:\n      # about 1\n      - 1  # one\n      - 2  # two\nn: 1\n';
    await e.paste(host);
    await expect.poll(() => saved(page)).toContain('# two');
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"sub"');
    const shown = await e.value();
    const list = shown.match(/"sub": (\[[^\]]*\])/)[1];
    await select(e, list);
    await page.keyboard.type('[2, 1]');
    await expect
      .poll(() => saved(page))
      .toBe('l:\n  - name: a\n    sub:\n      - 2  # two\n      # about 1\n      - 1  # one\nn: 1\n');
  });
  test('a list emptied as TOML keeps the gap before its comment', async ({ page }) => {
    const e = editor(page);
    await e.paste('ports:    # pc\n  - 80\nn: 1\n');
    await expect.poll(() => saved(page)).toContain('- 80');
    await pick(page, 'toml');
    await expect.poll(() => e.value()).toContain('ports = [80]');
    await select(e, '80');
    await page.keyboard.press('Delete');
    await expect.poll(() => saved(page)).toBe('ports: []    # pc\nn: 1\n');
  });
  test('a string over lines edited as JSON keeps its lines', async ({ page }) => {
    const e = editor(page);
    await e.paste('# c\nml: "l1\n  l2"  # mc\nn: 1\n');
    await expect.poll(() => saved(page)).toContain('# mc');
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"l1 l2"');
    await select(e, 'l1 l2', true);
    await page.keyboard.type('Q');
    await expect.poll(() => saved(page)).toBe('# c\nml: "l1\n  l2Q"  # mc\nn: 1\n');
  });
  test('a whole float edited as HCL stays a float', async ({ page }) => {
    const e = editor(page);
    await e.paste('# top\nf: 1.0\nn: x\n');
    await expect.poll(() => saved(page)).toContain('f: 1.0');
    await pick(page, 'hcl');
    // Shown as written (round 21), and an integer typed over it is a float.
    await expect.poll(() => e.value()).toContain('f = 1.0');
    await select(e, '1.0');
    await page.keyboard.type('2');
    await expect.poll(() => saved(page)).toBe('# top\nf: 2.0\nn: x\n');
  });
  test('0x51 typed in the XML view is a number', async ({ page }) => {
    const e = editor(page);
    await e.paste('# top\nport: 80  # pc\nn: x\n');
    await expect.poll(() => saved(page)).toContain('# pc');
    await pick(page, 'xml');
    await expect.poll(() => e.value()).toContain('<port>80</port>');
    await select(e, '>80<');
    await page.keyboard.type('>0x51<');
    // A number, kept as typed (round 21): YAML reads `0x51` as 81 too.
    await expect.poll(() => saved(page)).toBe('# top\nport: 0x51  # pc\nn: x\n');
  });
});

test.describe('YAML tags and several documents', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('tags are valid YAML, read as the values under them, kept through Edit as', async ({ page }) => {
    await open(page);
    const headless = await page.evaluate((text) => ({ problems: window.tools.validate('yaml', text), data: window.tools.parse('yaml', text) }), TAGGED);
    expect(headless.problems).toEqual([]);
    expect(headless.data).toEqual({ res: ['r', 'Arn'], name: 'x-${y}', port: 1, inc: { k: 'v' }, key: 'pw', refs: ['.base', 'script'] });
    await mount(page, { language: 'yaml', value: TAGGED, editAs: 'json' });
    await expect.poll(() => value(page)).toContain('"port": 1');
    await caretAt(page, '"port": 1', 8, 1);
    await page.keyboard.type('2');
    expect(await page.evaluate(() => window.qa.save())).toBe(true);
    expect(await page.evaluate(() => window.saved)).toEqual([TAGGED.replace('port: 1', 'port: 2')]);
  });

  test('several documents: a plain message, and Edit as refused with it', async ({ page }) => {
    await open(page);
    const problems = await page.evaluate(() => window.tools.validate('yaml', 'a: 1\n---\nb: 2\n'));
    expect(problems).toHaveLength(1);
    expect(problems[0]).toMatchObject({ line: 2, column: 1 });
    expect(problems[0].message).toContain('more than one YAML document');
    expect(problems[0].message).not.toMatch(/deserializ|serde/);
    await mount(page, { language: 'yaml', value: 'a: 1\n---\nb: 2\n', editAs: 'json' });
    const messages = () =>
      page.locator('#qa .config-editor__diagnostic-marker').evaluateAll((markers) => markers.map((marker) => marker.dataset.message));
    await expect.poll(messages).toContainEqual(expect.stringMatching(/^Cannot switch to JSON yet: .*more than one YAML document/));
    await expect(input(page)).toHaveValue('a: 1\n---\nb: 2\n');
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  // QA round 21 Edit-as saves: list items moved while others are added or
  // removed keep their comments (scalars, XML elements with children, TOML
  // inline tables), numbers and booleans typed in their own spelling in the
  // XML view keep the host's type and the spelling, a quoted string emptied
  // and typed again after the save keeps its quotes, and an HCL heredoc whose
  // text gains its delimiter takes another one.
  const saved = (page) => page.locator('.demo__output-text').textContent();

  const pick = (page, value) => page.locator('.demo__edit-select').selectOption(value);

  // Replaces the first `needle` in the view with `text`, as one input.
  async function replace(page, e, needle, text) {
    await e.input.evaluate((element, find) => {
      const start = element.value.indexOf(find);
      if (start < 0) throw new Error(`no ${find} in ${element.value}`);
      element.focus();
      element.setSelectionRange(start, start + find.length);
    }, needle);
    if (text) await page.keyboard.insertText(text);
    else await page.keyboard.press('Delete');
  }

  // Loads `host` and switches the view to `view`, waiting for `shown` there.
  async function open(page, host, view, shown) {
    const e = editor(page);
    await e.paste(host);
    await expect.poll(() => saved(page)).toBe(host);
    await pick(page, view);
    await expect.poll(() => e.value()).toContain(shown);
    return e;
  }


  test.describe('YAML host', () => {
    setupEditor(test, '?language=yaml');

    test('items moved with one added keep their comment lines, as JSON', async ({ page }) => {
      const host = "tags:\n  # first tag\n  - web\n  - 'api'   # quoted\nn: 1\n";
      const e = await open(page, host, 'json', '"tags"');
      const list = (await e.value()).match(/"tags": (\[[^\]]*\])/)[1];
      await replace(page, e, list, '["new", "api", "web"]');
      await expect
        .poll(() => saved(page))
        .toBe("tags:\n  - new\n  - 'api'   # quoted\n  # first tag\n  - web\nn: 1\n");
    });

    test('numbers and booleans typed in their own spelling as XML keep their types', async ({ page }) => {
      const host = 'f: 1.50  # f\ni: 5\nb: true\n';
      const e = await open(page, host, 'xml', '<f>1.5</f>');
      await replace(page, e, '<f>1.5</f>', '<f>2.50</f>');
      await expect.poll(() => saved(page)).toBe('f: 2.50  # f\ni: 5\nb: true\n');
      await replace(page, e, '<i>5</i>', '<i>+6</i>');
      await replace(page, e, '<b>true</b>', '<b>False</b>');
      await expect.poll(() => saved(page)).toBe('f: 2.50  # f\ni: +6\nb: false\n');
    });

    test('a quoted string emptied, saved and typed again as JSON keeps its quotes', async ({ page }) => {
      const host = 'a:\n  s: "abc"  # c\n  t: \'def\'\nn: 1\n';
      const e = await open(page, host, 'json', '"abc"');
      await replace(page, e, 'abc', '');
      await replace(page, e, 'def', '');
      // The typing pause ends: the emptied strings are saved.
      await expect.poll(() => saved(page)).toBe('a:\n  s: ""  # c\n  t: \'\'\nn: 1\n');
      await replace(page, e, '""', '"xyz"');
      await replace(page, e, '""', '"uvw"');
      await expect.poll(() => saved(page)).toBe('a:\n  s: "xyz"  # c\n  t: \'uvw\'\nn: 1\n');
    });
  });

  test.describe('XML host', () => {
    setupEditor(test, '?language=xml');

    test('repeated elements with children reordered as JSON take their comments', async ({ page }) => {
      const host =
        '<r>\n  <!-- about a -->\n  <s name="a">\n    <p>1</p>\n  </s>\n  <!-- about b -->\n  <s name="b">\n    <p>2</p>\n  </s>\n</r>\n';
      const e = await open(page, host, 'yaml', 'name: a');
      const shown = await e.value();
      const items = shown.slice(shown.indexOf('    - name: a'));
      await replace(page, e, items, '    - name: b\n      p: 2\n    - name: a\n      p: 1\n');
      await expect
        .poll(() => saved(page))
        .toBe(
          '<r>\n  <!-- about b -->\n  <s name="b">\n    <p>2</p>\n  </s>\n  <!-- about a -->\n  <s name="a">\n    <p>1</p>\n  </s>\n</r>\n',
        );
    });
  });

  test.describe('TOML host', () => {
    setupEditor(test, '?language=toml');

    test('inline tables reordered as JSON take their comment lines', async ({ page }) => {
      const host = 's = [\n  # about a\n  { n = "a" },\n  # about b\n  { n = "b" },\n]\n';
      const e = await open(page, host, 'json', '"s"');
      const list = (await e.value()).match(/"s": (\[[\s\S]*\])/)[1];
      await replace(page, e, list, '[{"n": "b"}, {"n": "a"}, {"n": "c"}]');
      await expect
        .poll(() => saved(page))
        .toBe('s = [\n  # about b\n  { n = "b" },\n  # about a\n  { n = "a" },\n  { n = "c" },\n]\n');
    });
  });

  test.describe('HCL host', () => {
    setupEditor(test, '?language=hcl');

    test('a heredoc whose text gains its delimiter takes another one', async ({ page }) => {
      const host = 'script = <<EOT\nline one\nEOT\nz = 1\n';
      const e = await open(page, host, 'yaml', 'line one');
      await replace(page, e, 'line one', 'line one\n  EOT');
      await expect.poll(() => saved(page)).toBe('script = <<EOT_1\nline one\nEOT\nEOT_1\nz = 1\n');
    });
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  // QA round 21 typing: a YAML block scalar holding an empty line froze the
  // tab (loading it, Enter in it, Edit as YAML of a string with a blank line);
  // copy and cut on a folded header take the whole block; Enter on an empty
  // `- ` item and inside a flow sequence; XML's `</` and an attribute quote
  // before `/>`; HCL's `"${`; a find pattern matching empty text (`^`).

  const saved = (page) => page.locator('.demo__output-text').textContent();

  const pick = (page, value) => page.locator('.demo__edit-select').selectOption(value);

  // Puts the caret at `offset` of the text on screen.
  const caretAt = (e, offset) =>
    e.input.evaluate((element, at) => {
      element.focus();
      element.setSelectionRange(at, at);
    }, offset);


  test.describe('YAML block scalars with empty lines', () => {
    setupEditor(test, '?language=yaml');
    test('a string with a blank line edited as YAML', async ({ page }) => {
      await page.goto(page.url().replace('language=yaml', 'language=json'));
      await page.locator('.config-editor__input').waitFor();
      const e = editor(page);
      await e.paste('{\n  "s": "a\\n\\nb",\n  "n": 1\n}\n');
      await expect.poll(() => saved(page)).toContain('"n": 1');
      await pick(page, 'yaml');
      await expect.poll(() => e.value()).toContain('s: |');
      const shown = await e.value();
      // Enter at the end of the block's first line, then text.
      await caretAt(e, shown.indexOf('\n\n'));
      await page.keyboard.press('Enter');
      await page.keyboard.type('x');
      await expect.poll(() => saved(page)).toContain('"s": "a\\nx\\n\\nb"');
    });
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  // QA round 24 editing, as a user makes the saves: a leading byte order
  // mark stays through an edit of the first key from another view, a YAML
  // root written as a flow collection keeps its form, an XML attribute stays
  // one when the child of its name goes, and tagged YAML list items move
  // with their tags.
  const saved = (page) => page.locator('.demo__output-text').textContent();

  const pick = (page, value) => page.locator('.demo__edit-select').selectOption(value);

  // Puts the caret just after `needle`.
  async function caretAfter(e, needle) {
    await e.input.evaluate((element, find) => {
      const start = element.value.indexOf(find);
      if (start < 0) throw new Error(`no ${find}`);
      element.focus();
      element.setSelectionRange(start + find.length, start + find.length);
    }, needle);
  }

  // Selects `needle` and types `text` over it.
  async function replace(page, e, needle, text) {
    await e.input.evaluate((element, find) => {
      const start = element.value.indexOf(find);
      if (start < 0) throw new Error(`no ${find}`);
      element.focus();
      element.setSelectionRange(start, start + find.length);
    }, needle);
    await page.keyboard.insertText(text);
  }


  test.describe('a YAML root written as a flow collection', () => {
    setupEditor(test, '?language=yaml');

    test('keeps its form and comments through an edit as JSON', async ({ page }) => {
      const e = editor(page);
      await e.paste('# c\n[a, b] # t\n');
      await expect.poll(() => saved(page)).toBe('# c\n[a, b] # t\n');
      await pick(page, 'json');
      await expect.poll(() => e.value()).toContain('"b"');
      await caretAfter(e, '"b');
      await page.keyboard.type('x');
      await expect.poll(() => saved(page)).toBe('# c\n[a, bx] # t\n');
    });

    test('tagged list items move with their tags', async ({ page }) => {
      const e = editor(page);
      const host = 'l:\n  # A\n  - !A {x: 1}\n  # B\n  - !B {x: 2}\n';
      await e.paste(host);
      await expect.poll(() => saved(page)).toBe(host);
      await pick(page, 'json');
      await expect.poll(() => e.value()).toContain('"x": 2');
      const shown = await e.value();
      const list = shown.slice(shown.indexOf('['), shown.lastIndexOf(']') + 1);
      await replace(page, e, list, '[{"x": 2}, {"x": 1}]');
      await expect.poll(() => saved(page)).toBe('l:\n  # B\n  - !B {x: 2}\n  # A\n  - !A {x: 1}\n');
    });
  });

  test.describe('an XML attribute beside a child of its name', () => {
    setupEditor(test, '?language=xml');

    test('stays an attribute when the child is removed as JSON', async ({ page }) => {
      const e = editor(page);
      const host = '<r a="1">\n  <a>2</a> <!-- c -->\n  <z>0</z> <!-- z -->\n</r>\n';
      await e.paste(host);
      await expect.poll(() => saved(page)).toBe(host);
      await pick(page, 'json');
      await expect.poll(() => e.value()).toContain('"@a"');
      const shown = await e.value();
      const child = shown.match(/\n\s*"a": 2,/)[0];
      await replace(page, e, child, '');
      await expect.poll(() => saved(page)).toBe('<r a="1">\n  <z>0</z> <!-- z -->\n</r>\n');
    });
  });
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  // QA round 25 (editing), through the npm package's example page with real
  // keys: folds stay closed through an Edit-as switch (carried by their
  // field); an XML child removed and added back in one Edit-as session is a
  // child again, its attribute kept; a YAML merge key stays when a merged
  // value is edited from another view. Run `npm run build` in js/ first.
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (options) =>
    `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

  const input = (page) => page.locator('.config-editor__input').first();

  async function open(page, options) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    await page.goto(example(options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', {
      timeout: 20000,
    });
    await input(page).waitFor();
    return errors;
  }

  /** Selects `needle` (its `nth` occurrence) in the textarea. */
  async function select(page, needle, nth = 0) {
    await input(page).evaluate(
      (area, [needle, nth]) => {
        let at = -1;
        for (let index = 0; index <= nth; index++) at = area.value.indexOf(needle, at + 1);
        if (at < 0) throw new Error(`no ${needle}`);
        area.focus();
        area.setSelectionRange(at, at + needle.length);
        area.dispatchEvent(new Event('select', { bubbles: true }));
      },
      [needle, nth],
    );
  }

  const host = (page) => page.evaluate(() => window.editor.value);

  test.skip(!built, 'run `npm run build` in js/ first');

  test('an XML child removed and added back is a child, its attribute kept', async ({ page }) => {
    const value = '<r a="1">\n  <a>2</a> <!-- c -->\n  <z>0</z>\n</r>\n';
    const errors = await open(page, { language: 'xml', value, editAs: 'json' });
    await expect(input(page)).toHaveValue(/"@a": 1/);
    await select(page, ',\n    "a": 2');
    await page.keyboard.press('Delete');
    await expect.poll(() => host(page)).toBe('<r a="1">\n  <z>0</z>\n</r>\n');
    await select(page, '"z": 0');
    await page.keyboard.insertText('"z": 0,\n    "a": 5');
    await expect.poll(() => host(page)).toBe('<r a="1">\n  <z>0</z>\n  <a>5</a>\n</r>\n');
    expect(errors).toEqual([]);
  });

  // The override goes above the merge line's comment, which stays on it
  // (round 26).
  test('a YAML merge stays when a merged value is edited from another view', async ({ page }) => {
    const value = 'defaults: &D\n  a: 1 # da\n  b: 2\nsvc:\n  # merged\n  <<: *D\n  b: 20 # own\n';
    const errors = await open(page, { language: 'yaml', value, editAs: 'json' });
    await select(page, '"a": 1', 1);
    await page.keyboard.insertText('"a": 9');
    await expect
      .poll(() => host(page))
      .toBe('defaults: &D\n  a: 1 # da\n  b: 2\nsvc:\n  a: 9\n  # merged\n  <<: *D\n  b: 20 # own\n');
    expect(errors).toEqual([]);
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  // QA round 29 (editing). The host's byte order mark survives every edit of
  // the first line, once, at the start; keys reordered beside members a view
  // shows but the host writes elsewhere (XML attributes, YAML merged keys)
  // reach the host; an HCL expression moved to another block stays an
  // expression; an element copied in an XML view keeps the host's types, and
  // a secret copied so waits. Build the packages first with `npm run build`
  // in js/.
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

  /** Mounts a fresh ConfigEditor with `options` on the vanilla page. */
  async function open(page, options) {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.evaluate(async (options) => {
      const { ConfigEditor } = await import('../config-editor/index.js');
      const parent = document.querySelector('.config-editor').parentElement;
      window.editor.destroy();
      window.saved = null;
      window.problems = [];
      window.editor = new ConfigEditor(parent, {
        ...options,
        onSave: (text) => (window.saved = text),
        onProblems: (problems) => (window.problems = problems),
      });
    }, options);
    await page.locator('.config-editor__input').waitFor();
  }

  const input = (page) => page.locator('.config-editor__input');

  const host = (page) => page.evaluate(() => window.editor.value);

  /** Puts the caret at the start of the `nth` match of `needle` on screen. */
  async function caretAt(page, needle, nth = 0) {
    await input(page).evaluate((element, [needle, nth]) => {
      let at = -1;
      for (let index = 0; index <= nth; index++) at = element.value.indexOf(needle, at + 1);
      if (at < 0) throw new Error(`no ${needle}`);
      element.focus();
      element.setSelectionRange(at, at);
    }, [needle, nth]);
  }

  async function press(page, ...keys) {
    for (const key of keys) await page.keyboard.press(key);
  }

  async function editAs(page, language) {
    await page.evaluate((language) => window.editor.update({ editAs: language }), language);
    await page.waitForTimeout(200);
  }

  /** The host's value once the typing pause is over. */
  async function settled(page) {
    await page.waitForTimeout(700);
    return host(page);
  }


  test.describe('editing', () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');
    const marked = {
      yaml: '# head\nname: a\nport: 1\n',
      json: '{\n  "name": "a",\n  "port": 1\n}\n',
      toml: '# head\nname = "a"\nport = 1\n',
      hcl: '# head\nname = "a"\nport = 1\n',
      xml: '<?xml version="1.0"?>\n<c>\n  <name>a</name>\n</c>\n',
    };
    const firstLineEdits = {
      'Shift+Down Delete': (page) => press(page, 'Shift+ArrowDown', 'Delete'),
      'Ctrl+X': (page) => press(page, 'Control+x'),
      'Ctrl+A and a paste': async (page) => {
        await press(page, 'Control+a');
        await page.keyboard.insertText('x: 1\n');
      },
      'Ctrl+Shift+K': (page) => press(page, 'Control+Shift+K'),
      'typing at the start': (page) => page.keyboard.type('x'),
      'Ctrl+A Tab': (page) => press(page, 'Control+a', 'Tab'),
      'Tab on the line': (page) => press(page, 'Shift+End', 'Tab'),
      'Ctrl+]': (page) => press(page, 'Control+BracketRight'),
      'Alt+Down': (page) => press(page, 'Alt+ArrowDown'),
      'Shift+Alt+Up': (page) => press(page, 'Shift+Alt+ArrowUp'),
    };
    for (const view of ['yaml', 'json', 'toml', 'hcl']) {
      test(`children of an XML element with attributes reordered as ${view} reach the host`, async ({ page }) => {
        const value = '<project xmlns="urn:x" a="1">\n  <groupId>g</groupId>\n  <artifactId>x</artifactId>\n  <version>1</version>\n</project>\n';
        await open(page, { language: 'xml', value });
        await editAs(page, view);
        await caretAt(page, 'groupId');
        await press(page, 'Alt+ArrowDown');
        expect(await settled(page)).toBe(value.replace('  <groupId>g</groupId>\n  <artifactId>x</artifactId>\n', '  <artifactId>x</artifactId>\n  <groupId>g</groupId>\n'));
      });
    }
    for (const view of ['toml', 'hcl', 'xml', 'json']) {
      test(`a YAML map's own keys beside a merge reordered as ${view} reach the host`, async ({ page }) => {
        const value = 'base: &b\n  level: info\nsvc:\n  <<: *b\n  password: p # dev only\n  db: app\n  user: u\n';
        await open(page, { language: 'yaml', value });
        await editAs(page, view);
        await caretAt(page, 'password');
        await press(page, 'Alt+ArrowDown');
        expect(await settled(page)).toBe(value.replace('  password: p # dev only\n  db: app\n', '  db: app\n  password: p # dev only\n'));
      });
    }
    for (const [kind, written] of [
      ['a reference', 'var.z'],
      ['an interpolated string', '"x-${var.y}"'],
      ['a call', 'lower(var.q)'],
    ]) {
      for (const view of ['yaml', 'toml']) {
        test(`${kind} cut and pasted into another block as ${view} stays an expression`, async ({ page }) => {
          const value = `locals {\n  n = 1\n  r = ${written}\n}\n\nother {\n  k = 1\n}\n`;
          await open(page, { language: 'hcl', value });
          await editAs(page, view);
          await caretAt(page, view === 'yaml' ? '  r:' : 'r =');
          await press(page, 'Control+x');
          await caretAt(page, view === 'yaml' ? '  k:' : 'k =');
          await press(page, 'Control+v');
          const saved = await settled(page);
          expect(saved).not.toContain('$${');
          expect(saved).toContain(`other {\n  r = ${written}\n  k = 1\n}`);
        });
      }
    }
    test('a heredoc with an interpolation moved by lines into a nested block as JSON stays a heredoc', async ({ page }) => {
      const value = 'resource "x" "a" {\n  user_data = <<-EOT\n    echo ${var.y}\n  EOT\n  n = 1\n\n  inner {\n    v = 2\n  }\n}\n';
      await open(page, { language: 'hcl', value });
      await editAs(page, 'json');
      await caretAt(page, '"user_data"');
      await press(page, 'Alt+ArrowDown', 'Alt+ArrowDown');
      const shown = await input(page).inputValue();
      expect(shown.indexOf('"user_data"')).toBeGreaterThan(shown.indexOf('"inner"'));
      const saved = await settled(page);
      expect(saved).not.toContain('$${');
      expect(saved).toMatch(/inner \{\n[^}]*user_data = <<-EOT\n\s*echo \$\{var\.y\}\n\s*EOT/);
    });
    for (const language of ['yaml', 'toml', 'hcl']) {
      test(`a ${language} string element copied in an XML view stays a list of strings`, async ({ page }) => {
        const value = language === 'yaml' ? 'deps:\n  anyhow: "1"\n' : '[deps]\nanyhow = "1"\n';
        const text = language === 'hcl' ? 'deps {\n  anyhow = "1"\n}\n' : value;
        await open(page, { language, value: text });
        await editAs(page, 'xml');
        await caretAt(page, '<anyhow>');
        await press(page, 'Shift+Alt+ArrowDown');
        const saved = await settled(page);
        expect(saved).toContain('"1", "1"');
      });
    }
  });
});
