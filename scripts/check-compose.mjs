#!/usr/bin/env node
// RF-C3, RF-C4, RF-C6 and plan-c decisions 9 and 10: the production compose, the local override,
// .dockerignore and the Dockerfiles keep the exact shape the deployment depends on.
//
// Every problem is `<file>: <rule>: <detail>`. The Traefik header values are derived from
// SECURITY_HEADERS (apps/web/src/lib/server/hooks/security-headers.ts), the single source of truth.
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { isAlias, isScalar, parseDocument, visit } from 'yaml';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

export const FILES = {
  compose: 'infra/docker-compose.yml',
  local: 'infra/compose.local.yml',
  dev: 'infra/compose.dev.yml',
  dockerignore: '.dockerignore',
  serverDockerfile: 'apps/server/Dockerfile',
  webDockerfile: 'apps/web/Dockerfile',
  toolchain: 'rust-toolchain.toml',
  nodeVersion: '.node-version',
  headers: 'apps/web/src/lib/server/hooks/security-headers.ts',
};

// Fixture trees of this script are test data that BuildKit never reads.
const LISTING_EXEMPT = 'scripts/__fixtures__/check-compose/';

const REGISTRY = 'ghcr.io/kike1303/torneo-absurdo';
const SERVICES = ['server', 'web'];
const HEALTH_TIMING = { interval: '15s', timeout: '3s', retries: 3, start_period: '30s' };
const HEALTH_TEST = {
  server: ['CMD', '/server', 'healthcheck'],
  web: [
    'CMD',
    '/nodejs/bin/node',
    '-e',
    "fetch('http://127.0.0.1:3000/healthz').then(r=>process.exit(r.ok?0:1)).catch(()=>process.exit(1))",
  ],
};
const MEMORY = { server: '256m', web: '384m' };
const PUBLIC_ORIGIN = 'https://${PUBLIC_HOST:?}';
// ADR-0012 B2: SECURITY_HEADERS must carry exactly this value.
const PERMISSIONS_POLICY =
  'camera=(), microphone=(), geolocation=(), payment=(), usb=(), display-capture=()';
const ENV = {
  // Exactly info (RF-C4): `warn` or `off` drop the /api/health line the log retention relies on,
  // and a bare target (`info,sqlx`) or a number (`server=5`) turns on TRACE.
  // TRUSTED_PROXIES is exactly coolify-proxy's IP (ADR-0006 D2, 008 plan-a decision 13): without
  // it every client shares the proxy's IpKey, and the subnet would also trust web.
  server: {
    DATABASE_URL: '${DATABASE_URL:?}',
    PUBLIC_ORIGIN,
    RUST_LOG: 'info',
    TRUSTED_PROXIES: '10.250.0.2/32',
    // Required secrets from Coolify, never literals in git (spec 008, RF-B18).
    INTERNAL_API_SECRET: '${INTERNAL_API_SECRET:?}',
    IP_HASH_KEY: '${IP_HASH_KEY:?}',
    // Any integer from 1 to 10 000, checked below: TA-27 changes the value, not this script.
    DAILY_GRANT: null,
  },
  web: {
    PUBLIC_ORIGIN,
    ADDRESS_HEADER: 'X-Forwarded-For',
    XFF_DEPTH: '1',
    HOST: '0.0.0.0',
    PORT: '3000',
    NODE_OPTIONS: '--max-old-space-size=256',
  },
};
const FORBIDDEN_ENV = {
  server: ['BIND_ADDR'],
  web: ['ORIGIN', 'PROTOCOL_HEADER', 'HOST_HEADER', 'PORT_HEADER'],
};
// Closed lists of keys (RF-C4): anything else (privileged, cap_add, user, env_file, extends,
// volumes, network_mode, ...) fails, so the hardening cannot be bypassed by an extra key.
const ROOT_KEYS = ['networks', 'services'];
const SERVICE_KEYS = {
  server: [
    'cap_drop',
    'environment',
    'healthcheck',
    'image',
    'labels',
    'logging',
    'mem_limit',
    'memswap_limit',
    'networks',
    'platform',
    'read_only',
    'restart',
    'security_opt',
    'stop_grace_period',
  ],
  web: [
    'cap_drop',
    'depends_on',
    'environment',
    'healthcheck',
    'image',
    'labels',
    'logging',
    'mem_limit',
    'memswap_limit',
    'networks',
    'platform',
    'read_only',
    'restart',
    'security_opt',
    'tmpfs',
  ],
};
const HEALTH_KEYS = ['interval', 'retries', 'start_period', 'test', 'timeout'];
const POSTGRES_KEYS = [
  'command',
  'environment',
  'healthcheck',
  'image',
  'mem_limit',
  'memswap_limit',
  'networks',
  'volumes',
];
/** Tuning of the Coolify Postgres (ADR-0006 D4). */
const POSTGRES_TUNING = ['shared_buffers=128MB', 'max_connections=20', 'work_mem=4MB'];
/**
 * Log parameters of ADR-0012 A9: no DETAIL, STATEMENT, statements or client hosts in the logs.
 * The last four are the defaults, fixed so that no sampling ever logs statements or transactions.
 */
