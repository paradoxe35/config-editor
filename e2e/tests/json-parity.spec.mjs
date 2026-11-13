import { expect, test } from '@playwright/test';
import { editor, pointAt, pointerOn, scrollToLine, setupEditor } from './helpers.mjs';

// JSON parity with the YAML suites in editor.spec.mjs and scenarios.spec.mjs:
// every shared behavior (completion, hover, diagnostics, diff, viewport,
// themes) is covered in `?language=json` mode too. JSON-only mechanics
// (commas, quotes, braces) live in json.spec.mjs; YAML-only mechanics
// (colons, dashes, `#` comments) live in the YAML suites.
setupEditor(test, '?language=json');

const LARGE_JSON = `{
  "listen": 7835,
  "public_address": "tunnel.example.com",
  "insecure_plaintext": false,
  "heartbeat": {
    "interval_secs": 15
  },
  "limits": {
    "handshake_timeout_secs": 10,
    "max_handshakes": 64,
    "max_connections_per_client": 32,
    "max_pending": 128
  },
  "pool": {
    "min_idle": 2,
    "max_idle": 8
  },
  "clients": {
    "alex": {
      "secret": "alpha-secret",
      "max_connections": 16
    },
    "laptop": {
      "secret": "beta-secret"
    },
    "nas": {
      "secret": "gamma-secret"
    },
    "ci-runner": {
      "secret": "delta-secret"
    }
  },
  "visitors": {
    "ops": {
      "secret": "ops-secret"
    }
  },
  "services": {
    "dashboard": {
      "target": 8080,
      "protocol": "tcp",
      "mode": "failover"
    },
    "dns": {
      "target": 53,
      "protocol": "udp"
    },
    "backup": {
      "client": "nas",
      "target": 22,
      "protocol": "tcp"
    },
    "billing": {
      "target": 9000,
      "protocol": "tcp",
      "clients": [
        "alex",
        {
          "name": "laptop",
          "weight": 2
        }
      ],
      "mode": "balance"
    }
  },
  "web": {
    "http": 80,
    "https": 443
  },
  "tcp": {
    "nodelay": true
  }
}`;

test.describe('accepting keys and values', () => {
  test('listen: accepting the key opens its value list, and a value closes it', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  ""\n}');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(5, 5);
    });
    await page.keyboard.type('lis');
    expect(await e.keys()).toContain('listen');
    await e.clickKey('listen');
    await expect(e.input).toHaveValue('{\n  "listen": \n}');
    expect(await e.isOpen()).toBe(true);
    await expect.poll(() => e.keys()).toEqual(expect.arrayContaining(['7835', '0.0.0.0:7835']));
    await e.clickKey('7835');
    await expect(e.input).toHaveValue('{\n  "listen": 7835\n}');
    expect(await e.isOpen()).toBe(false);
  });

  test('completing a key before its existing colon does not panic or duplicate the key', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "insecure_plaintext": false\n}');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('"insecure_plain') + '"insecure_pla'.length;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('i');
    await expect.poll(() => e.keys()).toContain('insecure_plaintext');
    await e.clickKey('insecure_plaintext');
    await expect(e.input).toHaveValue('{\n  "insecure_plaintext": false\n}');
  });

  test('keyboard: arrow then Enter accepts the key, then an arrow and Enter accept a value', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  ""\n}');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(5, 5);
    });
    await page.keyboard.type('list');
    await expect.poll(() => e.keys()).toEqual(['listen']);
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('{\n  "listen": \n}');
    expect(await e.isOpen()).toBe(true);
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    expect(await e.isOpen()).toBe(false);
    expect(await e.value()).toContain('"listen":');
  });

  test('Enter with a list opened by typing accepts the highlighted suggestion', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  ""\n}');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(5, 5);
    });
    await page.keyboard.type('lis');
    await expect.poll(() => e.isOpen()).toBe(true);
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('{\n  "listen": \n}');
    expect(await e.isOpen()).toBe(true);
  });

  test('Tab accepts the highlighted suggestion of an auto-triggered list', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  \n}');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(4, 4);
    });
    await page.keyboard.type('"public_add');
    await expect.poll(() => e.keys()).toContain('public_address');
    await page.keyboard.press('Tab');
    await expect(e.input).toHaveValue('{\n  "public_address": ""\n}');
  });

  test('a value list opened after the colon offers values, and Escape closes it', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "insecure_plaintext": \n}');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(element.value.length - 2, element.value.length - 2);
    });
    await page.keyboard.type('t');
    await expect.poll(() => e.keys()).toEqual(['true']);
    await page.keyboard.press('Escape');
    expect(await e.isOpen()).toBe(false);
  });

  test('accepting an object key keeps the child list open and never inserts a null value', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  ""\n}');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(5, 5);
    });
    await page.keyboard.type('web');
    await expect.poll(() => e.keys()).toEqual(['web', 'websocket']);
    await e.clickKey('web');
    expect(await e.value()).not.toContain('null');
    // Like YAML: accepting a key keeps the list open for its child keys.
    await expect.poll(() => e.isOpen()).toBe(true);
    await expect.poll(() => e.keys()).toEqual(expect.arrayContaining(['http', 'https']));
  });

  test('a new indented line does not open suggestions, Ctrl+Space does', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "clients": {\n    "alex": {\n      \n    }\n  }\n}');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(element.value.indexOf('{\n      \n'), 0);
    });
    // Caret on the blank line inside the alex object: nothing opens by itself.
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('"alex": {\n') + '"alex": {\n'.length + 6;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.waitForTimeout(150);
    expect(await e.isOpen()).toBe(false);
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toEqual(expect.arrayContaining(['max_connections', 'secret']));
  });
});

