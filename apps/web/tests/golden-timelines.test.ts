import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { timelineFromFightLog } from '../src/lib/features/fight/timeline-from-fight-log.js';
import { readGoldenLog, type GoldenLogName } from './support/golden-logs.js';

// 005 plan decision 4: the renderer tests play the timelines of the engine goldens that end by
// time, but the renderer cannot import the adapter (002 boundary). They are generated here and
// committed in packages/renderer/test/fixtures/golden/: inside the renderer's `test/`, so neither
// `src/` nor the package `exports` (`./fixtures/*`) can reach them.
// Regenerate with `vitest -u`; in CI (`CI=true`) Vitest never writes, so a stale file fails. A
// missing file fails too (except with `-u`), instead of being written silently.
const ASSETS = { a: { asset: 'dummy_a' }, b: { asset: 'dummy_b' } } as const;
const NAMES = ['decision-a', 'decision-b', 'draw'] as const;
const DIR = '../../../packages/renderer/test/fixtures/golden/';
const fileOf = (name: string) => `${DIR}${name}.timeline.json`;
const EXISTED = new Map(
  NAMES.map((name) => [name, existsSync(fileURLToPath(new URL(fileOf(name), import.meta.url)))]),
);

describe('golden timelines for the renderer tests', () => {
  it.each(NAMES)('%s.timeline.json is the adapter output of its golden', async (name) => {
    const result = timelineFromFightLog(readGoldenLog(`log-${name}` as GoldenLogName), ASSETS);
    if (!result.ok) throw new Error(JSON.stringify(result.errors));
    const updating = expect.getState().snapshotState.snapshotUpdateState === 'all';
    expect(EXISTED.get(name) || updating, `${fileOf(name)} is missing: run vitest -u`).toBe(true);
    await expect(`${JSON.stringify(result.timeline, null, 2)}\n`).toMatchFileSnapshot(fileOf(name));
  });
});
