import { describe, expect, it } from 'vitest';
import { reroute } from './hooks.js';

function rr(path: string): string | void | Promise<string | void> {
  return reroute({ url: new URL(`http://localhost${path}`), fetch });
}

describe('reroute', () => {
  it.each([
    ['/es/dev/components', '/dev/components'],
    ['/en/dev/components', '/dev/components'],
    ['/es/', '/'],
    ['/en/', '/'],
  ])('%s → %s', async (input, expected) => {
    expect(await rr(input)).toBe(expected);
  });

  it.each(['/healthz', '/api/x', '/ws'])('%s stays unchanged', async (path) => {
    expect(await rr(path)).toBe(path);
  });
});
