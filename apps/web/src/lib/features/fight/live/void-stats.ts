import type { VoidStats } from '@torneo/protocol';

// 012 RF22: the public counter, for a fight whose `secret_lost` arrived by the socket. It reads once
// per fight, after a random wait: everyone receives that message at once. A failure only leaves out
// one line, and it is not read again.

export type { VoidStats };

/** The longest wait before reading, like the rereading of 012 RF36. */
export const MAX_READ_DELAY_MS = 3000;

/** The counter if it has the shape of the protocol; `null` otherwise. */
export function parseVoidStats(value: unknown): VoidStats | null {
  if (typeof value !== 'object' || value === null) return null;
  const { voided, secret_lost: secretLost } = value as Record<string, unknown>;
  if (!Number.isSafeInteger(voided) || !Number.isSafeInteger(secretLost)) return null;
  return { voided: voided as number, secret_lost: secretLost as number };
}

export interface VoidStatsDeps {
  readonly fetchFn: typeof fetch;
  readonly random: () => number;
  readonly wait: (ms: number) => Promise<void>;
}

const browserDeps = (): VoidStatsDeps => ({
  fetchFn: (...args) => fetch(...args),
  random: Math.random,
  wait: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
});

export interface VoidStatsReader {
  /** The counter, the first time for this fight. `null` if it was already read or failed. */
  read(fightId: number): Promise<VoidStats | null>;
}

/** `alreadyRead`: the fight the SSR read it for. */
export function createVoidStatsReader(
  alreadyRead: number | null,
  deps: VoidStatsDeps = browserDeps(),
): VoidStatsReader {
  const seen = new Set<number>(alreadyRead === null ? [] : [alreadyRead]);
  return {
    async read(fightId) {
      if (seen.has(fightId)) return null;
      seen.add(fightId);
      await deps.wait(deps.random() * MAX_READ_DELAY_MS);
      try {
        const response = await deps.fetchFn('/api/fights/void-stats', {
          headers: { accept: 'application/json' },
          cache: 'no-store',
        });
        return response.ok ? parseVoidStats(await response.json()) : null;
      } catch {
        return null;
      }
    },
  };
}
