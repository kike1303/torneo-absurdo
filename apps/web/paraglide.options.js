// Shared by the Vite plugin (vite.config.ts) and scripts/compile-paraglide.js, which `pnpm run sync`
// and `just _web-prepare` run (plan-a, decision 2).
// Paraglide only reads the locale from the URL: `/` without a prefix is redirected by our own
// handle (src/lib/server/hooks/locale-redirect.ts), and the `locale` cookie is written by us.

/** @type {import('@inlang/paraglide-js').CompilerOptions} */
const options = {
  project: './project.inlang',
  outdir: './src/lib/paraglide',
  strategy: ['url', 'baseLocale'],
  // Explicit patterns so that the base locale (`en`) is prefixed too.
  urlPatterns: [
    {
      pattern: ':protocol://:domain(.*)::port?/:path(.*)?',
      localized: [
        ['es', ':protocol://:domain(.*)::port?/es/:path(.*)?'],
        ['en', ':protocol://:domain(.*)::port?/en/:path(.*)?'],
      ],
    },
  ],
  emitGitIgnore: false,
  emitPrettierIgnore: false,
  emitReadme: false,
};

export default options;
