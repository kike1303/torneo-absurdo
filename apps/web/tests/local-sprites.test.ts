import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { request } from 'node:http';
import type { AddressInfo } from 'node:net';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { createServer, type ViteDevServer } from 'vite';
import dummyA from '@torneo/renderer/placeholders/dummy_a/manifest.json';
import {
  createLocalSpritesHandler,
  localSprites,
  resolveLocalSpriteRequest,
  type LocalSpritesFs,
} from '../vite/local-sprites.js';

// 002 RF-B7, plan-b decision 14 and section "Pruebas".
const SHEET = fileURLToPath(
  new URL('../../../packages/renderer/placeholders/dummy_a/sheet-0.png', import.meta.url),
);
let root: string;
let base: string;

beforeAll(() => {
  root = realpathSync(mkdtempSync(path.join(tmpdir(), 'local-sprites-')));
  base = path.join(root, 'sprites');
  const write = (id: string, manifest: unknown) => {
    mkdirSync(path.join(base, id), { recursive: true });
    writeFileSync(path.join(base, id, 'manifest.json'), JSON.stringify(manifest));
    writeFileSync(path.join(base, id, 'sheet-0.png'), readFileSync(SHEET));
  };
  write('local_b', { ...dummyA, id: 'local_b' });
  write('local_a', { ...dummyA, id: 'local_a' });
  write('local_bad', { ...dummyA, id: 'other' }); // id mismatch: not listed
  mkdirSync(path.join(base, 'Upper'));
  writeFileSync(path.join(root, 'secret.json'), '{"secret":true}');
  symlinkSync(path.join(root, 'secret.json'), path.join(base, 'local_a', 'sheet-1.png'));
  symlinkSync(root, path.join(base, 'escape'));
});
afterAll(() => rmSync(root, { recursive: true, force: true }));

const TRAVERSAL = [
  '/__local-sprites/../secret.json',
  '/__local-sprites/local_a/../../secret.json',
  '/__local-sprites/%2e%2e/secret.json',
  '/__local-sprites/%2E%2E/secret.json',
  '/__local-sprites/local_a%2fmanifest.json',
  '/__local-sprites/local_a%5cmanifest.json',
  '/__local-sprites/local_a\\manifest.json',
  '/__local-sprites//local_a/manifest.json',
  '/__local-sprites/local_a//manifest.json',
  '/__local-sprites/Local_a/manifest.json',
  `/__local-sprites/${'a'.repeat(49)}/manifest.json`,
  '/__local-sprites/local_a/sheet-a.png',
  '/__local-sprites/local_a/sheet-1234.png',
  '/__local-sprites/local_a/manifest.json.bak',
  '/__local-sprites/local_a/.env',
  '/__local-sprites/.env',
  '/__local-sprites/escape/secret.json',
  '/__local-sprites/',
];

describe('resolveLocalSpriteRequest', () => {
  it.each(TRAVERSAL.filter((u) => !u.includes('escape')))('%s is null', (url) => {
    expect(resolveLocalSpriteRequest(url)).toBeNull();
  });

  it('accepts the index, manifests and sheets, cutting query and fragment', () => {
    expect(resolveLocalSpriteRequest('/__local-sprites/index.json')).toEqual({ kind: 'index' });
    expect(resolveLocalSpriteRequest('/__local-sprites/local_a/manifest.json?x=1#y')).toEqual({
      kind: 'file',
      id: 'local_a',
      file: 'manifest.json',
    });
    expect(resolveLocalSpriteRequest('/__local-sprites/a/sheet-12.png#z')).toEqual({
      kind: 'file',
      id: 'a',
      file: 'sheet-12.png',
    });
    expect(
      resolveLocalSpriteRequest(`/__local-sprites/${'a'.repeat(48)}/manifest.json`),
    ).not.toBeNull();
  });
});

/** The handler with a fake response, recording status, headers and body. */
async function call(
  handler: ReturnType<typeof createLocalSpritesHandler>,
  url: string,
  method = 'GET',
) {
  return new Promise<{
    status: number;
    headers: Record<string, string>;
    body: Buffer;
    next: boolean;
  }>((resolve) => {
    const headers: Record<string, string> = {};
    const res = {
      statusCode: 200,
      headersSent: false,
      setHeader: (k: string, v: string) => (headers[k.toLowerCase()] = v),
      end: (body?: Uint8Array) =>
        resolve({ status: res.statusCode, headers, body: Buffer.from(body ?? []), next: false }),
    };
    handler({ url, method } as never, res as never, () =>
      resolve({ status: 0, headers, body: Buffer.alloc(0), next: true }),
    );
  });
}

function spyFs(): LocalSpritesFs & { calls: number } {
  const fsImpl = {
    calls: 0,
    realpath: async (p: string) => (fsImpl.calls++, realpathSync(p)),
    stat: async (p: string) => (fsImpl.calls++, (await import('node:fs/promises')).stat(p)),
    readFile: async (p: string) => (fsImpl.calls++, readFileSync(p)),
    readdir: async (p: string) => (fsImpl.calls++, (await import('node:fs/promises')).readdir(p)),
  };
  return fsImpl;
}