const POSTGRES_LOG_PARAMS = [
  'log_error_verbosity=terse',
  'log_min_error_statement=panic',
  'log_statement=none',
  'log_min_duration_statement=-1',
  'log_min_duration_sample=-1',
  'log_connections=off',
  'log_disconnections=off',
  'log_transaction_sample_rate=0',
  'log_statement_sample_rate=1',
  'log_lock_waits=off',
];
/** @param {string[]} params */
const postgresCommand = (params) => ['postgres', ...params.flatMap((p) => ['-c', p])];
const POSTGRES_COMMAND = postgresCommand([...POSTGRES_TUNING, ...POSTGRES_LOG_PARAMS]);
const POSTGRES_DEV_COMMAND = postgresCommand(POSTGRES_LOG_PARAMS);
const POSTGRES_ENV = ['POSTGRES_DB', 'POSTGRES_PASSWORD', 'POSTGRES_USER'];
const POSTGRES_HEALTH_KEYS = ['interval', 'retries', 'test', 'timeout'];
const LOCAL_VOLUME = 'torneo-local-pg';
const DEV_ONLY_VARS = ['ENABLE_DEV_CATALOG', 'ENABLE_WS_ECHO'];
const LOCAL_PORTS = { server: ['127.0.0.1:8080:8080'], web: ['127.0.0.1:3000:3000'] };
const DOCKERIGNORE_REQUIRED = [
  '.git',
  '**/target',
  '**/node_modules',
  '**/.env',
  '**/.env.*',
  '!**/.env.example',
  'private/',
  'content/_local/',
  'tools/',
];

const HOST_RULE = 'Host(`${PUBLIC_HOST:?}`)';
const API_RULE = `${HOST_RULE} && (PathPrefix(\`/api\`) || PathPrefix(\`/ws\`))`;
const REDIRECT = {
  'traefik.http.middlewares.torneo-https-redirect.redirectscheme.scheme': 'https',
  'traefik.http.middlewares.torneo-https-redirect.redirectscheme.permanent': 'true',
};

/** Routers, service and redirect of one service (ADR-0006 D2, plan-c "Etiquetas del compose"). */
function routedLabels({ router, rule, priority, port, middleware }) {
  const r = `traefik.http.routers.${router}`;
  const h = `traefik.http.routers.${router}-http`;
  /** @type {Record<string, string>} */
  const labels = {
    'traefik.enable': 'true',
    'traefik.docker.network': 'torneo-edge',
    [`${r}.rule`]: rule,
    [`${r}.priority`]: priority,
    [`${r}.entrypoints`]: 'https',
    [`${r}.tls.certresolver`]: 'letsencrypt',
    [`${r}.service`]: router,
    [`${r}.observability.accesslogs`]: 'false',
    [`${h}.rule`]: rule,
    [`${h}.priority`]: priority,
    [`${h}.entrypoints`]: 'http',
    [`${h}.service`]: router,
    [`${h}.middlewares`]: 'torneo-https-redirect',
    [`${h}.observability.accesslogs`]: 'false',
    [`traefik.http.services.${router}.loadbalancer.server.port`]: port,
    ...REDIRECT,
  };
  if (middleware) labels[`${r}.middlewares`] = middleware;
  return labels;
}

/**
 * Reads SECURITY_HEADERS from the source text (no TypeScript import, plan-c decision 9).
 * @param {string | null} source
 * @returns {{ headers: Record<string, string> | null, problems: string[] }}
 */
export function parseSecurityHeaders(source) {
  const rule = 'security-headers';
  if (source === null) return { headers: null, problems: [`${FILES.headers}: ${rule}: missing`] };
  const block = /export const SECURITY_HEADERS = \{([^}]*)\}/.exec(source);
  if (!block) {
    return {
      headers: null,
      problems: [`${FILES.headers}: ${rule}: SECURITY_HEADERS object not found`],
    };
  }
  /** @type {Record<string, string>} */
  const headers = {};
  for (const m of block[1].matchAll(/'([a-z-]+)'\s*:\s*'([^']*)'/g)) headers[m[1]] = m[2];
  const names = Object.keys(headers).sort();
  const expected = [
    'cross-origin-opener-policy',
    'permissions-policy',
    'referrer-policy',
    'strict-transport-security',
    'x-content-type-options',
  ];
  if (names.join(',') !== expected.join(',')) {
    return {
      headers: null,
      problems: [
        `${FILES.headers}: ${rule}: SECURITY_HEADERS must have exactly ${expected.join(', ')} (found ${names.join(', ') || 'none'})`,
      ],
    };
  }
  return { headers, problems: [] };
}