test.describe('focus', () => {
  test('clicking outside the editor closes an open suggestion list', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  ""\n}');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(5, 5);
    });
    await page.keyboard.type('lis');
    await expect.poll(() => e.isOpen()).toBe(true);
    await page.mouse.click(1000, 600);
    await expect.poll(() => e.isOpen(), 'list closes on blur').toBe(false);
  });

  test('Escape dismisses an open diff preview', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "listen": 7836,\n  "public_address": "tunnel.example.com",\n  "insecure_plaintext": false\n}');
    const marker = page.locator('.config-editor__diff-marker-modified').first();
    await expect(marker).toHaveCount(1);
    await marker.click();
    const preview = page.locator('.config-editor__diff-peek');
    await expect(preview).toHaveCount(1);
    await page.keyboard.press('Escape');
    await expect(preview).toHaveCount(0);
  });

  test('a change preview shows before and after and closes from its close button', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "listen": 7836,\n  "public_address": "tunnel.example.com",\n  "insecure_plaintext": false\n}');
    await page.locator('.config-editor__diff-marker-modified').first().click();
    const preview = page.locator('.config-editor__diff-peek');
    await expect(preview).toHaveCount(1);
    await expect(preview).toContainText('7835');
    await expect(preview).toContainText('7836');
    await preview.getByRole('button', { name: 'Close change preview' }).click();
    await expect(preview).toHaveCount(0);
  });

  test('a removed-lines preview shows the deleted text and closes from its close button', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "listen": 7835,\n  "insecure_plaintext": false\n}');
    await page.locator('.config-editor__diff-deleted').first().click();
    const preview = page.locator('.config-editor__diff-peek');
    await expect(preview).toHaveCount(1);
    await expect(preview).toContainText('Removed lines');
    await expect(preview).toContainText('public_address');
    await expect(preview).toContainText('tunnel.example.com');
    expect(await preview.getByRole('heading', { name: 'After' }).count()).toBe(0);
    await preview.getByRole('button', { name: 'Close change preview' }).click();
    await expect(preview).toHaveCount(0);
  });

  test('a change preview survives typing elsewhere but not on its own line', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "listen": 7836,\n  "public_address": "tunnel.example.com",\n  "insecure_plaintext": false\n}');
    await page.locator('.config-editor__diff-marker-modified').first().click();
    const preview = page.locator('.config-editor__diff-peek');
    await expect(preview).toHaveCount(1);
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(element.value.length, element.value.length);
    });
    await page.keyboard.press('Enter');
    await page.keyboard.type('"added_key": 1');
    await expect(preview).toHaveCount(1);
    const at = await pointAt(page, 1, 13);
    await page.mouse.click(at.x, at.y);
    await page.keyboard.type('0');
    await expect(preview).toHaveCount(0);
  });

  test('typing again after Escape reopens suggestions', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  ""\n}');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(5, 5);
    });
    await page.keyboard.type('lis');
    await page.keyboard.press('Escape');
    expect(await e.isOpen()).toBe(false);
    await page.keyboard.type('t');
    await expect.poll(() => e.isOpen()).toBe(true);
  });
});

test.describe('read-only view mode', () => {
  test('typing and shortcuts cannot modify the document, but hover docs still work', async ({ page, baseURL }) => {
    await page.goto(`${baseURL}?language=json&readonly=1`);
    await page.locator('.config-editor__input').waitFor();
    const e = editor(page);
    await expect(e.input).toHaveAttribute('inputmode', 'none');
    await expect(e.input).toHaveAttribute('aria-readonly', 'true');
    const before = await e.value();

    await e.input.click();
    await page.keyboard.type('garbage-text');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Backspace');
    await page.keyboard.press('Delete');
    expect(await e.value()).toBe(before);

    await page.keyboard.press('Control+Space');
    await page.waitForTimeout(150);
    expect(await e.isOpen()).toBe(false);

    const onKey = await pointAt(page, 1, 3);
    await page.mouse.move(onKey.x, onKey.y, { steps: 4 });
    const card = page.locator('.config-editor__hover');
    await expect(card).toHaveCount(1);
    await expect(card.locator('.config-editor__hover-key')).toHaveText('listen');
  });
});

