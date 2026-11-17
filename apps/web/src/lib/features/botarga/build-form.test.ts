import { describe, expect, it } from 'vitest';
import { parseBuildForm, parseClientKey, parseDrawForm } from './build-form.js';

function form(entries: [string, string][]): FormData {
  const data = new FormData();
  for (const [k, v] of entries) data.append(k, v);
  return data;
}

const STATS: [string, string][] = [
  ['hp', '40'],
  ['attack', '40'],
  ['defense', '40'],
  ['speed', '40'],
  ['special', '40'],
];

describe('parseBuildForm', () => {
  it('reads a whole build, in the shape of the protocol', () => {
    const body = parseBuildForm(
      form([
        ...STATS,
        ['style_id', 'balanced'],
        ['ability', 'foam_headbutt'],
        ['ability', 'spicy_slap'],
        ['accessory', 'party_hat'],
        ['accessory', ''],
        ['idle', 'idle_2'],
        ['victory', ''],
        ['skin', 'tournament_suit'],
      ]),
    );
    expect(body).toEqual({
      stats: { hp: 40, attack: 40, defense: 40, speed: 40, special: 40 },
      style_id: 'balanced',
      abilities: ['foam_headbutt', 'spicy_slap'],
      accessories: ['party_hat'],
      cosmetics: { idle: 'idle_2', victory: null },
      skin: 'tournament_suit',
    });
    // "Base botarga", and a form without the field: no suit.
    expect(
      parseBuildForm(form([...STATS, ['style_id', 'balanced'], ['skin', '']]))?.skin,
    ).toBeNull();
    expect(parseBuildForm(form([...STATS, ['style_id', 'balanced']]))?.skin).toBeNull();
  });

  it('leaves the rules to the server, and refuses what is not a build', () => {
    // Over the budget is the server's call: it has the shape.
    expect(
      parseBuildForm(form([...STATS.slice(0, 4), ['special', '900'], ['style_id', 'x']])),
    ).not.toBeNull();
    for (const bad of [
      form([['style_id', 'balanced']]),
      form([...STATS.slice(1), ['hp', '-1'], ['style_id', 'balanced']]),
      form([...STATS.slice(1), ['hp', '4.5'], ['style_id', 'balanced']]),
      form([...STATS.slice(1), ['hp', '0040'], ['style_id', 'balanced']]),
      form([...STATS]),
      form([...STATS, ['style_id', '']]),
      form([...STATS, ['style_id', 'x'.repeat(65)]]),
      form([...STATS, ['style_id', 'balanced'], ['idle', 'dance']]),
      form([...STATS, ['style_id', 'balanced'], ['victory', 'idle']]),
      form([
        ...STATS,
        ['style_id', 'balanced'],
        ...Array.from({ length: 5 }, (_, i): [string, string] => ['ability', `a${i}`]),
      ]),
      form([...STATS, ['style_id', 'balanced'], ['ability', '']]),
      form([...STATS, ['style_id', 'balanced'], ['skin', 'Tournament_Suit']]),
      form([...STATS, ['style_id', 'balanced'], ['skin', '../suit']]),
      form([...STATS, ['style_id', 'balanced'], ['skin', 's'.repeat(65)]]),
    ]) {
      expect(parseBuildForm(bad)).toBeNull();
    }
  });
});

describe('parseDrawForm and parseClientKey', () => {
  it('take only what the server takes', () => {
    expect(
      parseDrawForm(
        form([
          ['draw_id', '7'],
          ['item_id', 'spicy_slap'],
        ]),
      ),
    ).toEqual({
      draw_id: 7,
      item_id: 'spicy_slap',
    });
    for (const id of ['0', '-1', '07', '2147483648', 'x', '']) {
      expect(
        parseDrawForm(
          form([
            ['draw_id', id],
            ['item_id', 'a'],
          ]),
        ),
        id,
      ).toBeNull();
    }
    expect(
      parseDrawForm(
        form([
          ['draw_id', '1'],
          ['item_id', ''],
        ]),
      ),
    ).toBeNull();
    const key = '0b0e8e5e-8a43-4c3c-9b55-0e6e3e1b2a10';
    expect(parseClientKey(form([['client_key', key]]))).toBe(key);
    for (const bad of [key.toUpperCase(), key.replaceAll('-', ''), '', 'key']) {
      expect(parseClientKey(form([['client_key', bad]])), bad).toBeNull();
    }
  });
});
