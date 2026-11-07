// Takes the screenshots in docs/screenshots from the demo and the JS example
// pages, with real keys and mouse, so they can be refreshed after any UI
// change:
//
//   trunk build --release && (cd js && npm run build)
//   cd e2e && node screenshots.mjs            # every scene
//   cd e2e && node screenshots.mjs folding    # the scenes whose name matches
//   cd e2e && node screenshots.mjs --gallery  # only docs/screenshots/README.md
//   cd e2e && node screenshots.mjs --check    # fail if that page is stale (CI)
//
// The gallery page is written from screenshots-gallery.mjs after each run.
//
// Each scene is 1200 CSS pixels wide at device scale 2, like the README's.

import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { galleryFiles, galleryMarkdown } from './screenshots-gallery.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const out = join(here, '..', 'docs', 'screenshots');
const demo = pathToFileURL(join(here, '.standalone', 'index.html')).href;
const example = (name) => pathToFileURL(join(here, '..', 'js', 'examples', name)).href;
const pause = (page, ms = 250) => page.waitForTimeout(ms);

const SYMBOLS = [
  'https://fonts.gstatic.com/s/notosansmath/v19/7Aump_cpkSecTWaHRlH2hyV5UHkG.ttf',
  'https://fonts.gstatic.com/s/notosanssymbols2/v25/I_uyMoGduATTei9eI8daxVHDyfisHr71ypM.ttf',
];

/**
 * Prepares a loaded page for a screenshot: arrows and shapes (`↗`, `▸`) the
 * machine's fonts lack come from a symbols font, under the name of each
 * font the page asks for first, limited to those characters (nothing else
 * changes); the page behind the editor takes the editor's background.
 */
