import { describe, expect, it } from 'vitest';
import type { ClientMessage, FightSummary, FightsCurrent, ServerMessage } from '@torneo/protocol';
import {
  BACKOFF_MAX_MS,
  CHECK_THROTTLE_MS,
  OPEN,
  POLL_MS,
  PROBE_TIMEOUT_MS,
  createFightSocket,
  type SocketLike,
} from './fight-socket.js';
import { followedFights } from './followed-fights.js';

// 010 RF57, DP-B7 and plan-d decisions 5 and 6, with fake sockets and timers.
class FakeSocket implements SocketLike {
  readyState = 0;
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  sent: ClientMessage[] = [];
  closed = false;
  send(text: string): void {
    this.sent.push(JSON.parse(text) as ClientMessage);
  }
  close(): void {
    this.closed = true;
  }
  accept(): void {
    this.readyState = OPEN;
    this.onopen?.();
  }
  deliver(message: ServerMessage): void {
    this.onmessage?.({ data: JSON.stringify(message) });
  }
  cut(): void {
    this.readyState = 3;
    this.onclose?.();
  }
  channels(type: 'subscribe' | 'unsubscribe'): string[] {
    return this.sent.flatMap((m) => (m.type === type ? [m.channel] : []));
  }
}

function setup(random = () => 0.5) {
  let now = 0;
  const timers = new Map<number, { at: number; run: () => void }>();
  let nextTimer = 0;
  const sockets: FakeSocket[] = [];
  const messages: ServerMessage[] = [];
  let polls = 0;
  let connections = 0;
  const socket = createFightSocket({
    open: () => {
      const s = new FakeSocket();
      sockets.push(s);
      return s;
    },
    setTimer: (run, ms) => {
      nextTimer++;
      timers.set(nextTimer, { at: now + ms, run });
      return nextTimer;
    },
    clearTimer: (timer) => timers.delete(timer as number),
    now: () => now,
    random,
    onMessage: (m) => messages.push(m),
    onNewConnection: () => connections++,
    onChange: () => undefined,
    poll: () => polls++,
  });
  const advance = (ms: number) => {
    const end = now + ms;
    for (;;) {
      const due = [...timers.entries()]
        .filter(([, t]) => t.at <= end)
        .sort((p, q) => p[1].at - q[1].at)[0];
      if (due === undefined) break;
      timers.delete(due[0]);
      now = due[1].at;
      due[1].run();
    }
    now = end;
  };
  const last = () => sockets.at(-1) as FakeSocket;
  return {
    socket,
    sockets,
    messages,
    advance,
    last,
    polls: () => polls,
    connections: () => connections,
    pendingTimers: () => timers.size,
  };
}

const subscribed = (channel: string): ServerMessage => ({ type: 'subscribed', channel });

describe('connecting', () => {
  it('subscribes schedule on open, and the wanted fights', () => {
    const { socket, last } = setup();
    socket.setFights([7]);
    socket.start();
    last().accept();
    expect(last().channels('subscribe')).toEqual(['schedule', 'fight:7']);
    expect(socket.state().subscribes).toBe(1);
  });

  it('asks only for what is not open, and closes what is no longer wanted', () => {
    const { socket, last } = setup();
    socket.start();
    last().accept();
    socket.setFights([7]);
    socket.setFights([7]);
    socket.setFights([]);
    expect(last().channels('subscribe')).toEqual(['schedule', 'fight:7']);
    expect(last().channels('unsubscribe')).toEqual(['fight:7']);
  });

  it('passes every message on', () => {
    const { socket, last, messages } = setup();
    socket.start();
    last().accept();
    last().deliver(subscribed('schedule'));
    expect(messages).toEqual([subscribed('schedule')]);
  });

  it('ignores what does not parse, and binary data', () => {
    const { socket, last, messages } = setup();
    socket.start();
    last().accept();
    last().onmessage?.({ data: 'not json' });
    last().onmessage?.({ data: new ArrayBuffer(1) });
    expect(messages).toEqual([]);
  });
});

