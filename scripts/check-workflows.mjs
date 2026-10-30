#!/usr/bin/env node
// RF-C12 and plan-c decisions 15 and 23: the GitHub workflows and the local composite actions
// keep the security policy of a public repository (ADR-0006 D6 and D7).
//
// Reads .github/workflows/*.yml and .github/actions/**/action.yml. Every problem is
// `<file>: <rule>: <detail>`. Runs in the CI job `policy`, never in `web` (RF-C10).
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { isAlias, isScalar, parseDocument, visit } from 'yaml';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

const WORKFLOWS_DIR = '.github/workflows';
const ACTIONS_DIR = '.github/actions';

const READ = { contents: 'read' };

/**
 * Closed list of jobs and of their exact `permissions` (plan-c decision 23). A job that is not
 * here, a missing job, or one permission more, less or at another level fails: a new job is
 * added to this table in the same PR, in front of the reviewer.
 * @type {Record<string, Record<string, Record<string, string>>>}
 */
export const JOB_PERMISSIONS = {
  'ci.yml': {
    changes: { contents: 'read', 'pull-requests': 'read' },
    policy: READ,
    rust: READ,
    web: READ,
    'e2e-server': READ,
    e2e: READ,
    images: READ,
    'ci-ok': {},
  },
  'release.yml': {
    build: { contents: 'read', packages: 'write' },
    deploy: {},
    verify: {},
  },
};

// 008 plan-d decision 12: the artifact of e2e-server is downloaded only by name, from this run.
const DOWNLOAD_ARTIFACT = /^actions\/download-artifact@/i;
const DOWNLOAD_FORBIDDEN = ['run-id', 'github-token', 'repository'];
// rust-cache puts these in its key: with save-if: false, e2e-server would run cold forever.
const CACHE_KEY_ENV = /^(?:CARGO_|RUST)/;
const E2E_IF = "needs.changes.outputs.e2e == 'true'";
const COMPOSE_DEV = 'infra/compose.dev.yml';

// 008 RF-D9, plan-d decision 12: the server image of a release is built twice with the same
// inputs, checked between the two builds and compared by config digest before the retention.
const BUILD_PUSH = /^docker\/build-push-action@/i;
const RETENTION = /^actions\/delete-package-versions@/i;
const SERVER_DOCKERFILE = 'apps/server/Dockerfile';
const RELEASE_CHECK_TAG = 'torneo-release-check/server:local';
const SAME_INPUTS = [
  'context',
  'file',
  'platforms',
  'build-args',
  'labels',
  'provenance',
  'sbom',
  'cache-from',
];
const BUILD_INPUTS = [...SAME_INPUTS, 'push', 'load', 'tags'];
const RELEASE_CHECK = /node scripts\/check-release-binary\.mjs(?! --expect-dev)\s/;

