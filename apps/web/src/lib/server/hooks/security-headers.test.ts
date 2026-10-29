import { describe, expect, it } from 'vitest';
import { fakeEvent } from '../../../../tests/support/event.js';
import { SECURITY_HEADERS, securityHeaders, withSecurityHeaders } from './security-headers.js';

function expectHeaders(response: Response): void {
  for (const [name, value] of Object.entries(SECURITY_HEADERS)) {
    expect(response.headers.get(name)).toBe(value);
  }
}

describe('securityHeaders', () => {
  it.each([200, 307, 403, 404])('adds the headers to a %s', async (status) => {
    const res = await securityHeaders({
      event: fakeEvent({ url: 'http://h/' }),
      resolve: async () =>
        new Response(null, { status, headers: status === 307 ? { location: '/en/' } : {} }),
    });
    expect(res.status).toBe(status);
    expectHeaders(res);
  });

  it('copies a response with immutable headers instead of failing', async () => {
    const immutable = Response.redirect('http://h/en/', 307);
    expect(() => immutable.headers.set('x', 'y')).toThrow(TypeError);
    const res = withSecurityHeaders(immutable);
    expect(res.status).toBe(307);
    expect(res.headers.get('location')).toBe('http://h/en/');
    expectHeaders(res);
  });

  it('has no includeSubDomains or preload', () => {
    expect(SECURITY_HEADERS['strict-transport-security']).toBe('max-age=31536000');
  });

  it('has the ADR-0012 B2 values for COOP and Permissions-Policy', () => {
    expect(SECURITY_HEADERS['cross-origin-opener-policy']).toBe('same-origin');
    expect(SECURITY_HEADERS['permissions-policy']).toBe(
      'camera=(), microphone=(), geolocation=(), payment=(), usb=(), display-capture=()',
    );
  });
});
