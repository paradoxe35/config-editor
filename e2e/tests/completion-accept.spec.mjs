import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Accepting suggestions with real keys in every language: what is inserted, renames, commas, end tags,
// Tab through inserted values, and what is offered where.

// Pastes `marked` and puts the caret at `|`.
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

// Regressions found by the first hands-on QA round, replayed with real keys.
test.describe('json: suggestions', () => {
  setupEditor(test, '?language=json');

  test('Enter on an inserted default keeps it and starts a new line', async ({ page }) => {
    const e = await at(page, '{\n  "tcp": {\n    |\n  }\n}');
    await page.keyboard.type('keep');
    await expect.poll(() => e.keys()).toContain('keepalive_secs');
    await page.keyboard.press('Enter');
    expect(await e.isOpen()).toBe(false);
    await page.keyboard.press('Enter');
    const text = await e.value();
    expect(text).toContain('"keepalive_secs": 30\n');
    expect(text).not.toContain('3030');
  });

  test('Tab on an inserted default steps past it', async ({ page }) => {
    const e = await at(page, '{\n  "tcp": {\n    |\n  }\n}');
    await page.keyboard.type('keep');
    await expect.poll(() => e.keys()).toContain('keepalive_secs');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Tab');
    expect(await e.value()).toContain('"keepalive_secs": 30\n');
  });

  test('Enter after typing a whole enum value inside quotes does not split it', async ({ page }) => {
    const e = await at(page, '{\n  "services": {\n    "ssh": {\n      "target": 22,\n      |\n    }\n  }\n}');
    await page.keyboard.type('mode');
    await expect.poll(() => e.keys()).toContain('mode');
    await page.keyboard.press('Enter');
    await page.keyboard.type('balance');
    await page.keyboard.press('Enter');
    expect(await e.value()).toContain('"mode": "balance"\n');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });

  test('a new key after a value without a comma puts the comma after the value', async ({ page }) => {
    const e = await at(page, '{\n  "listen": 7835|\n}');
    await page.keyboard.press('Enter');
    await page.keyboard.type('tcp');
    await expect.poll(() => e.keys()).toContain('tcp');
    await page.keyboard.press('Enter');
    expect(await e.value()).toContain('"listen": 7835,\n  "tcp": {');
  });
});

test.describe('yaml: typing your own value', () => {
  setupEditor(test, '?language=yaml');

  test('Enter keeps a typed value instead of swapping in an example', async ({ page }) => {
    const e = await at(page, '|');
    await page.keyboard.type('public_address: box');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('public_address: box\n');
  });
});

test.describe('toml: a table before another table', () => {
  setupEditor(test, '?language=toml');

  test('accepting a table then a key keeps them on separate lines', async ({ page }) => {
    const e = await at(page, 'listen = 1\n|\n[tcp]\nnodelay = true\n');
    await page.keyboard.type('pool');
    await expect.poll(() => e.keys()).toContain('pool');
    await page.keyboard.press('Enter');
    await expect.poll(() => e.keys()).toContain('max_idle');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Tab');
    expect(await e.value()).toContain('[pool]\nmax_idle = 64');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });
});

test.describe('xml: child names typed inside an element', () => {
  setupEditor(test, '?language=xml');

  test('typing in an empty object element filters its children', async ({ page }) => {
    const e = await at(page, '|');
    await page.keyboard.type('<web');
    await expect.poll(() => e.keys()).toContain('web');
    await page.keyboard.press('Enter');
    await page.keyboard.type('certif');
    await expect.poll(() => e.keys()).toContain('certificates');
  });
});

