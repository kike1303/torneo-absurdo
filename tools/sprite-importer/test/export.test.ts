import {
  mkdirSync,
  renameSync,
  rmSync,
  symlinkSync,
  truncateSync,
  unlinkSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import { afterAll, describe, expect, it } from 'vitest';
import { readExport } from '../src/export.ts';
import { IssueSink, type Issue } from '../src/report.ts';
import { encodePng, encodeRgb16 } from './support/png.ts';
import { copyExportRef, readExportJson, writeExportJson } from './support/root.ts';
import { cleanupTmp } from './support/tmp.ts';

afterAll(cleanupTmp);

// 007 RF-B1 and plan-b decision 12: one case per export.* code, with the exact list of lines.
function read(dir: string): { ok: boolean; issues: readonly Issue[] } {
  const sink = new IssueSink();
  const result = readExport(sink, dir);
  return { ok: result !== null, issues: sink.issues };
}

const CELL = 192;
const blank = () => new Uint8Array(CELL * CELL * 4);
const png = (dir: string, file: string) => path.join(dir, 'frames', ...file.split('/'));

type Mutate = (dir: string) => void;
const json =
  (fn: (j: Record<string, unknown>) => void): Mutate =>
  (dir) => {
    const j = readExportJson(dir);
    fn(j);
    writeExportJson(dir, j);
  };
const anims = (j: Record<string, unknown>) =>
  j['animations'] as Record<string, Record<string, unknown>>;

const CASES: ReadonlyArray<readonly [string, Mutate, readonly Issue[]]> = [
  [
    'unexpected_entry at the root',
    (d) => writeFileSync(path.join(d, 'notes.txt'), ''),
    [{ code: 'export.unexpected_entry', file: 'notes.txt', path: '' }],
  ],
  [
    'unexpected_entry in frames/',
    (d) => mkdirSync(path.join(d, 'frames', 'taunt')),
    [{ code: 'export.unexpected_entry', file: 'frames/taunt', path: '' }],
  ],
  [
    'unexpected_entry: a PNG with another name',
    (d) => writeFileSync(png(d, 'idle/0.png'), ''),
    [{ code: 'export.unexpected_entry', file: 'frames/idle/0.png', path: '' }],
  ],
  [
    'symlink: a file',
    (d) => {
      unlinkSync(png(d, 'win/01.png'));
      symlinkSync('00.png', png(d, 'win/01.png'));
    },
    [{ code: 'export.symlink', file: 'frames/win/01.png', path: '' }],
  ],
  [
    'symlink: a folder',
    (d) => {
      renameSync(path.join(d, 'frames', 'win'), path.join(d, 'win-real'));
      rmSync(path.join(d, 'win-real'), { recursive: true });
      symlinkSync(path.join(d, 'frames', 'idle'), path.join(d, 'frames', 'win'));
    },
    [{ code: 'export.symlink', file: 'frames/win', path: '' }],
  ],
  [
    'missing_file',
    (d) => unlinkSync(path.join(d, 'export.json')),
    [{ code: 'export.missing_file', file: 'export.json', path: '' }],
  ],
  [
    'file_too_large: export.json',
    (d) => truncateSync(path.join(d, 'export.json'), 1024 * 1024 + 1),
    [{ code: 'export.file_too_large', file: 'export.json', path: '' }],
  ],
  [
    'invalid_json',
    (d) => writeFileSync(path.join(d, 'export.json'), '{'),
    [{ code: 'export.invalid_json', file: 'export.json', path: '' }],
  ],
  [
    'unknown_field and missing_field at the root',
    json((j) => {
      j['scale'] = 2;
      delete j['pixel_art'];
    }),
    [
      { code: 'export.unknown_field', file: 'export.json', path: '/scale' },
      { code: 'export.missing_field', file: 'export.json', path: '/pixel_art' },
    ],
  ],
  [
    'unknown_field and missing_field in an animation',
    json((j) => {
      anims(j)['win'] = { frame_ms: [200, 200, 200, 200], pivot: 1 };
    }),
    [
      { code: 'export.unknown_field', file: 'export.json', path: '/animations/win/pivot' },
      { code: 'export.missing_field', file: 'export.json', path: '/animations/win/loop' },
    ],
  ],
  [
    'unsupported_format',
    json((j) => {
      j['format'] = 'torneo.sprites';
      j['format_version'] = 2;
    }),
    [
      { code: 'export.unsupported_format', file: 'export.json', path: '/format' },
      { code: 'export.unsupported_format', file: 'export.json', path: '/format_version' },
    ],
  ],
  [
    'invalid_value: id, pixel_art and frame_ms',
    json((j) => {
      j['id'] = 'Ref-A';
      j['pixel_art'] = 'yes';
      anims(j)['win'] = { frame_ms: 200, loop: true };
      anims(j)['walk'] = { frame_ms: [], loop: true };
    }),
    [
      { code: 'export.invalid_value', file: 'export.json', path: '/id' },
      { code: 'export.invalid_value', file: 'export.json', path: '/pixel_art' },
      { code: 'export.invalid_value', file: 'export.json', path: '/animations/win/frame_ms' },
      { code: 'export.invalid_value', file: 'export.json', path: '/animations/walk/frame_ms' },
    ],
  ],
  [
    'invalid_value: a root that is not an object',
    (d) => writeFileSync(path.join(d, 'export.json'), '[]'),
    [{ code: 'export.invalid_value', file: 'export.json', path: '' }],
  ],
  [
    'unknown_animation',
    json((j) => {
      anims(j)['taunt'] = { frame_ms: [100], loop: false };
    }),
    [{ code: 'export.unknown_animation', file: 'export.json', path: '/animations/taunt' }],
  ],
  [
    'frame_missing: a folder',
    (d) => rmSync(path.join(d, 'frames', 'win'), { recursive: true }),
    [{ code: 'export.frame_missing', file: 'frames/win', path: '/animations/win/frames' }],
  ],
  [
    'frame_missing and frame_extra: a gap in the numbering',
    (d) => renameSync(png(d, 'win/02.png'), png(d, 'win/04.png')),
    [
      { code: 'export.frame_extra', file: 'frames/win/04.png', path: '/animations/win/frames/4' },
      { code: 'export.frame_missing', file: 'frames/win/02.png', path: '/animations/win/frames/2' },
    ],
  ],
  [
    'file_too_large: a PNG',
    (d) => truncateSync(png(d, 'win/01.png'), 1024 * 1024 + 1),
    [
      {
        code: 'export.file_too_large',
        file: 'frames/win/01.png',
        path: '/animations/win/frames/1',
      },
    ],
  ],
  [
    'png_invalid: the signature',
    (d) => writeFileSync(png(d, 'win/01.png'), 'not a png at all, but long enough to be read'),
    [{ code: 'export.png_invalid', file: 'frames/win/01.png', path: '/animations/win/frames/1' }],
  ],
  [
    'png_invalid: a truncated body',
    (d) => {
      const bytes = encodePng(CELL, CELL, blank());
      writeFileSync(png(d, 'win/01.png'), bytes.subarray(0, 40));
    },
    [{ code: 'export.png_invalid', file: 'frames/win/01.png', path: '/animations/win/frames/1' }],
  ],
  [
    'png_size',
    (d) => writeFileSync(png(d, 'win/01.png'), encodePng(64, 96, new Uint8Array(64 * 96 * 4))),
    [
      {
        code: 'export.png_size',
        file: 'frames/win/01.png',
        path: '/animations/win/frames/1',
        data: { w: 64, h: 96 },
      },
    ],
  ],
];

describe('readExport', () => {
  it('reads the reference export', () => {
    const sink = new IssueSink();
    const result = readExport(sink, copyExportRef());
    expect(sink.issues).toEqual([]);
    expect(result?.id).toBe('ref_a');
    expect(result?.frames.map((f) => f.file).slice(0, 5)).toEqual([
      'frames/idle/00.png',
      'frames/idle/01.png',
      'frames/idle/02.png',
      'frames/idle/03.png',
      'frames/attack_light/00.png',
    ]);
  });

  it.each(CASES.map(([name, mutate, issues]) => [name, mutate, issues] as const))(
    '%s',
    (_name, mutate, issues) => {
      const dir = copyExportRef();
      mutate(dir);
      expect(read(dir)).toEqual({ ok: false, issues });
    },
  );

  it('covers every export code that readExport emits', () => {
    const covered = new Set(CASES.flatMap(([, , issues]) => issues.map((i) => i.code)));
    expect([...covered].sort()).toEqual([
      'export.file_too_large',
      'export.frame_extra',
      'export.frame_missing',
      'export.invalid_json',
      'export.invalid_value',
      'export.missing_field',
      'export.missing_file',
      'export.png_invalid',
      'export.png_size',
      'export.symlink',
      'export.unexpected_entry',
      'export.unknown_animation',
      'export.unknown_field',
      'export.unsupported_format',
    ]);
  });

  it('gray with alpha, 16-bit RGB and RGBA give the same RGBA pixels', () => {
    // An opaque gray cell: the same in every color type (RGB has no alpha channel).
    const rgba = blank();
    for (let i = 0; i < rgba.length; i += 4) rgba.set([90, 90, 90, 255], i);
    const decode = (bytes: Buffer): Buffer => {
      const dir = copyExportRef();
      writeFileSync(png(dir, 'win/01.png'), bytes);
      const sink = new IssueSink();
      const result = readExport(sink, dir);
      expect(sink.issues).toEqual([]);
      return Buffer.from(result?.frames.find((f) => f.file === 'frames/win/01.png')?.rgba ?? []);
    };
    // Compared as booleans: a diff of 147 456 bytes would take minutes to print.
    const same = (a: Buffer) => a.equals(Buffer.from(rgba));
    expect(same(decode(encodePng(CELL, CELL, rgba)))).toBe(true);
    expect(same(decode(encodePng(CELL, CELL, rgba, { colorType: 4 })))).toBe(true);
    expect(same(decode(encodeRgb16(CELL, CELL, rgba)))).toBe(true);
  });

  it('passes frame_ms values through: an invalid one is the manifest’s business', () => {
    const dir = copyExportRef();
    json((j) => {
      (anims(j)['win'] as { frame_ms: number[] }).frame_ms[1] = 5;
    })(dir);
    expect(read(dir)).toEqual({ ok: true, issues: [] });
  });
});
