import { expect, test } from '@playwright/test';
import { setupEditor } from './helpers.mjs';

// Every built-in theme keeps its text readable: the theme's own variables,
// read off the editor in each theme, measured with the WCAG contrast ratio.
// Text in the code is held to 4.5:1, the editor's own interface text (line
// numbers, labels, chips, icons) to 3:1, and a disabled control looks
// disabled (clearly dimmer than when enabled) while staying readable.
//
// Each pair is [what, foreground, background (bottom layer first)]; a
// translucent layer is composited over the ones under it.
const CODE = 4.5;
const UI = 3;
const PAIRS = [
  ...['text', 'key', 'string', 'literal', 'value', 'punctuation', 'comment'].map((name) => [
    `code ${name}`,
    CODE,
    name,
    ['bg'],
  ]),
  ['read-only value', CODE, 'locked', ['bg']],
  ['line number', UI, 'line-number', ['bg']],
  ['fold toggle', UI, 'fold-toggle', ['bg']],
  ['fold chip', UI, 'fold-chip-text', ['bg', 'fold-chip']],
  ['muted on panel (footer, tabs)', UI, 'muted', ['panel']],
  ['tab text', UI, 'tab-text', ['tab-bar']],
  ['toolbar text', CODE, 'toolbar-text', ['panel']],
  ['popup text', CODE, 'raised-text', ['raised']],
  ['popup muted', UI, 'raised-muted', ['raised']],
  ['completion kind icon', UI, 'completion-icon-text', ['raised']],
  ['completion type', UI, 'completion-type', ['raised']],
  ['completion detail', UI, 'completion-detail', ['raised']],
  ['completion key, selected', CODE, 'completion-key', ['raised', 'completion-active']],
  ['matched letters, selected row', UI, 'completion-match-active', ['raised', 'completion-active']],
  ['matched letters', UI, 'accent', ['raised']],
  ['control text', CODE, 'control-text', ['control-bg']],
  ['disabled control text', 2.5, 'control-disabled-text', ['control-bg']],
  ['footer error count', UI, 'status-error', ['panel']],
  // Outlines and markers (WCAG's 3:1 for interface parts).
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
        probe.style.color = '';
        probe.style.color = `var(--config-editor-${name})`;
        const computed = getComputedStyle(probe).color;
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
