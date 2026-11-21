import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 88 on vanilla (?revisions).
// The host's own change while editing as another language names the new
// text's revision, so the host keeps it as its text and ends on the document.
test("vanilla: host (seed 88)", async ({ browser }) => {
  await replay(browser, "vanilla", "?revisions", [
    {
      "k": "editAs",
      "language": "toml"
    },
    {
      "k": "hostChange",
      "at": 0.395,
      "base": true,
      "line": "# host note"
    }
  ], "host");
});