/**
 * Maps SECURITY_HEADERS to the seven labels of the `torneo-web-headers` middleware (RF-C4,
 * ADR-0012 B2).
 * @param {Record<string, string>} headers
 * @returns {{ labels: Record<string, string>, problems: string[] }}
 */
export function headerLabels(headers) {
  const rule = 'security-headers';
  /** @type {string[]} */
  const problems = [];
  const m = 'traefik.http.middlewares.torneo-web-headers.headers';
  if (headers['x-content-type-options'] !== 'nosniff') {
    problems.push(`${FILES.headers}: ${rule}: x-content-type-options must be nosniff`);
  }
  const hsts = headers['strict-transport-security'];
  if (/includesubdomains|preload/i.test(hsts)) {
    problems.push(`${FILES.headers}: ${rule}: HSTS must not have includeSubDomains or preload`);
  }
  if (headers['cross-origin-opener-policy'] !== 'same-origin') {
    problems.push(`${FILES.headers}: ${rule}: cross-origin-opener-policy must be same-origin`);
  }
  if (headers['permissions-policy'] !== PERMISSIONS_POLICY) {
    problems.push(`${FILES.headers}: ${rule}: permissions-policy must be ${PERMISSIONS_POLICY}`);
  }
  const maxAge = /^max-age=(\d+)$/.exec(hsts);
  if (!maxAge) problems.push(`${FILES.headers}: ${rule}: HSTS must be exactly max-age=<seconds>`);
  return {
    labels: {
      [`${m}.contentTypeNosniff`]: 'true',
      [`${m}.referrerPolicy`]: headers['referrer-policy'],
      [`${m}.stsSeconds`]: maxAge ? maxAge[1] : '?',
      [`${m}.stsIncludeSubdomains`]: 'false',
      [`${m}.stsPreload`]: 'false',
      [`${m}.customResponseHeaders.Cross-Origin-Opener-Policy`]:
        headers['cross-origin-opener-policy'],
      [`${m}.permissionsPolicy`]: headers['permissions-policy'],
    },
    problems,
  };
}

/** `${VAR:?any message}` → `${VAR:?}`, so the error text of the interpolation is free. */
const normalizeInterpolation = (value) => value.replace(/\$\{([A-Z_]+):\?[^}]*\}/g, '${$1:?}');

/** Compose accepts labels and environment as a list of `K=V` or as a map. */
function toMap(value) {
  /** @type {Map<string, string | null>} */
  const out = new Map();
  if (Array.isArray(value)) {
    for (const item of value) {
      const text = String(item);
      const eq = text.indexOf('=');
      if (eq === -1) out.set(text, null);
      else out.set(text.slice(0, eq), text.slice(eq + 1));
    }
  } else if (value && typeof value === 'object') {
    for (const [k, v] of Object.entries(value)) out.set(k, v === null ? null : String(v));
  }
  return out;
}

/** Compose accepts service networks as a list or as a map. */
function networksOf(value) {
  /** @type {Map<string, any>} */
  const out = new Map();
  if (Array.isArray(value)) for (const n of value) out.set(String(n), {});
  else if (value && typeof value === 'object') {
    for (const [k, v] of Object.entries(value)) out.set(k, v ?? {});
  }
  return out;
}

const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

/**
 * @param {string | null} text
 * @param {string} file
 * @param {string[]} problems
 */
function parseYaml(text, file, problems) {
  if (text === null) {
    problems.push(`${file}: exists: missing`);
    return null;
  }
  try {
    const document = parseDocument(text);
    if (document.errors.length > 0) throw document.errors[0];
    // A warning (for example an unknown tag) means the parser and Compose may disagree.
    for (const warning of document.warnings) {
      problems.push(`${file}: yaml-warnings: ${warning.message.split('\n')[0]}`);
    }
    // Compose resolves anchors, aliases and `<<` merge keys; the YAML 1.2 core schema of `yaml`
    // keeps `<<` as a literal key. Forbidding all three makes the script see what Compose sees.
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
        // Compose honours !reset and !override in any file and drops or replaces the key, while
        // the parser keeps the value: no explicit tag at all (!reset, !override, !!str, ...).
        if (node.tag) problems.push(`${file}: yaml-tags: ${node.tag} is not allowed`);
      },
    });
    const doc = document.toJS();
    if (!doc || typeof doc !== 'object') {
      problems.push(`${file}: yaml: not a mapping`);
      return null;
    }
    return doc;
  } catch (error) {
    problems.push(
      `${file}: yaml: ${error instanceof Error ? error.message.split('\n')[0] : error}`,
    );
    return null;
  }
}

