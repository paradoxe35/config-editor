import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 3700000768 on documents-react (?random).
test("documents-react: lock-marks (seed 3700000768)", async ({ browser }) => {
  await replay(browser, "documents-react", "?random", [{"k":"hostValue","text":null,"pick":6,"how":"file"},{"k":"type","text":"x😀"},{"k":"suggest","open":true,"pick":0},{"k":"suggest","open":true,"pick":0},{"k":"suggestMouse","outside":true,"pick":1},{"k":"key","key":"Control+Shift+Z","n":2},{"k":"clip","key":"Control+V"},{"k":"type","text":"é👍🏽"},{"k":"lock","range":{"at":0.1,"length":3,"line":false}}], "lock-marks");
});