const FORBIDDEN_TRIGGERS = ['pull_request_target', 'workflow_run'];
// Data an outsider controls. Plan-c goes further: no `${{ }}` at all inside `run:` (see below).
const SHA_REF = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+(\/[^@\s]+)?@[0-9a-f]{40}$/;
// Owners and repositories are case-insensitive on GitHub: `Actions/checkout` is the same action.
const CHECKOUT = /^actions\/checkout@/i;
// Only the composite actions under .github/actions/ are analysed, so no other local path is usable
// (review of PR #5): no `./`, no `./tools/x`, no `..`.
const LOCAL_ACTION = /^\.\/\.github\/actions\/([a-z0-9-]+)$/;
const IMAGE_DIGEST = /@sha256:[0-9a-f]{64}$/;
// Closed lists for container: and services: (review of PR #22, security-reviewer). Anything else
// (--privileged, -v/--volume, --cap-add, --ipc=host, --network, --user, --security-opt,
// credentials, volumes, ...) fails.
const CONTAINER_KEYS = ['image', 'options'];
const SERVICE_KEYS = ['env', 'image', 'options', 'ports'];
const CONTAINER_OPTIONS = [/^--shm-size=[0-9]+[kmg]?$/];
const EXPRESSION = /\$\{\{([\s\S]*?)\}\}/g;
// The secrets context in any syntax: secrets.X, secrets['X'], toJSON(secrets), ...
// Contexts are case-insensitive in GitHub expressions.
const SECRETS_WORD = /(^|[^A-Za-z0-9_.-])secrets\b/gi;
const SECRETS_ALLOWED = /^secrets\s*\.\s*GITHUB_TOKEN\b/i;
const FORBIDDEN_TEXT = [
  // No call to the deployment platform in 001 (RF-C14): neither its name nor its deploy endpoint.
  { rule: 'deploy-platform', pattern: /coolify/i, what: 'the deployment platform name' },
  { rule: 'deploy-platform', pattern: /\/api\/v1\/deploy/i, what: '/api/v1/deploy' },
  // Dependabot PRs are never merged automatically (RF-C16).
  { rule: 'auto-merge', pattern: /gh\s+pr\s+merge/i, what: 'gh pr merge' },
  {
    rule: 'auto-merge',
    pattern: /enable-pull-request-automerge/i,
    what: 'enable-pull-request-automerge',
  },
  { rule: 'auto-merge', pattern: /dependabot\/fetch-metadata/i, what: 'dependabot/fetch-metadata' },
];

/**
 * @typedef {{ read: (rel: string) => string | null, files: string[] }} Tree
 */

const isMap = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const same = (a, b) => JSON.stringify(sortKeys(a)) === JSON.stringify(sortKeys(b));
function sortKeys(v) {
  if (!isMap(v)) return v;
  return Object.fromEntries(
    Object.keys(v)
      .sort()
      .map((k) => [k, sortKeys(v[k])]),
  );
}
const show = (v) => JSON.stringify(v);

/**
 * Parses with the same hardening as check-compose: no anchors, aliases, merge keys or explicit
 * tags, and any warning is a problem, so the script reads exactly what GitHub reads.
 * @returns {{ doc: any, document: import('yaml').Document } | null}
 */
function parseYaml(text, file, problems) {
  try {
    const document = parseDocument(text);
    if (document.errors.length > 0) throw document.errors[0];
    for (const warning of document.warnings) {
      problems.push(`${file}: yaml-warnings: ${warning.message.split('\n')[0]}`);
    }
    visit(document, {
      Pair(_, pair) {
        if (isScalar(pair.key) && pair.key.value === '<<') {
          problems.push(`${file}: yaml-anchors: merge key << is not allowed`);
        }
      },
      Alias(_, node) {
        problems.push(`${file}: yaml-anchors: alias *${node.source} is not allowed`);
      },
      Node(_, node) {
        if (!isAlias(node) && node.anchor) {
          problems.push(`${file}: yaml-anchors: anchor &${node.anchor} is not allowed`);
        }
        if (node.tag) problems.push(`${file}: yaml-tags: ${node.tag} is not allowed`);
      },
    });
    const doc = document.toJS();
    if (!isMap(doc)) {
      problems.push(`${file}: yaml: not a mapping`);
      return null;
    }
    return { doc, document };
  } catch (error) {
    problems.push(
      `${file}: yaml: ${error instanceof Error ? error.message.split('\n')[0] : error}`,
    );
    return null;
  }
}

/**
 * Every scalar (keys and values) with the key that holds it, so the rules see the parsed values
 * and never the comments.
 * @returns {{ key: string, value: string, isKey: boolean }[]}
 */
function scalars(document) {
  /** @type {{ key: string, value: string, isKey: boolean }[]} */
  const out = [];
  visit(document, {
    Pair(_, pair) {
      const key = isScalar(pair.key) ? String(pair.key.value) : '';
      out.push({ key, value: key, isKey: true });
      if (isScalar(pair.value) && pair.value.value !== null) {
        out.push({ key, value: String(pair.value.value), isKey: false });
      }
    },
    Seq(_, seq) {
      for (const item of seq.items) {
        if (isScalar(item) && item.value !== null) {
          out.push({ key: '', value: String(item.value), isKey: false });
        }
      }
    },
  });
  return out;
}

