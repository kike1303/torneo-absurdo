import type { FightEvent, FightSummary, FightsCurrent, ServerMessage } from '@torneo/protocol';
import type { LiveSession, StageErrorCode, StageSnapshot } from '../live-session.js';
import { createClockSync } from './clock-sync.js';
import { createEventBuffer, type EventBuffer } from './event-buffer.js';
import { createFightSession, type FightSession } from './fight-session.js';
import {
  CHECK_THROTTLE_MS,
  createFightSocket,
  socketUrl,
  type FightSocket,
  type SocketLike,
} from './fight-socket.js';
import { followedFights } from './followed-fights.js';
import { roundClockMs } from './round-clock.js';
import { endEvent, hasPendingEvents, textSceneAt, type TextScene } from './text-scene.js';
import { adoptsCurrent, viewState, type View, type Watched } from './view-state.js';

// Joins the socket, the clock and the buffers of the home page (010 plan-d decisions 2 to 9), and
// gives what the view reads. All the decisions are in the pure modules around it.

const TICK_MS = 250;
/** The placeholders, one per side (ADR-0004). */
const ASSETS = { a: 'dummy_a', b: 'dummy_b' } as const;

/** Only for `?debug=1` with the dev catalog (plan-d decision 14): nothing of the future in it. */
export interface FightDebug {
  readonly t: number | null;
  readonly tAt: number;
  readonly events: number;
  readonly digest: string;
  readonly subscribes: number;
  readonly failures: number;
  readonly connection: 'live' | 'degraded';
  /** `t` when the first frame was on screen. */
  readonly paintedT: number | null;
}

function isBoard(value: unknown): value is FightsCurrent {
  if (typeof value !== 'object' || value === null) return false;
  const board = value as Record<string, unknown>;
  return typeof board.server_time === 'string' && 'current' in board && 'halted' in board;
}

/** FNV-1a of what was received: two clients with the same events give the same text. */
function digestOf(events: readonly FightEvent[]): string {
  let hash = 0x811c9dc5;
  const text = JSON.stringify(events);
  for (let i = 0; i < text.length; i++) {
    hash ^= text.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16);
}

export class FightStore {
  board = $state.raw<FightsCurrent | null>(null);
  serverNow = $state(0);
  degraded = $state(false);
  private readonly ssrFailed: boolean;

  private watchedSummary = $state.raw<FightSummary | null>(null);
  private eventsVersion = $state(0);
  private snapshot = $state.raw<StageSnapshot | null>(null);
  private painted = $state(false);
  private textMode = $state(false);
  private lateNoPixi = $state(false);
  /** A rejection or a `fail`, not a browser without WebGL. */
  playbackError = $state(false);
  noAnimation = $state(false);
  private debugSocket = $state({ subscribes: 0, failures: 0 });
  private tAt = $state(0);
  private paintedT = $state<number | null>(null);

  private buffer: EventBuffer = createEventBuffer();
  private session: FightSession | null = null;
  private socket: FightSocket | null = null;
  private readonly clock = createClockSync(() => performance.now());
  private lastVisibleSample = Number.NEGATIVE_INFINITY;

  constructor(board: FightsCurrent | null) {
    this.board = board;
    this.ssrFailed = board === null;
    // The same value on the server and in the first render of the client: no hydration mismatch.
    this.serverNow = board === null ? 0 : Number(board.server_time);
  }

  private readonly events = $derived.by((): readonly FightEvent[] => {
    void this.eventsVersion;
    return [...this.buffer.events];
  });

  private readonly t = $derived.by((): number | null => {
    const fight = this.watchedSummary;
    if (fight === null || fight.live_started_at === null) return null;
    return this.serverNow - Number(fight.live_started_at) - fight.playback_delay_ms;
  });

