// inspect (002 RF-B2, plan-b decision 11): headers, nodes and the AIR only; no pixel decoded.
import { parseAir } from './air.ts';
import type { CharacterMap } from './map.ts';
import { readSff, spriteIndexer, spriteInfo } from './sff.ts';

export interface InspectInput {
  readonly sff: Uint8Array;
  readonly air?: Uint8Array | string;
  readonly map?: CharacterMap;
}

export interface InspectReport {
  readonly sff_version: string;
  readonly sprites: number;
  /** SFF v2: the number of palettes; SFF v1: the palette mode. */
  readonly palettes: number | 'shared' | 'individual';
  /** Sprites by compression; `<name>_unsupported` for the ones the converter cannot decode. */
  readonly compression: Readonly<Record<string, number>>;
  readonly actions?: number;
  readonly missing_actions?: readonly { readonly animation: string; readonly action: number }[];
  readonly missing_sprites?: number;
  readonly unsupported_sprites?: readonly {
    readonly animation: string;
    readonly sprite: number;
    readonly compression: string;
  }[];
}

export function inspect(input: InspectInput): InspectReport {
  const sff = readSff(input.sff);
  const compression: Record<string, number> = {};
  for (const sprite of sff.sprites) {
    let key = 'linked';
    if (sprite.linked === null) {
      const info = spriteInfo(sff, sprite.index);
      key = info.supported ? info.compression : `${info.compression}_unsupported`;
    }
    compression[key] = (compression[key] ?? 0) + 1;
  }
  const report: InspectReport = {
    sff_version: sff.versionLabel,
    sprites: sff.sprites.length,
    palettes: sff.version === 2 ? sff.palettes.length : sff.sharedPalette ? 'shared' : 'individual',
    compression,
  };
  if (input.air === undefined) return report;
  const air = parseAir(input.air);
  if (input.map === undefined) return { ...report, actions: air.actions.size };

  const find = spriteIndexer(sff);
  const missing: { animation: string; action: number }[] = [];
  const unsupported: { animation: string; sprite: number; compression: string }[] = [];
  let missingSprites = 0;
  for (const [animation, chain] of Object.entries(input.map.actions)) {
    const seen = new Set<number>();
    for (const action of chain ?? []) {
      const found = air.actions.get(action);
      if (found === undefined) {
        missing.push({ animation, action });
        continue;
      }
      for (const frame of found.frames) {
        if (frame.group === -1) continue;
        const index = find(frame.group, frame.image);
        if (index === -1) {
          missingSprites++;
          continue;
        }
        if (seen.has(index)) continue;
        seen.add(index);
        const info = spriteInfo(sff, index);
        if (!info.supported)
          unsupported.push({ animation, sprite: index, compression: info.compression });
      }
    }
  }
  return {
    ...report,
    actions: air.actions.size,
    missing_actions: missing,
    missing_sprites: missingSprites,
    unsupported_sprites: unsupported,
  };
}
