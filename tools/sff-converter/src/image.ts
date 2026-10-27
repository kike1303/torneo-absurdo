// Decoded sprite: RGBA, not premultiplied (the format of the sheets).
export interface RgbaImage {
  readonly width: number;
  readonly height: number;
  readonly rgba: Uint8Array;
}
