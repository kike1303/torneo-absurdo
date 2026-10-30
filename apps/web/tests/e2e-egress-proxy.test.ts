import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import http from 'node:http';
import net from 'node:net';
import type { AddressInfo } from 'node:net';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { absoluteTarget, startEgressProxy } from '../e2e/support/egress-proxy.mjs';

// 008 plan-d decision 10 (change 1 of architect) and condition 1 of devops.
const dir = mkdtempSync(path.join(tmpdir(), 'egress-'));
const logFile = path.join(dir, 'logs', 'egress.log');
let proxy: http.Server;
let port = 0;

// Every socket connect of the process: the proxy must never open one towards the outside.
const connects: string[] = [];
const original = net.Socket.prototype.connect;

beforeAll(async () => {
  net.Socket.prototype.connect = function (this: net.Socket, ...args: unknown[]) {
    // `net.connect()` hands `socket.connect()` its normalized `[options, callback]` array.
    const [first, second] = Array.isArray(args[0]) ? (args[0] as unknown[]) : args;
    const options =
      typeof first === 'object' && first !== null
        ? (first as { port?: unknown; host?: unknown; path?: unknown })
        : { port: first, host: typeof second === 'string' ? second : undefined };
    connects.push(`${String(options.host ?? 'localhost')}:${String(options.port ?? options.path)}`);
    return (original as (...a: unknown[]) => net.Socket).apply(this, args);
  } as typeof original;
  proxy = await startEgressProxy({ port: 0, logFile });
  port = (proxy.address() as AddressInfo).port;
});

afterAll(() => {
  net.Socket.prototype.connect = original;
  proxy.closeAllConnections();
  proxy.close();
  rmSync(dir, { recursive: true, force: true });
});

const lines = () => readFileSync(logFile, 'utf8').split('\n').filter(Boolean);

function send(text: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const socket = net.connect(port, '127.0.0.1', () => socket.write(text));
    let data = '';
    socket.on('data', (c) => (data += c.toString()));
    socket.on('close', () => resolve(data));
    socket.on('error', reject);
  });
}

describe('egress proxy', () => {
  it('starts with an empty log', () => {
    expect(existsSync(logFile)).toBe(true);
  });

  it('empties a log left over from an earlier run when it starts', async () => {
    const stale = path.join(dir, 'stale', 'egress.log');
    mkdirSync(path.dirname(stale), { recursive: true });
    writeFileSync(stale, 'accounts.google.com:443\n');
    const other = await startEgressProxy({ port: 0, logFile: stale });
    try {
      expect(readFileSync(stale, 'utf8')).toBe('');
    } finally {
      other.close();
    }
  });

  it('listens on 127.0.0.1 only', () => {
    expect((proxy.address() as AddressInfo).address).toBe('127.0.0.1');
  });

  it('records a CONNECT and cuts it with 403', async () => {
    const before = lines().length;
    const answer = await send(
      'CONNECT accounts.google.com:443 HTTP/1.1\r\nhost: accounts.google.com:443\r\n\r\n',
    );
    expect(answer).toMatch(/^HTTP\/1.1 403/);
    expect(lines().slice(before)).toEqual(['accounts.google.com:443']);
  });

  it('records an absolute-form request and cuts it', async () => {
    const before = lines().length;
    const answer = await send(
      'GET http://Example.com/x?y HTTP/1.1\r\nhost: example.com\r\nconnection: close\r\n\r\n',
    );
    expect(answer).toMatch(/^HTTP\/1.1 403/);
    expect(lines().slice(before)).toEqual(['example.com:80']);
  });

  it('answers /healthz without logging it', async () => {
    const before = lines().length;
    const answer = await send('GET /healthz HTTP/1.1\r\nhost: x\r\nconnection: close\r\n\r\n');
    expect(answer).toMatch(/^HTTP\/1.1 200/);
    expect(lines().length).toBe(before);
  });

  it('never opens a connection of its own', () => {
    expect(connects.filter((c) => c !== `127.0.0.1:${port}`)).toEqual([]);
    expect(connects.length).toBeGreaterThan(0);
  });

  it.each([
    ['https://discord.com/api/webhooks/x', 'discord.com:443'],
    ['http://user:pw@example.com:8080/', 'example.com:8080'],
    ['HTTP://EXAMPLE.COM', 'example.com:80'],
    ['/relative', null],
    ['', null],
  ])('absoluteTarget(%j) → %j', (target, expected) => {
    expect(absoluteTarget(target)).toBe(expected);
  });
});
