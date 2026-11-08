// Bundles the example pages (examples/*.html load examples/dist/*.js), which
// the end-to-end tests open. React pages through esbuild's JSX; Vue pages
// with Vue's full build, so their components are written as templates the
// way an app writes them (`v-model`, `@save`); Svelte pages through the
// Svelte compiler, in development mode so its warnings show.
import { build } from 'esbuild';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { compile } from 'svelte/compiler';

const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const common = { bundle: true, format: 'esm', target: 'es2022', outdir: join(here, 'dist'), logLevel: 'warning' };

/** Compiles `.svelte` files (Svelte 5 reads their TypeScript itself). */
const svelte = {
  name: 'svelte',
  setup(builder) {
    builder.onLoad({ filter: /\.svelte$/ }, async ({ path }) => {
      const source = await readFile(path, 'utf8');
      const { js, warnings } = compile(source, { filename: relative(here, path), generate: 'client', dev: true, css: 'injected' });
      return { contents: js.code, loader: 'js', warnings: warnings.map((warning) => ({ text: `${warning.code}: ${warning.message}` })) };
    });
  },
};

await Promise.all([
  build({
    ...common,
    entryPoints: ['react.tsx', 'load-retry.ts', 'documents-react.tsx', 'csp-react.tsx'].map((name) => join(here, name)),
    jsx: 'automatic',
  }),
  build({
    ...common,
    entryPoints: ['vue.ts', 'documents-vue.ts', 'csp-vue.ts'].map((name) => join(here, name)),
    alias: { vue: require.resolve('vue/dist/vue.esm-bundler.js') },
    define: {
      'process.env.NODE_ENV': '"development"',
      __VUE_OPTIONS_API__: 'true',
      __VUE_PROD_DEVTOOLS__: 'false',
      __VUE_PROD_HYDRATION_MISMATCH_DETAILS__: 'false',
    },
  }),
  build({
    ...common,
    entryPoints: ['svelte.ts', 'documents-svelte.ts', 'csp-svelte.ts'].map((name) => join(here, name)),
    conditions: ['svelte', 'development', 'browser'],
    plugins: [svelte],
  }),
]);
