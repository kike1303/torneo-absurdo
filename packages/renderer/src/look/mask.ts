// Which body skin a garment covers (022): in the rest pose, a point is inside the garment when the
// garment is hit in all four horizontal directions from it, so its open ends (waist, cuffs,
// armholes) do not count as cover. Triangles are binned by height: a horizontal ray only meets the
// ones that span its y.

const SLABS = 64;
/** Skin this close (m) to an opening stays, so nothing looks hollow from outside. */
export const MASK_MARGIN = 0.03;

export interface Garment {
  /** Whether the point is covered, with the margin above and below. */
  covers(x: number, y: number, z: number): boolean;
}

/** `triangles`: nine numbers each (three corners), in the garment's own space. */
export function createGarment(triangles: Float32Array): Garment {
  let low = Infinity;
  let high = -Infinity;
  for (let i = 1; i < triangles.length; i += 3) {
    const y = triangles[i] ?? 0;
    if (y < low) low = y;
    if (y > high) high = y;
  }
  const span = high - low || 1;
  const slabOf = (y: number): number =>
    Math.max(0, Math.min(SLABS - 1, Math.floor(((y - low) / span) * SLABS)));
  const slabs: number[][] = Array.from({ length: SLABS }, () => []);
  for (let i = 0; i + 8 < triangles.length; i += 9) {
    const ay = triangles[i + 1] ?? 0;
    const by = triangles[i + 4] ?? 0;
    const cy = triangles[i + 7] ?? 0;
    const last = slabOf(Math.max(ay, by, cy));
    for (let s = slabOf(Math.min(ay, by, cy)); s <= last; s++) slabs[s]?.push(i);
  }
  const t = triangles;
  const at = (i: number): number => t[i] ?? 0;

  const inside = (x: number, y: number, z: number): boolean => {
    if (y < low || y > high) return false;
    let bits = 0;
    for (const i of slabs[slabOf(y)] ?? []) {
      const ax = at(i);
      const ay = at(i + 1);
      const az = at(i + 2);
      const bx = at(i + 3);
      const by = at(i + 4);
      const bz = at(i + 5);
      const cx = at(i + 6);
      const cy = at(i + 7);
      const cz = at(i + 8);
      if ((y < ay && y < by && y < cy) || (y > ay && y > by && y > cy)) continue;
      // A ray along x: the triangle seen in the y-z plane.
      if (!((z < az && z < bz && z < cz) || (z > az && z > bz && z > cz))) {
        const d = (by - ay) * (cz - az) - (cy - ay) * (bz - az);
        const u = ((y - ay) * (cz - az) - (cy - ay) * (z - az)) / d;
        const v = ((by - ay) * (z - az) - (y - ay) * (bz - az)) / d;
        if (u >= 0 && v >= 0 && u + v <= 1) bits |= ax + u * (bx - ax) + v * (cx - ax) > x ? 1 : 2;
      }
      // A ray along z: the triangle seen in the x-y plane.
      if (!((x < ax && x < bx && x < cx) || (x > ax && x > bx && x > cx))) {
        const d = (bx - ax) * (cy - ay) - (cx - ax) * (by - ay);
        const u = ((x - ax) * (cy - ay) - (cx - ax) * (y - ay)) / d;
        const v = ((bx - ax) * (y - ay) - (x - ax) * (by - ay)) / d;
        if (u >= 0 && v >= 0 && u + v <= 1) bits |= az + u * (bz - az) + v * (cz - az) > z ? 4 : 8;
      }
      if (bits === 15) return true;
    }
    return false;
  };

  return {
    covers: (x, y, z) =>
      inside(x, y, z) && inside(x, y + MASK_MARGIN, z) && inside(x, y - MASK_MARGIN, z),
  };
}

/** The triangles of `index` that are not fully covered; `index` itself when none is. */
export function visibleTriangles(
  index: ArrayLike<number>,
  covered: Uint8Array,
): Uint32Array | null {
  const keep: number[] = [];
  for (let i = 0; i + 2 < index.length; i += 3) {
    const a = index[i] ?? 0;
    const b = index[i + 1] ?? 0;
    const c = index[i + 2] ?? 0;
    if (!(covered[a] === 1 && covered[b] === 1 && covered[c] === 1)) keep.push(a, b, c);
  }
  return keep.length === index.length ? null : new Uint32Array(keep);
}
