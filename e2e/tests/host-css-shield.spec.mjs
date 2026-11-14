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
  // QA 32, UX32-3: every element the editor draws, not only its controls
  // and inline text (the frame, the gutter, the highlighted text, the
  // footer and the tabs are `div`s, `section`s and `pre`s).
  // The roots' own boxes (and the element the editor is in) are the
  // page's to style: the box rules leave them out, the text rules do not.
  'rules for every element': `
    div:where(:not(#host, .config-editor, .config-editor-documents)), section, header, footer, nav, aside, article, pre {
      margin: 4px; padding: 1rem; border: 1px solid #f00; background: #ff0; box-shadow: 0 0 0 3px #f00; min-height: 30px; }
    div, section, header, footer, nav, aside, article, pre { letter-spacing: 2px; word-spacing: 6px; font-family: serif;
      font-size: 17px; line-height: 2.4; color: #f00; text-transform: uppercase; text-align: center; }
    div:where(:not(#host, .config-editor, .config-editor-documents))::before,
    div:where(:not(#host, .config-editor, .config-editor-documents))::after, span::after { content: "x"; margin: 2px; }
    ::placeholder { color: #f00; letter-spacing: 3px; }`,
  // QA 32, UX32-2: the built-in toolbar's own elements too.
  'toolbar elements': `
    span { margin: 3px; padding: 3px; letter-spacing: 2px; background: #ff0; color: #f00; font-size: 18px; }
    span, div { display: block; }`,
  // QA 32, CSS32-1 and CSS32-2: universal and attribute rules reach the
  // textarea and the drawn text alike, or neither (here: neither).
  'universal rules': `
    * { letter-spacing: 1px; word-spacing: 4px; line-height: 2; font-family: Georgia, serif; font-size: 15px;
      text-transform: uppercase; font-weight: 700; tab-size: 8; font-variant-ligatures: none; margin: 0; padding: 0; }`,
  'attribute rules': `
    [aria-hidden=true] { opacity: .3; }
    [role=listbox], [data-line], [role=status], [role=search], [role=tab], [type=button] { padding: 6px; margin: 3px; }
    [spellcheck], [autocomplete] { letter-spacing: 2px; padding: 9px; }`,
  'Tailwind-forms-like attribute rules': `
    [type='text'], input:where(:not([type])), [type='search'], textarea { appearance: none; background-color: #fff;
      border-color: #6b7280; border-width: 1px; border-radius: 0; padding: .5rem .75rem; font-size: 1rem; line-height: 1.5rem; }
    [type='text']:focus, input:where(:not([type])):focus, textarea:focus { outline: 2px solid transparent; outline-offset: 2px;
      box-shadow: 0 0 0 3px #2563eb; border-color: #2563eb; }
    [type='button'] { background-color: #fff; padding: 8px 16px; }`,
};

const LONG = `key_001: ${'abcdefghij'.repeat(30)}\nmode: fast\nlisten: 7835\n`;

/**
 * Every rendered element in the editors (the built-in toolbar too), with
 * its box and the styles a host rule could change; measured from the
 * frame (or the documents view's column), as the root's own box (`div {
 * padding: 1rem }` on it) is the page's to style.
 */