test.describe('highlight', () => {
  test('hover and arrow keys share exactly one highlighted row', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "clients": {\n    "alex": {\n      ""\n    }\n  }\n}');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('""') + 1;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('Control+Space');
    await expect(e.options).toHaveCount(2);
    const second = (await e.keys())[1];
    await e.options.nth(1).hover();
    await expect.poll(() => e.highlighted()).toHaveLength(1);
    expect(await e.highlighted()).toEqual([second]);
    await page.keyboard.press('ArrowDown');
    expect(await e.highlighted()).toHaveLength(1);
  });
});

test.describe('hover documentation', () => {
  test('hovering a key shows its schema documentation, and leaving the key hides it', async ({ page }) => {
    const e = editor(page);
    await e.paste(LARGE_JSON);
    const card = page.locator('.config-editor__hover');

    const onKey = await pointAt(page, 1, 5);
    await page.mouse.move(onKey.x, onKey.y, { steps: 4 });
    await expect(card).toHaveCount(1);
    await expect(card.locator('.config-editor__hover-key')).toHaveText('listen');
    await expect(card).toContainText('0.0.0.0:7835');

    // Off the line's text (its value shows the key's docs too).
    const offKey = await pointAt(page, 1, 45);
    await page.mouse.move(offKey.x, offKey.y, { steps: 3 });
    await expect(card).toHaveCount(0);
  });

  test('the hover card is suppressed while suggestions are open', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "insecure_plaintext": \n}');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(element.value.length - 2, element.value.length - 2);
    });
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.isOpen()).toBe(true);
    const onKey = await pointAt(page, 1, 5);
    await page.mouse.move(onKey.x, onKey.y, { steps: 4 });
    await page.waitForTimeout(500);
    await expect(page.locator('.config-editor__hover')).toHaveCount(0);
  });
});

test.describe('large documents', () => {
  test('a large JSON document loads silently and reports its diagnostics', async ({ page }) => {
    const e = editor(page);
    await e.paste(LARGE_JSON);
    expect(await e.isOpen()).toBe(false);
    // All of it loaded (only the rows in view are rendered): the text holds every line,
    // and the last one renders once scrolled to.
    const lines = LARGE_JSON.split('\n').length;
    await expect.poll(() => e.input.inputValue().then((value) => value.split('\n').length)).toBe(lines);
    await e.input.evaluate((element) => {
      element.scrollTop = element.scrollHeight;
      element.dispatchEvent(new Event('scroll'));
    });
    await expect(page.locator(`.config-editor__gutter-content > div[data-line="${lines}"]`)).toHaveCount(1);
    expect(await e.diagnosticCount(), 'a valid document has no diagnostics').toBe(0);
  });

  test('a long document keeps the suggestion list inside the viewport near the bottom', async ({ page }) => {
    const e = editor(page);
    await e.paste(LARGE_JSON);
    // The last line inside the root object (after its `}` nothing is offered).
    await e.input.evaluate((element) => {
      const at = element.value.lastIndexOf('}');
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('Enter');
    await page.keyboard.type('"');
    await expect.poll(() => e.isOpen()).toBe(true);
    const box = await e.popup.boundingBox();
    const viewport = page.viewportSize();
    expect(box.y).toBeGreaterThanOrEqual(0);
    expect(box.y + box.height).toBeLessThanOrEqual(viewport.height + 1);
  });
});

test.describe('scale', () => {
  test('300 client entries stay responsive while typing, suggesting and hovering', async ({ page }) => {
    const e = editor(page);
    const entries = Array.from({ length: 300 }, (_, i) => `    "client-${i}": {\n      "secret": "secret-${i}",\n      "max_connections": ${(i % 50) + 1}\n    }`);
    const doc = `{\n  "clients": {\n${entries.join(',\n')}\n  }\n}\n`;
    await e.paste(doc);
    // Only the rows in view are rendered, however long the document.
    await expect(e.input).toHaveValue(doc);
    expect(await page.locator('.config-editor__gutter-content > div').count()).toBeLessThan(200);
    // New clients belong inside "clients": caret at the end of the last
    // entry, before the clients closing brace (EOF is outside the root object).
    await e.input.evaluate((element) => {
      const at = element.value.lastIndexOf('\n  }\n}');
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('Enter');
    const started = Date.now();
    await page.keyboard.type('"client-new": {', { delay: 0 });
    await page.keyboard.press('Enter');
    await page.keyboard.type('"sec');
    await expect.poll(() => e.keys(), { timeout: 5000 }).toContain('secret');
    const typing = Date.now() - started;
    console.log(`scale: typed and suggested in ${typing} ms for ${doc.split('\n').length} lines`);
    expect(typing, 'typing 25 characters in a 900-line document stays under 8 s').toBeLessThan(8000);
  });
});

test.describe('schema-driven completion', () => {
  test('enum values are offered and filtered as the user types', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "services": {\n    "dns": {\n      "target": 53,\n      "protocol": ""\n    }\n  }\n}');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('"protocol": ""') + '"protocol": "'.length;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toEqual(['tcp', 'udp']);
    await page.keyboard.type('u');
    await expect.poll(() => e.keys()).toEqual(['udp']);
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    expect(await e.value()).toContain('"protocol": "udp"');
    expect(await e.isOpen()).toBe(false);
  });

  test('initials match keys such as max_connections', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "clients": {\n    "alex": {\n      ""\n    }\n  }\n}');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('""') + 1;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('mc');
    // Only max_connections matches: secret has no "m" for the initials to hit.
    await expect.poll(() => e.keys()).toEqual(['max_connections']);
    const matched = page.locator('.config-editor__completion-match');
    await expect.poll(() => matched.allTextContents()).toEqual(['m', 'c']);
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    expect(await e.value()).toContain('"max_connections": ');
  });

  test('a typed prefix highlights the leading run of the match', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "clients": {\n    "alex": {\n      ""\n    }\n  }\n}');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('""') + 1;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('max');
    await expect.poll(() => e.keys()).toEqual(['max_connections']);
    const matched = page.locator('.config-editor__completion-match');
    await expect.poll(() => matched.allTextContents()).toEqual(['m', 'a', 'x']);
  });

  test('keys already present in a mapping are not suggested again', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "clients": {\n    "alex": {\n      "secret": "x",\n      ""\n    }\n  }\n}');
    await e.input.evaluate((element) => {
      const at = element.value.lastIndexOf('""') + 1;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toEqual(['max_connections']);
  });

  test('a partially typed key is replaced, not appended to', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "services": {\n    "billing": {\n      ""\n    }\n  }\n}');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('""') + 1;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('prot');
    await expect.poll(() => e.keys()).toContain('protocol');
    await e.clickKey('protocol');
    await expect(e.input).toHaveValue('{\n  "services": {\n    "billing": {\n      "protocol": ""\n    }\n  }\n}');
  });

  test('a service entry with required target suggests the target key', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "services": {\n    "dashboard": {\n      ""\n    }\n  }\n}');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('""') + 1;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toEqual(expect.arrayContaining(['target', 'protocol', 'mode']));
    expect((await e.keys())[0], 'required target ranks first').toBe('target');
  });

  test('a dynamic client name gets its own key suggestions', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "clients": {\n    "ci-runner": {\n      ""\n    }\n  }\n}');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('""') + 1;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toEqual(expect.arrayContaining(['secret', 'max_connections']));
  });

  test('array items open the suggestion list', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "services": {\n    "billing": {\n      "clients": [\n        ""\n      ]\n    }\n  }\n}');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('""') + 1;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.isOpen()).toBe(true);
  });

  test('a document with no matching schema path offers nothing', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "unknown_section": {}\n}');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('{}') + 1;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('Control+Space');
    await page.waitForTimeout(150);
    expect(await e.isOpen()).toBe(false);
  });
});

