import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { parse } from 'yaml';
import { describe, expect, it } from 'vitest';
import { checkWorkflows, loadTree } from '../check-workflows.mjs';

// Runs only in the CI job `policy` (scripts/vitest.policy.config.mjs), never in `web` (RF-C10).
const valid = loadTree(
  fileURLToPath(new URL('../__fixtures__/check-workflows/valid/', import.meta.url)),
);

const CI = '.github/workflows/ci.yml';
const REL = '.github/workflows/release.yml';
const ACT = '.github/actions/setup/action.yml';

/**
 * The valid fixture with one edit per rule. Each edit asserts that the text it replaces exists,
 * so a drift of the valid fixture breaks the test instead of hiding a rule.
 * @param {Record<string, [string, string][]>} edits file → [from, to] replacements
 */
function mutated(edits) {
  /** @type {Record<string, string>} */
  const changed = {};
  for (const [file, pairs] of Object.entries(edits)) {
    let text = valid.read(file);
    if (text === null) throw new Error(`fixture has no ${file}`);
    for (const [from, to] of pairs) {
      if (!text.includes(from)) throw new Error(`fixture ${file} has no ${JSON.stringify(from)}`);
      text = text.replace(from, to);
    }
    changed[file] = text;
  }
  return { read: (rel) => (rel in changed ? changed[rel] : valid.read(rel)), files: valid.files };
}

const POLICY_PERMS =
  '    permissions:\n      contents: read\n    steps:\n      - uses: actions/checkout';
const IMAGES_PERMS =
  '  images:\n    needs: changes\n    runs-on: ubuntu-24.04\n    timeout-minutes: 25\n    permissions:\n      contents: read\n';
const CHECKOUT_WITH =
  'with:\n          persist-credentials: false\n      - uses: ./.github/actions/setup';
const RUN_STEP = '      - run: echo rust\n';
const DEPLOY_ENV = '    environment: production\n';
const DEPLOY_PERMS = '    environment: production\n    permissions: {}\n';

