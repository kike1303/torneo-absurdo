import { describe, expect, it } from 'vitest';
import {
  CIRCLE_ANGLE,
  CIRCLE_PERIOD_MS,
  SIDESTEP_M,
  arenaAngle,
  sidestep,
} from '../src/model/circling.ts';
import { sceneAt } from '../src/scene/scene-at.ts';
import { GROUND_Y, MARK_X, STAGE_WIDTH } from '../src/scene/types.ts';
import { arenaPoint, worldX, worldY } from '../src/three/world.ts';
import { GOLDEN_NAMES, goldenScene, spikeScene, times } from './support/scenes.ts';

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

  it('only moves fighters that dodge in the real fights', () => {
    for (const prepared of [spikeScene(), ...GOLDEN_NAMES.map((n) => goldenScene(n))]) {
      for (const t of times(0, prepared.durationMs, 50)) {
        const scene = sceneAt(prepared, t, moving);
        for (const f of Object.values(scene.fighters))
          if (f.animation !== 'dodge') expect(sidestep(f, scene)).toBe(0);
      }
    }
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
});
