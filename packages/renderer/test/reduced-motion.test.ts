import { describe, expect, it } from 'vitest';
import { FIXED_CAMERA, GROUND_Y, sceneAt, type PreparedScene } from '../src/scene/index.ts';
import type { Timeline } from '../src/timeline/index.ts';
import {
  goldenScene,
  goldenTimeline,
  spikeScene,
  spikeTimeline,
  spread,
} from './support/scenes.ts';

// RF-A14 (ADR-0009 D7, test 7, on the scene part): reduced motion never changes the timing. Also
// on the goldens that end by time, with the time banner (005 RF10).
const CASES: [string, Timeline, PreparedScene, number[]][] = [
  ['spike', spikeTimeline(), spikeScene(), [3210, 26_050, 57_550]],
  ['log-draw', goldenTimeline('draw'), goldenScene('draw'), []],
  ['log-decision-a', goldenTimeline('decision-a'), goldenScene('decision-a'), []],
];

describe.each(CASES)('reduced motion (%s)', (_name, timeline, prepared, extra) => {
  const SAMPLES = spread(timeline.duration_ms, 200);

  it('keeps fighters, HUD, phase and ending identical for 200 times', () => {
    for (const t of SAMPLES) {
      const full = sceneAt(prepared, t, { reducedMotion: false });
      const reduced = sceneAt(prepared, t, { reducedMotion: true });
      expect(reduced.fighters).toEqual(full.fighters);
      expect(reduced.hud).toEqual(full.hud);
      expect(reduced.phase).toBe(full.phase);
      expect(reduced.ending).toBe(full.ending);
      expect(reduced.banners.map((b) => b.code)).toEqual(full.banners.map((b) => b.code));
    }
  });

  it('has no shake, no zoom, no particles, still numbers and no banner tween', () => {
    let numbers = 0;
    let banners = 0;
    for (let t = 0; t <= timeline.duration_ms; t += 10) {
      const scene = sceneAt(prepared, t, { reducedMotion: true });
      expect(scene.camera).toEqual(FIXED_CAMERA);
      expect(scene.effects.filter((e) => e.kind === 'particle')).toEqual([]);
      for (const e of scene.effects) {
        if (e.kind !== 'damage') continue;
        numbers++;
        expect(e.y).toBe(GROUND_Y - 104);
        expect(e.alpha).toBe(1);
      }
      for (const b of scene.banners) {
        banners++;
        expect(b).toMatchObject({ alpha: 1, scale: 1 });
      }
    }
    expect(numbers).toBeGreaterThan(0);
    expect(banners).toBeGreaterThan(0);
  });

  it('the full-motion scene does move the camera somewhere', () => {
    const moved = SAMPLES.concat(extra).some((t) => {
      const c = sceneAt(prepared, t, { reducedMotion: false }).camera;
      return c.zoom !== 1 || c.offsetX !== 0 || c.offsetY !== 0;
    });
    expect(moved).toBe(true);
  });
});
