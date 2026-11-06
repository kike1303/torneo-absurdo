import { Object3D, Vector3 } from 'three';
import { describe, expect, it } from 'vitest';
import dummyA from '../placeholders/dummy_a/manifest.json' with { type: 'json' };
import dummyB from '../placeholders/dummy_b/manifest.json' with { type: 'json' };
import { validateManifest, type SpriteManifest } from '../src/manifest/index.ts';
import {
  CIRCLE_ANGLE,
  CIRCLE_PERIOD_MS,
  SETTLE_MS,
  SIDESTEP_M,
  arenaAngle,
  depthAt,
  sidestep,
} from '../src/model/circling.ts';
import { paceManifest } from '../src/model/pacing.ts';
import { prepareScene } from '../src/scene/prepare.ts';
import { sceneAt } from '../src/scene/scene-at.ts';
import { GROUND_Y, MARK_X, STAGE_WIDTH, type FighterScene } from '../src/scene/types.ts';
import { arenaPoint, facingYaw, worldX, worldY } from '../src/three/world.ts';
import { MANIFESTS } from './support/placeholders.ts';
import {
  GOLDEN_NAMES,
  goldenTimeline,
  spikeTimeline,
  syntheticTimeline,
  times,
} from './support/scenes.ts';

function paced(json: unknown, id: string): SpriteManifest {
  const result = validateManifest(paceManifest(json), { expectedId: id });
  if (!result.ok) throw new Error(JSON.stringify(result.errors));
  return result.manifest;
}

/** The botarga's pace (3D), where a counterattack cuts the 750 ms dodge halfway. */
const PACED = new Map([
  ['dummy_a', paced(dummyA, 'dummy_a')],
  ['dummy_b', paced(dummyB, 'dummy_b')],
]);

const moving = { reducedMotion: false };

describe('arenaAngle (018)', () => {
  it('turns the axis slowly, the same to each side, within its bound', () => {
    expect(arenaAngle({ t: 0, ...moving })).toBe(0);
    expect(arenaAngle({ t: CIRCLE_PERIOD_MS / 4, ...moving })).toBeCloseTo(CIRCLE_ANGLE);
    expect(arenaAngle({ t: (3 * CIRCLE_PERIOD_MS) / 4, ...moving })).toBeCloseTo(-CIRCLE_ANGLE);
    for (const t of times(0, 120_000, 37))
      expect(Math.abs(arenaAngle({ t, ...moving }))).toBeLessThanOrEqual(CIRCLE_ANGLE);
  });

  it('never jumps between two frames', () => {
    for (const t of times(0, 120_000, 16))
      expect(
        Math.abs(arenaAngle({ t: t + 16, ...moving }) - arenaAngle({ t, ...moving })),
      ).toBeLessThan(0.01);
  });

  it('stays at 0 with reduced motion', () => {
    for (const t of times(0, 60_000, 250)) expect(arenaAngle({ t, reducedMotion: true })).toBe(0);
  });
});

describe('sidestep (018)', () => {
  const dodge = (elapsed: number, start = 4000) => ({
    side: 'b' as const,
    animation: 'dodge' as const,
    elapsed,
    total: 750,
    dodgeCut: null,
    t: start + elapsed,
  });

  it('steps out and back over a dodge, never further than its bound', () => {
    const at = (elapsed: number) => {
      const f = dodge(elapsed);
      return sidestep(f, { t: f.t, ...moving });
    };
    expect(at(0)).toBeCloseTo(0);
    expect(Math.abs(at(375))).toBeCloseTo(SIDESTEP_M);
    expect(at(750)).toBeCloseTo(0);
    for (let e = 0; e <= 750; e += 25) expect(Math.abs(at(e))).toBeLessThanOrEqual(SIDESTEP_M);
  });

  it('keeps one side for the whole dodge, and both sides happen', () => {
    const sides = new Set<number>();
    for (let start = 1000; start < 40_000; start += 997) {
      const signs = new Set<number>();
      for (let e = 25; e < 750; e += 50) {
        const f = dodge(e, start);
        signs.add(Math.sign(sidestep(f, { t: f.t, ...moving })));
      }
      expect(signs.size).toBe(1);
      sides.add([...signs][0] as number);
    }
    expect(sides).toEqual(new Set([1, -1]));
  });

  it('is 0 for anything but a dodge, and with reduced motion', () => {
    const f = dodge(375);
    expect(sidestep({ ...f, animation: 'hit' }, { t: f.t, ...moving })).toBe(0);
    expect(sidestep(f, { t: f.t, reducedMotion: true })).toBe(0);
  });

  it('eases back from a dodge cut short, with no jump', () => {
    const start = 4000;
    const cutAt = start + 430;
    const before = dodge(429, start);
    const last = sidestep(before, { t: before.t, ...moving });
    const after = (t: number): number =>
      sidestep(
        {
          side: 'b',
          animation: 'attack_light',
          elapsed: t - cutAt,
          total: 600,
          dodgeCut: { start, at: cutAt, total: 750 },
        },
        { t, ...moving },
      );
    expect(Math.abs(last)).toBeGreaterThan(0.25);
    expect(after(cutAt)).toBeCloseTo(last, 2);
    expect(Math.abs(after(cutAt + SETTLE_MS / 2))).toBeLessThan(Math.abs(last));
    expect(after(cutAt + SETTLE_MS)).toBe(0);
  });

  it('never jumps between two frames in the fights, at the 2D and at the 3D pace', () => {
    const scenes = [
      ...[MANIFESTS, PACED].flatMap((manifests) =>
        [spikeTimeline(), ...GOLDEN_NAMES.map((n) => goldenTimeline(n))].map((tl) =>
          prepareScene(tl, manifests),
        ),
      ),
      prepareScene(syntheticTimeline(), PACED),
    ];
    let cuts = 0;
    for (const prepared of scenes) {
      const last = { a: 0, b: 0 };
      const was = { a: '', b: '' };
      for (const t of times(0, prepared.durationMs, 16)) {
        const scene = sceneAt(prepared, t, moving);
        for (const f of Object.values(scene.fighters)) {
          const now = sidestep(f, scene);
          // The fastest the step moves: a 400 ms dodge (2D pace) at its middle, ~0.035 m per
          // 16 ms frame. Snapping back from a cut dodge was ~0.27 m.
          expect(Math.abs(now - last[f.side]), `${f.side} at ${t}`).toBeLessThan(0.04);
          if (was[f.side] === 'dodge' && f.animation !== 'dodge' && f.animation !== 'idle') cuts++;
          last[f.side] = now;
          was[f.side] = f.animation;
        }
      }
    }
    // The 3D pace does cut dodges short, so the bound above is not vacuous.
    expect(cuts).toBeGreaterThan(5);
  });
});

