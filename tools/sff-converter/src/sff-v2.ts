// SFF v2 (002 RF-B3, plan-b "Alcance de formatos"): a 512-byte header, tables of 28-byte sprite
// nodes and 16-byte palette nodes, and the `ldata` and `tdata` blocks.
import { fail } from './errors.ts';
import { INPUT_LIMITS } from './limits.ts';
import type { ByteReader } from './reader.ts';
import type { PaletteNodeV2, SffV2, SpriteNodeV2 } from './sff-types.ts';

const SPRITE_NODE = 28;
const PALETTE_NODE = 16;

export function readSffV2(r: ByteReader, versionLabel: string): SffV2 {
  r.at(36);
  const spriteOffset = r.u32();
  const spriteCount = r.u32();
  const paletteOffset = r.u32();
  const paletteCount = r.u32();
  const ldataOffset = r.u32();
  const ldataLength = r.u32();
  const tdataOffset = r.u32();
  const tdataLength = r.u32();
  if (spriteCount > INPUT_LIMITS.sprites) fail('sff.too_many_sprites', { count: spriteCount });
  if (paletteCount > INPUT_LIMITS.palettes) {
    fail('sff.too_many_palettes', { count: paletteCount });
  }
  const ldata = r.slice(ldataOffset, ldataLength);
  const tdata = r.slice(tdataOffset, tdataLength);

  const pnodes = r.slice(paletteOffset, paletteCount * PALETTE_NODE);
  const palettes: PaletteNodeV2[] = [];
  for (let index = 0; index < paletteCount; index++) {
    pnodes.u16(); // group
    pnodes.u16(); // item
    const colors = pnodes.u16();
    const linkedIndex = pnodes.u16();
    const offset = pnodes.u32();
    const length = pnodes.u32();
    const linked = length === 0 ? linkedIndex : null;
    if (linked !== null && linked >= paletteCount) {
      fail('sff.link_out_of_range', { palette: index, linked });
    }
    const data = length === 0 ? null : ldata.slice(offset, length);
    palettes.push({ index, colors, linked, data });
  }

  const snodes = r.slice(spriteOffset, spriteCount * SPRITE_NODE);
  const sprites: SpriteNodeV2[] = [];
  for (let index = 0; index < spriteCount; index++) {
    const group = snodes.u16();
    const image = snodes.u16();
    const width = snodes.u16();
    const height = snodes.u16();
    const axisX = snodes.i16();
    const axisY = snodes.i16();
    const linkedIndex = snodes.u16();
    const format = snodes.u8();
    const depth = snodes.u8();
    const offset = snodes.u32();
    const length = snodes.u32();
    const paletteIndex = snodes.u16();
    const flags = snodes.u16();
    const linked = length === 0 ? linkedIndex : null;
    if (linked !== null && linked >= spriteCount) {
      fail('sff.link_out_of_range', { sprite: index, linked });
    }
    const block = (flags & 1) === 1 ? tdata : ldata;
    const data = length === 0 ? null : block.slice(offset, length);
    sprites.push({
      index,
      group,
      image,
      width,
      height,
      axisX,
      axisY,
      linked,
      format,
      depth,
      paletteIndex,
      data,
    });
  }
  return { version: 2, versionLabel, sprites, palettes };
}
