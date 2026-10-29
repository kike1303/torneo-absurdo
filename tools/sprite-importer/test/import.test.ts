import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterAll, describe, expect, it } from 'vitest';
import { validateGuide } from '@torneo/renderer/guide';
import { validateManifest } from '@torneo/renderer/manifest';
import { loadGolden } from '../src/golden.ts';
import { runImport, type ImportResult } from '../src/import.ts';
import { decodePng, encodePng } from './support/png.ts';
import { copyExportRef, goldenRoot, readExportJson, writeExportJson } from './support/root.ts';
import { cleanupTmp, tmp } from './support/tmp.ts';

afterAll(cleanupTmp);

// 007 RF-B2 and plan-b decision 13, in process, with temporary roots.
const CELL = 192;
const LOCAL = ['content', '_local', 'sprites'];
const COMMITTED = ['content', 'sprites'];

function importTo(inDir: string, rel: readonly string[], root = goldenRoot()) {
  const outDir = path.join(root, ...rel);
  const result = runImport({ root, inDir, outDir });
  return { root, outDir, result };
}

/** No line of the result names a temporary directory (plan-b decision 15). */
function noAbsolutePaths(result: ImportResult): void {
  const text = JSON.stringify(result);
  expect(text).not.toContain(os.tmpdir());
  expect(text).not.toContain(path.basename(tmp()));
  expect(text).not.toContain('/private/');
}

function readJson(file: string): unknown {
  return JSON.parse(readFileSync(file, 'utf8'));
}

function snapshot(dir: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const name of readdirSync(dir).sort()) {
    out[name] = readFileSync(path.join(dir, name)).toString('base64');
  }
  return out;
}

describe('runImport with the reference export', () => {
  it('writes a manifest and sheets that pass validateManifest and validateGuide', () => {
    const { root, outDir, result } = importTo(copyExportRef(), [...LOCAL, 'ref_a']);
    expect(result.issues).toEqual([]);
    expect(result.status).toBe(0);
    noAbsolutePaths(result);
    const json = readJson(path.join(outDir, 'manifest.json'));
    const manifest = validateManifest(json, { expectedId: 'ref_a' });
    expect(manifest.ok).toBe(true);
    if (!manifest.ok) return;
    expect(manifest.manifest.scale).toBeUndefined();
    const sheets = manifest.manifest.sheets.map((s) =>
      decodePng(readFileSync(path.join(outDir, s.file))),
    );
    expect(validateGuide(manifest.manifest, sheets, loadGolden(root) ?? [])).toEqual({ ok: true });
    expect(readdirSync(outDir).sort()).toEqual(['manifest.json', 'sheet-0.png']);
    expect(result.summary).toEqual({
      frames: manifest.manifest.frames.length,
      sheets: 1,
      colors: 2,
      partial_alpha_pixels: 0,
    });
  });

  it('deduplicates repeated frames and keeps the indices of every animation', () => {
    const { outDir } = importTo(copyExportRef(), [...LOCAL, 'ref_a']);
    const manifest = readJson(path.join(outDir, 'manifest.json')) as {
      frames: unknown[];
      animations: Record<string, { frames: number[] }>;
    };
    const total = Object.values(manifest.animations).reduce((n, a) => n + a.frames.length, 0);
    expect(manifest.frames.length).toBeLessThan(total);
    expect(manifest.animations['idle']?.frames).toEqual([0, 1, 2, 1]);
    expect(manifest.animations['attack_light']?.frames).toEqual([3, 3, 4, 5]);
  });

  it('gives the same bytes on two imports', () => {
    const inDir = copyExportRef();
    const a = importTo(inDir, [...LOCAL, 'ref_a']);
    const b = importTo(inDir, [...COMMITTED, 'ref_a']);
    expect(snapshot(a.outDir)).toEqual(snapshot(b.outDir));
  });
});

/** Rewrites an animation with `n` frames, each a copy of a reference step with one pixel changed. */
function extend(dir: string, frameMs: Record<string, number[]>): void {
  const json = readExportJson(dir);
  const anims = json['animations'] as Record<string, { frame_ms: number[] }>;
  let counter = 0;
  for (const [code, anim] of Object.entries(anims)) {
    const folder = path.join(dir, 'frames', code);
    const bases = readdirSync(folder)
      .sort()
      .map((f) => decodePng(readFileSync(path.join(folder, f))).data);
    const ms = frameMs[code] ?? anim.frame_ms;
    rmSync(folder, { recursive: true });
    mkdirSync(folder);
    ms.forEach((_, k) => {
      const rgba = new Uint8Array(bases[k % bases.length] as Uint8Array);
      // An interior pixel of every pose (x 80–100, y 162–178) turns dark: a distinct frame.
      const x = 80 + (counter % 21);
      const y = 162 + Math.floor(counter / 21);
      rgba.set([40, 24, 16, 255], (y * CELL + x) * 4);
      counter++;
      writeFileSync(
        path.join(folder, `${String(k).padStart(2, '0')}.png`),
        encodePng(CELL, CELL, rgba),
      );
    });
    anim.frame_ms = ms;
  }
  writeExportJson(dir, json);
}

