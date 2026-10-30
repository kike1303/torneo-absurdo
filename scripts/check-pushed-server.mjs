#!/usr/bin/env node
// 008 RF-D9 (plan-d decision 12): in release.yml, what was pushed is what was checked.
//
//   PUSHED=<imageid of the push> TAG=vX.Y.Z SHORT=sha-<7> IMAGE_REPO=ghcr.io/o/r \
//     node scripts/check-pushed-server.mjs
//
// The checked side is the config blob of the local image (`docker save` of
// torneo-release-check/server:local, hashed here): `docker image inspect` and the imageid of a
// `load` build give the manifest digest with the containerd image store (plan-d deviation D38).
// The pushed side is the imageid of the push (the config digest of the image exporter) and the
// `config.digest` of each tag. Any mismatch, missing value or error exits non-zero: the step
// fails closed, and check-workflows requires the step to be exactly this command.
// Node built-ins only: release.yml runs it with the runner's Node, without setup-js.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const CHECKED_TAG = 'torneo-release-check/server:local';
const DIGEST = /^sha256:[0-9a-f]{64}$/;
const TOKEN = /^[A-Za-z0-9][A-Za-z0-9._/:-]*$/;

/**
 * The digest of the config blob of a `docker save` extracted into `dir`.
 * @param {string} dir
 */
export function checkedConfig(dir) {
  const manifest = JSON.parse(readFileSync(path.join(dir, 'manifest.json'), 'utf8'));
  if (!Array.isArray(manifest) || manifest.length !== 1) {
    throw new Error(`manifest.json must describe exactly one image, found ${manifest?.length}`);
  }
  const config = manifest[0]?.Config;
  if (typeof config !== 'string' || config === '') {
    throw new Error('manifest.json has no Config');
  }
  const blob = path.resolve(dir, config);
  if (path.relative(path.resolve(dir), blob).startsWith('..') || path.isAbsolute(config)) {
    throw new Error(`Config ${config} is outside the saved image`);
  }
  return `sha256:${createHash('sha256').update(readFileSync(blob)).digest('hex')}`;
}

/**
 * @param {{ pushed: string, checked: string, configs: { ref: string, config: unknown }[] }} input
 * @returns {string[]} problems, empty when the pushed server is the checked one
 */
export function comparePushed({ pushed, checked, configs }) {
  if (!DIGEST.test(checked)) return [`the checked config ${checked} is not a sha256 digest`];
  const problems = [];
  if (pushed !== checked) {
    problems.push(
      `the pushed server (imageid ${pushed || '<none>'}) is not the checked one (${checked})`,
    );
  }
  if (configs.length === 0) problems.push('no pushed tag to compare');
  for (const { ref, config } of configs) {
    if (config !== checked) {
      problems.push(`${ref} has config ${String(config)}, the checked image is ${checked}`);
    }
  }
  return problems;
}

/** @param {NodeJS.ProcessEnv} env */
export function readEnv(env) {
  const values = {};
  const problems = [];
  for (const key of ['PUSHED', 'TAG', 'SHORT', 'IMAGE_REPO']) {
    const value = env[key] ?? '';
    if (!TOKEN.test(value)) problems.push(`${key} must be set (got ${JSON.stringify(value)})`);
    values[key] = value;
  }
  return { values, problems };
}

function main() {
  const { values, problems: usage } = readEnv(process.env);
  if (process.argv.length !== 2 || usage.length > 0) {
    console.error('usage: PUSHED=… TAG=… SHORT=… IMAGE_REPO=… check-pushed-server.mjs');
    for (const problem of usage) console.error(`  ${problem}`);
    process.exit(2);
  }
  const dir = mkdtempSync(path.join(tmpdir(), 'pushed-server-'));
  let problems;
  try {
    const tar = path.join(dir, 'image.tar');
    const root = path.join(dir, 'image');
    execFileSync('docker', ['save', '-o', tar, CHECKED_TAG], {
      stdio: ['ignore', 'inherit', 'inherit'],
    });
    mkdirSync(root);
    execFileSync('tar', ['-x', '-f', tar, '-C', root]);
    const checked = checkedConfig(root);
    console.log(`checked server config: ${checked}`);
    const configs = [values.TAG, values.SHORT].map((tag) => {
      const ref = `${values.IMAGE_REPO}/server:${tag}`;
      const raw = execFileSync('docker', ['buildx', 'imagetools', 'inspect', '--raw', ref], {
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'inherit'],
      });
      return { ref, config: JSON.parse(raw)?.config?.digest };
    });
    problems = comparePushed({ pushed: values.PUSHED, checked, configs });
    if (problems.length === 0) {
      for (const { ref, config } of configs)
        console.log(`${ref} config ${config} is the checked image`);
    }
  } catch (error) {
    console.error(`check-pushed-server: ${error instanceof Error ? error.message : error}`);
    process.exit(2);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
  if (problems.length > 0) {
    for (const problem of problems) console.error(`check-pushed-server: ${problem}`);
    process.exit(1);
  }
  console.log('check-pushed-server: ok');
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