/** @type {[string, Record<string, [string, string][]>, RegExp][]} */
const CASES = [
  // Triggers
  [
    'pull_request_target',
    { [CI]: [['  pull_request:\n', '  pull_request_target:\n']] },
    /forbidden-trigger: pull_request_target/,
  ],
  [
    'workflow_run',
    {
      [REL]: [
        ['  workflow_dispatch:\n', '  workflow_run:\n    workflows: [ci]\n  workflow_dispatch:\n'],
      ],
    },
    /forbidden-trigger: workflow_run/,
  ],
  // The four syntaxes of secrets, plus `if:` and the case of the context
  [
    'secrets.X',
    { [CI]: [['${{ secrets.GITHUB_TOKEN }}', '${{ secrets.NPM_TOKEN }}']] },
    /secrets: only secrets\.GITHUB_TOKEN/,
  ],
  [
    "secrets['X']",
    { [CI]: [['${{ secrets.GITHUB_TOKEN }}', "${{ secrets['NPM_TOKEN'] }}"]] },
    /secrets: only/,
  ],
  [
    'toJSON(secrets)',
    { [CI]: [['${{ secrets.GITHUB_TOKEN }}', '${{ toJSON(secrets) }}']] },
    /secrets: only/,
  ],
  [
    'secrets: inherit',
    { [REL]: [[DEPLOY_PERMS, `${DEPLOY_PERMS}    secrets: inherit\n`]] },
    /secrets: only.*secrets: \(key\)/,
  ],
  [
    'secrets in if: without ${{ }}',
    { [CI]: [["if: needs.changes.outputs.web == 'true'", "if: secrets.NPM_TOKEN != ''"]] },
    /secrets: only/,
  ],
  [
    'SECRETS in upper case',
    { [CI]: [['${{ secrets.GITHUB_TOKEN }}', '${{ SECRETS.NPM_TOKEN }}']] },
    /secrets: only/,
  ],
  [
    'secrets in a composite action',
    { [ACT]: [['${{ inputs.filter }}', '${{ secrets.NPM_TOKEN }}']] },
    /action\.yml: secrets: only/,
  ],
  // SHA pinning
  [
    'action by tag',
    {
      [CI]: [
        ['dorny/paths-filter@ceb8a2b8f2d89434be7ff52d3de7ec3738c5cc9d', 'dorny/paths-filter@v4'],
      ],
    },
    /sha-pin: .*dorny\/paths-filter@v4/,
  ],
  [
    'action by short SHA',
    {
      [CI]: [
        [
          'dorny/paths-filter@ceb8a2b8f2d89434be7ff52d3de7ec3738c5cc9d',
          'dorny/paths-filter@ceb8a2b',
        ],
      ],
    },
    /sha-pin:/,
  ],
  [
    'unpinned action in a composite action',
    {
      [ACT]: [
        ['actions/setup-node@949feb2413d6458794dcd2491c4babbbce0c15c1', 'actions/setup-node@main'],
      ],
    },
    /action\.yml: sha-pin:/,
  ],
  ['docker:// reference', { [CI]: [[RUN_STEP, '      - uses: docker://alpine:3\n']] }, /sha-pin:/],
  // Root permissions
  [
    'root permissions missing',
    { [CI]: [['permissions:\n  contents: read\njobs:', 'jobs:']] },
    /ci\.yml: permissions-root: must be exactly/,
  ],
  [
    'root contents: write',
    {
      [REL]: [
        [
          'permissions:\n  contents: read\nconcurrency:',
          'permissions:\n  contents: write\nconcurrency:',
        ],
      ],
    },
    /release\.yml: permissions-root:/,
  ],
  [
    'root packages: write',
    {
      [REL]: [
        [
          'permissions:\n  contents: read\nconcurrency:',
          'permissions:\n  contents: read\n  packages: write\nconcurrency:',
        ],
      ],
    },
    /release\.yml: permissions-root:/,
  ],
  [
    'write-all at the root',
    { [CI]: [['permissions:\n  contents: read\njobs:', 'permissions: write-all\njobs:']] },
    /permissions: the root uses write-all/,
  ],
  // Job permissions (plan-c decision 23)
  [
    'read-all in a job',
    {
      [CI]: [
        [POLICY_PERMS, '    permissions: read-all\n    steps:\n      - uses: actions/checkout'],
      ],
    },
    /permissions: jobs\.policy uses read-all/,
  ],
  [
    'id-token: write in a job',
    {
      [CI]: [
        [
          POLICY_PERMS,
          '    permissions:\n      contents: read\n      id-token: write\n    steps:\n      - uses: actions/checkout',
        ],
      ],
    },
    /permissions: jobs\.policy declares id-token/,
  ],
  [
    'id-token: none in a job',
    { [REL]: [[DEPLOY_PERMS, `${DEPLOY_ENV}    permissions:\n      id-token: none\n`]] },
    /permissions: jobs\.deploy declares id-token/,
  ],
  [
    'packages: write in ci.yml/images',
    {
      [CI]: [
        [
          IMAGES_PERMS,
          IMAGES_PERMS.replace(
            '      contents: read\n',
            '      contents: read\n      packages: write\n',
          ),
        ],
      ],
    },
    /permissions-job: jobs\.images must be exactly \{"contents":"read"\}/,
  ],
  [
    'deploy with contents: read instead of {}',
    { [REL]: [[DEPLOY_PERMS, `${DEPLOY_ENV}    permissions:\n      contents: read\n`]] },
    /permissions-job: jobs\.deploy must be exactly \{\}/,
  ],
  [
    'changes without pull-requests: read',
    { [CI]: [['      contents: read\n      pull-requests: read\n', '      contents: read\n']] },
    /permissions-job: jobs\.changes must be exactly/,
  ],
  [
    'changes with pull-requests: write',
    { [CI]: [['pull-requests: read', 'pull-requests: write']] },
    /permissions-job: jobs\.changes/,
  ],
  [
    'job without permissions',
    { [CI]: [[POLICY_PERMS, '    steps:\n      - uses: actions/checkout']] },
    /permissions-job: jobs\.policy must declare permissions/,
  ],
  [
    'job outside the table',
    {
      [CI]: [
        [
          '  ci-ok:\n',
          '  lint:\n    runs-on: ubuntu-24.04\n    timeout-minutes: 5\n    permissions: {}\n    steps:\n      - run: echo lint\n  ci-ok:\n',
        ],
      ],
    },
    /permissions-job: jobs\.lint is not in the permissions table/,
  ],
  [
    'job of the table missing',
    { [REL]: [['  verify:\n', '  check:\n']] },
    /permissions-job: job verify is missing/,
  ],
  // timeout-minutes and runs-on
  [
    'job without timeout-minutes',
    {
      [CI]: [
        [
          '    timeout-minutes: 20\n    permissions:\n      contents: read\n    steps:\n      - run: echo rust',
          '    permissions:\n      contents: read\n    steps:\n      - run: echo rust',
        ],
      ],
    },
    /timeout-minutes: jobs\.rust/,
  ],
  [
    'runs-on macos',
    {
      [CI]: [
        [
          '  e2e:\n    needs: changes\n    runs-on: ubuntu-24.04',
          '  e2e:\n    needs: changes\n    runs-on: macos-15',
        ],
      ],
    },
    /runs-on: jobs\.e2e/,
  ],
  [
    'runs-on self-hosted list',
    {
      [CI]: [
        [
          '  e2e:\n    needs: changes\n    runs-on: ubuntu-24.04',
          '  e2e:\n    needs: changes\n    runs-on: [self-hosted]',
        ],
      ],
    },
    /runs-on: jobs\.e2e/,
  ],
  // continue-on-error
  [
    'continue-on-error: true in a step',
    { [CI]: [[RUN_STEP, '      - run: echo rust\n        continue-on-error: true\n']] },
    /continue-on-error: jobs\.rust\.steps\[0\]/,
  ],
  [
    'continue-on-error: false in a job',
    {
      [CI]: [
        [
          '  e2e:\n    needs: changes\n',
          '  e2e:\n    needs: changes\n    continue-on-error: false\n',
        ],
      ],
    },
    /continue-on-error: jobs\.e2e must not/,
  ],
  [
    'continue-on-error in a composite step',
    { [ACT]: [['    - shell: bash\n', '    - shell: bash\n      continue-on-error: true\n']] },
    /action\.yml: continue-on-error:/,
  ],
  // checkout
  [
    'checkout without persist-credentials',
    {
      [CI]: [
        [CHECKOUT_WITH, 'with:\n          fetch-depth: 1\n      - uses: ./.github/actions/setup'],
      ],
    },
    /persist-credentials: jobs\.policy\.steps\[0\]/,
  ],
  [
    'checkout with persist-credentials: true',
    {
      [CI]: [
        [
          CHECKOUT_WITH,
          'with:\n          persist-credentials: true\n      - uses: ./.github/actions/setup',
        ],
      ],
    },
    /persist-credentials: jobs\.policy/,
  ],
  // ${{ }} inside run:
  [
    'github.event in run:',
    { [CI]: [['run: echo "$HEAD_REF"', 'run: echo "${{ github.event.pull_request.title }}"']] },
    /run-expression: jobs\.web\.steps\[0\]/,
  ],
  [
    'github.head_ref in run:',
    { [CI]: [['run: echo "$HEAD_REF"', 'run: echo "${{ github.head_ref }}"']] },
    /run-expression:/,
  ],
  [
    'inputs.tag in run:',
    { [REL]: [['run: echo "$INPUT_TAG"', 'run: echo "${{ inputs.tag }}"']] },
    /release\.yml: run-expression: jobs\.build/,
  ],
  [
    'github.ref_name in a block run:',
    { [REL]: [['          fi\n', '          fi\n          echo ${{ github.ref_name }}\n']] },
    /run-expression: jobs\.deploy/,
  ],
  [
    'inputs in a composite run:',
    { [ACT]: [['run: echo "$FILTER"', 'run: echo "${{ inputs.filter }}"']] },
    /action\.yml: run-expression:/,
  ],
  // Deployment platform and auto-merge (raw text, comments included)
  [
    'platform name in a comment',
    { [REL]: [['# Minimal valid', '# Coolify: minimal valid']] },
    /deploy-platform: the deployment platform name/,
  ],
  [
    'DEPLOY_MODE=api message naming the platform',
    { [REL]: [['requires a new OK from the owner', 'calls the COOLIFY API']] },
    /deploy-platform:/,
  ],
  [
    'deploy endpoint',
    { [REL]: [['echo verify', 'curl https://example.com/api/v1/deploy?uuid=x']] },
    /deploy-platform: \/api\/v1\/deploy/,
  ],
  ['gh pr merge', { [CI]: [['echo e2e', 'gh pr merge --auto']] }, /auto-merge: gh pr merge/],
  [
    'enable-pull-request-automerge',
    {
      [CI]: [
        [
          RUN_STEP,
          '      - uses: peter-evans/enable-pull-request-automerge@0123456789012345678901234567890123456789\n',
        ],
      ],
    },
    /auto-merge: enable-pull-request-automerge/,
  ],
  [
    'dependabot/fetch-metadata',
    {
      [CI]: [
        [
          RUN_STEP,
          '      - uses: dependabot/fetch-metadata@0123456789012345678901234567890123456789\n',
        ],
      ],
    },
    /auto-merge: dependabot\/fetch-metadata/,
  ],
  // deploy environment
  ['deploy without environment', { [REL]: [[DEPLOY_ENV, '']] }, /deploy-environment:/],
  [
    'deploy on another environment',
    { [REL]: [[DEPLOY_ENV, '    environment: staging\n']] },
    /deploy-environment:/,
  ],
  // policy and ci-ok
  [
    'policy with if:',
    { [CI]: [['  policy:\n', "  policy:\n    if: github.event_name == 'push'\n"]] },
    /policy-always:/,
  ],
  [
    'ci-ok without one need',
    {
      [CI]: [
        [
          'needs: [changes, policy, rust, web, e2e, images]',
          'needs: [changes, rust, web, e2e, images]',
        ],
      ],
    },
    /ci-ok: jobs\.ci-ok must need every other job/,
  ],
  [
    'ci-ok without always()',
    { [CI]: [['    if: always()\n', '    if: success()\n']] },
    /ci-ok: jobs\.ci-ok must run with if: always\(\)/,
  ],
  // YAML hardening
  [
    'anchor and alias',
    {
      [CI]: [
        ['permissions:\n  contents: read\njobs:', 'permissions: &read\n  contents: read\njobs:'],
        [POLICY_PERMS, '    permissions: *read\n    steps:\n      - uses: actions/checkout'],
      ],
    },
    /yaml-anchors:/,
  ],
  [
    'explicit tag',
    { [CI]: [['    timeout-minutes: 15\n', '    timeout-minutes: !!int 15\n']] },
    /yaml-tags:/,
  ],
  // Composite actions
  [
    'non-composite local action',
    { [ACT]: [['  using: composite', '  using: node24']] },
    /composite: local actions must be composite/,
  ],
];

