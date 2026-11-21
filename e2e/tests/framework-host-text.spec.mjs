import { hostTextCases } from './framework-host-text-cases.mjs';
import { FRAMEWORK_PAGES } from './helpers.mjs';

// The texts the host's lists name (`rangesFor`, `diagnosticsFor`), its own
// change (`applyChanges`) and revisions through late echoes, hide and show
// and re-mounts, on every framework's example page.
for (const name of FRAMEWORK_PAGES) hostTextCases(name);
