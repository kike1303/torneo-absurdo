import http from 'node:http';
import net from 'node:net';
import type { AddressInfo } from 'node:net';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import {
  REDIRECT_OUT_PATH,
  clientAddress,
  forwardHeaders,
  routesToServer,
  startEdge,
} from '../e2e/support/edge.mjs';

// 008 plan-d decision 10: the edge proxy of the e2e.

interface Echo {
  target: 'server' | 'web';
  url: string;
  headers: Record<string, string | string[] | undefined>;
}

function echoServer(target: Echo['target']): http.Server {
  const server = http.createServer((req, res) => {
    const body: Echo = { target, url: req.url ?? '', headers: req.headers };
    res.writeHead(200, [
      'content-type',
      'application/json',
      'set-cookie',
      'a=1',
      'set-cookie',
      'b=2',
    ]);
    res.end(JSON.stringify(body));
  });
  server.on('upgrade', (req, socket) => {
    socket.write(
      'HTTP/1.1 101 Switching Protocols\r\nupgrade: websocket\r\nconnection: Upgrade\r\n\r\n',
    );
    socket.write(JSON.stringify({ target, url: req.url, headers: req.headers }));
    socket.end();
  });
  return server;
}

const listen = (server: http.Server) =>
  new Promise<number>((resolve) =>
    server.listen(0, '127.0.0.1', () => resolve((server.address() as AddressInfo).port)),
  );

let edge: http.Server;
let upstreams: http.Server[] = [];
let port = 0;

/** A raw request, so the path reaches the edge exactly as written. */
function raw(
  path: string,
  headers: Record<string, string> = {},
): Promise<{
  status: number;
  headers: http.IncomingHttpHeaders;
  rawHeaders: string[];
  body: string;
}> {
  return new Promise((resolve, reject) => {
    const socket = net.connect(port, '127.0.0.1', () => {
      let text = `GET ${path} HTTP/1.1\r\nhost: localhost:${port}\r\nconnection: close\r\n`;
      for (const [k, v] of Object.entries(headers)) text += `${k}: ${v}\r\n`;
      socket.write(`${text}\r\n`);
    });
    let data = '';
    socket.on('data', (chunk) => (data += chunk.toString()));
    socket.on('error', reject);
    socket.on('end', () => {
      const [head = '', ...rest] = data.split('\r\n\r\n');
      const [statusLine = '', ...lines] = head.split('\r\n');
      const rawHeaders: string[] = [];
      const parsed: http.IncomingHttpHeaders = {};
      for (const line of lines) {
        const at = line.indexOf(':');
        const name = line.slice(0, at).toLowerCase();
        const value = line.slice(at + 1).trim();
        rawHeaders.push(name, value);
        parsed[name] = value;
      }
      let body = rest.join('\r\n\r\n');
      if (/transfer-encoding: chunked/i.test(head)) {
        body = body.replace(/^[0-9a-f]+\r\n/, '').replace(/\r\n0\r\n\r\n$/, '');
      }
      resolve({ status: Number(statusLine.split(' ')[1]), headers: parsed, rawHeaders, body });
    });
  });
}

const echo = async (path: string, headers: Record<string, string> = {}) =>
  JSON.parse((await raw(path, headers)).body) as Echo;

beforeAll(async () => {
  const server = echoServer('server');
  const web = echoServer('web');
  upstreams = [server, web];
  const [serverPort, webPort] = await Promise.all([listen(server), listen(web)]);
  edge = await startEdge({ port: 0, serverPort, webPort });
  port = (edge.address() as AddressInfo).port;
});

afterAll(() => {
  for (const s of [edge, ...upstreams]) {
    s.closeAllConnections();
    s.close();
  }
});

describe('routing', () => {
  it.each([
    ['/api', 'server'],
    ['/api/', 'server'],
    ['/api/me?x=1', 'server'],
    ['/api/auth/dev/start', 'server'],
    ['/ws', 'server'],
    ['/ws?x', 'server'],
    ['/apix', 'web'],
    ['/wsx', 'web'],
    ['/ws/', 'web'],
    ['/es/api', 'web'],
    ['/', 'web'],
    ['/es/login?error=x', 'web'],
  ])('%s → %s', async (path, target) => {
    expect(routesToServer(path)).toBe(target === 'server');
    expect((await echo(path)).target).toBe(target);
  });

  it.each(['//evil.example/', '/\\evil.example', '/es/%0d%0aSet-Cookie:x', '/a/../b', '/%2e%2e/x'])(
    'forwards the raw path %s untouched',
    async (path) => {
      expect((await echo(path)).url).toBe(path);
    },
  );

  it('keeps Host, Origin and Sec-Fetch-*, and every Set-Cookie of the answer', async () => {
    const answer = await raw('/es/', {
      origin: 'http://localhost:4173',
      'sec-fetch-site': 'same-origin',
    });
    const body = JSON.parse(answer.body) as Echo;
    expect(body.headers.host).toBe(`localhost:${port}`);
    expect(body.headers.origin).toBe('http://localhost:4173');
    expect(body.headers['sec-fetch-site']).toBe('same-origin');
    const cookies = answer.rawHeaders.filter((_, i, a) => a[i - 1] === 'set-cookie');
    expect(cookies).toEqual(['a=1', 'b=2']);
  });

  it(`${REDIRECT_OUT_PATH} answers 303 to example.com, without an upstream`, async () => {
    const answer = await raw(REDIRECT_OUT_PATH);
    expect(answer.status).toBe(303);
    expect(answer.headers.location).toBe('https://example.com/');
  });
});

