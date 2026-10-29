// Spec 008, plan-b decision 6: `just prod-local` generates its two secrets on first use in a file
// ignored by git, and never anywhere else. Runs a copy of the script in a temporary tree.
import { execFileSync } from 'node:child_process';
import {
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';

const script = new URL('../local-secrets.sh', import.meta.url);
const serverExample = readFileSync(
  new URL('../../apps/server/.env.example', import.meta.url),
  'utf8',
);

/** @type {string[]} */
const trees = [];
afterEach(() => {
  for (const tree of trees.splice(0)) rmSync(tree, { recursive: true, force: true });
});

function tree() {
  const root = mkdtempSync(join(tmpdir(), 'local-secrets-'));
  trees.push(root);
  mkdirSync(join(root, 'scripts'));
  mkdirSync(join(root, 'infra'));
  copyFileSync(script, join(root, 'scripts', 'local-secrets.sh'));
  return root;
}

/** @param {string} dir @returns {string[]} */
function files(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory() ? files(join(dir, entry.name)) : [join(dir, entry.name)],
  );
}

const run = (/** @type {string} */ root) =>
  execFileSync('bash', [join(root, 'scripts', 'local-secrets.sh')], { cwd: tmpdir() });

describe('local-secrets.sh', () => {
  it('creates only infra/local.secrets.env, with two 32-byte base64url secrets and mode 600', () => {
    const root = tree();
    run(root);
    const out = join(root, 'infra', 'local.secrets.env');
    expect(files(root).sort()).toEqual([out, join(root, 'scripts', 'local-secrets.sh')].sort());
    expect(statSync(out).mode & 0o777).toBe(0o600);
    const lines = readFileSync(out, 'utf8')
      .trimEnd()
      .split('\n')
      .filter((line) => !line.startsWith('#'));
    expect(lines.map((line) => line.split('=')[0])).toEqual(['INTERNAL_API_SECRET', 'IP_HASH_KEY']);
    for (const line of lines) {
      const value = line.slice(line.indexOf('=') + 1);
      expect(value).toMatch(/^[A-Za-z0-9_-]{43}$/);
      // Never the public development values of the .env.example.
      expect(serverExample).not.toContain(value);
    }
    expect(lines[0].split('=')[1]).not.toBe(lines[1].split('=')[1]);
  });

  it('is idempotent: a second run keeps the file as it is', () => {
    const root = tree();
    run(root);
    const out = join(root, 'infra', 'local.secrets.env');
    const first = readFileSync(out, 'utf8');
    run(root);
    expect(readFileSync(out, 'utf8')).toBe(first);
  });

  it('is ignored by git and by the Docker build context', () => {
    const gitignore = readFileSync(new URL('../../.gitignore', import.meta.url), 'utf8');
    const dockerignore = readFileSync(new URL('../../.dockerignore', import.meta.url), 'utf8');
    expect(gitignore.split('\n')).toContain('infra/local.secrets.env');
    expect(dockerignore.split('\n')).toContain('infra/local.secrets.env');
  });
});