const LOCAL_USES = '      - uses: ./.github/actions/setup\n';
const RUST_JOB = '  rust:\n    needs: changes\n';

/**
 * Review of PR #5 (security-reviewer, qa-engineer): each case failed to fail before the fix.
 * @type {[string, Record<string, [string, string][]>, RegExp][]}
 */
const REVIEW_CASES = [
  // Local actions outside .github/actions/ are never analysed, so they must not be usable.
  [
    'local action ./tools/evil',
    { [CI]: [[LOCAL_USES, '      - uses: ./tools/evil\n']] },
    /local-action: .*\.\/tools\/evil/,
  ],
  ['local action ./', { [CI]: [[LOCAL_USES, '      - uses: ./\n']] }, /local-action:/],
  [
    'local action with ..',
    { [CI]: [[LOCAL_USES, '      - uses: ./.github/actions/../x\n']] },
    /local-action:/,
  ],
  [
    'local action escaping the repo',
    { [CI]: [[LOCAL_USES, '      - uses: ./.github/actions/../../tools/x\n']] },
    /local-action:/,
  ],
  [
    'local action that does not exist',
    { [CI]: [[LOCAL_USES, '      - uses: ./.github/actions/missing\n']] },
    /local-action: .*missing.*has no action\.yml/,
  ],
  [
    'local action in upper case',
    { [CI]: [[LOCAL_USES, '      - uses: ./.github/actions/Setup\n']] },
    /local-action:/,
  ],
  [
    'local action used from a composite action',
    { [ACT]: [['    - shell: bash\n', '    - uses: ./tools/evil\n    - shell: bash\n']] },
    /action\.yml: local-action:/,
  ],
  // Checkout in any case
  [
    'Actions/checkout without persist-credentials',
    {
      [CI]: [
        [
          '      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1\n        with:\n          persist-credentials: false\n      - uses: ./.github',
          '      - uses: Actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1\n      - uses: ./.github',
        ],
      ],
    },
    /persist-credentials: jobs\.policy/,
  ],
  // container: and services: by digest
  [
    'container by tag',
    { [CI]: [[RUST_JOB, `${RUST_JOB}    container: node:latest\n`]] },
    /container-image: jobs\.rust\.container/,
  ],
  [
    'container map by tag',
    { [CI]: [[RUST_JOB, `${RUST_JOB}    container:\n      image: node:22\n`]] },
    /container-image: jobs\.rust\.container/,
  ],
  [
    'service by tag',
    { [CI]: [[RUST_JOB, `${RUST_JOB}    services:\n      x:\n        image: evil/img:latest\n`]] },
    /container-image: jobs\.rust\.services\.x/,
  ],
  // ${{ }} inside with.script (actions that evaluate code)
  [
    'expression in with.script',
    {
      [CI]: [
        [
          RUN_STEP,
          '      - uses: actions/github-script@0123456789012345678901234567890123456789\n        with:\n          script: console.log("${{ github.event.pull_request.title }}")\n',
        ],
      ],
    },
    /script-expression: jobs\.rust\.steps\[0\]/,
  ],
  // Text rules on parsed values: a YAML escape must not hide the text
  [
    'escaped deploy endpoint',
    {
      [REL]: [['      - run: echo verify\n', '      - run: "curl https://x/api/v1/deplo\\x79"\n']],
    },
    /deploy-platform: \/api\/v1\/deploy/,
  ],
  [
    'escaped platform name',
    { [REL]: [['      - run: echo verify\n', '      - run: "echo \\x43oolify"\n']] },
    /deploy-platform: the deployment platform name/,
  ],
];

