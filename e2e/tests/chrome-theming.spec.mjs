import { expect, test } from '@playwright/test';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Theming the chrome (the tabs, the toolbar and the footer): a host's
// global resets (Tailwind's preflight, modern-normalize) leave its layout
// as it is; the `--config-editor-*` variables restyle its looks, from the
// host's own CSS or the `chrome` and `colors` settings; a host's unlayered
// rule on a chrome class wins over the editor's cosmetic rules (in
// `@layer config-editor.chrome`); and with none of it, every computed
// style is what it was before the variables (`e2e/fixtures/chrome-defaults.json`,
// recorded from the build before them with `RECORD_CHROME_DEFAULTS=1`;
// boxes left out, as they depend on the machine's fonts, but for the
// fixed sizes).
// Through the npm package (run `npm run build` in js/ first).

const here = dirname(fileURLToPath(import.meta.url));
const js = join(here, '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const VANILLA = pathToFileURL(join(js, 'examples', 'vanilla.html')).href;
const fixtures = join(here, '..', 'fixtures');
const resetSheet = (name) => readFileSync(join(fixtures, 'resets', name), 'utf8');
const DEFAULTS = join(fixtures, 'chrome-defaults.json');
const RESETS = { 'Tailwind preflight': 'tailwind-preflight.css', 'modern-normalize': 'modern-normalize.css' };
const THEMES = ['vscode-dark', 'vscode-light', 'github-light', 'solarized-light', 'nord', 'dracula'];
const WIDTHS = { desktop: 1200, phone: 390 };

/** The chrome's parts, by what they are. */
const PARTS = {
  strip: '.config-editor-documents__strip',
  tab: '.config-editor-documents__tab[data-document-id="c"]',
  'active tab': '.config-editor-documents__tab[data-document-id="a"]',
  'active tab bar': '.config-editor-documents__tab[data-document-id="a"]::before',
  'read-only tab': '.config-editor-documents__tab[data-document-id="b"]',
  'read-only name': '.config-editor-documents__tab[data-document-id="b"] .config-editor-documents__name',
  name: '.config-editor-documents__tab[data-document-id="c"] .config-editor-documents__name',
  close: '.config-editor-documents__tab[data-document-id="c"] .config-editor-documents__close',
  'dirty close': '.config-editor-documents__tab[data-document-id="a"] .config-editor-documents__close',
  add: '.config-editor-documents__add',
  toolbar: '.config-editor-documents__panel:not(.config-editor-documents__panel-hidden) .config-editor__toolbar',
  'file name': '.config-editor-documents__panel:not(.config-editor-documents__panel-hidden) .config-editor__file',
  footer: '.config-editor-documents__panel:not(.config-editor-documents__panel-hidden) .config-editor__status',
  'footer meta': '.config-editor-documents__panel:not(.config-editor-documents__panel-hidden) .config-editor__status-meta',
};

const PROPS = [
  'display', 'position', 'box-sizing', 'flex-direction', 'flex-wrap', 'align-items', 'justify-content', 'overflow-x', 'overflow-y',
  'padding-top', 'padding-right', 'padding-bottom', 'padding-left', 'margin-top', 'margin-left', 'column-gap', 'row-gap',
  'border-top-width', 'border-right-width', 'border-bottom-width', 'border-left-width',
  'border-top-style', 'border-right-style', 'border-bottom-style', 'border-left-style',
  'border-top-color', 'border-right-color', 'border-bottom-color', 'border-left-color',
  'border-top-left-radius', 'border-top-right-radius', 'border-bottom-left-radius', 'border-bottom-right-radius',
  'font-family', 'font-size', 'font-weight', 'font-style', 'line-height', 'letter-spacing', 'text-transform',
  'color', 'background-color', 'cursor', 'visibility', 'transition-property', 'transition-duration',
];

/** Each part's box (from the documents view's column) and computed styles. */
function measure(page) {
  return page.evaluate(([parts, props]) => {
    const view = document.querySelector('#host .config-editor-documents__view').getBoundingClientRect();
    const out = {};
    for (const [part, selector] of Object.entries(parts)) {
      const [element_, pseudo] = selector.split('::');
      const element = document.querySelector(`#host ${element_}`);
      if (!element) {
        out[part] = null;
        continue;
      }
      const style = getComputedStyle(element, pseudo ? `::${pseudo}` : null);
      const entry = {};
      if (!pseudo) {
        const box = element.getBoundingClientRect();
        entry.box = [box.left - view.left, box.top - view.top, box.width, box.height].map((value) => Math.round(value * 2) / 2).join(',');
      } else {
        entry.size = `${style.width} ${style.height}`;
      }
      for (const prop of props) entry[prop] = style.getPropertyValue(prop);
      out[part] = entry;
    }
    return out;
  }, [PARTS, PROPS]);
}

async function mount(page, extra = {}) {
  await page.evaluate(async (extra) => {
    const CE = await import(new URL('../config-editor/index.js', location.href).href);
    const host = document.createElement('div');
    host.id = 'host';
    host.style.cssText = 'height:340px;width:100%;max-width:900px';
    document.body.prepend(host);
    window.hosted = new CE.ConfigDocuments(host, {
      documents: [
        { id: 'a', fileName: 'service.yaml', value: 'mode: fast\nlisten: 7835\n' },
        { id: 'b', fileName: 'conf.d/limits.toml', value: 'a = 1\n', readOnly: true },
        { id: 'c', fileName: 'clients.json', value: '{}\n' },
      ],
      showToolbar: true,
      showFooter: true,
      onClose() {},
      onAdd() {},
      ...extra,
    });
  }, extra);
  const input = page.locator('#host .config-editor__input:visible');
  await expect(input).toBeVisible();
  // The active tab dirty: its dot shows in place of the close button.
  await input.click();
  await page.keyboard.press('End');
  await page.keyboard.type('x');
  await expect(page.locator('#host .config-editor-documents__tab-dirty')).toHaveCount(1);
  // The pointer off the tabs: no hover state.
  await page.mouse.move(1, 699);
}

async function open(page, width) {
  await page.setViewportSize({ width, height: 700 });
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto(VANILLA);
  await page.waitForFunction(() => document.body.dataset.ready === 'true');
  await page.evaluate(() => window.editor.destroy());
}

/** The styles of a hovered part. */
async function hovered(page, part) {
  await page.locator(`#host ${PARTS[part]}`).hover();
  const style = await page.locator(`#host ${PARTS[part]}`).evaluate((element) => {
    const style = getComputedStyle(element);
    return { color: style.color, background: style.backgroundColor };
  });
  await page.mouse.move(1, 699);
  return style;
}

/**
 * The parts whose size the preset fixes, whatever the fonts: the close and
 * add buttons' hit areas, and the bars' heights. Every other box depends
 * on how wide the machine's fonts lay the text out, so the defaults test
 * leaves it out.
 */
const FIXED = {
  close: 'size',
  'dirty close': 'size',
  add: 'size',
  strip: 'height',
  tab: 'height',
  'active tab': 'height',
  'read-only tab': 'height',
  'active tab bar': 'height',
  toolbar: 'height',
  footer: 'height',
};

/** A part's measure without its font-dependent box: its fixed size, if any, then its styles. */
function fontFree(part, { box, size, ...styles }) {
  const fixed = {};
  if (FIXED[part] === 'size') fixed.size = box.split(',').slice(2).join('x');
  if (FIXED[part] === 'height') fixed.height = box ? box.split(',')[3] : size.split(' ')[1];
  return { ...fixed, ...styles };
}

/** Everything the defaults test compares, per width and theme. */
async function snapshot(page) {
  const out = {};
  for (const [label, width] of Object.entries(WIDTHS)) {
    await open(page, width);
    await mount(page);
    for (const theme of THEMES) {
      await page.evaluate((theme) => window.hosted.update({ theme }), theme);
      await expect(page.locator('#host .config-editor-documents')).toHaveAttribute('data-color-theme', theme);
      const measured = await measure(page);
      const seen = {
        ...Object.fromEntries(Object.entries(measured).map(([part, entry]) => [part, entry && fontFree(part, entry)])),
        'hovered close': await hovered(page, 'close'),
        'hovered add': await hovered(page, 'add'),
      };
      // Every style in the first theme; the colors alone in the others
      // (the rest is the same in every theme).
      out[`${label} ${theme}`] = Object.fromEntries(
        Object.entries(seen).map(([part, styles]) => [
          part,
          styles &&
            Object.entries(styles)
              .filter(([prop]) => theme === THEMES[0] || /color|background/.test(prop))
              .map(([prop, value]) => `${prop}: ${value}`)
              .join('; '),
        ]),
      );
    }
  }
  return out;
}

test.describe('theming the chrome', () => {
  test.skip(!built, 'the npm package is not built: cd js && npm run build');

  test('with no theming, every part computes the styles it did before the variables', async ({ page }) => {
    test.setTimeout(120_000);
    const now = await snapshot(page);
    if (process.env.RECORD_CHROME_DEFAULTS) {
      writeFileSync(DEFAULTS, `${JSON.stringify(now, null, 1)}\n`);
      return;
    }
    const before = JSON.parse(readFileSync(DEFAULTS, 'utf8'));
    expect(Object.keys(now)).toEqual(Object.keys(before));
    for (const [where, parts] of Object.entries(before)) {
      for (const [part, styles] of Object.entries(parts)) expect(now[where][part], `${where}: ${part}`).toEqual(styles);
    }
  });

  for (const [reset, file] of Object.entries(RESETS)) {
    for (const [label, width] of Object.entries(WIDTHS)) {
      test(`${reset} on the page leaves the chrome laid out as without it (${label})`, async ({ page }) => {
        await open(page, width);
        await mount(page);
        const plain = await measure(page);
        await page.evaluate(() => {
          window.hosted.destroy();
          document.getElementById('host').remove();
        });
        await page.addStyleTag({ content: resetSheet(file) });
        await mount(page);
        const styled = await measure(page);
        // What the reset may change: nothing the chrome draws.
        for (const part of Object.keys(PARTS)) expect(styled[part], `${reset}: ${part}`).toEqual(plain[part]);
        // The parts that make it work, spelled out.
        const [, , closeWidth, closeHeight] = styled.close.box.split(',').map(Number);
        expect([closeWidth, closeHeight], 'the close button keeps its 24×24 hit area').toEqual([24, 24]);
        expect(styled.tab['padding-left']).toBe(label === 'phone' ? '8px' : '10px');
        expect(Number(styled.tab.box.split(',')[3])).toBe(35);
        expect(Number(styled.toolbar.box.split(',')[3])).toBeGreaterThanOrEqual(36);
        expect(styled.toolbar.display).toBe('flex');
        expect(styled.footer.display).toBe('flex');
      });
    }
  }

  test('plain host CSS on the variables restyles the tabs, the toolbar and the footer', async ({ page }) => {
    await open(page, 1200);
    await page.addStyleTag({
      content: `
        .config-editor-documents {
          --config-editor-tab-padding: 0 12px 0 20px;
          --config-editor-tab-gap: 9px;
          --config-editor-tab-font-size: 15px;
          --config-editor-tab-font-family: serif;
          --config-editor-tab-active-font-weight: 700;
          --config-editor-tab-readonly-font-style: normal;
          --config-editor-tab-radius: 5px 5px 0 0;
          --config-editor-tab-border-width: 3px;
          --config-editor-tab-bar-radius: 0;
          --config-editor-tab-close-radius: 12px;
          --config-editor-tab-bg: rgb(1, 2, 3);
          --config-editor-tab-active-bg: rgb(4, 5, 6);
        }
        .config-editor {
          --config-editor-toolbar-padding: 0 30px;
          --config-editor-toolbar-gap: 20px;
          --config-editor-toolbar-font-size: 14px;
          --config-editor-toolbar-font-weight: 600;
          --config-editor-toolbar-border-width: 2px;
          --config-editor-toolbar-bg: rgb(7, 8, 9);
          --config-editor-footer-padding: 0 25px;
          --config-editor-footer-gap: 16px;
          --config-editor-footer-font-size: 11px;
          --config-editor-footer-font-style: italic;
          --config-editor-footer-border-width: 2px;
          --config-editor-footer-bg: rgb(10, 11, 12);
          --config-editor-footer-text: rgb(200, 201, 202);
        }`,
    });
    await mount(page);
    const seen = await measure(page);
    expect([seen.tab['padding-left'], seen.tab['padding-right'], seen.tab['column-gap']]).toEqual(['20px', '12px', '9px']);
    expect([seen.tab['font-size'], seen.tab['font-family'], seen.tab['background-color']]).toEqual(['15px', 'serif', 'rgb(1, 2, 3)']);
    expect([seen.tab['border-top-left-radius'], seen.tab['border-right-width']]).toEqual(['5px', '3px']);
    expect([seen['active tab']['font-weight'], seen['active tab']['background-color']]).toEqual(['700', 'rgb(4, 5, 6)']);
    expect(seen['read-only name']['font-style']).toBe('normal');
    expect(seen.strip['border-top-left-radius']).toBe('0px');
    expect(seen.close['border-top-left-radius']).toBe('12px');
    expect(seen.close.box.split(',').slice(2)).toEqual(['24', '24']);
    expect([seen.toolbar['padding-left'], seen.toolbar['column-gap'], seen.toolbar['font-size'], seen.toolbar['font-weight']])
      .toEqual(['30px', '20px', '14px', '600']);
    expect([seen.toolbar['border-bottom-width'], seen.toolbar['background-color']]).toEqual(['2px', 'rgb(7, 8, 9)']);
    expect([seen.footer['padding-left'], seen.footer['column-gap'], seen.footer['font-size'], seen.footer['font-style']])
      .toEqual(['25px', '16px', '11px', 'italic']);
    expect([seen.footer['border-top-width'], seen.footer['background-color'], seen.footer.color])
      .toEqual(['2px', 'rgb(10, 11, 12)', 'rgb(200, 201, 202)']);
  });

  test('a host rule in its own unlayered CSS on a chrome class wins over the cosmetic layer', async ({ page }) => {
    await open(page, 1200);
    // Lower in weight than the layered hover rules, and loaded before the
    // editor's stylesheet: the layer still loses.
    await page.addStyleTag({
      content: `
        .config-editor-documents__close:hover { --config-editor-tab-close-bg: rgb(0, 128, 0); }
        .config-editor-documents__add:hover { --config-editor-tab-add-bg: rgb(0, 0, 128); --config-editor-tab-add-text: rgb(255, 255, 0); }
        .config-editor-documents__tab:hover { --config-editor-tab-bg: rgb(128, 0, 0); }`,
    });
    await mount(page);
    expect((await hovered(page, 'close')).background).toBe('rgb(0, 128, 0)');
    expect(await hovered(page, 'add')).toEqual({ color: 'rgb(255, 255, 0)', background: 'rgb(0, 0, 128)' });
    expect((await hovered(page, 'tab')).background).toBe('rgb(128, 0, 0)');
    // The editor's own hover effects, from the layer, without the host's.
    await page.evaluate(() => {
      window.hosted.destroy();
      document.getElementById('host').remove();
      for (const sheet of document.querySelectorAll('style:not([data-config-editor])')) {
        if (sheet.textContent.includes('rgb(0, 128, 0)')) sheet.remove();
      }
    });
    await mount(page);
    const raised = await page.locator('#host .config-editor-documents').evaluate((root) => {
      const probe = document.createElement('span');
      probe.style.backgroundColor = 'var(--config-editor-raised-hover)';
      root.append(probe);
      const color = getComputedStyle(probe).backgroundColor;
      probe.remove();
      return color;
    });
    expect((await hovered(page, 'close')).background).toBe(raised);
    const layers = await page.evaluate(() =>
      [...document.styleSheets].flatMap((sheet) => {
        try {
          return [...sheet.cssRules].filter((rule) => rule instanceof CSSLayerBlockRule).map((rule) => rule.name);
        } catch {
          return [];
        }
      }),
    );
    expect(layers).toContain('config-editor.chrome');
  });

  test('the chrome and colors settings set the variables, live', async ({ page }) => {
    await open(page, 1200);
    await mount(page, {
      chrome: { 'tab-padding': '0 14px 0 18px', 'tab-font-size': '13px', 'toolbar-padding': '0 22px', 'footer-font-size': '12px' },
      colors: { 'toolbar-bg': 'rgb(20, 21, 22)', 'footer-border': 'rgb(30, 31, 32)', 'tab-close-hover-bg': 'rgb(40, 41, 42)' },
    });
    let seen = await measure(page);
    expect([seen.tab['padding-left'], seen.tab['padding-right'], seen.tab['font-size']]).toEqual(['18px', '14px', '13px']);
    expect([seen.toolbar['padding-left'], seen.toolbar['background-color']]).toEqual(['22px', 'rgb(20, 21, 22)']);
    expect([seen.footer['font-size'], seen.footer['border-top-color']]).toEqual(['12px', 'rgb(30, 31, 32)']);
    expect((await hovered(page, 'close')).background).toBe('rgb(40, 41, 42)');
    await page.evaluate(() => window.hosted.update({ chrome: { 'tab-padding': '0 4px 0 30px', 'tab-font-size': null } }));
    seen = await measure(page);
    expect([seen.tab['padding-left'], seen.tab['font-size'], seen.toolbar['padding-left']]).toEqual(['30px', '12px', '22px']);
    const errors = await page.evaluate(() => {
      const attempt = (chrome) => {
        try {
          window.hosted.update({ chrome });
          return null;
        } catch (error) {
          return error.message;
        }
      };
      return [attempt({ 'tab-padding': 'red' }), attempt({ 'tab-pading': '0' }), attempt([1])];
    });
    expect(errors[0]).toContain('chrome: "tab-padding": "red" is not a padding');
    expect(errors[1]).toContain('unknown chrome token "tab-pading" (did you mean "tab-padding"?)');
    expect(errors[2]).toContain('chrome must be an object of token: CSS value');
  });
});