/** The secrets context outside secrets.GITHUB_TOKEN, in expressions and `if:` (RF-C12). */
function checkSecrets(document, file, problems) {
  const found = new Set();
  const scan = (expr) => {
    for (const m of expr.matchAll(SECRETS_WORD)) {
      const at = m.index + m[1].length;
      if (!SECRETS_ALLOWED.test(expr.slice(at))) found.add(expr.trim());
    }
  };
  for (const { key, value, isKey } of scalars(document)) {
    if (isKey) {
      // `secrets: inherit` or a `secrets:` map passed to a reusable workflow.
      if (key.toLowerCase() === 'secrets') found.add('secrets: (key)');
      continue;
    }
    for (const m of value.matchAll(EXPRESSION)) scan(m[1]);
    // `if:` is an expression even without ${{ }}.
    if (key === 'if') scan(value);
  }
  for (const what of found) {
    problems.push(`${file}: secrets: only secrets.GITHUB_TOKEN is allowed, found ${show(what)}`);
  }
}

/**
 * Text rules on the raw text (comments included, on purpose) and on every parsed value, so a YAML
 * escape such as `\x79` cannot hide the text from the raw check (review of PR #5).
 */
function checkText(text, document, file, problems) {
  const values = document ? scalars(document).map((s) => s.value) : [];
  for (const { rule, pattern, what } of FORBIDDEN_TEXT) {
    if (pattern.test(text) || values.some((v) => pattern.test(v))) {
      problems.push(`${file}: ${rule}: ${what} must never appear`);
    }
  }
}

/** `write-all`, `read-all` and `id-token` fail everywhere (plan-c decision 23). */
function checkPermissionShape(permissions, where, file, problems) {
  if (permissions === 'write-all' || permissions === 'read-all') {
    problems.push(`${file}: permissions: ${where} uses ${permissions}`);
    return false;
  }
  if (isMap(permissions) && 'id-token' in permissions) {
    problems.push(`${file}: permissions: ${where} declares id-token`);
  }
  return true;
}

/**
 * @param {any[]} steps
 * @param {string} where
 */
function checkSteps(steps, where, file, problems, local) {
  if (!Array.isArray(steps)) return;
  steps.forEach((step, i) => {
    if (!isMap(step)) return;
    const at = `${where}.steps[${i}]${step.name ? ` (${step.name})` : ''}`;
    if ('continue-on-error' in step) {
      problems.push(`${file}: continue-on-error: ${at} must not declare it`);
    }
    if (typeof step.uses === 'string') {
      checkUses(step.uses, at, file, problems, local);
      if (DOWNLOAD_ARTIFACT.test(step.uses)) checkDownload(step, at, file, problems);
      if (CHECKOUT.test(step.uses)) {
        const persist = isMap(step.with) ? step.with['persist-credentials'] : undefined;
        if (persist !== false && persist !== 'false') {
          problems.push(`${file}: persist-credentials: ${at} must set persist-credentials: false`);
        }
      }
    }
    if (typeof step.run === 'string' && step.run.includes('${{')) {
      // Stricter than RF-C12 (controllable data only): every value reaches `run:` through env.
      problems.push(`${file}: run-expression: ${at} interpolates \${{ }} inside run: use env:`);
    }
    // Actions that evaluate code (actions/github-script) are the same injection as run:.
    if (
      isMap(step.with) &&
      typeof step.with.script === 'string' &&
      step.with.script.includes('${{')
    ) {
      problems.push(`${file}: script-expression: ${at} interpolates \${{ }} inside with.script`);
    }
  });
}

