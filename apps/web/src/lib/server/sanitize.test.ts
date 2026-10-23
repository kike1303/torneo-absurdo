import { describe, expect, it } from 'vitest';
import { sanitize } from './sanitize.js';

// Every fragment of an address that must not survive sanitizing.
function expectNoFragments(out: string, address: string): void {
  const parts = address.split(/[.:%[\]]+/).filter((p) => p.length >= 2 && p !== 'ffff');
  for (let i = 0; i < parts.length - 1; i++) {
    expect(out, `${address} → ${out}`).not.toContain(`${parts[i]}.${parts[i + 1]}`);
    expect(out, `${address} → ${out}`).not.toContain(`${parts[i]}:${parts[i + 1]}`);
  }
}

describe('sanitize', () => {
  it.each([
    [
      'connect ECONNREFUSED ::ffff:203.0.113.77:5432',
      '203.0.113.77',
      'connect ECONNREFUSED [ip]:5432',
    ],
    ['from ::FFFF:198.51.100.23 now', '198.51.100.23', 'from [ip] now'],
    ['peer [::ffff:192.0.2.5]:443', '192.0.2.5', 'peer [[ip]]:443'],
    ['nat64 64:ff9b::203.0.113.9 x', '203.0.113.9', 'nat64 [ip] x'],
    ['compat ::203.0.113.10 y', '203.0.113.10', 'compat [ip] y'],
    ['full 0:0:0:0:0:ffff:203.0.113.11 z', '203.0.113.11', 'full [ip] z'],
  ])('removes IPv4 embedded in IPv6: %s', (input, ipv4, expected) => {
    const out = sanitize(input);
    expect(out).toBe(expected);
    expect(out).not.toContain(ipv4.split('.').slice(1).join('.'));
    expectNoFragments(out, ipv4);
  });

  it('removes IPv4 addresses', () => {
    expect(sanitize('from 203.0.113.77 via 10.0.0.1:8080')).toBe('from [ip] via [ip]:8080');
  });

  it('removes IPv6 addresses: full, compressed, with zone and IPv4-mapped', () => {
    const out = sanitize(
      'a 2001:0db8:85a3:0000:0000:8a2e:0370:7334 b 2001:db8::1 c fe80::1%eth0 d [::1]:3000 e ::ffff:203.0.113.77',
    );
    expect(out).not.toMatch(/2001|db8|fe80|eth0|203\.0\.113/i);
    // No fragment of the IPv4-mapped address may survive (QA H1).
    expect(out).not.toMatch(/0\.113\.77|113\.77|ffff/i);
    expect(out).toContain('e [ip]');
    expect(out).toContain('[ip]');
    expect(out).toContain('d [[ip]]:3000');
  });

  it('keeps line:column in stack frames', () => {
    expect(sanitize('at load (src/routes/x/+page.server.ts:12:34)')).toBe(
      'at load (src/routes/x/+page.server.ts:12:34)',
    );
  });

  it('removes the query string of absolute and relative URLs', () => {
    expect(sanitize('GET https://torneo.example/es/x?token=abc&q=1 failed')).toBe(
      'GET https://torneo.example/es/x failed',
    );
    expect(sanitize('HTTP://A.EXAMPLE/P?SECRET=1')).toBe('HTTP://A.EXAMPLE/P');
    expect(sanitize("fetch '/es/dev/error-probe?q=canary-123'")).toBe(
      "fetch '/es/dev/error-probe'",
    );
    expect(sanitize('see /api/x?y=2#frag')).toBe('see /api/x');
    expect(sanitize('boom ?q=secret end')).toBe('boom  end');
    expect(sanitize('Is it ok? yes')).toBe('Is it ok? yes');
  });

  it('removes credentials from URLs', () => {
    expect(sanitize('connect postgres://torneo:s3cr3t-pw@db.internal:5432/app failed')).toBe(
      'connect postgres://[redacted]@db.internal:5432/app failed',
    );
    expect(sanitize('GET https://user@api.example/x')).toBe('GET https://[redacted]@api.example/x');
    expect(sanitize('mail someone@example.com')).toBe('mail someone@example.com');
  });

  it('removes the value after sensitive header names, any case', () => {
    const cases = [
      'cookie: session=abc; theme=dark',
      'Cookie=session=abc',
      'COOKIE: a=b',
      'set-cookie: a=b',
      'Authorization: Bearer abc.def.ghi',
      'authorization=Basic Zm9vOmJhcg==',
      'x-forwarded-for=203.0.113.77, 10.0.0.1',
      'X-Forwarded-For: 198.51.100.1',
      'x-internal-secret: s3cr3t',
      'X-Client-Ip: 192.0.2.5',
      '{"cookie":"a=b","x":1}',
    ];
    for (const input of cases) {
      const out = sanitize(input);
      expect(out, input).toMatch(/\[redacted\]/);
      expect(out, input).not.toMatch(/abc|a=b|Zm9v|s3cr3t|203\.0|198\.51|192\.0/);
    }
  });
});

