import type { ClientMessage, ServerMessage } from '@torneo/protocol';

// The connection of the fight view to /ws (010 RF57, DP-B7, plan-d decisions 5 and 6). Everything
// from outside is injected.
//
// What it asks for: `schedule`, and the one fight the caller follows (`followed-fights.ts`), never
// `next` nor a `last` it is not watching: the board already brings their summaries, and following
// them could go over the limit of DP-B7. A channel is never
// closed to open it again, and the probe uses `schedule`, so a socket asks for one `fight_sync` per
// fight, and for one fight at a time.

export const OPEN = 1;
export const BACKOFF_MAX_MS = 30_000;
const BACKOFF_FIRST_MS = 1000;
const JITTER = 0.2;
/** After this many failures in a row the view says it and starts polling (RF57). */
export const FAILURES_TO_DEGRADE = 3;
export const POLL_MS = 30_000;
/** At most one probe, and one immediate reconnection, in this time. */
export const CHECK_THROTTLE_MS = 2000;
export const PROBE_TIMEOUT_MS = 3000;
/** After a protocol error, this long without another one before trusting the connection again. */
export const STABLE_MS = 30_000;

/**
 * The address of the socket. A relative URL throws in `new WebSocket` on browsers before Chrome 125,
 * Firefox 124 and Safari 17.3, and it would freeze the view.
 */
export function socketUrl(location: { readonly protocol: string; readonly host: string }): string {
  return `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws`;
}

const SCHEDULE = 'schedule';
const fightChannel = (id: number): string => `fight:${id}`;

export interface SocketLike {
  readyState: number;
  onopen: (() => void) | null;
  onmessage: ((event: { data: unknown }) => void) | null;
  onclose: (() => void) | null;
  onerror: (() => void) | null;
  send(text: string): void;
  close(): void;
}

export interface FightSocketDeps {
  readonly open: () => SocketLike;
  readonly setTimer: (run: () => void, ms: number) => unknown;
  readonly clearTimer: (timer: unknown) => void;
  /** Monotonic ms. */
  readonly now: () => number;
  /** In [0, 1). Only for the jitter: it never touches the engine nor a fight. */
  readonly random: () => number;
  /** Each message of the server, once parsed. */
  readonly onMessage: (message: ServerMessage) => void;
  /** A new socket opened: its first sample starts the offset over. */
  readonly onNewConnection: () => void;
  /** After every change of what `state()` gives. */
  readonly onChange: () => void;
  /** `GET /api/fights/current`, in degraded mode. */
  readonly poll: () => void;
}

export interface FightSocketState {
  readonly degraded: boolean;
  /** Failures since this page opened, for the e2e: reconnecting after a message is not one. */
  readonly failures: number;
  /** `fight:*` opened on a closed channel, on any socket, for the e2e. */
  readonly subscribes: number;
}

export interface FightSocket {
  start(): void;
  stop(): void;
  /** `visibilitychange` to visible, or `online`: reconnects if needed, or probes the socket. */
  check(): void;
  /** Which fights to follow. Asks only for what is not open. */
  setFights(ids: readonly number[]): void;
  /** The server named this fight again: it can be asked for after a `ws.forbidden_channel`. */
  allow(id: number): void;
  /** A page with a gap or past the guard: this connection cannot be trusted. */
  protocolError(): void;
  state(): FightSocketState;
}

/** What was sent and is waiting for its answer: they come in order. */
type Awaiting = { readonly kind: 'schedule' } | { readonly kind: 'fight'; readonly id: number };

