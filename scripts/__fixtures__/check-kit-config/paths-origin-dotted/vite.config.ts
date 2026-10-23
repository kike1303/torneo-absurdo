import adapter from '@sveltejs/adapter-node';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

// Never add trustedOrigins, checkOrigin or paths.origin.
const kit = { paths: {} };
kit.paths.origin = 'https://x';
export default defineConfig({
  plugins: [sveltekit({ adapter: adapter(), csp: { mode: 'auto', directives: { 'script-src': ['self'] } } })],
});