test.describe('update.remote', () => {
  test('typing remote: and accepting true keeps the value on the same line', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "update": {\n    ""\n  }\n}');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('""') + 1;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('remote');
    await expect.poll(() => e.keys()).toEqual(expect.arrayContaining(['remote']));
    await e.clickKey('remote');
    expect(await e.value()).toContain('"remote": ');
    expect(await e.isOpen()).toBe(true);
    await expect.poll(() => e.keys()).toEqual(expect.arrayContaining(['true', 'false']));
    await e.clickKey('true');
    expect(await e.value()).toContain('"remote": true');
    expect(await e.isOpen()).toBe(false);
  });

  test('keyboard acceptance of remote and true stays inline', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "update": {\n    ""\n  }\n}');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('""') + 1;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('remote');
    await expect.poll(() => e.keys()).toEqual(['remote']);
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    expect(await e.value()).toContain('"remote": ');
    await expect.poll(() => e.keys()).toEqual(expect.arrayContaining(['true', 'false']));
    await page.keyboard.type('t');
    await expect.poll(() => e.keys()).toEqual(['true']);
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    expect(await e.value()).toContain('"remote": true');
  });

  test('Enter after typing a key name accepts the suggestion', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "update": {\n    ""\n  }\n}');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('""') + 1;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('remote');
    await expect.poll(() => e.keys()).toEqual(['remote']);
    await page.keyboard.press('Enter');
    expect(await e.value()).toContain('"remote": ');
  });
});

test.describe('required expansion', () => {
  test('metrics expands listen and the example value stays on the listen line', async ({ page }) => {
    const e = editor(page);
    await e.paste('{}');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(1, 1);
    });
    await page.keyboard.type('"met');
    await expect.poll(() => e.keys()).toContain('metrics');
    await e.clickKey('metrics');
    expect(await e.value()).toContain('"metrics": {');
    expect(await e.value()).toContain('"listen": ');
    await expect.poll(() => e.keys()).toContain('0.0.0.0:7835');
    await e.clickKey('0.0.0.0:7835');
    expect(await e.value()).toContain('"listen": "0.0.0.0:7835"');
  });
});