/** Raw-text rules shared by every compose file. */
function checkComposeText(text, file, problems) {
  if (text === null) return;
  for (const name of DEV_ONLY_VARS) {
    if (text.includes(name)) problems.push(`${file}: dev-only-vars: ${name} must never appear`);
  }
}

/**
 * @param {any} doc parsed docker-compose.yml
 * @param {Record<string, string>} webHeaderLabels
 * @param {string[]} problems
 */
function checkProduction(doc, webHeaderLabels, problems) {
  const f = FILES.compose;
  const services = doc.services ?? {};
  const names = Object.keys(services).sort();
  if (!same(names, SERVICES)) {
    problems.push(`${f}: services: must be exactly server and web (found ${names.join(', ')})`);
  }
  if ('name' in doc) problems.push(`${f}: no-name: top-level name is set by Coolify`);
  for (const key of Object.keys(doc)) {
    if (!ROOT_KEYS.includes(key)) problems.push(`${f}: root-keys: ${key} is not allowed`);
  }

  const nets = doc.networks ?? {};
  for (const net of ['torneo-edge', 'torneo-db']) {
    // `name:` would point the external network somewhere else.
    if (!same(nets[net], { external: true })) {
      problems.push(`${f}: networks: ${net} must be declared with external: true and nothing else`);
    }
  }
  for (const net of Object.keys(nets)) {
    if (!['torneo-edge', 'torneo-db'].includes(net)) {
      problems.push(`${f}: networks: only torneo-edge and torneo-db (found ${net})`);
    }
  }

  /** @type {Record<string, Map<string, string | null>>} */
  const envs = {};
  /** @type {Record<string, Map<string, string>>} */
  const labelsBySvc = {};

  for (const svc of SERVICES) {
    const s = services[svc];
    if (!s || typeof s !== 'object') continue;
    const at = `${f}: services.${svc}`;

    for (const key of Object.keys(s)) {
      if (!SERVICE_KEYS[svc].includes(key)) {
        problems.push(`${at}: service-keys: ${key} is not allowed`);
      }
    }
    for (const key of SERVICE_KEYS[svc]) {
      if (!(key in s)) problems.push(`${at}: service-keys: ${key} is required`);
    }
    if ('build' in s) problems.push(`${at}: no-build: build is forbidden (images come from GHCR)`);
    if ('ports' in s) problems.push(`${at}: no-ports: ports are forbidden (Traefik routes)`);

    const image = String(s.image ?? '');
    if (normalizeInterpolation(image) !== `${REGISTRY}/${svc}:\${IMAGE_TAG:?}`) {
      problems.push(`${at}: image: must be ${REGISTRY}/${svc}:\${IMAGE_TAG:?...} (found ${image})`);
    }
    if (s.platform !== 'linux/amd64') problems.push(`${at}: platform: must be linux/amd64`);
    if (s.restart !== 'unless-stopped') problems.push(`${at}: restart: must be unless-stopped`);
    if (s.read_only !== true) problems.push(`${at}: read_only: must be true`);
    if (!same(s.cap_drop, ['ALL'])) problems.push(`${at}: cap_drop: must be [ALL]`);
    if (!same(s.security_opt, ['no-new-privileges:true'])) {
      problems.push(`${at}: security_opt: must be ["no-new-privileges:true"]`);
    }
    if (s.mem_limit !== MEMORY[svc] || s.memswap_limit !== MEMORY[svc]) {
      problems.push(`${at}: memory: mem_limit and memswap_limit must both be ${MEMORY[svc]}`);
    }

    const hc = s.healthcheck ?? {};
    // `disable: true` would make depends_on service_healthy and `up --wait` meaningless.
    if (!same(Object.keys(hc).sort(), HEALTH_KEYS)) {
      problems.push(`${at}: healthcheck: keys must be exactly ${HEALTH_KEYS.join(', ')}`);
    }
    if (!same(hc.test, HEALTH_TEST[svc])) {
      problems.push(`${at}: healthcheck: test must be ${JSON.stringify(HEALTH_TEST[svc])}`);
    }
    for (const [key, value] of Object.entries(HEALTH_TIMING)) {
      if (hc[key] !== value) problems.push(`${at}: healthcheck: ${key} must be ${value}`);
    }

    const logging = s.logging ?? {};
    const opts = logging.options ?? {};
    if (
      logging.driver !== 'json-file' ||
      opts['max-size'] !== '10m' ||
      opts['max-file'] !== '3' ||
      Object.keys(opts).length !== 2 ||
      !same(Object.keys(logging).sort(), ['driver', 'options'])
    ) {
      problems.push(
        `${at}: logging: must be driver json-file with options max-size "10m" and max-file "3"`,
      );
    }

    // Environment: closed list per service.
    const env = toMap(s.environment);
    envs[svc] = env;
    for (const name of FORBIDDEN_ENV[svc]) {
      if (env.has(name)) problems.push(`${at}: forbidden-env: ${name} must not be set`);
    }
    for (const name of env.keys()) {
      if (!(name in ENV[svc])) {
        problems.push(`${at}: env-allowlist: ${name} is not an allowed variable for ${svc}`);
      }
    }
    for (const [name, expected] of Object.entries(ENV[svc])) {
      if (!env.has(name)) {
        problems.push(`${at}: env: ${name} is required`);
        continue;
      }
      const value = normalizeInterpolation(env.get(name) ?? '');
      if (expected !== null && value !== expected) {
        problems.push(`${at}: env: ${name} must be ${expected} (found ${env.get(name)})`);
      }
    }
    if (svc === 'server' && env.has('DAILY_GRANT')) {
      const grant = String(env.get('DAILY_GRANT') ?? '');
      if (!/^[1-9][0-9]{0,4}$/.test(grant) || Number(grant) > 10000) {
        problems.push(`${at}: daily-grant: DAILY_GRANT must be an integer from 1 to 10000`);
      }
    }
    if (env.has('RUST_LOG') && /\b(debug|trace)\b/i.test(env.get('RUST_LOG') ?? '')) {
      problems.push(`${at}: rust-log: RUST_LOG must not be more verbose than info`);
    }

    // Labels: exact set. Docker keeps keys that differ only in case as separate labels, so two
    // such keys are rejected (one could hide the other) and each key must have the exact
    // spelling of RF-C4.
    /** @type {Map<string, string>} lowercase key → value */
    const labels = new Map();
    /** @type {Map<string, string>} lowercase key → key as written */
    const spelled = new Map();
    for (const [k, v] of toMap(s.labels)) {
      const lower = k.toLowerCase();
      if (spelled.has(lower)) {
        problems.push(
          `${at}: labels: ${k} collides with ${spelled.get(lower)} (keys differ only in case)`,
        );
        continue;
      }
      spelled.set(lower, k);
      labels.set(lower, normalizeInterpolation(v ?? ''));
    }
    labelsBySvc[svc] = labels;
    for (const [k, v] of labels) {
      if (k.includes('forcestsheader')) {
        problems.push(`${at}: hsts: forceSTSHeader is forbidden`);
      }
      if (/\.(stsincludesubdomains|stspreload)$/.test(k) && v.toLowerCase() === 'true') {
        problems.push(`${at}: hsts: ${k} must not be true`);
      }
      if (k.startsWith('traefik.http.middlewares.torneo-web-headers.') && svc !== 'web') {
        problems.push(`${at}: web-headers-scope: torneo-web-headers belongs to web only`);
      }
      const routerMw = /^traefik\.http\.routers\.([^.]+)\.middlewares$/.exec(k);
      if (routerMw && routerMw[1] !== 'torneo-web' && v.split(',').includes('torneo-web-headers')) {
        problems.push(`${at}: web-headers-scope: router ${routerMw[1]} must not use it`);
      }
    }
    const expected =
      svc === 'web'
        ? {
            ...routedLabels({
              router: 'torneo-web',
              rule: HOST_RULE,
              priority: '1',
              port: '3000',
              middleware: 'torneo-web-headers',
            }),
            ...webHeaderLabels,
          }
        : routedLabels({ router: 'torneo-api', rule: API_RULE, priority: '100', port: '8080' });
    const expectedLower = new Map(Object.entries(expected).map(([k, v]) => [k.toLowerCase(), v]));
    for (const [k, v] of expectedLower) {
      if (!labels.has(k)) problems.push(`${at}: labels: missing ${k}=${v}`);
      else if (labels.get(k) !== v) {
        problems.push(`${at}: labels: ${k} must be ${v} (found ${labels.get(k)})`);
      }
    }
    for (const k of labels.keys()) {
      if (!expectedLower.has(k)) problems.push(`${at}: labels: unexpected ${k}`);
    }
    const expectedSpelling = new Map(Object.keys(expected).map((k) => [k.toLowerCase(), k]));
    for (const [lower, written] of spelled) {
      const exact = expectedSpelling.get(lower);
      if (exact && exact !== written) {
        problems.push(`${at}: labels: ${written} must be spelled ${exact}`);
      }
    }
  }

  // Cross-service rules.
  const web = services.web ?? {};
  const server = services.server ?? {};
  if (
    envs.web &&
    envs.server &&
    envs.web.get('PUBLIC_ORIGIN') !== envs.server.get('PUBLIC_ORIGIN')
  ) {
    problems.push(`${f}: public-origin: PUBLIC_ORIGIN must be identical in web and server`);
  }
  if (labelsBySvc.web && labelsBySvc.server) {
    const redirect = (labels) =>
      [...labels].filter(([k]) => k.includes('.middlewares.torneo-https-redirect.')).sort();
    const a = redirect(labelsBySvc.web);
    const b = redirect(labelsBySvc.server);
    if (a.length > 0 && b.length > 0 && !same(a, b)) {
      problems.push(
        `${f}: https-redirect: torneo-https-redirect must be identical in both services`,
      );
    }
  }
  if (!same(web.depends_on, { server: { condition: 'service_healthy' } })) {
    problems.push(`${f}: services.web: depends-on: server with condition service_healthy`);
  }
  if (server.stop_grace_period !== '15s') {
    problems.push(`${f}: services.server: stop-grace: stop_grace_period must be 15s`);
  }
  const tmpfs = Array.isArray(web.tmpfs) ? web.tmpfs : web.tmpfs ? [web.tmpfs] : [];
  if (!tmpfs.some((t) => String(t).startsWith('/tmp:'))) {
    problems.push(`${f}: services.web: tmpfs: /tmp with a size= is required`);
  }
  for (const t of tmpfs) {
    if (!/[:,]size=\d+[kmg]?\b/.test(String(t))) {
      problems.push(`${f}: services.web: tmpfs: ${t} needs size= (unbounded tmpfs)`);
    }
  }
  // Networks of each service.
  const webNets = networksOf(web.networks);
  if (!same([...webNets.keys()], ['torneo-edge'])) {
    problems.push(`${f}: services.web: networks: web must be only on torneo-edge`);
  }
  const serverNets = networksOf(server.networks);
  if (!same([...serverNets.keys()].sort(), ['torneo-db', 'torneo-edge'])) {
    problems.push(`${f}: services.server: networks: server must be on torneo-edge and torneo-db`);
  }
  // Only the alias: no fixed ipv4_address (10.250.0.2 is the proxy, TRUSTED_PROXIES).
  if (!same(serverNets.get('torneo-edge'), { aliases: ['torneo-server'] })) {
    problems.push(
      `${f}: services.server: networks: alias torneo-server on torneo-edge and nothing else`,
    );
  }
  const emptyNet = (v) => v === null || v === undefined || same(v, {});
  if (!emptyNet(serverNets.get('torneo-db'))) {
    problems.push(`${f}: services.server: networks: torneo-db takes no options`);
  }
  if (!emptyNet(webNets.get('torneo-edge'))) {
    problems.push(`${f}: services.web: networks: torneo-edge takes no options`);
  }
}

