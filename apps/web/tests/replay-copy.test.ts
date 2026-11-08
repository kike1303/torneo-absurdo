import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

// 011 RF16: the texts of the check promise only what the browser proved.
const messages = (locale: string) =>
  JSON.parse(
    readFileSync(new URL(`../messages/${locale}.json`, import.meta.url), 'utf8'),
  ) as Record<string, string>;

const FORBIDDEN: Record<string, readonly string[]> = {
  es: [
    'imposible',
    'garantiza',
    'a prueba de trampas',
    'comprometió',
    'antes de empezar',
    'infalible',
  ],
  en: ['impossible', 'guarantee', 'cheat-proof', 'tamper-proof', 'committed', 'before it started'],
};

describe.each(['es', 'en'])('the texts of the check, %s', (locale) => {
  const all = messages(locale);
  const verify = Object.entries(all).filter(([key]) => key.startsWith('replay_verify_'));

  it('has them', () => {
    expect(verify.length).toBeGreaterThan(20);
  });

  it.each(FORBIDDEN[locale] ?? [])('never says "%s"', (word) => {
    for (const [key, text] of verify) {
      expect(text.toLowerCase(), key).not.toContain(word);
    }
  });

  it('the limit says what this page cannot prove', () => {
    const limit = all.replay_verify_limit ?? '';
    expect(limit).toMatch(locale === 'es' ? /no puede probarlo/ : /cannot prove/);
    expect(limit).toMatch(locale === 'es' ? /al azar/ : /at random/);
  });

  it('a fight that matches never claims the seal came first', () => {
    for (const key of ['replay_verify_match_fight', 'replay_verify_match_commit']) {
      expect(all[key]?.toLowerCase(), key).not.toMatch(/antes|before/);
    }
  });
});

describe('the limit next to a match', () => {
  it('ReplayVerdict draws it outside every <details>', () => {
    const source = readFileSync(
      new URL('../src/lib/components/replay/ReplayVerdict.svelte', import.meta.url),
      'utf8',
    );
    const limit = source.indexOf('m.replay_verify_limit()');
    expect(limit).toBeGreaterThan(-1);
    expect(limit).toBeLessThan(source.indexOf('<details class'));
    expect(source.slice(0, limit)).not.toContain('</details>');
  });
});