describe('runImport packing', () => {
  it('opens a second sheet at the 101st unique frame', () => {
    const dir = copyExportRef();
    const n = (count: number, ms: number) => Array.from({ length: count }, () => ms);
    // 12 + 12 + 8 + 12 + 12 + 12 + 6 × 3 and the attacks (4 + 5 + 6): 101 frames, all distinct.
    extend(dir, {
      idle: n(12, 150),
      walk: n(12, 50),
      intro: n(8, 50),
      win: n(12, 100),
      ko: n(12, 100),
      knockdown: n(12, 100),
      hit: n(6, 100),
      block: n(6, 100),
      dodge: n(6, 100),
    });
    const { outDir, result } = importTo(dir, [...LOCAL, 'ref_a']);
    expect(result.issues).toEqual([]);
    expect(result.summary?.frames).toBe(101);
    const manifest = readJson(path.join(outDir, 'manifest.json')) as {
      sheets: { file: string; width: number; height: number }[];
    };
    expect(manifest.sheets).toEqual([
      { file: 'sheet-0.png', width: 1920, height: 1920 },
      { file: 'sheet-1.png', width: 192, height: 192 },
    ]);
  });

  it('fails with export.too_many_frames past sheetsMax × 100 unique frames, writing nothing', () => {
    const dir = copyExportRef();
    const json = readExportJson(dir);
    rmSync(path.join(dir, 'frames'), { recursive: true });
    const animations: Record<string, unknown> = {};
    let n = 0;
    for (const code of [
      'idle',
      'attack_light',
      'attack_heavy',
      'special',
      'hit',
      'block',
      'dodge',
    ]) {
      const count = Math.min(64, 401 - n);
      mkdirSync(path.join(dir, 'frames', code), { recursive: true });
      for (let k = 0; k < count; k++, n++) {
        const rgba = new Uint8Array(CELL * CELL * 4);
        rgba[(n + CELL) * 4 + 3] = 255;
        writeFileSync(
          path.join(dir, 'frames', code, `${String(k).padStart(2, '0')}.png`),
          encodePng(CELL, CELL, rgba),
        );
      }
      animations[code] = { frame_ms: Array.from({ length: count }, () => 100), loop: false };
    }
    writeExportJson(dir, { ...json, animations });
    const { outDir, result } = importTo(dir, [...LOCAL, 'ref_a']);
    expect(result).toEqual({
      status: 1,
      issues: [
        {
          code: 'export.too_many_frames',
          file: 'export.json',
          path: '',
          data: { frames: 401, max: 400 },
        },
      ],
    });
    expect(existsSync(outDir)).toBe(false);
  });
});

