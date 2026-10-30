import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { FILES, checkCompose, loadTree } from '../check-compose.mjs';

const FIXTURES = new URL('../__fixtures__/check-compose/', import.meta.url);
const fixture = (name) => fileURLToPath(new URL(`${name}/`, FIXTURES));
const valid = loadTree(fixture('valid'));

/**
 * One failing fixture per rule: the valid fixture with one edit. Each edit asserts that the text
 * it replaces exists, so a drift of the valid fixture breaks the test instead of hiding a rule.
 * @param {Record<string, [string, string][]>} edits file → [from, to] replacements
 * @param {string[]} [extraFiles] files added to the listing
 */
function mutated(edits, extraFiles = []) {
  /** @type {Record<string, string | null>} */
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
  return {
    read: (rel) => (rel in changed ? changed[rel] : valid.read(rel)),
    files: [...valid.files, ...extraFiles],
  };
}

const C = FILES.compose;
const L = FILES.local;
const D = FILES.dev;
const DI = FILES.dockerignore;
const SD = FILES.serverDockerfile;
const WD = FILES.webDockerfile;
const H = FILES.headers;

const SERVER_IMAGE = 'server:${IMAGE_TAG:?IMAGE_TAG is required}';
const WEB_ENV = '      INTERNAL_API_URL: http://torneo-server:8080\n';
const SERVER_ENV = '      RUST_LOG: info\n';
const SERVER_LOGGING =
  "    logging:\n      driver: json-file\n      options:\n        max-size: '10m'\n        max-file: '3'\n    networks:\n      torneo-edge:";
const WEB_LOGGING =
  "    logging:\n      driver: json-file\n      options:\n        max-size: '10m'\n        max-file: '3'\n    networks:\n      - torneo-edge";
const HEADERS = 'traefik.http.middlewares.torneo-web-headers.headers';

