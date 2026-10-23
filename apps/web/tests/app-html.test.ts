import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const html = readFileSync(fileURLToPath(new URL('../src/app.html', import.meta.url)), 'utf8');

describe('app.html', () => {
  it('has one nonce script of at most 1 KB (RF-A7)', () => {
    const scripts = [...html.matchAll(/<script nonce="%sveltekit\.nonce%">([\s\S]*?)<\/script>/g)];
    expect(scripts).toHaveLength(1);
    expect(new TextEncoder().encode(scripts[0]![1]!).length).toBeLessThanOrEqual(1024);
  });

  it('keeps the theme markers before </head>', () => {
    const head = html.slice(0, html.indexOf('</head>'));
    expect(head).toContain('data-theme="%theme%"');
    expect(head).toContain('%theme.meta%');
    expect(html.slice(html.indexOf('</head>'))).not.toMatch(/%theme/);
  });
});