describe('createLocalSpritesHandler', () => {
  it('passes anything outside the prefix to next()', async () => {
    const handler = createLocalSpritesHandler(base);
    expect((await call(handler, '/es/dev/arena')).next).toBe(true);
    expect((await call(handler, '/__local-sprites')).next).toBe(true);
  });

  it('answers 404 without touching the disk when the resolver says null, or for other methods', async () => {
    const fs = spyFs();
    const handler = createLocalSpritesHandler(base, fs);
    for (const url of TRAVERSAL.filter((u) => !u.includes('escape'))) {
      expect((await call(handler, url)).status, url).toBe(404);
    }
    expect((await call(handler, '/__local-sprites/local_a/manifest.json', 'POST')).status).toBe(
      404,
    );
    expect(fs.calls).toBe(0);
  });

  it('serves a manifest and a PNG with nosniff and no-store', async () => {
    const handler = createLocalSpritesHandler(base);
    const json = await call(handler, '/__local-sprites/local_a/manifest.json');
    expect(json.status).toBe(200);
    expect(json.headers['content-type']).toBe('application/json; charset=utf-8');
    expect(json.headers['cross-origin-resource-policy']).toBe('same-origin');
    expect(JSON.parse(json.body.toString('utf8')).id).toBe('local_a');
    const png = await call(handler, '/__local-sprites/local_a/sheet-0.png');
    expect(png.headers).toMatchObject({
      'cross-origin-resource-policy': 'same-origin',
      'content-type': 'image/png',
      'x-content-type-options': 'nosniff',
      'cache-control': 'no-store',
    });
    expect(png.body.equals(readFileSync(SHEET))).toBe(true);
    const head = await call(handler, '/__local-sprites/local_a/sheet-0.png', 'HEAD');
    expect([head.status, head.body.length]).toEqual([200, 0]);
  });

  it('a symbolic link that leaves the base is 404 (realpath containment)', async () => {
    const handler = createLocalSpritesHandler(base);
    expect((await call(handler, '/__local-sprites/local_a/sheet-1.png')).status).toBe(404);
    expect((await call(handler, '/__local-sprites/escape/manifest.json')).status).toBe(404);
    expect((await call(handler, '/__local-sprites/local_c/manifest.json')).status).toBe(404);
  });

  it('index.json lists only ids with a valid manifest, sorted; a missing base gives no ids', async () => {
    const index = await call(createLocalSpritesHandler(base), '/__local-sprites/index.json');
    expect(JSON.parse(index.body.toString('utf8'))).toEqual({ ids: ['local_a', 'local_b'] });
    const empty = await call(
      createLocalSpritesHandler(path.join(root, 'nope')),
      '/__local-sprites/index.json',
    );
    expect(JSON.parse(empty.body.toString('utf8'))).toEqual({ ids: [] });
  });
});

describe('localSprites() in a real Vite server', () => {
  let server: ViteDevServer;
  let port = 0;
  const fs = spyFs();

  beforeAll(async () => {
    server = await createServer({
      configFile: false,
      root,
      logLevel: 'silent',
      plugins: [localSprites({ baseDir: base, fs })],
      server: { port: 0, host: '127.0.0.1', strictPort: false, hmr: false, watch: null },
    });
    await server.listen();
    port = (server.httpServer?.address() as AddressInfo).port;
  });
  afterAll(async () => {
    await server?.close();
  });

  function raw(pathname: string, headers: Record<string, string> = {}) {
    return new Promise<{
      status: number;
      headers: Record<string, string | string[] | undefined>;
      body: Buffer;
    }>((resolve, reject) => {
      const req = request({ host: '127.0.0.1', port, path: pathname, headers }, (res) => {
        const chunks: Buffer[] = [];
        res.on('data', (c: Buffer) => chunks.push(c));
        res.on('end', () =>
          resolve({
            status: res.statusCode ?? 0,
            headers: res.headers,
            body: Buffer.concat(chunks),
          }),
        );
      });
      req.on('error', reject);
      req.end();
    });
  }

  it('is a serve-only plugin', () => {
    expect(localSprites().apply).toBe('serve');
    expect(localSprites()).not.toHaveProperty('configurePreviewServer');
  });

  it('serves the PNG with nosniff and never leaks a file through traversal', async () => {
    const png = await raw('/__local-sprites/local_a/sheet-0.png');
    expect(png.status).toBe(200);
    expect(png.headers['x-content-type-options']).toBe('nosniff');
    for (const url of TRAVERSAL.filter((u) => !u.includes('\\'))) {
      const res = await raw(url);
      expect(res.status, url).toBe(404);
      expect(res.body.toString('utf8'), url).not.toContain('secret');
    }
  });

  it('a foreign Origin gets no Access-Control-Allow-Origin', async () => {
    const res = await raw('/__local-sprites/index.json', { Origin: 'http://evil.example' });
    expect(res.status).toBe(200);
    expect(res.headers['access-control-allow-origin']).toBeUndefined();
  });

  it('a foreign Host gets 403 from Vite before the handler touches the disk', async () => {
    const spy = vi.spyOn(fs, 'realpath');
    const readdir = vi.spyOn(fs, 'readdir');
    const res = await raw('/__local-sprites/index.json', { Host: 'evil.example' });
    expect(res.status).toBe(403);
    expect(spy).not.toHaveBeenCalled();
    expect(readdir).not.toHaveBeenCalled();
  });
});
