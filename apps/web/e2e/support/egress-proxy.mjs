#!/usr/bin/env node
// The network guard of the e2e (008 RF-D7, plan-d decision 10, change 1 of architect). Every
// browser of the e2e uses it as its HTTP proxy, with loopback in `bypass`, so it sees every
// request that would leave loopback, the destination of a 303 included (which `page.route`
// never sees). For each `CONNECT host:port`, or request in absolute form, it appends
// `host:port` to the log and cuts it: it never opens a connection to the outside.
//
// `GET /healthz` in origin form answers 200 without logging (the wait of Playwright's
// `webServer`). Node built-ins only.
import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/** Never read from the environment (change 5 of devops). */
const LISTEN_HOST = '127.0.0.1';

/** `host:port` of an absolute-form request target, or `null`. @param {string} target */
export function absoluteTarget(target) {
  const match = /^([a-z][a-z0-9+.-]*):\/\/([^/?#]*)/i.exec(target);
  if (!match) return null;
  const scheme = (match[1] ?? '').toLowerCase();
  const authority = (match[2] ?? '').replace(/^.*@/, '').toLowerCase();
  if (authority === '') return null;
  if (/:\d+$/.test(authority)) return authority;
  return `${authority}:${scheme === 'https' ? 443 : 80}`;
}

/**
 * Starts the proxy on 127.0.0.1. The log starts empty, so a proxy that never started leaves no
 * file and the teardown fails (condition 1 of devops).
 * @param {{ port: number, logFile: string }} options
 * @returns {Promise<http.Server>}
 */
export function startEgressProxy({ port, logFile }) {
  mkdirSync(path.dirname(logFile), { recursive: true });
  writeFileSync(logFile, '');
  /** @param {string} target */
  const record = (target) => appendFileSync(logFile, `${target}\n`);

  const server = http.createServer((req, res) => {
    const target = req.url ?? '';
    const absolute = absoluteTarget(target);
    if (absolute) {
      record(absolute);
      res.writeHead(403, { connection: 'close', 'content-length': '0' });
      res.end();
      req.socket.destroy();
      return;
    }
    if (req.method === 'GET' && target === '/healthz') {
      res.writeHead(200, { 'content-type': 'text/plain', 'content-length': '2' });
      res.end('ok');
      return;
    }
    res.writeHead(404, { 'content-length': '0' });
    res.end();
  });

  server.on('connect', (req, socket) => {
    record((req.url ?? '').toLowerCase());
    socket.on('error', () => undefined);
    socket.end('HTTP/1.1 403 Forbidden\r\nconnection: close\r\ncontent-length: 0\r\n\r\n');
  });

  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(port, LISTEN_HOST, () => resolve(server));
  });
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const port = Number(process.argv[2]);
  const logFile = process.argv[3];
  if (!Number.isInteger(port) || port <= 0 || !logFile) {
    process.stderr.write('usage: egress-proxy.mjs <port> <log file>\n');
    process.exit(2);
  }
  const server = await startEgressProxy({ port, logFile: path.resolve(logFile) });
  const stop = () => {
    server.close(() => process.exit(0));
    server.closeAllConnections();
  };
  process.on('SIGTERM', stop);
  process.on('SIGINT', stop);
}
