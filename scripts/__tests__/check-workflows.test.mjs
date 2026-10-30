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
          '  images:\n    needs: changes\n    runs-on: ubuntu-24.04',
          '  images:\n    needs: changes\n    runs-on: macos-15',
        ],
      ],
    },
    /runs-on: jobs\.images/,
  ],
  [
    'runs-on self-hosted list',
    {
      [CI]: [
        [
          '  images:\n    needs: changes\n    runs-on: ubuntu-24.04',
          '  images:\n    needs: changes\n    runs-on: [self-hosted]',
        ],
      ],
    },
    /runs-on: jobs\.images/,
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
          '  images:\n    needs: changes\n',
          '  images:\n    needs: changes\n    continue-on-error: false\n',
        ],
      ],
    },
    /continue-on-error: jobs\.images must not/,
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
          'needs: [changes, policy, rust, web, e2e-server, e2e, images]',
          'needs: [changes, rust, web, e2e-server, e2e, images]',
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

const DIGEST = `@sha256:${'a'.repeat(64)}`;
const BASH = '    defaults:\n      run:\n        shell: bash\n';
/** The rust job of the valid fixture inside a pinned container, with bash, plus `extra`. */
const inContainer = (extra = '', options = '') => ({
  [CI]: [
    [
      RUST_JOB,
      `${RUST_JOB}    container:\n      image: node:22${DIGEST}\n${options ? `      options: ${options}\n` : ''}${extra}${BASH}`,
    ],
  ],
});

/**
 * Review of PR #22 (qa-engineer, security-reviewer): container jobs.
 * @type {[string, Record<string, [string, string][]>, RegExp][]}
 */
const CONTAINER_CASES = [
  // Inside a container, run: defaults to `sh -e`, so `set -o pipefail` exits 2 (qa-engineer).
  [
    'container without shell: bash',
    { [CI]: [[RUST_JOB, `${RUST_JOB}    container: node:22${DIGEST}\n`]] },
    /container-shell: jobs\.rust/,
  ],
  [
    'container with shell: sh',
    {
      [CI]: [
        [
          RUST_JOB,
          `${RUST_JOB}    container: node:22${DIGEST}\n    defaults:\n      run:\n        shell: sh\n`,
        ],
      ],
    },
    /container-shell: jobs\.rust/,
  ],
  // Closed list of container options (security-reviewer).
  [
    '--privileged',
    inContainer('', '--privileged'),
    /container-options: jobs\.rust\.container: --privileged/,
  ],
  ['a mount of /', inContainer('', '-v /:/host'), /container-options: jobs\.rust\.container: -v/],
  [
    '--volume',
    inContainer('', '--volume=/var/run/docker.sock:/var/run/docker.sock'),
    /container-options:/,
  ],
  ['--cap-add', inContainer('', '--cap-add=SYS_ADMIN'), /container-options: .*--cap-add/],
  ['--ipc=host', inContainer('', '--ipc=host'), /container-options: .*--ipc=host/],
  ['--network host', inContainer('', '--network host'), /container-options:/],
  ['--user 0', inContainer('', '--user 0'), /container-options:/],
  ['--security-opt', inContainer('', '--security-opt seccomp=unconfined'), /container-options:/],
  ['--shm-size without a size', inContainer('', '--shm-size=lots'), /container-options:/],
  [
    'options in a service',
    {
      [CI]: [
        [
          RUST_JOB,
          `${RUST_JOB}    services:\n      pg:\n        image: postgres:17${DIGEST}\n        options: --privileged\n`,
        ],
      ],
    },
    /container-options: jobs\.rust\.services\.pg: --privileged/,
  ],
  // Closed list of keys: no credentials, no volumes.
  [
    'container credentials',
    inContainer('      credentials:\n        username: x\n        password: y\n'),
    /container-keys: jobs\.rust\.container: credentials/,
  ],
  [
    'container volumes',
    inContainer('      volumes:\n        - /:/host\n'),
    /container-keys: jobs\.rust\.container: volumes/,
  ],
  [
    'service volumes',
    {
      [CI]: [
        [
          RUST_JOB,
          `${RUST_JOB}    services:\n      pg:\n        image: postgres:17${DIGEST}\n        volumes:\n          - /:/host\n`,
        ],
      ],
    },
    /container-keys: jobs\.rust\.services\.pg: volumes/,
  ],
  [
    'service credentials',
    {
      [CI]: [
        [
          RUST_JOB,
          `${RUST_JOB}    services:\n      pg:\n        image: postgres:17${DIGEST}\n        credentials:\n          username: x\n`,
        ],
      ],
    },
    /container-keys: jobs\.rust\.services\.pg: credentials/,
  ],
];