describe('depthAt (018)', () => {
  const fighter = (side: 'a' | 'b', x: number, elapsed: number): FighterScene =>
    ({
      side,
      x,
      animation: elapsed < 0 ? 'idle' : 'dodge',
      elapsed: Math.max(0, elapsed),
      total: 750,
      dodgeCut: null,
    }) as FighterScene;
  const scene = (a: FighterScene, b: FighterScene) => ({ t: 4375, ...moving, fighters: { a, b } });

  it('is the sidestep of the fighter at its x, and goes from one to the other in between', () => {
    const s = scene(fighter('a', MARK_X.a, -1), fighter('b', MARK_X.b, 375));
    const step = sidestep(s.fighters.b, s);
    expect(Math.abs(step)).toBeCloseTo(SIDESTEP_M);
    expect(depthAt(MARK_X.a, s)).toBe(0);
    expect(depthAt(MARK_X.b, s)).toBeCloseTo(step);
    expect(depthAt(MARK_X.b + 20, s)).toBeCloseTo(step);
    expect(depthAt((MARK_X.a + MARK_X.b) / 2, s)).toBeCloseTo(step / 2);
  });

  it('is 0 with nobody off the axis', () => {
    const s = scene(fighter('a', MARK_X.a, -1), fighter('b', MARK_X.b, -1));
    expect(depthAt(STAGE_WIDTH / 2, s)).toBe(0);
  });
});

describe('arenaPoint (018)', () => {
  it('is the 2D mapping with the axis unturned', () => {
    expect(arenaPoint(MARK_X.a, 100, 0)).toEqual({ x: worldX(MARK_X.a), y: worldY(100), z: 0 });
  });

  it('turns around the center of the stage and keeps the distance between the fighters', () => {
    const center = arenaPoint(STAGE_WIDTH / 2, GROUND_Y, 0.3);
    expect(center.x).toBeCloseTo(0);
    expect(center.z).toBeCloseTo(0);
    const a = arenaPoint(MARK_X.a, GROUND_Y, 0.3);
    const b = arenaPoint(MARK_X.b, GROUND_Y, 0.3);
    expect(Math.hypot(b.x - a.x, b.z - a.z)).toBeCloseTo(worldX(MARK_X.b) - worldX(MARK_X.a));
  });

  it('steps off the axis at a right angle to it', () => {
    const on = arenaPoint(MARK_X.b, GROUND_Y, 0.3);
    const off = arenaPoint(MARK_X.b, GROUND_Y, 0.3, 0.25);
    const along = { x: Math.cos(0.3), z: -Math.sin(0.3) };
    const step = { x: off.x - on.x, z: off.z - on.z };
    expect(Math.hypot(step.x, step.z)).toBeCloseTo(0.25);
    expect(step.x * along.x + step.z * along.z).toBeCloseTo(0);
  });

  it('faces each fighter towards the other along the turned axis (review of #102)', () => {
    const body = new Object3D();
    const front = new Vector3();
    for (const angle of [-CIRCLE_ANGLE, 0, CIRCLE_ANGLE]) {
      const a = arenaPoint(MARK_X.a, GROUND_Y, angle);
      const b = arenaPoint(MARK_X.b, GROUND_Y, angle);
      const towardsB = new Vector3(b.x - a.x, 0, b.z - a.z).normalize();
      body.rotation.y = facingYaw('a', angle, 0);
      body.updateMatrixWorld();
      expect(body.getWorldDirection(front).dot(towardsB)).toBeCloseTo(1);
      body.rotation.y = facingYaw('b', angle, 0);
      body.updateMatrixWorld();
      expect(body.getWorldDirection(front).dot(towardsB)).toBeCloseTo(-1);
      // And the turn to the camera brings both faces towards +z.
      for (const side of ['a', 'b'] as const) {
        body.rotation.y = facingYaw(side, 0);
        body.updateMatrixWorld();
        expect(body.getWorldDirection(front).z).toBeGreaterThan(0);
      }
    }
  });
});
