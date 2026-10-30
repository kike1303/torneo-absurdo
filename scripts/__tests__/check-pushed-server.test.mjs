import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterAll, describe, expect, it } from 'vitest';
import { checkedConfig, comparePushed, readEnv } from '../check-pushed-server.mjs';

// 008 RF-D9, plan-d decision 12 (QA finding 3 of phase D): the comparison fails closed.
const SCRIPT = fileURLToPath(new URL('../check-pushed-server.mjs', import.meta.url));
const work = mkdtempSync(path.join(tmpdir(), 'check-pushed-server-test-'));
afterAll(() => rmSync(work, { recursive: true, force: true }));

const CONFIG = Buffer.from('{"architecture":"amd64","os":"linux"}');
const DIGEST = `sha256:${createHash('sha256').update(CONFIG).digest('hex')}`;
const OTHER = `sha256:${'b'.repeat(64)}`;

let n = 0;
/** A `docker save` layout extracted into a new directory. */
function saved(manifest, blobs = { [`blobs/sha256/${DIGEST.slice(7)}`]: CONFIG }) {
  const dir = path.join(work, `saved-${n++}`);
  mkdirSync(path.join(dir, 'blobs/sha256'), { recursive: true });
  writeFileSync(path.join(dir, 'manifest.json'), JSON.stringify(manifest));
  for (const [rel, bytes] of Object.entries(blobs)) writeFileSync(path.join(dir, rel), bytes);
  return dir;
}

describe('checkedConfig', () => {
  it('hashes the config blob named by manifest.json', () => {
    expect(checkedConfig(saved([{ Config: `blobs/sha256/${DIGEST.slice(7)}` }]))).toBe(DIGEST);
  });

  it('fails without exactly one image', () => {
    expect(() => checkedConfig(saved([]))).toThrow(/exactly one image/);
    expect(() => checkedConfig(saved([{ Config: 'a' }, { Config: 'b' }]))).toThrow(
      /exactly one image/,
    );
  });

  it('fails without Config', () => {
    expect(() => checkedConfig(saved([{}]))).toThrow(/no Config/);
  });

  it('fails on a Config outside the saved image', () => {
    expect(() => checkedConfig(saved([{ Config: '../outside.json' }]))).toThrow(/outside/);
    expect(() => checkedConfig(saved([{ Config: '/etc/hosts' }]))).toThrow(/outside/);
  });

  it('fails on a missing blob', () => {
    expect(() => checkedConfig(saved([{ Config: 'blobs/sha256/missing' }]))).toThrow();
  });
});

describe('comparePushed', () => {
  const tags = (...configs) => configs.map((config, i) => ({ ref: `repo/server:t${i}`, config }));

  it('passes when the push and every tag are the checked config', () => {
    expect(
      comparePushed({ pushed: DIGEST, checked: DIGEST, configs: tags(DIGEST, DIGEST) }),
    ).toEqual([]);
  });

  it('fails when the imageid of the push differs', () => {
    expect(
      comparePushed({ pushed: OTHER, checked: DIGEST, configs: tags(DIGEST, DIGEST) }),
    ).toEqual([`the pushed server (imageid ${OTHER}) is not the checked one (${DIGEST})`]);
  });

  it('fails when the push has no imageid', () => {
    expect(comparePushed({ pushed: '', checked: DIGEST, configs: tags(DIGEST) })[0]).toMatch(
      /imageid <none>/,
    );
  });

  it('fails when one tag has another config', () => {
    expect(
      comparePushed({ pushed: DIGEST, checked: DIGEST, configs: tags(DIGEST, OTHER) }),
    ).toEqual([`repo/server:t1 has config ${OTHER}, the checked image is ${DIGEST}`]);
  });

  it('fails when a tag has no config.digest (an index, not an image manifest)', () => {
    expect(
      comparePushed({ pushed: DIGEST, checked: DIGEST, configs: tags(DIGEST, undefined) }),
    ).toHaveLength(1);
  });

  it('fails with no tags', () => {
    expect(comparePushed({ pushed: DIGEST, checked: DIGEST, configs: [] })).toEqual([
      'no pushed tag to compare',
    ]);
  });

  it('fails when the checked side is not a digest, even if both sides are equal', () => {
    expect(comparePushed({ pushed: '', checked: '', configs: tags('') })).toHaveLength(1);
    expect(comparePushed({ pushed: 'x', checked: 'x', configs: tags('x') })).toHaveLength(1);
  });
});

