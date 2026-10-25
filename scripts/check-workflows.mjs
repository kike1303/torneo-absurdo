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

const FORBIDDEN_TRIGGERS = ['pull_request_target', 'workflow_run'];
// Data an outsider controls. Plan-c goes further: no `${{ }}` at all inside `run:` (see below).
const SHA_REF = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+(\/[^@\s]+)?@[0-9a-f]{40}$/;
const CHECKOUT = /^actions\/checkout@/;
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

/** Raw-text rules (they also see comments, on purpose). */
function checkText(text, file, problems) {
  for (const { rule, pattern, what } of FORBIDDEN_TEXT) {
    if (pattern.test(text)) problems.push(`${file}: ${rule}: ${what} must never appear`);
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
function checkSteps(steps, where, file, problems) {
  if (!Array.isArray(steps)) return;
  steps.forEach((step, i) => {
    if (!isMap(step)) return;
    const at = `${where}.steps[${i}]${step.name ? ` (${step.name})` : ''}`;
    if ('continue-on-error' in step) {
      problems.push(`${file}: continue-on-error: ${at} must not declare it`);
    }
    if (typeof step.uses === 'string') {
      checkUses(step.uses, at, file, problems);
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
  });
}

function checkUses(uses, at, file, problems) {
  if (uses.startsWith('./')) return;
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
function checkWorkflow(doc, name, file, problems) {
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
    if (typeof job.uses === 'string') checkUses(job.uses, where, file, problems);
    checkSteps(job.steps, where, file, problems);
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

/** @param {any} doc */
function checkAction(doc, file, problems) {
  const runs = isMap(doc.runs) ? doc.runs : {};
  if (runs.using !== 'composite') {
    problems.push(`${file}: composite: local actions must be composite, got ${show(runs.using)}`);
  }
  checkSteps(runs.steps, 'runs', file, problems);
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
  if (workflows.length === 0) problems.push(`${WORKFLOWS_DIR}: exists: no workflow found`);
  for (const name of Object.keys(JOB_PERMISSIONS)) {
    if (!workflows.includes(`${WORKFLOWS_DIR}/${name}`)) {
      problems.push(`${WORKFLOWS_DIR}/${name}: exists: missing`);
    }
  }

  for (const file of [...workflows, ...actions]) {
    const text = tree.read(file);
    if (text === null) continue;
    checkText(text, file, problems);
    const parsed = parseYaml(text, file, problems);
    if (!parsed) continue;
    checkSecrets(parsed.document, file, problems);
    if (workflows.includes(file))
      checkWorkflow(parsed.doc, path.posix.basename(file), file, problems);
    else checkAction(parsed.doc, file, problems);
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