// Regressions found by the second hands-on QA round, replayed with real keys.
test.describe('json: editing', () => {
  setupEditor(test, '?language=json');
  test('nothing is suggested after the root object closes', async ({ page }) => {
    const e = await at(page, '{\n  "listen": 1\n}|');
    await page.keyboard.press('Control+Space');
    await page.waitForTimeout(200);
    expect(await e.isOpen()).toBe(false);
  });
  test('Enter after a value typed in suggested quotes steps past them', async ({ page }) => {
    const e = await at(page, '{\n  "self": {\n    |\n  }\n}');
    await page.keyboard.type('"al');
    await expect.poll(() => e.keys()).toContain('allow');
    await page.keyboard.press('Enter');
    await page.keyboard.type('10.0.0.0/8');
    await page.keyboard.press('Enter');
    expect(await e.value()).toContain('"allow": ["10.0.0.0/8"]\n');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });
  test('completing a key that has a value keeps the value', async ({ page }) => {
    const e = await at(page, '{\n  "publ|": "my.host"\n}');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toContain('public_address');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('{\n  "public_address": "my.host"\n}');
  });
});

test.describe('toml: headers', () => {
  setupEditor(test, '?language=toml');

  test('a table header completes its segment with tables', async ({ page }) => {
    const e = await at(page, 'listen = 1\n|');
    await page.keyboard.type('[hea');
    await expect.poll(() => e.keys()).toEqual(['heartbeat']);
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('listen = 1\n[heartbeat]');
  });
});

test.describe('hcl: end of file', () => {
  setupEditor(test, '?language=hcl');

  test('a new line after the last attribute suggests keys', async ({ page }) => {
    const e = await at(page, 'listen = 7835\n|');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toContain('limits');
    expect(await e.keys()).not.toContain('7835');
  });
});

test.describe('xml: values and end tags', () => {
  setupEditor(test, '?language=xml');

  test('after an end tag the parent is completed', async ({ page }) => {
    const e = await at(page, '<pool>\n  <min_idle>4</min_idle>\n</pool>|');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toContain('limits');
    expect(await e.keys()).not.toContain('max_idle');
  });

  test('Enter after a typed value steps past the end tag', async ({ page }) => {
    // The end tag must be the editor's (auto-closed); a pasted one splits.
    const e = await at(page, '<services>\n  <web>\n    <target>80</target>\n    |\n  </web>\n</services>');
    await page.keyboard.type('<mode');
    await page.keyboard.press('Escape');
    await page.keyboard.type('>balance');
    await page.keyboard.press('Enter');
    expect(await e.value()).toContain('<mode>balance</mode>\n    \n');
  });
});

// Features deferred from the second QA round, replayed with real keys.
test.describe('yaml: Tab through an accepted block', () => {
  setupEditor(test, '?language=yaml');

  test('Tab moves from one required value to the next', async ({ page }) => {
    const e = await at(page, 'web:\n  letsencrypt:\n    email: a@b.c\n    dns:\n      - domains: [a.example]\n        |\n');
    await page.keyboard.type('rfc');
    await expect.poll(() => e.keys()).toContain('rfc2136');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Escape');
    await page.keyboard.type('ns1.example');
    await page.keyboard.press('Tab');
    await page.keyboard.type('key1');
    const text = await e.value();
    expect(text).toContain('server: ns1.example');
    expect(text).toMatch(/: key1/);
    expect(text).not.toContain('ns1.example  ');
  });
});