test.describe('typing in values', () => {
  test('typing inside an existing value suggests and replaces it', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "update": {\n    "remote": true\n  }\n}');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('true');
      element.focus();
      element.setSelectionRange(at, at + 'true'.length);
    });
    await page.keyboard.type('f');
    await expect.poll(() => e.keys()).toContain('false');
    await e.clickKey('false');
    expect(await e.value()).toContain('"remote": false');
  });
});

test.describe('typing triggers', () => {
  test('typing suggests keys after Enter, filtering keys already present', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "listen": 7835\n}');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('7835') + '7835'.length;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('Enter');
    await page.keyboard.type('"l');
    await expect.poll(() => e.keys()).toContain('limits');
    expect(await e.keys()).not.toContain('listen');
  });

  test('typing a key on a blank line suggests the root keys', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "listen": 7835,\n  \n}');
    await e.input.evaluate((element) => {
      const at = element.value.lastIndexOf('\n}') - 0;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('"cl');
    await expect.poll(() => e.keys()).toContain('clients');
  });

  test('a partially typed duplicate root key is not suggested again', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "listen": 7835,\n  "insecure_plaintext": false,\n  \n}');
    await e.input.evaluate((element) => {
      const at = element.value.lastIndexOf('\n}') - 0;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('"ins');
    await page.waitForTimeout(200);
    expect(await e.keys()).not.toContain('insecure_plaintext');
  });

  test('typing after Enter suggests nested keys, while free-form strings have no fixed values', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "self": {}\n}');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('{}') + 1;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('Enter');
    await page.keyboard.type('"al');
    await expect.poll(() => e.keys()).toContain('allow');
    await e.clickKey('allow');
    // A list of strings starts with a quoted item, caret inside.
    expect(await e.value()).toContain('"allow": [""]');
    expect(await e.isOpen()).toBe(false);
    await page.keyboard.type('192.0.2.1');
    expect(await e.isOpen()).toBe(false);
    expect(await e.value()).toContain('"allow": ["192.0.2.1"]');
  });
});

test.describe('clicking lines', () => {
  test('every line of a long document with trailing blank lines is clickable', async ({ page }) => {
    const e = editor(page);
    let doc = '{\n';
    for (let index = 0; index < 25; index += 1) doc += `  "key-${index}": ${index}${index < 24 ? ',' : ''}\n`;
    doc += '}\n'.repeat(1) + '\n'.repeat(12);
    await e.paste(doc);
    const lines = doc.split('\n');
    const box = await e.input.boundingBox();
    const metrics = await e.input.evaluate((element) => {
      const style = getComputedStyle(element);
      return { left: parseFloat(style.paddingLeft), top: parseFloat(style.paddingTop), lineHeight: parseFloat(style.lineHeight) };
    });
    const failures = [];
    for (let line = 0; line < lines.length; line += 1) {
      await e.input.evaluate((element, target) => {
        const style = getComputedStyle(element);
        element.scrollTop = Math.max(0, target * parseFloat(style.lineHeight) - element.clientHeight / 2);
        element.dispatchEvent(new Event('scroll'));
      }, line);
      const scrollTop = await e.input.evaluate((element) => element.scrollTop);
      const y = box.y + metrics.top + (line + 0.5) * metrics.lineHeight - scrollTop;
      if (y < box.y + 2 || y > box.y + box.height - 2) continue;
      await page.mouse.click(box.x + metrics.left + 12, y);
      const got = await e.input.evaluate((element) => {
        const position = element.selectionStart;
        return element.value.slice(0, position).split('\n').length - 1;
      });
      if (got !== line) failures.push({ line, got });
    }
    expect(failures).toEqual([]);
  });

  test('clicking a diagnostic squiggle moves the caret instead of being swallowed', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "listen": 7835,\n  "websocket": {\n    "listen": 0\n  }\n}');
    const marker = page.locator('.config-editor__diagnostic-marker').first();
    await expect(marker).toHaveCount(1);
    await e.input.evaluate((element) => element.setSelectionRange(0, 0));
    await pointerOn(page, marker, { click: true });
    const state = await e.input.evaluate((element) => {
      const position = element.selectionStart;
      const before = element.value.slice(0, position);
      return { line: before.split('\n').length, column: position - (before.lastIndexOf('\n') + 1) + 1 };
    });
    expect(state.line).toBe(4);
  });
});

test.describe('diagnostics lines', () => {
  test('a nested key error lands on its own line, not the first key with the same name', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "listen": 7835,\n  "websocket": {\n    "listen": 0\n  }\n}');
    const marker = page.locator('.config-editor__diagnostic-marker').first();
    await expect(marker).toHaveCount(1);
    await expect(marker).toHaveAttribute('aria-label', /line 4/);
    await expect(page.locator('.config-editor__line-number-error')).toHaveText(['4']);
  });

  test('a trailing comma is reported on the comma', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "listen": 7835,\n}');
    const marker = page.locator('.config-editor__diagnostic-marker').first();
    await expect(marker).toHaveCount(1);
    await expect(marker).toHaveAttribute('aria-label', /line 2/);
    await expect(marker).toHaveText(',');
    await expect(page.locator('.config-editor__line-number-error')).toHaveText(['2']);
  });
});