const PW_IMAGE = `mcr.microsoft.com/playwright:v1.64.1-noble${DIGEST}`;
/** The rust job inside the Playwright image, with bash, and `env` lines for the job. */
const inPlaywright = (env) => ({
  [CI]: [
    [
      RUST_JOB,
      `${RUST_JOB}    container:\n      image: ${PW_IMAGE}\n${BASH}${env ? `    env:\n      ${env}\n` : ''}`,
    ],
  ],
});

/**
 * Review of PR #22 (architect): the Playwright image tag, PLAYWRIGHT_IMAGE_VERSION and
 * @playwright/test move together.
 * @type {[string, Record<string, [string, string][]>, RegExp][]}
 */
const PLAYWRIGHT_CASES = [
  ['PLAYWRIGHT_IMAGE_VERSION missing', inPlaywright(''), /playwright-image: jobs\.rust/],
  [
    'PLAYWRIGHT_IMAGE_VERSION different from the tag',
    inPlaywright('PLAYWRIGHT_IMAGE_VERSION: 1.65.0'),
    /playwright-image: jobs\.rust: PLAYWRIGHT_IMAGE_VERSION 1\.65\.0 .*v1\.64\.1/,
  ],
  [
    'PLAYWRIGHT_IMAGE_VERSION without the Playwright image',
    { [CI]: [[RUST_JOB, `${RUST_JOB}    env:\n      PLAYWRIGHT_IMAGE_VERSION: 1.64.1\n`]] },
    /playwright-image: jobs\.rust/,
  ],
];

// 008 plan-d decision 12 (step 9): the e2e-server job, the artifact and the Postgres service.
const E2E_SERVER_IF =
  "  e2e-server:\n    needs: changes\n    if: needs.changes.outputs.e2e == 'true'\n";
const E2E_SERVER_ENV = '        env:\n          SOMETHING: else\n';
const DOWNLOAD = '          name: server-dev-auth\n          path: bin\n';
const PG_IMAGE =
  'postgres:17.10-alpine@sha256:742f40ea20b9ff2ff31db5458d127452988a2164df9e17441e191f3b72252193';
const COMPOSE_DEV = 'infra/compose.dev.yml';