describe('check-workflows (RF-C12, plan-c decision 23)', () => {
  it('passes on the valid fixture', () => {
    expect(checkWorkflows(valid)).toEqual([]);
  });

  it('passes on the real repository', () => {
    expect(checkWorkflows(loadTree(fileURLToPath(new URL('../../', import.meta.url))))).toEqual([]);
  });

  it('accepts environment: { name: production } on deploy', () => {
    const tree = mutated({ [REL]: [[DEPLOY_ENV, '    environment:\n      name: production\n']] });
    expect(checkWorkflows(tree)).toEqual([]);
  });

  it('accepts ${{ }} outside run: (env, with, outputs, if)', () => {
    const problems = checkWorkflows(valid);
    expect(valid.read(CI)).toContain('HEAD_REF: ${{ github.head_ref }}');
    expect(problems).toEqual([]);
  });

  it('fails when a workflow of the table is missing', () => {
    const tree = { read: valid.read, files: valid.files.filter((f) => f !== REL) };
    expect(checkWorkflows(tree)).toContain(`${REL}: exists: missing`);
  });

  it.each(CASES)('fails on: %s', (_name, edits, pattern) => {
    const problems = checkWorkflows(mutated(edits));
    expect(
      problems.some((p) => pattern.test(p)),
      problems.join('\n') || '(no problems)',
    ).toBe(true);
  });

  it.each(REVIEW_CASES)('fails on: %s', (_name, edits, pattern) => {
    const problems = checkWorkflows(mutated(edits));
    expect(
      problems.some((p) => pattern.test(p)),
      problems.join('\n') || '(no problems)',
    ).toBe(true);
  });

  it('accepts container: and services: pinned by digest', () => {
    const digest = `@sha256:${'a'.repeat(64)}`;
    const tree = mutated({
      [CI]: [
        [
          RUST_JOB,
          `${RUST_JOB}    container: node:22${digest}\n    services:\n      pg:\n        image: postgres:17${digest}\n`,
        ],
      ],
    });
    expect(checkWorkflows(tree)).toEqual([]);
  });
});

