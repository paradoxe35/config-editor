import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// The host shield (QA 31, L31-1, L31-2, FW31-2): a page's rules for bare
// elements (`input { width: 100% }`, `span { margin: 2px }`), a framework's
// higher-weight `:not()` rules (Pico) and its equal-weight attribute rules
// loaded after the editor (Tailwind forms) leave every control and text
// element the editor draws as it is without them: the same boxes, colors
// and spacing, the highlighted text over the textarea's, in the find,
// replace and go-to-line widgets, the suggestion list, the hover card, the
// change preview and the documents' tabs. Through the npm package (run
// `npm run build` in js/ first).

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const VANILLA = pathToFileURL(join(js, 'examples', 'vanilla.html')).href;

const HOSTILE = {
  'bare elements': `
    * { box-sizing: content-box; }
    input, textarea, button, select { display: block; width: 100%; height: 56px; margin: 0 0 16px; padding: 12px 16px;
      border: 2px solid #f00; border-radius: 9px; background: #fff; color: #000; font-size: 18px; line-height: 2;
      letter-spacing: 2px; text-transform: uppercase; box-shadow: 0 0 0 3px #f00; min-width: 120px; }
    span, a, code, kbd, strong, p, li, h3, label { margin: 2px; padding: 2px; border: 1px solid #f00; letter-spacing: 2px;
      word-spacing: 4px; font-size: 20px; line-height: 3; background: #ff0; color: #f00; vertical-align: middle; }
    code, pre, kbd { font-family: serif; }`,
  'Pico-like :not() rules': `
    input:not([type=checkbox],[type=radio],[type=range],[type=file]), select, textarea {
      width: 100%; height: 56px; padding: 12px 16px; margin-bottom: 16px; font-size: 18px; border: 1px solid #ccc;
      background-color: #fff; color: #111; border-radius: 8px; }
    input:not([type=checkbox],[type=radio],[type=range],[type=file]):is(:active,:focus) { box-shadow: 0 0 0 4px #09f; border-color: #09f; }
    :is(button,[type=submit],[type=button],[role=button]) { padding: 12px 24px; border: 2px solid #09f; border-radius: 8px;
      background-color: #09f; color: #fff; font-size: 18px; line-height: 1.5; }
    :is(button,[type=submit],[type=button],[role=button]):is(:hover,:active,:focus) { background-color: #07c; box-shadow: 0 0 0 4px #09f; }`,
  'Tailwind-forms-like attribute rules': `
    [type='text'], input:where(:not([type])), [type='search'], textarea { appearance: none; background-color: #fff;
      border-color: #6b7280; border-width: 1px; border-radius: 0; padding: .5rem .75rem; font-size: 1rem; line-height: 1.5rem; }
    [type='text']:focus, input:where(:not([type])):focus, textarea:focus { outline: 2px solid transparent; outline-offset: 2px;
      box-shadow: 0 0 0 3px #2563eb; border-color: #2563eb; }
    [type='button'] { background-color: #fff; padding: 8px 16px; }`,
};

const LONG = `key_001: ${'abcdefghij'.repeat(30)}\nmode: fast\nlisten: 7835\n`;

/** Every rendered element in the editors (the toolbar aside: the page's own chrome), with its box and the styles a host rule could change. */
function measure(page) {
  return page.evaluate(() => {
    const root = document.getElementById('host');
    const origin = root.getBoundingClientRect();
    const round = (value) => Math.round(value * 2) / 2;
    const props = ['color', 'background-color', 'border-top-width', 'border-left-width', 'border-top-color', 'margin-top', 'margin-left',
      'padding-top', 'padding-left', 'letter-spacing', 'word-spacing', 'font-size', 'font-family', 'line-height', 'box-shadow', 'text-transform'];
    return [...root.querySelectorAll('*')]
      .filter((element) => element.getClientRects().length && !element.closest('.config-editor__toolbar') && !(element instanceof SVGElement))
      .map((element) => {
        const box = element.getBoundingClientRect();
        const style = getComputedStyle(element);
        const name = `${element.tagName.toLowerCase()}.${String(element.className).split(' ')[0]}`;
        return `${name} ${round(box.left - origin.left)},${round(box.top - origin.top)} ${round(box.width)}x${round(box.height)} ${props.map((prop) => style.getPropertyValue(prop)).join('|')}`;
      });
  });
}

/** Where the highlighted first line ends against where the textarea lays the same text out. */
function alignment(page) {
  return page.evaluate(() => {
    const root = document.getElementById('host');
    const area = root.querySelector('.config-editor__input');
    const line = root.querySelector('.config-editor__highlight .config-editor__code-line');
    const spans = [...line.querySelectorAll('span')].filter((span) => span.textContent);
    // A textarea's scroll width ends at its longest line (Chromium leaves
    // out the end padding).
    const textEnd = area.getBoundingClientRect().left + area.scrollWidth;
    return Math.abs(spans.at(-1).getBoundingClientRect().right - textEnd);
  });
}

