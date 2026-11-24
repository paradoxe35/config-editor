import { expect, test } from '@playwright/test';
import { setupEditor } from './helpers.mjs';

// A Leptos host that remounts the editor in place (`?remount`: a language
// picker over `move || view! { <ConfigEditor … /> }`), with `Lines`, a
// language that cannot write documents, so Edit-as from it is refused.
// - Asked back to Native, the refusal goes.
// - Each remount leaves one editor: the parts a setting switches (footer,
//   toolbar, find, go to line) of the editor before go with it, whatever
//   it showed (a refused Edit-as among them).

setupEditor(test, '?remount');

const native = (page) => page.locator('.demo__native-select');
const editAs = (page) => page.locator('.demo__edit-as-select');
const status = (page) => page.locator('.config-editor__status');
const mounts = (page) => page.locator('.demo__remount-status').getAttribute('data-mounts');

test('a refused Edit-as says so until Native is picked again', async ({ page }) => {
  await editAs(page).selectOption('YAML');
  await expect(status(page)).toContainText('1 error');
  await expect(page.locator('.config-editor__diagnostic-marker')).toHaveCount(1);
  await editAs(page).selectOption('');
  await expect(status(page)).toContainText('No errors');
  await expect(page.locator('.config-editor__diagnostic-marker')).toHaveCount(0);
  await expect(page.locator('.config-editor__input')).toHaveValue('name: edge\nlisten: 7835\n');
});

test('each remount leaves one editor, with or without a refused Edit-as before it', async ({ page }) => {
  // Lines (0) is on screen first; Lines is the one Edit-as refuses.
  let shown = 0;
  const pick = async (index) => {
    const before = Number(await mounts(page));
    await native(page).selectOption(String(index));
    await expect.poll(() => mounts(page)).toBe(String(before + 1));
    shown = index;
  };
  for (const refuse of [false, true, false, true, true]) {
    if (refuse) {
      if (shown !== 0) await pick(0);
      await editAs(page).selectOption('YAML');
      await expect(status(page).last()).toContainText('1 error');
      await editAs(page).selectOption('');
    }
    await pick(shown === 0 ? 1 : 0);
    await expect(page.locator('.config-editor')).toHaveCount(1);
    await expect(page.locator('.config-editor__input')).toHaveCount(1);
    await expect(status(page)).toHaveCount(1);
    await expect(page.locator('.config-editor__toolbar')).toHaveCount(1);
    await expect(page.locator('.config-editor__find')).toHaveCount(1);
    await expect(page.locator('.config-editor__goto')).toHaveCount(1);
    const lines = (await page.locator('.config-editor__input').inputValue()).split('\n').length;
    await expect(status(page)).toContainText(`${lines} lines`);
    await expect(status(page)).toContainText('No errors');
  }
});
