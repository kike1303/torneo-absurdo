// convert({ sff, air, map }): pure, no disk (002 RF-B5, plan-b decisions 9 and 10).
import {
  ATTACK_ANIMATIONS,
  MANIFEST_FORMAT,
  MANIFEST_FORMAT_VERSION,
  OPTIONAL_ANIMATIONS,
  REQUIRED_ANIMATIONS,
  validateManifest,
  type AnimationCode,
  type ManifestError,
  type SpriteAnimation,
  type SpriteFrame,
  type SpriteManifest,
} from '@torneo/renderer/manifest';
import { parseAir } from './air.ts';
import { fail, type ErrorData, type Note } from './errors.ts';
import { buildFrame, EMPTY_FRAME, type FrameImage } from './frames.ts';
import { INPUT_LIMITS } from './limits.ts';
import { isLooping, timedFrames, type TimedFrame } from './manifest.ts';
import type { CharacterMap } from './map.ts';
import { pack } from './pack.ts';
import {
  decodeSprite,
  readSff,
  resolveSprite,
  spriteIndexer,
  spriteInfo,
  unsupportedCompression,
} from './sff.ts';

export interface ConvertInput {
  readonly sff: Uint8Array;
  readonly air: Uint8Array | string;
  readonly map: CharacterMap;
}

export interface SheetImage {
  readonly file: string;
  readonly width: number;
  readonly height: number;
  readonly rgba: Uint8Array;
}

export interface NoteSummary {
  readonly code: string;
  readonly count: number;
  /** Up to three examples of its data. */
  readonly data: readonly ErrorData[];
}

/** What the owner pastes in the PR (RF-B8): no paths, no file names of the input. */
export interface ConvertReport {
  readonly id: string;
  readonly sff_version: string;
  readonly sprites: { readonly total: number; readonly decoded: number };
  readonly frames: number;
  readonly sheets: readonly {
    readonly file: string;
    readonly width: number;
    readonly height: number;
  }[];
  readonly animations: Readonly<Partial<Record<AnimationCode, number>>>;
  readonly notes: readonly NoteSummary[];
  readonly manifest_errors?: readonly ManifestError[];
}

export type ConvertResult =
  | {
      readonly ok: true;
      readonly manifest: SpriteManifest;
      readonly sheets: readonly SheetImage[];
      readonly report: ConvertReport;
    }
  | {
      readonly ok: false;
      readonly errors: readonly ManifestError[];
      readonly report: ConvertReport;
    };

const ORDER: readonly AnimationCode[] = [...REQUIRED_ANIMATIONS, ...OPTIONAL_ANIMATIONS];
const OPTIONAL: readonly AnimationCode[] = OPTIONAL_ANIMATIONS;
const ATTACKS: readonly AnimationCode[] = ATTACK_ANIMATIONS;

export function summarize(notes: readonly Note[]): NoteSummary[] {
  const byCode = new Map<string, { count: number; data: ErrorData[] }>();
  for (const note of notes) {
    const entry = byCode.get(note.code) ?? { count: 0, data: [] };
    entry.count++;
    if (note.data !== undefined && entry.data.length < 3) entry.data.push(note.data);
    byCode.set(note.code, entry);
  }
  return [...byCode]
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([code, e]) => ({ code, count: e.count, data: e.data }));
}

