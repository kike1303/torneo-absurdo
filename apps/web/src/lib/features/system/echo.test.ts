import type { WsErrorCode } from '@torneo/protocol';
import { describe, expect, it, vi } from 'vitest';
import {
  type CloseReasonKey,
  type EchoEvents,
  type WebSocketLike,
  closeReasonKey,
  openEcho,
  parseServerMessage,
} from './echo.js';

class FakeSocket implements WebSocketLike {
  static last: FakeSocket | null = null;
  readyState = 0;
  sent: string[] = [];
  url: string;
  onopen: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onclose: ((event: CloseEvent) => void) | null = null;

  constructor(url: string) {
    this.url = url;
    FakeSocket.last = this;
  }
  send(data: string) {
    this.sent.push(data);
  }
  close() {
    this.readyState = 3;
  }
  open() {
    this.readyState = 1;
    this.onopen?.({} as Event);
  }
  receive(data: unknown) {
    this.onmessage?.({ data } as MessageEvent);
  }
  closedByServer(code: number, reason: string) {
    this.readyState = 3;
    this.onclose?.({ code, reason } as CloseEvent);
  }
}

function events() {
  return {
    onStatus: vi.fn(),
    onMessage: vi.fn(),
    onClose: vi.fn(),
  } satisfies EchoEvents;
}

describe('openEcho (RF-B7)', () => {
  it('opens /ws with a relative URL and does a round trip', () => {
    const ev = events();
    const connection = openEcho(ev, FakeSocket);
    const socket = FakeSocket.last!;
    expect(socket.url).toBe('/ws');
    expect(ev.onStatus).toHaveBeenLastCalledWith('connecting');
    expect(connection.send('hola')).toBe(false);

    socket.open();
    expect(ev.onStatus).toHaveBeenLastCalledWith('connected');
    expect(connection.send('hola')).toBe(true);
    expect(socket.sent).toEqual(['{"type":"echo","payload":"hola"}']);
    expect(ev.onStatus).toHaveBeenLastCalledWith('sent');

    socket.receive('{"type":"echo","payload":"hola"}');
    expect(ev.onMessage).toHaveBeenCalledWith('hola');
    expect(ev.onStatus).toHaveBeenLastCalledWith('received');
  });

  it('ignores messages it does not understand', () => {
    const ev = events();
    openEcho(ev, FakeSocket);
    const socket = FakeSocket.last!;
    socket.open();
    for (const data of ['nope', '{"type":"other"}', '{"type":"echo","payload":1}', 42]) {
      socket.receive(data);
    }
    expect(ev.onMessage).not.toHaveBeenCalled();
  });

  it('reports the close with the translated reason key', () => {
    const ev = events();
    openEcho(ev, FakeSocket);
    FakeSocket.last!.closedByServer(1008, 'ws.rate_limited');
    expect(ev.onClose).toHaveBeenCalledWith(1008, 'ws_close_reason_rate_limited');
    expect(ev.onStatus).toHaveBeenLastCalledWith('closed');
  });
});

describe('closeReasonKey', () => {
  const cases: [WsErrorCode, CloseReasonKey][] = [
    ['ws.origin_forbidden', 'ws_close_reason_origin_forbidden'],
    ['ws.capacity', 'ws_close_reason_capacity'],
    ['ws.invalid_message', 'ws_close_reason_invalid_message'],
    ['ws.echo_disabled', 'ws_close_reason_echo_disabled'],
    ['ws.rate_limited', 'ws_close_reason_rate_limited'],
    ['ws.pong_timeout', 'ws_close_reason_pong_timeout'],
    ['ws.idle_timeout', 'ws_close_reason_idle_timeout'],
    ['ws.ip_capacity', 'ws_close_reason_ip_capacity'],
  ];
  it.each(cases)('%s → %s', (code, key) => {
    expect(closeReasonKey(code)).toBe(key);
  });
  it.each(['', 'ws.unknown', 'http.not_found', 'Going away'])('%j → unknown', (reason) => {
    expect(closeReasonKey(reason)).toBe('ws_close_reason_unknown');
  });
});

describe('parseServerMessage', () => {
  it('accepts an echo and rejects the rest', () => {
    expect(parseServerMessage('{"type":"echo","payload":"x"}')).toEqual({
      type: 'echo',
      payload: 'x',
    });
    expect(parseServerMessage('{"type":"echo"}')).toBeNull();
    expect(parseServerMessage('[]')).toBeNull();
  });
});