test.describe('textarea and syntax-layer sync', () => {
  test('rapid typing and deletion keep visible syntax text equal to the editor value', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "listen": 7835\n}');
    await e.input.evaluate((element) => {
      const position = element.value.indexOf('7835') + 2;
      element.focus();
      element.setSelectionRange(position, position);
    });
    await page.keyboard.type('ABCD');
    await page.keyboard.press('Backspace');
    await page.keyboard.press('Backspace');

    const state = await page.evaluate(() => ({
      value: document.querySelector('.config-editor__input').value,
      rendered: [...document.querySelectorAll('.config-editor__highlight .config-editor__code-line')]
        .map((line) => line.textContent)
        .join('\n'),
    }));
    expect(state.rendered).toBe(state.value);
  });
});

test.describe('gutter and caret', () => {
  test('clicking a line number moves the caret to that line', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "a": 1,\n  "b": 2,\n  "c": 3\n}\n');
    await page.locator('.config-editor__gutter-content > div[data-line="3"]').click();
    const state = await e.input.evaluate((element) => {
      const position = element.selectionStart;
      const before = element.value.slice(0, position);
      return { line: before.split('\n').length, column: position - (before.lastIndexOf('\n') + 1) };
    });
    expect(state.line).toBe(3);
  });

  test('the caret color follows every theme', async ({ page }) => {
    const e = editor(page);
    const select = page.locator('.demo__theme-select');
    const caret = () => e.input.evaluate((element) => getComputedStyle(element).caretColor);
    await select.selectOption('vscode-dark');
    expect(await caret()).toBe('rgb(174, 175, 173)');
    await select.selectOption('github-light');
    expect(await caret()).toBe('rgb(31, 35, 40)');
    await select.selectOption('dracula');
    expect(await caret()).toBe('rgb(248, 248, 242)');
  });
});

test.describe('editing behaviour', () => {
  test('Ctrl+Z undoes an accepted completion and Ctrl+Shift+Z redoes it', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  ""\n}');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(5, 5);
    });
    await page.keyboard.type('lis');
    await e.clickKey('listen');
    await e.clickKey('7835');
    await expect(e.input).toHaveValue('{\n  "listen": 7835\n}');
    await page.keyboard.press('Control+Z');
    await expect(e.input).toHaveValue('{\n  "listen": \n}');
    await page.keyboard.press('Control+Shift+Z');
    await expect(e.input).toHaveValue('{\n  "listen": 7835\n}');
  });

  test('Tab indents selected lines and Shift+Tab outdents them', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n"a": 1,\n"b": 2\n}');
    await e.input.click();
    await page.keyboard.press('Control+A');
    await page.keyboard.press('Tab');
    expect(await e.value()).toContain('  "a": 1,');
    await page.keyboard.press('Shift+Tab');
    await expect(e.input).toHaveValue('{\n"a": 1,\n"b": 2\n}');
  });

  test('quotes and brackets are paired and typing the closer steps over', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "secret": ""\n}');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('""') + 1;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('ab');
    await expect(e.input).toHaveValue('{\n  "secret": "ab"\n}');
    await page.keyboard.type('"');
    await expect(e.input).toHaveValue('{\n  "secret": "ab"\n}');
    await page.keyboard.press('ArrowLeft');
    await page.keyboard.type('x');
    await expect(e.input).toHaveValue('{\n  "secret": "abx"\n}');
  });

  test('Enter after an array item opens a new line without breaking the document', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "self": {\n    "allow": [\n      "192.0.2.1"\n    ]\n  }\n}');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('"192.0.2.1"') + '"192.0.2.1"'.length;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('Enter');
    expect(await e.value()).toContain('"192.0.2.1"\n');
    expect(await e.isOpen()).toBe(false);
  });

  test('paste never opens suggestions', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "services": {\n    "billing": {\n      "prot\n    }\n  }\n}');
    await page.waitForTimeout(150);
    expect(await e.isOpen()).toBe(false);
  });
});

