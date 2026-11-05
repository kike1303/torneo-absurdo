import { describe, expect, it } from 'vitest';
import dummyA from '../placeholders/dummy_a/manifest.json' with { type: 'json' };
import dummyB from '../placeholders/dummy_b/manifest.json' with { type: 'json' };
import { LIMITS, validateManifest, type SpriteManifest } from '../src/manifest/index.ts';
import { BOTARGA_PACE, paceAssets, paceManifest, stretch } from '../src/model/pacing.ts';
import { prepareScene } from '../src/scene/prepare.ts';
import { sceneAt } from '../src/scene/scene-at.ts';
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

  it('lands every strike on its cue with the paced manifests (review of #99)', () => {
    const manifests = new Map([
      ['dummy_a', paced(dummyA, 'dummy_a')],
      ['dummy_b', paced(dummyB, 'dummy_b')],
    ]);
    let strikes = 0;
    for (const timeline of [spikeTimeline(), ...GOLDEN_NAMES.map((n) => goldenTimeline(n))]) {
      const prepared = prepareScene(timeline, manifests);
      for (const cue of timeline.cues) {
        if (cue.type !== 'strike') continue;
        const f = sceneAt(prepared, cue.at_ms, { reducedMotion: false }).fighters[cue.actor];
        if (f.animation !== cue.move) continue;
        expect(f.elapsed, `${cue.actor} ${cue.move} at ${cue.at_ms}`).toBe(f.windup);
        strikes++;
      }
    }
    expect(strikes).toBeGreaterThan(50);
  });

  it('paces every asset of a map', () => {
    const out = paceAssets(new Map([['dummy_a', dummyA]]));
    expect(out.get('dummy_a')).toEqual(paceManifest(dummyA));
  });
});

/** The `special` of `manifest` with these frame lengths, reusing its drawn frames in turn. */
function withFrames(manifest: typeof dummyA, frameMs: number[]) {
  const special = manifest.animations.special as {
    frames: number[];
    frame_ms: number[];
    impact_frame: number;
  };
  const drawn = special.frames;
  special.frames = frameMs.map((_, i) => drawn[i % drawn.length] as number);
  special.frame_ms = frameMs;
  return special;
}

describe('stretch (review of #99)', () => {
  it('sums exactly the target with every frame inside the limits', () => {
    for (const frames of [
      [60, 60],
      [133, 33, 33, 33, 33, 33, 33, 33, 17],
      [17, 17, 17, 17],
      [500],
    ]) {
      for (const target of [100, 260, 399, 400, 1300]) {
        const out = stretch(frames, target);
        if (out === null) {
          expect(target < frames.length * LIMITS.frameMsMin).toBe(true);
          continue;
        }
        expect(sum(out), `${frames} → ${target}`).toBe(target);
        for (const ms of out) {
          expect(ms).toBeGreaterThanOrEqual(LIMITS.frameMsMin);
          expect(ms).toBeLessThanOrEqual(LIMITS.frameMsMax);
        }
      }
    }
  });

  it('gives null when the frames cannot fit the target', () => {
    expect(
      stretch(
        Array.from({ length: 30 }, () => 33),
        400,
      ),
    ).toBeNull();
  });

  it('keeps a valid special valid when its windup has many short frames', () => {
    // A MUGEN set in ticks of 17 and 33 ms: before the fix its windup went to 401 ms.
    const manifest = structuredClone(dummyA) as typeof dummyA;
    const special = withFrames(manifest, [133, 33, 33, 33, 33, 33, 33, 33, 17, 50, 50]);
    special.impact_frame = 9;
    expect(validateManifest(manifest, { expectedId: 'dummy_a' }).ok).toBe(true);
    const result = validateManifest(paceManifest(manifest), { expectedId: 'dummy_a' });
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const out = result.manifest.animations.special;
    expect(sum(out?.frame_ms.slice(0, 9) ?? [])).toBeLessThanOrEqual(LIMITS.windupMaxMs);
  });

  it('keeps the original animation when the paced one cannot be valid', () => {
    const manifest = structuredClone(dummyA) as typeof dummyA;
    // After the impact, 57 frames of 16 ms cannot shrink to the 900 ms the pace asks for.
    const special = withFrames(manifest, [40, ...Array.from({ length: 57 }, () => 16)]);
    special.impact_frame = 1;
    expect(validateManifest(manifest, { expectedId: 'dummy_a' }).ok).toBe(true);
    const paced = paceManifest(manifest) as typeof dummyA;
    expect(paced.animations.special).toEqual(manifest.animations.special);
  });
});
