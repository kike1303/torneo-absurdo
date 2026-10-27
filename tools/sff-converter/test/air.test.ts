import { describe, expect, it } from 'vitest';
import { parseAir } from '../src/air.ts';
import { code } from './support/expect-error.ts';

describe('parseAir', () => {
  it('reads actions case-insensitively, with comments, Clsn and Interpolate ignored', () => {
    const air = parseAir(
      [
        '; header comment',
        '[begin ACTION 0] ; idle',
        'clsn2Default: 1',
        '  Clsn2[0] = -10,0,10,-60',
        '0,0, 0,0, 5',
        'LoopStart',
        ' 0, 1 , 2, -3, 6, H ; flip',
        '0,2,0,0,7,V,A',
        '0,3,0,0,8,HV,,1.5,1.5,30',
        'Interpolate Offset',
        '',
        '[Begin Action 5150]',
        '-1,0,0,0,-1',
      ].join('\r\n'),
    );
    const idle = air.actions.get(0);
    expect(idle?.loopStart).toBe(1);
    expect(idle?.frames).toEqual([
      { group: 0, image: 0, x: 0, y: 0, ticks: 5, flipH: false, flipV: false },
      { group: 0, image: 1, x: 2, y: -3, ticks: 6, flipH: true, flipV: false },
      { group: 0, image: 2, x: 0, y: 0, ticks: 7, flipH: false, flipV: true },
      { group: 0, image: 3, x: 0, y: 0, ticks: 8, flipH: true, flipV: true },
    ]);
    expect(air.actions.get(5150)?.frames[0]).toMatchObject({ group: -1, ticks: -1 });
    const notes = Object.fromEntries(air.notes.map((n) => [n.code, n.data?.['count']]));
    expect(notes).toEqual({
      'air.clsn_ignored': 2,
      'air.blend_ignored': 1,
      'air.transform_ignored': 1,
      'air.interpolate_ignored': 1,
    });
  });

  it('a duplicate action keeps the first one, with a note', () => {
    const air = parseAir('[Begin Action 1]\n0,0,0,0,1\n[Begin Action 1]\n0,1,0,0,1\n');
    expect(air.actions.get(1)?.frames[0]?.image).toBe(0);
    expect(air.notes).toContainEqual({ code: 'air.duplicate_action', data: { action: 1 } });
  });

  it('bad frame lines are counted on their action, other sections are skipped', () => {
    const air = parseAir(
      '[Remap]\n1,1 = 2,2\n[Begin Action 2]\n0,0,x,0,1\n0,0,0,0,-2\n0,0,0,0,1,Q\n0,0,0,0,1\n',
    );
    expect(air.actions.get(2)).toMatchObject({ badLines: 3 });
    expect(air.actions.get(2)?.frames).toHaveLength(1);
    expect(air.actions.size).toBe(1);
  });

  it('a line longer than 4096 characters or too many frames fail', () => {
    expect(code(() => parseAir(`[Begin Action 0]\n${'0'.repeat(4097)}\n`))).toBe(
      'air.line_too_long',
    );
    const many = ['[Begin Action 0]', ...new Array<string>(1001).fill('0,0,0,0,1')].join('\n');
    expect(code(() => parseAir(many))).toBe('air.too_many_frames');
  });

  it('too many actions fail', () => {
    const text = Array.from({ length: 10_001 }, (_, i) => `[Begin Action ${i}]`).join('\n');
    expect(code(() => parseAir(text))).toBe('air.too_many_actions');
  });

  it('reads bytes as latin1', () => {
    const air = parseAir(
      Uint8Array.from(Buffer.from('[Begin Action 3]\n;\xe9\n0,0,0,0,2\n', 'latin1')),
    );
    expect(air.actions.get(3)?.frames).toHaveLength(1);
  });
});
