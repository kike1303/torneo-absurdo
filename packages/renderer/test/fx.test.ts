import { describe, expect, it } from 'vitest';
import {
  FLASH_MS,
  MAX_PARTICLES,
  NUMBER_LIFE_MS,
  SHAKE_AMPLITUDE,
  SHAKE_MS,
  ZOOM_MS,
  ZOOM_PEAK,
  admitFlashes,
  damageNumberAt,
  particlesAt,
  shakeOffset,
  type ShakeKind,
} from '../src/fx/index.ts';
import { prepareScene, sceneAt, type PreparedScene } from '../src/scene/index.ts';
import type { Timeline } from '../src/timeline/index.ts';
import { MANIFESTS } from './support/placeholders.ts';
import { spikeScene, spikeTimeline, times } from './support/scenes.ts';
import { timeline } from './support/timeline-builder.ts';

// RF-A10 and the pure part of RF-A12.
const OPTS = { reducedMotion: false };

describe('effects as functions of (seed, τ)', () => {
  it('shake is null outside [0, 250] ms, light on hit and strong on crit and special', () => {
    const peak = (kind: ShakeKind) =>
      Math.max(
        ...times(0, SHAKE_MS, 1).map((tau) => {
          const o = shakeOffset(99, kind, tau);
          return Math.max(Math.abs(o.x), Math.abs(o.y));
        }),
      );
    for (const tau of [-1, SHAKE_MS, SHAKE_MS + 1, 1000]) {
      expect(shakeOffset(99, 'crit', tau)).toEqual({ x: 0, y: 0 });
    }
    expect(SHAKE_MS).toBeLessThanOrEqual(250);
    expect(peak('hit')).toBeLessThan(peak('crit'));
    expect(peak('crit')).toBe(peak('special'));
    expect(peak('crit')).toBeLessThanOrEqual(SHAKE_AMPLITUDE.crit);
  });

  it('damage numbers live at most 900 ms', () => {
    expect(NUMBER_LIFE_MS).toBeLessThanOrEqual(900);
    expect(damageNumberAt(5, NUMBER_LIFE_MS - 1, false, false)).not.toBeNull();
    expect(damageNumberAt(5, NUMBER_LIFE_MS, false, false)).toBeNull();
    expect(damageNumberAt(5, 0, true, false)?.scale).toBeGreaterThan(
      damageNumberAt(5, 0, false, false)?.scale ?? Infinity,
    );
  });

  it('a particle at τ is the same computed directly or after other τ', () => {
    const direct = particlesAt(1234, 'crit', 200, 300, 260, 1);
    for (const tau of [0, 50, 399, 120]) particlesAt(1234, 'crit', tau, 300, 260, 1);
    expect(particlesAt(1234, 'crit', 200, 300, 260, 1)).toEqual(direct);
    expect(direct.length).toBeGreaterThan(0);
  });

  it('admits at most 3 flash starts in any 1 s window', () => {
    expect(admitFlashes([0, 100, 200, 300, 1000, 1100, 1150])).toEqual([
      true,
      true,
      true,
      false,
      true,
      true,
      false,
    ]);
  });
});

function maxPerWindow(starts: readonly number[], window = 1000): number {
  let max = 0;
  for (const s of starts)
    max = Math.max(max, starts.filter((x) => x >= s && x < s + window).length);
  return max;
}

function checkScene(prepared: PreparedScene, tl: Timeline) {
  const flashStarts = new Set<number>();
  const zoomSources = tl.cues
    .filter(
      (c) =>
        (c.type === 'strike' &&
          c.move === 'special' &&
          c.result !== 'dodge' &&
          c.result !== 'block') ||
        c.type === 'ko',
    )
    .map((c) => c.at_ms);
  let maxParticles = 0;
  for (const t of times(0, tl.duration_ms, 8)) {
    const scene = sceneAt(prepared, t, OPTS);
    const particles = scene.effects.filter((e) => e.kind === 'particle').length;
    maxParticles = Math.max(maxParticles, particles);
    expect(scene.camera.zoom).toBeLessThanOrEqual(ZOOM_PEAK);
    if (scene.camera.zoom > 1) {
      expect(zoomSources.some((s) => t > s && t - s < ZOOM_MS)).toBe(true);
    }
    for (const e of scene.effects) {
      if (e.kind === 'flash') flashStarts.add(tl.cues[e.cue]?.at_ms ?? -1);
    }
  }
  return { maxParticles, flashStarts: [...flashStarts] };
}

describe('effects on the scene', () => {
  it('the spike script respects the particle cap, the zoom limits and ≤ 3 flashes/s', () => {
    const result = checkScene(spikeScene(), spikeTimeline());
    expect(result.maxParticles).toBeGreaterThan(0);
    expect(result.maxParticles).toBeLessThanOrEqual(MAX_PARTICLES);
    expect(result.flashStarts.length).toBeGreaterThan(0);
    expect(maxPerWindow(result.flashStarts)).toBeLessThanOrEqual(3);
  });

  it('special strikes every 600 ms never exceed 64 live particles', () => {
    const b = timeline({ maxHp: 100_000 });
    for (let i = 0; i < 20; i++) {
      b.strike(2000 + i * 600, i % 2 === 0 ? 'a' : 'b', 'special', 'crit', 10);
    }
    const tl = b.finish(2000 + 20 * 600);
    const result = checkScene(prepareScene(tl, MANIFESTS), tl);
    expect(result.maxParticles).toBeLessThanOrEqual(MAX_PARTICLES);
    expect(maxPerWindow(result.flashStarts)).toBeLessThanOrEqual(3);
  });

  it('flashes last FLASH_MS', () => {
    const scene = spikeScene();
    const flashAt = (t: number) => sceneAt(scene, t, OPTS).effects.some((e) => e.kind === 'flash');
    expect(flashAt(3200)).toBe(true);
    expect(flashAt(3200 + FLASH_MS)).toBe(false);
  });
});