test.describe('diagnostics', () => {
  test('an unknown root key is reported and removing it clears the report', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "listen": 7835,\n  "bogus_key": 1\n}');
    await expect.poll(() => e.diagnosticCount()).toBeGreaterThan(0);
    await e.paste('{\n  "listen": 7835\n}');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });

  test('hovering a diagnostic marker shows its message', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "listen": 7835,\n  "bogus_key": 1\n}');
    const marker = page.locator('.config-editor__diagnostic-marker').first();
    await expect(marker).toHaveCount(1);
    await pointerOn(page, marker);
    await expect(marker).toHaveAttribute('data-message', /bogus_key|bogus/);
  });

  test('broken JSON is marked with a syntax error', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "web": {"http": [80}\n}');
    await expect.poll(() => e.diagnosticCount()).toBeGreaterThan(0);
  });

  test('squiggles stay out of the tab order; the status button jumps to the first error', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "listen": 7835,\n  "bogus_key": 1\n}');
    const marker = page.locator('.config-editor__diagnostic-marker').first();
    await expect(marker).toHaveCount(1);
    await expect(marker).toHaveAttribute('tabindex', '-1');
    const status = page.locator('.config-editor__diagnostic-status');
    await expect(status).not.toBeDisabled();
    await e.input.evaluate((element) => element.setSelectionRange(0, 0));
    await status.click();
    await expect.poll(() => e.caret()).toMatchObject({ line: 3 });
  });

  test('the status button is disabled when the document is clean', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "listen": 7835\n}');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
    await expect(page.locator('.config-editor__diagnostic-status')).toBeDisabled();
  });

  test('diagnostics wait for a pause in typing instead of flashing', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "listen": 7835\n}');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
    await page.keyboard.type(',');
    // Breaking the document must not report synchronously: validation runs
    // against a debounced snapshot, so the memo cannot recompute before the
    // pause elapses. This immediate read is deterministic, not timing-based.
    expect(await e.diagnosticCount()).toBe(0);
    // ...but the error shows up once typing pauses.
    await expect.poll(() => e.diagnosticCount()).toBeGreaterThan(0);
  });

  test('F8 cycles through errors and wraps, Shift+F8 goes backward', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "insecure_plaintext": "xyz",\n  "web": 5\n}');
    await expect.poll(() => e.diagnosticCount()).toBe(2);
    await e.input.evaluate((element) => element.setSelectionRange(0, 0));
    await page.keyboard.press('F8');
    expect((await e.caret()).line).toBe(2);
    await page.keyboard.press('F8');
    expect((await e.caret()).line).toBe(3);
    await page.keyboard.press('F8');
    expect((await e.caret()).line).toBe(2);
    await page.keyboard.press('Shift+F8');
    expect((await e.caret()).line).toBe(3);
  });

  test('an empty document reports no diagnostics and still suggests keys', async ({ page }) => {
    const e = editor(page);
    await e.paste('');
    expect(await e.value()).toBe('');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(element.value.length, element.value.length);
    });
    await page.keyboard.type('lis');
    await expect.poll(() => e.isOpen()).toBe(true);
    expect(await e.keys()).toContain('listen');
  });
});

test.describe('diff stability', () => {
  test('blank lines added at the end are only added, before and after typing', async ({ page }) => {
    const e = editor(page);
    await e.input.click();
    await page.keyboard.press('Control+End');
    for (let index = 0; index < 3; index += 1) {
      await page.keyboard.press('Enter');
    }
    await expect(page.locator('.config-editor__diff-marker-added')).toHaveCount(3);
    expect(await page.locator('.config-editor__diff-marker-modified').count()).toBe(0);
    await page.keyboard.type('"update": true');
    await expect(page.locator('.config-editor__diff-marker-added')).toHaveCount(3);
    expect(await page.locator('.config-editor__diff-marker-modified').count()).toBe(0);
  });

  test('the baseline final empty line stays anchored when typing in added lines', async ({ page }) => {
    const e = editor(page);
    await e.input.click();
    await page.keyboard.press('Control+End');
    for (let index = 0; index < 3; index += 1) await page.keyboard.press('Enter');
    await page.keyboard.press('ArrowUp');
    await page.keyboard.type('"middle": true');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.type('"end": true');

    const addedLines = await page.locator('.config-editor__line-number-added').allTextContents();
    expect(addedLines.length).toBeGreaterThan(0);
    expect(await page.locator('.config-editor__line-number-modified').count()).toBe(0);
  });
});

