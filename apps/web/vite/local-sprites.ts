// Dev-only plugin (002 RF-B7, plan-b decision 14): serves the fighters converted into
// content/_local/sprites/ at /__local-sprites/, so /dev/arena can play them with `just dev`.
// `apply: 'serve'` and no `configurePreviewServer`: it exists neither in `vite build` nor in
// `vite preview`. Vite registers its host validation and CORS middlewares before the
// `configureServer` hooks, so a foreign `Host` gets 403 before reaching this handler.
import { promises as fsp } from 'node:fs';
import type { IncomingMessage, ServerResponse } from 'node:http';
import path from 'node:path';
import type { Plugin } from 'vite';
import { validateManifest } from '@torneo/renderer/manifest';

export const LOCAL_SPRITES_PREFIX = '/__local-sprites/';
const PATTERN =
  /^\/__local-sprites\/(?:index\.json|([a-z][a-z0-9_]{0,47})\/(manifest\.json|sheet-[0-9]{1,3}\.png))$/;
const ID = /^[a-z][a-z0-9_]{0,47}$/;
const MAX_FILE = 32 * 1024 * 1024;
const MAX_MANIFEST = 1024 * 1024;
const MAX_IDS = 100;

export type LocalSpriteRequest =
  | { readonly kind: 'index' }
  | { readonly kind: 'file'; readonly id: string; readonly file: string }
  | null;

/** Pure: never touches the disk. Anything but an exact, unencoded name is null. */
export function resolveLocalSpriteRequest(rawUrl: string): LocalSpriteRequest {
  const cut = rawUrl.search(/[?#]/);
  const pathname = cut === -1 ? rawUrl : rawUrl.slice(0, cut);
  // No valid id nor file needs encoding: `%2e%2e`, `%2f` and `%5c` die here, undecoded.
  if (pathname.includes('%') || pathname.includes('\\')) return null;
  const match = PATTERN.exec(pathname);
  if (match === null) return null;
  const [, id, file] = match;
  if (id === undefined || file === undefined) return { kind: 'index' };
  return { kind: 'file', id, file };
}

/** The file system calls of the handler (injected so tests can spy on them). */
export interface LocalSpritesFs {
  realpath(p: string): Promise<string>;
  stat(p: string): Promise<{ isFile(): boolean; isDirectory(): boolean; size: number }>;
  readFile(p: string): Promise<Uint8Array>;
  readdir(p: string): Promise<string[]>;
}

const nodeFs: LocalSpritesFs = {
  realpath: (p) => fsp.realpath(p),
  stat: (p) => fsp.stat(p),
  readFile: (p) => fsp.readFile(p),
  readdir: (p) => fsp.readdir(p),
};

function send(
  res: ServerResponse,
  status: number,
  type: string | null,
  body: Uint8Array,
  head: boolean,
) {
  res.statusCode = status;
  res.setHeader('X-Content-Type-Options', 'nosniff');
  res.setHeader('Cache-Control', 'no-store');
  // Same origin only, also for <img> from another site (ADR-0012).
  res.setHeader('Cross-Origin-Resource-Policy', 'same-origin');
  if (type !== null) res.setHeader('Content-Type', type);
  res.setHeader('Content-Length', String(body.length));
  res.end(head ? undefined : body);
}

const notFound = (res: ServerResponse, head: boolean) =>
  send(res, 404, null, new Uint8Array(0), head);

/** A regular file of at most `max` bytes whose real path stays inside the real base, or null. */
async function containedFile(
  fs: LocalSpritesFs,
  baseDir: string,
  file: string,
  max: number,
): Promise<string | null> {
  const realBase = await fs.realpath(baseDir).catch(() => null);
  if (realBase === null) return null;
  const real = await fs.realpath(file).catch(() => null);
  if (real === null || !real.startsWith(realBase + path.sep)) return null;
  const stat = await fs.stat(real).catch(() => null);
  if (stat === null || !stat.isFile() || stat.size > max) return null;
  return real;
}

async function listIds(fs: LocalSpritesFs, baseDir: string): Promise<string[]> {
  const names = await fs.readdir(baseDir).catch(() => [] as string[]);
  const ids: string[] = [];
  for (const id of names.filter((n) => ID.test(n)).sort()) {
    if (ids.length >= MAX_IDS) break;
    const file = await containedFile(
      fs,
      baseDir,
      path.join(baseDir, id, 'manifest.json'),
      MAX_MANIFEST,
    );
    if (file === null) continue;
    const json = await fs
      .readFile(file)
      .then((bytes) => JSON.parse(Buffer.from(bytes).toString('utf8')) as unknown)
      .catch(() => null);
    if (json !== null && validateManifest(json, { expectedId: id }).ok) ids.push(id);
  }
  return ids;
}

type Next = (error?: unknown) => void;

export function createLocalSpritesHandler(baseDir: string, fs: LocalSpritesFs = nodeFs) {
  return (req: IncomingMessage, res: ServerResponse, next: Next): void => {
    const url = req.url ?? '';
    if (!url.startsWith(LOCAL_SPRITES_PREFIX)) {
      next();
      return;
    }
    const head = req.method === 'HEAD';
    const request = resolveLocalSpriteRequest(url);
    if (request === null || (req.method !== 'GET' && !head)) {
      notFound(res, head);
      return;
    }
    void (async () => {
      if (request.kind === 'index') {
        const body = Buffer.from(JSON.stringify({ ids: await listIds(fs, baseDir) }));
        send(res, 200, 'application/json; charset=utf-8', body, head);
        return;
      }
      const file = await containedFile(
        fs,
        baseDir,
        path.join(baseDir, request.id, request.file),
        MAX_FILE,
      );
      const bytes = file === null ? null : await fs.readFile(file).catch(() => null);
      if (bytes === null) {
        notFound(res, head);
        return;
      }
      const type = request.file.endsWith('.png') ? 'image/png' : 'application/json; charset=utf-8';
      send(res, 200, type, bytes, head);
    })().catch(() => {
      if (!res.headersSent) notFound(res, head);
    });
  };
}

export interface LocalSpritesOptions {
  /** Default: `<repo>/content/_local/sprites`, from the Vite root (apps/web). */
  readonly baseDir?: string;
  readonly fs?: LocalSpritesFs;
}

export function localSprites(options: LocalSpritesOptions = {}): Plugin {
  return {
    name: 'torneo:local-sprites',
    apply: 'serve',
    configureServer(server) {
      const baseDir =
        options.baseDir ??
        path.resolve(server.config.root, '..', '..', 'content', '_local', 'sprites');
      server.middlewares.use(createLocalSpritesHandler(baseDir, options.fs));
    },
  };
}
