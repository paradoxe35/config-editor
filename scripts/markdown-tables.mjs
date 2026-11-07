#!/usr/bin/env node
// Fails when a Markdown table in the repository is broken so that its rows
// render as plain text: a row (`| … |`) outside any table, usually because a
// paragraph was written between the table's lines, or a table whose header
// follows text without a blank line. CI runs it (`scripts/ci.sh docs`).
//
//   node scripts/markdown-tables.mjs

import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const files = execFileSync('git', ['ls-files', '*.md'], { cwd: root, encoding: 'utf8' })
  .split('\n')
  .filter((file) => file && !file.includes('node_modules/'));

const isRow = (line) => line.trimStart().startsWith('|');
const isSeparator = (line) => /^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)*\|?\s*$/.test(line);

/** The problems in one file, as `line: message`. */
function problems(text) {
  const lines = text.split('\n');
  const found = [];
  let fence = null;
  let inTable = false;
  lines.forEach((line, index) => {
    const marker = line.match(/^\s*(```+|~~~+)/);
    if (marker) {
      if (fence === null) fence = marker[1];
      else if (line.trim().startsWith(fence)) fence = null;
      inTable = false;
      return;
    }
    if (fence !== null) return;
    if (!isRow(line)) {
      inTable = false;
      return;
    }
    if (inTable) return;
    const next = lines[index + 1] ?? '';
    if (isRow(line) && isSeparator(next)) {
      const before = lines[index - 1] ?? '';
      if (before.trim() !== '' && !before.trim().startsWith('<!--') && !before.trim().startsWith('<summary')) {
        found.push(`${index + 1}: a table starts right after text; leave a blank line before it`);
      }
      inTable = true;
      return;
    }
    found.push(`${index + 1}: a table row outside any table (text between a table's rows breaks it): ${line.trim().slice(0, 60)}`);
    // One report per broken stretch.
    inTable = true;
  });
  return found;
}

let failed = 0;
for (const file of files) {
  for (const problem of problems(readFileSync(join(root, file), 'utf8'))) {
    console.error(`${file}:${problem}`);
    failed += 1;
  }
}
process.exit(failed ? 1 : 0);