/** @type {[string, Record<string, [string, string][]>, RegExp][]} */
const E2E_CASES = [
  [
    'e2e-server outside ci-ok',
    {
      [CI]: [
        [
          '[changes, policy, rust, web, e2e-server, e2e, images]',
          '[changes, policy, rust, web, e2e, images]',
        ],
      ],
    },
    /ci-ok: jobs\.ci-ok must need every other job/,
  ],
  [
    'e2e-server with another if',
    {
      [CI]: [
        [
          E2E_SERVER_IF,
          "  e2e-server:\n    needs: changes\n    if: needs.changes.outputs.web == 'true'\n",
        ],
      ],
    },
    /e2e-server-if: jobs\.e2e-server must run with the same if: as jobs\.e2e/,
  ],
  [
    'e2e without the e2e filter',
    {
      [CI]: [
        [
          "    needs: [changes, e2e-server]\n    if: needs.changes.outputs.e2e == 'true'\n",
          "    needs: [changes, e2e-server]\n    if: needs.changes.outputs.web == 'true'\n",
        ],
      ],
    },
    /e2e-server-if: jobs\.e2e must run with if: needs\.changes\.outputs\.e2e == 'true'/,
  ],
  [
    'e2e-server with a CARGO_ variable in a step',
    { [CI]: [[E2E_SERVER_ENV, '        env:\n          CARGO_INCREMENTAL: 0\n']] },
    /e2e-server-env: jobs\.e2e-server.*CARGO_INCREMENTAL/,
  ],
  [
    'e2e-server with a RUST variable in the job',
    {
      [CI]: [[E2E_SERVER_IF, `${E2E_SERVER_IF}    env:\n      RUSTFLAGS: -Dwarnings\n`]],
    },
    /e2e-server-env: jobs\.e2e-server.*RUSTFLAGS/,
  ],
  [
    'download-artifact with run-id',
    { [CI]: [[DOWNLOAD, `${DOWNLOAD}          run-id: 123\n`]] },
    /download-artifact: .*run-id/,
  ],
  [
    'download-artifact with github-token',
    { [CI]: [[DOWNLOAD, `${DOWNLOAD}          github-token: x\n`]] },
    /download-artifact: .*github-token/,
  ],
  [
    'download-artifact with repository',
    { [CI]: [[DOWNLOAD, `${DOWNLOAD}          repository: someone/else\n`]] },
    /download-artifact: .*repository/,
  ],
  [
    'download-artifact without a name',
    { [CI]: [[DOWNLOAD, '          path: bin\n']] },
    /download-artifact: .*by name/,
  ],
  [
    'Postgres service with another digest',
    { [CI]: [[PG_IMAGE, `postgres:17.10-alpine@sha256:${'b'.repeat(64)}`]] },
    /postgres-digest: jobs\.e2e\.services\.postgres/,
  ],
  [
    'Postgres service with another tag',
    { [CI]: [[PG_IMAGE, PG_IMAGE.replace('17.10', '17.11')]] },
    /postgres-digest:/,
  ],
  [
    'compose.dev.yml with another digest',
    { [COMPOSE_DEV]: [[PG_IMAGE, `postgres:17.10-alpine@sha256:${'c'.repeat(64)}`]] },
    /postgres-digest:/,
  ],
];

// ADR-0012 B6.4 item 7 (2026-10-30 amendment): the b1= of images and of verify, once each and
// identical byte for byte.
const B1_LINE =
  "          b1=\"default-src 'self'; script-src 'self' 'nonce-*'; form-action 'self' https://accounts.google.com/o/oauth2/v2/auth https://discord.com/oauth2/authorize; frame-ancestors 'none'\"\n";
const VERIFY_HEADERS =
  '      - name: Security headers (ADR-0012 B2)\n        run: |\n          # ADR-0012 B1 plus B4.6.\n';
const IMAGES_HEADERS =
  '      - name: Health, version and headers (RF-C7)\n        run: |\n          # ADR-0012 B1 plus B4.6.\n';

/** @type {[string, Record<string, [string, string][]>, RegExp][]} */
const CSP_CASES = [
  [
    'the b1= of release.yml differs from the one of images',
    { [REL]: [[B1_LINE, B1_LINE.replace("form-action 'self' ", "form-action 'self' https: ")]] },
    /csp-b1: the b1= of jobs\.images .* and of jobs\.verify .* must be identical byte for byte/,
  ],
  [
    'the b1= of ci.yml differs only in a space',
    { [CI]: [[B1_LINE, B1_LINE.replace("'self'; script-src", "'self';  script-src")]] },
    /csp-b1: .* must be identical byte for byte/,
  ],
  [
    'verify without b1=',
    { [REL]: [[B1_LINE, '']] },
    /release\.yml: csp-b1: jobs\.verify .*Security headers \(ADR-0012 B2\).* must set b1= exactly once, found 0/,
  ],
  [
    'images without b1=',
    { [CI]: [[B1_LINE, '']] },
    /ci\.yml: csp-b1: jobs\.images .*Health, version and headers \(RF-C7\).* must set b1= exactly once, found 0/,
  ],
  [
    'b1= twice in images',
    { [CI]: [[B1_LINE, B1_LINE + B1_LINE]] },
    /ci\.yml: csp-b1: jobs\.images .* found 2/,
  ],
  [
    'the headers step of verify renamed',
    { [REL]: [[VERIFY_HEADERS, VERIFY_HEADERS.replace('(ADR-0012 B2)', '(B2)')]] },
    /release\.yml: csp-b1: jobs\.verify must have exactly one step named "Security headers \(ADR-0012 B2\)", found 0/,
  ],
  [
    'the headers step of images twice',
    { [CI]: [[IMAGES_HEADERS, `${IMAGES_HEADERS}${B1_LINE}      - name: x\n${IMAGES_HEADERS}`]] },
    /ci\.yml: csp-b1: jobs\.images must have exactly one step named "Health, version and headers \(RF-C7\)", found 2/,
  ],
];