describe('reconnecting (RF57)', () => {
  it('backs off 1, 2, 4… up to 30 s while it fails', () => {
    const { socket, sockets, advance, last } = setup();
    socket.start();
    const expected = [1000, 2000, 4000, 8000, 16_000, BACKOFF_MAX_MS, BACKOFF_MAX_MS];
    for (const [i, ms] of expected.entries()) {
      last().cut();
      advance(ms - 1);
      expect(sockets).toHaveLength(i + 1);
      advance(1);
      expect(sockets).toHaveLength(i + 2);
    }
  });

  it('the jitter is ±20 %', () => {
    for (const [random, ms] of [
      [0, 800],
      [0.999999, 1200],
    ] as const) {
      const { socket, sockets, advance, last } = setup(() => random);
      socket.start();
      last().cut();
      advance(ms - 2);
      expect(sockets).toHaveLength(1);
      advance(3);
      expect(sockets).toHaveLength(2);
    }
  });

  it('a message resets the backoff, and a cut after a message is not a failure', () => {
    const { socket, sockets, advance, last } = setup();
    socket.start();
    last().cut();
    advance(1000);
    last().cut();
    advance(2000);
    last().accept();
    last().deliver(subscribed('schedule'));
    last().cut();
    expect(socket.state().failures).toBe(2);
    advance(1000);
    expect(sockets).toHaveLength(4);
  });

  it('subscribes again what it had, and starts the connection over', () => {
    const { socket, advance, last, connections } = setup();
    socket.setFights([7]);
    socket.start();
    last().accept();
    last().deliver(subscribed('schedule'));
    last().cut();
    advance(1000);
    last().accept();
    expect(last().channels('subscribe')).toEqual(['schedule', 'fight:7']);
    expect(connections()).toBe(2);
    expect(socket.state().subscribes).toBe(2);
  });

  it('ws.rate_limited and any other close take the same way, with backoff', () => {
    const { socket, sockets, advance, last } = setup();
    socket.start();
    last().accept();
    last().deliver(subscribed('schedule'));
    last().cut(); // 1008 ws.rate_limited: the code does not change anything.
    expect(sockets).toHaveLength(1);
    advance(1000);
    expect(sockets).toHaveLength(2);
  });

  it('a gap closes this connection and reconnects with backoff', () => {
    const { socket, sockets, advance, last } = setup();
    socket.start();
    last().accept();
    last().deliver(subscribed('schedule'));
    const first = last();
    socket.protocolError();
    expect(first.closed).toBe(true);
    advance(1000);
    expect(sockets).toHaveLength(2);
  });
});

describe('degraded (RF57)', () => {
  it('says it once after 3 failures, polls at once and every 30 s with jitter', () => {
    const { socket, advance, last, polls } = setup();
    socket.start();
    last().cut();
    advance(1000);
    last().cut();
    expect(socket.state().degraded).toBe(false);
    expect(polls()).toBe(0);
    advance(2000);
    last().cut();
    expect(socket.state().degraded).toBe(true);
    expect(polls()).toBe(1);
    advance(POLL_MS);
    expect(polls()).toBe(2);
    expect(socket.state().degraded).toBe(true);
  });

  it('keeps trying, and a message ends it and the polling', () => {
    const { socket, advance, last, polls } = setup();
    socket.start();
    for (const ms of [1000, 2000]) {
      last().cut();
      advance(ms);
    }
    last().cut();
    advance(4000);
    last().accept();
    last().deliver(subscribed('schedule'));
    expect(socket.state().degraded).toBe(false);
    const count = polls();
    advance(POLL_MS * 3);
    expect(polls()).toBe(count);
  });

  it('a connection that opens without a message is still a failure', () => {
    const { socket, advance, last } = setup();
    socket.start();
    for (const ms of [1000, 2000]) {
      last().accept();
      last().cut();
      advance(ms);
    }
    last().accept();
    last().cut();
    expect(socket.state().degraded).toBe(true);
  });
});

