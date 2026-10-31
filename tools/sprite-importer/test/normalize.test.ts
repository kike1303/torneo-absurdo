import { existsSync, mkdirSync, readFileSync, symlinkSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { afterAll, describe, expect, it } from 'vitest';
import { GUIDE_LIMITS } from '@torneo/renderer/guide';
import { runImport } from '../src/import.ts';
import { PIVOT } from '../src/limits.ts';
import { checkOut, main } from '../src/normalize-cli.ts';
import {
  bottomRow,
  feetCenter,
  medianCut,
  normalizeFrames,
  placeInCell,
  runNormalize,
  scaleImage,
  sharePalette,
  validateRecipe,
  type RawImage,
  type Recipe,
} from '../src/normalize.ts';
import { EXPORT_REF, goldenRoot, readExportJson, REPO } from './support/root.ts';
import { cleanupTmp, tmp } from './support/tmp.ts';

afterAll(cleanupTmp);

// 004: raw frames + recipe → the open export of 007.
const CELL = GUIDE_LIMITS.cellPx;

/** A `w`×`h` image with one opaque rectangle. */
function box(w: number, h: number, x0: number, y0: number, x1: number, y1: number): RawImage {
  const data = new Uint8Array(w * h * 4);
  for (let y = y0; y <= y1; y++) {
    for (let x = x0; x <= x1; x++) data.set([10, 20, 30, 255], (y * w + x) * 4);
  }
  return { width: w, height: h, data };
}

function alphaAt(image: RawImage, x: number, y: number): number {
  return image.data[(y * image.width + x) * 4 + 3] ?? 0;
}

describe('normalize: pixels', () => {
  it('scales by an integer with hard edges', () => {
    const scaled = scaleImage(box(4, 4, 1, 1, 1, 1), 2);
    expect([scaled.width, scaled.height]).toEqual([8, 8]);
    expect(alphaAt(scaled, 2, 2)).toBe(255);
    expect(alphaAt(scaled, 3, 3)).toBe(255);
    expect(alphaAt(scaled, 4, 4)).toBe(0);
  });

  it('finds the ground and the center of the feet', () => {
    const image = box(20, 20, 4, 2, 8, 15);
    expect(bottomRow(image)).toBe(15);
    expect(feetCenter(image, 4)).toBe(6);
    expect(bottomRow(box(4, 4, 0, 0, -1, -1))).toBeUndefined();
  });

  it('puts the feet on the row above the pivot and centered on it', () => {
    const [cell] = normalizeFrames([box(20, 20, 4, 2, 8, 15)], {}, 2);
    expect(cell).toBeDefined();
    const image = cell as RawImage;
    expect([image.width, image.height]).toEqual([CELL, CELL]);
    expect(bottomRow(image)).toBe(PIVOT.y - 1);
    expect(feetCenter(image, 8)).toBe(PIVOT.x);
  });

  it('anchors the whole animation on its first frame unless `ground` is `each`', () => {
    const raw = [box(20, 20, 4, 2, 8, 15), box(20, 20, 4, 2, 8, 12)];
    const first = normalizeFrames(raw, { ground: 'first' }, 1);
    expect(bottomRow(first[1] as RawImage)).toBe(PIVOT.y - 1 - 3);
    const each = normalizeFrames(raw, { ground: 'each' }, 1);
    expect(bottomRow(each[1] as RawImage)).toBe(PIVOT.y - 1);
  });

  it('applies the hand offset of the recipe', () => {
    const [cell] = normalizeFrames([box(20, 20, 4, 2, 8, 15)], { offset: [3, -2] }, 1);
    expect(bottomRow(cell as RawImage)).toBe(PIVOT.y - 1 - 2);
    expect(feetCenter(cell as RawImage, 4)).toBe(PIVOT.x + 3);
  });

  it('cuts the alpha to binary and drops what falls outside the cell', () => {
    const image = box(2, 1, 0, 0, 1, 0);
    image.data[3] = 127;
    image.data[7] = 128;
    const cell = placeInCell(image, 0, 0);
    expect(alphaAt(cell, 0, 0)).toBe(0);
    expect(alphaAt(cell, 1, 0)).toBe(255);
    expect(alphaAt(placeInCell(image, CELL, 0), 0, 0)).toBe(0);
  });
});

describe('normalize: palette', () => {
  const ramp = Array.from({ length: 200 }, (_, i) => [i, 255 - i, (i * 7) % 256] as const);

  it('is deterministic and never exceeds the maximum', () => {
    const palette = medianCut(ramp, 32);
    expect(palette).toHaveLength(32);
    expect(medianCut([...ramp].reverse(), 32)).toEqual(palette);
  });

  it('keeps every color when they already fit', () => {
    expect(medianCut(ramp.slice(0, 3), 32)).toHaveLength(3);
  });

  it('shares one palette across all the images and leaves transparency alone', () => {
    const images = ramp.map((c): RawImage => {
      return { width: 1, height: 1, data: Uint8Array.of(c[0], c[1], c[2], 255) };
    });
    const clear: RawImage = { width: 1, height: 1, data: Uint8Array.of(9, 9, 9, 0) };
    expect(sharePalette([...images, clear], 8)).toBe(8);
    const seen = new Set(images.map((i) => `${i.data[0]},${i.data[1]},${i.data[2]}`));
    expect(seen.size).toBeLessThanOrEqual(8);
    expect([...clear.data]).toEqual([9, 9, 9, 0]);
  });
});

const VALID = {
  format: 'torneo.sprites.recipe',
  format_version: 1,
  id: 'x',
  scale: 2,
  animations: { idle: { frames: [0], frame_ms: [100], loop: true } },
};

describe('normalize: recipe', () => {
  it('accepts a valid recipe', () => {
    expect(validateRecipe(VALID)).toEqual([]);
  });

  it.each<[string, unknown, string]>([
    ['not an object', null, ''],
    ['format', { ...VALID, format: 'other' }, '/format'],
    ['id', { ...VALID, id: 'a text with spaces' }, '/id'],
    [
      'a source outside the input folder',
      {
        ...VALID,
        animations: { idle: { source: '../x', frames: [0], frame_ms: [100], loop: true } },
      },
      '/animations/idle/source',
    ],
    [
      'an unknown ground mode',
      { ...VALID, animations: { idle: { ground: 'x', frames: [0], frame_ms: [100], loop: true } } },
      '/animations/idle/ground',
    ],
    ...(
      [
        ['a text source', 'source', 123],
        ['a source in a list', 'source', ['idle']],
        ['a text loop', 'loop', 'a text'],
        ['a missing loop', 'loop', undefined],
        ['a text impact_frame', 'impact_frame', 'a text'],
        ['a negative impact_frame', 'impact_frame', -1],
        ['a fractional impact_frame', 'impact_frame', 0.5],
        ['a text timing', 'frame_ms', ['a text']],
        ['a zero timing', 'frame_ms', [0]],
        ['a fractional timing', 'frame_ms', [1.5]],
        ['a text offset', 'offset', 'a text'],
        ['an offset of one number', 'offset', [1]],
        ['an offset of three numbers', 'offset', [1, 2, 3]],
        ['a fractional offset', 'offset', [1.5, 0]],
        ['a text inside the offset', 'offset', ['a text', 0]],
      ] as const
    ).map(([name, key, value]): [string, unknown, string] => [
      name,
      {
        ...VALID,
        animations: { idle: { frames: [0], frame_ms: [100], loop: true, [key]: value } },
      },
      `/animations/idle/${key}`,
    ]),
    [
      'an animation name outside the output folder',
      {
        ...VALID,
        animations: { '../../x': { source: 'idle', frames: [0], frame_ms: [100], loop: true } },
      },
      '/animations/../../x',
    ],
    ['scale', { ...VALID, scale: 1.5 }, '/scale'],
    ['max_colors', { ...VALID, max_colors: 1 }, '/max_colors'],
    ['animations', { ...VALID, animations: null }, '/animations'],
    ['a huge scale', { ...VALID, scale: 1000 }, '/scale'],
    ['no animations', { ...VALID, animations: {} }, '/animations'],
    ['a list of animations', { ...VALID, animations: [] }, '/animations'],
    ['a null animation', { ...VALID, animations: { idle: null } }, '/animations/idle'],
    ['an unknown key', { ...VALID, prompt: 'a text' }, '/prompt'],
    [
      'an unknown key of an animation',
      { ...VALID, animations: { idle: { frames: [0], frame_ms: [100], loop: true, note: 'x' } } },
      '/animations/idle/note',
    ],
    [
      'frames and timings of another length',
      { ...VALID, animations: { idle: { frames: [0, 1], frame_ms: [100], loop: true } } },
      '/animations/idle',
    ],
    [
      'a negative frame',
      { ...VALID, animations: { idle: { frames: [-1], frame_ms: [100], loop: true } } },
      '/animations/idle',
    ],
  ])('rejects %s', (_name, recipe, at) => {
    expect(validateRecipe(recipe)).toContainEqual({ code: 'normalize.recipe_invalid', path: at });
  });

  it('accepts every optional field with its right type', () => {
    const idle = {
      source: 'idle_raw',
      frames: [0, 1],
      frame_ms: [100, 120],
      loop: false,
      impact_frame: 1,
      ground: 'each',
      offset: [-2, 3],
    };
    expect(validateRecipe({ ...VALID, animations: { idle } })).toEqual([]);
  });
});

/** Raw frames with the reference export's art: the same cells, as the source of a recipe. */
function recipeOfRef(): { inDir: string; recipe: Recipe } {
  const ref = readExportJson(EXPORT_REF) as {
    animations: Record<string, { frame_ms: number[]; loop: boolean; impact_frame?: number }>;
  };
  const inDir = tmp('sprite-raw-');
  const animations: Record<string, Recipe['animations'][string]> = {};
  for (const [name, a] of Object.entries(ref.animations)) {
    mkdirSync(path.join(inDir, name));
    a.frame_ms.forEach((_, i) => {
      const n = String(i).padStart(2, '0');
      writeFileSync(
        path.join(inDir, name, `${n}.png`),
        readFileSync(path.join(EXPORT_REF, 'frames', name, `${n}.png`)),
      );
    });
    animations[name] = {
      frames: a.frame_ms.map((_, i) => i),
      frame_ms: a.frame_ms,
      loop: a.loop,
      ground: name === 'idle' ? 'each' : 'first',
      ...(a.impact_frame === undefined ? {} : { impact_frame: a.impact_frame }),
    };
  }
  return {
    inDir,
    recipe: {
      format: 'torneo.sprites.recipe',
      format_version: 1,
      id: 'ref_a',
      scale: 1,
      max_colors: 16,
      animations,
    },
  };
}

describe('normalize: end to end', () => {
  it('writes an export that `import` accepts', () => {
    const { inDir, recipe } = recipeOfRef();
    const outDir = path.join(tmp('sprite-out-'), 'ref_a');
    const result = runNormalize({ inDir, recipe, outDir });
    expect(result.status).toBe(0);
    const root = goldenRoot();
    const imported = runImport({
      root,
      inDir: outDir,
      outDir: path.join(root, 'content', '_local', 'sprites', 'ref_a'),
    });
    expect(imported.issues).toEqual([]);
    expect(imported.status).toBe(0);
    expect(imported.summary?.['colors']).toBeLessThanOrEqual(16);
    expect(imported.summary?.['partial_alpha_pixels']).toBe(0);
  });

  it('is byte-for-byte repeatable', () => {
    const { inDir, recipe } = recipeOfRef();
    const a = path.join(tmp('sprite-out-'), 'a');
    const b = path.join(tmp('sprite-out-'), 'b');
    runNormalize({ inDir, recipe, outDir: a });
    runNormalize({ inDir, recipe, outDir: b });
    const file = path.join('frames', 'idle', '00.png');
    expect(readFileSync(path.join(a, file)).equals(readFileSync(path.join(b, file)))).toBe(true);
    expect(readFileSync(path.join(a, 'export.json'), 'utf8')).toBe(
      readFileSync(path.join(b, 'export.json'), 'utf8'),
    );
  });

  it('names a missing source frame and writes nothing', () => {
    const { inDir, recipe } = recipeOfRef();
    const outDir = path.join(tmp('sprite-out-'), 'ref_a');
    const missing: Recipe = {
      ...recipe,
      animations: { idle: { frames: [99], frame_ms: [100], loop: true } },
    };
    expect(runNormalize({ inDir, recipe: missing, outDir })).toEqual({
      status: 1,
      issues: [{ code: 'normalize.source_missing', path: 'idle/99.png' }],
    });
    expect(() => readFileSync(path.join(outDir, 'export.json'))).toThrow();
  });

  it('writes nothing when an animation name would leave the output folder', () => {
    const { inDir, recipe } = recipeOfRef();
    const root = tmp('sprite-out-');
    const result = runNormalize({
      inDir,
      outDir: path.join(root, 'export', 'x'),
      recipe: {
        ...recipe,
        animations: {
          '../../escaped': { source: 'idle', frames: [0], frame_ms: [100], loop: true },
        },
      },
    });
    expect(result.status).toBe(1);
    expect(existsSync(path.join(root, 'escaped'))).toBe(false);
    expect(existsSync(path.join(root, 'export'))).toBe(false);
  });

  it('does not read a corrupt source as a frame', () => {
    const { inDir, recipe } = recipeOfRef();
    writeFileSync(path.join(inDir, 'idle', '00.png'), 'not a png');
    const outDir = path.join(tmp('sprite-out-'), 'ref_a');
    const result = runNormalize({ inDir, recipe, outDir });
    expect(result.status).toBe(1);
  });
});

describe('normalize: CLI', () => {
  function run(argv: readonly string[]): { status: number; lines: unknown[] } {
    const lines: unknown[] = [];
    const status = main(argv, {}, (text) => lines.push(JSON.parse(text)));
    return { status, lines };
  }

  it('asks for its arguments', () => {
    expect(run(['--in', 'x'])).toEqual({
      status: 2,
      lines: [{ code: 'cli.missing_argument', file: '', path: '', data: { arg: 'recipe' } }],
    });
  });

  it('reports a bad recipe before looking at the destination', () => {
    const inDir = tmp('sprite-raw-');
    const recipe = path.join(inDir, 'recipe.json');
    writeFileSync(recipe, JSON.stringify({ ...VALID, id: '../x' }));
    expect(run(['--in', inDir, '--recipe', recipe, '--out', tmp('sprite-out-')])).toEqual({
      status: 1,
      lines: [{ code: 'normalize.recipe_invalid', file: '', path: '/id' }],
    });
  });

  it('refuses the export folder of another id', () => {
    const inDir = tmp('sprite-raw-');
    const recipe = path.join(inDir, 'recipe.json');
    writeFileSync(recipe, JSON.stringify(VALID));
    const out = path.join(REPO, 'content', '_local', 'export', 'not_the_recipe_id');
    expect(run(['--in', inDir, '--recipe', recipe, '--out', out])).toEqual({
      status: 2,
      lines: [{ code: 'cli.out_id_mismatch', file: '', path: '' }],
    });
    expect(existsSync(out)).toBe(false);
  });

  it('never writes outside content/_local/', () => {
    const inDir = tmp('sprite-raw-');
    const recipe = path.join(inDir, 'recipe.json');
    writeFileSync(recipe, JSON.stringify(VALID));
    expect(run(['--in', inDir, '--recipe', recipe, '--out', tmp('sprite-out-')])).toEqual({
      status: 2,
      lines: [{ code: 'cli.out_forbidden', file: '', path: '' }],
    });
  });
});

/** Whether the disk of the temporary folders opens a file under another case. */
function caseInsensitiveDisk(): boolean {
  const dir = tmp('sprite-case-');
  writeFileSync(path.join(dir, 'Probe'), '');
  return existsSync(path.join(dir, 'probe'));
}

describe('normalize: destination', () => {
  /** A temporary repo root with its raw frames, so nothing touches the real content/_local/. */
  function repo(): { root: string; inDir: string; exportDir: string; out: string } {
    const root = tmp('sprite-root-');
    const inDir = path.join(root, 'content', '_local', 'raw', 'x');
    const exportDir = path.join(root, 'content', '_local', 'export');
    mkdirSync(inDir, { recursive: true });
    return { root, inDir, exportDir, out: path.join(exportDir, 'x') };
  }

  it('accepts content/_local/export/<id>, existing or not', () => {
    const { root, inDir, out } = repo();
    expect(checkOut(root, out, 'x', inDir)).toBeUndefined();
    mkdirSync(out, { recursive: true });
    expect(checkOut(root, out, 'x', inDir)).toBeUndefined();
  });

  it.each([
    ['the local root', ['content', '_local']],
    ['the export root', ['content', '_local', 'export']],
    ['the local sets', ['content', '_local', 'sprites', 'x']],
    ['the raw frames of another set', ['content', '_local', 'raw', 'y']],
    ['the committed sets', ['content', 'sprites', 'x']],
    ['a folder below the set', ['content', '_local', 'export', 'x', 'x']],
    ['the root of the repo', []],
  ])('refuses %s', (_name, parts) => {
    const { root, inDir } = repo();
    expect(checkOut(root, path.join(root, ...parts), 'x', inDir)).toBe('cli.out_forbidden');
  });

  it('sees a source folder whose name starts with dots inside the destination', () => {
    const { root, out } = repo();
    const inDir = path.join(out, '..raw');
    mkdirSync(inDir, { recursive: true });
    expect(checkOut(root, out, 'x', inDir)).toBe('cli.out_forbidden');
  });

  it('refuses a path outside the root', () => {
    const { root, inDir } = repo();
    expect(checkOut(root, tmp('sprite-out-'), 'x', inDir)).toBe('cli.out_forbidden');
  });

  it('refuses the folder of another id', () => {
    const { root, inDir, exportDir } = repo();
    expect(checkOut(root, path.join(exportDir, 'y'), 'x', inDir)).toBe('cli.out_id_mismatch');
  });

  it('never replaces the folder of the raw frames', () => {
    const { root, out } = repo();
    mkdirSync(out, { recursive: true });
    const raw = path.join(out, 'raw');
    mkdirSync(raw);
    expect(checkOut(root, out, 'x', out)).toBe('cli.out_forbidden');
    expect(checkOut(root, out, 'x', raw)).toBe('cli.out_forbidden');
    expect(checkOut(root, out, 'x', path.dirname(out))).toBe('cli.out_forbidden');
  });

  it('sees through a symlink in the path of the raw frames', () => {
    const { root, exportDir, out } = repo();
    mkdirSync(out, { recursive: true });
    const alias = path.join(root, 'content', '_local', 'alias');
    symlinkSync(exportDir, alias);
    expect(checkOut(root, out, 'x', path.join(alias, 'x'))).toBe('cli.out_forbidden');
  });

  describe.runIf(caseInsensitiveDisk())('on a disk that does not tell case apart', () => {
    it('sees the raw frames behind another case', () => {
      const { root, out } = repo();
      mkdirSync(out, { recursive: true });
      const other = path.join(root, 'CONTENT', '_local', 'export', 'x');
      expect(checkOut(root, out, 'x', other)).toBe('cli.out_forbidden');
    });

    it('sees a destination whose folder has another case on disk', () => {
      const { root, exportDir, out } = repo();
      const onDisk = path.join(exportDir, 'X');
      mkdirSync(onDisk, { recursive: true });
      expect(checkOut(root, out, 'x', onDisk)).toBe('cli.out_forbidden');
    });

    it('sees raw frames that hold a destination that does not exist yet', () => {
      const { root, out } = repo();
      const parent = path.join(root, 'content', '_local');
      mkdirSync(path.join(parent, 'EXPORT'), { recursive: true });
      expect(checkOut(root, out, 'x', path.join(parent, 'EXPORT'))).toBe('cli.out_forbidden');
    });
  });

  it('refuses a symlink at any level of the destination', () => {
    const { root, inDir, exportDir, out } = repo();
    const target = tmp('sprite-target-');
    symlinkSync(target, exportDir);
    expect(checkOut(root, out, 'x', inDir)).toBe('cli.out_symlink');
    expect(existsSync(path.join(target, 'x'))).toBe(false);
  });

  it('refuses a symlink as the destination itself', () => {
    const { root, inDir, exportDir, out } = repo();
    mkdirSync(exportDir, { recursive: true });
    symlinkSync(tmp('sprite-target-'), out);
    expect(checkOut(root, out, 'x', inDir)).toBe('cli.out_symlink');
  });

  it('refuses a file on the way', () => {
    const { root, inDir, exportDir, out } = repo();
    writeFileSync(exportDir, '');
    expect(checkOut(root, out, 'x', inDir)).toBe('cli.out_not_a_directory');
  });
});
