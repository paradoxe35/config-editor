# JavaScript packages

Browser packages built on the Rust crate in `crates/config-editor-leptos`
(which stays pure Rust):

- `config-editor/` → [`@pngwasi/config-editor`](config-editor): the
  framework-free package, a small wasm-bindgen crate plus the npm package
  around its output.
- `config-editor-react/` → [`@pngwasi/config-editor-react`](config-editor-react):
  the React component, built on `@pngwasi/config-editor`.
- `examples/`: `vanilla.html` and `react.html`, also used by the e2e tests
  (`e2e/tests/js-packages.spec.mjs`).

```sh
npm install
npm run build     # wasm package, React wrapper, examples
```

Building `@pngwasi/config-editor` needs Rust with the
`wasm32-unknown-unknown` target and `wasm-bindgen-cli` of the version in
`Cargo.lock`; `wasm-opt` is used when it is installed.