describe('readEnv', () => {
  const env = { PUSHED: DIGEST, TAG: 'v1.2.3', SHORT: 'sha-abc1234', IMAGE_REPO: 'ghcr.io/o/r' };

  it('accepts the four values', () => {
    expect(readEnv(env).problems).toEqual([]);
  });

  it.each(['PUSHED', 'TAG', 'SHORT', 'IMAGE_REPO'])('fails without %s', (key) => {
    expect(readEnv({ ...env, [key]: '' }).problems).toEqual([`${key} must be set (got "")`]);
  });

  it('fails on a value with spaces', () => {
    expect(readEnv({ ...env, TAG: 'v1 v2' }).problems).toHaveLength(1);
  });
});

describe('check-pushed-server.mjs (with a fake docker)', () => {
  // `docker save -o FILE TAG` writes FAKE_TAR to FILE; `docker buildx imagetools inspect --raw
  // REF` prints FAKE_CONFIG_<last char of REF> as the config.digest. FAKE_FAIL=save fails save.
  const bin = path.join(work, 'bin');
  mkdirSync(bin, { recursive: true });
  const docker = path.join(bin, 'docker');
  writeFileSync(
    docker,
    [
      '#!/bin/sh',
      'set -eu',
      'if [ "$1" = save ]; then',
      '  [ "${FAKE_FAIL:-}" = save ] && { echo "no such image" >&2; exit 1; }',
      '  [ "$4" = torneo-release-check/server:local ] || exit 3',
      '  cp "$FAKE_TAR" "$3"',
      '  exit 0',
      'fi',
      'if [ "$1 $2 $3 $4" = "buildx imagetools inspect --raw" ]; then',
      '  case "$5" in',
      '    *:v1.2.3) printf \'{"config":{"digest":"%s"}}\' "$FAKE_CONFIG_TAG" ;;',
      '    *:sha-abc1234) printf \'{"config":{"digest":"%s"}}\' "$FAKE_CONFIG_SHORT" ;;',
      '    *) exit 4 ;;',
      '  esac',
      '  exit 0',
      'fi',
      'exit 5',
      '',
    ].join('\n'),
  );
  chmodSync(docker, 0o755);
  const layout = saved([{ Config: `blobs/sha256/${DIGEST.slice(7)}` }]);
  const tar = path.join(work, 'image.tar');
  const made = spawnSync('tar', ['-c', '-f', tar, '-C', layout, '.']);
  if (made.status !== 0) throw new Error(`tar failed: ${made.stderr}`);

  const run = (env) =>
    spawnSync(process.execPath, [SCRIPT], {
      encoding: 'utf8',
      env: {
        PATH: `${bin}${path.delimiter}${process.env.PATH}`,
        FAKE_TAR: tar,
        FAKE_CONFIG_TAG: DIGEST,
        FAKE_CONFIG_SHORT: DIGEST,
        PUSHED: DIGEST,
        TAG: 'v1.2.3',
        SHORT: 'sha-abc1234',
        IMAGE_REPO: 'ghcr.io/o/r',
        ...env,
      },
    });

  it('exits 0 when the pushed server is the checked one', () => {
    const r = run({});
    expect(r.stderr).toBe('');
    expect(r.status).toBe(0);
    expect(r.stdout).toContain(`checked server config: ${DIGEST}`);
    expect(r.stdout).toContain('check-pushed-server: ok');
  });

  it('exits 1 when the imageid of the push differs', () => {
    const r = run({ PUSHED: OTHER });
    expect(r.status).toBe(1);
    expect(r.stderr).toMatch(/is not the checked one/);
  });

  it('exits 1 when the short tag has another config', () => {
    const r = run({ FAKE_CONFIG_SHORT: OTHER });
    expect(r.status).toBe(1);
    expect(r.stderr).toMatch(/ghcr\.io\/o\/r\/server:sha-abc1234 has config/);
  });

  it('exits 2 when docker save fails', () => {
    const r = run({ FAKE_FAIL: 'save' });
    expect(r.status).toBe(2);
  });

  it('exits 2 without PUSHED', () => {
    const r = run({ PUSHED: '' });
    expect(r.status).toBe(2);
    expect(r.stderr).toMatch(/PUSHED must be set/);
  });

  it('exits 2 with arguments', () => {
    const r = spawnSync(process.execPath, [SCRIPT, 'extra'], {
      encoding: 'utf8',
      env: { ...process.env, PUSHED: DIGEST, TAG: 'v1', SHORT: 's', IMAGE_REPO: 'r' },
    });
    expect(r.status).toBe(2);
  });
});
