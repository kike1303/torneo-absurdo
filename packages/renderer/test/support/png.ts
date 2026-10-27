// Minimal PNG header reader (RF-A6): signature and IHDR, without pngjs.
const SIGNATURE = [137, 80, 78, 71, 13, 10, 26, 10];

export interface PngHeader {
  readonly width: number;
  readonly height: number;
  readonly bitDepth: number;
  readonly colorType: number;
}

export function readIhdr(bytes: Uint8Array): PngHeader {
  if (bytes.length < 33 || SIGNATURE.some((b, i) => bytes[i] !== b)) {
    throw new Error('png.bad_signature');
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const type = String.fromCharCode(...bytes.subarray(12, 16));
  if (view.getUint32(8) !== 13 || type !== 'IHDR') throw new Error('png.missing_ihdr');
  return {
    width: view.getUint32(16),
    height: view.getUint32(20),
    bitDepth: bytes[24] ?? 0,
    colorType: bytes[25] ?? 0,
  };
}