export function convert(input: ConvertInput): ConvertResult {
  const { map } = input;
  const sff = readSff(input.sff);
  const air = parseAir(input.air);
  const notes: Note[] = [...air.notes];
  const find = spriteIndexer(sff);

  // 1. Animations: which AIR frames each one plays.
  const timed = new Map<AnimationCode, TimedFrame[]>();
  for (const code of ORDER) {
    const actionId = map.actions[code];
    if (actionId === undefined) continue;
    const action = air.actions.get(actionId);
    if (action === undefined) {
      if (OPTIONAL.includes(code)) {
        notes.push({
          code: 'convert.optional_missing',
          data: { animation: code, action: actionId },
        });
        continue;
      }
      fail('air.missing_action', { animation: code, action: actionId });
    }
    if (action.badLines > 0) fail('air.bad_frame', { animation: code, action: actionId });
    if (action.frames.length === 0) fail('air.empty_action', { animation: code, action: actionId });
    timed.set(code, timedFrames(code, action, notes));
  }

  // 2. The sprites they use: supported, and bounded in memory before any decoding.
  const used = new Map<string, number>();
  for (const frames of timed.values()) {
    for (const { frame } of frames) {
      if (frame.group === -1) continue;
      const key = `${frame.group},${frame.image}`;
      if (used.has(key)) continue;
      const index = find(frame.group, frame.image);
      if (index === -1)
        notes.push({
          code: 'convert.sprite_not_found',
          data: { group: frame.group, image: frame.image },
        });
      used.set(key, index);
    }
  }
  let area = 0;
  for (const index of used.values()) {
    if (index === -1) continue;
    const info = spriteInfo(sff, index);
    if (!info.supported) {
      fail(info.compression === 'pcx' ? 'pcx.unsupported' : 'sff.unsupported_compression', {
        sprite: index,
        compression: info.compression,
      });
    }
    area += Math.max(0, info.width) * Math.max(0, info.height);
    if (area > INPUT_LIMITS.mappedAreaMax) fail('convert.too_large', { area });
  }
  // Unsupported sprites that no mapped animation uses: a note each, never an abort (RF-B3).
  const mapped = new Set<number>();
  for (const index of used.values()) if (index !== -1) mapped.add(resolveSprite(sff, index));
  for (const sprite of sff.sprites) {
    if (mapped.has(sprite.index)) continue;
    const compression = unsupportedCompression(sff, sprite.index);
    if (compression === null) continue;
    notes.push({
      code: compression === 'pcx' ? 'pcx.unsupported' : 'sff.unsupported_compression',
      data: { sprite: sprite.index, compression },
    });
  }
  const paletteSprite = sff.version === 1 ? find(map.paletteSprite[0], map.paletteSprite[1]) : -1;

  // 3. Frames, deduplicated by (sprite, mirror, pivot).
  const images = new Map<string, FrameImage>();
  const order: string[] = [];
  const indexOf = new Map<string, number>();
  const decoded = new Set<number>();
  const animationFrames = new Map<AnimationCode, number[]>();
  for (const [code, frames] of timed) {
    const indices: number[] = [];
    for (const { frame } of frames) {
      const sprite = frame.group === -1 ? -1 : (used.get(`${frame.group},${frame.image}`) ?? -1);
      const node = sprite === -1 ? undefined : sff.sprites[sprite];
      const key =
        node === undefined
          ? 'empty'
          : `${resolveSprite(sff, sprite)}|${sprite}|${frame.flipH ? 1 : 0}${frame.flipV ? 1 : 0}|${node.axisX - frame.x},${node.axisY - frame.y}`;
      let at = indexOf.get(key);
      if (at === undefined) {
        let image = EMPTY_FRAME;
        if (node !== undefined) {
          const pixels = decodeSprite(sff, sprite, { paletteSprite });
          decoded.add(sprite);
          image = buildFrame(pixels, {
            axisX: node.axisX,
            axisY: node.axisY,
            x: frame.x,
            y: frame.y,
            flipH: frame.flipH,
            flipV: frame.flipV,
          });
        }
        at = order.length;
        order.push(key);
        images.set(key, image);
        indexOf.set(key, at);
      }
      indices.push(at);
    }
    animationFrames.set(code, indices);
  }

  // 4. Sheets.
  const packed = pack(
    order.map((key) => {
      const image = images.get(key) ?? EMPTY_FRAME;
      return { key, width: image.width, height: image.height };
    }),
  );
  const sheets: SheetImage[] = packed.sheets.map((s, i) => ({
    file: `sheet-${i}.png`,
    width: s.width,
    height: s.height,
    rgba: new Uint8Array(s.width * s.height * 4),
  }));
  const frames: SpriteFrame[] = order.map((key) => {
    const image = images.get(key) ?? EMPTY_FRAME;
    const place = packed.placements.get(key) ?? { sheet: 0, x: 0, y: 0 };
    const sheet = sheets[place.sheet];
    if (sheet !== undefined) {
      for (let y = 0; y < image.height; y++) {
        const src = y * image.width * 4;
        sheet.rgba.set(
          image.rgba.subarray(src, src + image.width * 4),
          ((place.y + y) * sheet.width + place.x) * 4,
        );
      }
    }
    return {
      sheet: place.sheet,
      x: place.x,
      y: place.y,
      w: image.width,
      h: image.height,
      pivot_x: image.pivotX,
      pivot_y: image.pivotY,
    };
  });

  // 5. Manifest, validated like any untrusted input.
  const animations: Partial<Record<AnimationCode, SpriteAnimation>> = {};
  for (const [code, list] of timed) {
    const entry: SpriteAnimation = {
      frames: animationFrames.get(code) ?? [],
      frame_ms: list.map((t) => t.ms),
      loop: isLooping(code),
      ...(ATTACKS.includes(code)
        ? { impact_frame: map.impactFrames[code as (typeof ATTACK_ANIMATIONS)[number]] }
        : {}),
    };
    animations[code] = entry;
  }
  const manifest = {
    format: MANIFEST_FORMAT,
    format_version: MANIFEST_FORMAT_VERSION,
    id: map.id,
    pixel_art: true,
    scale: map.scale,
    sheets: sheets.map(({ file, width, height }) => ({ file, width, height })),
    frames,
    animations,
  };
  const report: ConvertReport = {
    id: map.id,
    sff_version: sff.versionLabel,
    sprites: { total: sff.sprites.length, decoded: decoded.size },
    frames: frames.length,
    sheets: manifest.sheets,
    animations: Object.fromEntries([...animationFrames].map(([code, list]) => [code, list.length])),
    notes: summarize(notes),
  };
  const result = validateManifest(manifest, { expectedId: map.id });
  if (!result.ok) {
    return {
      ok: false,
      errors: result.errors,
      report: { ...report, manifest_errors: result.errors },
    };
  }
  return { ok: true, manifest: result.manifest, sheets, report };
}
