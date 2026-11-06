import type { Page, WebSocketRoute } from '@playwright/test';
import type { FightEvent, FightSummary, FightsCurrent, ServerMessage } from '@torneo/protocol';

// A server of mentira for /ws (010 plan-d decision 15): every state of the view in seconds. The times
// are relative to the real clock, so a fight can be at any `t` right away.

export const DELAY_MS = 3500;
export const MAX_DURATION_MS = 90_000;
export const TIME_LIMIT_MS = 86_000;

export function fight(id: number, over: Partial<FightSummary> = {}): FightSummary {
  return {
    id,
    state: 'scheduled',
    a: { slug: 'fontanero-saltarin', max_hp: 100 },
    b: { slug: 'guerrero-erizado', max_hp: 100 },
    ruleset_version: 1,
    snapshot_sha256: 'ab'.repeat(32),
    seed_commit: null,
    opens_at: String(Date.now() - 60_000),
    closes_at: String(Date.now() - 30_000),
    live_at: String(Date.now() + 300_000),
    live_started_at: null,
    time_limit_ms: TIME_LIMIT_MS,
    max_duration_ms: MAX_DURATION_MS,
    playback_delay_ms: DELAY_MS,
    final: null,
    ...over,
  };
}

/** A fight in `live`, whose playback is at `t` now. */
export function liveAt(id: number, t: number, over: Partial<FightSummary> = {}): FightSummary {
  return fight(id, {
    state: 'live',
    live_started_at: String(Date.now() - DELAY_MS - t),
    seed_commit: 'cd'.repeat(32),
    ...over,
  });
}

export function board(over: Partial<FightsCurrent> = {}): FightsCurrent {
  return {
    server_time: String(Date.now()),
    halted: false,
    current: null,
    next: null,
    last: null,
    ...over,
  };
}

const hit = (t: number, attacker: 'a' | 'b', hp: number): FightEvent[] => [
  { type: 'attack', t: t - 400, attacker, ability: 'x', kind: 'strike' },
  { type: 'hit', t, attacker, ability: 'x', damage: 1, crit: false, weak: false, hp },
];

/** A fight of its own: strikes from 3000 ms on, every 2000 ms, HP going down by one. */
export function strikes(until: number): FightEvent[] {
  const events: FightEvent[] = [{ type: 'intro', t: 0 }];
  let hp = 100;
  for (let t = 3000; t <= until; t += 2000) events.push(...hit(t, 'a', --hp));
  return events;
}

/** A complete KO at `koAt`; `end` comes 4000 ms later, and the closing 1500 ms after the KO. */
export function koFight(koAt: number): FightEvent[] {
  return [
    ...strikes(koAt - 2000),
    { type: 'attack', t: koAt - 400, attacker: 'a', ability: 'x', kind: 'strike' },
    {
      type: 'hit',
      t: koAt,
      attacker: 'a',
      ability: 'x',
      damage: 1,
      crit: true,
      weak: false,
      hp: 0,
    },
    { type: 'ko', t: koAt, side: 'b' },
    { type: 'end', t: koAt + 4000, winner: 'a', reason: 'ko' },
  ].map((event) => {
    // The strikes leave HP where the finishing one can take it to 0.
    return event.type === 'hit' && event.hp === 0
      ? { ...event, damage: 100 - strikes(koAt - 2000).filter((e) => e.type === 'hit').length }
      : event;
  }) as FightEvent[];
}

export interface FakeServer {
  /** To every open socket. */
  send(message: ServerMessage): void;
  update(next: FightsCurrent): void;
  events(id: number, fromIndex: number, list: readonly FightEvent[]): void;
  /** Refuses every socket from now on, and closes those open. */
  block(): void;
  allow(): void;
  /** Sockets that were opened so far. */
  readonly connections: () => number;
  /** What the client sent, as it arrived. */
  readonly received: () => { type: string; channel?: string }[];
  closeAll(): void;
}

export async function fakeServer(
  page: Page,
  initial: () => FightsCurrent = board,
  onSubscribeFight?: (id: number, server: FakeServer) => void,
): Promise<FakeServer> {
  const sockets = new Set<WebSocketRoute>();
  const received: { type: string; channel?: string }[] = [];
  let connections = 0;
  let blocked = false;
  const server: FakeServer = {
    send(message) {
      for (const socket of sockets) socket.send(JSON.stringify(message));
    },
    update(next) {
      server.send({ type: 'schedule_update', board: next });
    },
    events(id, fromIndex, list) {
      server.send({
        type: 'fight_events',
        fight_id: id,
        from_index: fromIndex,
        events: [...list],
        server_time: String(Date.now()),
      });
    },
    block() {
      blocked = true;
      server.closeAll();
    },
    allow() {
      blocked = false;
    },
    connections: () => connections,
    received: () => received,
    closeAll() {
      for (const socket of [...sockets]) void socket.close();
      sockets.clear();
    },
  };
  await page.routeWebSocket('**/ws', (socket) => {
    connections++;
    if (blocked) {
      void socket.close();
      return;
    }
    sockets.add(socket);
    socket.onClose(() => sockets.delete(socket));
    socket.onMessage((data) => {
      const message = JSON.parse(String(data)) as { type: string; channel?: string };
      received.push(message);
      if (message.type !== 'subscribe' || message.channel === undefined) return;
      socket.send(JSON.stringify({ type: 'subscribed', channel: message.channel }));
      if (message.channel === 'schedule') {
        socket.send(JSON.stringify({ type: 'schedule_update', board: initial() }));
      } else if (message.channel.startsWith('fight:')) {
        onSubscribeFight?.(Number(message.channel.slice('fight:'.length)), server);
      }
    });
  });
  return server;
}
