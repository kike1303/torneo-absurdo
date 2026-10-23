import { describe, expect, it } from 'vitest';
import { getLocale } from '#lib/paraglide/runtime.js';
import { fakeEvent } from '../../../../tests/support/event.js';
import { paraglide } from './paraglide.js';

// QA H2: pages rendered on excluded paths (404s under /api, /ws, /healthz/…) must never leak the
// `%paraglide.lang%` marker, and `<html lang>` must match the language of the rendered messages.
const HTML = '<html lang="%paraglide.lang%"><head></head><body></body></html>';

async function render(path: string, cookies: Record<string, string> = {}) {
  let html = '';
  let messageLocale = '';
  const res = await paraglide({
    event: fakeEvent({ url: `http://localhost${path}`, cookies }),
    resolve: async (_event, opts) => {
      messageLocale = getLocale();
      html = (await opts?.transformPageChunk?.({ html: HTML, done: true })) ?? HTML;
      return new Response(html, { status: 404, headers: { 'content-type': 'text/html' } });
    },
  });
  return { res, html, messageLocale };
}

describe('paraglide handle on excluded paths', () => {
  it.each(['/api', '/api/x', '/ws', '/healthz/x', '/healthz//'])(
    '%s: lang from a valid cookie',
    async (path) => {
      const { res, html, messageLocale } = await render(path, { locale: 'es' });
      expect(html).toContain('<html lang="es">');
      expect(html).not.toContain('%paraglide.');
      expect(messageLocale).toBe('es');
      expect(res.headers.get('location')).toBeNull();
      expect(res.status).toBe(404);
    },
  );

  it.each<Record<string, string>>([{}, { locale: 'xx' }, { locale: '"><script>' }])(
    'falls back to en with cookies %j',
    async (cookies) => {
      const { html, messageLocale } = await render('/api/x', cookies);
      expect(html).toContain('<html lang="en">');
      expect(html).not.toContain('%paraglide.');
      expect(messageLocale).toBe('en');
    },
  );

  it('never redirects /healthz, even as a document request', async () => {
    let called = false;
    const res = await paraglide({
      event: fakeEvent({
        url: 'http://evil.example/healthz',
        headers: { 'sec-fetch-dest': 'document' },
      }),
      resolve: async () => {
        called = true;
        return new Response('ok');
      },
    });
    expect(called).toBe(true);
    expect(res.status).toBe(200);
    expect(res.headers.get('location')).toBeNull();
  });
});
