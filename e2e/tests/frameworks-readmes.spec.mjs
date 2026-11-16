import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

// The framework READMEs list every exported type and tool, and their React snippets import what they call.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

const js = join(root, 'js');

test('the framework READMEs list every type and tool their packages export', () => {
  const exported = (path) => {
    const source = readFileSync(join(js, path), 'utf8');
    return [...source.matchAll(/export (?:type )?\{([^}]*)\}/g)]
      .flatMap((match) => match[1].split(','))
      .map((name) => name.trim().replace(/^type /, '').split(' as ').pop().trim())
      .filter(Boolean);
  };
  const tools = exported('config-editor/tools.js').filter((name) => name !== 'loadConfigEditor');
  const missing = [];
  for (const [name, entry] of [
    ['config-editor-react', 'src/index.tsx'],
    ['config-editor-vue', 'src/index.ts'],
    ['config-editor-svelte', 'src/index.ts'],
  ]) {
    const readme = readFileSync(join(js, name, 'README.md'), 'utf8');
    const start = readme.indexOf('## Exports');
    const end = readme.indexOf('\n## ', start + 1);
    const section = readme.slice(start, end < 0 ? undefined : end);
    const listed = new Set([...section.matchAll(/`([A-Za-z]+)`/g)].map((match) => match[1]));
    for (const item of [...exported(`${name}/${entry}`), ...tools]) {
      if (!listed.has(item)) missing.push(`${name}: ${item}`);
    }
  }
  expect(missing).toEqual([]);
});

// The hooks a README's React snippet calls are imported in it (the
// root README's snippet called useState without importing it).
test("the READMEs' React snippets import the hooks they call", () => {
  const readmes = [
    'README.md',
    'js/README.md',
    'js/config-editor/README.md',
    'js/config-editor-react/README.md',
    'crates/config-editor-leptos/README.md',
  ];
  const missing = [];
  let snippets = 0;
  for (const readme of readmes) {
    const text = readFileSync(join(root, readme), 'utf8');
    for (const [, code] of text.matchAll(/```(?:tsx|jsx)\n([\s\S]*?)```/g)) {
      snippets += 1;
      const imported = new Set(
        [...code.matchAll(/import\s+(?:\w+\s*,\s*)?\{([^}]*)\}\s+from\s+"[^"]+"/g)].flatMap((match) =>
          match[1].split(',').map((name) => name.trim().replace(/^type /, '')),
        ),
      );
      for (const [, hook] of code.matchAll(/(?<![.\w])(use[A-Z]\w*)\(/g)) {
        // A hook defined in the snippet itself.
        if (new RegExp(`function ${hook}\\b|const ${hook}\\b`).test(code)) continue;
        if (!imported.has(hook) && !/^React\./.test(hook)) missing.push(`${readme}: ${hook}`);
      }
    }
  }
  expect(snippets).toBeGreaterThan(3);
  expect([...new Set(missing)]).toEqual([]);
});
