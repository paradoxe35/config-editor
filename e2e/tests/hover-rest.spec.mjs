import { expect, test } from '@playwright/test';
import { editor, pointAt, setupEditor } from './helpers.mjs';

// The hover card and the problem tip open once the pointer rests (the
// `hoverDelay` setting, 300 ms by default), not a fixed time after it came
// onto a key: a hand passing slowly across the text opens nothing. From one
// popup to the next is quick; the popups fade in without delaying the
// pointer; more contrast draws their edges in their text color.

// Comment lines first, so a card opens above its key and leaves the next
// lines free.
const DOC = '#\n#\n#\n#\n#\n#\n#\nlisten: 7835\npublic_address: example.com\ninsecure_plaintext: maybe\n';
const LISTEN = 7;
const ADDRESS = 8;
const PROBLEM = 9;

const POPUPS = '.config-editor__hover, .config-editor__problem-tip:not([hidden])';

/** Starts counting the popups that show from now on (`window.__shown`). */
function countShown(page) {
  return page.evaluate((selector) => {
    window.__shown = 0;
    const root = document.querySelector('.config-editor');
    let was = false;
    const look = () => {
      const now = [...root.querySelectorAll(selector)].some((element) => element.getClientRects().length > 0);
      if (now && !was) window.__shown += 1;
      was = now;
    };
    new MutationObserver(look).observe(root, { subtree: true, childList: true, attributes: true });
  }, POPUPS);
}

/**
 * Milliseconds from now until a popup shows (`key`: a card for that key),
 * or null within `limit`.
 */
function msUntilShown(page, key = null, limit = 2000) {
  return page.evaluate(
    ([selector, key, limit]) =>
      new Promise((resolve) => {
        const start = performance.now();
        const check = () => {
          const shown = [...document.querySelectorAll(selector)].some(
            (element) =>
              element.getClientRects().length > 0 &&
              (key === null || element.querySelector('.config-editor__hover-key')?.textContent === key),
          );
          if (shown) resolve(performance.now() - start);
          else if (performance.now() - start > limit) resolve(null);
          else requestAnimationFrame(check);
        };
        check();
      }),
    [POPUPS, key, limit],
  );
}

/** Moves the pointer along `line` from `from` to `to` (columns), 3 px at a time, a move every 30 ms. */
async function slowPass(page, line, from, to) {
  const start = await pointAt(page, line, from);
  const end = await pointAt(page, line, to);
  await page.mouse.move(start.x, start.y);
  for (let x = start.x; x <= end.x; x += 3) {
    await page.mouse.move(x, start.y);
    await page.waitForTimeout(30);
  }
}

test.describe('hover opens on rest', () => {
  setupEditor(test, '?language=yaml');

  test.beforeEach(async ({ page }) => {
    await editor(page).paste(DOC);
    await page.mouse.move(2, 2);
  });

  test('a slow pass across a key opens no card; resting opens it after the delay', async ({ page }) => {
    await countShown(page);
    // `public_address` is wide: the pass stays on it well over the delay.
    await slowPass(page, ADDRESS, 0, 26);
    expect(await page.evaluate(() => window.__shown), 'no popup while the pointer moves').toBe(0);
    const ms = await msUntilShown(page, 'public_address');
    expect(ms, 'the card opens once the pointer rests').not.toBeNull();
    expect(ms).toBeGreaterThan(200);
    expect(ms).toBeLessThan(900);
  });

  test('a problem tip waits for the rest too', async ({ page }) => {
    await countShown(page);
    await slowPass(page, PROBLEM, 20, 23);
    expect(await page.evaluate(() => window.__shown), 'no popup while the pointer moves').toBe(0);
    const ms = await msUntilShown(page);
    expect(ms).not.toBeNull();
    expect(ms).toBeGreaterThan(200);
    // The card of the key lists the problem.
    await expect(page.locator('.config-editor__hover-problems')).toContainText('maybe');
  });

  test('from one card to the next key is quick', async ({ page }) => {
    const listen = await pointAt(page, LISTEN, 2);
    await page.mouse.move(listen.x, listen.y);
    await expect(page.locator('.config-editor__hover-key')).toHaveText('listen');
    const address = await pointAt(page, ADDRESS, 4);
    await page.mouse.move(address.x, address.y, { steps: 3 });
    const ms = await msUntilShown(page, 'public_address');
    expect(ms).not.toBeNull();
    expect(ms, 'the next card after a short rest').toBeLessThan(220);
  });

  test('the card takes the pointer from its first frame, while it fades in', async ({ page }) => {
    const listen = await pointAt(page, LISTEN, 2);
    await page.mouse.move(listen.x, listen.y);
    const card = page.locator('.config-editor__hover');
    await card.waitFor();
    const style = await card.evaluate((element) => {
      const style = getComputedStyle(element);
      return { animation: style.animationName, duration: style.animationDuration, events: style.pointerEvents };
    });
    expect(style).toEqual({ animation: 'config-editor-popup-in', duration: '0.1s', events: 'auto' });
    // Onto the card, it stays.
    const box = await card.boundingBox();
    await page.mouse.move(box.x + 20, box.y + box.height - 6, { steps: 6 });
    await page.waitForTimeout(600);
    await expect(card).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(card).toHaveCount(0);
  });

  test('no fade with reduced motion; stronger edges with more contrast', async ({ page }) => {
    await page.emulateMedia({ reducedMotion: 'reduce', contrast: 'more' });
    const listen = await pointAt(page, LISTEN, 2);
    await page.mouse.move(listen.x, listen.y);
    const card = page.locator('.config-editor__hover');
    await card.waitFor();
    const look = await card.evaluate((element) => {
      const style = getComputedStyle(element);
      return { animation: style.animationName, border: style.borderTopColor, text: style.color };
    });
    expect(look.animation).toBe('none');
    expect(look.border, 'the border takes the text color').toBe(look.text);
  });
});

test.describe('hover delay setting', () => {
  setupEditor(test, '?language=yaml&hoverDelay=900');

  test('the card waits the host’s delay; Ctrl+K Ctrl+I stays instant', async ({ page }) => {
    await editor(page).paste(DOC);
    const listen = await pointAt(page, LISTEN, 2);
    await page.mouse.move(listen.x, listen.y);
    const ms = await msUntilShown(page, 'listen', 3000);
    expect(ms).toBeGreaterThan(800);
    await page.keyboard.press('Escape');
    await page.mouse.move(2, 2);
    await page.waitForTimeout(400);
    await page.locator('.config-editor__input').evaluate((element) => {
      const at = element.value.indexOf('public_address') + 2;
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+i');
    expect(await msUntilShown(page, 'public_address')).toBeLessThan(150);
  });
});
