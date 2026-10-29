import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { drawRef } from '../scripts/ref/draw.ts';
import { decodePng } from './support/png.ts';
import { EXPORT_REF } from './support/root.ts';

// 007 RF-B8 and plan-b decision 22: fixtures/export-ref is what scripts/ref/draw.ts draws. The PNG
// are compared by pixels, not bytes (the bytes depend on the zlib version).
const REGENERATE = 'run pnpm --filter @torneo/sprite-importer gen:fixture';

function files(dir: string, base = ''): string[] {
  return readdirSync(path.join(dir, base), { withFileTypes: true }).flatMap((e) => {
    const rel = base === '' ? e.name : `${base}/${e.name}`;
    return e.isDirectory() ? files(dir, rel) : [rel];
  });
}

describe('fixtures/export-ref is fresh', () => {
  const ref = drawRef();

  it('has exactly the files the drawer makes', () => {
    const expected = ['export.json', ...[...ref.frames.keys()].map((k) => `frames/${k}.png`)];
    expect(files(EXPORT_REF).sort(), REGENERATE).toEqual(expected.sort());
  });

  it('has the same export.json bytes', () => {
    expect(readFileSync(path.join(EXPORT_REF, 'export.json'), 'utf8'), REGENERATE).toBe(
      JSON.stringify(ref.exportJson, null, 2) + '\n',
    );
  });

  it('has the same pixels in every PNG', () => {
    const stale: string[] = [];
    for (const [key, image] of ref.frames) {
      const png = decodePng(
        readFileSync(path.join(EXPORT_REF, 'frames', ...`${key}.png`.split('/'))),
      );
      const same =
        png.width === image.width &&
        png.height === image.height &&
        Buffer.from(png.data).equals(Buffer.from(image.data));
      if (!same) stale.push(key);
    }
    expect(stale, REGENERATE).toEqual([]);
  });

  it('draws the 12 animations with the windups 120/300/320', () => {
    const anims = ref.exportJson['animations'] as Record<
      string,
      { frame_ms: number[]; impact_frame?: number }
    >;
    expect(Object.keys(anims)).toHaveLength(12);
    const windup = (a: string) => {
      const { frame_ms, impact_frame } = anims[a] ?? { frame_ms: [] };
      return frame_ms.slice(0, impact_frame).reduce((x, y) => x + y, 0);
    };
    expect([windup('attack_light'), windup('attack_heavy'), windup('special')]).toEqual([
      120, 300, 320,
    ]);
  });
});