// 008 RF-D9, plan-d decision 12: the release binary check.
const CHECK_BUILD = '      - name: Build server for the binary check (not pushed)\n';
const PUSH_BUILD = '      - name: Build and push server\n';
const CHECK_STEP =
  '      - name: The release binary has no dev-auth\n        if: github.event_name == \'push\'\n        run: node scripts/check-release-binary.mjs "$RUNNER_TEMP/server"\n';
const DIGEST_STEP_NAME = '      - name: The pushed server is the checked one\n';
const RETENTION_STEP =
  "      - name: Retention of server\n        if: github.event_name == 'push'\n        uses: actions/delete-package-versions@e5bc658cc4c965c472efe991f8beea3981499c55\n        with:\n          package-name: example/server\n";
const LOCAL_TAG = '          tags: torneo-release-check/server:local\n';
const PUSH_TAG = '          tags: ghcr.io/example/server:v1\n';
const MUSL_CHECK =
  '      - name: The musl binary has no dev-auth\n        run: node scripts/check-release-binary.mjs target/x86_64-unknown-linux-musl/debug/server\n';
const IMAGES_CHECK =
  '      - name: Server binary\n        run: node scripts/check-release-binary.mjs "$RUNNER_TEMP/server"\n';
/** The `with:` of one of the two server builds, located by the line after its tags. */
const inBuild = (tag, from, to) => ({ [REL]: [[`${tag}${from}`, `${tag}${to}`]] });
const LABELS = '          labels: org.opencontainers.image.source=https://example.com/repo\n';
// QA finding 3 of phase D: both RF-D9 steps of the release fail closed.
const DIGEST_RUN = '        run: node scripts/check-pushed-server.mjs\n';
const PUSHED_ENV = '          PUSHED: ${{ steps.server-push.outputs.imageid }}\n';
const CHECK_RUN = '        run: node scripts/check-release-binary.mjs "$RUNNER_TEMP/server"\n';
const CHECK_CMD = 'node scripts/check-release-binary.mjs "$RUNNER_TEMP/server"';
const MUSL_RUN =
  '        run: node scripts/check-release-binary.mjs target/x86_64-unknown-linux-musl/debug/server\n';
const BUILD_JOB = '  build:\n    runs-on: ubuntu-24.04\n';
/** The check step of release.yml as a block run: with the given lines. */
const checkBlock = (...lines) => `        run: |\n${lines.map((l) => `          ${l}\n`).join('')}`;
const DIGEST_AT = String.raw`jobs\.build\.steps\[\d+\] \(The pushed server is the checked one\)`;
const CHECK_AT = String.raw`jobs\.build\.steps\[\d+\] \(The release binary has no dev-auth\)`;

