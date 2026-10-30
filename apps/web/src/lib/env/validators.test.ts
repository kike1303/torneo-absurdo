import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import {
  DEV_INTERNAL_API_SECRET,
  checkInternalApiEnv,
  isLoopbackHttpOrigin,
  validateEnableDevCatalog,
  validateInternalApiSecret,
  validateInternalApiUrl,
  validatePublicOrigin,
} from './validators.js';

/** The message of the error `fn` throws (fails the test if it does not throw). */
function thrown(fn: () => unknown): string {
  try {
    fn();
  } catch (e) {
    return (e as Error).message;
  }
  throw new Error('expected a throw');
}

describe('validatePublicOrigin', () => {
  it.each([
    'http://localhost:5173',
    'https://torneo.example',
    'http://127.0.0.1:4173',
    'https://torneo.example:8443',
  ])('accepts %s', (value) => expect(validatePublicOrigin(value, false)).toBe(value));

  const invalid = [
    'https://torneo.example/es',
    'https://torneo.example/',
    'https://torneo.example?x=1',
    'https://torneo.example#x',
    'https://user:pass@torneo.example',
    'ftp://torneo.example',
    'https://Torneo.example',
    'https://torneo.example:443',
    'http://torneo.example:80',
    'torneo.example',
    'not a url',
  ];
  it.each(invalid)('rejects %s without echoing it', (value) => {
    let message = '';
    expect(() => {
      try {
        validatePublicOrigin(value, false);
      } catch (e) {
        message = (e as Error).message;
        throw e;
      }
    }).toThrow();
    expect(message).toMatch(/^PUBLIC_ORIGIN /);
    expect(message).not.toContain(value);
  });

  it('is required at runtime and optional while building', () => {
    expect(() => validatePublicOrigin(undefined, false)).toThrow(/required/);
    expect(() => validatePublicOrigin('', false)).toThrow(/required/);
    expect(validatePublicOrigin(undefined, true)).toBeUndefined();
  });
});

describe('validateEnableDevCatalog', () => {
  it.each([
    ['1', '1'],
    ['0', undefined],
    ['true', undefined],
    ['', undefined],
    [undefined, undefined],
  ] as const)('%j → %j', (value, expected) =>
    expect(validateEnableDevCatalog(value)).toBe(expected),
  );
});

// RF-B18 and the security note of spec 008: the same table decides "on loopback" in the server
// (`is_loopback_http_origin`) and here.
const root = new URL('../../../../../', import.meta.url);

describe('isLoopbackHttpOrigin', () => {
  const table = JSON.parse(
    readFileSync(new URL('scripts/__fixtures__/loopback-origins.json', root), 'utf8'),
  ) as [string, boolean][];
  it('has the shared cases', () => expect(table.length).toBeGreaterThanOrEqual(9));
  it.each(table)('%s → %s', (origin, expected) => {
    expect(isLoopbackHttpOrigin(origin)).toBe(expected);
  });
});

describe('validateInternalApiUrl (008 decision 1)', () => {
  it.each([
    ['http://torneo-server:8080', 'http://torneo-server:8080'],
    ['http://127.0.0.1:8080', 'http://127.0.0.1:8080'],
    ['https://api.torneo.example', 'https://api.torneo.example'],
    // Serialized like `Request.url`, which is what handleFetch compares (architect's note).
    ['HTTP://Torneo-Server:8080', 'http://torneo-server:8080'],
    ['http://torneo-server:80', 'http://torneo-server'],
    ['https://api.torneo.example:443', 'https://api.torneo.example'],
  ])('%s → %s', (value, expected) => expect(validateInternalApiUrl(value, false)).toBe(expected));

  const invalid = [
    'http://torneo-server:8080/api',
    'http://torneo-server:8080/',
    'http://torneo-server:8080?x=1',
    'http://torneo-server:8080#x',
    'http://user:pass@torneo-server:8080',
    'ftp://torneo-server:8080',
    'torneo-server:8080',
    'not a url',
  ];
  it.each(invalid)('rejects %s without echoing it', (value) => {
    const message = thrown(() => validateInternalApiUrl(value, false));
    expect(message).toMatch(/^INTERNAL_API_URL /);
    expect(message).not.toContain(value);
    expect(message.toLowerCase()).not.toContain('torneo-server');
  });

  it('is required at runtime and optional while building', () => {
    expect(thrown(() => validateInternalApiUrl(undefined, false))).toMatch(/required/);
    expect(thrown(() => validateInternalApiUrl('', false))).toMatch(/required/);
    expect(validateInternalApiUrl(undefined, true)).toBeUndefined();
  });
});

// 32 random bytes, base64url without padding: not the development value.
const OTHER = 'kq3Z0m9sV7cR2xW1pL8nB4tY6uE5aJ0dF3hG7iK9oQs';