test.describe('editor viewport and scrolling', () => {
  test('adding many blank lines scrolls inside the editor instead of growing the page', async ({ page }) => {
    const e = editor(page);
    await e.input.click();
    await page.keyboard.press('Control+End');
    for (let index = 0; index < 100; index += 1) {
      await page.keyboard.press('Enter');
    }

    const layout = await page.evaluate(() => {
      const surface = document.querySelector('.config-editor__surface');
      const textarea = document.querySelector('.config-editor__input');
      return {
        viewportHeight: innerHeight,
        pageHeight: document.documentElement.scrollHeight,
        surfaceHeight: surface.getBoundingClientRect().height,
        textareaHeight: textarea.getBoundingClientRect().height,
        textareaClientHeight: textarea.clientHeight,
        textareaScrollHeight: textarea.scrollHeight,
        textareaScrollTop: textarea.scrollTop,
      };
    });
    expect(layout.surfaceHeight).toBeGreaterThan(240);
    expect(Math.abs(layout.surfaceHeight - layout.textareaHeight)).toBeLessThan(2);
    expect(layout.surfaceHeight).toBeLessThanOrEqual(layout.viewportHeight * 0.8 + 2);
    expect(layout.textareaHeight).toBeLessThanOrEqual(layout.viewportHeight * 0.8 + 2);
    expect(layout.pageHeight).toBeLessThanOrEqual(layout.viewportHeight + 1);
    expect(layout.textareaScrollHeight).toBeGreaterThan(layout.textareaClientHeight);
    expect(layout.textareaScrollTop).toBeGreaterThan(0);
    await page.keyboard.press('ArrowUp');
    await page.keyboard.press('ArrowDown');
    expect(await page.evaluate(() => scrollY)).toBe(0);
  });

  test('the last gutter lines remain clickable after scrolling through many blank lines', async ({ page }) => {
    const e = editor(page);
    await e.input.click();
    await page.keyboard.press('Control+End');
    for (let index = 0; index < 100; index += 1) {
      await page.keyboard.press('Enter');
    }
    const lineCount = await e.input.evaluate((element) => element.value.split('\n').length);
    const lastLine = page.locator(`.config-editor__gutter-content > div[data-line="${lineCount}"]`);
    await lastLine.click();
    const caretLine = await e.input.evaluate((element) =>
      element.value.slice(0, element.selectionStart).split('\n').length,
    );
    expect(caretLine).toBe(lineCount);
    await page.keyboard.type('"last_line": true');
    expect(await e.value()).toContain('"last_line": true');
  });

  test('consumers can choose a shorter fixed editor height', async ({ page }) => {
    const e = editor(page);
    await page.addStyleTag({
      content: '.demo { --config-editor-height: 180px; --config-editor-min-height: 160px; --config-editor-max-height: 300px; }',
    });
    await expect.poll(() => e.input.evaluate((element) => Math.round(element.getBoundingClientRect().height))).toBe(180);
    await e.input.click();
    for (let index = 0; index < 50; index += 1) await page.keyboard.press('Enter');
    const layout = await e.input.evaluate((element) => ({
      height: element.getBoundingClientRect().height,
      scrollHeight: element.scrollHeight,
      clientHeight: element.clientHeight,
      scrollTop: element.scrollTop,
    }));
    expect(layout.height).toBe(180);
    expect(layout.scrollHeight).toBeGreaterThan(layout.clientHeight);
    expect(layout.scrollTop).toBeGreaterThan(0);
  });
});

test.describe('diff gutter and apply', () => {
  test('a new line is marked as added and Apply changes clears the marker', async ({ page }) => {
    const e = editor(page);
    await page.waitForTimeout(150);
    const initial = await e.value();
    // A new root entry goes before the final closing brace, not after it.
    await e.input.evaluate((element) => {
      const at = element.value.lastIndexOf('}');
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type(',');
    await page.keyboard.press('Enter');
    await page.keyboard.type('"insecure_plaintext": true');
    await expect(page.locator('.config-editor__diff-marker-added')).toHaveCount(1);
    await page.locator('.config-editor__action-button').click();
    await expect(page.locator('.config-editor__diff-marker-added')).toHaveCount(0);
    expect(initial).not.toEqual(await e.value());
  });
});

test.describe('themes', () => {
  test('every theme keeps the same editor font size', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "listen": 7835\n}');
    const select = page.locator('.demo__theme-select');
    const values = await select.locator('option').evaluateAll((options) => options.map((o) => o.value));
    const sizes = new Set();
    for (const theme of values) {
      await select.selectOption(theme);
      await page.waitForTimeout(80);
      sizes.add(await e.input.evaluate((el) => getComputedStyle(el).fontSize));
    }
    expect([...sizes]).toEqual(['12px']);
  });

  test('the suggestion list is readable in light and dark themes', async ({ page }) => {
    const e = editor(page);
    const select = page.locator('.demo__theme-select');
    await e.paste('{\n  ""\n}');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(5, 5);
    });
    await page.keyboard.type('"lis');
    await expect.poll(() => e.isOpen()).toBe(true);
    await e.screenshot('theme-dark-suggestions-json');
    await select.selectOption('vscode-light');
    await page.waitForTimeout(80);
    await expect(e.popup).toBeVisible();
    await e.screenshot('theme-light-suggestions-json');
  });
});

test.describe('hover on nested and dynamic keys', () => {
  test('hovering a dynamic client key reports its schema fields', async ({ page }) => {
    const e = editor(page);
    await e.paste(LARGE_JSON);
    const card = page.locator('.config-editor__hover');
    const line = LARGE_JSON.split('\n').findIndex((l) => l.includes('"secret": "alpha-secret"'));
    await scrollToLine(page, line);
    const p = await pointAt(page, line, 6);
    await page.mouse.move(p.x, p.y, { steps: 4 });
    await expect(card).toHaveCount(1);
    await expect(card.locator('.config-editor__hover-key')).toHaveText('secret');
  });

  test('hovering a key inside a sequence item reports its schema', async ({ page }) => {
    const e = editor(page);
    await e.paste(LARGE_JSON);
    const card = page.locator('.config-editor__hover');
    const line = LARGE_JSON.split('\n').findIndex((l) => l.includes('"weight": 2'));
    await scrollToLine(page, line);
    const p = await pointAt(page, line, 12);
    await page.mouse.move(p.x, p.y, { steps: 4 });
    await expect(card).toHaveCount(1);
    await expect(card.locator('.config-editor__hover-key')).toHaveText('weight');
  });
});