describe('sanitize: sensitive header values in arrays and maps (TA-10)', () => {
  it.each([
    ['{"set-cookie":["sid=abc123"]}', '{"set-cookie":[redacted]}'],
    ['{"set-cookie":["sid=abc123","theme=dark"],"x":1}', '{"set-cookie":[redacted],"x":1}'],
    ["{ cookie: [ 'sid=abc123' ] }", '{ cookie: [redacted] }'],
    ["Map(1) { 'cookie' => 'session=abc123' }", "Map(1) { 'cookie' => '[redacted]' }"],
    ["Map(1) { 'set-cookie' => [ 'sid=abc123' ] }", "Map(1) { 'set-cookie' => [redacted] }"],
    ["Headers { authorization: 'Bearer abc123' }", "Headers { authorization: '[redacted]' }"],
  ])('%s', (input, expected) => {
    const out = sanitize(input);
    expect(out).toBe(expected);
    expect(out).not.toMatch(/abc123|dark/);
  });

  it('redacts a multi-line array (util.inspect breaks long arrays)', () => {
    const out = sanitize("{\n  'set-cookie': [\n    'sid=abc123',\n    'theme=dark'\n  ]\n}");
    expect(out).not.toMatch(/abc123|dark/);
  });
});

describe('sanitize: credentials, fragments and glued IPv4 (TA-10)', () => {
  it.each([
    [
      'connect postgres://user:p/ss@db.internal/app',
      'connect postgres://[redacted]@db.internal/app',
    ],
    [
      'connect postgres://user:pa@ss@db.internal/app',
      'connect postgres://[redacted]@db.internal/app',
    ],
    ['GET https://user:p@ss/w0rd@api.example/x', 'GET https://[redacted]@api.example/x'],
  ])('removes credentials with `/` or `@`: %s', (input, expected) => {
    expect(sanitize(input)).toBe(expected);
  });

  it('keeps URLs without credentials, stack paths and emails intact', () => {
    expect(sanitize('GET https://api.example/users/a@b.example failed')).toBe(
      'GET https://api.example/users/a@b.example failed',
    );
    const frame =
      'at x (file:///app/node_modules/.pnpm/@sveltejs+kit@3.1.0/node_modules/@sveltejs/kit/src/x.js:1:2)';
    expect(sanitize(frame)).toBe(frame);
  });

  it('removes fragments without a query', () => {
    expect(sanitize('redirect to /cb#access_token=abc&state=1 done')).toBe('redirect to /cb done');
    expect(sanitize('open https://a.example/p#secret-section')).toBe('open https://a.example/p');
    expect(sanitize("go '/x#token=abc'")).toBe("go '/x'");
  });

  it('keeps `#` that is not a URL fragment', () => {
    expect(sanitize('at Foo.#bar (src/x.ts:1:2)')).toBe('at Foo.#bar (src/x.ts:1:2)');
    expect(sanitize('see issue #12 and C# code')).toBe('see issue #12 and C# code');
  });

  it.each([
    ['client_203.0.113.5 failed', 'client_[ip] failed'],
    ['peer=ip203.0.113.6', 'peer=ip[ip]'],
    ['x203.0.113.7y', 'x[ip]y'],
  ])('removes IPv4 glued to a letter or `_`: %s', (input, expected) => {
    expect(sanitize(input)).toBe(expected);
  });
});

describe('sanitize: bounded input (TA-10)', () => {
  const adversarial = [
    ['IPv6 run without ::', 'a:'.repeat(80_000)],
    ['hex groups', '1:'.repeat(80_000)],
    ['dotted words', 'a.'.repeat(80_000)],
    ['scheme prefixes', 'a://a:'.repeat(27_000)],
    ['header names', 'cookie:'.repeat(23_000)],
    ['IPv4-ish digits', '1.'.repeat(80_000)],
  ] as const;

  it.each(adversarial)('sanitizes 160 KB of %s in under 50 ms', (_name, input) => {
    expect(input.length).toBeGreaterThanOrEqual(160_000);
    sanitize('warm up 2001:db8::1 203.0.113.5');
    const start = performance.now();
    sanitize(input);
    expect(performance.now() - start).toBeLessThan(50);
  });

  it('truncates to 8 KB and marks it', () => {
    const out = sanitize('x '.repeat(10_000));
    expect(out.length).toBeLessThanOrEqual(8 * 1024 + 20);
    expect(out.endsWith('[truncated]')).toBe(true);
  });

  it('never leaves a partial secret at the cut', () => {
    const pad = 'x'.repeat(8 * 1024 - 12) + ' ';
    for (const tail of [
      '203.0.113.77 end',
      'postgres://user:s3cr3t@db end',
      '/cb?token=abcdef end',
      '2001:db8:85a3::8a2e:370:7334 end',
    ]) {
      const out = sanitize(pad + tail);
      expect(out, tail).not.toMatch(/203|113|s3cr|user|abcd|2001|db8|85a/);
    }
  });
});