function measure(page) {
  return page.evaluate(() => {
    const root = document.getElementById('host');
    const origin = root.querySelector('.config-editor__frame, .config-editor-documents__view').getBoundingClientRect();
    const round = (value) => Math.round(value * 2) / 2;
    const props = ['color', 'background-color', 'border-top-width', 'border-left-width', 'border-top-color', 'margin-top', 'margin-left',
      'padding-top', 'padding-left', 'letter-spacing', 'word-spacing', 'font-size', 'font-family', 'line-height', 'box-shadow', 'text-transform',
      'text-align', 'display', 'opacity'];
    return [...root.querySelectorAll('*')]
      .filter((element) => element.getClientRects().length && !(element instanceof SVGElement) && element.parentElement !== root)
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

/**
 * Where a click on drawn characters puts the caret (QA 32, CSS32-1): on
 * the last character of each word, the textarea's offset, and the offset
 * of that character in the text.
 */
async function clicks(page) {
  const out = [];
  for (const word of ['mode', 'fast', '7835']) {
    const point = await page.evaluate((word) => {
      const layer = document.querySelector('#host .config-editor__highlight');
      const walker = document.createTreeWalker(layer, NodeFilter.SHOW_TEXT);
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        const at = node.data.lastIndexOf(word);
        if (at < 0) continue;
        const range = document.createRange();
        range.setStart(node, at + word.length - 1);
        range.setEnd(node, at + word.length);
        const box = range.getBoundingClientRect();
        return { x: box.x + box.width * 0.4, y: box.y + box.height / 2 };
      }
      return null;
    }, word);
    await page.mouse.click(point.x, point.y);
    const [caret, text] = await page
      .locator('#host .config-editor__input:visible')
      .evaluate((element) => [element.selectionStart, element.value]);
    out.push([word, caret, text.lastIndexOf(word) + word.length - 1]);
  }
  await page.locator('#host .config-editor__input:visible').focus();
  await page.keyboard.press('Control+Home');
  return out;
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
        : new CE.ConfigEditor(host, { language: 'yaml', value, schema, baseline: value.replace('fast', 'slow'), fileName: 'x.yaml', showToolbar: true, showFooter: true });
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
  seen.clicks = await clicks(page);
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
          for (const [word, caret, at] of styled.clicks) expect(caret, `a click on the end of "${word}" puts the caret there`).toBe(at);
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

// --- Inherited text properties (QA 32, UX32-1). -------------------------------

// Each inherited property that changes where glyphs go or what they look
// like, at a value other than its initial one, as a page sets it on `body`
// or on the element the editor is in (a centered or right-aligned
// container, an underlined section, a page's tab size or font features):
// none reaches inside, so the editor's pixels are the same as without it
// (the highlighted text stays over the textarea's).
const INHERITED = [
  'text-align: center', 'text-align: right', 'text-align: justify', 'text-align: end', 'text-align-last: center',
  'text-justify: inter-character', 'text-indent: 3em', 'text-transform: uppercase', 'letter-spacing: 2px',
  'word-spacing: 8px', 'text-shadow: 1px 1px #f00', 'text-decoration: underline #f00 2px', 'text-decoration: line-through',
  'text-emphasis: dot #f00', 'text-underline-position: under', 'text-rendering: geometricPrecision',
  'white-space: nowrap', 'white-space: pre-wrap', 'text-wrap: balance', 'word-break: break-all', 'overflow-wrap: anywhere',
  'line-break: anywhere', 'hyphens: auto', 'tab-size: 13', 'direction: rtl', 'font-feature-settings: "smcp", "zero"',
  'font-variant: small-caps', 'font-kerning: none', 'font-variation-settings: "wght" 700', 'font-size-adjust: 0.3',
  'font-synthesis: none', 'font-style: italic', 'font-weight: 700', 'font-size: 20px', 'line-height: 3',
  'font-family: serif', '-webkit-text-stroke: 1px #f00', '-webkit-text-fill-color: #f00', '-webkit-text-security: disc',
  'paint-order: stroke', 'list-style: square inside', 'quotes: "«" "»"', 'color: #f00',
];

/** A picture of the editors in `#host`: the text with a selection, the find widget open on a query. */
async function picture(page) {
  const input = page.locator('#host .config-editor__input:visible');
  await input.click();
  await page.keyboard.press('Control+Home');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Shift+End');
  await page.keyboard.press('Control+f');
  await page.keyboard.press('Control+a');
  await page.keyboard.type('mode');
  await expect(page.locator('#host .config-editor__find-count')).toHaveText(/1/);
  const box = await page.locator('#host').boundingBox();
  const shoot = () => page.screenshot({ clip: box, animations: 'disabled', caret: 'hide' });
  // Settled: the problems' underlines come after a typing pause.
  let shot = await shoot();
  for (let tries = 0; tries < 10; tries++) {
    await page.waitForTimeout(150);
    const again = await shoot();
    if (again.equals(shot)) return shot;
    shot = again;
  }
  return shot;
}

/**
 * How many pixels differ between two pictures. A few may, from one editor
 * to the next: the frame's rounded corners are not always smoothed alike.
 * What a page's text property would change (a moved, underlined or
 * restyled text) is hundreds.
 */
function differing(page, a, b) {
  return page.evaluate(
    async ([a, b]) => {
      const pixels = async (source) => {
        const image = new Image();
        image.src = source;
        await image.decode();
        const canvas = new OffscreenCanvas(image.width, image.height);
        const context = canvas.getContext('2d');
        context.drawImage(image, 0, 0);
        return context.getImageData(0, 0, image.width, image.height).data;
      };
      const [first, second] = [await pixels(a), await pixels(b)];
      let count = 0;
      for (let at = 0; at < first.length; at += 4) {
        if (first[at] !== second[at] || first[at + 1] !== second[at + 1] || first[at + 2] !== second[at + 2]) count++;
      }
      return count;
    },
    [a, b].map((png) => `data:image/png;base64,${png.toString('base64')}`),
  );
}

const CORNER_NOISE = 16;

test.describe('the editor under inherited text properties', () => {
  test.skip(!built, 'the npm package is not built: cd js && npm run build');

  for (const kind of ['editor', 'documents']) {
    for (const on of ['body', '#host']) {
      for (const when of ['before', 'after']) {
        test(`${kind}: each set on ${on}, ${when} the editor mounts`, async ({ page }) => {
          await page.emulateMedia({ reducedMotion: 'reduce' });
          await page.goto(VANILLA);
          await page.waitForFunction(() => document.body.dataset.ready === 'true');
          await page.evaluate(() => window.editor.destroy());
          await mount(page, kind);
          const plain = await picture(page);
          await unmount(page);
          const changed = [];
          for (const declaration of INHERITED) {
            const sheet = `${on} { ${declaration} !important; }`;
            if (when === 'before') await addSheet(page, sheet);
            await mount(page, kind);
            if (when === 'after') await addSheet(page, sheet);
            const count = await differing(page, plain, await picture(page));
            if (count > CORNER_NOISE) changed.push(`${declaration} (${count} pixels)`);
            await unmount(page);
            await page.evaluate((text) => {
              for (const style of document.querySelectorAll('style')) if (style.textContent === text) style.remove();
            }, sheet);
          }
          expect(changed, 'inherited properties that change the editor').toEqual([]);
        });
      }
    }
  }
});
