# JavaScript packages

Browser packages built on the Rust crate in `crates/leptos-config-editor`
(which stays pure Rust):

- `config-editor/` — the framework-free package: a small wasm-bindgen crate
  plus the npm package around its output.
- `config-editor-react/` — the React component.
- `examples/` — `vanilla.html` and `react.html`, also used by the e2e tests
  (`e2e/tests/js-packages.spec.mjs`).

```sh
npm install
npm run build     # wasm package, React wrapper, examples
```