test.describe('toml: one-line objects', () => {
  setupEditor(test, '?language=toml');

  test('a key after a value without a comma gets the comma', async ({ page }) => {
    const e = await at(page, 'pool = { min_idle = 4 | }\n');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toContain('max_idle');
    await e.clickKey('max_idle');
    expect(await e.value()).toMatch(/^pool = \{ min_idle = 4, max_idle = /);
  });
});

test.describe('hcl: a space after a value', () => {
  setupEditor(test, '?language=hcl');

  test('ends the value, so Enter starts a new line', async ({ page }) => {
    const e = await at(page, '|');
    await page.keyboard.type('insecure_plaintext = false ');
    await page.keyboard.press('Enter');
    expect(await e.value()).not.toContain('false true');
    expect((await e.caret()).line).toBe(2);
  });

  test('completing a block name keeps its body', async ({ page }) => {
    const e = await at(page, 'pol| {\n  max_idle = 1\n}\n');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toContain('pool');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('pool {\n  max_idle = 1\n}\n');
  });
});

test.describe('yaml: whole lines and selections', () => {
  setupEditor(test, '?language=yaml');
  test('an indentless list completes its items', async ({ page }) => {
    const e = await at(page, 'web:\n  certificates:\n  - |');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toContain('cert');
  });
});

test.describe('toml: headers by bracket kind', () => {
  setupEditor(test, '?language=toml');

  test('[[ ]] offers arrays of tables and the list closes after accepting', async ({ page }) => {
    const e = await at(page, '|');
    await page.keyboard.type('[[web.');
    await expect.poll(() => e.keys()).toEqual(['certificates']);
    await page.keyboard.press('Enter');
    await expect.poll(() => e.isOpen()).toBe(false);
    expect(await e.value()).toBe('[[web.certificates]]');
  });
});

// Leftovers from earlier rounds and fourth-round findings, replayed with real keys.
// --- Fourth-round findings: completion ---

test.describe('json: Enter after a value typed in suggested quotes, before a comma', () => {
  setupEditor(test, '?language=json');

  test('takes the comma along to the end of the line', async ({ page }) => {
    const e = await at(page, '{\n  |\n  "listen": 1\n}');
    await page.keyboard.type('"pub');
    await expect.poll(() => e.keys()).toContain('public_address');
    await page.keyboard.press('Enter');
    await page.keyboard.type('foo');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('{\n  "public_address": "foo",\n  \n  "listen": 1\n}');
  });
});

test.describe('xml: Enter after a value accepted from the list', () => {
  setupEditor(test, '?language=xml');

  test('steps past the end tag the editor wrote', async ({ page }) => {
    const e = await at(page, '<listen>1</listen>\n|');
    await page.keyboard.type('<insec');
    await expect.poll(() => e.keys()).toContain('insecure_plaintext');
    await page.keyboard.press('Enter');
    await page.keyboard.type('t');
    await expect.poll(() => e.keys()).toContain('true');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Enter');
    expect(await e.value()).toContain('<insecure_plaintext>true</insecure_plaintext>\n');
  });
});

test.describe('Tab walks the empty values of a one-line insert', () => {
  test.describe('toml', () => {
    setupEditor(test, '?language=toml');

    test('inline table', async ({ page }) => {
      const e = await at(page, '[[web.letsencrypt.dns]]\n|');
      await page.keyboard.type('rfc');
      await expect.poll(() => e.keys()).toContain('rfc2136');
      await page.keyboard.press('Enter');
      await page.keyboard.type('a');
      await page.keyboard.press('Tab');
      await page.keyboard.type('b');
      expect(await e.value()).toContain('server = "a", key_name = "b"');
    });
  });
});

test.describe('renaming a key with Enter closes the list', () => {
  setupEditor(test, '?language=yaml');

  test('the next Enter does not split the key', async ({ page }) => {
    const e = await at(page, 'web:\n  htt|: 80\n');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toContain('https');
    const keys = await e.keys();
    for (let index = 0; index < keys.indexOf('https'); index += 1) await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('web:\n  https: 80\n');
    await expect.poll(() => e.isOpen()).toBe(false);
  });
});

test.describe('toml: a header with a typed prefix', () => {
  setupEditor(test, '?language=toml');

  test('leaves out tables that already exist', async ({ page }) => {
    const e = await at(page, '[web]\nhttp = 1\n\n[w|]');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toContain('websocket');
    expect(await e.keys()).not.toContain('web');
  });
});

test.describe('yaml: Tab walks the empty values of a flow map', () => {
  setupEditor(test, '?language=yaml');

  test('accepted into a one-line map', async ({ page }) => {
    const e = await at(page, 'websocket: {listen: 1, |}\n');
    await page.keyboard.type('tls');
    await expect.poll(() => e.keys()).toContain('tls');
    await page.keyboard.press('Enter');
    await page.keyboard.type('a');
    await page.keyboard.press('Tab');
    await page.keyboard.type('b');
    expect(await e.value()).toMatch(/cert: a, key: b/);
  });
});

// Fifth-round findings, replayed with real keys.
test.describe('Tab at the end of the last empty value steps out of it', () => {
  test.describe('toml', () => {
    setupEditor(test, '?language=toml');

    test('after the last string of an accepted table', async ({ page }) => {
      const e = await at(page, '[websocket]\nlisten = 1\n|');
      await page.keyboard.type('tls');
      await expect.poll(() => e.keys()).toContain('tls');
      await page.keyboard.press('Enter');
      await page.keyboard.type('a.pem');
      await page.keyboard.press('Tab');
      await page.keyboard.type('k.pem');
      await page.keyboard.press('Tab');
      const text = await e.value();
      expect(text).toContain('"k.pem"');
      expect(text).not.toContain('k.pem ');
    });
  });

  test.describe('xml', () => {
    setupEditor(test, '?language=xml');

    test('after the text of an element the editor closed', async ({ page }) => {
      const e = await at(page, '|');
      await page.keyboard.type('<listen>7836');
      await page.keyboard.press('Tab');
      const text = await e.value();
      expect(text).toBe('<listen>7836</listen>');
      expect((await e.caret()).column).toBe('<listen>7836</listen>'.length + 1);
    });
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  // Sixth-round findings, replayed with real keys.
  async function select(page, text, from, to) {
    const e = editor(page);
    await e.paste(text);
    await e.input.evaluate(
      (element, [start, end]) => {
        element.focus();
        element.setSelectionRange(start, end);
      },
      [from, to],
    );
    return e;
  }

  async function at(page, marked) {
    const position = marked.indexOf('¦');
    return select(page, marked.replace('¦', ''), position, position);
  }


  test.describe('completion', () => {
    test.describe('yaml', () => {
      setupEditor(test, '?language=yaml');

    test('ArrowDown after typing moves the caret and keeps the list closed', async ({ page }) => {
        const e = await at(page, 'tcp:\n  nodelay: ¦\n  keepalive_secs: 3\n');
      await page.keyboard.type('yes');
        await page.keyboard.press('ArrowDown');
      await page.waitForTimeout(200);
        expect(await e.isOpen()).toBe(false);
        expect((await e.caret()).line).toBe(3);
      });

      test('an item under a mouse that has not moved is not preselected', async ({ page }) => {
        const e = await at(page, 'public_address: ¦\n');
      const box = await e.input.boundingBox();
        // Where the list opens, below the first line.
        await page.mouse.move(box.x + 140, box.y + 110);
        await page.keyboard.press('Control+Space');
      await expect.poll(() => e.isOpen()).toBe(true);
        await page.waitForTimeout(200);
        const active = await page.locator('.config-editor__completion').evaluateAll((items) =>
          items.findIndex((item) => item.classList.contains('config-editor__completion-active')),
      );
        expect(active).toBe(0);
      });
    });

    test.describe('toml', () => {
      setupEditor(test, '?language=toml');

    test('Enter on a typed key that matches its only suggestion keeps the value', async ({ page }) => {
        const e = await at(page, '[tcp]\nnodela¦ = true\n');
      await page.keyboard.type('y');
        await expect.poll(() => e.keys()).toEqual(['nodelay']);
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe('[tcp]\nnodelay = true\n');
    });
    });

    test.describe('json', () => {
      setupEditor(test, '?language=json');

    test('typing over the closing quote closes the value list', async ({ page }) => {
        const e = await at(page, '{\n  ¦\n}');
      await page.keyboard.type('"public_a');
      await expect.poll(() => e.keys()).toContain('public_address');
      await page.keyboard.press('Enter');
      await page.keyboard.type('"');
        await page.waitForTimeout(150);
        expect(await e.isOpen()).toBe(false);
      });
    });

    test.describe('xml', () => {
      setupEditor(test, '?language=xml');

    test('a key typed in front of an attribute is inserted before it', async ({ page }) => {
        const e = await at(page, '<tcp ¦nodelay="true"/>');
      await page.keyboard.type('keep');
        await expect.poll(() => e.keys()).toContain('keepalive_secs');
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe('<tcp keepalive_secs="30" nodelay="true"/>');
    });
    });

    test.describe('hcl', () => {
      setupEditor(test, '?language=hcl');

    test('a block label gets no key suggestions', async ({ page }) => {
        const e = await at(page, 'services "¦" {\n}\n');
      await page.keyboard.press('Control+Space');
      await page.waitForTimeout(200);
        expect(await e.isOpen()).toBe(false);
      });
    });
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  // Seventh-round findings, replayed with real keys. The language-wide classes
  // behind them are covered by the unit matrices (`completion_matrix`,
  // `editing_matrix`, `save_properties`).
  async function at(page, marked) {
    const e = editor(page);
    const [start, end] = [marked.indexOf('¦'), marked.lastIndexOf('¦') - (marked.indexOf('¦') === marked.lastIndexOf('¦') ? 0 : 1)];
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


  test.describe('xml', () => {
    setupEditor(test, '?language=xml');
    test('an element name accepted in front of a tag keeps that element', async ({ page }) => {
      const e = await at(page, '<burrow config-root="true">\n  <¦tcp/>\n</burrow>');
      await page.keyboard.type('pu');
      await expect.poll(() => e.keys()).toContain('public_address');
      await page.keyboard.press('Enter');
      const value = await e.value();
      expect(value).toContain('<tcp/>');
      expect(value).toContain('<public_address>');
    });
  });

  test.describe('hcl', () => {
    setupEditor(test, '?language=hcl');

    test('a bare block label gets no suggestions', async ({ page }) => {
      const e = await at(page, 'services ¦');
      await page.keyboard.type('we');
      await page.waitForTimeout(200);
      expect(await e.isOpen()).toBe(false);
    });
  });

  test.describe('toml', () => {
    setupEditor(test, '?language=toml');

    test('Enter after Ctrl+Space on the value already written only closes the list', async ({ page }) => {
      const e = await at(page, '[services.s]\nmode = "ba¦lance"\n');
      await page.keyboard.press('Control+Space');
      await expect.poll(() => e.keys()).toEqual(['balance']);
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe('[services.s]\nmode = "balance"\n');
      expect(await e.isOpen()).toBe(false);
    });
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  // Eighth-round findings, replayed with real keys. The classes behind them are
  // covered by the unit matrices (`completion_matrix`, `editing_matrix`,
  // `save_properties`).
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


  test.describe('xml', () => {
    setupEditor(test, '?language=xml');
    test('a name accepted inside an element with children renames both tags', async ({ page }) => {
      const e = await at(page, '<burrow config-root="true">\n  <t¦cp>\n    <nodelay>true</nodelay>\n  </tcp>\n</burrow>');
      await page.keyboard.press('Control+Space');
      await expect.poll(() => e.keys()).toContain('metrics');
      const keys = await e.keys();
      for (let index = 0; index < keys.indexOf('metrics'); index += 1) await page.keyboard.press('ArrowDown');
      await page.keyboard.press('Enter');
      const value = await e.value();
      expect(value).toContain('<metrics>');
      expect(value).toContain('</metrics>');
      expect(value).not.toContain('tcp');
    });
  });

  test.describe('hcl', () => {
    setupEditor(test, '?language=hcl');

    test('a key typed in front of a block goes before it, the block kept', async ({ page }) => {
      const e = await at(page, '¦tcp {\n  nodelay = true\n}\n');
      await page.keyboard.type('hea');
      await expect.poll(() => e.keys()).toContain('heartbeat');
      await page.keyboard.press('Enter');
      const value = await e.value();
      expect(value).toContain('heartbeat {');
      expect(value).toContain('tcp {\n  nodelay = true\n}');
    });
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  // Ninth-round findings, replayed with real keys. The classes behind them are
  // covered by the unit matrices (`completion_matrix`, `editing_matrix`).
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

  const selection = (e) => e.input.evaluate((element) => [element.selectionStart, element.selectionEnd]);


  test.describe('yaml', () => {
    setupEditor(test, '?language=yaml');

    test('a rename leaves the caret after the new key, the next key typed after it', async ({ page }) => {
      const e = await at(page, 'limits:\n  max_h¦pending: 1\n');
      await page.keyboard.press('Control+Space');
      await expect.poll(() => e.keys()).toContain('max_handshakes');
      const keys = await e.keys();
      for (let index = 0; index < keys.indexOf('max_handshakes'); index += 1) await page.keyboard.press('ArrowDown');
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe('limits:\n  max_handshakes: 1\n');
      const [start, end] = await selection(e);
      expect(start).toBe(end);
      expect(await e.isOpen()).toBe(false);
    });
  });

  test.describe('xml', () => {
    setupEditor(test, '?language=xml');
    test('an attribute accepted right before > keeps the tag whole', async ({ page }) => {
      const e = await at(page, '<tcp ¦>\n</tcp>\n');
      await page.keyboard.type('no');
      await expect.poll(() => e.keys()).toContain('nodelay');
      await page.keyboard.press('Enter');
      expect(await e.value()).toMatch(/^<tcp nodelay="[^"]*">\n<\/tcp>\n$/);
    });
  });

  test.describe('toml', () => {
    setupEditor(test, '?language=toml');

    test('accepting the key already written before a dot changes nothing', async ({ page }) => {
      const e = await at(page, 'tcp¦.nodelay = true\n');
      await page.keyboard.press('Control+Space');
      await expect.poll(() => e.keys()).toContain('tcp');
      const keys = await e.keys();
      for (let index = 0; index < keys.indexOf('tcp'); index += 1) await page.keyboard.press('ArrowDown');
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe('tcp.nodelay = true\n');
    });
  });

  test.describe('json', () => {
    setupEditor(test, '?language=json');

    test('a key typed at another key’s opening quote goes before it on its own line', async ({ page }) => {
      const e = await at(page, '{\n  "¦tcp": {}\n}\n');
      await page.keyboard.press('Control+Space');
      await expect.poll(() => e.keys()).toContain('clients');
      expect(await e.keys()).not.toContain('tcp');
      const keys = await e.keys();
      for (let index = 0; index < keys.indexOf('clients'); index += 1) await page.keyboard.press('ArrowDown');
      await page.keyboard.press('Enter');
      const value = await e.value();
      expect(value).toContain('"clients": {');
      expect(value).toContain(',\n  "tcp": {}');
    });
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  // Tenth-round findings, replayed with real keys. The classes behind them are
  // covered by the unit matrices (`completion_matrix`, `editing_matrix`).
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

  async function accept(page, e, label) {
    await expect.poll(() => e.keys()).toContain(label);
    const keys = await e.keys();
    for (let index = 0; index < keys.indexOf(label); index += 1) await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
  }


  test.describe('json', () => {
    setupEditor(test, '?language=json');

    test('a key typed after another key’s opening quote goes before it', async ({ page }) => {
      const e = await at(page, '{"listen": 7835, "¦tcp": {"nodelay": true}}');
      await page.keyboard.type('ud');
      await accept(page, e, 'udp');
      const value = JSON.parse(await e.value());
      expect(Object.keys(value)).toEqual(['listen', 'udp', 'tcp']);
      expect(value.tcp).toEqual({ nodelay: true });
    });
  });

  test.describe('toml', () => {
    setupEditor(test, '?language=toml');

    test('a table typed in front of a header goes before it', async ({ page }) => {
      const e = await at(page, 'listen = 7835\n[¦tcp]\nnodelay = true\n');
      await page.keyboard.type('ud');
      await accept(page, e, 'udp');
      const value = await e.value();
      expect(value).toContain('[udp]');
      expect(value).toContain('[tcp]\nnodelay = true');
    });

    test('a table renamed by a completion takes its sub-tables along', async ({ page }) => {
      const e = await at(page, '[t¦cp]\nnodelay = true\n[tcp.x]\na = 1\n');
      await page.keyboard.press('Control+Space');
      await accept(page, e, 'metrics');
      expect(await e.value()).toBe('[metrics]\nnodelay = true\n[metrics.x]\na = 1\n');
    });
  });

  test.describe('yaml', () => {
    setupEditor(test, '?language=yaml');
    test('a suggestion accepted in the indentation lines up with the line', async ({ page }) => {
      const e = await at(page, 'tcp:\n¦  nodelay: true\n');
      await page.keyboard.press('Control+Space');
      await accept(page, e, 'keepalive_secs');
      const value = await e.value();
      expect(value).toMatch(/^tcp:\n  keepalive_secs: \d+\n  nodelay: true\n$/);
    });
  });
});
