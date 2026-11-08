import { describe, expect, it } from 'vitest';
import { parseAmount, parseBetForm } from './form.js';

// 012 RF44: only the shape of what is posted.
const KEY = '0f8fad5b-d9cb-469f-a165-70867728950e';

function form(values: Record<string, string>): FormData {
  const data = new FormData();
  for (const [name, value] of Object.entries(values)) data.set(name, value);
  return data;
}

describe('the bet form', () => {
  it('reads a whole bet', () => {
    const bet = parseBetForm(form({ fight_id: '7', side: 'b', amount: ' 25 ', client_key: KEY }));
    expect(bet).toEqual({ fight_id: 7, side: 'b', amount: 25, client_key: KEY });
  });

  it.each([
    ['no side', { fight_id: '7', amount: '25', client_key: KEY }],
    ['a third side', { fight_id: '7', side: 'c', amount: '25', client_key: KEY }],
    [
      'a key in capitals',
      { fight_id: '7', side: 'a', amount: '25', client_key: KEY.toUpperCase() },
    ],
    ['a fight out of range', { fight_id: '2147483648', side: 'a', amount: '25', client_key: KEY }],
    ['a negative fight', { fight_id: '-1', side: 'a', amount: '25', client_key: KEY }],
  ])('refuses %s', (_, values) => {
    expect(parseBetForm(form(values))).toBeNull();
  });

  it('takes only whole, non-negative amounts', () => {
    expect(parseAmount('0')).toBe(0);
    expect(parseAmount('1000')).toBe(1000);
    for (const bad of ['', '1.5', '-3', '1e3', '١٢', '9999999999999999', null]) {
      expect(parseAmount(bad)).toBeNull();
    }
  });
});