/**
 * `actions/download-artifact` only by `name`, without `run-id`, `github-token` or `repository`,
 * which would bring a binary of another run (008 plan-d decision 12, change 10 of devops).
 */
function checkDownload(step, at, file, problems) {
  const withs = isMap(step.with) ? step.with : {};
  for (const key of DOWNLOAD_FORBIDDEN) {
    if (key in withs) {
      problems.push(`${file}: download-artifact: ${at} must not set ${key} (only this run)`);
    }
  }
  if (typeof withs.name !== 'string' || withs.name === '') {
    problems.push(`${file}: download-artifact: ${at} must download by name`);
  }
}

/** The image of the `postgres` service of infra/compose.dev.yml, or `undefined`. */
function composeDevPostgres(tree) {
  const text = tree.read(COMPOSE_DEV);
  if (text === null) return undefined;
  try {
    const doc = parseDocument(text).toJS();
    const image = doc?.services?.postgres?.image;
    return typeof image === 'string' ? image : undefined;
  } catch {
    return undefined;
  }
}

/**
 * ci.yml, 008 plan-d decision 12: e2e-server runs exactly when e2e does, without CARGO_* or
 * RUST* variables of its own, and every Postgres service uses the image of compose.dev.yml (a
 * Dependabot bump of one fails here, in `policy`, until the other moves too).
 */
function checkE2e(jobs, file, problems, tree) {
  const server = jobs['e2e-server'];
  const e2e = jobs.e2e;
  if (isMap(e2e) && String(e2e.if ?? '').trim() !== E2E_IF) {
    problems.push(`${file}: e2e-server-if: jobs.e2e must run with if: ${E2E_IF}`);
  }
  if (isMap(server)) {
    if (String(server.if ?? '').trim() !== String(e2e?.if ?? '').trim()) {
      problems.push(
        `${file}: e2e-server-if: jobs.e2e-server must run with the same if: as jobs.e2e`,
      );
    }
    const envs = [
      ['jobs.e2e-server.env', server.env],
      ...(Array.isArray(server.steps) ? server.steps : []).map((step, i) => [
        `jobs.e2e-server.steps[${i}].env`,
        isMap(step) ? step.env : undefined,
      ]),
    ];
    for (const [where, env] of envs) {
      if (!isMap(env)) continue;
      for (const name of Object.keys(env).filter((k) => CACHE_KEY_ENV.test(k))) {
        problems.push(
          `${file}: e2e-server-env: ${where} sets ${name}: rust-cache keys on CARGO_*/RUST*, and with save-if: false the job would run cold`,
        );
      }
    }
  }
  const expected = composeDevPostgres(tree);
  for (const [id, job] of Object.entries(jobs)) {
    if (!isMap(job) || !isMap(job.services)) continue;
    for (const [name, service] of Object.entries(job.services)) {
      const image = isMap(service) ? service.image : service;
      if (typeof image !== 'string' || !/^postgres:/.test(image)) continue;
      if (expected === undefined) {
        problems.push(
          `${file}: postgres-digest: jobs.${id}.services.${name}: ${COMPOSE_DEV} has no postgres image to compare with`,
        );
      } else if (image !== expected) {
        problems.push(
          `${file}: postgres-digest: jobs.${id}.services.${name} must use the image of ${COMPOSE_DEV} (${expected})`,
        );
      }
    }
  }
}

/**
 * 008 RF-D9: `rust` checks its musl binary and `images` the `/server` of its image, both as a
 * release (without --expect-dev).
 */
function checkBinaryChecks(jobs, file, problems) {
  for (const id of ['rust', 'images']) {
    const steps = Array.isArray(jobs[id]?.steps) ? jobs[id].steps : [];
    if (!steps.some((s) => isMap(s) && RELEASE_CHECK.test(`${s.run ?? ''}\n`))) {
      problems.push(
        `${file}: release-binary: jobs.${id} must run scripts/check-release-binary.mjs without --expect-dev`,
      );
    }
  }
}