  private readonly watched = $derived.by((): Watched | null => {
    const summary = this.watchedSummary;
    if (summary === null) return null;
    const t = this.t;
    const end = endEvent(this.events);
    const scene = t === null ? null : textSceneAt(this.maxHp(summary), this.events, t);
    const snapshot = this.snapshot;
    const ending = this.textMode
      ? (scene?.ended ?? null)
      : snapshot?.ending
        ? { reason: snapshot.ending, winner: snapshot.winner }
        : null;
    return {
      summary,
      t,
      end: end === null ? null : { t: end.t, winner: end.winner, reason: end.reason },
      ending,
      painted: this.painted || this.textMode,
      textMode: this.textMode,
      lateNoPixi: this.lateNoPixi,
      pending: t !== null && hasPendingEvents(this.events, t),
    };
  });

  readonly view = $derived.by((): View =>
    viewState({
      board: this.board,
      watched: this.watched,
      serverNow: this.serverNow,
      degraded: this.degraded,
      ssrFailed: this.ssrFailed,
    }),
  );

  /** The id of the fight on the stage, to mount it once per fight. */
  readonly stageFight = $derived(this.watchedSummary?.id ?? null);

  /** HP without the canvas, by `t`. */
  readonly textScene = $derived.by((): TextScene | null => {
    const summary = this.watchedSummary;
    const t = this.t;
    if (!this.textMode || summary === null || t === null) return null;
    return textSceneAt(this.maxHp(summary), this.events, t);
  });

  readonly roundClock = $derived.by((): number | null => {
    const view = this.view;
    const fight = view.live;
    if (!view.showRoundClock || fight === null || fight.live_started_at === null) return null;
    const t = this.serverNow - Number(fight.live_started_at) - fight.playback_delay_ms;
    const events = fight.id === this.watchedSummary?.id ? this.events : [];
    return roundClockMs(fight.time_limit_ms, events, t);
  });

  readonly debug = $derived.by((): FightDebug => ({
    t: this.t,
    tAt: this.tAt,
    events: this.events.length,
    digest: digestOf(this.events),
    subscribes: this.debugSocket.subscribes,
    failures: this.debugSocket.failures,
    connection: this.degraded ? 'degraded' : 'live',
    paintedT: this.paintedT,
  }));

  private maxHp(summary: FightSummary) {
    return { a: summary.a.max_hp, b: summary.b.max_hp };
  }

  /** The session of the stage: pending until it is up to date, so `loadLive` waits for the intro. */
  openSession = (): Promise<LiveSession> => {
    if (this.session === null) return new Promise(() => undefined);
    return this.session.ready;
  };

  onSnapshot = (snapshot: StageSnapshot | null): void => {
    this.snapshot = snapshot;
  };

  onPainted = (): void => {
    this.painted = true;
    this.tick();
    this.paintedT = this.t;
  };

  onStageFailure = (code: StageErrorCode): void => {
    this.textMode = true;
    if (code === 'webgl_unavailable') this.noAnimation = true;
    else this.playbackError = true;
  };

  private tick(): void {
    if (this.clock.known()) this.serverNow = this.clock.now();
    this.tAt = Date.now();
    // No message says when a pose ends.
    this.adoptCurrent();
  }

  private sampleOf(serverTime: string): void {
    this.clock.sample(Number(serverTime));
    this.tick();
  }

  private applyBoard(board: FightsCurrent): void {
    this.board = board;
    this.sampleOf(board.server_time);
    const named = [board.current, board.next, board.last];
    for (const fight of named) if (fight !== null) this.socket?.allow(fight.id);
    const seen = named.find((fight) => fight?.id === this.watchedSummary?.id);
    if (seen !== undefined && seen !== null) this.updateWatched(seen);
    this.adoptCurrent();
    this.follow();
  }

  /** The next fight takes the stage only when the one watched is finished. */
  private adoptCurrent(): void {
    const current = this.board?.current ?? null;
    if (current === null || !adoptsCurrent(current, this.watched)) return;
    this.watch(current);
    this.follow();
  }

  private updateWatched(summary: FightSummary): void {
    this.watchedSummary = summary;
    this.session?.changed();
  }

  private follow(): void {
    this.socket?.setFights(
      followedFights(
        this.board,
        this.watchedSummary?.id ?? null,
        // Nothing else will come: an end, or a final that has none.
        endEvent(this.buffer.events) !== null ||
          (this.watchedSummary?.final != null && this.watchedSummary.final.kind !== 'decided'),
      ),
    );
  }

