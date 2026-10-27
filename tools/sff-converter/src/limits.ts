// Limits on untrusted input (002 RF-B4, plan-b decision 4).
const MIB = 1024 * 1024;

export const INPUT_LIMITS = Object.freeze({
  sffBytes: 64 * MIB,
  airBytes: 1 * MIB,
  mapBytes: 64 * 1024,
  sprites: 10_000,
  palettes: 10_000,
  spriteSideMax: 1024,
  actions: 10_000,
  framesPerAction: 1_000,
  airLineChars: 4_096,
  /** Output: sheets of at most 2048 px per side, at most 4 of them (format v1). */
  sheetSide: 2048,
  sheets: 4,
  /** Area of every mapped sprite together, checked before decoding (decision 10). */
  mappedAreaMax: 4 * 2048 * 2048,
});
