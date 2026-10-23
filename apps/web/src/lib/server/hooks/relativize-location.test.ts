import { describe, expect, it } from 'vitest';
import { relativizeLocation } from './relativize-location.js';

function redirect(location: string, status = 307): Response {
  return new Response(null, { status, headers: { location } });
}

describe('relativizeLocation', () => {
  it('turns an absolute Location into pathname + search', () => {
    const res = relativizeLocation(redirect('http://evil.example/es/?a=1#frag'));
    expect(res.headers.get('location')).toBe('/es/?a=1');
  });

  it('strips extra leading slashes and backslashes', () => {
    expect(relativizeLocation(redirect('http://h//evil.example/')).headers.get('location')).toBe(
      '/evil.example/',
    );
    expect(relativizeLocation(redirect('//evil.example/x')).headers.get('location')).toBe(
      '/evil.example/x',
    );
    expect(relativizeLocation(redirect('/\\evil.example')).headers.get('location')).toBe(
      '/evil.example',
    );
  });

  it('leaves non-redirects untouched', () => {
    const ok = new Response('x', { status: 200, headers: { location: 'http://a/b' } });
    expect(relativizeLocation(ok).headers.get('location')).toBe('http://a/b');
  });

  it('copies responses with immutable headers', () => {
    const immutable = Response.redirect('http://evil.example/en/', 307);
    const res = relativizeLocation(immutable);
    expect(res.status).toBe(307);
    expect(res.headers.get('location')).toBe('/en/');
  });
});