  private watch(summary: FightSummary): void {
    this.session?.stop();
    this.buffer = createEventBuffer();
    this.eventsVersion++;
    this.watchedSummary = summary;
    this.snapshot = null;
    this.painted = false;
    this.paintedT = null;
    this.textMode = false;
    this.lateNoPixi = false;
    this.playbackError = false;
    this.noAnimation = false;
    this.session = createFightSession({
      summary: () => this.watchedSummary ?? summary,
      events: () => this.buffer.events,
      serverNow: () => this.clock.now(),
      assets: ASSETS,
      setTimer: (run, ms) => setTimeout(run, ms),
      clearTimer: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
      onLate: () => {
        this.lateNoPixi = true;
      },
    });
  }

  private onMessage(message: ServerMessage): void {
    if (message.type === 'schedule_update') {
      this.applyBoard(message.board);
    } else if (message.type === 'fight_sync') {
      this.sampleOf(message.server_time);
      if (message.fight.id === this.watchedSummary?.id) this.updateWatched(message.fight);
    } else if (message.type === 'fight_events') {
      this.sampleOf(message.server_time);
      if (message.fight_id !== this.watchedSummary?.id) return;
      const placed = this.buffer.place(message.from_index, message.events);
      if (placed !== 'ok') {
        this.socket?.protocolError();
        return;
      }
      this.eventsVersion++;
      this.session?.changed();
      this.follow();
    }
  }

  private async pollBoard(applyAll: boolean): Promise<void> {
    try {
      const response = await fetch('/api/fights/current', {
        headers: { accept: 'application/json' },
        cache: 'no-store',
      });
      const body: unknown = await response.json();
      if (!isBoard(body)) return;
      if (applyAll) this.applyBoard(body);
      else this.sampleOf(body.server_time);
    } catch {
      // The next poll tries again; a failed one changes nothing on screen.
    }
  }

  /** The socket, the clock and the events of the page. Returns what undoes it. */
  connect(): () => void {
    const board = this.board;
    if (board !== null) this.clock.sample(Number(board.server_time));
    const socket = createFightSocket({
      // `SocketLike` is the part it uses.
      open: () => new WebSocket(socketUrl(location)) as unknown as SocketLike,
      setTimer: (run, ms) => setTimeout(run, ms),
      clearTimer: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
      now: () => performance.now(),
      random: () => Math.random(),
      onMessage: (message) => this.onMessage(message),
      onNewConnection: () => this.clock.newConnection(),
      onChange: () => {
        const state = socket.state();
        this.degraded = state.degraded;
        this.debugSocket = { subscribes: state.subscribes, failures: state.failures };
      },
      poll: () => void this.pollBoard(true),
    });
    this.socket = socket;
    if (board !== null) this.applyBoard(board);
    socket.start();

    const check = (): void => {
      socket.check();
    };
    const onVisibility = (): void => {
      if (document.visibilityState !== 'visible') return;
      check();
      // The probe brings no time, and a paused clock would keep the offset behind (decision 2).
      const now = performance.now();
      if (now - this.lastVisibleSample < CHECK_THROTTLE_MS) return;
      this.lastVisibleSample = now;
      void this.pollBoard(false);
    };
    const onPageHide = (): void => socket.stop();
    const onPageShow = (event: PageTransitionEvent): void => {
      if (event.persisted) socket.start();
    };
    document.addEventListener('visibilitychange', onVisibility);
    window.addEventListener('online', check);
    window.addEventListener('pagehide', onPageHide);
    window.addEventListener('pageshow', onPageShow);
    const ticker = setInterval(() => this.tick(), TICK_MS);
    this.tick();

    return () => {
      clearInterval(ticker);
      document.removeEventListener('visibilitychange', onVisibility);
      window.removeEventListener('online', check);
      window.removeEventListener('pagehide', onPageHide);
      window.removeEventListener('pageshow', onPageShow);
      this.session?.stop();
      socket.stop();
      this.socket = null;
    };
  }
}