/** @type {[string, Record<string, [string, string][]>, RegExp][]} */
const CASES = [
  // docker-compose.yml: services and images
  [
    'extra service',
    { [C]: [['\n  web:\n', '\n  postgres:\n    image: x\n\n  web:\n']] },
    /services: must be exactly server and web/,
  ],
  [
    'build',
    {
      [C]: [
        [
          `    image: ghcr.io/kike1303/torneo-absurdo/${SERVER_IMAGE}\n`,
          `    image: ghcr.io/kike1303/torneo-absurdo/${SERVER_IMAGE}\n    build: .\n`,
        ],
      ],
    },
    /services\.server: no-build/,
  ],
  [
    'ports',
    {
      [C]: [
        [
          '    read_only: true\n    # Sized',
          "    read_only: true\n    ports: ['3000:3000']\n    # Sized",
        ],
      ],
    },
    /services\.web: no-ports/,
  ],
  ['image latest', { [C]: [[SERVER_IMAGE, 'server:latest']] }, /services\.server: image: must be/],
  [
    'image without :?',
    { [C]: [[SERVER_IMAGE, 'server:${IMAGE_TAG}']] },
    /services\.server: image: must be/,
  ],
  ['top-level name', { [C]: [['services:\n', 'name: torneo\nservices:\n']] }, /no-name/],
  // labels
  [
    'missing label',
    { [C]: [['      - traefik.http.routers.torneo-api.priority=100\n', '']] },
    /labels: missing traefik\.http\.routers\.torneo-api\.priority=100/,
  ],
  [
    'unexpected label',
    {
      [C]: [
        [
          '      - traefik.enable=true\n',
          '      - traefik.enable=true\n      - traefik.http.routers.torneo-api.tls.domains[0].main=x\n',
        ],
      ],
    },
    /labels: unexpected traefik\.http\.routers\.torneo-api\.tls\.domains/,
  ],
  [
    'access log on',
    {
      [C]: [
        ['torneo-web.observability.accesslogs=false', 'torneo-web.observability.accesslogs=true'],
      ],
    },
    /torneo-web\.observability\.accesslogs must be false/,
  ],
  ['wrong api rule', { [C]: [['|| PathPrefix(`/ws`))\n', ')\n']] }, /torneo-api\.rule must be/],
  [
    'headers middleware missing',
    { [C]: [['      - traefik.http.routers.torneo-web.middlewares=torneo-web-headers\n', '']] },
    /labels: missing traefik\.http\.routers\.torneo-web\.middlewares=torneo-web-headers/,
  ],
  [
    'header value missing',
    { [C]: [[`      - ${HEADERS}.stsPreload=false\n`, '']] },
    /labels: missing .*stspreload=false/,
  ],
  [
    'forceSTSHeader',
    {
      [C]: [
        [
          `      - ${HEADERS}.stsPreload=false\n`,
          `      - ${HEADERS}.stsPreload=false\n      - ${HEADERS}.forceSTSHeader=true\n`,
        ],
      ],
    },
    /hsts: forceSTSHeader is forbidden/,
  ],
  [
    'stsIncludeSubdomains true',
    { [C]: [[`${HEADERS}.stsIncludeSubdomains=false`, `${HEADERS}.stsIncludeSubdomains=true`]] },
    /hsts: .*stsincludesubdomains must not be true/,
  ],
  [
    'stsPreload true',
    { [C]: [[`${HEADERS}.stsPreload=false`, `${HEADERS}.stsPreload=true`]] },
    /hsts: .*stspreload must not be true/,
  ],
  [
    'stsSeconds changed',
    { [C]: [[`${HEADERS}.stsSeconds=31536000`, `${HEADERS}.stsSeconds=63072000`]] },
    /stsseconds must be 31536000/,
  ],
  [
    'headers on the http router',
    {
      [C]: [
        [
          'torneo-web-http.middlewares=torneo-https-redirect',
          'torneo-web-http.middlewares=torneo-https-redirect,torneo-web-headers',
        ],
      ],
    },
    /web-headers-scope: router torneo-web-http/,
  ],
  [
    'headers on server',
    {
      [C]: [
        [
          '      - traefik.http.services.torneo-api.loadbalancer.server.port=8080\n',
          `      - traefik.http.services.torneo-api.loadbalancer.server.port=8080\n      - ${HEADERS}.stsSeconds=31536000\n`,
        ],
      ],
    },
    /services\.server: web-headers-scope/,
  ],
  [
    'redirect differs',
    {
      [C]: [
        [
          '      - traefik.http.middlewares.torneo-https-redirect.redirectscheme.permanent=true\n\n  web:',
          '      - traefik.http.middlewares.torneo-https-redirect.redirectscheme.permanent=false\n\n  web:',
        ],
      ],
    },
    /https-redirect: torneo-https-redirect must be identical/,
  ],
  // networks
  [
    'network not external',
    { [C]: [['  torneo-db:\n    external: true', '  torneo-db:\n    external: false']] },
    /networks: torneo-db must be declared with external: true/,
  ],
  [
    'web on torneo-db',
    {
      [C]: [
        [
          '    networks:\n      - torneo-edge\n',
          '    networks:\n      - torneo-edge\n      - torneo-db\n',
        ],
      ],
    },
    /web must be only on torneo-edge/,
  ],
  [
    'server without alias',
    { [C]: [['        aliases: [torneo-server]\n', '        aliases: [server]\n']] },
    /alias torneo-server on torneo-edge/,
  ],
  // healthchecks, startup, memory, restart, logging
  [
    'healthcheck interval',
    { [C]: [['      interval: 15s\n', '      interval: 30s\n']] },
    /services\.server: healthcheck: interval must be 15s/,
  ],
  [
    'healthcheck shell form',
    {
      [C]: [
        [
          "      test: ['CMD', '/server', 'healthcheck']",
          "      test: ['CMD-SHELL', '/server healthcheck']",
        ],
      ],
    },
    /services\.server: healthcheck: test must be/,
  ],
  [
    'depends_on without condition',
    { [C]: [['        condition: service_healthy\n', '        condition: service_started\n']] },
    /depends-on: server with condition service_healthy/,
  ],
  [
    'memswap differs',
    { [C]: [['    memswap_limit: 256m\n', '    memswap_limit: 512m\n']] },
    /services\.server: memory: mem_limit and memswap_limit must both be 256m/,
  ],
  [
    'restart always',
    { [C]: [['    restart: unless-stopped\n', '    restart: always\n']] },
    /services\.server: restart: must be unless-stopped/,
  ],
  [
    'logging missing',
    { [C]: [[WEB_LOGGING, '    networks:\n      - torneo-edge']] },
    /services\.web: logging: must be driver json-file/,
  ],
  [
    'logging size',
    { [C]: [[SERVER_LOGGING, SERVER_LOGGING.replace("'10m'", "'50m'")]] },
    /services\.server: logging: must be driver json-file/,
  ],
  [
    'stop_grace_period missing',
    { [C]: [['    stop_grace_period: 15s\n', '']] },
    /stop-grace: stop_grace_period must be 15s/,
  ],
  // hardening
  [
    'platform',
    {
      [C]: [
        [
          '    platform: linux/amd64\n    restart: unless-stopped\n    stop_grace_period',
          '    platform: linux/arm64\n    restart: unless-stopped\n    stop_grace_period',
        ],
      ],
    },
    /services\.server: platform: must be linux\/amd64/,
  ],
  [
    'read_only false',
    { [C]: [['    read_only: true\n    cap_drop', '    read_only: false\n    cap_drop']] },
    /services\.server: read_only: must be true/,
  ],
  [
    'cap_drop missing',
    { [C]: [['    cap_drop: [ALL]\n', '']] },
    /services\.server: cap_drop: must be \[ALL\]/,
  ],
  [
    'no-new-privileges missing',
    { [C]: [["    security_opt: ['no-new-privileges:true']\n", '']] },
    /services\.server: security_opt/,
  ],
  [
    'tmpfs without size',
    { [C]: [['      - /tmp:size=16m\n', '      - /tmp\n']] },
    /tmpfs: \/tmp with a size= is required/,
  ],
  // environment
  [
    'env outside the list',
    { [C]: [[WEB_ENV, '      FOO: bar\n' + WEB_ENV]] },
    /services\.web: env-allowlist: FOO/,
  ],
  [
    'ORIGIN in web',
    { [C]: [[WEB_ENV, '      ORIGIN: https://x\n' + WEB_ENV]] },
    /services\.web: forbidden-env: ORIGIN must not be set/,
  ],
  [
    'PROTOCOL_HEADER in web',
    { [C]: [[WEB_ENV, '      PROTOCOL_HEADER: x-forwarded-proto\n' + WEB_ENV]] },
    /forbidden-env: PROTOCOL_HEADER/,
  ],
  [
    'HOST_HEADER in web',
    { [C]: [[WEB_ENV, '      HOST_HEADER: x-forwarded-host\n' + WEB_ENV]] },
    /forbidden-env: HOST_HEADER/,
  ],
  [
    'PORT_HEADER in web',
    { [C]: [[WEB_ENV, '      PORT_HEADER: x-forwarded-port\n' + WEB_ENV]] },
    /forbidden-env: PORT_HEADER/,
  ],
  // 008 decision 12 (step 3): web reaches the server by its alias, with the server's secret.
  [
    'INTERNAL_API_URL missing in web',
    { [C]: [['      INTERNAL_API_URL: http://torneo-server:8080\n', '']] },
    /services\.web: env: INTERNAL_API_URL is required/,
  ],
  [
    'INTERNAL_API_URL to another host',
    {
      [C]: [
        [
          '      INTERNAL_API_URL: http://torneo-server:8080\n',
          '      INTERNAL_API_URL: https://torneo.example\n',
        ],
      ],
    },
    /services\.web: env: INTERNAL_API_URL must be http:\/\/torneo-server:8080/,
  ],
  [
    'INTERNAL_API_SECRET missing in web',
    {
      [C]: [
        [
          '      INTERNAL_API_SECRET: ${INTERNAL_API_SECRET:?INTERNAL_API_SECRET is required}\n      ADDRESS_HEADER',
          '      ADDRESS_HEADER',
        ],
      ],
    },
    /services\.web: env: INTERNAL_API_SECRET is required/,
  ],
  [
    'INTERNAL_API_SECRET literal in web',
    {
      [C]: [
        [
          '      INTERNAL_API_SECRET: ${INTERNAL_API_SECRET:?INTERNAL_API_SECRET is required}\n      ADDRESS_HEADER',
          '      INTERNAL_API_SECRET: c2VjcmV0\n      ADDRESS_HEADER',
        ],
      ],
    },
    /services\.web: env: INTERNAL_API_SECRET must be \$\{INTERNAL_API_SECRET:\?\}/,
  ],
  [
    'BIND_ADDR in server',
    { [C]: [[SERVER_ENV, SERVER_ENV + '      BIND_ADDR: 0.0.0.0\n']] },
    /services\.server: forbidden-env: BIND_ADDR must not be set/,
  ],
  [
    'PUBLIC_ORIGIN differs',
    {
      [C]: [
        [
          '      PUBLIC_ORIGIN: https://${PUBLIC_HOST:?PUBLIC_HOST is required}\n      RUST_LOG',
          '      PUBLIC_ORIGIN: http://${PUBLIC_HOST:?PUBLIC_HOST is required}\n      RUST_LOG',
        ],
      ],
    },
    /public-origin: PUBLIC_ORIGIN must be identical/,
  ],
  [
    'RUST_LOG debug',
    { [C]: [[SERVER_ENV, '      RUST_LOG: info,server=debug\n']] },
    /rust-log: RUST_LOG must not be more verbose than info/,
  ],
  ['RUST_LOG missing', { [C]: [[SERVER_ENV, '']] }, /env: RUST_LOG is required/],
  [
    'TRUSTED_PROXIES missing',
    { [C]: [['      TRUSTED_PROXIES: 10.250.0.2/32\n', '']] },
    /services\.server: env: TRUSTED_PROXIES is required/,
  ],
  [
    'INTERNAL_API_SECRET missing',
    {
      [C]: [
        [
          '      INTERNAL_API_SECRET: ${INTERNAL_API_SECRET:?INTERNAL_API_SECRET is required}\n',
          '',
        ],
      ],
    },
    /services\.server: env: INTERNAL_API_SECRET is required/,
  ],
  [
    'INTERNAL_API_SECRET without :?',
    {
      [C]: [
        [
          '      INTERNAL_API_SECRET: ${INTERNAL_API_SECRET:?INTERNAL_API_SECRET is required}\n',
          '      INTERNAL_API_SECRET: ${INTERNAL_API_SECRET}\n',
        ],
      ],
    },
    /services\.server: env: INTERNAL_API_SECRET must be \$\{INTERNAL_API_SECRET:\?\}/,
  ],
  [
    'IP_HASH_KEY missing',
    { [C]: [['      IP_HASH_KEY: ${IP_HASH_KEY:?IP_HASH_KEY is required}\n', '']] },
    /services\.server: env: IP_HASH_KEY is required/,
  ],
  [
    'IP_HASH_KEY as a literal',
    {
      [C]: [
        [
          '      IP_HASH_KEY: ${IP_HASH_KEY:?IP_HASH_KEY is required}\n',
          '      IP_HASH_KEY: mtPdFLwnW5YGomZ3cu58sF-v6WhmO0aQRTksx_Ypr90\n',
        ],
      ],
    },
    /services\.server: env: IP_HASH_KEY must be \$\{IP_HASH_KEY:\?\}/,
  ],
  [
    'GOOGLE_CLIENT_ID missing',
    { [C]: [['      GOOGLE_CLIENT_ID: ${GOOGLE_CLIENT_ID:?GOOGLE_CLIENT_ID is required}\n', '']] },
    /services\.server: env: GOOGLE_CLIENT_ID is required/,
  ],
  [
    'GOOGLE_CLIENT_ID as a literal',
    {
      [C]: [
        [
          '      GOOGLE_CLIENT_ID: ${GOOGLE_CLIENT_ID:?GOOGLE_CLIENT_ID is required}\n',
          '      GOOGLE_CLIENT_ID: filler\n',
        ],
      ],
    },
    /services\.server: env: GOOGLE_CLIENT_ID must be \$\{GOOGLE_CLIENT_ID:\?\}/,
  ],
  [
    'GOOGLE_CLIENT_SECRET missing',
    {
      [C]: [
        [
          '      GOOGLE_CLIENT_SECRET: ${GOOGLE_CLIENT_SECRET:?GOOGLE_CLIENT_SECRET is required}\n',
          '',
        ],
      ],
    },
    /services\.server: env: GOOGLE_CLIENT_SECRET is required/,
  ],
  [
    'GOOGLE_CLIENT_SECRET as a literal',
    {
      [C]: [
        [
          '      GOOGLE_CLIENT_SECRET: ${GOOGLE_CLIENT_SECRET:?GOOGLE_CLIENT_SECRET is required}\n',
          '      GOOGLE_CLIENT_SECRET: filler\n',
        ],
      ],
    },
    /services\.server: env: GOOGLE_CLIENT_SECRET must be \$\{GOOGLE_CLIENT_SECRET:\?\}/,
  ],
  [
    'DISCORD_CLIENT_ID missing',
    {
      [C]: [['      DISCORD_CLIENT_ID: ${DISCORD_CLIENT_ID:?DISCORD_CLIENT_ID is required}\n', '']],
    },
    /services\.server: env: DISCORD_CLIENT_ID is required/,
  ],
  [
    'DISCORD_CLIENT_ID as a literal',
    {
      [C]: [
        [
          '      DISCORD_CLIENT_ID: ${DISCORD_CLIENT_ID:?DISCORD_CLIENT_ID is required}\n',
          '      DISCORD_CLIENT_ID: filler\n',
        ],
      ],
    },
    /services\.server: env: DISCORD_CLIENT_ID must be \$\{DISCORD_CLIENT_ID:\?\}/,
  ],
  [
    'DISCORD_CLIENT_SECRET missing',
    {
      [C]: [
        [
          '      DISCORD_CLIENT_SECRET: ${DISCORD_CLIENT_SECRET:?DISCORD_CLIENT_SECRET is required}\n',
          '',
        ],
      ],
    },
    /services\.server: env: DISCORD_CLIENT_SECRET is required/,
  ],
  [
    'DISCORD_CLIENT_SECRET as a literal',
    {
      [C]: [
        [
          '      DISCORD_CLIENT_SECRET: ${DISCORD_CLIENT_SECRET:?DISCORD_CLIENT_SECRET is required}\n',
          '      DISCORD_CLIENT_SECRET: filler\n',
        ],
      ],
    },
    /services\.server: env: DISCORD_CLIENT_SECRET must be \$\{DISCORD_CLIENT_SECRET:\?\}/,
  ],
  [
    'DAILY_GRANT missing',
    { [C]: [["      DAILY_GRANT: '100'\n", '']] },
    /services\.server: env: DAILY_GRANT is required/,
  ],
  ...['0', '10001', '1.5', '+5', '1e3', 'abc', '${DAILY_GRANT:?x}'].map(
    (value) =>
      /** @type {const} */ ([
        `DAILY_GRANT ${value}`,
        { [C]: [["      DAILY_GRANT: '100'\n", `      DAILY_GRANT: '${value}'\n`]] },
        /services\.server: daily-grant: DAILY_GRANT must be an integer from 1 to 10000/,
      ]),
  ),
  [
    'TRUSTED_PROXIES is the subnet',
    { [C]: [['      TRUSTED_PROXIES: 10.250.0.2/32\n', '      TRUSTED_PROXIES: 10.250.0.0/28\n']] },
    /services\.server: env: TRUSTED_PROXIES must be 10\.250\.0\.2\/32/,
  ],
  [
    'web value changed',
    { [C]: [["      XFF_DEPTH: '1'\n", "      XFF_DEPTH: '2'\n"]] },
    /env: XFF_DEPTH must be 1/,
  ],
  [
    'ENABLE_DEV_CATALOG',
    { [C]: [[WEB_ENV, '      ENABLE_DEV_CATALOG: 1\n' + WEB_ENV]] },
    /docker-compose\.yml: dev-only-vars: ENABLE_DEV_CATALOG/,
  ],
  [
    'ENABLE_WS_ECHO',
    { [C]: [[SERVER_ENV, SERVER_ENV + '      ENABLE_WS_ECHO: 1\n']] },
    /dev-only-vars: ENABLE_WS_ECHO/,
  ],
  // compose.local.yml
  [
    'local changes an image',
    { [L]: [['  web:\n    ports:', '  web:\n    image: nginx\n    ports:']] },
    /services\.web: only-adds: only ports may be added/,
  ],
  [
    'local changes the environment',
    {
      [L]: [
        ['  server:\n    ports:', '  server:\n    environment:\n      RUST_LOG: debug\n    ports:'],
      ],
    },
    /services\.server: only-adds/,
  ],
  [
    'local port on all interfaces',
    { [L]: [["      - '127.0.0.1:3000:3000'", "      - '3000:3000'"]] },
    /loopback-ports: 3000:3000 must bind 127\.0\.0\.1/,
  ],
  [
    'local postgres port public',
    {
      [L]: [
        [
          '    networks:\n      - torneo-db\n',
          "    networks:\n      - torneo-db\n    ports: ['5432:5432']\n",
        ],
      ],
    },
    /services\.postgres: loopback-ports/,
  ],
  [
    'local postgres digest differs',
    { [L]: [['@sha256:742f', '@sha256:0000']] },
    /services\.postgres: image must equal/,
  ],
  [
    'local postgres memory',
    { [L]: [['    memswap_limit: 512m\n', '    memswap_limit: 1g\n']] },
    /services\.postgres: memory/,
  ],
  [
    'local postgres on torneo-edge',
    { [L]: [['      - torneo-db\n', '      - torneo-db\n      - torneo-edge\n']] },
    /postgres must be only on torneo-db/,
  ],
  [
    'local postgres without a log parameter',
    { [L]: [['      - -c\n      - log_error_verbosity=terse\n', '']] },
    /compose\.local\.yml: services\.postgres: command: must be/,
  ],
  [
    'local postgres logs failing statements',
    { [L]: [['log_min_error_statement=panic', 'log_min_error_statement=error']] },
    /compose\.local\.yml: services\.postgres: command: must be/,
  ],
  [
    'dev postgres without command',
    { [D]: [['    command:\n', '    unused:\n']] },
    /compose\.dev\.yml: services\.postgres: command: must be/,
  ],
  [
    'dev postgres with default verbosity',
    { [D]: [['log_error_verbosity=terse', 'log_error_verbosity=default']] },
    /compose\.dev\.yml: services\.postgres: command: must be/,
  ],
  [
    'dev postgres samples transactions',
    { [D]: [['log_transaction_sample_rate=0', 'log_transaction_sample_rate=0.1']] },
    /compose\.dev\.yml: services\.postgres: command: must be/,
  ],
  [
    'dev postgres with the tuning of compose.local.yml',
    {
      [D]: [['      - postgres\n', '      - postgres\n      - -c\n      - shared_buffers=128MB\n']],
    },
    /compose\.dev\.yml: services\.postgres: command: must be/,
  ],
  [
    'local top-level networks',
    { [L]: [['\nvolumes:\n', '\nnetworks:\n  torneo-edge:\n    external: false\n\nvolumes:\n']] },
    /only-adds: top-level networks/,
  ],
  [
    'local dev-only var',
    {
      [L]: [
        ['      POSTGRES_DB: torneo\n', '      POSTGRES_DB: torneo\n      ENABLE_WS_ECHO: 1\n'],
      ],
    },
    /compose\.local\.yml: dev-only-vars: ENABLE_WS_ECHO/,
  ],
  // .dockerignore
  [
    '.env without **/',
    { [DI]: [['\n**/.env\n', '\n.env\n']] },
    /\.dockerignore: entries: missing \*\*\/\.env$/,
  ],
  ['target without **/', { [DI]: [['**/target\n', 'target\n']] }, /entries: missing \*\*\/target/],
  ['private/ missing', { [DI]: [['private/\n', '']] }, /entries: missing private\//],
  ['tools/ missing', { [DI]: [['\ntools/\n', '\n']] }, /entries: missing tools\//],
  [
    'example before .env.*',
    { [DI]: [['**/.env.*\n!**/.env.example\n', '!**/.env.example\n**/.env.*\n']] },
    /!\*\*\/\.env\.example must come after/,
  ],
  // Dockerfiles
  [
    'FROM without digest',
    {
      [SD]: [
        [
          'FROM gcr.io/distroless/static-debian13:nonroot@sha256:516757106de35205efb70b91ba03f6f1298424a0678c603255cf2852a7000087',
          'FROM gcr.io/distroless/static-debian13:nonroot',
        ],
      ],
    },
    /apps\/server\/Dockerfile:\d+: digest/,
  ],
  [
    'FROM latest',
    { [WD]: [['node:22.23.4-bookworm-slim@', 'node:latest@']] },
    /apps\/web\/Dockerfile:\d+: no-latest/,
  ],
  [
    'rust version differs',
    { [SD]: [['rust:1.99.1-slim-bookworm@', 'rust:1.99.0-slim-bookworm@']] },
    /rust-version: rust:1\.99\.0 must equal rust-toolchain\.toml \(1\.99\.1\)/,
  ],
  [
    'node version differs',
    { [WD]: [['node:22.23.4-bookworm-slim@', 'node:22.23.5-bookworm-slim@']] },
    /node-version: node:22\.23\.5 must equal \.node-version/,
  ],
  [
    'web final base of another major',
    { [WD]: [['gcr.io/distroless/nodejs22-debian13', 'gcr.io/distroless/nodejs24-debian13']] },
    /node-major: the final base must be gcr\.io\/distroless\/nodejs22-/,
  ],
  [
    'final base not nonroot',
    { [SD]: [['static-debian13:nonroot@', 'static-debian13:latest@']] },
    /nonroot: the final base must be a :nonroot tag/,
  ],
  // SECURITY_HEADERS is the source of the Traefik values
  [
    'SECURITY_HEADERS max-age differs',
    { [H]: [["'max-age=31536000'", "'max-age=63072000'"]] },
    /stsseconds must be 63072000 \(found 31536000\)/,
  ],
  [
    'SECURITY_HEADERS with includeSubDomains',
    { [H]: [["'max-age=31536000'", "'max-age=31536000; includeSubDomains'"]] },
    /security-headers: HSTS must not have includeSubDomains/,
  ],
  [
    'SECURITY_HEADERS referrer differs',
    { [H]: [["'strict-origin-when-cross-origin'", "'no-referrer'"]] },
    /referrerpolicy must be no-referrer/,
  ],
  [
    'SECURITY_HEADERS COOP differs',
    {
      [H]: [
        [
          "'cross-origin-opener-policy': 'same-origin'",
          "'cross-origin-opener-policy': 'unsafe-none'",
        ],
      ],
    },
    /security-headers: cross-origin-opener-policy must be same-origin/,
  ],
  [
    'SECURITY_HEADERS Permissions-Policy relaxed',
    { [H]: [['camera=(), microphone=()', 'camera=(self), microphone=()']] },
    /security-headers: permissions-policy must be camera=\(\)/,
  ],
  [
    'COOP label missing',
    {
      [C]: [
        [`      - ${HEADERS}.customResponseHeaders.Cross-Origin-Opener-Policy=same-origin\n`, ''],
      ],
    },
    /labels: missing traefik\.http\.middlewares\.torneo-web-headers\.headers\.customresponseheaders\.cross-origin-opener-policy=same-origin/,
  ],
  [
    'Permissions-Policy label differs',
    {
      [C]: [
        [`${HEADERS}.permissionsPolicy=camera=()`, `${HEADERS}.permissionsPolicy=camera=(self)`],
      ],
    },
    /permissionspolicy must be camera=\(\), microphone/,
  ],
  [
    'COOP label spelled differently',
    {
      [C]: [
        [
          `${HEADERS}.customResponseHeaders.Cross-Origin-Opener-Policy`,
          `${HEADERS}.customresponseheaders.cross-origin-opener-policy`,
        ],
      ],
    },
    /must be spelled .*customResponseHeaders\.Cross-Origin-Opener-Policy/,
  ],
  [
    'SECURITY_HEADERS entry missing',
    { [H]: [["  'x-content-type-options': 'nosniff',\n", '']] },
    /security-headers: SECURITY_HEADERS must have exactly/,
  ],
];

const SERVER_START = `  server:\n    image: ghcr.io/kike1303/torneo-absurdo/${SERVER_IMAGE}\n`;
const WEB_START = '  web:\n    image: ghcr.io/kike1303/torneo-absurdo/web:';
const SERVER_HC = "      test: ['CMD', '/server', 'healthcheck']\n";
const PG_START = '  postgres:\n';

/** PR #4 review: RUST_LOG, YAML merge keys and closed key lists. */
/** @type {[string, Record<string, [string, string][]>, RegExp][]} */
const REVIEW_CASES = [
  // RUST_LOG must be exactly info (RF-C4): levels, numbers and bare targets all change the output.
  ...['warn', 'off', 'server', "'info,server=5'", 'info,sqlx'].map(
    (value) =>
      /** @type {[string, Record<string, [string, string][]>, RegExp]} */ ([
        `RUST_LOG ${value}`,
        { [C]: [[SERVER_ENV, `      RUST_LOG: ${value}\n`]] },
        /services\.server: env: RUST_LOG must be info/,
      ]),
  ),
  // YAML anchors, aliases and merge keys: compose resolves them, the parser does not.
  [
    'merge key with an anchor',
    {
      [C]: [
        [
          'services:\n',
          "x-evil: &evil\n  ports: ['0.0.0.0:8080:8080']\n  privileged: true\n\nservices:\n",
        ],
        [SERVER_START, `${SERVER_START}    <<: *evil\n`],
      ],
    },
    /docker-compose\.yml: yaml-anchors: merge key << is not allowed/,
  ],
  [
    'alias without merge',
    {
      [C]: [
        [
          '    mem_limit: 256m\n    memswap_limit: 256m\n',
          '    mem_limit: &m 256m\n    memswap_limit: *m\n',
        ],
      ],
    },
    /docker-compose\.yml: yaml-anchors: alias \*m is not allowed/,
  ],
  [
    'merge key in compose.local.yml',
    { [L]: [[PG_START, `${PG_START}    <<: { ports: ['0.0.0.0:5432:5432'] }\n`]] },
    /compose\.local\.yml: yaml-anchors: merge key << is not allowed/,
  ],
  // Closed list of top-level keys
  [
    'include at the root',
    { [C]: [['services:\n', 'include:\n  - extra.yml\n\nservices:\n']] },
    /docker-compose\.yml: root-keys: include is not allowed/,
  ],
  [
    'x- extension at the root',
    { [C]: [['services:\n', 'x-defaults: {}\n\nservices:\n']] },
    /root-keys: x-defaults is not allowed/,
  ],
  [
    'include in compose.local.yml',
    { [L]: [['services:\n', 'include:\n  - extra.yml\n\nservices:\n']] },
    /compose\.local\.yml: only-adds: top-level include/,
  ],
  // Closed list of service keys
  ...[
    ['privileged in web', 'web', '    privileged: true\n', 'privileged'],
    ['cap_add in web', 'web', '    cap_add: [SYS_ADMIN]\n', 'cap_add'],
    ["user '0' in web", 'web', "    user: '0'\n", 'user'],
    ['env_file in server', 'server', '    env_file: [../apps/server/.env]\n', 'env_file'],
    ['extends in server', 'server', '    extends: { file: extra.yml, service: base }\n', 'extends'],
    [
      'docker.sock in server',
      'server',
      "    volumes: ['/var/run/docker.sock:/var/run/docker.sock']\n",
      'volumes',
    ],
    ['network_mode in server', 'server', '    network_mode: host\n', 'network_mode'],
  ].map(
    ([name, svc, line, key]) =>
      /** @type {[string, Record<string, [string, string][]>, RegExp]} */ ([
        name,
        {
          [C]: [
            [
              svc === 'web' ? WEB_START : SERVER_START,
              (svc === 'web' ? WEB_START : SERVER_START).replace(/^(  \w+:\n)/, `$1${line}`),
            ],
          ],
        },
        new RegExp(`services\\.${svc}: service-keys: ${key} is not allowed`),
      ]),
  ),
  [
    'healthcheck disabled',
    { [C]: [[SERVER_HC, `${SERVER_HC}      disable: true\n`]] },
    /services\.server: healthcheck: keys must be exactly/,
  ],
  [
    'logging with another option key',
    { [C]: [["        max-file: '3'\n", "        max-file: '3'\n      mode: non-blocking\n"]] },
    /services\.server: logging/,
  ],
  [
    'depends_on with more keys',
    {
      [C]: [
        [
          '        condition: service_healthy\n',
          '        condition: service_healthy\n        restart: true\n',
        ],
      ],
    },
    /depends-on: server with condition service_healthy/,
  ],
  [
    'external network renamed',
    {
      [C]: [
        [
          '  torneo-edge:\n    external: true\n',
          '  torneo-edge:\n    external: true\n    name: coolify\n',
        ],
      ],
    },
    /networks: torneo-edge must be declared with external: true and nothing else/,
  ],
  [
    'server with a fixed IP',
    {
      [C]: [
        [
          '        aliases: [torneo-server]\n',
          '        aliases: [torneo-server]\n        ipv4_address: 10.250.0.2\n',
        ],
      ],
    },
    /alias torneo-server on torneo-edge/,
  ],
  // postgres of compose.local.yml
  [
    'privileged postgres',
    { [L]: [[PG_START, `${PG_START}    privileged: true\n`]] },
    /services\.postgres: service-keys: privileged is not allowed/,
  ],
  [
    'postgres with docker.sock',
    {
      [L]: [
        [
          '      - torneo-local-pg:/var/lib/postgresql/data\n',
          '      - torneo-local-pg:/var/lib/postgresql/data\n      - /var/run/docker.sock:/var/run/docker.sock\n',
        ],
      ],
    },
    /services\.postgres: volumes: must be/,
  ],
  [
    'postgres env outside the list',
    {
      [L]: [
        [
          '      POSTGRES_DB: torneo\n',
          '      POSTGRES_DB: torneo\n      POSTGRES_HOST_AUTH_METHOD: trust\n',
        ],
      ],
    },
    /services\.postgres: env-allowlist: POSTGRES_HOST_AUTH_METHOD/,
  ],
  [
    'postgres healthcheck disabled',
    {
      [L]: [
        [
          "      test: ['CMD-SHELL', 'pg_isready -U torneo -d torneo']\n",
          "      test: ['CMD-SHELL', 'pg_isready -U torneo -d torneo']\n      disable: true\n",
        ],
      ],
    },
    /services\.postgres: healthcheck: keys must be exactly/,
  ],
  [
    'another local volume',
    {
      [L]: [
        [
          '  torneo-local-pg:\n',
          '  torneo-local-pg:\n  host:\n    driver_opts: { type: none, o: bind, device: / }\n',
        ],
      ],
    },
    /compose\.local\.yml: volumes: only torneo-local-pg/,
  ],
  // .dockerignore re-inclusions
  [
    're-included .env.production',
    { [DI]: [['docs/\n', 'docs/\n!**/.env.production\n']] },
    /\.dockerignore: reinclude: !\*\*\/\.env\.production is not allowed/,
  ],
  [
    're-included private file',
    { [DI]: [['docs/\n', 'docs/\n!private/notes.md\n']] },
    /reinclude: !private\/notes\.md is not allowed/,
  ],
  // Explicit YAML tags: Compose honours !reset and !override even in the base file and drops or
  // replaces the key, while the parser keeps the value (security re-review of PR #4).
  [
    '!reset on cap_drop',
    { [C]: [['    cap_drop: [ALL]\n', '    cap_drop: !reset [ALL]\n']] },
    /docker-compose\.yml: yaml-tags: !reset is not allowed/,
  ],
  [
    '!reset on labels',
    {
      [C]: [
        [
          '    labels:\n      - traefik.enable=true\n',
          '    labels: !reset\n      - traefik.enable=true\n',
        ],
      ],
    },
    /docker-compose\.yml: yaml-tags: !reset is not allowed/,
  ],
  [
    '!reset on the postgres volume',
    {
      [L]: [
        ['    volumes:\n      - torneo-local-pg', '    volumes: !reset\n      - torneo-local-pg'],
      ],
    },
    /compose\.local\.yml: yaml-tags: !reset is not allowed/,
  ],
  [
    '!override in compose.local.yml',
    {
      [L]: [
        [
          "    ports:\n      - '127.0.0.1:8080:8080'",
          "    ports: !override\n      - '127.0.0.1:8080:8080'",
        ],
      ],
    },
    /compose\.local\.yml: yaml-tags: !override is not allowed/,
  ],
  [
    '!!str on a value',
    { [C]: [['    restart: unless-stopped\n', '    restart: !!str unless-stopped\n']] },
    /yaml-tags: tag:yaml\.org,2002:str is not allowed/,
  ],
  // Label keys that differ only in case: Docker keeps both, so they must not collide and must be
  // spelled exactly as in RF-C4.
  [
    'labels colliding in case (STSPRELOAD)',
    {
      [C]: [
        [
          `      - ${HEADERS}.stsPreload=false\n`,
          `      - ${HEADERS}.STSPRELOAD=true\n      - ${HEADERS}.stsPreload=false\n`,
        ],
      ],
    },
    /labels: .*\.stsPreload collides with .*\.STSPRELOAD \(keys differ only in case\)/,
  ],
  [
    'labels colliding in case (empty middlewares)',
    {
      [C]: [
        [
          '      - traefik.http.routers.torneo-web.middlewares=torneo-web-headers\n',
          '      - traefik.http.routers.TORNEO-WEB.MIDDLEWARES=\n      - traefik.http.routers.torneo-web.middlewares=torneo-web-headers\n',
        ],
      ],
    },
    /labels: traefik\.http\.routers\.torneo-web\.middlewares collides with traefik\.http\.routers\.TORNEO-WEB\.MIDDLEWARES/,
  ],
];

/**
 * What Dependabot moves on its own: image digests and the versions of the node, rust and postgres
 * images, plus .node-version and the rust-toolchain.toml channel that check-compose ties to them.
 * Everything else must match the real files byte for byte (plan-c deviation 22).
 * @param {string} file
 * @param {string | null} text
 */
function forSync(file, text) {
  if (text === null) return null;
  if (file === FILES.nodeVersion) return '<version>\n';
  return text
    .replace(/@sha256:[0-9a-f]{64}/g, '@sha256:<digest>')
    .replace(/\b(node|rust|postgres):\d+(?:\.\d+)+(?=-)/g, '$1:<version>')
    .replace(/^channel = "[^"]*"$/m, 'channel = "<version>"');
}

/**
 * Files of FILES whose structure differs between the fixture and the real tree.
 * @param {{ read: (rel: string) => string | null }} fixtureTree
 * @param {{ read: (rel: string) => string | null }} realTree
 */
function drift(fixtureTree, realTree) {
  return Object.values(FILES).filter(
    (file) => forSync(file, fixtureTree.read(file)) !== forSync(file, realTree.read(file)),
  );
}

describe('valid fixture vs the real files (plan-c deviations 12 and 22)', () => {
  const real = loadTree(fileURLToPath(new URL('../../', import.meta.url)));
  /** @param {Record<string, [RegExp, string][]>} edits */
  const realWith = (edits) => ({
    read: (rel) => {
      let text = real.read(rel);
      for (const [from, to] of edits[rel] ?? []) {
        if (text === null || !from.test(text)) throw new Error(`${rel} has no ${from}`);
        text = text.replace(from, to);
      }
      return text;
    },
  });
  const digest = (c) => `@sha256:${c.repeat(64)}`;

  it('keeps the structure of the valid fixture in sync with the real files', () => {
    expect(drift(valid, real), 'resync scripts/__fixtures__/check-compose/valid/').toEqual([]);
  });

  it.each([
    ['server digests', { [SD]: [[/@sha256:[0-9a-f]{64}/g, digest('a')]] }],
    ['web digests', { [WD]: [[/@sha256:[0-9a-f]{64}/g, digest('b')]] }],
    [
      'postgres digest and minor',
      {
        [FILES.dev]: [
          [/postgres:17\.\d+-alpine@sha256:[0-9a-f]{64}/g, `postgres:17.99-alpine${digest('c')}`],
        ],
        [L]: [
          [/postgres:17\.\d+-alpine@sha256:[0-9a-f]{64}/g, `postgres:17.99-alpine${digest('c')}`],
        ],
      },
    ],
    [
      'node version with .node-version',
      {
        [WD]: [[/node:\d+(?:\.\d+)+-bookworm-slim/g, 'node:22.99.0-bookworm-slim']],
        [FILES.nodeVersion]: [[/^.*$/m, '22.99.0']],
      },
    ],
    [
      'rust version with the toolchain',
      {
        [SD]: [[/rust:\d+(?:\.\d+)+-slim-bookworm/g, 'rust:1.999.0-slim-bookworm']],
        [FILES.toolchain]: [[/^channel = "[^"]*"$/m, 'channel = "1.999.0"']],
      },
    ],
  ])('does not ask for a resync on a Dependabot bump: %s', (_name, edits) => {
    expect(drift(valid, realWith(edits))).toEqual([]);
  });

  it.each([
    ['a new service line', C, [/\nservices:\n/, '\nservices:\n  extra:\n    image: x\n']],
    ['the zig version', SD, [/0\.15\.2/g, '0.15.3']],
    ['an image suffix', WD, [/-bookworm-slim@/, '-trixie-slim@']],
    ['the distroless tag', SD, [/:nonroot@/, ':latest@']],
  ])('still asks for a resync on a structural change: %s', (_name, file, edit) => {
    expect(drift(valid, realWith({ [file]: [edit] }))).toEqual([file]);
  });
});

describe('check-compose (RF-C3, RF-C4, RF-C6)', () => {
  it.each(REVIEW_CASES)('fails on: %s', (_name, edits, pattern) => {
    const problems = checkCompose(mutated(edits));
    expect(
      problems.some((p) => pattern.test(p)),
      problems.join('\n') || '(no problems)',
    ).toBe(true);
  });

  it('passes on the valid fixture', () => {
    expect(checkCompose(valid)).toEqual([]);
  });

  it('passes on the real repository', () => {
    expect(checkCompose(loadTree(fileURLToPath(new URL('../../', import.meta.url))))).toEqual([]);
  });

  it('accepts the postgres service of compose.local.yml', () => {
    const problems = checkCompose(valid);
    expect(problems.filter((p) => p.includes('postgres'))).toEqual([]);
    expect(valid.read(L)).toContain('  postgres:\n');
  });

  it('requires the exact spelling of each label key, not only the same key without case', () => {
    const tree = mutated({
      [C]: [
        ['torneo-web.observability.accesslogs=false', 'torneo-web.observability.accessLogs=false'],
      ],
    });
    expect(checkCompose(tree)).toContain(
      `${C}: services.web: labels: traefik.http.routers.torneo-web.observability.accessLogs must be spelled traefik.http.routers.torneo-web.observability.accesslogs`,
    );
  });

  it('accepts any message in ${VAR:?message}', () => {
    const tree = mutated({ [C]: [[SERVER_IMAGE, 'server:${IMAGE_TAG:?set it}']] });
    expect(checkCompose(tree)).toEqual([]);
  });

  it.each(CASES)('fails on: %s', (_name, edits, pattern) => {
    const problems = checkCompose(mutated(edits));
    expect(
      problems.some((p) => pattern.test(p)),
      problems.join('\n') || '(no problems)',
    ).toBe(true);
  });

  it('fails on a <Dockerfile>.dockerignore outside the root', () => {
    const extra = loadTree(fixture('dockerignore-elsewhere')).files;
    expect(extra).toEqual(['apps/server/Dockerfile.dockerignore']);
    const problems = checkCompose(mutated({}, extra));
    expect(problems).toEqual([
      'apps/server/Dockerfile.dockerignore: single-dockerignore: only the root .dockerignore is allowed (BuildKit prefers <Dockerfile>.dockerignore)',
    ]);
  });

  it('reports a missing compose file instead of crashing', () => {
    const tree = { read: (rel) => (rel === C ? null : valid.read(rel)), files: valid.files };
    expect(checkCompose(tree)).toContain(`${C}: exists: missing`);
  });
});
