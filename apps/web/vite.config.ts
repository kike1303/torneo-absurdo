import { fileURLToPath } from 'node:url';
import adapter from '@sveltejs/adapter-node';
import { sveltekit } from '@sveltejs/kit/vite';
import { paraglideVitePlugin } from '@inlang/paraglide-js';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vitest/config';
import paraglideOptions from './paraglide.options.js';
import { chunkModules } from './vite/chunk-modules.js';
import { localSprites } from './vite/local-sprites.js';

// Kit 3 has no svelte.config.js: all Kit options live here (plan-a, decision 17).
// Never add `csrf.trustedOrigins`, `csrf.checkOrigin` or `paths.origin`, and keep `csp` exactly
// ADR-0012 B1 plus, at most, its B4 extensions, written as a literal in `sveltekit({ … })`
// (both enforced by scripts/check-kit-config.mjs).
export default defineConfig({
  plugins: [
    // First, so /__local-sprites/ is answered before Kit; only in `vite dev` (002 RF-B7).
    localSprites(),
    chunkModules(),
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
          // ADR-0012 B4.6 (2026-10-30 amendment): the authorize endpoints of
          // apps/server/src/auth/providers.rs, with their path, for the 303 of start and link.
          'form-action': [
            'self',
            'https://accounts.google.com/o/oauth2/v2/auth',
            'https://discord.com/oauth2/authorize',
          ],
          'frame-ancestors': ['none'],
        },
      },
    }),
  ],
  // RF-B17, plan-b decision 23: /api and /ws go to the server without rewriting the path, without
  // changeOrigin (the browser Origin reaches the server intact) and without xfwd. The anchored
  // regular expressions keep /apix and /wsx in web.
  server: {
    // Kit narrows `server.fs.allow` to apps/web, so `?url` imports of the placeholder sheets in
    // packages/renderer answered 403 in `vite dev` (found in 002 phase B, which plays them there).
    // Only that folder is added; content/_local/ is served by localSprites(), never through fs.
    fs: {
      allow: [fileURLToPath(new URL('../../packages/renderer/placeholders/', import.meta.url))],
    },
    proxy: {
      '^/api(?:/|$)': { target: 'http://127.0.0.1:8080' },
      '^/ws$': { target: 'ws://127.0.0.1:8080', ws: true },
    },
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts', 'tests/**/*.test.ts'],
  },
});
