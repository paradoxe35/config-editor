import { expect, test } from '@playwright/test';
import { editor, pointAt, setupEditor } from './helpers.mjs';

// The hover card and the problem tip open once the pointer rests (the
// `hoverDelay` setting, 300 ms by default), not a fixed time after it came
// onto a key: a hand passing slowly across the text opens nothing. From one
// popup to the next is quick; the popups fade in without delaying the
// pointer; more contrast draws their edges in their text color. The lock
// tip (what protected text under the pointer is) waits for the same rest;
// a refused edit's tip shows at once.

// Comment lines first, so a card opens above its key and leaves the next
// lines free.
const DOC = '#\n#\n#\n#\n#\n#\n#\nlisten: 7835\npublic_address: example.com\ninsecure_plaintext: maybe\n';
const LISTEN = 7;
const ADDRESS = 8;
const PROBLEM = 9;

const POPUPS = '.config-editor__hover, .config-editor__problem-tip:not([hidden])';

/**
 * Starts counting the popups that show from now on (`window.__shown`), and
 * keeps the time of the pointer's last move (`window.__lastMove`).
 */
function countShown(page, popups = POPUPS) {
  return page.evaluate((selector) => {
    window.__shown = 0;
    window.__lastMove = performance.now();
    document.addEventListener('pointermove', () => (window.__lastMove = performance.now()), { capture: true });
    // The whole page: the lock tip is placed outside the editor's root.
    const root = document.body;
    let was = false;
    const look = () => {
      const now = [...root.querySelectorAll(selector)].some((element) => element.getClientRects().length > 0);
      if (now && !was) window.__shown += 1;
      was = now;
    };
    new MutationObserver(look).observe(root, { subtree: true, childList: true, attributes: true });
  }, popups);
}

/**
 * Milliseconds from now (`fromRest`: from the pointer's last move, see
 * `countShown`) until a popup shows (`key`: a card for that key), or null
 * within `limit`. From the rest, a slow machine's gap between the last move
 * and this call doesn't shorten the delay measured.
 */
function msUntilShown(page, key = null, limit = 2000, fromRest = false, popups = POPUPS) {
  return page.evaluate(
    ([selector, key, limit, fromRest]) =>
      new Promise((resolve) => {
        const start = fromRest ? window.__lastMove : performance.now();
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
    [popups, key, limit, fromRest],
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
    const ms = await msUntilShown(page, 'public_address', 2000, true);
    expect(ms, 'the card opens once the pointer rests').not.toBeNull();
    expect(ms).toBeGreaterThan(200);
    expect(ms).toBeLessThan(900);
  });

  test('a problem tip waits for the rest too', async ({ page }) => {
    await countShown(page);
    await slowPass(page, PROBLEM, 20, 23);
    expect(await page.evaluate(() => window.__shown), 'no popup while the pointer moves').toBe(0);
    const ms = await msUntilShown(page, null, 2000, true);
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

// The protected fixture: `id: srv-42` (read-only, "Assigned by the server"),
// `name: edge` (free), the `limits` block (read-only, "Set by your plan").
const LOCK_TIP = '.config-editor__protect-tip';
const ID = 0;
const NAME = 1;
const MAX = 4;

test.describe('the lock tip opens on rest', () => {
  setupEditor(test, '?schema=protected');

  test.beforeEach(async ({ page }) => {
    await page.mouse.move(2, 2);
  });

  test('a slow pass over a locked value shows no tip; resting shows it after the delay', async ({ page }) => {
    await countShown(page, LOCK_TIP);
    await slowPass(page, ID, 4, 8);
    expect(await page.evaluate(() => window.__shown), 'no lock tip while the pointer moves').toBe(0);
    const ms = await msUntilShown(page, null, 2000, true, LOCK_TIP);
    expect(ms, 'the lock tip shows once the pointer rests').not.toBeNull();
    expect(ms).toBeGreaterThan(200);
    expect(ms).toBeLessThan(900);
    await expect(page.locator(LOCK_TIP)).toHaveText('Assigned by the server');
    // Off the text, it goes at once.
    await page.mouse.move(2, 2);
    await expect(page.locator(LOCK_TIP)).toHaveCount(0, { timeout: 150 });
  });

  test('from one lock tip to the next is quick, and each fades in', async ({ page }) => {
    const id = await pointAt(page, ID, 6);
    await page.mouse.move(id.x, id.y);
    const tip = page.locator(LOCK_TIP);
    await expect(tip).toHaveText('Assigned by the server');
    expect(await tip.evaluate((element) => getComputedStyle(element).animationName)).toBe('config-editor-popup-in');
    await countShown(page, LOCK_TIP);
    const max = await pointAt(page, MAX, 8);
    await page.mouse.move(max.x, max.y, { steps: 3 });
    const ms = await msUntilShown(page, null, 2000, true, LOCK_TIP);
    expect(ms).not.toBeNull();
    expect(ms, 'the next lock tip after a short rest').toBeLessThan(220);
    await expect(tip).toHaveText('Set by your plan');
  });

  test('a refused edit says why at once', async ({ page }) => {
    const free = await pointAt(page, NAME, 7);
    await page.mouse.click(free.x, free.y);
    await page.locator('.config-editor__input').evaluate((element) => {
      const at = element.value.indexOf('srv-42') + 2;
      element.setSelectionRange(at, at);
      element.addEventListener('keydown', () => (window.__lastMove = performance.now()), { capture: true });
    });
    await page.keyboard.type('x');
    const ms = await msUntilShown(page, null, 2000, true, `${LOCK_TIP}--refused`);
    expect(ms).not.toBeNull();
    expect(ms, 'no rest for a refusal').toBeLessThan(150);
    expect(
      await page.locator(`${LOCK_TIP}--refused`).evaluate((element) => getComputedStyle(element).animationName),
    ).toBe('none');
  });

  test('no fade with reduced motion; stronger edges with more contrast', async ({ page }) => {
    await page.emulateMedia({ reducedMotion: 'reduce', contrast: 'more' });
    const id = await pointAt(page, ID, 6);
    await page.mouse.move(id.x, id.y);
    const tip = page.locator(LOCK_TIP);
    await tip.waitFor();
    const look = await tip.evaluate((element) => {
      const style = getComputedStyle(element);
      return { animation: style.animationName, border: style.borderTopColor, text: style.color };
    });
    expect(look.animation).toBe('none');
    expect(look.border, 'the border takes the text color').toBe(look.text);
  });
});

test.describe('the lock tip and the hover delay setting', () => {
  setupEditor(test, '?schema=protected&hoverDelay=900');

  test('the lock tip waits the host’s delay', async ({ page }) => {
    await page.mouse.move(2, 2);
    await countShown(page, LOCK_TIP);
    const id = await pointAt(page, ID, 6);
    await page.mouse.move(id.x, id.y);
    const ms = await msUntilShown(page, null, 3000, true, LOCK_TIP);
    expect(ms).toBeGreaterThan(800);
  });
});