/** @type {[string, Record<string, [string, string][]>, RegExp][]} */
const RELEASE_CASES = [
  [
    'a single server build (the local one builds another Dockerfile)',
    {
      [REL]: [
        ['          file: apps/server/Dockerfile\n', '          file: apps/web/Dockerfile\n'],
      ],
    },
    /release-binary: jobs\.build must build apps\/server\/Dockerfile exactly twice .*found 1/,
  ],
  [
    'a third server build',
    {
      [REL]: [
        [
          RETENTION_STEP,
          `${PUSH_BUILD}        uses: docker/build-push-action@c3c9e263c25d99ce0380d002d59b67737d91b0dc\n        with:\n          file: apps/server/Dockerfile\n${RETENTION_STEP}`,
        ],
      ],
    },
    /release-binary: .*exactly twice .*found 3/,
  ],
  [
    'the check build with a GHCR tag',
    { [REL]: [[LOCAL_TAG, '          tags: ghcr.io/example/server:check\n']] },
    /release-binary: .*the first server build must tag only torneo-release-check\/server:local/,
  ],
  [
    'the check build pushed',
    {
      [REL]: [
        [
          '          push: false\n          load: true\n',
          '          push: true\n          load: true\n',
        ],
      ],
    },
    /release-binary: .*the first server build must have push: false and load: true/,
  ],
  [
    'the push build without push',
    { [REL]: [['          push: true\n', '          push: false\n']] },
    /release-binary: .*the second server build must have push: true/,
  ],
  [
    'different build-args',
    {
      [REL]: [
        [
          `          build-args: APP_VERSION=\${{ inputs.tag }}\n${PUSH_TAG}`,
          `          build-args: APP_VERSION=other\n${PUSH_TAG}`,
        ],
      ],
    },
    /release-binary: .*build-args must be the same in both server builds/,
  ],
  [
    'different labels',
    inBuild(PUSH_TAG, LABELS, '          labels: org.opencontainers.image.source=https://other\n'),
    /release-binary: .*labels must be the same in both server builds/,
  ],
  [
    'cache-from missing in the push',
    inBuild(PUSH_TAG, `${LABELS}          cache-from: type=gha,scope=server\n`, LABELS),
    /release-binary: .*cache-from must be set in both server builds/,
  ],
  [
    'cache-to in a release build',
    inBuild(PUSH_TAG, LABELS, `${LABELS}          cache-to: type=gha,mode=max,scope=server\n`),
    /release-binary: .*cache-to is not allowed in the server builds/,
  ],
  [
    'the two builds with different if',
    {
      [REL]: [
        [
          `${PUSH_BUILD}        id: server-push\n        if: github.event_name == 'push'\n`,
          `${PUSH_BUILD}        id: server-push\n        if: always()\n`,
        ],
      ],
    },
    /release-binary: .*both server builds must have the same if:/,
  ],
  [
    'no binary check between the builds',
    { [REL]: [[CHECK_STEP, '']] },
    /release-binary: .*a step between the two server builds must run scripts\/check-release-binary\.mjs/,
  ],
  [
    'the binary check with --expect-dev',
    { [REL]: [[CHECK_STEP, CHECK_STEP.replace('.mjs "', '.mjs --expect-dev "')]] },
    /release-binary: .*a step between the two server builds must run scripts\/check-release-binary\.mjs/,
  ],
  [
    'the binary check after the push',
    {
      [REL]: [
        [CHECK_STEP, ''],
        [DIGEST_STEP_NAME, `${CHECK_STEP}${DIGEST_STEP_NAME}`],
      ],
    },
    /release-binary: .*a step between the two server builds/,
  ],
  [
    'no imageid of the push in the digest comparison',
    { [REL]: [[PUSHED_ENV, '']] },
    /release-binary: .*\(The pushed server is the checked one\) must set PUSHED: \$\{\{ steps\.server-push\.outputs\.imageid \}\}/,
  ],
  [
    'the digest comparison as inline shell instead of the script',
    {
      [REL]: [
        [
          '        run: node scripts/check-pushed-server.mjs\n',
          '        run: |\n          set -euo pipefail\n          node scripts/check-pushed-server.mjs\n',
        ],
      ],
    },
    /release-binary: jobs\.build\.steps\[\d+\] \(The pushed server is the checked one\) must run exactly node scripts\/check-pushed-server\.mjs/,
  ],
  [
    'the imageid of the local build (the manifest digest with the containerd store)',
    {
      [REL]: [
        [
          '          PUSHED: ${{ steps.server-push.outputs.imageid }}\n',
          '          PUSHED: ${{ steps.server-push.outputs.imageid }}\n          CHECKED: ${{ steps.server-check.outputs.imageid }}\n',
        ],
      ],
    },
    /release-binary: .*uses the imageid of the local build, which depends on the image store/,
  ],
  [
    'the digest comparison after the retention',
    {
      [REL]: [
        [RETENTION_STEP, ''],
        [DIGEST_STEP_NAME, `${RETENTION_STEP}${DIGEST_STEP_NAME}`],
      ],
    },
    /release-binary: after the push and before the retention, a step must compare the config of the checked image/,
  ],
  // The comparison step is exactly the script, unconditional within the release, and on bash -e.
  [
    'the comparison with || true',
    { [REL]: [[DIGEST_RUN, '        run: node scripts/check-pushed-server.mjs || true\n']] },
    new RegExp(
      `release-binary: ${DIGEST_AT} must run exactly node scripts/check-pushed-server\\.mjs`,
    ),
  ],
  [
    'the comparison followed by true',
    { [REL]: [[DIGEST_RUN, '        run: node scripts/check-pushed-server.mjs; true\n']] },
    new RegExp(`release-binary: ${DIGEST_AT} must run exactly`),
  ],
  [
    'the comparison replaced by true',
    { [REL]: [[DIGEST_RUN, '        run: "true"\n']] },
    /release-binary: after the push and before the retention, a step must compare the config of the checked image/,
  ],
  [
    'continue-on-error: true on the comparison',
    { [REL]: [[DIGEST_STEP_NAME, `${DIGEST_STEP_NAME}        continue-on-error: true\n`]] },
    new RegExp(`continue-on-error: ${DIGEST_AT} must not declare it`),
  ],
  [
    'continue-on-error: true on the build job',
    { [REL]: [[BUILD_JOB, `${BUILD_JOB}    continue-on-error: true\n`]] },
    /continue-on-error: jobs\.build must not declare it/,
  ],
  [
    'the comparison with if: false',
    {
      [REL]: [
        [
          `${DIGEST_STEP_NAME}        if: github.event_name == 'push'\n`,
          `${DIGEST_STEP_NAME}        if: false\n`,
        ],
      ],
    },
    new RegExp(`release-binary: ${DIGEST_AT} must have the if: of the server builds`),
  ],
  [
    'the comparison without if: (it would also run on a rollback)',
    { [REL]: [[`${DIGEST_STEP_NAME}        if: github.event_name == 'push'\n`, DIGEST_STEP_NAME]] },
    new RegExp(`release-binary: ${DIGEST_AT} must have the if: of the server builds`),
  ],
  [
    'the comparison with shell: bash {0} (no -e)',
    { [REL]: [[DIGEST_RUN, `        shell: bash {0}\n${DIGEST_RUN}`]] },
    new RegExp(`release-binary: ${DIGEST_AT} must fail closed: shell: must be the default or bash`),
  ],
  [
    'the comparison without TAG',
    { [REL]: [['          TAG: v1\n', '']] },
    new RegExp(`release-binary: ${DIGEST_AT} must set TAG and SHORT`),
  ],
  [
    'the comparison with a fixed PUSHED',
    { [REL]: [[PUSHED_ENV, `          PUSHED: sha256:${'a'.repeat(64)}\n`]] },
    new RegExp(`release-binary: ${DIGEST_AT} must set PUSHED`),
  ],
  [
    'defaults.run.shell without -e on the build job',
    { [REL]: [[BUILD_JOB, `${BUILD_JOB}    defaults:\n      run:\n        shell: bash {0}\n`]] },
    new RegExp(`release-binary: ${CHECK_AT} must fail closed: shell: must be the default or bash`),
  ],
  [
    'defaults.run.shell without -e on the release workflow',
    { [REL]: [['jobs:\n', 'defaults:\n  run:\n    shell: sh {0}\njobs:\n']] },
    new RegExp(`release-binary: ${DIGEST_AT} must fail closed: shell:`),
  ],
  // The binary check between the builds fails the step when the script fails.
  [
    'the binary check with || true',
    { [REL]: [[CHECK_RUN, CHECK_RUN.replace('server"\n', 'server" || true\n')]] },
    new RegExp(`release-binary: ${CHECK_AT} must fail closed: exactly one line must be the bare`),
  ],
  [
    'the binary check followed by true without set -e',
    { [REL]: [[CHECK_RUN, checkBlock(CHECK_CMD, 'true')]] },
    new RegExp(
      `release-binary: ${CHECK_AT} must fail closed: a multi-line run: must start with set -euo pipefail`,
    ),
  ],
  [
    'the binary check after set +e',
    { [REL]: [[CHECK_RUN, checkBlock('set -euo pipefail', 'set +e', CHECK_CMD, 'true')]] },
    new RegExp(`release-binary: ${CHECK_AT} must fail closed: set \\+ is not allowed`),
  ],
  [
    'the binary check with an EXIT trap that exits 0',
    {
      [REL]: [[CHECK_RUN, checkBlock('set -euo pipefail', "trap 'exit 0' EXIT", CHECK_CMD)]],
    },
    new RegExp(`release-binary: ${CHECK_AT} must fail closed: exit must have a non-zero status`),
  ],
  [
    'the binary check skipped by a bare exit',
    { [REL]: [[CHECK_RUN, checkBlock('set -euo pipefail', 'exit', CHECK_CMD)]] },
    new RegExp(`release-binary: ${CHECK_AT} must fail closed: exit must have`),
  ],
  [
    'the binary check inside an if',
    {
      [REL]: [[CHECK_RUN, checkBlock('set -euo pipefail', `if ${CHECK_CMD}; then echo ok; fi`)]],
    },
    new RegExp(
      `release-binary: ${CHECK_AT} must fail closed: exactly one line must be the bare command, found 0`,
    ),
  ],
  [
    'continue-on-error: true on the binary check',
    { [REL]: [[CHECK_RUN, `        continue-on-error: true\n${CHECK_RUN}`]] },
    new RegExp(`continue-on-error: ${CHECK_AT} must not declare it`),
  ],
  [
    'the binary check with if: false',
    { [REL]: [[CHECK_STEP, CHECK_STEP.replace("if: github.event_name == 'push'", 'if: false')]] },
    new RegExp(`release-binary: ${CHECK_AT} must have the if: of the server builds`),
  ],
  [
    'the binary check with shell: sh {0}',
    { [REL]: [[CHECK_RUN, `        shell: sh {0}\n${CHECK_RUN}`]] },
    new RegExp(`release-binary: ${CHECK_AT} must fail closed: shell:`),
  ],
  [
    'rust checking its musl binary with || true',
    { [CI]: [[MUSL_RUN, MUSL_RUN.replace('server\n', 'server || true\n')]] },
    /ci\.yml: release-binary: jobs\.rust\.steps\[\d+\] \(The musl binary has no dev-auth\) must fail closed/,
  ],
  [
    'rust checking its musl binary with if: false',
    { [CI]: [[MUSL_RUN, `        if: false\n${MUSL_RUN}`]] },
    /ci\.yml: release-binary: jobs\.rust\.steps\[\d+\] .* must not have an if:/,
  ],
  [
    'images checking its server binary after set +e',
    {
      [CI]: [
        [
          IMAGES_CHECK,
          `      - name: Server binary\n${checkBlock('set -euo pipefail', 'set +e', CHECK_CMD, 'true')}`,
        ],
      ],
    },
    /ci\.yml: release-binary: jobs\.images\.steps\[\d+\] \(Server binary\) must fail closed: set \+/,
  ],
  [
    'rust without the check of its musl binary',
    { [CI]: [[MUSL_CHECK, '']] },
    /ci\.yml: release-binary: jobs\.rust must run scripts\/check-release-binary\.mjs without --expect-dev/,
  ],
  [
    'images without the check of its server binary',
    { [CI]: [[IMAGES_CHECK, '']] },
    /ci\.yml: release-binary: jobs\.images must run/,
  ],
  [
    'images checking with --expect-dev',
    { [CI]: [[IMAGES_CHECK, IMAGES_CHECK.replace('.mjs "', '.mjs --expect-dev "')]] },
    /ci\.yml: release-binary: jobs\.images must run/,
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

  it.each(E2E_CASES)('fails on: %s', (_name, edits, pattern) => {
    const problems = checkWorkflows(mutated(edits));
    expect(
      problems.some((p) => pattern.test(p)),
      problems.join('\n') || '(no problems)',
    ).toBe(true);
  });

  it.each([...CSP_CASES, ...RELEASE_CASES])('fails on: %s', (_name, edits, pattern) => {
    const problems = checkWorkflows(mutated(edits));
    expect(
      problems.some((p) => pattern.test(p)),
      problems.join('\n') || '(no problems)',
    ).toBe(true);
  });

  it('compares b1= byte for byte, also when the two copies differ only in quotes', () => {
    const tree = mutated({
      [REL]: [[B1_LINE, B1_LINE.replace('b1="', "b1='").replace(/"\n$/, "'\n")]],
    });
    expect(checkWorkflows(tree).filter((p) => p.includes('csp-b1'))).toHaveLength(1);
  });

  it('fails without infra/compose.dev.yml when ci.yml has a Postgres service', () => {
    const tree = {
      read: (rel) => (rel === COMPOSE_DEV ? null : valid.read(rel)),
      files: valid.files,
    };
    expect(checkWorkflows(tree).some((p) => /postgres-digest: .*compose\.dev\.yml/.test(p))).toBe(
      true,
    );
  });

  it('accepts timeout-minutes on a step (e2e browser install)', () => {
    const tree = mutated({
      [CI]: [[RUN_STEP, '      - run: echo rust\n        timeout-minutes: 6\n']],
    });
    expect(checkWorkflows(tree)).toEqual([]);
  });

  it.each(PLAYWRIGHT_CASES)('fails on: %s', (_name, edits, pattern) => {
    const problems = checkWorkflows(mutated(edits));
    expect(
      problems.some((p) => pattern.test(p)),
      problems.join('\n') || '(no problems)',
    ).toBe(true);
  });

  it('accepts PLAYWRIGHT_IMAGE_VERSION equal to the image tag', () => {
    expect(checkWorkflows(mutated(inPlaywright('PLAYWRIGHT_IMAGE_VERSION: 1.64.1')))).toEqual([]);
  });

  it.each(CONTAINER_CASES)('fails on: %s', (_name, edits, pattern) => {
    const problems = checkWorkflows(mutated(edits));
    expect(
      problems.some((p) => pattern.test(p)),
      problems.join('\n') || '(no problems)',
    ).toBe(true);
  });

  it('accepts a container with --shm-size and bash in the job defaults', () => {
    expect(checkWorkflows(mutated(inContainer('', '--shm-size=1g')))).toEqual([]);
  });

  it('accepts bash in the workflow defaults for a container job', () => {
    const tree = mutated({
      [CI]: [
        [
          'permissions:\n  contents: read\njobs:',
          'permissions:\n  contents: read\ndefaults:\n  run:\n    shell: bash\njobs:',
        ],
        [RUST_JOB, `${RUST_JOB}    container: node:22${DIGEST}\n`],
      ],
    });
    expect(checkWorkflows(tree)).toEqual([]);
  });

  it('accepts container: and services: pinned by digest', () => {
    const digest = `@sha256:${'a'.repeat(64)}`;
    const tree = mutated({
      [CI]: [
        [
          RUST_JOB,
          `${RUST_JOB}    container: node:22${digest}\n    defaults:\n      run:\n        shell: bash\n    services:\n      cache:\n        image: redis:7${digest}\n`,
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
    expect(jobs.map(([id]) => id).sort()).toEqual(['e2e', 'e2e-server', 'images', 'rust', 'web']);
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

  it('the web filter covers what the sprite importer check reads (007 plan-b decision 27)', () => {
    const globs = filters.web.flat(Infinity).map(globToRegExp);
    const read = [
      'content/sprites/x/manifest.json',
      'content/characters/x/character.json',
      'crates/content/tests/golden/ruleset-timing.json',
      'docs/guides/sprites.md',
    ];
    expect(read.filter((file) => !globs.some((g) => g.test(file)))).toEqual([]);
  });
});
