import { describe, expect, it } from 'vitest';
import { DEFAULT_ACTIONS, parseMap } from '../src/map.ts';
import { converterError } from './support/expect-error.ts';
import { MAP_JSON } from './support/fixtures.ts';

const base = (): Record<string, unknown> => JSON.parse(JSON.stringify(MAP_JSON));

describe('parseMap (RF-B2)', () => {
  it('fills the standard MUGEN actions and the defaults', () => {
    const rest = base();
    delete rest['scale'];
    const map = parseMap(rest);
    expect(map.actions).toMatchObject(DEFAULT_ACTIONS);
    expect(map.actions.attack_light).toBe(200);
    expect(map.scale).toBe(1);
    expect(map.paletteSprite).toEqual([0, 0]);
    expect(parseMap({ ...base(), actions: { ...MAP_JSON.actions, idle: 7 } }).actions.idle).toBe(7);
  });

  it('every character-specific action must be mapped', () => {
    for (const animation of ['attack_light', 'attack_heavy', 'special', 'block', 'dodge', 'ko']) {
      const json = base();
      const actions = { ...(json['actions'] as Record<string, number>) };
      delete actions[animation];
      expect(converterError(() => parseMap({ ...json, actions }))).toMatchObject({
        code: 'map.missing_action',
        data: { animation },
      });
    }
  });

  it('every attack needs its impact frame', () => {
    const json = base();
    expect(
      converterError(() => parseMap({ ...json, impact_frames: { attack_light: 1, special: 1 } })),
    ).toMatchObject({
      code: 'map.missing_impact_frame',
      data: { animation: 'attack_heavy' },
    });
  });

  it.each([
    [{ extra: 1 }, 'map.unknown_field'],
    [{ actions: { ...MAP_JSON.actions, taunt: 3 } }, 'map.unknown_field'],
    [{ impact_frames: { ...MAP_JSON.impact_frames, idle: 0 } }, 'map.unknown_field'],
    [{ id: 'Local-1' }, 'map.invalid_id'],
    [{ id: 'a'.repeat(49) }, 'map.invalid_id'],
    [{ scale: 5 }, 'map.scale_out_of_range'],
    [{ scale: 0.1 }, 'map.scale_out_of_range'],
    [{ actions: { ...MAP_JSON.actions, block: 1.5 } }, 'map.invalid_action'],
    [{ palette_sprite: [0] }, 'map.invalid_field'],
  ])('%j is %s', (patch, expected) => {
    expect(converterError(() => parseMap({ ...base(), ...patch })).code).toBe(expected);
  });

  it('a non-object is map.not_object', () => {
    expect(converterError(() => parseMap([1])).code).toBe('map.not_object');
  });
});