describe('runImport refuses without writing', () => {
  it('a guide rule (windup too short) fails with 1 and writes nothing', () => {
    const dir = copyExportRef();
    const json = readExportJson(dir);
    (json['animations'] as Record<string, { frame_ms: number[] }>)['attack_light']!.frame_ms = [
      40, 40, 90, 120,
    ];
    writeExportJson(dir, json);
    const { outDir, result } = importTo(dir, [...LOCAL, 'ref_a']);
    expect(result).toEqual({
      status: 1,
      issues: [
        {
          code: 'guide.windup_short',
          file: 'export.json',
          path: '/animations/attack_light/impact_frame',
          data: { windup_ms: 80, min: 100 },
        },
      ],
    });
    expect(existsSync(outDir)).toBe(false);
  });

  it('a pixel rule points at the PNG it came from', () => {
    const dir = copyExportRef();
    // An empty cell in win (a new unique frame) and idle feet one row up.
    writeFileSync(
      path.join(dir, 'frames', 'win', '01.png'),
      encodePng(CELL, CELL, new Uint8Array(CELL * CELL * 4)),
    );
    const { result } = importTo(dir, [...LOCAL, 'ref_a']);
    expect(result.status).toBe(1);
    expect(result.issues).toEqual([
      expect.objectContaining({ code: 'guide.frame_empty', file: 'frames/win/01.png' }),
    ]);
    expect(result.issues[0]?.path).toMatch(/^\/frames\/[0-9]+$/);
  });

  it('a manifest error keeps its path and points at export.json', () => {
    const dir = copyExportRef();
    const json = readExportJson(dir);
    (json['animations'] as Record<string, { frame_ms: number[] }>)['win']!.frame_ms[1] = 5;
    writeExportJson(dir, json);
    const { outDir, result } = importTo(dir, [...LOCAL, 'ref_a']);
    expect(result.status).toBe(1);
    expect(result.issues.map((i) => [i.code.split('.')[0], i.file, i.path])).toEqual([
      ['manifest', 'export.json', '/animations/win/frame_ms/1'],
    ]);
    expect(existsSync(outDir)).toBe(false);
  });

  it.each([
    ['another folder', ['content', 'other', 'ref_a'], 'cli.out_forbidden'],
    ['a path with ..', ['content', 'sprites', '..', '..', 'ref_a'], 'cli.out_forbidden'],
    ['the sprites folder itself', ['content', 'sprites'], 'cli.out_forbidden'],
    ['a folder name that is not an id', ['content', 'sprites', 'Ref-A'], 'cli.out_forbidden'],
    ['another id', ['content', '_local', 'sprites', 'ref_b'], 'cli.out_id_mismatch'],
  ])('--out in %s is %s with 2', (_name, rel, code) => {
    const root = goldenRoot();
    const { outDir, result } = importTo(copyExportRef(), rel, root);
    expect(result.status).toBe(2);
    expect(result.issues.map((i) => i.code)).toEqual([code]);
    noAbsolutePaths(result);
    expect(existsSync(outDir)).toBe(false);
    expect(existsSync(path.join(root, 'content'))).toBe(false);
  });

  it('--out outside the root is cli.out_forbidden', () => {
    const elsewhere = tmp();
    const result = runImport({
      root: goldenRoot(),
      inDir: copyExportRef(),
      outDir: path.join(elsewhere, 'content', 'sprites', 'ref_a'),
    });
    expect(result).toEqual({
      status: 2,
      issues: [{ code: 'cli.out_forbidden', file: '', path: '' }],
    });
    expect(readdirSync(elsewhere)).toEqual([]);
  });

  it('content/sprites as a symlink is cli.out_symlink', () => {
    const root = goldenRoot();
    const real = tmp();
    mkdirSync(path.join(root, 'content'));
    symlinkSync(real, path.join(root, 'content', 'sprites'));
    const { result } = importTo(copyExportRef(), [...COMMITTED, 'ref_a'], root);
    expect(result).toEqual({
      status: 2,
      issues: [{ code: 'cli.out_symlink', file: 'content/sprites', path: '' }],
    });
    expect(readdirSync(real)).toEqual([]);
  });

  it('a destination with the marker of the MUGEN converter is cli.out_unexpected_entry', () => {
    const root = goldenRoot();
    const out = path.join(root, ...LOCAL, 'ref_a');
    mkdirSync(out, { recursive: true });
    writeFileSync(path.join(out, '.sff-converter-output'), '');
    writeFileSync(path.join(out, 'manifest.json'), '{}');
    const { result } = importTo(copyExportRef(), [...LOCAL, 'ref_a'], root);
    expect(result).toEqual({
      status: 2,
      issues: [
        {
          code: 'cli.out_unexpected_entry',
          file: 'content/_local/sprites/ref_a/.sff-converter-output',
          path: '',
        },
      ],
    });
    expect(readFileSync(path.join(out, 'manifest.json'), 'utf8')).toBe('{}');
  });

  it('provenance.json is only accepted in content/sprites/<id>', () => {
    const root = goldenRoot();
    const out = path.join(root, ...LOCAL, 'ref_a');
    mkdirSync(out, { recursive: true });
    writeFileSync(path.join(out, 'provenance.json'), '{}');
    expect(importTo(copyExportRef(), [...LOCAL, 'ref_a'], root).result.issues).toEqual([
      {
        code: 'cli.out_unexpected_entry',
        file: 'content/_local/sprites/ref_a/provenance.json',
        path: '',
      },
    ]);
  });

  it('a missing or invalid golden is io.golden_invalid with 2', () => {
    const root = tmp();
    expect(importTo(copyExportRef(), [...LOCAL, 'ref_a'], root).result).toEqual({
      status: 2,
      issues: [{ code: 'io.golden_invalid', file: '', path: '' }],
    });
  });
});

describe('runImport over an existing set', () => {
  it('keeps provenance.json untouched and replaces the sheets without leftovers', () => {
    const root = goldenRoot();
    const out = path.join(root, ...COMMITTED, 'ref_a');
    mkdirSync(out, { recursive: true });
    const provenance = path.join(out, 'provenance.json');
    writeFileSync(provenance, '{"kept":true}\n');
    for (const k of [0, 1, 2, 3]) writeFileSync(path.join(out, `sheet-${k}.png`), 'old');
    writeFileSync(path.join(out, 'manifest.json'), '{}');
    const before = statSync(provenance);
    const { result } = importTo(copyExportRef(), [...COMMITTED, 'ref_a'], root);
    expect(result.status).toBe(0);
    expect(readdirSync(out).sort()).toEqual(['manifest.json', 'provenance.json', 'sheet-0.png']);
    expect(readFileSync(provenance, 'utf8')).toBe('{"kept":true}\n');
    expect(statSync(provenance).mtimeMs).toBe(before.mtimeMs);
    expect(readFileSync(path.join(out, 'sheet-0.png')).subarray(1, 4).toString()).toBe('PNG');
  });
});
