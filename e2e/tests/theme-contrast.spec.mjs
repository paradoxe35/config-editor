import { expect, test } from '@playwright/test';
import { setupEditor } from './helpers.mjs';

// Every built-in theme keeps its text readable: the theme's own variables,
// read off the editor in each theme, measured with the WCAG contrast ratio.
// Text is held to 4.5:1 (QA 21: code, line numbers and the selected
// suggestion's type were 3.4 to 4.4 in some themes; QA 24: tab names, the
// hover card's link, preview code, kind icons, code on tinted lines and the
// empty documents view's text),
// the editor's other interface parts (glyphs, outlines) to 3:1, and a
// disabled control looks disabled (clearly dimmer than when enabled) while
// staying readable.
//
// Each pair is [what, foreground, background (bottom layer first)], each a
// theme variable or a `.class` (the color its rule gives; `.a .b`: an
// element `.b` in an element `.a`); a translucent layer is composited over
// the ones under it.
const CODE = 4.5;
const UI = 3;
const TOKENS = ['text', 'key', 'string', 'literal', 'value', 'punctuation', 'comment', 'locked'];
// A line's tints, alone and under the active line's (bottom layer first).
const TINTS = ['active-line', 'error-line', 'added-line', 'modified-line'];
const LINES = [
  ['', ['bg']],
  ...TINTS.map((tint) => [` on the ${tint}`, ['bg', tint]]),
  ...TINTS.slice(1).map((tint) => [` on the active ${tint}`, ['bg', tint, 'active-line']]),
];
const PAIRS = [
  ...TOKENS.flatMap((name) => LINES.map(([where, layers]) => [`code ${name}${where}`, CODE, name, layers])),
  ['line number', CODE, 'line-number', ['bg']],
  // A changed or broken line's number, as its rule colors it (QA 23:
  // Solarized Light's added lines were 2.97:1).
  ['line number, added line', CODE, '.config-editor__line-number-added', ['bg']],
  ['line number, modified line', CODE, '.config-editor__line-number-modified', ['bg']],
  ['line number, error line', CODE, '.config-editor__line-number-error', ['bg']],
  ['fold toggle', UI, 'fold-toggle', ['bg']],
  ['fold chip', UI, 'fold-chip-text', ['bg', 'fold-chip']],
  ['muted on panel (footer, tabs)', UI, 'muted', ['panel']],
  ['tab name', CODE, '.config-editor-documents__tab', ['tab-bar', 'tab-bg']],
  ['tab name, selected', CODE, '.config-editor-documents__tab-active', ['tab-bar', 'tab-active-bg']],
  ['add tab glyph', UI, 'tab-text', ['tab-bar']],
  // "No open files" in a documents view with none.
  ['no documents text', CODE, '.config-editor-documents__empty', ['bg']],
  ['toolbar text', CODE, 'toolbar-text', ['panel']],
  ['edit-as notice text', CODE, '.config-editor__edit-as-notice', ['panel']],
  ['popup text', CODE, 'raised-text', ['raised']],
  ['popup muted', UI, 'raised-muted', ['raised']],
  // The hover card's "deprecated" badge (QA 23: 2.1:1 in light themes).
  ['deprecated badge on the hover card', CODE, 'status-warning', ['raised']],
  ['completion kind icon', CODE, 'completion-icon-text', ['raised']],
  ['completion kind icon, selected', CODE, 'completion-icon-text', ['raised', 'completion-active']],
  ['completion key', CODE, 'completion-key', ['raised']],
  ['deprecated completion key', CODE, '.config-editor__completion--deprecated .config-editor__completion-key', ['raised']],
  ['deprecated completion key, selected', CODE, '.config-editor__completion--deprecated .config-editor__completion-key', ['raised', 'completion-active']],
  ['completion type', CODE, 'completion-type', ['raised']],
  ['completion detail', CODE, 'completion-detail', ['raised']],
  // The hover card's Documentation link, the tips' text and the change
  // preview's code.
  ['hover card Documentation link', CODE, '.config-editor__hover-link', ['raised']],
  ['problem tip text', CODE, 'tooltip-text', ['raised']],
  ['problem tip text, current entry', CODE, 'tooltip-text', ['raised', 'raised-hover']],
  ['problem tip detail', CODE, '.config-editor__problem-tip-detail', ['raised']],
  ['problem tip detail, current entry', CODE, '.config-editor__problem-tip-detail', ['raised', 'raised-hover']],
  ['problem tip link', CODE, '.config-editor__problem-tip-link', ['raised']],
  ['problem tip link, current entry', CODE, '.config-editor__problem-tip-link', ['raised', 'raised-hover']],
  ['preview removed code', CODE, 'diff-removed-text', ['raised', 'diff-removed-bg']],
  ['preview current code', CODE, 'diff-current-text', ['raised', 'diff-current-bg']],
  ['completion key, selected', CODE, 'completion-key', ['raised', 'completion-active']],
  ['completion type, selected', CODE, 'completion-type-active', ['raised', 'completion-active']],
  ['matched letters, selected row', CODE, 'completion-match-active', ['raised', 'completion-active']],
  ['matched letters', CODE, '.config-editor__completion-match', ['raised']],
  ['control text', CODE, 'control-text', ['control-bg']],
  ['disabled control text', 2.5, 'control-disabled-text', ['control-bg']],
  // The footer's small text (its counts, the detail after them and the
  // separators) is text, held to 4.5:1 like the code.
  ['footer text (detail, separators)', CODE, 'raised-muted', ['panel']],
  ['footer error count', CODE, 'status-error', ['panel']],
  ['footer warning count', CODE, 'status-warning', ['panel']],
  ['footer info count', CODE, 'status-info', ['panel']],
  ['footer hint count', CODE, 'status-hint', ['panel']],
  ['footer changes count', CODE, 'status-changes', ['panel']],
  // Outlines and markers (WCAG's 3:1 for interface parts).
  ['schema check mark', UI, '.config-editor__schema-check', ['panel']],
  ['focus ring', UI, 'focus-ring', ['bg']],
  ['change bar focus ring', UI, 'diff-focus-ring', ['bg']],
];

