// The server's time as the client estimates it (010 RNF "Sincronización", plan-d decision 2). Pure:
// the monotonic clock is injected. The offset is the largest `server_time − monotonic` of what this
// connection received, the sample with the least latency. Monotonic and not `Date`: changing the
// time of the phone does not move the fight, and if the clock pauses while the screen is locked,
// the maximum goes up with the next sample.

export interface ClockSync {
  /** A `server_time` (ms since the epoch) received just now. */
  sample(serverMs: number): void;
  /** A new connection: the old offset stays until its first sample, and then starts over. */
  newConnection(): void;
  /** The estimated server time; before any sample, the monotonic clock itself. */
  now(): number;
  /** Whether there is any sample at all. */
  known(): boolean;
}

export function createClockSync(monotonic: () => number): ClockSync {
  let offset: number | null = null;
  let startOver = false;
  return {
    sample(serverMs) {
      const value = serverMs - monotonic();
      offset = offset === null || startOver ? value : Math.max(offset, value);
      startOver = false;
    },
    newConnection() {
      startOver = true;
    },
    now() {
      return monotonic() + (offset ?? 0);
    },
    known() {
      return offset !== null;
    },
  };
}
