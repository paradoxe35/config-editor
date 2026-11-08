// Starts the trunk build of the demo (the CSP spec serves it under /leptos/);
// the demo reads its flags (`styled=0`, `colors=custom`, …) from the URL.
import init from '/leptos/demo.js';

await init({ module_or_path: '/leptos/demo_bg.wasm' });
