// SFF v1 (002 RF-B3, plan-b "Alcance de formatos"): a 512-byte header and a linked list of
// subfiles, each a 32-byte subheader followed by a PCX.
import { fail } from './errors.ts';
import { INPUT_LIMITS } from './limits.ts';
import type { ByteReader } from './reader.ts';
import type { SffV1, SpriteNodeV1 } from './sff-types.ts';

const SUBHEADER = 32;

export function readSffV1(r: ByteReader, versionLabel: string): SffV1 {
  r.at(16);
  r.u32(); // groups
  const count = r.u32();
  const first = r.u32();
  r.u32(); // subheader size, always 32
  const sharedPalette = r.u8() === 1;
  if (count > INPUT_LIMITS.sprites) fail('sff.too_many_sprites', { count });
  const sprites: SpriteNodeV1[] = [];
  const visited = new Set<number>();
  let offset = first;
  for (let index = 0; index < count; index++) {
    if (visited.has(offset)) fail('sff.offset_cycle', { sprite: index, offset });
    visited.add(offset);
    r.at(offset);
    const next = r.u32();
    const length = r.u32();
    const axisX = r.i16();
    const axisY = r.i16();
    const group = r.u16();
    const image = r.u16();
    const linkedIndex = r.u16();
    const samePalette = r.u8() === 1;
    const linked = length === 0 ? linkedIndex : null;
    if (linked !== null && linked >= count) {
      fail('sff.link_out_of_range', { sprite: index, linked });
    }
    const data = length === 0 ? null : r.slice(offset + SUBHEADER, length);
    sprites.push({ index, group, image, axisX, axisY, linked, data, samePalette });
    offset = next;
  }
  return { version: 1, versionLabel, sharedPalette, sprites };
}
