import adapter from '@sveltejs/adapter-node';
import { sveltekit } from '@sveltejs/kit/vite';
import { paraglideVitePlugin } from '@inlang/paraglide-js';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vitest/config';
import paraglideOptions from './paraglide.options.js';

// Kit 3 has no svelte.config.js: all Kit options live here (plan-a, decision 17).
// Never add `csrf.trustedOrigins`, `csrf.checkOrigin` or `paths.origin`
// (enforced by scripts/check-kit-config.mjs).
export default defineConfig({
  plugins: [
    tailwindcss(),
    paraglideVitePlugin(paraglideOptions),
    sveltekit({
      adapter: adapter(),
      csp: {
        mode: 'auto',
        directives: {
          'default-src': ['self'],
          'script-src': ['self'],
          'style-src': ['self', 'unsafe-inline'],
          'img-src': ['self', 'data:', 'blob:'],
          'font-src': ['self'],
          'connect-src': ['self'],
          'object-src': ['none'],
          'base-uri': ['self'],
          'form-action': ['self'],
          'frame-ancestors': ['none'],
        },
      },
    }),
  ],
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts', 'tests/**/*.test.ts'],
  },
});