describe('check: visibilitychange and online (RF57)', () => {
  it('without a socket it reconnects at once and restarts the backoff', () => {
    const { socket, sockets, advance, last } = setup();
    socket.start();
    last().cut();
    socket.check();
    expect(sockets).toHaveLength(2);
    last().cut();
    advance(1000);
    expect(sockets).toHaveLength(3);
  });

  it('with an open socket it probes with schedule, and spends no fight_sync', () => {
    const { socket, sockets, advance, last } = setup();
    socket.start();
    last().accept();
    last().deliver(subscribed('schedule'));
    const before = socket.state().subscribes;
    socket.check();
    expect(last().channels('subscribe')).toEqual(['schedule', 'schedule']);
    expect(socket.state().subscribes).toBe(before);
    last().deliver(subscribed('schedule'));
    advance(PROBE_TIMEOUT_MS * 2);
    expect(sockets).toHaveLength(1);
  });

  it('without an answer in 3 s it closes and reconnects', () => {
    const { socket, sockets, advance, last } = setup();
    socket.start();
    last().accept();
    last().deliver(subscribed('schedule'));
    const first = last();
    socket.check();
    advance(PROBE_TIMEOUT_MS - 1);
    expect(sockets).toHaveLength(1);
    advance(1);
    expect(first.closed).toBe(true);
    expect(sockets).toHaveLength(2);
    expect(socket.state().failures).toBe(0);
  });

  it('at most one probe every 2 s', () => {
    const { socket, advance, last } = setup();
    socket.start();
    last().accept();
    last().deliver(subscribed('schedule'));
    socket.check();
    socket.check();
    socket.check();
    expect(last().channels('subscribe')).toHaveLength(2);
    last().deliver(subscribed('schedule'));
    advance(CHECK_THROTTLE_MS);
    socket.check();
    expect(last().channels('subscribe')).toHaveLength(3);
  });

  it('at most one immediate reconnection every 2 s', () => {
    const { socket, sockets, advance, last } = setup();
    socket.start();
    last().cut();
    socket.check();
    last().cut();
    socket.check();
    expect(sockets).toHaveLength(2);
    advance(CHECK_THROTTLE_MS);
    expect(sockets.length).toBeGreaterThan(2);
  });

  it('while connecting it waits for the outcome', () => {
    const { socket, sockets, last } = setup();
    socket.start();
    socket.check();
    expect(sockets).toHaveLength(1);
    expect(last().closed).toBe(false);
  });
});

describe('ws.forbidden_channel', () => {
  it('does not ask again until the board names it, and is not a failure', () => {
    const { socket, last } = setup();
    socket.setFights([7]);
    socket.start();
    last().accept();
    last().deliver(subscribed('schedule'));
    last().deliver({ type: 'error', code: 'ws.forbidden_channel' });
    socket.setFights([7]);
    expect(last().channels('subscribe')).toEqual(['schedule', 'fight:7']);
    expect(socket.state().failures).toBe(0);
    socket.allow(7);
    expect(last().channels('subscribe')).toEqual(['schedule', 'fight:7', 'fight:7']);
  });

  it('an error after subscribing schedule forbids nothing', () => {
    const { socket, last } = setup();
    socket.setFights([7]);
    socket.start();
    last().accept();
    last().deliver({ type: 'error', code: 'ws.invalid_message' });
    last().deliver(subscribed('fight:7'));
    socket.setFights([7]);
    expect(last().channels('subscribe')).toEqual(['schedule', 'fight:7']);
  });
});

describe('stop', () => {
  it('closes the socket and every timer', () => {
    const { socket, advance, last, sockets, pendingTimers } = setup();
    socket.start();
    last().accept();
    const first = last();
    socket.stop();
    expect(first.closed).toBe(true);
    expect(pendingTimers()).toBe(0);
    advance(60_000);
    expect(sockets).toHaveLength(1);
  });
});

