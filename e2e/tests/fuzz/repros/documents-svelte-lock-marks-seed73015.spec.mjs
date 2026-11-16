import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 73015 on documents-svelte.
// A lock the host gives is read in the host's text and marked where it
// names, after edits that have not reached the host.
test("documents-svelte: lock-marks (seed 73015)", async ({ browser }) => {
  await replay(browser, "documents-svelte", "", [
    {
      "k": "lock",
      "range": {
        "at": 0.805,
        "length": 14,
        "line": false
      }
    },
    {
      "k": "key",
      "key": "End",
      "n": 1
    },
    {
      "k": "key",
      "key": "ArrowRight",
      "n": 1
    },
    {
      "k": "ime",
      "steps": [
        "한"
      ],
      "commit": "한"
    },
    {
      "k": "dragMove",
      "at": 0.193,
      "length": 6,
      "x": 0.241,
      "y": 0.059
    },
    {
      "k": "key",
      "key": "Backspace",
      "n": 1
    },
    {
      "k": "lock",
      "range": {
        "at": 0.711,
        "length": 14,
        "line": false
      }
    }
  ], "lock-marks");
});
