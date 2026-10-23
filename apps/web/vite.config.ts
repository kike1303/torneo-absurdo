import adapter from '@sveltejs/adapter-node';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

// Kit 3 has no svelte.config.js: all Kit options live here (plan-a, decision 17).
// Never add `csrf.trustedOrigins`, `csrf.checkOrigin` or `paths.origin`
// (enforced by scripts/check-kit-config.mjs).
export default defineConfig({
  plugins: [
    tailwindcss(),
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
});