/**
 * @param {any} local parsed compose.local.yml
 * @param {any} dev parsed compose.dev.yml
 * @param {string[]} problems
 */
function checkLocal(local, dev, problems) {
  const f = FILES.local;
  const services = local.services ?? {};
  for (const name of Object.keys(services)) {
    if (!['postgres', 'server', 'web'].includes(name)) {
      problems.push(`${f}: services: only postgres, server and web are allowed (found ${name})`);
    }
  }
  for (const key of Object.keys(local)) {
    if (!['services', 'volumes'].includes(key)) {
      problems.push(
        `${f}: only-adds: top-level ${key} is not allowed (it would override production)`,
      );
    }
  }
  for (const svc of SERVICES) {
    const s = services[svc];
    if (s === undefined) continue;
    const keys = Object.keys(s ?? {});
    if (!same(keys, ['ports'])) {
      problems.push(
        `${f}: services.${svc}: only-adds: only ports may be added (found ${keys.join(', ')})`,
      );
    }
    const ports = Array.isArray(s?.ports) ? s.ports.map(String) : [];
    if (!same(ports, LOCAL_PORTS[svc])) {
      problems.push(`${f}: services.${svc}: ports: must be ${JSON.stringify(LOCAL_PORTS[svc])}`);
    }
  }
  for (const [svc, s] of Object.entries(services)) {
    for (const p of Array.isArray(s?.ports) ? s.ports : []) {
      if (!String(p).startsWith('127.0.0.1:')) {
        problems.push(`${f}: services.${svc}: loopback-ports: ${p} must bind 127.0.0.1`);
      }
    }
  }
  // compose.dev.yml carries the same log parameters, without the tuning (ADR-0012 A9).
  if (!same(dev?.services?.postgres?.command, POSTGRES_DEV_COMMAND)) {
    problems.push(
      `${FILES.dev}: services.postgres: command: must be ${JSON.stringify(POSTGRES_DEV_COMMAND)}`,
    );
  }
  const pg = services.postgres;
  if (!pg) {
    problems.push(`${f}: services.postgres: required`);
    return;
  }
  const devImage = dev?.services?.postgres?.image;
  if (!/@sha256:[0-9a-f]{64}$/.test(String(devImage ?? ''))) {
    problems.push(`${FILES.dev}: services.postgres: image must be pinned by digest`);
  }
  if (pg.image !== devImage) {
    problems.push(`${f}: services.postgres: image must equal the one of ${FILES.dev}`);
  }
  if (pg.mem_limit !== '512m' || pg.memswap_limit !== '512m') {
    problems.push(`${f}: services.postgres: memory: mem_limit and memswap_limit must both be 512m`);
  }
  if (!same([...networksOf(pg.networks).keys()], ['torneo-db'])) {
    problems.push(`${f}: services.postgres: networks: postgres must be only on torneo-db`);
  }
  const at = `${f}: services.postgres`;
  for (const key of Object.keys(pg)) {
    if (!POSTGRES_KEYS.includes(key)) problems.push(`${at}: service-keys: ${key} is not allowed`);
  }
  if (!same(pg.command, POSTGRES_COMMAND)) {
    problems.push(`${at}: command: must be ${JSON.stringify(POSTGRES_COMMAND)}`);
  }
  for (const name of toMap(pg.environment).keys()) {
    if (!POSTGRES_ENV.includes(name)) {
      problems.push(`${at}: env-allowlist: ${name} is not an allowed variable for postgres`);
    }
  }
  const volumes = [`${LOCAL_VOLUME}:/var/lib/postgresql/data`];
  if (!same(pg.volumes, volumes)) {
    problems.push(`${at}: volumes: must be ${JSON.stringify(volumes)}`);
  }
  if (!same(Object.keys(pg.healthcheck ?? {}).sort(), POSTGRES_HEALTH_KEYS)) {
    problems.push(`${at}: healthcheck: keys must be exactly ${POSTGRES_HEALTH_KEYS.join(', ')}`);
  }
  const localVolumes = local.volumes ?? {};
  if (
    !same(Object.keys(localVolumes), [LOCAL_VOLUME]) ||
    !(localVolumes[LOCAL_VOLUME] === null || same(localVolumes[LOCAL_VOLUME], {}))
  ) {
    problems.push(`${f}: volumes: only ${LOCAL_VOLUME}, without options`);
  }
}