describe('validateInternalApiSecret (008 decision 1, RF-B18)', () => {
  it.each([OTHER, `${OTHER}=`, DEV_INTERNAL_API_SECRET, 'A'.repeat(64)])('accepts %s', (v) =>
    expect(validateInternalApiSecret(v, false)).toBe(v),
  );

  const invalid = [
    // 31 bytes.
    'A'.repeat(42),
    // Not base64url: standard alphabet, spaces, a stray character.
    `${OTHER.slice(0, -2)}+/`,
    ` ${OTHER}`,
    `${OTHER}!`,
    // Non-canonical trailing bits (the server's decoder refuses them too).
    `${OTHER.slice(0, -1)}t`,
    // Padding where none fits.
    `${OTHER}==`,
    `${OTHER.slice(0, -1)}=`,
  ];
  it.each(invalid)('rejects %j without echoing it', (value) => {
    const message = thrown(() => validateInternalApiSecret(value, false));
    expect(message).toMatch(/^INTERNAL_API_SECRET /);
    expect(message).not.toContain(value.trim());
  });

  it('is required at runtime and optional while building', () => {
    expect(thrown(() => validateInternalApiSecret(undefined, false))).toMatch(/required/);
    expect(thrown(() => validateInternalApiSecret('', false))).toMatch(/required/);
    expect(validateInternalApiSecret(undefined, true)).toBeUndefined();
  });
});

describe('checkInternalApiEnv: the cross-checks, run by `init` (008 decision 1)', () => {
  // Kit 3 hands each `schema` only its own value (`EnvVarConfig.schema(value)`), and in `vite
  // dev` the values come from `.env` through `loadEnv`, not from `process.env`: a schema cannot
  // read the other variables. The checks that compare two variables run in `init` instead.
  const base = {
    publicOrigin: 'https://torneo.example',
    internalApiUrl: 'http://torneo-server:8080',
    internalApiSecret: OTHER,
  };
  it('passes with a production configuration', () =>
    expect(() => checkInternalApiEnv(base)).not.toThrow());

  it('skips everything while building (values undefined)', () =>
    expect(() =>
      checkInternalApiEnv({
        publicOrigin: undefined,
        internalApiUrl: undefined,
        internalApiSecret: undefined,
      }),
    ).not.toThrow());

  it.each([DEV_INTERNAL_API_SECRET, `${DEV_INTERNAL_API_SECRET}=`])(
    'refuses the development secret %s outside loopback, naming only the variable',
    (secret) => {
      const message = thrown(() => checkInternalApiEnv({ ...base, internalApiSecret: secret }));
      expect(message).toMatch(/env_invalid/);
      expect(message).toMatch(/INTERNAL_API_SECRET/);
      expect(message).not.toContain(DEV_INTERNAL_API_SECRET);
    },
  );

  it.each([
    ['http://localhost:5173', 'http://127.0.0.1:8080'],
    ['http://127.0.0.1:4173', 'http://localhost:8080'],
    ['http://[::1]:5173', 'http://127.0.0.1:8080'],
  ])('accepts the development secret on %s', (publicOrigin, internalApiUrl) =>
    expect(() =>
      checkInternalApiEnv({
        publicOrigin,
        internalApiUrl,
        internalApiSecret: `${DEV_INTERNAL_API_SECRET}=`,
      }),
    ).not.toThrow(),
  );

  // Kit's server `fetch` forwards every browser cookie to a URL whose host is the site's host
  // or a subdomain of it, whatever the port (`runtime/server/fetch.js`).
  it.each([
    ['https://torneo.example', 'https://torneo.example:8443'],
    ['https://torneo.example', 'http://api.torneo.example:8080'],
    ['http://localhost:5173', 'http://localhost:8080'],
  ])('refuses %s with INTERNAL_API_URL %s (same host or a subdomain)', (publicOrigin, url) => {
    const message = thrown(() =>
      checkInternalApiEnv({ ...base, publicOrigin, internalApiUrl: url }),
    );
    expect(message).toMatch(/env_invalid: INTERNAL_API_URL/);
    expect(message).not.toContain(new URL(url).hostname);
  });

  it.each([
    ['https://torneo.example', 'http://torneo-server:8080'],
    ['https://torneo.example', 'http://torneo.example.internal:8080'],
    ['http://localhost:5173', 'http://127.0.0.1:8080'],
    ['http://localhost:4173', 'http://127.0.0.1:4180'],
  ])('accepts %s with INTERNAL_API_URL %s', (publicOrigin, url) =>
    expect(() =>
      checkInternalApiEnv({ ...base, publicOrigin, internalApiUrl: url, internalApiSecret: OTHER }),
    ).not.toThrow(),
  );

  it('refuses INTERNAL_API_URL equal to PUBLIC_ORIGIN, without the value', () => {
    const message = thrown(() =>
      checkInternalApiEnv({ ...base, internalApiUrl: 'https://torneo.example' }),
    );
    expect(message).toMatch(/env_invalid/);
    expect(message).toMatch(/INTERNAL_API_URL/);
    expect(message).not.toContain('torneo.example');
  });
});

describe('.env.example', () => {
  const secret = (file: string) =>
    readFileSync(new URL(file, root), 'utf8')
      .split('\n')
      .find((line) => line.startsWith('INTERNAL_API_SECRET='));
  it('web and server share the development INTERNAL_API_SECRET', () => {
    const web = secret('apps/web/.env.example');
    expect(web).toMatch(/^INTERNAL_API_SECRET=[A-Za-z0-9_-]{43}$/);
    expect(web).toBe(secret('apps/server/.env.example'));
    expect(web).toBe(`INTERNAL_API_SECRET=${DEV_INTERNAL_API_SECRET}`);
  });
  it('points INTERNAL_API_URL at the server of `just dev`', () => {
    const lines = readFileSync(new URL('apps/web/.env.example', root), 'utf8').split('\n');
    expect(lines).toContain('INTERNAL_API_URL=http://127.0.0.1:8080');
    expect(lines.join('\n')).not.toMatch(/until phase D/);
  });
});