/**
 * 008 RF-D9 (plan-d decision 12, open question 7): in release.yml the server is built twice with
 * the same inputs. The first build only loads a local tag; a step between the two runs
 * check-release-binary on it; the second pushes; and before the retention a step compares the
 * `imageid` of both builds with the `config.digest` that the registry publishes.
 */
function checkReleaseServer(jobs, file, problems) {
  const rule = 'release-binary';
  const steps = Array.isArray(jobs.build?.steps) ? jobs.build.steps : [];
  const builds = steps
    .map((step, index) => ({ step, index }))
    .filter(
      ({ step }) =>
        isMap(step) &&
        typeof step.uses === 'string' &&
        BUILD_PUSH.test(step.uses) &&
        isMap(step.with) &&
        step.with.file === SERVER_DOCKERFILE,
    );
  if (builds.length !== 2) {
    problems.push(
      `${file}: ${rule}: jobs.build must build ${SERVER_DOCKERFILE} exactly twice (a local build for the check, then the push), found ${builds.length}`,
    );
    return;
  }
  const [check, push] = builds;
  const a = check.step.with;
  const b = push.step.with;
  const is = (value, bool) => value === bool || value === String(bool);
  if (!is(a.push, false) || !is(a.load, true)) {
    problems.push(`${file}: ${rule}: the first server build must have push: false and load: true`);
  }
  if (String(a.tags ?? '').trim() !== RELEASE_CHECK_TAG) {
    problems.push(
      `${file}: ${rule}: the first server build must tag only ${RELEASE_CHECK_TAG} (never a GHCR tag)`,
    );
  }
  if (!is(b.push, true) || is(b.load, true)) {
    problems.push(`${file}: ${rule}: the second server build must have push: true and no load`);
  }
  for (const key of SAME_INPUTS) {
    if (!(key in a) || !(key in b)) {
      problems.push(`${file}: ${rule}: ${key} must be set in both server builds`);
    } else if (String(a[key]) !== String(b[key])) {
      problems.push(`${file}: ${rule}: ${key} must be the same in both server builds`);
    }
  }
  for (const key of new Set([...Object.keys(a), ...Object.keys(b)])) {
    if (!BUILD_INPUTS.includes(key)) {
      problems.push(`${file}: ${rule}: ${key} is not allowed in the server builds`);
    }
  }
  if (String(check.step.if ?? '') !== String(push.step.if ?? '')) {
    problems.push(`${file}: ${rule}: both server builds must have the same if:`);
  }
  const between = steps.slice(check.index + 1, push.index);
  if (!between.some((s) => isMap(s) && RELEASE_CHECK.test(`${s.run ?? ''}\n`))) {
    problems.push(
      `${file}: ${rule}: a step between the two server builds must run scripts/check-release-binary.mjs on the local image (without --expect-dev)`,
    );
  }
  const retention = steps.findIndex(
    (s) => isMap(s) && typeof s.uses === 'string' && RETENTION.test(s.uses),
  );
  // The checked side comes from `docker save` (the config blob, the same with either image
  // store) and the pushed side from the imageid of the push and the config.digest of the tags.
  // The imageid of a `load` build is the manifest digest with the containerd image store, so it
  // is never used (008 plan-d deviation D38).
  const references = (s, id) =>
    typeof id === 'string' &&
    isMap(s.env) &&
    Object.values(s.env).some((v) => String(v).includes(`steps.${id}.outputs.imageid`));
  const compares = (s) =>
    isMap(s) &&
    typeof s.run === 'string' &&
    references(s, push.step.id) &&
    s.run.includes(`docker save ${RELEASE_CHECK_TAG}`) &&
    s.run.includes('imagetools inspect --raw') &&
    s.run.includes('.config.digest');
  const end = retention === -1 ? steps.length : retention;
  if (!steps.slice(push.index + 1, end).some(compares)) {
    problems.push(
      `${file}: ${rule}: after the push and before the retention, a step must compare the config of the checked image (docker save ${RELEASE_CHECK_TAG}) with the imageid of the push and the config.digest of imagetools inspect --raw`,
    );
  }
  steps.forEach((s, i) => {
    if (isMap(s) && references(s, check.step.id)) {
      problems.push(
        `${file}: ${rule}: jobs.build.steps[${i}] uses the imageid of the local build, which depends on the image store (the manifest digest with containerd): use docker save`,
      );
    }
  });
}

