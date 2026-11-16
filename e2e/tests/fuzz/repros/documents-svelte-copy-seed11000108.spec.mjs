import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// P1: a copy of a selection snapped to a secret placeholder copies the placeholder whole.
// Shrunk by the browser fuzz from seed 11000108 on documents-svelte (?random).
// Failed the `copy` check after step 19: copied "\n", selection "«secret 3f2a»".
test("documents-svelte: copy (seed 11000108)", async ({ browser }) => {
  await replay(browser, "documents-svelte", "?random", [
    {
      "k": "suggest",
      "open": true,
      "pick": 0
    },
    {
      "k": "key",
      "key": "Control+Z",
      "n": 2
    },
    {
      "k": "type",
      "text": "listen\nvalue"
    },
    {
      "k": "jump",
      "at": 0.07,
      "length": 4
    },
    {
      "k": "clip",
      "key": "Control+V"
    },
    {
      "k": "hover",
      "at": 0.478,
      "column": 0.296,
      "into": false,
      "escape": true
    },
    {
      "k": "key",
      "key": "Backspace",
      "n": 2
    },
    {
      "k": "drag",
      "x1": 0.853,
      "y1": 0.29,
      "x2": 0.308,
      "y2": 0.786
    },
    {
      "k": "key",
      "key": "Control+A",
      "n": 1
    },
    {
      "k": "f8",
      "shift": false,
      "n": 3
    },
    {
      "k": "type",
      "text": "ß🇫🇷"
    },
    {
      "k": "echo"
    },
    {
      "k": "suggestMouse",
      "outside": false,
      "pick": 2
    },
    {
      "k": "editAs",
      "language": "hcl"
    },
    {
      "k": "crossCopy",
      "from": 0,
      "to": 1,
      "load": false,
      "token": true,
      "at": 0.478,
      "length": 14,
      "at2": 0.429,
      "over": 0
    },
    {
      "k": "dragMove",
      "at": 0.534,
      "length": 6,
      "x": 0.405,
      "y": 0.936
    },
    {
      "k": "key",
      "key": "PageUp",
      "n": 2
    },
    {
      "k": "key",
      "key": "Enter",
      "n": 2
    },
    {
      "k": "f8",
      "shift": false,
      "n": 2
    },
    {
      "k": "crossCopy",
      "from": 1,
      "to": 2,
      "load": true,
      "token": true,
      "at": 0.179,
      "length": 5,
      "at2": 0.479,
      "over": 1
    }
  ], "copy");
});
