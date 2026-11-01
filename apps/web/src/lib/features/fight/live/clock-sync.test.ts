import { describe, expect, it } from 'vitest';
import { createClockSync } from './clock-sync.js';

// 010 RNF "Sincronización" and plan-d decision 2.
function setup() {
  let mono = 1000;
  const clock = createClockSync(() => mono);
  return { clock, at: (value: number) => (mono = value) };
}

describe('createClockSync', () => {
  it('without samples it is not known', () => {
    const { clock } = setup();
    expect(clock.known()).toBe(false);
    expect(clock.now()).toBe(1000);
  });

  it('keeps the sample with the least latency: the largest offset', () => {
    const { clock, at } = setup();
    clock.sample(50_000); // offset 49 000
    at(2000);
    clock.sample(51_500); // offset 49 500: this one arrived faster
    at(3000);
    clock.sample(52_000); // offset 49 000: slow, ignored
    expect(clock.known()).toBe(true);
    expect(clock.now()).toBe(3000 + 49_500);
  });

  it('a new connection keeps the old offset until its first sample, and then starts over', () => {
    const { clock, at } = setup();
    clock.sample(60_000); // offset 59 000
    clock.newConnection();
    expect(clock.now()).toBe(1000 + 59_000);
    at(2000);
    clock.sample(60_500); // offset 58 500: lower, and it replaces the old one
    expect(clock.now()).toBe(2000 + 58_500);
    at(3000);
    clock.sample(61_600); // offset 58 600
    expect(clock.now()).toBe(3000 + 58_600);
  });

  it('a monotonic clock that paused is corrected by the next sample', () => {
    const { clock, at } = setup();
    clock.sample(10_000); // offset 9000
    at(1000); // 60 s of real time went by with the clock paused
    clock.sample(70_000);
    expect(clock.now()).toBe(70_000);
  });
});