/**
 * @param {Set<string>} local names of the composite actions present under .github/actions/
 */
function checkUses(uses, at, file, problems, local) {
  if (uses.startsWith('.')) {
    const match = LOCAL_ACTION.exec(uses);
    if (!match) {
      problems.push(
        `${file}: local-action: ${at} uses ${uses}: local actions must be ./.github/actions/<name>`,
      );
    } else if (!local.has(match[1])) {
      problems.push(`${file}: local-action: ${at} uses ${uses}, which has no action.yml`);
    }
    return;
  }
  if (!SHA_REF.test(uses)) {
    problems.push(`${file}: sha-pin: ${at} uses ${uses}: pin a third-party action by full SHA`);
  }
}

function triggersOf(on) {
  if (typeof on === 'string') return [on];
  if (Array.isArray(on)) return on.map(String);
  if (isMap(on)) return Object.keys(on);
  return [];
}

/** @param {any} doc */
function checkWorkflow(doc, name, file, problems, local, tree) {
  for (const trigger of triggersOf(doc.on)) {
    if (FORBIDDEN_TRIGGERS.includes(trigger)) {
      problems.push(`${file}: forbidden-trigger: ${trigger} is not allowed`);
    }
  }

  if (checkPermissionShape(doc.permissions, 'the root', file, problems)) {
    if (!same(doc.permissions, READ)) {
      problems.push(
        `${file}: permissions-root: must be exactly {"contents":"read"}, got ${show(doc.permissions ?? null)}`,
      );
    }
  }

  const jobs = isMap(doc.jobs) ? doc.jobs : {};
  const table = JOB_PERMISSIONS[name];
  if (!table) {
    problems.push(`${file}: permissions-job: workflow ${name} is not in the permissions table`);
  } else {
    for (const job of Object.keys(table)) {
      if (!(job in jobs)) problems.push(`${file}: permissions-job: job ${job} is missing`);
    }
  }

  for (const [id, job] of Object.entries(jobs)) {
    if (!isMap(job)) continue;
    const where = `jobs.${id}`;
    if (table && !(id in table)) {
      problems.push(`${file}: permissions-job: ${where} is not in the permissions table`);
    }
    if (!('permissions' in job)) {
      problems.push(`${file}: permissions-job: ${where} must declare permissions`);
    } else if (checkPermissionShape(job.permissions, where, file, problems) && table?.[id]) {
      if (!same(job.permissions ?? {}, table[id]) || job.permissions === null) {
        problems.push(
          `${file}: permissions-job: ${where} must be exactly ${show(table[id])}, got ${show(job.permissions)}`,
        );
      }
    }
    const timeout = job['timeout-minutes'];
    if (typeof timeout !== 'number' || !(timeout > 0)) {
      problems.push(`${file}: timeout-minutes: ${where} must declare a positive timeout-minutes`);
    }
    if (typeof job['runs-on'] !== 'string' || !/^ubuntu-[0-9a-z.]+$/.test(job['runs-on'])) {
      problems.push(`${file}: runs-on: ${where} must run on a ubuntu-* runner`);
    }
    if ('continue-on-error' in job) {
      problems.push(`${file}: continue-on-error: ${where} must not declare it`);
    }
    if (typeof job.uses === 'string') checkUses(job.uses, where, file, problems, local);
    checkImages(job, where, file, problems, doc.defaults);
    checkPlaywrightImage(job, where, file, problems);
    checkSteps(job.steps, where, file, problems, local);
  }

  if (name === 'release.yml') {
    const deploy = jobs.deploy;
    const environment = isMap(deploy?.environment) ? deploy.environment.name : deploy?.environment;
    if (environment !== 'production') {
      problems.push(
        `${file}: deploy-environment: jobs.deploy must declare environment: production`,
      );
    }
  }

  if (name === 'ci.yml') {
    checkE2e(jobs, file, problems, tree);
    checkBinaryChecks(jobs, file, problems);
  }
  if (name === 'release.yml') checkReleaseServer(jobs, file, problems);

  if (name === 'ci.yml') {
    // RF-C12: policy always runs; ci-ok (the only required check) waits for every other job.
    if (isMap(jobs.policy) && 'if' in jobs.policy) {
      problems.push(`${file}: policy-always: jobs.policy must not have an if:`);
    }
    const ok = jobs['ci-ok'];
    if (isMap(ok)) {
      const others = Object.keys(jobs)
        .filter((j) => j !== 'ci-ok')
        .sort();
      const needs = (Array.isArray(ok.needs) ? ok.needs : [ok.needs]).map(String).sort();
      if (!same(needs, others)) {
        problems.push(
          `${file}: ci-ok: jobs.ci-ok must need every other job ${show(others)}, got ${show(needs)}`,
        );
      }
      if (String(ok.if ?? '').replace(/\s/g, '') !== 'always()') {
        problems.push(`${file}: ci-ok: jobs.ci-ok must run with if: always()`);
      }
    }
  }
}

