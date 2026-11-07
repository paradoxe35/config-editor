# JavaScript packages

Browser packages built on the Rust crate in `crates/config-editor-leptos`
(which stays pure Rust):

- `config-editor/` → [`@pngwasi/config-editor`](config-editor): the
  framework-free package, a small wasm-bindgen crate plus the npm package
  around its output. Three entries: the main one (`index.js`: loading,
  `ConfigEditor`, `ConfigDocuments`, the catalogs), `/tools` (`tools.js`:
  text without an editor) and `/binding` (`binding.js`: what a framework
  package builds on).
- `config-editor-react/`, `config-editor-vue/`, `config-editor-svelte/` →
  [`@pngwasi/config-editor-react`](config-editor-react),
  [`-vue`](config-editor-vue), [`-svelte`](config-editor-svelte): the
  framework components, built on `@pngwasi/config-editor/binding`.
- `examples/`: the example pages (`vanilla.html`, `documents.html`, and one
  per framework), also used by the e2e tests.
- `typecheck/`: the READMEs' examples, type-checked (`npm run typecheck`).

```sh
npm install
npm run build     # wasm package, framework wrappers, examples
```

Building `@pngwasi/config-editor` needs Rust with the
`wasm32-unknown-unknown` target and `wasm-bindgen-cli` of the version in
`Cargo.lock`; `wasm-opt` is used when it is installed.
