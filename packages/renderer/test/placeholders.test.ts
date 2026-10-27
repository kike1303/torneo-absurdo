import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { PNG } from 'pngjs';
import { describe, expect, it } from 'vitest';
import {
  OPTIONAL_ANIMATIONS,
  REQUIRED_ANIMATIONS,
  validateManifest,
} from '../src/manifest/index.ts';
import { PLACEHOLDER_IDS, drawPlaceholder } from '../scripts/placeholders/draw.ts';
import { readIhdr } from './support/png.ts';

// RF-A6 and plan-a1 decision 11: the committed output matches the generator, pixel by pixel.
const dir = (id: string) => fileURLToPath(new URL(`../placeholders/${id}/`, import.meta.url));

describe.each(PLACEHOLDER_IDS)('placeholder %s', (id) => {
  const json: unknown = JSON.parse(readFileSync(`${dir(id)}manifest.json`, 'utf8'));
  const result = validateManifest(json, { expectedId: id });

  it('passes validateManifest with the 12 animations', () => {
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(Object.keys(result.manifest.animations).sort()).toEqual(
      [...REQUIRED_ANIMATIONS, ...OPTIONAL_ANIMATIONS].sort(),
    );
  });

  it('has sheets whose IHDR matches sheets[], RGBA 8 bit, with every frame inside', () => {
    if (!result.ok) throw new Error('invalid manifest');
    const { sheets, frames } = result.manifest;
    for (const sheet of sheets) {
      const header = readIhdr(readFileSync(`${dir(id)}${sheet.file}`));
      expect(header).toEqual({
        width: sheet.width,
        height: sheet.height,
        bitDepth: 8,
        colorType: 6,
      });
      expect(sheet.width).toBeLessThanOrEqual(1024);
      expect(sheet.height).toBeLessThanOrEqual(512);
    }
    for (const f of frames) {
      const sheet = sheets[f.sheet];
      expect(sheet).toBeDefined();
      if (!sheet) continue;
      expect(f.x + f.w).toBeLessThanOrEqual(sheet.width);
      expect(f.y + f.h).toBeLessThanOrEqual(sheet.height);
    }
  });

  it('is fresh: JSON and decoded pixels equal drawPlaceholder()', () => {
    const drawn = drawPlaceholder(id);
    expect(json).toEqual(JSON.parse(JSON.stringify(drawn.manifest)));
    for (const sheet of drawn.sheets) {
      const png = PNG.sync.read(readFileSync(`${dir(id)}${sheet.file}`));
      expect([png.width, png.height]).toEqual([sheet.width, sheet.height]);
      expect(Buffer.compare(png.data, Buffer.from(sheet.data))).toBe(0);
    }
  });
});

describe('placeholders differ', () => {
  it('in frame size (silhouette) and in pixels (color)', () => {
    const a = drawPlaceholder('dummy_a');
    const b = drawPlaceholder('dummy_b');
    expect(a.manifest.frames[0]).not.toMatchObject({
      w: b.manifest.frames[0]?.w,
      h: b.manifest.frames[0]?.h,
    });
    expect(Buffer.from(a.sheets[0]?.data ?? []).equals(Buffer.from(b.sheets[0]?.data ?? []))).toBe(
      false,
    );
  });
});