/** `container:` and `services:` images are pinned by digest, like the actions (review of PR #5). */
function checkImages(job, where, file, problems, workflowDefaults) {
  /** @type {[string, unknown, unknown, string[]][]} at, image, map, allowed keys */
  const images = [];
  if ('container' in job) {
    const map = isMap(job.container) ? job.container : null;
    images.push([`${where}.container`, map ? map.image : job.container, map, CONTAINER_KEYS]);
    // Inside a container, run: defaults to `sh -e {0}` instead of bash, so `set -o pipefail`
    // exits 2 (review of PR #22, qa-engineer): the job or the workflow sets shell: bash.
    const shell = job.defaults?.run?.shell ?? workflowDefaults?.run?.shell;
    if (shell !== 'bash') {
      problems.push(
        `${file}: container-shell: ${where} runs in a container and needs defaults.run.shell: bash`,
      );
    }
  }
  if ('services' in job) {
    const services = isMap(job.services) ? job.services : {};
    if (!isMap(job.services)) images.push([`${where}.services`, undefined, null, SERVICE_KEYS]);
    for (const [name, service] of Object.entries(services)) {
      const map = isMap(service) ? service : null;
      images.push([`${where}.services.${name}`, map ? map.image : service, map, SERVICE_KEYS]);
    }
  }
  for (const [at, image, map, keys] of images) {
    if (typeof image !== 'string' || !IMAGE_DIGEST.test(image)) {
      problems.push(`${file}: container-image: ${at} must pin its image by @sha256 digest`);
    }
    if (!map) continue;
    for (const key of Object.keys(map)) {
      if (!keys.includes(key))
        problems.push(`${file}: container-keys: ${at}: ${key} is not allowed`);
    }
    if ('options' in map) {
      const options =
        typeof map.options === 'string' ? map.options.trim().split(/\s+/) : [String(map.options)];
      for (const option of options) {
        if (!CONTAINER_OPTIONS.some((re) => re.test(option))) {
          problems.push(
            `${file}: container-options: ${at}: ${option} is not allowed (only --shm-size=<size>)`,
          );
        }
      }
    }
  }
}

/**
 * The Playwright image tag and PLAYWRIGHT_IMAGE_VERSION of the job are the same version (review of
 * PR #22, architect). The job itself checks it against @playwright/test at run time, so the three
 * move together (plan-c deviation 21, ADR-0006 note of 2026-10-26).
 */
