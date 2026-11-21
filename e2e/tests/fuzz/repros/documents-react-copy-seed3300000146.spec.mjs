import { expect, test } from '@playwright/test';
import { PAGES, installProbe, pageUrl } from '../pages.mjs';
import { copyFits } from '../perform.mjs';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 3300000146 on documents-react (?random).
// Fails the `copy` check after step 17: copied "\n", selection " "
test('documents-react: copy (seed 3300000146)', async ({ browser }) => {
  await replay(
    browser,
    'documents-react',
    '?random',
    [
      {
        k: 'clip',
        key: 'Control+V',
      },
      {
        k: 'type',
        text: 'value\nx',
      },
      {
        k: 'key',
        key: 'Shift+Alt+ArrowDown',
        n: 1,
      },
      {
        k: 'key',
        key: 'Enter',
        n: 3,
      },
      {
        k: 'find',
        query: '1',
        replace: null,
        all: false,
      },
      {
        k: 'key',
        key: 'Control+Z',
        n: 2,
      },
      {
        k: 'dragMove',
        at: 0.875,
        length: 6,
        x: 0.473,
        y: 0.432,
      },
      {
        k: 'type',
        text: '- item',
      },
      {
        k: 'find',
        query: 'e',
        replace: null,
        all: false,
      },
      {
        k: 'clip',
        key: 'Control+V',
      },
      {
        k: 'key',
        key: 'Shift+ArrowRight',
        n: 4,
      },
      {
        k: 'drag',
        x1: 0.009,
        y1: 0.432,
        x2: 0.956,
        y2: 0.596,
      },
      {
        k: 'type',
        text: 'true',
      },
      {
        k: 'editAs',
        language: 'xml',
      },
      {
        k: 'type',
        text: 'null',
      },
      {
        k: 'key',
        key: 'Shift+Control+Home',
        n: 3,
      },
      {
        k: 'type',
        text: ' ',
      },
      {
        k: 'crossCopy',
        from: 0,
        to: 1,
        load: false,
        token: true,
        at: 0.474,
        length: 14,
        at2: 0.017,
        over: 4,
      },
    ],
    'copy',
  );
});

// The same race made certain: the selection is read, the Edit-as switch
// waiting for the text to parse goes through (rewriting " " as an empty
// XML view), then the copy. The copy check takes the selection as the
// text after the switch left it, not as it was read before.
test('documents-react: a copy after a waiting switch rewrote the text fits the selection it left', async ({ browser }) => {
  const context = await browser.newContext({ viewport: { width: 1100, height: 620 } });
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  const page = await context.newPage();
  await page.goto(pageUrl('documents-react'));
  await page.waitForFunction(() => document.body.dataset.ready === 'true', null, { timeout: 20000 });
  await page.evaluate(installProbe, PAGES['documents-react']);
  await page.evaluate(() => window.__fz.root().querySelector('.config-editor__input').focus());
  // A text that does not parse: the switch to XML waits.
  await page.keyboard.press('Control+A');
  await page.keyboard.type('a: b: c');
  await page.evaluate(() => {
    window.documentsHandle.current.editAs('main', 'xml');
    window.updateFile('main', { editAs: 'xml' });
  });
  const switched = () => document.getElementById('log').textContent.includes('language main xml');
  expect(await page.evaluate(switched)).toBe(false);
  // It parses now; the switch goes through after the typing pause.
  await page.keyboard.press('Control+A');
  await page.keyboard.type(' ');
  await page.keyboard.press('Shift+Home');
  const want = await page.evaluate(async () => {
    await navigator.clipboard.writeText('␀ before the copy');
    return window.__fz.selected();
  });
  expect(want.text).toBe(' ');
  await page.waitForFunction(switched);
  await page.keyboard.press('Control+C');
  await page.waitForTimeout(40);
  expect(await copyFits(page, want)).toBeNull();
  await context.close();
});
