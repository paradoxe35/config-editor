import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Suggestion correctness in every language: keys already set in a container
// are not offered again (also while the document is mid-edit), sibling
// containers keep their own keys, XML arrays repeat, and an object typed
// directly in an inline array gets its braces.

// Pastes `marked`, puts the caret at `|`, types `text`, and returns the keys.
async function keysAfterTyping(page, marked, text) {
  const e = editor(page);
  const at = marked.indexOf('|');
  await e.paste(marked.replace('|', ''));
  await e.input.evaluate((element, position) => {
    element.focus();
    element.setSelectionRange(position, position);
  }, at);
  await page.keyboard.type(text);
  await page.keyboard.press('Control+Space');
  await expect.poll(async () => (await e.keys()).length).toBeGreaterThan(0);
  return e.keys();
}

const DUPLICATES = [
  {
    language: 'toml',
    // The reported case: the document does not parse while the key is typed.
    marked: '[limits]\nhandshake_timeout_secs = 5\nmax_handshakes = 2048\n|',
    typed: 'max',
    taken: 'max_handshakes',
    free: 'max_pending',
  },
  {
    language: 'toml',
    marked: '[clients]\nacme = { secret = "x", | }\n',
    typed: '',
    taken: 'secret',
    free: 'max_connections',
  },
  {
    language: 'yaml',
    marked: 'limits:\n  handshake_timeout_secs: 5\n  max_handshakes: 2048\n  |',
    typed: 'max',
    taken: 'max_handshakes',
    free: 'max_pending',
  },
  {
    language: 'yaml',
    marked: 'clients:\n  acme: { secret: x, | }\n',
    typed: '',
    taken: 'secret',
    free: 'max_connections',
  },
  {
    language: 'json',
    marked: '{\n  "limits": {\n    "max_handshakes": 2048,\n    |\n  }\n}',
    typed: '"max',
    taken: 'max_handshakes',
    free: 'max_pending',
  },
  {
    language: 'hcl',
    marked: 'limits {\n  max_handshakes = 2048\n  |\n}\n',
    typed: 'max',
    taken: 'max_handshakes',
    free: 'max_pending',
  },
  {
    language: 'hcl',
    marked: 'clients = { acme = { secret = "x", | } }\n',
    typed: '',
    taken: 'secret',
    free: 'max_connections',
  },
  {
    language: 'xml',
    marked: '<limits>\n  <max_handshakes>2048</max_handshakes>\n  |\n</limits>\n',
    typed: '<max',
    taken: 'max_handshakes',
    free: 'max_pending',
  },
];

for (const { language, marked, typed, taken, free } of DUPLICATES) {
  test.describe(`${language}: no duplicate suggestions`, () => {
    setupEditor(test, `?language=${language}`);

    test(`${taken} is not offered again in ${JSON.stringify(marked.split('\n')[0])}`, async ({ page }) => {
      const keys = await keysAfterTyping(page, marked, typed);
      expect(keys).not.toContain(taken);
      expect(keys).toContain(free);
    });
  });
}

test.describe('sibling containers keep their own keys', () => {
  setupEditor(test, '?language=toml');

  test('a key set in one client is still offered in the next', async ({ page }) => {
    const keys = await keysAfterTyping(page, '[clients.a]\nsecret = "x"\n[clients.b]\n|', 'sec');
    expect(keys).toContain('secret');
  });
});

test.describe('XML arrays repeat', () => {
  setupEditor(test, '?language=xml');

  test('an array element is offered again, a scalar element is not', async ({ page }) => {
    const keys = await keysAfterTyping(
      page,
      '<services>\n  <web>\n    <target>80</target>\n    <visitors>alice</visitors>\n    |\n  </web>\n</services>\n',
      '<',
    );
    expect(keys).toContain('visitors');
    expect(keys).not.toContain('target');
  });
});

test.describe('TOML arrays of tables', () => {
  setupEditor(test, '?language=toml');

  test('accepting certificates under [web] opens [[web.certificates]] with its hosts', async ({ page }) => {
    const e = editor(page);
    await e.paste('[web]\n');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(element.value.length, element.value.length);
    });
    await page.keyboard.type('certif');
    await expect.poll(() => e.keys()).toContain('certificates');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('[web]\n[[web.certificates]]\nhosts = []\n');
    await page.keyboard.type('"a.example"');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });

  test('a key typed directly in an inline array starts a new table item', async ({ page }) => {
    const e = editor(page);
    await e.paste('[web]\ncertificates = []\n');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('[]') + 1;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('hos');
    await expect.poll(() => e.keys()).toContain('hosts');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('[web]\ncertificates = [{ hosts = [] }]\n');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });
});

