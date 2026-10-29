// PNG helpers for the tests: encode RGBA pixels, in any color type the decoder accepts.
import { crc32, deflateSync } from 'node:zlib';
import { PNG, type PNGOptions } from 'pngjs';

export function encodePng(
  width: number,
  height: number,
  rgba: Uint8Array,
  options: Pick<PNGOptions, 'colorType' | 'bitDepth' | 'inputHasAlpha'> = { colorType: 6 },
): Buffer {
  const png = new PNG({ width, height });
  png.data.set(rgba);
  return PNG.sync.write(png, options);
}

export function decodePng(bytes: Uint8Array): { width: number; height: number; data: Uint8Array } {
  const png = PNG.sync.read(Buffer.from(bytes));
  return { width: png.width, height: png.height, data: new Uint8Array(png.data) };
}

function chunk(type: string, data: Buffer): Buffer {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'latin1'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([length, body, crc]);
}

/**
 * A 16-bit RGB PNG built by hand (pngjs does not write 16 bits from 8-bit input correctly): each
 * 8-bit sample `v` becomes `v × 257`, which the decoder scales back to `v`. Alpha is dropped.
 */
export function encodeRgb16(width: number, height: number, rgba: Uint8Array): Buffer {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 16;
  ihdr[9] = 2;
  const raw = Buffer.alloc(height * (1 + width * 6));
  for (let y = 0; y < height; y++) {
    const row = y * (1 + width * 6);
    for (let x = 0; x < width; x++) {
      for (let c = 0; c < 3; c++) {
        raw.writeUInt16BE((rgba[(y * width + x) * 4 + c] ?? 0) * 257, row + 1 + x * 6 + c * 2);
      }
    }
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw)),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}
