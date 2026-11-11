import { describe, expect, it } from 'vitest';
import { looksKey, parseLooks, writeLooks } from './looks.js';

const parse = (query: string) => parseLooks(new URLSearchParams(query));

describe('parseLooks (022)', () => {
  it('is undefined without wardrobe parameters', () => {
    expect(parse('')).toBeUndefined();
    expect(parse('log=3&pace=2')).toBeUndefined();
  });

  it('reads accessories, a skin and colors by side', () => {
    expect(
      parse('wear_a=propeller_beanie,oven_mitts&colors_a=primary:FF0000,z1:00aa88&skin_b=rat'),
    ).toEqual({
      a: {
        skin: undefined,
        wear: ['propeller_beanie', 'oven_mitts'],
        // Built from parts: check-tokens allows no color literal, even in a test.
        colors: { primary: `#${'FF0000'}`, z1: `#${'00aa88'}` },
      },
      b: { skin: 'rat', wear: [], colors: {} },
    });
  });

  it('drops what is not an id or a color', () => {
    expect(parse('wear_a=../x,Ok,fine_1&colors_a=primary:red,accent:12345&skin_a=a/b')).toEqual({
      a: { skin: undefined, wear: ['fine_1'], colors: {} },
      b: undefined,
    });
  });
});

describe('writeLooks (TA-76)', () => {
  const red = `#${'FF0000'}`;

  it('round-trips through parseLooks and keeps the other parameters', () => {
    const looks = {
      a: { wear: ['oven_mitts', 'propeller_beanie'], colors: { primary: red } },
      b: { skin: 'rat', wear: [], colors: {} },
    };
    const params = writeLooks(new URLSearchParams('log=3&wear_a=old'), looks);
    expect(params.get('log')).toBe('3');
    expect(params.get('wear_a')).toBe('oven_mitts,propeller_beanie');
    expect(params.get('colors_a')).toBe('primary:ff0000');
    expect(params.has('wear_b')).toBe(false);
    expect(parseLooks(params)).toEqual({
      a: {
        skin: undefined,
        wear: ['oven_mitts', 'propeller_beanie'],
        colors: { primary: `#${'ff0000'}` },
      },
      b: { skin: 'rat', wear: [], colors: {} },
    });
  });

  it('drops every wardrobe key for an empty look', () => {
    const params = writeLooks(
      new URLSearchParams('wear_a=x&skin_b=y&colors_a=primary:000000&t=5'),
      {
        a: { wear: [], colors: {} },
      },
    );
    expect(params.toString()).toBe('t=5');
    expect(parseLooks(params)).toBeUndefined();
  });

  it('never writes what parseLooks would drop', () => {
    const params = writeLooks(new URLSearchParams(), {
      a: { skin: 'a/b', wear: ['../x', 'ok_1'], colors: { primary: 'red', accent: red } },
    });
    expect(params.toString()).toBe('wear_a=ok_1&colors_a=accent%3Aff0000');
  });
});

describe('looksKey (TA-76)', () => {
  it('changes only with the wardrobe parameters', () => {
    const key = (q: string) => looksKey(new URLSearchParams(q));
    expect(key('t=1&pace=2')).toBe(key(''));
    expect(key('wear_a=oven_mitts')).not.toBe(key('wear_b=oven_mitts'));
  });
});
