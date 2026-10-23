import { describe, expect, it } from 'vitest';
import { sanitize } from './sanitize.js';

describe('sanitize', () => {
  it('removes IPv4 addresses', () => {
    expect(sanitize('from 203.0.113.77 via 10.0.0.1:8080')).toBe('from [ip] via [ip]:8080');
  });

  it('removes IPv6 addresses: full, compressed, with zone and IPv4-mapped', () => {
    const out = sanitize(
      'a 2001:0db8:85a3:0000:0000:8a2e:0370:7334 b 2001:db8::1 c fe80::1%eth0 d [::1]:3000 e ::ffff:203.0.113.77',
    );
    expect(out).not.toMatch(/2001|db8|fe80|eth0|203\.0\.113/i);
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