const ROOT_URL = new URL('../../', import.meta.url);

/** Recipes of the justfile: name → { deps, body }. */
function justRecipes(text) {
  /** @type {Record<string, { deps: string[], body: string[] }>} */
  const recipes = {};
  let current = null;
  for (const line of text.split('\n')) {
    const header = /^([A-Za-z_][\w-]*)[^:=]*:(?!=)\s*(.*)$/.exec(line);
    if (header && !line.includes(':=')) {
      current = {
        deps: header[2].split(/\s+/).filter((d) => /^[A-Za-z_][\w-]*$/.test(d)),
        body: [],
      };
      recipes[header[1]] = current;
    } else if (current && /^\s+\S/.test(line)) {
      current.body.push(line);
    } else if (!/^\s*$/.test(line)) {
      current = null;
    }
  }
  return recipes;
}

/** Every scripts/... path a set of recipes runs, following dependencies and `just X` calls. */
function scriptsOfRecipes(recipes, start) {
  const seen = new Set();
  const scripts = new Set();
  const visit = (name) => {
    if (seen.has(name) || !recipes[name]) return;
    seen.add(name);
    for (const dep of recipes[name].deps) visit(dep);
    for (const line of recipes[name].body) {
      for (const m of line.matchAll(/scripts\/[\w./-]+/g)) scripts.add(m[0]);
      for (const m of line.matchAll(/\{\{just_executable\(\)\}\}\s+([\w-]+)/g)) visit(m[1]);
    }
  };
  start.forEach(visit);
  return scripts;
}