// DP-B7: at most 3 `fight_sync` asked for, by socket and minute. A legitimate client does not reach
// it: one per fight and socket, and 2 only on a new socket with a watched fight to finish.
describe('DP-B7: an hour of boards, with a legitimate client', () => {
  const CADENCE = 113_000; // The smallest grid.
  const WINDOW = 5000;
  const PRE_LIVE = 3000;
  const FIGHT = 54_000;
  const HOUR = 3_600_000;
  const STEP = 1000;

  function mulberry32(seed: number): () => number {
    let a = seed;
    return () => {
      a = (a + 0x6d2b79f5) | 0;
      let t = Math.imul(a ^ (a >>> 15), 1 | a);
      t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }

  function summary(id: number, state: FightSummary['state']): FightSummary {
    return { id, state } as FightSummary;
  }

  /** The board at `at`, in a grid: slot `n` opens at `n * cadence`. */
  function boardAt(at: number, cadence: number, window: number, preLive: number): FightsCurrent {
    const n = Math.floor(at / cadence);
    const into = at - n * cadence;
    const end = window + preLive + FIGHT + 5000;
    let current: FightSummary;
    let last: FightSummary | null = n > 0 ? summary(n - 1, 'settled') : null;
    let next: FightSummary | null = summary(n + 1, 'scheduled');
    if (into < window) current = summary(n, 'betting_open');
    else if (into < window + preLive) current = summary(n, 'betting_closed');
    else if (into < end) current = summary(n, 'live');
    else {
      current = summary(n + 1, 'scheduled');
      last = summary(n, 'settled');
      next = null;
    }
    return { server_time: String(at), halted: false, current, next, last };
  }

  function run(seed: number, cadence: number, window: number, preLive: number, startAt = 0) {
    const rng = mulberry32(seed);
    const { socket, sockets, advance, last } = setup(rng);
    socket.start();
    last().accept();
    const asked = new Map<FakeSocket, { at: number; id: number }[]>();
    const notFollowed: number[] = [];
    let watched: number | null = null;
    let watchedEnded = false;
    let time = startAt;
    while (time < startAt + HOUR) {
      const board = boardAt(time, cadence, window, preLive);
      const { current, last: previous } = board;
      if (current !== null && (current.state === 'betting_closed' || current.state === 'live')) {
        if (watched !== current.id) watchedEnded = false;
        watched = current.id;
      } else if (previous !== null && watched === previous.id) {
        watchedEnded = true;
      }
      socket.setFights(followedFights(board, watched, watchedEnded));
      if (last().readyState === OPEN) last().deliver({ type: 'schedule_update', board });
      for (const s of sockets) {
        const list = asked.get(s) ?? [];
        const fights = s.channels('subscribe').filter((c) => c.startsWith('fight:'));
        for (const channel of fights.slice(list.length)) {
          const id = Number(channel.slice('fight:'.length));
          list.push({ at: time, id });
          // Everything it asks for is the fight on stage now: never `next` nor a `last` unwatched.
          if (id !== watched) notFollowed.push(id);
        }
        asked.set(s, list);
      }
      const roll = rng();
      if (roll < 0.002) socket.check();
      else if (roll < 0.003 && last().readyState === OPEN) last().cut();
      else if (roll < 0.004 && last().readyState === OPEN) socket.protocolError();
      if (last().readyState === 0) last().accept();
      advance(STEP);
      time += STEP;
    }
    return { asked, notFollowed };
  }

  function assertUnderLimit(asked: Map<FakeSocket, { at: number; id: number }[]>): number {
    let total = 0;
    for (const list of asked.values()) {
      total += list.length;
      for (const [i, entry] of list.entries()) {
        const window = list.slice(i).filter((other) => other.at - entry.at < 60_000);
        // 2 only when the socket is new; never the 3 of the limit.
        const allowed = entry === list[0] ? 2 : 1;
        expect(window.length, `socket asked for ${JSON.stringify(window)}`).toBeLessThanOrEqual(
          allowed,
        );
        expect(new Set(window.map((w) => w.id)).size).toBe(window.length);
      }
    }
    return total;
  }

  it.each([1, 2, 3, 4, 5])('the smallest grid, seed %i, with probes and cuts', (seed) => {
    const { asked, notFollowed } = run(seed, CADENCE, WINDOW, PRE_LIVE);
    expect(assertUnderLimit(asked)).toBeGreaterThan(20);
    expect(notFollowed).toEqual([]);
  });

  it.each([540_000, 570_000, 575_000])(
    'a 10 min grid, coming in at %i ms of the slot: it never asks for next nor last',
    (startAt) => {
      const { asked, notFollowed } = run(9, 600_000, 450_000, 30_000, startAt);
      assertUnderLimit(asked);
      expect(notFollowed).toEqual([]);
    },
  );
});

describe('followedFights', () => {
  const fight = (id: number, state: FightSummary['state']) => ({ id, state }) as FightSummary;
  const board = (over: Partial<FightsCurrent>): FightsCurrent => ({
    server_time: '0',
    halted: false,
    current: null,
    next: null,
    last: null,
    ...over,
  });

  it('current only from pre_live on', () => {
    for (const state of ['scheduled', 'betting_open'] as const) {
      expect(followedFights(board({ current: fight(1, state) }), null, false)).toEqual([]);
    }
    for (const state of ['betting_closed', 'live'] as const) {
      expect(followedFights(board({ current: fight(1, state) }), null, false)).toEqual([1]);
    }
  });

  it('never next nor a last it is not watching', () => {
    const b = board({
      current: fight(2, 'live'),
      next: fight(3, 'scheduled'),
      last: fight(1, 'settled'),
    });
    expect(followedFights(b, 2, false)).toEqual([2]);
  });

  it('a watched fight without end stays followed, and is the only way to 2', () => {
    const b = board({ current: fight(2, 'live'), last: fight(1, 'settled') });
    expect(followedFights(b, 1, false)).toEqual([2, 1]);
    expect(followedFights(b, 1, true)).toEqual([2]);
  });

  it('without a board it only keeps the watched fight', () => {
    expect(followedFights(null, 1, false)).toEqual([1]);
    expect(followedFights(null, null, false)).toEqual([]);
  });
});
