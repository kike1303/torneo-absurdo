import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { MANIFEST_CODES, validateManifest } from '../src/manifest/index.ts';
import { applyOps, loadFixtures } from './support/patch.ts';

// RF-A4 and RF-A5: a minimal valid manifest passes, and one fixture per rule fails with its code
// and JSON Pointer.
const { valid, rules } = loadFixtures(
  fileURLToPath(new URL('./fixtures/manifest/', import.meta.url)),
);

describe('validateManifest', () => {
  it('accepts the minimal valid manifest', () => {
    const result = validateManifest(valid, { expectedId: 'mini' });
    expect(result).toMatchObject({ ok: true });
  });

  it('covers every rule code with a fixture', () => {
    const covered = new Set(rules.map((r) => r.expect.code));
    const structural = new Set<string>([
      MANIFEST_CODES.notObject,
      MANIFEST_CODES.invalidType,
      MANIFEST_CODES.missingField,
      MANIFEST_CODES.tooManyErrors,
    ]);
    const missing = Object.values(MANIFEST_CODES).filter(
      (c) => !covered.has(c) && !structural.has(c),
    );
    expect(missing).toEqual([]);
  });

  it.each(rules.map((r) => [r.name, r] as const))('%s fails with its code', (_name, rule) => {
    const result = validateManifest(applyOps(valid, rule.ops), rule.options);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.errors).toContainEqual(expect.objectContaining(rule.expect));
  });

  it('rejects non-objects and missing fields with codes', () => {
    expect(validateManifest(null)).toEqual({
      ok: false,
      errors: [{ code: 'manifest.not_object', path: '' }],
    });
    const noId = applyOps(valid, [{ op: 'delete', path: '/id' }]);
    expect(validateManifest(noId)).toMatchObject({
      ok: false,
      errors: [{ code: 'manifest.missing_field', path: '/id' }],
    });
  });

  it('treats __proto__ as an unknown animation without touching prototypes', () => {
    const text = JSON.stringify(valid).replace(
      '"animations":{',
      '"animations":{"__proto__":{"frames":[0],"frame_ms":[100],"loop":true,"polluted":true},',
    );
    const json: unknown = JSON.parse(text);
    const result = validateManifest(json);
    expect(result).toEqual({
      ok: false,
      errors: [{ code: 'manifest.unknown_animation', path: '/animations/__proto__' }],
    });
    expect(Object.hasOwn(Object.prototype, 'polluted')).toBe(false);
    expect(({} as Record<string, unknown>).frames).toBeUndefined();
  });

  it('stops at 100 errors with too_many_errors', () => {
    const frames = Array.from({ length: 101 }, () => ({
      sheet: 0,
      x: 0,
      y: 0,
      w: 0,
      h: 1,
      pivot_x: 0,
      pivot_y: 0,
    }));
    const result = validateManifest(
      applyOps(valid, [{ op: 'set', path: '/frames', value: frames }]),
    );
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.errors).toHaveLength(101);
    expect(result.errors.slice(0, 100).every((e) => e.code === 'manifest.frame_size')).toBe(true);
    expect(result.errors[100]?.code).toBe('manifest.too_many_errors');
  });

  it('returns a clean copy with only the known fields', () => {
    const result = validateManifest(valid);
    expect(result.ok && result.manifest).toEqual(valid);
    expect(result.ok && result.manifest).not.toBe(valid);
  });
});
