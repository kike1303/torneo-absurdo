import type { ClientMessage, ServerMessage, WsErrorCode } from '@torneo/protocol';

// Development echo over /ws for /dev/status (RF-B7). The URL is relative (WHATWG allows it), so
// no absolute URL is built here (plan-a decision 20).

export type EchoStatus = 'connecting' | 'connected' | 'sent' | 'received' | 'closed';

/** Paraglide keys of the close reasons: one per `WsErrorCode`, plus `unknown`. */
export type CloseReasonKey =
  | 'ws_close_reason_origin_forbidden'
  | 'ws_close_reason_capacity'
  | 'ws_close_reason_invalid_message'
  | 'ws_close_reason_echo_disabled'
  | 'ws_close_reason_rate_limited'
  | 'ws_close_reason_pong_timeout'
  | 'ws_close_reason_idle_timeout'
  | 'ws_close_reason_unknown';

/** Maps a close reason to its key. Fails to compile if a `WsErrorCode` has no case. */
export function closeReasonKey(reason: string): CloseReasonKey {
  const code = reason as WsErrorCode;
  switch (code) {
    case 'ws.origin_forbidden':
      return 'ws_close_reason_origin_forbidden';
    case 'ws.capacity':
      return 'ws_close_reason_capacity';
    case 'ws.invalid_message':
      return 'ws_close_reason_invalid_message';
    case 'ws.echo_disabled':
      return 'ws_close_reason_echo_disabled';
    case 'ws.rate_limited':
      return 'ws_close_reason_rate_limited';
    case 'ws.pong_timeout':
      return 'ws_close_reason_pong_timeout';
    case 'ws.idle_timeout':
      return 'ws_close_reason_idle_timeout';
    default: {
      // `code` is `never` here only while every WsErrorCode has a case above. At runtime an
      // empty or unknown reason (1003, 1009, 1012 have none) lands here.
      const unhandled: never = code;
      void unhandled;
      return 'ws_close_reason_unknown';
    }
  }
}

/** Type guard for messages from the server. */
export function parseServerMessage(data: unknown): ServerMessage | null {
  if (typeof data !== 'string') return null;
  let value: unknown;
  try {
    value = JSON.parse(data);
  } catch {
    return null;
  }
  if (typeof value !== 'object' || value === null) return null;
  const message = value as Record<string, unknown>;
  if (message.type === 'echo' && typeof message.payload === 'string') {
    return { type: 'echo', payload: message.payload };
  }
  return null;
}

export interface EchoEvents {
  onStatus(status: EchoStatus): void;
  onMessage(payload: string): void;
  onClose(code: number, reason: CloseReasonKey): void;
}

export interface EchoConnection {
  /** Sends an echo; `false` if the socket is not open. */
  send(payload: string): boolean;
  close(): void;
}

/** The subset of the WebSocket API this module uses (so tests can pass a fake). */
export interface WebSocketLike {
  readonly readyState: number;
  onopen: ((event: Event) => void) | null;
  onmessage: ((event: MessageEvent) => void) | null;
  onclose: ((event: CloseEvent) => void) | null;
  send(data: string): void;
  close(): void;
}

export type WebSocketConstructor = new (url: string) => WebSocketLike;

const OPEN = 1;

export function openEcho(
  events: EchoEvents,
  WebSocketCtor: WebSocketConstructor = WebSocket,
): EchoConnection {
  events.onStatus('connecting');
  const socket = new WebSocketCtor('/ws');
  socket.onopen = () => events.onStatus('connected');
  socket.onmessage = (event) => {
    const message = parseServerMessage(event.data);
    if (message === null) return;
    events.onMessage(message.payload);
    events.onStatus('received');
  };
  socket.onclose = (event) => {
    events.onClose(event.code, closeReasonKey(event.reason));
    events.onStatus('closed');
  };
  return {
    send(payload) {
      if (socket.readyState !== OPEN) return false;
      const message: ClientMessage = { type: 'echo', payload };
      socket.send(JSON.stringify(message));
      events.onStatus('sent');
      return true;
    },
    close() {
      socket.close();
    },
  };
}
