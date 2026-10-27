// Little-endian byte writer for the synthetic fixtures (002 RF-B6).
export class ByteWriter {
  #bytes: number[] = [];

  get length(): number {
    return this.#bytes.length;
  }

  u8(v: number): this {
    this.#bytes.push(v & 0xff);
    return this;
  }

  u16(v: number): this {
    return this.u8(v).u8(v >> 8);
  }

  i16(v: number): this {
    return this.u16(v < 0 ? v + 0x10000 : v);
  }

  u32(v: number): this {
    return this.u8(v)
      .u8(v >>> 8)
      .u8(v >>> 16)
      .u8(v >>> 24);
  }

  u32be(v: number): this {
    return this.u8(v >>> 24)
      .u8(v >>> 16)
      .u8(v >>> 8)
      .u8(v);
  }

  bytes(data: ArrayLike<number>): this {
    for (let i = 0; i < data.length; i++) this.#bytes.push((data[i] ?? 0) & 0xff);
    return this;
  }

  zeros(n: number): this {
    for (let i = 0; i < n; i++) this.#bytes.push(0);
    return this;
  }

  ascii(text: string): this {
    for (let i = 0; i < text.length; i++) this.#bytes.push(text.charCodeAt(i) & 0xff);
    return this;
  }

  /** Overwrites a u32 already written (offsets known only later). */
  patchU32(at: number, v: number): this {
    this.#bytes[at] = v & 0xff;
    this.#bytes[at + 1] = (v >>> 8) & 0xff;
    this.#bytes[at + 2] = (v >>> 16) & 0xff;
    this.#bytes[at + 3] = (v >>> 24) & 0xff;
    return this;
  }

  done(): Uint8Array {
    return Uint8Array.from(this.#bytes);
  }
}

export function concat(...parts: readonly Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}
