import { describe, expect, it } from 'vitest';
import { FRONT_SHOT, shotFor } from '../src/model/shot.ts';
import { sceneAt } from '../src/scene/scene-at.ts';
import { FACING, type FighterScene, type Scene } from '../src/scene/types.ts';
import { GOLDEN_NAMES, goldenScene, spikeScene, times } from './support/scenes.ts';

/** A scene of the spike in the middle of the exchange, with these changes. */
function exchange(t: number, changes: Partial<Scene> = {}): Scene {
  const scene = sceneAt(spikeScene(), 10_000, { reducedMotion: false });
  return { ...scene, t, phase: 'fight', winner: null, ...changes };
}

function withFighter(scene: Scene, fighter: Partial<FighterScene> & Pick<FighterScene, 'side'>) {
  return {
    ...scene,
    fighters: {
      ...scene.fighters,
      [fighter.side]: { ...scene.fighters[fighter.side], ...fighter },
    },
  };
}

describe('shotFor', () => {
  it('keeps the front camera with reduced motion', () => {
    const prepared = spikeScene();
    for (const t of times(0, prepared.durationMs, 250)) {
      expect(shotFor(sceneAt(prepared, t, { reducedMotion: true }))).toEqual(FRONT_SHOT);
    }
  });

  it('pushes in during the intro and lands on the exchange', () => {
    const start = shotFor(exchange(0, { phase: 'intro' }));
    expect(start.distance).toBeGreaterThan(1.3);
    const settled = shotFor(exchange(5000, { phase: 'intro' }));
    expect(settled).toEqual(shotFor(exchange(5000)));
  });

  it('sways the same to each side in the exchange', () => {
    const quarter = shotFor(exchange(3500));
    const threeQuarters = shotFor(exchange(10_500));
    expect(quarter.yaw).toBeGreaterThan(0.2);
    expect(threeQuarters.yaw).toBeCloseTo(-quarter.yaw);
    expect(quarter.distance).toBe(1);
    expect(quarter.focusX).toBeNull();
  });

  it('cuts to a close-up of the side that throws a special, from its front', () => {
    for (const side of ['a', 'b'] as const) {
      const scene = withFighter(exchange(9000), {
        side,
        animation: 'special',
        elapsed: 200,
        total: 900,
      });
      const shot = shotFor(scene);
      expect(Math.sign(shot.yaw)).toBe(FACING[side]);
      expect(shot.distance).toBeLessThan(1);
      expect(Math.abs((shot.focusX ?? 0) - scene.fighters[side].x)).toBeLessThan(40);
    }
  });

  it('starts the KO orbit where the exchange was, then closes on the fallen fighter', () => {
    const t = 9000;
    const first = withFighter(exchange(t), { side: 'b', animation: 'ko', elapsed: 0 });
    const shot = shotFor(first);
    expect(shot.yaw).toBeCloseTo(shotFor(exchange(t)).yaw);
    expect(shot.distance).toBeCloseTo(1);
    expect(shot.focusX).toBeCloseTo((first.fighters.a.x + first.fighters.b.x) / 2);

    const later = withFighter(exchange(t + 2000), { side: 'b', animation: 'ko', elapsed: 2000 });
    const closed = shotFor(later);
    expect(closed.distance).toBeLessThan(0.8);
    expect(closed.focusX).toBeCloseTo(later.fighters.b.x);
  });

  it('orbits the winner once the fight is won', () => {
    const scene = withFighter(exchange(20_000, { phase: 'win', winner: 'a' }), {
      side: 'a',
      animation: 'win',
      elapsed: 3000,
    });
    expect(shotFor(scene).focusX).toBeCloseTo(scene.fighters.a.x);
  });

  it('never jumps from the KO to the win (review of #92)', () => {
    const prepared = spikeScene();
    const ko = prepared.ko;
    expect(ko).not.toBeNull();
    let previous = null as ReturnType<typeof shotFor> | null;
    let fallenSeen = false;
    for (let t = ko?.at ?? 0; t < prepared.durationMs; t += 16) {
      const scene = sceneAt(prepared, t, { reducedMotion: false });
      fallenSeen ||= scene.fighters.a.animation === 'ko' || scene.fighters.b.animation === 'ko';
      if (!fallenSeen) continue;
      const shot = shotFor(scene);
      if (previous !== null) {
        expect(Math.abs(shot.yaw - previous.yaw), `yaw at ${t}`).toBeLessThan(0.02);
        expect(Math.abs(shot.distance - previous.distance), `distance at ${t}`).toBeLessThan(0.02);
        expect(Math.abs((shot.focusX ?? 0) - (previous.focusX ?? 0)), `focus at ${t}`).toBeLessThan(
          6,
        );
      }
      previous = shot;
    }
    expect(fallenSeen).toBe(true);
  });

  it('stays finite and near the framing distance in every golden fight', () => {
    for (const name of GOLDEN_NAMES) {
      const prepared = goldenScene(name);
      for (const t of times(0, prepared.durationMs, 100)) {
        const shot = shotFor(sceneAt(prepared, t, { reducedMotion: false }));
        expect(Number.isFinite(shot.yaw)).toBe(true);
        expect(shot.distance).toBeGreaterThanOrEqual(0.6);
        expect(shot.distance).toBeLessThanOrEqual(1.5);
      }
    }
  });
});
