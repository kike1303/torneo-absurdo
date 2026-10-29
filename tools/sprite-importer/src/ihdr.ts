// The IHDR of a PNG, read before decoding anything (007 RF-B2, plan-b decision 11): the PNG
// signature and a first chunk `IHDR` of length 13. No decompression, no CRC (the decoder checks
// it later). `null` when unreadable; the caller picks the code.
const SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

export interface Ihdr {
  readonly width: number;
  readonly height: number;
  readonly bitDepth: number;
  readonly colorType: number;
  readonly interlace: number;
}

export function readIhdr(bytes: Uint8Array): Ihdr | null {
  if (bytes.length < 8 + 8 + 13) return null;
  for (let i = 0; i < SIGNATURE.length; i++) if (bytes[i] !== SIGNATURE[i]) return null;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (view.getUint32(8) !== 13) return null;
  const type = String.fromCharCode(bytes[12] ?? 0, bytes[13] ?? 0, bytes[14] ?? 0, bytes[15] ?? 0);
  if (type !== 'IHDR') return null;
  return {
    width: view.getUint32(16),
    height: view.getUint32(20),
    bitDepth: view.getUint8(24),
    colorType: view.getUint8(25),
    interlace: view.getUint8(28),
  };
}