const globToRegExp = (glob) =>
  new RegExp(
    `^${glob
      .replace(/[.+^${}()|[\]\\]/g, '\\$&')
      .replace(/\*\*/g, '\u0000')
      .replace(/\*/g, '[^/]*')
      .replace(/\u0000/g, '.*')}$`,
  );

describe('ci.yml filters cover what each job runs (review of PR #5)', () => {
  const ci = parse(readFileSync(new URL('.github/workflows/ci.yml', ROOT_URL), 'utf8'));
  const recipes = justRecipes(readFileSync(new URL('justfile', ROOT_URL), 'utf8'));
  const filterStep = ci.jobs.changes.steps.find((step) => step.id === 'filter');
  const filters = parse(filterStep.with.filters);
  const jobs = Object.entries(ci.jobs)
    .map(([id, job]) => [
      id,
      job,
      /needs\.changes\.outputs\.(\w+) == 'true'/.exec(job.if ?? '')?.[1],
    ])
    .filter(([, , filter]) => filter);

  it('finds the gated jobs', () => {
    expect(jobs.map(([id]) => id).sort()).toEqual(['e2e', 'images', 'rust', 'web']);
  });

  it.each(jobs)('every script that %s runs is in its filter', (_id, job, filter) => {
    const globs = filters[filter].flat(Infinity).map(globToRegExp);
    const runs = job.steps.map((step) => step.run ?? '').join('\n');
    const called = [...runs.matchAll(/\bjust\s+([\w-]+)/g)].map((m) => m[1]);
    const scripts = scriptsOfRecipes(recipes, called);
    for (const m of runs.matchAll(/scripts\/[\w./-]+/g)) scripts.add(m[0]);
    const missing = [...scripts].filter((script) => !globs.some((g) => g.test(script)));
    expect(missing, `add to the ${filter} filter of ci.yml`).toEqual([]);
  });
});