describe('X-Forwarded-For', () => {
  it('drops the client value and puts the peer', async () => {
    const body = await echo('/api/me', { 'x-forwarded-for': '198.51.100.7, 10.0.0.1' });
    expect(body.headers['x-forwarded-for']).toBe('127.0.0.1');
  });

  it('translates x-e2e-client into a 2001:db8 /64 and removes it', async () => {
    const body = await echo('/api/me', {
      'x-e2e-client': 'e2e-abc',
      'x-forwarded-for': '198.51.100.7',
    });
    expect(body.headers['x-forwarded-for']).toBe(clientAddress('e2e-abc'));
    expect(body.headers['x-e2e-client']).toBeUndefined();
    expect(clientAddress('e2e-abc')).toMatch(/^2001:db8:[0-9a-f]{1,4}:[0-9a-f]{1,4}::1$/);
  });

  it('two ids get two different /64', () => {
    const a = clientAddress('e2e-a').split(':').slice(0, 4).join(':');
    const b = clientAddress('e2e-b').split(':').slice(0, 4).join(':');
    expect(a).not.toBe(b);
    expect(clientAddress('e2e-a')).toBe(clientAddress('e2e-a'));
  });

  it.each(['E2E', 'a b', 'x'.repeat(65), '', 'a_b', '../x'])(
    'an invalid x-e2e-client %j gives 400',
    async (id) => {
      expect((await raw('/api/me', { 'x-e2e-client': id })).status).toBe(400);
    },
  );

  it('a repeated x-e2e-client gives 400', () => {
    expect(forwardHeaders(['x-e2e-client', 'a', 'X-E2E-Client', 'b'], '127.0.0.1')).toBeNull();
  });
});

describe('upgrade of /ws', () => {
  function upgrade(path: string, headers: Record<string, string>): Promise<string> {
    return new Promise((resolve, reject) => {
      const socket = net.connect(port, '127.0.0.1', () => {
        let text = `GET ${path} HTTP/1.1\r\nhost: localhost\r\nconnection: Upgrade\r\nupgrade: websocket\r\n`;
        for (const [k, v] of Object.entries(headers)) text += `${k}: ${v}\r\n`;
        socket.write(`${text}\r\n`);
      });
      let data = '';
      socket.on('data', (c) => (data += c.toString()));
      socket.on('end', () => resolve(data));
      socket.on('error', reject);
    });
  }

  it('reaches the server with the same header rules as HTTP', async () => {
    const data = await upgrade('/ws', { 'x-e2e-client': 'e2e-ws', 'x-forwarded-for': '1.2.3.4' });
    expect(data).toMatch(/^HTTP\/1.1 101/);
    const body = JSON.parse(data.slice(data.indexOf('\r\n\r\n') + 4)) as Echo;
    expect(body.target).toBe('server');
    expect(body.headers['x-forwarded-for']).toBe(clientAddress('e2e-ws'));
    expect(body.headers['x-e2e-client']).toBeUndefined();
  });

  it('an invalid x-e2e-client gives 400', async () => {
    expect(await upgrade('/ws', { 'x-e2e-client': 'BAD' })).toMatch(/^HTTP\/1.1 400/);
  });
});

describe('a dead upstream', () => {
  it('gives 502', async () => {
    // Port 1 on loopback refuses the connection.
    const lonely = await startEdge({ port: 0, serverPort: 1, webPort: 1 });
    port = (lonely.address() as AddressInfo).port;
    try {
      expect((await raw('/api/me')).status).toBe(502);
      expect((await raw('/es/')).status).toBe(502);
    } finally {
      lonely.close();
      port = (edge.address() as AddressInfo).port;
    }
  });
});

describe('listen address', () => {
  it('is 127.0.0.1, never 0.0.0.0', () => {
    expect((edge.address() as AddressInfo).address).toBe('127.0.0.1');
  });
});
