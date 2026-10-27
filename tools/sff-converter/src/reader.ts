// Bounded byte reader (002 RF-B4, plan-b decision 4). Every read checks its range first and
// fails with `sff.out_of_range`; a slice never sees bytes outside its window.
import { fail } from './errors.ts';

function checkRange(offset: number, n: number, length: number): void {
  if (
    !Number.isSafeInteger(offset) ||
    !Number.isSafeInteger(n) ||
    offset < 0 ||
    n < 0 ||
    offset + n > length
  ) {
    fail('sff.out_of_range', {
      offset: Number.isFinite(offset) ? offset : -1,
      length: Number.isFinite(n) ? n : -1,
    });
  }
}

export class ByteReader {
  readonly bytes: Uint8Array;
  #pos = 0;

  constructor(bytes: Uint8Array) {
    this.bytes = bytes;
  }

  get length(): number {
    return this.bytes.length;
  }

  get offset(): number {
    return this.#pos;
  }

  get remaining(): number {
    return this.bytes.length - this.#pos;
  }

  /** Moves to `offset` (the end is a valid position). */
  at(offset: number): this {
    checkRange(offset, 0, this.bytes.length);
    this.#pos = offset;
    return this;
  }

  skip(n: number): this {
    return this.at(this.#pos + n);
  }

  #take(n: number): number {
    checkRange(this.#pos, n, this.bytes.length);
    const start = this.#pos;
    this.#pos += n;
    return start;
  }

  u8(): number {
    return this.bytes[this.#take(1)] ?? 0;
  }

  u16(): number {
    const i = this.#take(2);
    return (this.bytes[i] ?? 0) | ((this.bytes[i + 1] ?? 0) << 8);
  }

  i16(): number {
    const v = this.u16();
    return v >= 0x8000 ? v - 0x10000 : v;
  }

  u32(): number {
    const i = this.#take(4);
    const b = this.bytes;
    return (
      ((b[i] ?? 0) | ((b[i + 1] ?? 0) << 8) | ((b[i + 2] ?? 0) << 16)) + (b[i + 3] ?? 0) * 0x1000000
    );
  }

  /** Big endian, for PNG. */
  u32be(): number {
    const i = this.#take(4);
    const b = this.bytes;
    return (
      (b[i] ?? 0) * 0x1000000 + (((b[i + 1] ?? 0) << 16) | ((b[i + 2] ?? 0) << 8) | (b[i + 3] ?? 0))
    );
  }

  /** A view (no copy) of the next `n` bytes. */
  bytesView(n: number): Uint8Array {
    const start = this.#take(n);
    return this.bytes.subarray(start, start + n);
  }

  /** A reader over `[offset, offset + length)` of this one. */
  slice(offset: number, length: number): ByteReader {
    checkRange(offset, length, this.bytes.length);
    return new ByteReader(this.bytes.subarray(offset, offset + length));
  }
}
