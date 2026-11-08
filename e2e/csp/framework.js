// Loads the bundle of the binding named by `?framework=` (react, vue, svelte).
const framework = new URLSearchParams(location.search).get('framework');
if (!['react', 'vue', 'svelte'].includes(framework)) throw new Error(`unknown framework ${framework}`);
await import(`/js/examples/dist/csp-${framework}.js`);
