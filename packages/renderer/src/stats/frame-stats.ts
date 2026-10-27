// Frame statistics (RF-A19, plan-a2 decision 13). Pure: the intervals come from the Pixi
// ticker's `elapsedMS` (real time, no cap), so the package never reads `performance.now()`.

export interface FrameStats {
  readonly frames: number;
  readonly fpsMean: number;
  /** Frame time percentiles in ms (nearest rank). */
  readonly p50: number;
  readonly p95: number;
  readonly p99: number;
  /** Frames longer than 33 ms. */
  readonly over33: number;
}

/** 90 s at 120 Hz fit in the ring; older intervals are dropped. */
export const FRAME_BUFFER = 16_384;
export const SLOW_FRAME_MS = 33;

export const EMPTY_STATS: FrameStats = Object.freeze({
  frames: 0,
  fpsMean: 0,
  p50: 0,
  p95: 0,
  p99: 0,
  over33: 0,
});

/** Nearest-rank percentile of an ascending array. */
export function percentile(sorted: ArrayLike<number>, p: number): number {
  const n = sorted.length;
  if (n === 0) return 0;
  const rank = Math.min(n, Math.max(1, Math.ceil((p / 100) * n)));
  return sorted[rank - 1] ?? 0;
}

export interface FrameRecorder {
  record(intervalMs: number): void;
  reset(): void;
  stats(): FrameStats;
}

export function createFrameRecorder(capacity = FRAME_BUFFER): FrameRecorder {
  const ring = new Float64Array(capacity);
  let count = 0;
  let next = 0;
  return {
    record(intervalMs) {
      if (!Number.isFinite(intervalMs) || intervalMs <= 0) return;
      ring[next] = intervalMs;
      next = (next + 1) % capacity;
      count = Math.min(capacity, count + 1);
    },
    reset() {
      count = 0;
      next = 0;
    },
    stats() {
      if (count === 0) return EMPTY_STATS;
      const sorted = ring.slice(0, count).sort();
      let sum = 0;
      let over33 = 0;
      for (const ms of sorted) {
        sum += ms;
        if (ms > SLOW_FRAME_MS) over33++;
      }
      return {
        frames: count,
        fpsMean: (count * 1000) / sum,
        p50: percentile(sorted, 50),
        p95: percentile(sorted, 95),
        p99: percentile(sorted, 99),
        over33,
      };
    },
  };
}
