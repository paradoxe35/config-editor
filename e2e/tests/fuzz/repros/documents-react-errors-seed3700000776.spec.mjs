import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 3700000776 on documents-react (?random).
// The host's problems, given alone, are read in the value they come with: a
// file of the host's own loaded into a document (a copy between documents)
// takes the problems it found on the old text away, as any text of its own
// does, or they are read in the new file and do not fit it.
test("documents-react: errors (seed 3700000776)", async ({ browser }) => {
  await replay(browser, "documents-react", "?random", [
    { "k": "hostValue", "text": null, "pick": 2, "how": "file" },
    { "k": "lateDiagnostics", "at": 0.79, "by": "revision", "wait": 0 },
    { "k": "crossCopy", "from": 0, "to": 0, "load": true, "token": false, "at": 0.963, "length": 5, "at2": 0.082, "over": 4 }
  ], "errors");
});