/** Every theme's variables, as computed colors, keyed by theme. */
async function themeColors(page, names) {
  const themes = await page.locator('.demo__theme-select option').evaluateAll((options) =>
    options.map((option) => option.value),
  );
  const all = {};
  for (const theme of themes) {
    await page.locator('.demo__theme-select').selectOption(theme);
    all[theme] = await page.locator('.config-editor__frame').first().evaluate((frame, wanted) => {
      // Resolve each variable to rgba through a probe's computed color.
      const probe = document.createElement('span');
      frame.append(probe);
      const colors = {};
      for (const name of wanted) {
        // A variable, or (`.class`) the color a class's rule gives.
        const rule = name.startsWith('.');
        const classes = rule ? name.split(' ').map((part) => part.slice(1)) : [''];
        probe.replaceChildren();
        probe.className = classes[0];
        let inner = probe;
        for (const nested of classes.slice(1)) {
          const child = document.createElement('span');
          child.className = nested;
          inner.append(child);
          inner = child;
        }
        probe.style.color = '';
        if (!rule) probe.style.color = `var(--config-editor-${name})`;
        const computed = getComputedStyle(inner).color;
        const srgb = computed.match(/color\(srgb ([\d.e-]+) ([\d.e-]+) ([\d.e-]+)(?: \/ ([\d.]+))?\)/);
        const rgb = computed.match(/rgba?\(([\d.]+), ([\d.]+), ([\d.]+)(?:, ([\d.]+))?\)/);
        colors[name] = srgb
          ? [...srgb.slice(1, 4).map((part) => Number(part) * 255), Number(srgb[4] ?? 1)]
          : rgb
            ? [...rgb.slice(1, 4).map(Number), Number(rgb[4] ?? 1)]
            : computed;
      }
      probe.remove();
      return colors;
    }, names);
  }
  return all;
}

const over = (top, under) => top.slice(0, 3).map((part, i) => part * top[3] + under[i] * (1 - top[3]));
const luminance = (rgb) =>
  rgb
    .map((part) => part / 255)
    .map((part) => (part <= 0.03928 ? part / 12.92 : ((part + 0.055) / 1.055) ** 2.4))
    .reduce((sum, part, i) => sum + part * [0.2126, 0.7152, 0.0722][i], 0);
const ratio = (a, b) => {
  const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (light + 0.05) / (dark + 0.05);
};

/** `foreground` over the `layers` (bottom first) of `colors`. */
function contrast(colors, foreground, layers) {
  const back = layers.reduce((under, layer) => over(colors[layer], under), [255, 255, 255]);
  return ratio(over(colors[foreground], back), back);
}

test.describe('theme contrast', () => {
  setupEditor(test);

  test('every theme keeps code, interface and disabled text readable', async ({ page }) => {
    const names = [
      ...new Set([...PAIRS.flatMap(([, , foreground, layers]) => [foreground, ...layers]), 'diff-peek-divider', 'raised-border']),
    ];
    const all = await themeColors(page, names);
    expect(Object.keys(all).length).toBeGreaterThan(10);
    const failures = [];
    for (const [theme, colors] of Object.entries(all)) {
      for (const [what, minimum, foreground, layers] of PAIRS) {
        const measured = contrast(colors, foreground, layers);
        if (!(measured >= minimum)) failures.push(`${theme}: ${what} ${measured.toFixed(2)} < ${minimum}`);
      }
      // The change preview's dividers are no heavier than its border.
      const divider = contrast(colors, 'diff-peek-divider', ['raised']);
      const border = contrast(colors, 'raised-border', ['raised']);
      if (!(divider <= border + 0.01)) {
        failures.push(`${theme}: change preview divider ${divider.toFixed(2)} is heavier than its border ${border.toFixed(2)}`);
      }
      // Disabled looks disabled: well below the enabled text's contrast.
      const enabled = contrast(colors, 'control-text', ['control-bg']);
      const disabled = contrast(colors, 'control-disabled-text', ['control-bg']);
      if (!(enabled / disabled >= 1.6)) {
        failures.push(`${theme}: disabled ${disabled.toFixed(2)} is too close to enabled ${enabled.toFixed(2)}`);
      }
    }
    expect(failures, failures.join('\n')).toEqual([]);
  });
});
