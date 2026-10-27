import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import config from '../vite.config.js';

// 002 phase B: the placeholder sheets are `?url` imports from packages/renderer, outside the
// `server.fs.allow` Kit sets, so `vite dev` answered 403 and /dev/arena failed with
// asset_load_failed. Only the placeholders folder is added, never content/_local/.
const REPO = fileURLToPath(new URL('../../../', import.meta.url));

describe('vite.config server.fs.allow', () => {
  it('allows exactly the renderer placeholders, and nothing of content/', () => {
    const allow = (config.server?.fs?.allow ?? []).map((p) => path.relative(REPO, p));
    expect(allow).toEqual([path.join('packages', 'renderer', 'placeholders')]);
  });

  it('localSprites() is the first plugin', () => {
    const first = (config.plugins ?? [])[0] as { name?: string } | undefined;
    expect(first?.name).toBe('torneo:local-sprites');
  });
});