/**
 * @param {string | null} text
 * @param {string[]} files every file of the tree, relative and with `/`
 * @param {string[]} problems
 */
function checkDockerignore(text, files, problems) {
  const f = FILES.dockerignore;
  if (text === null) {
    problems.push(`${f}: exists: missing`);
  } else {
    const lines = text
      .split('\n')
      .map((l) => l.trim())
      .filter((l) => l && !l.startsWith('#'));
    for (const entry of DOCKERIGNORE_REQUIRED) {
      if (!lines.includes(entry)) problems.push(`${f}: entries: missing ${entry}`);
    }
    const env = lines.lastIndexOf('**/.env.*');
    const example = lines.lastIndexOf('!**/.env.example');
    if (env !== -1 && example !== -1 && example < env) {
      problems.push(`${f}: entries: !**/.env.example must come after **/.env.*`);
    }
    // A re-inclusion can bring back a secret (`!**/.env.production`, `!private/...`), and
    // check-dockerignore.sh only probes files named `.env`.
    for (const line of lines) {
      if (line.startsWith('!') && line !== '!**/.env.example') {
        problems.push(`${f}: reinclude: ${line} is not allowed (only !**/.env.example)`);
      }
    }
  }
  for (const file of files) {
    if (file === f || file.startsWith(LISTING_EXEMPT)) continue;
    if (path.posix.basename(file).endsWith('.dockerignore')) {
      problems.push(
        `${file}: single-dockerignore: only the root .dockerignore is allowed (BuildKit prefers <Dockerfile>.dockerignore)`,
      );
    }
  }
}