/** The WCAG contrast of each find, replace and go-to field's text over what is behind it. */
function fieldContrast(page) {
  return page.evaluate(() => {
    const parse = (color) => color.match(/[\d.]+/g).map(Number);
    const luminance = ([r, g, b]) => {
      const channel = (value) => ((value /= 255) <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
      return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
    };
    const behind = (element) => {
      for (let at = element; at; at = at.parentElement) {
        const color = parse(getComputedStyle(at).backgroundColor);
        if (color.length < 4 || color[3] > 0) return color;
      }
      return [255, 255, 255];
    };
    return [...document.querySelectorAll('#host input')].map((input) => {
      const [a, b] = [luminance(parse(getComputedStyle(input).color)), luminance(behind(input))];
      return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
    });
  });
}

async function mount(page, kind) {
  await page.evaluate(async ([kind, value]) => {
    const CE = await import(new URL('../config-editor/index.js', location.href).href);
    const host = document.createElement('div');
    host.id = 'host';
    host.style.cssText = 'height:340px;width:720px';
    document.body.prepend(host);
    const schema = await (await fetch('../../schema.json')).text();
    window.hosted =
      kind === 'documents'
        ? new CE.ConfigDocuments(host, {
            documents: [
              { id: 'a', fileName: 'a.yaml', value, schema, baseline: value.replace('fast', 'slow') },
              { id: 'b', fileName: 'conf.d/b.toml', value: 'a = 1\n' },
            ],
          })
        : new CE.ConfigEditor(host, { language: 'yaml', value, schema, baseline: value.replace('fast', 'slow'), fileName: 'x.yaml' });
  }, [kind, LONG]);
  await expect(page.locator('#host .config-editor__input:visible')).toBeVisible();
}

async function unmount(page) {
  await page.evaluate(() => {
    window.hosted.destroy();
    document.getElementById('host').remove();
  });
}

const addSheet = (page, css) => page.addStyleTag({ content: css });

/** Opens each widget in turn, measuring the editor with it open. */
async function walk(page) {
  const input = page.locator('#host .config-editor__input:visible');
  const seen = {};
  await input.click();
  await page.keyboard.press('Control+Home');
  seen.text = await measure(page);
  seen.alignment = await alignment(page);
  await page.keyboard.press('Control+h');
  await page.keyboard.type('mode');
  await page.keyboard.press('Tab');
  seen.find = await measure(page);
  seen.contrast = await fieldContrast(page);
  await page.keyboard.press('Escape');
  await input.focus();
  await page.keyboard.press('Control+g');
  seen.goto = await measure(page);
  seen.gotoContrast = await fieldContrast(page);
  await page.keyboard.press('Escape');
  await input.focus();
  await page.keyboard.press('Control+End');
  await page.keyboard.press('Control+Space');
  await expect(page.locator('#host .config-editor__completion').first()).toBeVisible();
  seen.suggestions = await measure(page);
  await page.keyboard.press('Escape');
  // `listen`, a key the schema documents.
  await page.keyboard.press('Control+Home');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Control+k');
  await page.keyboard.press('Control+i');
  await expect(page.locator('#host .config-editor__hover')).toBeVisible();
  seen.hover = await measure(page);
  await page.keyboard.press('Escape');
  await page.keyboard.press('Alt+F3');
  await expect(page.locator('#host .config-editor__diff-peek')).toBeVisible();
  seen.preview = await measure(page);
  await page.keyboard.press('Escape');
  return seen;
}

test.describe('the editor under hostile host stylesheets', () => {
  test.skip(!built, 'the npm package is not built: cd js && npm run build');

  for (const kind of ['editor', 'documents']) {
    for (const [name, css] of Object.entries(HOSTILE)) {
      for (const when of ['before', 'after']) {
        test(`${kind}: ${name}, added ${when} the editor mounts`, async ({ page }) => {
          // No transitions: boxes are measured as they end.
          await page.emulateMedia({ reducedMotion: 'reduce' });
          await page.goto(VANILLA);
          await page.waitForFunction(() => document.body.dataset.ready === 'true');
          await page.evaluate(() => window.editor.destroy());
          await mount(page, kind);
          const plain = await walk(page);
          await unmount(page);
          if (when === 'before') await addSheet(page, css);
          await mount(page, kind);
          if (when === 'after') await addSheet(page, css);
          const styled = await walk(page);
          expect(plain.alignment).toBeLessThanOrEqual(1);
          expect(styled.alignment, 'the highlighted text ends where the textarea lays it out').toBeLessThanOrEqual(1);
          for (const state of ['text', 'find', 'goto', 'suggestions', 'hover', 'preview']) {
            const changed = styled[state].filter((line, index) => line !== plain[state][index]);
            expect(changed.slice(0, 5), `${state}: elements the page's rules changed`).toEqual([]);
            expect(styled[state].length, state).toBe(plain[state].length);
          }
          for (const ratio of [...styled.contrast, ...styled.gotoContrast]) expect(ratio).toBeGreaterThanOrEqual(4.5);
        });
      }
    }
  }
});
