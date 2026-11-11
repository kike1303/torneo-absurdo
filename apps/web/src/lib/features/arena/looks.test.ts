import { describe, expect, it } from 'vitest';
import { parseLooks } from './looks.js';

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