/**
 * FROM lines of a Dockerfile, without the ones that name an earlier stage.
 * @param {string} text
 */
function externalFroms(text) {
  /** @type {{ image: string, line: number }[]} */
  const out = [];
  const stages = new Set();
  text.split('\n').forEach((raw, index) => {
    const m = /^\s*FROM\s+(?:--platform=\S+\s+)?(\S+)(?:\s+AS\s+(\S+))?\s*$/i.exec(raw);
    if (!m) return;
    if (!stages.has(m[1].toLowerCase())) out.push({ image: m[1], line: index + 1 });
    if (m[2]) stages.add(m[2].toLowerCase());
  });
  return out;
}

/**
 * @param {(rel: string) => string | null} read
 * @param {string[]} problems
 */
function checkDockerfiles(read, problems) {
  const channel = /^\s*channel\s*=\s*"([^"]+)"/m.exec(read(FILES.toolchain) ?? '')?.[1];
  const node = (read(FILES.nodeVersion) ?? '').trim();
  if (!channel) problems.push(`${FILES.toolchain}: channel: not found`);
  if (!node) problems.push(`${FILES.nodeVersion}: version: not found`);

  for (const [file, kind] of [
    [FILES.serverDockerfile, 'server'],
    [FILES.webDockerfile, 'web'],
  ]) {
    const text = read(file);
    if (text === null) {
      problems.push(`${file}: exists: missing`);
      continue;
    }
    const froms = externalFroms(text);
    if (froms.length === 0) problems.push(`${file}: from: no FROM found`);
    for (const { image, line } of froms) {
      const at = `${file}:${line}`;
      if (!/@sha256:[0-9a-f]{64}$/.test(image))
        problems.push(`${at}: digest: ${image} needs @sha256:`);
      if (/:latest(@|$)/.test(image)) problems.push(`${at}: no-latest: ${image} uses latest`);
      const rust = /^(?:docker\.io\/)?(?:library\/)?rust:([^-@]+)/.exec(image);
      if (rust && channel && rust[1] !== channel) {
        problems.push(
          `${at}: rust-version: rust:${rust[1]} must equal rust-toolchain.toml (${channel})`,
        );
      }
      const nodeImage = /^(?:docker\.io\/)?(?:library\/)?node:([^-@]+)/.exec(image);
      if (nodeImage && node && nodeImage[1] !== node) {
        problems.push(
          `${at}: node-version: node:${nodeImage[1]} must equal .node-version (${node})`,
        );
      }
    }
    const final = froms.at(-1);
    if (final && !/:nonroot@sha256:/.test(final.image)) {
      problems.push(`${file}:${final.line}: nonroot: the final base must be a :nonroot tag`);
    }
    if (
      kind === 'server' &&
      !/^(?:rust:|(?:docker\.io\/)?(?:library\/)?rust:)/.test(froms[0]?.image ?? '')
    ) {
      problems.push(`${file}: rust-version: the first FROM must be the rust: builder`);
    }
    if (kind === 'web') {
      const major = node.split('.')[0];
      if (final && !final.image.startsWith(`gcr.io/distroless/nodejs${major}-`)) {
        problems.push(
          `${file}:${final.line}: node-major: the final base must be gcr.io/distroless/nodejs${major}-*`,
        );
      }
      if (!froms.some((f) => /^(?:docker\.io\/)?(?:library\/)?node:/.test(f.image))) {
        problems.push(`${file}: node-version: the build stage must use node:`);
      }
    }
  }
}

