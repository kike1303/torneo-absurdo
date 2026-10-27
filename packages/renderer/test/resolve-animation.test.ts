import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
  ANIMATION_FALLBACK,
  resolveAnimation,
  validateManifest,
  windupMs,
  type SpriteManifest,
} from '../src/manifest/index.ts';
import { applyOps, loadFixtures } from './support/patch.ts';
import { DUMMY_A } from './support/placeholders.ts';

// RF-A5: the fallback is fixed by the format, not by each manifest.
const { valid } = loadFixtures(fileURLToPath(new URL('./fixtures/manifest/', import.meta.url)));

function manifest(json: unknown): SpriteManifest {
  const result = validateManifest(json);
  if (!result.ok) throw new Error(JSON.stringify(result.errors));
  return result.manifest;
}

describe('resolveAnimation', () => {
  const minimal = manifest(valid);

  it.each([
    ['intro', 'idle'],
    ['walk', 'idle'],
    ['knockdown', 'hit'],
  ] as const)('%s falls back to %s when missing', (code, fallback) => {
    expect(ANIMATION_FALLBACK[code]).toBe(fallback);
    const resolved = resolveAnimation(minimal, code);
    expect(resolved.code).toBe(fallback);
    expect(resolved.animation).toBe(minimal.animations[fallback]);
  });

  it.each(['intro', 'walk', 'knockdown'] as const)('%s plays itself when present', (code) => {
    const own = { frames: [1], frame_ms: [120], loop: code === 'walk' };
    const m = manifest(applyOps(valid, [{ op: 'set', path: `/animations/${code}`, value: own }]));
    expect(resolveAnimation(m, code)).toEqual({ code, animation: own });
  });

  it('required animations resolve to themselves', () => {
    expect(resolveAnimation(minimal, 'hit').code).toBe('hit');
  });

  it('windupMs sums frame_ms before the impact frame', () => {
    expect(windupMs(DUMMY_A.animations.attack_light)).toBe(120);
    expect(windupMs(DUMMY_A.animations.hit)).toBe(0);
  });
});
