import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

// The README's code samples do what their text says (round 3's
// `D3.docs-1`: the custom-language guide's `.env` parse cut a value at its
// second `=`).

const readme = readFileSync(join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js', 'config-editor', 'README.md'), 'utf8');

test('the custom-language sample reads a KEY=value file, a value holding = included', () => {
  const start = readme.indexOf('const env = registerLanguage({');
  expect(start).toBeGreaterThan(0);
  const from = readme.indexOf('  parse(text) {', start);
  const to = readme.indexOf('\n  },', from);
  const body = readme.slice(from + '  parse(text) {'.length, to);
  const parse = new Function('text', body);
  expect(parse('# a comment\nURL=postgres://u@h/db?sslmode=require\nPORT=5432\n')).toEqual({
    URL: 'postgres://u@h/db?sslmode=require',
    PORT: '5432',
  });
});
