#!/usr/bin/env node
// Writes the table of contents of each README (a list of its `##` and `###`
// sections, linked) between `<!-- toc -->` and `<!-- /toc -->`, inserted
// before the first section when missing. `--check` changes nothing and
// fails when a contents list is out of date (CI runs it).
//
//   node scripts/readme-toc.mjs          # update
//   node scripts/readme-toc.mjs --check  # verify

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const READMES = [
  'README.md',
  'crates/config-editor-leptos/README.md',
  'js/config-editor/README.md',
  'js/config-editor-react/README.md',
];
const START = '<!-- toc -->';
const END = '<!-- /toc -->';
const TITLE = '## Contents';

/** The `##`/`###` headings outside code fences, with their line indices. */
function headings(lines) {
  const found = [];
  let fence = null;
  lines.forEach((line, index) => {
    const marker = line.match(/^\s*(```+|~~~+)/);
    if (marker) {
      if (fence === null) fence = marker[1];
      else if (line.trim().startsWith(fence)) fence = null;
      return;
    }
    if (fence !== null) return;
    const heading = line.match(/^(#{2,3})\s+(.+?)\s*#*\s*$/);
    if (heading && line !== TITLE) found.push({ level: heading[1].length, text: heading[2], index });
  });
  return found;
}

/** GitHub's anchor for a heading: lower case, punctuation dropped, spaces to dashes. */
function slug(text) {
  return text
    .replace(/<[^>]*>|`/g, (match) => (match === '`' ? '' : match.slice(1, -1)))
    .toLowerCase()
    .replace(/[^\p{Letter}\p{Number}\s_-]/gu, '')
    .trim()
    .replace(/\s/g, '-');
}

/** The heading as plain link text (code spans kept). */
function label(text) {
  return text.replace(/\[([^\]]*)\]\([^)]*\)/g, '$1');
}

function contents(lines) {
  const seen = new Map();
  const items = headings(lines).map(({ level, text }) => {
    const base = slug(text);
    const count = seen.get(base) ?? 0;
    seen.set(base, count + 1);
    const anchor = count ? `${base}-${count}` : base;
    return `${level === 3 ? '  ' : ''}- [${label(text)}](#${anchor})`;
  });
  // The blank line after the marker is Prettier's, so formatting leaves it.
  return [START, '', TITLE, '', ...items, END];
}

function updated(text) {
  const lines = text.split('\n');
  const start = lines.indexOf(START);
  const end = lines.indexOf(END);
  // Sections are read with any old list taken out.
  const body = start >= 0 && end > start ? [...lines.slice(0, start), ...lines.slice(end + 1)] : lines;
  const at = start >= 0 && end > start ? start : headings(body)[0]?.index;
  if (at === undefined) return text;
  const rest = body.slice(at);
  // One blank line after the list, before the first section.
  while (rest[0] === '') rest.shift();
  return [...body.slice(0, at), ...contents(body), '', ...rest].join('\n');
}

const check = process.argv.includes('--check');
let stale = 0;
for (const name of READMES) {
  const path = join(root, name);
  const text = readFileSync(path, 'utf8');
  const next = updated(text);
  if (next === text) continue;
  if (check) {
    console.error(`${name}: the contents list is out of date; run node scripts/readme-toc.mjs`);
    stale += 1;
  } else {
    writeFileSync(path, next);
    console.log(`updated ${name}`);
  }
}
process.exit(stale ? 1 : 0);
