import { describe, expect, it } from 'vitest';
import { fakeEvent } from '../../../../tests/support/event.js';
import { createHeadTransformer, createThemeHandle } from './theme.js';

const DOC_HEAD = '<html data-theme="%theme%"><head>%theme.meta%<title>x</title></head>';
const DOC_BODY = '<body><p>user text %theme.meta% and %theme%</p></body></html>';

describe('createHeadTransformer', () => {
  it('replaces the markers only before </head>, once each', () => {
    const transform = createHeadTransformer('light');
    const out = transform(DOC_HEAD + DOC_BODY);
    expect(out).toContain('data-theme="light"');
    expect(out.match(/<meta/g)).toHaveLength(2);
    expect(out).toContain('user text %theme.meta% and %theme%');
  });

  it('leaves every chunk after </head> untouched', () => {
    const transform = createHeadTransformer('dark');
    const head = transform(DOC_HEAD);
    const body = transform(DOC_BODY);
    expect(head).toContain('data-theme="dark"');
    expect(body).toBe(DOC_BODY);
  });

  it('handles a head split across chunks', () => {
    const transform = createHeadTransformer('system');
    const first = transform('<html data-theme="%theme%"><head>');
    const second = transform('%theme.meta%</head><body>%theme.meta%</body>');
    expect(first).toContain('data-theme="system"');
    expect(second.match(/<meta/g)).toHaveLength(3);
    expect(second.endsWith('<body>%theme.meta%</body>')).toBe(true);
  });
});

describe('createThemeHandle', () => {
  it('stores the validated choice and transforms the page', async () => {
    const event = fakeEvent({
      url: 'http://h/es/',
      cookies: { theme: '"><script>' },
      routeId: '/',
    });
    let transformed = '';
    await createThemeHandle(undefined)({
      event,
      resolve: async (_e, opts) => {
        transformed = (await opts?.transformPageChunk?.({ html: DOC_HEAD, done: true })) ?? '';
        return new Response(transformed);
      },
    });
    expect(event.locals.theme).toBe('dark');
    expect(transformed).toContain('data-theme="dark"');
    expect(transformed).not.toContain('<script>');
  });
});