for (const [language, document, typed, expected] of [
  ['json', '{\n  "web": {\n    "certificates": []\n  }\n}', '"hos', '"certificates": [{"hosts": []}]'],
  ['yaml', 'web: { certificates: [] }\n', 'hos', 'web: { certificates: [{ hosts: [] }] }'],
  ['hcl', 'web = { certificates = [] }\n', 'hos', 'web = { certificates = [{ hosts = [] }] }'],
]) {
  test.describe(`${language}: object items in inline arrays`, () => {
    setupEditor(test, `?language=${language}`);

    test('a key typed directly in the array starts a new object item', async ({ page }) => {
      const e = editor(page);
      await e.paste(document);
      await e.input.evaluate((element) => {
        const at = element.value.indexOf('[]') + 1;
        element.focus();
        element.setSelectionRange(at, at);
      });
      await page.keyboard.type(typed);
      await expect.poll(() => e.keys()).toContain('hosts');
      await page.keyboard.press('Enter');
      await expect.poll(() => e.value()).toContain(expected);
    });
  });
}

// Accepting an object key inside a multi-line array, then filling the caret,
// must leave a valid document with every other required key placeholdered.
for (const [language, document, typed, fillWith] of [
  [
    'json',
    '{\n  "web": {\n    "letsencrypt": {\n      "email": "ops@example.com",\n      "dns": [\n        |\n      ]\n    }\n  }\n}\n',
    '"acme',
    '"https://auth.example"',
  ],
  ['yaml', 'web:\n  letsencrypt:\n    email: ops@example.com\n    dns: [\n      |\n    ]\n', 'acme', 'https://auth.example'],
  [
    'toml',
    '[web.letsencrypt]\nemail = "ops@example.com"\ndns = [\n  |\n]\n',
    'acme',
    '"https://auth.example"',
  ],
]) {
  test.describe(`${language}: object keys in a multi-line array`, () => {
    setupEditor(test, `?language=${language}`);

    test('acme_dns starts a valid item with its required keys placeholdered', async ({ page }) => {
      const e = editor(page);
      const at = document.indexOf('|');
      await e.paste(document.replace('|', ''));
      await e.input.evaluate((element, position) => {
        element.focus();
        element.setSelectionRange(position, position);
      }, at);
      await page.keyboard.type(typed);
      await expect.poll(() => e.keys()).toContain('acme_dns');
      await page.keyboard.press('Enter');
      await page.keyboard.press('Escape');
      await page.keyboard.type(fillWith);
      const text = await e.value();
      for (const key of ['api', 'username', 'password', 'subdomain']) expect(text).toContain(key);
      await expect.poll(() => e.diagnosticCount()).toBe(0);
    });
  });
}

// Puts the caret at `|` in a pasted document.
async function pasteAt(page, marked) {
  const e = editor(page);
  const at = marked.indexOf('|');
  await e.paste(marked.replace('|', ''));
  await e.input.evaluate((element, position) => {
    element.focus();
    element.setSelectionRange(position, position);
  }, at);
  return e;
}

for (const [language, marked, typed, expected] of [
  ['yaml', 'limits:\n  |', 'max_p', 'max_pending: 900'],
  ['toml', '[limits]\n|', 'max_p', 'max_pending = 900'],
  ['hcl', 'limits {\n  |\n}\n', 'max_p', 'max_pending = 900'],
  ['xml', '<limits>\n  |\n</limits>\n', '<max_p', '<max_pending>900</max_pending>'],
]) {
  test.describe(`${language}: inserted defaults`, () => {
    setupEditor(test, `?language=${language}`);

    test('typing right after accepting replaces the default', async ({ page }) => {
      const e = await pasteAt(page, marked);
      await page.keyboard.type(typed);
      await expect.poll(() => e.keys()).toContain('max_pending');
      await page.keyboard.press('Enter');
      await page.keyboard.type('900');
      await expect.poll(() => e.value()).toContain(expected);
      expect(await e.value()).not.toContain('512');
    });
  });
}

for (const [language, marked, typed, after] of [
  ['yaml', 'listen: |\n', '7835', 'listen: 7835\n\n'],
  ['toml', 'listen = |\n', '7835', 'listen = 7835\n\n'],
]) {
  test.describe(`${language}: Enter after a complete value`, () => {
    setupEditor(test, `?language=${language}`);

    test('starts a new line instead of re-accepting the same value', async ({ page }) => {
      const e = await pasteAt(page, marked);
      await page.keyboard.press('Control+Space');
      await page.keyboard.type(typed);
      await page.keyboard.press('Enter');
      await expect.poll(() => e.value()).toBe(after);
      expect((await e.caret()).line).toBe(2);
    });

    test('a number being typed is not completed to a longer suggestion', async ({ page }) => {
      const e = await pasteAt(page, marked);
      await page.keyboard.type('78');
      await page.keyboard.press('Enter');
      await expect.poll(() => e.value()).toBe(after.replace('7835', '78'));
    });
  });
}

test.describe('xml: list keys', () => {
  setupEditor(test, '?language=xml');

  test('accepting dns writes one valid item and offers its keys', async ({ page }) => {
    const e = await pasteAt(page, '<web>\n  <letsencrypt>\n    <email>a@b.c</email>\n    |\n  </letsencrypt>\n</web>\n');
    await page.keyboard.type('<dn');
    await expect.poll(() => e.keys()).toContain('dns');
    await page.keyboard.press('Enter');
    await expect.poll(() => e.value()).toContain('<dns></dns>');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toContain('cloudflare');
    await e.clickKey('cloudflare');
    await page.keyboard.press('Escape');
    await page.keyboard.type('tok');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });
});
