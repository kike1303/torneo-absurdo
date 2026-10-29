import {
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  symlinkSync,
  truncateSync,
  unlinkSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import { afterAll, describe, expect, it } from 'vitest';
import { runCheck } from '../src/check.ts';
import { GOLDEN_FILE } from '../src/golden.ts';
import type { Issue } from '../src/report.ts';
import {
  CHARACTER,
  PROVENANCE,
  SET,
  json,
  validRoot,
  writeCharacter,
} from './support/fixture-root.ts';
import { decodePng, encodeInterlaced, encodePng, encodeRgb16 } from './support/png.ts';
import { cleanupTmp } from './support/tmp.ts';

afterAll(cleanupTmp);

// 007 RF-B3, RF-B4 and RF-B6 over fixture roots: the valid root passes, and every code has a
// case that fails with exactly its lines.
const SET_REL = SET.join('/');
const CHAR_REL = CHARACTER.join('/');
const at = (root: string, ...rel: string[]) => path.join(root, ...SET, ...rel);

type Mutate = (root: string) => void;

const manifestOf = (root: string) =>
  JSON.parse(readFileSync(at(root, 'manifest.json'), 'utf8')) as {
    sheets: { file: string; width: number; height: number }[];
    animations: Record<string, { frame_ms: number[] }>;
  };
const patchManifest =
  (fn: (m: ReturnType<typeof manifestOf>) => void): Mutate =>
  (root) => {
    const m = manifestOf(root);
    fn(m);
    writeFileSync(at(root, 'manifest.json'), json(m));
  };
const provenance =
  (patch: Record<string, unknown>, drop: readonly string[] = []): Mutate =>
  (root) => {
    const p: Record<string, unknown> = { ...PROVENANCE, ...patch };
    for (const key of drop) delete p[key];
    writeFileSync(at(root, 'provenance.json'), json(p));
  };
const sheetSize = (root: string) => {
  const s = manifestOf(root).sheets[0];
  return { width: s?.width ?? 0, height: s?.height ?? 0 };
};
const character =
  (body: Record<string, unknown>): Mutate =>
  (root) =>
    writeCharacter(root, 'hero-one', body);

const sheet = (data?: Record<string, number>): Issue =>
  data === undefined
    ? { code: 'set.sheet_header', file: `${SET_REL}/sheet-0.png`, path: '/sheets/0' }
    : { code: 'set.sheet_header', file: `${SET_REL}/sheet-0.png`, path: '/sheets/0', data };
const prov = (code: string, p: string): Issue => ({
  code: `provenance.${code}`,
  file: `${SET_REL}/provenance.json`,
  path: p,
});
const link = (code: string, p = '/sprites', data?: Record<string, string>): Issue =>
  data === undefined
    ? { code: `link.${code}`, file: `${CHAR_REL}/character.json`, path: p }
    : { code: `link.${code}`, file: `${CHAR_REL}/character.json`, path: p, data };
const entry = (rel: string, code = 'set.unexpected_entry'): Issue => ({
  code,
  file: rel,
  path: '',
});

const CASES: ReadonlyArray<readonly [string, Mutate, readonly Issue[]]> = [
  // set.*
  [
    'set.unexpected_entry: .DS_Store',
    (r) => writeFileSync(at(r, '.DS_Store'), ''),
    [entry(`${SET_REL}/.DS_Store`)],
  ],
  [
    'set.unexpected_entry: a subfolder',
    (r) => mkdirSync(at(r, 'source')),
    [entry(`${SET_REL}/source`)],
  ],
  [
    'set.unexpected_entry: a symlink',
    (r) => symlinkSync('sheet-0.png', at(r, 'sheet-9.png')),
    [entry(`${SET_REL}/sheet-9.png`)],
  ],
  [
    'set.unexpected_entry: a sheet the manifest does not declare',
    (r) => writeFileSync(at(r, 'sheet-1.png'), readFileSync(at(r, 'sheet-0.png'))),
    [entry(`${SET_REL}/sheet-1.png`)],
  ],
  [
    'set.unexpected_entry: a loose file in content/sprites',
    (r) => writeFileSync(path.join(r, 'content', 'sprites', 'notes.txt'), ''),
    [entry('content/sprites/notes.txt')],
  ],
  [
    'set.missing_file: the manifest',
    (r) => unlinkSync(at(r, 'manifest.json')),
    [entry(`${SET_REL}/manifest.json`, 'set.missing_file')],
  ],
  [
    'set.missing_file: the provenance',
    (r) => unlinkSync(at(r, 'provenance.json')),
    [entry(`${SET_REL}/provenance.json`, 'set.missing_file')],
  ],
  [
    'set.missing_file: a sheet',
    (r) => unlinkSync(at(r, 'sheet-0.png')),
    [entry(`${SET_REL}/sheet-0.png`, 'set.missing_file')],
  ],
  [
    'set.file_too_large: the manifest',
    (r) => truncateSync(at(r, 'manifest.json'), 64 * 1024 + 1),
    [entry(`${SET_REL}/manifest.json`, 'set.file_too_large')],
  ],
  [
    'set.file_too_large: a sheet',
    (r) => truncateSync(at(r, 'sheet-0.png'), 4 * 1024 * 1024 + 1),
    [{ code: 'set.file_too_large', file: `${SET_REL}/sheet-0.png`, path: '/sheets/0' }],
  ],
  [
    'set.invalid_json',
    (r) => writeFileSync(at(r, 'manifest.json'), '{'),
    [entry(`${SET_REL}/manifest.json`, 'set.invalid_json')],
  ],
  [
    'set.sheet_header: another size',
    (r) => {
      const { width, height } = sheetSize(r);
      writeFileSync(at(r, 'sheet-0.png'), encodePng(width, 192, new Uint8Array(width * 192 * 4)));
      expect(height).not.toBe(192);
    },
    [sheet({ width: 1920, height: 192, bit_depth: 8, color_type: 6, interlace: 0 })],
  ],
  [
    'set.sheet_header: RGB without alpha',
    (r) => {
      const png = decodePng(readFileSync(at(r, 'sheet-0.png')));
      writeFileSync(
        at(r, 'sheet-0.png'),
        encodePng(png.width, png.height, png.data, { colorType: 2 }),
      );
    },
    [sheet({ width: 1920, height: 384, bit_depth: 8, color_type: 2, interlace: 0 })],
  ],
  [
    'set.sheet_header: 16 bits',
    (r) => {
      const png = decodePng(readFileSync(at(r, 'sheet-0.png')));
      writeFileSync(at(r, 'sheet-0.png'), encodeRgb16(png.width, png.height, png.data));
    },
    [sheet({ width: 1920, height: 384, bit_depth: 16, color_type: 2, interlace: 0 })],
  ],
  [
    'set.sheet_header: Adam7 interlaced (refused before decoding)',
    (r) => {
      const png = decodePng(readFileSync(at(r, 'sheet-0.png')));
      writeFileSync(at(r, 'sheet-0.png'), encodeInterlaced(png.width, png.height, png.data));
    },
    [sheet({ width: 1920, height: 384, bit_depth: 8, color_type: 6, interlace: 1 })],
  ],
  [
    'set.sheet_invalid: not a PNG',
    (r) => writeFileSync(at(r, 'sheet-0.png'), 'definitely not a PNG file, but long enough'),
    [{ code: 'set.sheet_invalid', file: `${SET_REL}/sheet-0.png`, path: '/sheets/0' }],
  ],
  [
    'set.sheet_invalid: a truncated body',
    (r) => writeFileSync(at(r, 'sheet-0.png'), readFileSync(at(r, 'sheet-0.png')).subarray(0, 60)),
    [{ code: 'set.sheet_invalid', file: `${SET_REL}/sheet-0.png`, path: '/sheets/0' }],
  ],
  [
    'manifest.id_mismatch: the folder renamed',
    (r) => renameSync(at(r), path.join(r, 'content', 'sprites', 'ref_b')),
    [
      {
        code: 'manifest.id_mismatch',
        file: 'content/sprites/ref_b/manifest.json',
        path: '/id',
        data: { expected: 'ref_b' },
      },
      {
        code: 'provenance.set_mismatch',
        file: 'content/sprites/ref_b/provenance.json',
        path: '/set',
      },
      link('unknown_set', '/sprites', { set: 'ref_a' }),
    ],
  ],
  // provenance.*
  [
    'provenance.file_too_large',
    (r) => truncateSync(at(r, 'provenance.json'), 64 * 1024 + 1),
    [prov('file_too_large', '')],
  ],
  [
    'provenance.invalid_json',
    (r) => writeFileSync(at(r, 'provenance.json'), '{"format":'),
    [prov('invalid_json', '')],
  ],
  ['provenance.unknown_field', provenance({ email: 'x' }), [prov('unknown_field', '/email')]],
  ['provenance.missing_field', provenance({}, ['date']), [prov('missing_field', '/date')]],
  ['provenance.set_mismatch', provenance({ set: 'ref_b' }), [prov('set_mismatch', '/set')]],
  [
    'provenance.invalid_value: /format',
    provenance({ format: 'x' }),
    [prov('invalid_value', '/format')],
  ],
  [
    'provenance.invalid_value: /format_version',
    provenance({ format_version: 2 }),
    [prov('invalid_value', '/format_version')],
  ],
  ...['ana@mail.com', 'ana.perez', 'ana perez', 'a', 'a'.repeat(33), 'Ana', '-ana', 'ana--b'].map(
    (author) =>
      [
        `provenance.invalid_value: /author ${JSON.stringify(author.slice(0, 12))}`,
        provenance({ author }),
        [prov('invalid_value', '/author')],
      ] as const,
  ),
  [
    'provenance.invalid_value: /origin',
    provenance({ origin: 'bought' }),
    [prov('invalid_value', '/origin')],
  ],
  ...['2026-02-30', '2026-13-01', '26-01-01', '2026-00-10', '2025-02-29'].map(
    (date) =>
      [
        `provenance.invalid_value: /date ${date}`,
        provenance({ date }),
        [prov('invalid_value', '/date')],
      ] as const,
  ),
  [
    'provenance.invalid_value: /ai_assisted',
    provenance({ ai_assisted: 'no' }),
    [prov('invalid_value', '/ai_assisted')],
  ],
  [
    'provenance.invalid_value: /license',
    provenance({ license: 'cc-by' }),
    [prov('invalid_value', '/license')],
  ],
  [
    'provenance.invalid_value: a root that is not an object',
    (r) => writeFileSync(at(r, 'provenance.json'), '[]'),
    [prov('invalid_value', '')],
  ],
  // link.*
  ['link.invalid: null', character({ sprites: null }), [link('invalid')]],
  ['link.invalid: a number', character({ sprites: 3 }), [link('invalid')]],
  ['link.invalid: Bad-Id', character({ sprites: 'Bad-Id' }), [link('invalid')]],
  [
    'link.unknown_set',
    character({ sprites: 'ref_b' }),
    [link('unknown_set', '/sprites', { set: 'ref_b' })],
  ],
  [
    'link.unknown_set: a symlink to the set',
    (r) => {
      symlinkSync(at(r), path.join(r, 'content', 'sprites', 'ref_c'));
      writeCharacter(r, 'hero-one', { sprites: 'ref_c' });
    },
    [entry('content/sprites/ref_c'), link('unknown_set', '/sprites', { set: 'ref_c' })],
  ],
  [
    'link.misplaced_sprites',
    (r) => mkdirSync(path.join(r, ...CHARACTER, 'sprites')),
    [{ code: 'link.misplaced_sprites', file: `${CHAR_REL}/sprites`, path: '' }],
  ],
  [
    'link.character_unreadable: broken JSON',
    (r) => writeFileSync(path.join(r, ...CHARACTER, 'character.json'), '{'),
    [link('character_unreadable', '')],
  ],
  [
    'link.character_unreadable: a symlink',
    (r) => {
      const file = path.join(r, ...CHARACTER, 'character.json');
      renameSync(file, path.join(r, 'real.json'));
      symlinkSync(path.join(r, 'real.json'), file);
    },
    [link('character_unreadable', '')],
  ],
  // guide.*, one per family
  [
    'guide (timing): windup_short',
    patchManifest((m) => {
      if (m.animations['attack_light']) m.animations['attack_light'].frame_ms = [40, 40, 90, 120];
    }),
    [
      {
        code: 'guide.windup_short',
        file: `${SET_REL}/manifest.json`,
        path: '/animations/attack_light/impact_frame',
        data: { windup_ms: 80, min: 100 },
      },
    ],
  ],
  [
    'guide (pixels): frame_clipped',
    (r) => {
      const png = decodePng(readFileSync(at(r, 'sheet-0.png')));
      png.data.set([40, 24, 16, 255], (100 * png.width + 191) * 4);
      writeFileSync(at(r, 'sheet-0.png'), encodePng(png.width, png.height, png.data));
    },
    [{ code: 'guide.frame_clipped', file: `${SET_REL}/manifest.json`, path: '/frames/0' }],
  ],
  [
    'guide (order): windup_order',
    patchManifest((m) => {
      if (m.animations['attack_heavy'])
        m.animations['attack_heavy'].frame_ms = [100, 100, 160, 150, 220];
    }),
    [
      {
        code: 'guide.windup_order',
        file: `${SET_REL}/manifest.json`,
        path: '/animations/special/impact_frame',
        data: { light_ms: 120, heavy_ms: 360, special_ms: 320, min_step: 100 },
      },
    ],
  ],
];

describe('runCheck', () => {
  it('passes the valid root', () => {
    expect(runCheck({ root: validRoot() })).toEqual({
      status: 0,
      issues: [],
      summary: { sets: 1, characters: 1, links: 1 },
    });
  });

  it('passes without content/sprites and with only its README', () => {
    const none = validRoot();
    rmSync(path.join(none, 'content', 'sprites'), { recursive: true });
    writeCharacter(none, 'hero-one', {});
    expect(runCheck({ root: none }).summary).toEqual({ sets: 0, characters: 1, links: 0 });
    const readme = validRoot();
    rmSync(at(readme), { recursive: true });
    writeCharacter(readme, 'hero-one', {});
    expect(runCheck({ root: readme }).summary).toEqual({ sets: 0, characters: 1, links: 0 });
  });

  it('passes a set shared by two characters and a set without a character', () => {
    const shared = validRoot();
    writeCharacter(shared, 'hero-two', { sprites: 'ref_a' });
    expect(runCheck({ root: shared }).summary).toEqual({ sets: 1, characters: 2, links: 2 });
    const alone = validRoot();
    writeCharacter(alone, 'hero-one', {});
    expect(runCheck({ root: alone }).summary).toEqual({ sets: 1, characters: 1, links: 0 });
  });

  it.each(CASES.map(([name, mutate, issues]) => [name, mutate, issues] as const))(
    '%s',
    (_name, mutate, issues) => {
      const root = validRoot();
      mutate(root);
      expect(runCheck({ root })).toEqual({ status: 1, issues });
    },
  );

  it('a new ruleset version with a lower windup breaks a set that was valid', () => {
    const root = validRoot();
    const file = path.join(root, ...GOLDEN_FILE.split('/'));
    const golden = JSON.parse(readFileSync(file, 'utf8')) as { versions: Record<string, number>[] };
    const v1 = golden.versions[0] ?? {};
    golden.versions.push({ ...v1, ruleset_version: 2, windup_heavy_ms: 250 });
    writeFileSync(file, json(golden));
    expect(runCheck({ root })).toEqual({
      status: 1,
      issues: [
        {
          code: 'guide.windup_long',
          file: `${SET_REL}/manifest.json`,
          path: '/animations/attack_heavy/impact_frame',
          data: { windup_ms: 300, cap_ms: 250, ruleset_version: 2 },
        },
      ],
    });
  });

  it('a missing golden is io.golden_invalid with 2', () => {
    const root = validRoot();
    rmSync(path.join(root, 'crates'), { recursive: true });
    expect(runCheck({ root })).toEqual({
      status: 2,
      issues: [{ code: 'io.golden_invalid', file: '', path: '' }],
    });
  });

  it('covers every set, provenance and link code', () => {
    const covered = new Set(CASES.flatMap(([, , issues]) => issues.map((i) => i.code)));
    const families = [...covered].filter((c) => /^(set|provenance|link)\./.test(c)).sort();
    expect(families).toEqual([
      'link.character_unreadable',
      'link.invalid',
      'link.misplaced_sprites',
      'link.unknown_set',
      'provenance.file_too_large',
      'provenance.invalid_json',
      'provenance.invalid_value',
      'provenance.missing_field',
      'provenance.set_mismatch',
      'provenance.unknown_field',
      'set.file_too_large',
      'set.invalid_json',
      'set.missing_file',
      'set.sheet_header',
      'set.sheet_invalid',
      'set.unexpected_entry',
    ]);
  });
});
