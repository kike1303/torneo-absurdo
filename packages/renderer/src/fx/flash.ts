// Tint flash of the struck sprite (RF-A12), never more than 3 per second (ADR-0009 D6).
// It is a sprite tint, not a full-screen flash, so it stays with reduced motion (RF-A14).

export const FLASH_MS = 120;
export const MAX_FLASHES_PER_SECOND = 3;
const WINDOW_MS = 1000;

/** Alpha of the flash `tau` ms after it starts, or 0 when it is over. */
export function flashAlpha(tau: number): number {
  if (!(tau >= 0 && tau < FLASH_MS)) return 0;
  return 1 - tau / FLASH_MS;
}

/**
 * Greedy admission over candidate start times (sorted): a start is admitted when fewer than
 * MAX_FLASHES_PER_SECOND admitted starts lie in (start − 1 s, start]. Then every 1 s window
 * holds at most MAX_FLASHES_PER_SECOND starts.
 */
export function admitFlashes(starts: readonly number[]): boolean[] {
  const admitted: number[] = [];
  return starts.map((start) => {
    let recent = 0;
    for (let i = admitted.length - 1; i >= 0; i--) {
      const prev = admitted[i] ?? 0;
      if (prev <= start - WINDOW_MS) break;
      recent++;
    }
    if (recent >= MAX_FLASHES_PER_SECOND) return false;
    admitted.push(start);
    return true;
  });
}
