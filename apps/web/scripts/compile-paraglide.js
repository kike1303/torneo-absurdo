// Compiles the Paraglide messages into src/lib/paraglide/ (git-ignored) with the same options as
// the Vite plugin. `pnpm run sync` runs it before `svelte-kit sync`, so every entry point
// (`check`, `test`, `e2e`, `just`) works on a clean clone (TA-10).
import { compile } from '@inlang/paraglide-js';
import options from '../paraglide.options.js';

await compile(options);
