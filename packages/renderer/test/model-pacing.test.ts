import { describe, expect, it } from 'vitest';
import dummyA from '../placeholders/dummy_a/manifest.json' with { type: 'json' };
import dummyB from '../placeholders/dummy_b/manifest.json' with { type: 'json' };
import { LIMITS, validateManifest, type SpriteManifest } from '../src/manifest/index.ts';
import { BOTARGA_PACE, paceAssets, paceManifest } from '../src/model/pacing.ts';
import { validateTimeline } from '../src/timeline/validate.ts';
import { GOLDEN_NAMES, goldenTimeline, spikeTimeline } from './support/scenes.ts';

function paced(json: unknown, id: string): SpriteManifest {
  const result = validateManifest(paceManifest(json), { expectedId: id });
  if (!result.ok) throw new Error(JSON.stringify(result.errors));
  return result.manifest;
}

const sum = (ms: readonly number[]) => ms.reduce((a, b) => a + b, 0);

describe('paceManifest (017)', () => {
  it('stretches each action to its pace and keeps the manifest valid', () => {
    for (const [json, id] of [
      [dummyA, 'dummy_a'],
      [dummyB, 'dummy_b'],
    ] as const) {
      const manifest = paced(json, id);
      for (const [code, pace] of Object.entries(BOTARGA_PACE)) {
        const animation = manifest.animations[code as keyof typeof manifest.animations];
        if (animation === undefined) continue;
        expect(sum(animation.frame_ms), `${id} ${code}`).toBe(pace.total);
        if (pace.windup !== undefined && animation.impact_frame !== undefined) {
          const windup = sum(animation.frame_ms.slice(0, animation.impact_frame));
          expect(windup, `${id} ${code}`).toBe(pace.windup);
          expect(windup).toBeLessThanOrEqual(LIMITS.windupMaxMs);
        }
      }
    }
  });

  it('is slower than the sprites it starts from', () => {
    const before = validateManifest(dummyA, { expectedId: 'dummy_a' });
    if (!before.ok) throw new Error('dummy_a');
    const after = paced(dummyA, 'dummy_a');
    for (const code of Object.keys(BOTARGA_PACE) as (keyof typeof after.animations)[]) {
      const a = after.animations[code];
      const b = before.manifest.animations[code];
      if (a !== undefined && b !== undefined)
        expect(sum(a.frame_ms)).toBeGreaterThan(sum(b.frame_ms));
    }
  });

  it('leaves the other animations, and what it does not recognize, as they are', () => {
    const manifest = paceManifest(dummyA) as typeof dummyA;
    expect(manifest.animations.idle).toEqual(dummyA.animations.idle);
    expect(manifest.animations.ko).toEqual(dummyA.animations.ko);
    for (const junk of [null, 7, 'x', [], { animations: 3 }]) expect(paceManifest(junk)).toBe(junk);
  });

  it('keeps every golden timeline and the spike valid', () => {
    const manifests = new Map([
      ['dummy_a', paced(dummyA, 'dummy_a')],
      ['dummy_b', paced(dummyB, 'dummy_b')],
    ]);
    for (const timeline of [spikeTimeline(), ...GOLDEN_NAMES.map((n) => goldenTimeline(n))]) {
      expect(validateTimeline(timeline, manifests).ok).toBe(true);
    }
  });

  it('paces every asset of a map', () => {
    const out = paceAssets(new Map([['dummy_a', dummyA]]));
    expect(out.get('dummy_a')).toEqual(paceManifest(dummyA));
  });
});
