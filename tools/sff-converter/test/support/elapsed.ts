// The only file of the package allowed to read the clock (plan-b precision 4): Vitest cannot cut
// a synchronous test by timeout, so the "< 1 s per case" of RF-B4 is measured.
export interface Timed<T> {
  readonly ms: number;
  readonly value?: T;
  readonly error?: unknown;
}

export function elapsed<T>(fn: () => T): Timed<T> {
  const start = performance.now();
  try {
    const value = fn();
    return { ms: performance.now() - start, value };
  } catch (error) {
    return { ms: performance.now() - start, error };
  }
}
