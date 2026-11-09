import { describe, expect, it } from 'vitest';
import { parseSeasonId } from './season-id.js';

describe('parseSeasonId (020)', () => {
  it('takes exactly YYYYMM', () => {
    expect(parseSeasonId('202611')).toBe(202611);
    expect(parseSeasonId('200001')).toBe(200001);
  });

  it.each([
    '',
    '2026',
    '20261',
    '2026111',
    '202600',
    '202613',
    '199912',
    '+20261',
    ' 202611',
    '2026-1',
  ])('%j is not a season', (text) => {
    expect(parseSeasonId(text)).toBe(null);
  });
});