/**
 * @typedef {{ read: (rel: string) => string | null, files: string[] }} Tree
 */

/**
 * @param {Tree} tree
 * @returns {string[]}
 */
export function checkCompose(tree) {
  /** @type {string[]} */
  const problems = [];
  const { headers, problems: headerProblems } = parseSecurityHeaders(tree.read(FILES.headers));
  problems.push(...headerProblems);
  let webHeaderLabels = {};
  if (headers) {
    const mapped = headerLabels(headers);
    problems.push(...mapped.problems);
    webHeaderLabels = mapped.labels;
  }

  const composeText = tree.read(FILES.compose);
  const localText = tree.read(FILES.local);
  const devText = tree.read(FILES.dev);
  for (const [text, file] of [
    [composeText, FILES.compose],
    [localText, FILES.local],
    [devText, FILES.dev],
  ]) {
    checkComposeText(text, file, problems);
  }
  const compose = parseYaml(composeText, FILES.compose, problems);
  if (compose) checkProduction(compose, webHeaderLabels, problems);
  const local = parseYaml(localText, FILES.local, problems);
  const dev = parseYaml(devText, FILES.dev, problems);
  if (local) checkLocal(local, dev, problems);

  checkDockerignore(tree.read(FILES.dockerignore), tree.files, problems);
  checkDockerfiles(tree.read, problems);
  return problems;
}

/** Tracked files plus untracked ones that are not ignored (plan-c decision 10). */
function listFiles(dir) {
  try {
    const out = execFileSync('git', ['ls-files', '--cached', '--others', '--exclude-standard'], {
      cwd: dir,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    });
    return out
      .split('\n')
      .filter(Boolean)
      .filter((f) => existsSync(path.join(dir, f)));
  } catch {
    // Not a git checkout: walk the directory.
    /** @type {string[]} */
    const out = [];
    const walk = (rel) => {
      for (const name of readdirSync(path.join(dir, rel))) {
        if (name === '.git' || name === 'node_modules' || name === 'target') continue;
        const child = rel ? `${rel}/${name}` : name;
        if (statSync(path.join(dir, child)).isDirectory()) walk(child);
        else out.push(child);
      }
    };
    walk('');
    return out;
  }
}

/**
 * @param {string} dir
 * @returns {Tree}
 */
export function loadTree(dir) {
  return {
    read: (rel) => {
      const file = path.join(dir, rel);
      return existsSync(file) ? readFileSync(file, 'utf8') : null;
    },
    files: listFiles(dir),
  };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const dir = process.argv[2] ? path.resolve(process.argv[2]) : ROOT;
  const problems = checkCompose(loadTree(dir));
  if (problems.length > 0) {
    console.error(`check-compose: ${problems.length} problem(s)`);
    for (const p of problems) console.error(`  ${p}`);
    process.exit(1);
  }
  console.log('check-compose: ok');
}