export function createFightSocket(deps: FightSocketDeps): FightSocket {
  let socket: SocketLike | null = null;
  let running = false;
  let attempts = 0;
  let failuresInRow = 0;
  let failures = 0;
  let subscribes = 0;
  let degraded = false;
  let gotMessage = false;
  let retry: unknown = null;
  let pollTimer: unknown = null;
  let probeTimer: unknown = null;
  let stableTimer: unknown = null;
  /** A protocol error, and no stable connection since: a first message does not clear it. */
  let suspect = false;
  let lastCheck = Number.NEGATIVE_INFINITY;
  let wanted: readonly number[] = [];
  let open = new Set<number>();
  const forbidden = new Set<number>();
  let awaiting: Awaiting[] = [];

  const cancel = (timer: unknown): null => {
    if (timer !== null) deps.clearTimer(timer);
    return null;
  };

  const jittered = (ms: number): number => ms * (1 - JITTER + 2 * JITTER * deps.random());

  function send(message: ClientMessage): void {
    socket?.send(JSON.stringify(message));
  }

  function syncFights(): void {
    if (socket === null || socket.readyState !== OPEN) return;
    for (const id of wanted) {
      if (open.has(id) || forbidden.has(id)) continue;
      open.add(id);
      subscribes++;
      awaiting.push({ kind: 'fight', id });
      send({ type: 'subscribe', channel: fightChannel(id) });
    }
    for (const id of [...open]) {
      if (wanted.includes(id)) continue;
      open.delete(id);
      send({ type: 'unsubscribe', channel: fightChannel(id) });
    }
  }

  function stopPolling(): void {
    pollTimer = cancel(pollTimer);
  }

  function schedulePoll(): void {
    pollTimer = deps.setTimer(() => {
      deps.poll();
      schedulePoll();
    }, jittered(POLL_MS));
  }

  function connect(): void {
    retry = cancel(retry);
    probeTimer = cancel(probeTimer);
    gotMessage = false;
    open = new Set();
    awaiting = [];
    let next: SocketLike;
    try {
      next = deps.open();
    } catch {
      // A failure like any other: never a view frozen without retrying.
      lost();
      return;
    }
    socket = next;
    deps.onNewConnection();
    next.onopen = () => {
      if (socket !== next) return;
      awaiting.push({ kind: 'schedule' });
      send({ type: 'subscribe', channel: SCHEDULE });
      syncFights();
    };
    next.onmessage = (event) => {
      if (socket !== next) return;
      onMessage(event.data);
    };
    next.onclose = () => {
      if (socket === next) lost();
    };
    next.onerror = () => {
      // The close comes after it, always: only then it is a failure.
    };
  }

  function drop(): void {
    const old = socket;
    socket = null;
    if (old === null) return;
    old.onopen = null;
    old.onmessage = null;
    old.onclose = null;
    old.onerror = null;
    old.close();
  }

  function countFailure(): void {
    failures++;
    failuresInRow++;
    if (failuresInRow >= FAILURES_TO_DEGRADE && !degraded) {
      degraded = true;
      deps.poll();
      schedulePoll();
    }
    deps.onChange();
  }

  function healthy(): void {
    suspect = false;
    attempts = 0;
    failuresInRow = 0;
    if (degraded) {
      degraded = false;
      stopPolling();
    }
    deps.onChange();
  }

  function lost(): void {
    socket = null;
    probeTimer = cancel(probeTimer);
    stableTimer = cancel(stableTimer);
    if (!running) return;
    if (!gotMessage) countFailure();
    const delay = Math.min(BACKOFF_MAX_MS, BACKOFF_FIRST_MS * 2 ** attempts);
    attempts++;
    retry = deps.setTimer(connect, jittered(delay));
  }

  function onMessage(data: unknown): void {
    if (typeof data !== 'string') return;
    let message: ServerMessage;
    try {
      message = JSON.parse(data) as ServerMessage;
    } catch {
      return;
    }
    if (!gotMessage) {
      gotMessage = true;
      if (suspect) {
        stableTimer = deps.setTimer(() => {
          stableTimer = null;
          healthy();
        }, STABLE_MS);
      } else {
        healthy();
      }
    }
    probeTimer = cancel(probeTimer);
    if (message.type === 'subscribed') {
      awaiting.shift();
    } else if (message.type === 'error') {
      const answered = awaiting.shift();
      if (answered?.kind === 'fight') {
        // Not a failure: after a restart, until the first evaluation (plan-b, "Para el plan D").
        open.delete(answered.id);
        forbidden.add(answered.id);
      }
    }
    deps.onMessage(message);
    deps.onChange();
  }

  return {
    start() {
      if (running) return;
      running = true;
      connect();
    },
    stop() {
      running = false;
      retry = cancel(retry);
      probeTimer = cancel(probeTimer);
      stableTimer = cancel(stableTimer);
      stopPolling();
      degraded = false;
      drop();
    },
    check() {
      if (!running) return;
      const now = deps.now();
      if (now - lastCheck < CHECK_THROTTLE_MS) return;
      lastCheck = now;
      if (socket === null || socket.readyState !== OPEN) {
        if (socket !== null) return; // Still connecting: it has its own outcome.
        attempts = 0;
        connect();
        return;
      }
      // It may be dead without knowing. `subscribe schedule` on an open channel answers
      // `subscribed` and does not count for DP-B7.
      awaiting.push({ kind: 'schedule' });
      send({ type: 'subscribe', channel: SCHEDULE });
      probeTimer = cancel(probeTimer);
      probeTimer = deps.setTimer(() => {
        probeTimer = null;
        attempts = 0;
        drop();
        connect();
      }, PROBE_TIMEOUT_MS);
    },
    setFights(ids) {
      wanted = ids;
      syncFights();
    },
    allow(id) {
      if (forbidden.delete(id)) syncFights();
    },
    protocolError() {
      if (socket === null) return;
      drop();
      // A first message arrived, so `lost` would not count it, and the next one would reset the
      // backoff: a server that repeats the error would make it reconnect without a brake.
      suspect = true;
      gotMessage = false;
      lost();
    },
    state() {
      return { degraded, failures, subscribes };
    },
  };
}