function checkPlaywrightImage(job, where, file, problems) {
  const image = isMap(job.container) ? job.container.image : job.container;
  const tag =
    typeof image === 'string'
      ? /^mcr\.microsoft\.com\/playwright:v([0-9]+\.[0-9]+\.[0-9]+)-/.exec(image)?.[1]
      : undefined;
  const declared = isMap(job.env) ? job.env.PLAYWRIGHT_IMAGE_VERSION : undefined;
  if (tag === undefined && declared === undefined) return;
  if (tag === undefined) {
    problems.push(
      `${file}: playwright-image: ${where}: PLAYWRIGHT_IMAGE_VERSION without a mcr.microsoft.com/playwright:v<X>- container image`,
    );
  } else if (String(declared) !== tag) {
    problems.push(
      `${file}: playwright-image: ${where}: PLAYWRIGHT_IMAGE_VERSION ${declared ?? '(missing)'} must equal the image tag v${tag}`,
    );
  }
}

/** @param {any} doc */
function checkAction(doc, file, problems, local) {
  const runs = isMap(doc.runs) ? doc.runs : {};
  if (runs.using !== 'composite') {
    problems.push(`${file}: composite: local actions must be composite, got ${show(runs.using)}`);
  }
  checkSteps(runs.steps, 'runs', file, problems, local);
}

/**
 * @param {Tree} tree
 * @returns {string[]}
 */
export function checkWorkflows(tree) {
  /** @type {string[]} */
  const problems = [];
  const workflows = tree.files.filter((f) => /^\.github\/workflows\/[^/]+\.ya?ml$/.test(f));
  const actions = tree.files.filter((f) => /^\.github\/actions\/.+\/action\.ya?ml$/.test(f));
  const local = new Set(
    actions
      .map((f) => /^\.github\/actions\/([^/]+)\/action\.ya?ml$/.exec(f)?.[1])
      .filter((name) => name !== undefined),
  );
  if (workflows.length === 0) problems.push(`${WORKFLOWS_DIR}: exists: no workflow found`);
  for (const name of Object.keys(JOB_PERMISSIONS)) {
    if (!workflows.includes(`${WORKFLOWS_DIR}/${name}`)) {
      problems.push(`${WORKFLOWS_DIR}/${name}: exists: missing`);
    }
  }

  for (const file of [...workflows, ...actions]) {
    const text = tree.read(file);
    if (text === null) continue;
    const parsed = parseYaml(text, file, problems);
    checkText(text, parsed?.document, file, problems);
    if (!parsed) continue;
    checkSecrets(parsed.document, file, problems);
    if (workflows.includes(file))
      checkWorkflow(parsed.doc, path.posix.basename(file), file, problems, local, tree);
    else checkAction(parsed.doc, file, problems, local);
  }
  return problems;
}

function walk(dir, rel, out) {
  const full = path.join(dir, rel);
  if (!existsSync(full)) return;
  for (const name of readdirSync(full)) {
    const child = `${rel}/${name}`;
    if (statSync(path.join(dir, child)).isDirectory()) walk(dir, child, out);
    else out.push(child);
  }
}

/**
 * @param {string} dir
 * @returns {Tree}
 */
export function loadTree(dir) {
  /** @type {string[]} */
  const files = [];
  walk(dir, WORKFLOWS_DIR, files);
  walk(dir, ACTIONS_DIR, files);
  return {
    read: (rel) => {
      const file = path.join(dir, rel);
      return existsSync(file) ? readFileSync(file, 'utf8') : null;
    },
    files,
  };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const dir = process.argv[2] ? path.resolve(process.argv[2]) : ROOT;
  const problems = checkWorkflows(loadTree(dir));
  if (problems.length > 0) {
    console.error(`check-workflows: ${problems.length} problem(s)`);
    for (const p of problems) console.error(`  ${p}`);
    process.exit(1);
  }
  console.log('check-workflows: ok');
}