async function ready(page) {
  const families = await page.evaluate(() => {
    const first = new Set();
    for (const element of document.querySelectorAll('*')) {
      for (const pseudo of [null, '::before', '::after']) {
        const family = getComputedStyle(element, pseudo).fontFamily.split(',')[0].trim().replace(/^["']|["']$/g, '');
        if (family) first.add(family);
      }
    }
    return [...first];
  });
  const faces = families
    .map((family) => `@font-face { font-family: "${family}"; src: ${SYMBOLS.map((url) => `url(${url})`).join(', ')}; unicode-range: U+2190-21FF, U+2300-23FF, U+25A0-25FF, U+2700-27BF; }`)
    .join('\n');
  await page.addStyleTag({ content: faces });
  await page.evaluate(async () => {
    const editor = document.querySelector('.config-editor__frame');
    if (editor) {
      const background = getComputedStyle(editor).backgroundColor;
      document.documentElement.style.background = background;
      document.body.style.background = background;
    }
    await document.fonts.ready;
  });
  await pause(page, 300);
}

/** Opens the demo with `query` and waits for the editor. */
async function open(page, query = '') {
  await page.goto(`${demo}${query}`);
  await page.locator('.config-editor__input').first().waitFor();
  await ready(page);
}

/** The caret at the end of the document, on a new line. */
async function newLineAtEnd(page) {
  await page.locator('.config-editor__input').first().click();
  await page.keyboard.press('Control+End');
}

/** Moves the caret to `line` (one-based) with go to line, then closes it. */
async function caretTo(page, line, column = 1) {
  await page.keyboard.press('Control+g');
  await page.keyboard.type(`${line}:${column}`);
  await page.keyboard.press('Enter');
  await pause(page, 150);
}

/** Hovers the middle of the first element matching `selector`. */
async function hover(page, selector) {
  const box = await page.locator(selector).first().boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await pause(page, 700);
}

/** Hovers the text `text` on the editor's highlighted rows. */
async function hoverText(page, text) {
  const box = await page
    .locator('.config-editor__highlight .config-editor__code-line')
    .filter({ hasText: text })
    .first()
    .boundingBox();
  const line = await page.locator('.config-editor__highlight .config-editor__code-line').filter({ hasText: text }).first().textContent();
  const width = await page.evaluate(() => {
    const probe = document.querySelector('.config-editor__input');
    const style = getComputedStyle(probe);
    const canvas = document.createElement('canvas').getContext('2d');
    canvas.font = `${style.fontSize} ${style.fontFamily}`;
    return canvas.measureText('M').width;
  });
  const column = line.indexOf(text) + text.length / 2;
  const padding = await page.evaluate(() => parseFloat(getComputedStyle(document.querySelector('.config-editor__input')).paddingLeft));
  const input = await page.locator('.config-editor__input').first().boundingBox();
  await page.mouse.move(input.x + padding + column * width, box.y + box.height / 2);
  await pause(page, 900);
}

/** Saves the page from the top down to the bottom of `selector`. */
async function shot(page, name, selector = '.demo', from = null) {
  const box = await page.locator(selector).first().boundingBox();
  const top = from ? Math.floor((await page.locator(from).first().boundingBox()).y) : 0;
  const height = Math.ceil(box.y + box.height) - top;
  // A crop to the wrong element (a selector that matches something else
  // first) leaves a strip: fail instead of saving it.
  if (height < 150) throw new Error(`${name}: ${selector} gives a ${height}px tall image`);
  await page.screenshot({ path: join(out, `${name}.png`), clip: { x: 0, y: top, width: 1200, height } });
  console.log(`docs/screenshots/${name}.png`);
}

/** The demo with its "Saved value" row, as the README shows it. */
const demoShot = (page, name) => shot(page, name, '.demo__output > .demo__summary');

const scenes = {
  // The README's six.
  async completion(page) {
    await open(page);
    await newLineAtEnd(page);
    await page.keyboard.type('li');
    await page.locator('.config-editor__completion').first().waitFor();
    await pause(page);
    await demoShot(page, 'completion');
  },
  async diagnostics(page) {
    await open(page);
    await newLineAtEnd(page);
    await page.keyboard.type('limits:\n');
    await page.keyboard.type('max_pending: lots');
    await page.keyboard.press('Escape');
    await pause(page, 900);
    await hoverText(page, 'lots');
    await demoShot(page, 'diagnostics');
  },
  async hover(page) {
    await open(page);
    await hoverText(page, 'public_address');
    await demoShot(page, 'hover');
  },
  async 'edit-as'(page) {
    await open(page, '?edit=toml');
    await demoShot(page, 'edit-as');
  },
  async 'edit-as-notice'(page) {
    await open(page, '?edit=toml');
    await page.locator('.config-editor__input').first().click();
    await page.keyboard.press('Control+Home');
    await page.keyboard.type('# a note typed in TOML');
    await page.keyboard.press('Enter');
    await page.locator('.config-editor__edit-as-notice').filter({ hasText: 'Comments typed in TOML' }).waitFor();
    await pause(page, 400);
    await demoShot(page, 'edit-as-notice');
  },
  async diff(page) {
    await open(page, '?language=json');
    await page.locator('.config-editor__input').first().click();
    await caretTo(page, 2, 15);
    await page.keyboard.press('Shift+End');
    await page.keyboard.press('ArrowLeft');
    await page.keyboard.press('Shift+Home');
    await caretTo(page, 2, 13);
    for (let index = 0; index < 4; index += 1) await page.keyboard.press('Delete');
    await page.keyboard.type('9000');
    await pause(page, 400);
    await page.locator('.config-editor__diff-marker').first().click();
    await page.locator('.config-editor__diff-peek').waitFor();
    await pause(page);
    await demoShot(page, 'diff');
  },
  async 'light-theme'(page) {
    await open(page, '?language=toml');
    await page.selectOption('.demo__theme-select', 'vscode-light');
    await ready(page);
    await demoShot(page, 'light-theme');
  },

  // One per feature, for the gallery.
  async 'find-replace'(page) {
    await open(page, '?sample=large');
    await page.locator('.config-editor__input').first().click();
    await page.keyboard.press('Control+h');
    await page.keyboard.type('timeout');
    await page.locator('.config-editor__find-input').nth(1).click();
    await page.keyboard.type('deadline');
    await pause(page, 400);
    await demoShot(page, 'find-replace');
  },
  async 'go-to-line'(page) {
    await open(page, '?sample=large');
    await page.locator('.config-editor__input').first().click();
    await page.keyboard.press('Control+g');
    await page.keyboard.type('120:5');
    await pause(page, 300);
    await demoShot(page, 'go-to-line');
  },
  async folding(page) {
    await open(page, '?sample=large&language=yaml');
    await page.locator('.config-editor__input').first().click();
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+0');
    await pause(page, 400);
    await caretTo(page, 1);
    await hover(page, '.config-editor__gutter');
    await demoShot(page, 'folding');
  },
  async 'read-only'(page) {
    await open(page, '?schema=protected&lock=/name');
    await page.locator('.config-editor__input').first().click();
    await caretTo(page, 2, 7);
    await page.keyboard.type('x');
    await pause(page, 300);
    await demoShot(page, 'read-only');
  },
  async secrets(page) {
    await open(page, '?schema=protected');
    await hover(page, '.config-editor__secret');
    await demoShot(page, 'secrets');
  },
  async 'host-diagnostics'(page) {
    await page.goto(example('vanilla.html'));
    await page.locator('.config-editor__input').first().waitFor();
    await ready(page);
    await page.evaluate(() =>
      window.editor.update({ hostDiagnostics: [
        { path: '/listen', severity: 'error', message: 'port 7835 is already used by another tunnel', source: 'burrow', code: 'E_PORT', docsUrl: 'https://example.com/docs/ports' },
        { path: '/public_address', severity: 'info', message: 'resolves to 203.0.113.7', source: 'burrow' },
        { path: '/insecure_plaintext', severity: 'hint', message: 'TLS is recommended in production', source: 'burrow' },
      ] }),
    );
    await pause(page, 500);
    await hoverText(page, '7835');
    await shot(page, 'host-diagnostics', '#editor', '#editor');
  },
  async changes(page) {
    await open(page, '?sample=large');
    await page.locator('.config-editor__input').first().click();
    await caretTo(page, 3);
    await page.keyboard.press('End');
    await page.keyboard.type('  # changed');
    await caretTo(page, 12, 1);
    await page.keyboard.press('Shift+ArrowDown');
    await page.keyboard.press('Delete');
    await pause(page, 400);
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('Alt+F3');
    await page.locator('.config-editor__diff-peek').waitFor();
    await pause(page);
    await demoShot(page, 'changes');
  },
  async 'server-change'(page) {
    await open(page);
    await newLineAtEnd(page);
    await page.keyboard.type('limits:\n');
    await page.keyboard.type('max_pending: 10');
    await page.locator('button:has-text("Server change")').first().click();
    await page.locator('.config-editor__notice').waitFor();
    await pause(page);
    await demoShot(page, 'server-change');
  },
  async documents(page) {
    await page.goto(`${demo}?documents`);
    await page.locator('.config-editor__input').first().waitFor();
    await ready(page);
    const tabs = page.getByRole('tab');
    await tabs.nth(1).click();
    await page.locator('.config-editor__input:visible').click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('\n# edited');
    await tabs.nth(0).click();
    await pause(page, 400);
    await shot(page, 'documents', '.config-editor-documents', '.config-editor-documents');
  },
  async 'schema-help'(page) {
    await open(page, '?schema=help');
    await page.locator('.config-editor__input').first().click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('ti');
    await page.locator('.config-editor__completion').first().waitFor();
    await pause(page, 400);
    await demoShot(page, 'schema-help');
  },
  async deprecated(page) {
    await open(page, '?schema=help');
    await page.locator('.config-editor__input').first().click();
    await page.keyboard.type('timeout: 5');
    await page.keyboard.press('Escape');
    await pause(page, 900);
    await hoverText(page, 'timeout');
    await demoShot(page, 'deprecated');
  },
  async 'schema-docs'(page) {
    await open(page, '?schema=help');
    await page.locator('.config-editor__input').first().click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('mode: fast');
    await page.keyboard.press('Escape');
    await pause(page, 500);
    await hoverText(page, 'fast');
    await demoShot(page, 'schema-docs');
  },
  async 'tab-focus'(page) {
    await open(page);
    await page.locator('.config-editor__input').first().click();
    await page.keyboard.press('Control+m');
    await pause(page, 400);
    await demoShot(page, 'tab-focus');
  },
  async phone(page) {
    await page.setViewportSize({ width: 390, height: 640 });
    await open(page);
    await newLineAtEnd(page);
    await page.keyboard.type('li');
    await page.locator('.config-editor__completion').first().waitFor();
    await pause(page);
    const box = await page.locator('.demo__output > .demo__summary').first().boundingBox();
    await page.screenshot({ path: join(out, 'phone.png'), clip: { x: 0, y: 0, width: 390, height: Math.ceil(box.y + box.height) } });
    console.log('docs/screenshots/phone.png');
  },
  async themes(page) {
    for (const [theme, language] of [['dracula', 'hcl'], ['nord', 'xml'], ['github-light', 'json'], ['solarized-light', 'yaml']]) {
      await open(page, `?language=${language}`);
      await page.selectOption('.demo__theme-select', theme);
      await ready(page);
      await demoShot(page, `theme-${theme}`);
    }
  },
};

const flags = process.argv.slice(2).filter((arg) => arg.startsWith('--'));
const only = process.argv.slice(2).filter((arg) => !arg.startsWith('--'));
const page = join(out, 'README.md');

/** Problems with the gallery: missing images, images it leaves out, a stale page. */
function galleryProblems() {
  const listed = galleryFiles();
  const present = readdirSync(out).filter((file) => file.endsWith('.png'));
  return [
    ...listed.filter((file) => !present.includes(file)).map((file) => `${file} is in the gallery but missing`),
    ...present.filter((file) => !listed.includes(file)).map((file) => `${file} is not in screenshots-gallery.mjs`),
    ...(existsSync(page) && readFileSync(page, 'utf8') === galleryMarkdown()
      ? []
      : ['docs/screenshots/README.md is out of date: run node screenshots.mjs --gallery']),
  ];
}

if (flags.includes('--check')) {
  const problems = galleryProblems();
  for (const problem of problems) console.error(problem);
  process.exit(problems.length ? 1 : 0);
}

let failed = 0;
if (!flags.includes('--gallery')) {
  const { chromium } = await import('@playwright/test');
  const { default: globalSetup } = await import('./global-setup.mjs');
  mkdirSync(out, { recursive: true });
  globalSetup();
  const browser = await chromium.launch({
    executablePath: process.env.CHROME_PATH || undefined,
    args: ['--no-sandbox', '--disable-gpu', '--allow-file-access-from-files'],
  });
  for (const [name, scene] of Object.entries(scenes)) {
    if (only.length && !only.some((pattern) => name.includes(pattern))) continue;
    const tab = await browser.newPage({ viewport: { width: 1200, height: 700 }, deviceScaleFactor: 2 });
    try {
      await scene(tab);
    } catch (error) {
      failed += 1;
      console.error(`${name}: ${error.message.split('\n')[0]}`);
    } finally {
      await tab.close();
    }
  }
  await browser.close();
}
writeFileSync(page, galleryMarkdown());
for (const problem of galleryProblems()) {
  failed += 1;
  console.error(problem);
}
process.exit(failed ? 1 : 0);
