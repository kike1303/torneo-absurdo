// Build-only plugin for scripts/check-renderer-split.mjs (002 RF-A17). The Vite manifest gives
// the chunk graph but not the modules inside each chunk, so a static import of Pixi that lands
// in a shared chunk would go unseen. This writes `chunk-modules.json` (chunk file → module ids,
// relative to apps/web like the manifest keys) next to the client manifest, in `.vite/`, which
// adapter-node never publishes.
import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import type { Plugin } from 'vite';

export const CHUNK_MODULES_FILE = 'chunk-modules.json';

export function chunkModules(): Plugin {
  let root = '';
  return {
    name: 'torneo:chunk-modules',
    apply: 'build',
    applyToEnvironment: (environment) => environment.name === 'client',
    configResolved(config) {
      root = config.root;
    },
    writeBundle(options, bundle) {
      if (options.dir === undefined) return;
      const out: Record<string, string[]> = {};
      for (const item of Object.values(bundle)) {
        if (item.type !== 'chunk') continue;
        out[item.fileName] = item.moduleIds.map((id) =>
          path.relative(root, id).split(path.sep).join('/'),
        );
      }
      const dir = path.join(options.dir, '.vite');
      mkdirSync(dir, { recursive: true });
      writeFileSync(path.join(dir, CHUNK_MODULES_FILE), `${JSON.stringify(out)}\n`);
    },
  };
}
