#!/usr/bin/env node
// The edge proxy of the e2e (008 RF-D7, plan-d decision 10): it plays Traefik in front of the
// server and the web, on loopback only. Node built-ins only, and it logs nothing.
//
// - `^/api(?:/|$)` and `^/ws$` go to the server, everything else to the web: the same anchored
//   expressions as the Vite proxy (`/apix` stays in the web).
// - The raw path (`req.url`) is forwarded untouched: the open-redirect tests send `//evil` and
//   `%0d%0a`.
// - The client's `X-Forwarded-For` is dropped, like Traefik does for an untrusted client, and a
//   new one is set: from `x-e2e-client: <id>` (removed before forwarding) a `/64` of
//   `2001:db8::/32` per test, so each test has its own `IpKey` (the server groups IPv6 by `/64`);
//   without it, the peer address.
// - `/__e2e/redirect-out` exists only here: a 303 to https://example.com/ for egress.spec.ts.
import { createHash } from 'node:crypto';
import http from 'node:http';
import net from 'node:net';
import { fileURLToPath } from 'node:url';

/** Never read from the environment: the e2e never listens outside loopback (change 5 of devops). */
const LISTEN_HOST = '127.0.0.1';
const UPSTREAM_HOST = '127.0.0.1';

const TO_SERVER = /^\/(?:api(?:\/|$)|ws$)/;
const CLIENT_ID = /^[a-z0-9-]{1,64}$/;
export const REDIRECT_OUT_PATH = '/__e2e/redirect-out';
export const REDIRECT_OUT_TARGET = 'https://example.com/';

/** The path part of a raw request target (before `?`). @param {string} rawUrl */
function pathOf(rawUrl) {
  const at = rawUrl.indexOf('?');
  return at === -1 ? rawUrl : rawUrl.slice(0, at);
}

/** Whether a raw request target goes to the server. @param {string} rawUrl */
export function routesToServer(rawUrl) {
  return TO_SERVER.test(pathOf(rawUrl));
}

/**
 * The documentation address of a test client: `2001:db8:<h1>:<h2>::1`, from SHA-256 of `id`.
 * @param {string} id
 */
export function clientAddress(id) {
  const digest = createHash('sha256').update(id).digest();
  const h1 = digest.readUInt16BE(0).toString(16);
  const h2 = digest.readUInt16BE(2).toString(16);
  return `2001:db8:${h1}:${h2}::1`;
}

/**
 * The headers to forward, as a flat `[name, value, …]` list (duplicates kept), or `null` if
 * `x-e2e-client` is invalid or repeated.
 * @param {string[]} rawHeaders
 * @param {string | undefined} peer
 * @returns {string[] | null}
 */
export function forwardHeaders(rawHeaders, peer) {
  /** @type {string[]} */
  const out = [];
  /** @type {string[]} */
  const clients = [];
  for (let i = 0; i < rawHeaders.length; i += 2) {
    const name = rawHeaders[i] ?? '';
    const value = rawHeaders[i + 1] ?? '';
    const lower = name.toLowerCase();
    if (lower === 'x-e2e-client') {
      clients.push(value);
      continue;
    }
    if (lower === 'x-forwarded-for') continue;
    out.push(name, value);
  }
  if (clients.length > 1) return null;
  if (clients.length === 1) {
    const [id] = clients;
    if (id === undefined || !CLIENT_ID.test(id)) return null;
    out.push('X-Forwarded-For', clientAddress(id));
  } else {
    out.push('X-Forwarded-For', normalizePeer(peer));
  }
  return out;
}

/** `::ffff:127.0.0.1` → `127.0.0.1`. @param {string | undefined} peer */
function normalizePeer(peer) {
  const value = peer ?? '';
  return value.startsWith('::ffff:') ? value.slice(7) : value;
}

/**
 * Starts the edge on 127.0.0.1.
 * @param {{ port: number, serverPort: number, webPort: number }} options
 * @returns {Promise<http.Server>}
 */
export function startEdge({ port, serverPort, webPort }) {
  /** @param {string} rawUrl */
  const upstreamPort = (rawUrl) => (routesToServer(rawUrl) ? serverPort : webPort);

  const server = http.createServer((req, res) => {
    const rawUrl = req.url ?? '/';
    if (pathOf(rawUrl) === REDIRECT_OUT_PATH) {
      res.writeHead(303, { location: REDIRECT_OUT_TARGET, 'content-length': '0' });
      res.end();
      return;
    }
    const headers = forwardHeaders(req.rawHeaders, req.socket.remoteAddress);
    if (!headers) {
      res.writeHead(400, { 'content-length': '0' });
      res.end();
      return;
    }
    const upstream = http.request(
      {
        host: UPSTREAM_HOST,
        port: upstreamPort(rawUrl),
        method: req.method,
        path: rawUrl,
        headers,
        setHost: false,
      },
      (answer) => {
        res.writeHead(answer.statusCode ?? 502, answer.statusMessage, answer.rawHeaders);
        answer.pipe(res);
      },
    );
    upstream.on('error', () => {
      if (!res.headersSent) {
        res.writeHead(502, { 'content-length': '0' });
        res.end();
      } else {
        res.destroy();
      }
    });
    req.pipe(upstream);
  });

  server.on('upgrade', (req, socket, head) => {
    const rawUrl = req.url ?? '/';
    const headers = forwardHeaders(req.rawHeaders, req.socket.remoteAddress);
    if (!headers) {
      socket.end('HTTP/1.1 400 Bad Request\r\ncontent-length: 0\r\n\r\n');
      return;
    }
    const upstream = net.connect(upstreamPort(rawUrl), UPSTREAM_HOST, () => {
      let text = `${req.method} ${rawUrl} HTTP/1.1\r\n`;
      for (let i = 0; i < headers.length; i += 2) text += `${headers[i]}: ${headers[i + 1]}\r\n`;
      upstream.write(`${text}\r\n`);
      if (head.length > 0) upstream.write(head);
      upstream.pipe(socket);
      socket.pipe(upstream);
    });
    const close = () => {
      upstream.destroy();
      socket.destroy();
    };
    upstream.on('error', () => {
      if (socket.writable) socket.end('HTTP/1.1 502 Bad Gateway\r\ncontent-length: 0\r\n\r\n');
      close();
    });
    socket.on('error', close);
  });

  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(port, LISTEN_HOST, () => resolve(server));
  });
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const [port = 0, serverPort = 0, webPort = 0] = process.argv.slice(2).map(Number);
  if (![port, serverPort, webPort].every((p) => Number.isInteger(p) && p > 0)) {
    process.stderr.write('usage: edge.mjs <port> <server port> <web port>\n');
    process.exit(2);
  }
  const server = await startEdge({ port, serverPort, webPort });
  const stop = () => {
    server.close(() => process.exit(0));
    server.closeAllConnections();
  };
  process.on('SIGTERM', stop);
  process.on('SIGINT', stop);
}
