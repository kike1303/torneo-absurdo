import type { BotargaView } from '@torneo/protocol';
import { describe, expect, it } from 'vitest';
import { buildChoices, drawOptions } from './view.js';

const botarga = {
  build: {
    stats: { hp: 40, attack: 40, defense: 40, speed: 40, special: 40 },
    style_id: 'balanced',
    abilities: ['foam_headbutt'],
    accessories: [],
    cosmetics: { idle: null, victory: null },
  },
  progress: {
    level: 5,
    xp: 0,
    xp_to_next: 200,
    budget_free: 5,
    slots_open: 2,
    daily: { attacks: 0, factor_pct: 100 },
  },
  unlocks: {
    abilities: ['foam_headbutt', 'not_in_catalog'],
    accessories: [],
    cosmetics: ['idle_3', 'win_2', 'nonsense'],
  },
  draws: [],
  queued_draws: 0,
  body_type: 'normal',
  face_id: 'rookie',
  rating: 1000,
  rated_fights: 0,
} satisfies BotargaView;

describe('buildChoices', () => {
  it('names what the user owns in the locale, and an unknown id as itself', () => {
    const es = buildChoices(botarga, 'es');
    expect(es.abilities).toEqual([
      { id: 'foam_headbutt', name: 'Cabezazo Acolchado', kind: 'heavy' },
      { id: 'not_in_catalog', name: 'not_in_catalog', kind: null },
    ]);
    expect(es.styles.map((s) => s.id)).toEqual(['balanced', 'aggressive', 'defensive', 'showman']);
    expect(es.idles).toEqual([{ code: 'idle_3', clip: 'idle', n: 3 }]);
    expect(es.victories).toEqual([{ code: 'win_2', clip: 'win', n: 2 }]);
    expect(es.statMin).toBe(20);
    expect(buildChoices(botarga, 'en').abilities[0]?.name).not.toBe(es.abilities[0]?.name);
  });
});

describe('drawOptions', () => {
  it('names attacks and words clips, leaving out what is neither', () => {
    expect(
      drawOptions({ id: 1, level: 5, kind: 'ability', options: ['spicy_slap'] }, 'es'),
    ).toEqual([{ id: 'spicy_slap', kind: 'ability', name: 'Cachetada Picosa' }]);
    expect(
      drawOptions({ id: 2, level: 10, kind: 'cosmetic', options: ['idle', 'win_3', 'x'] }, 'en'),
    ).toEqual([
      { id: 'idle', kind: 'clip', clip: { code: 'idle', clip: 'idle', n: 1 } },
      { id: 'win_3', kind: 'clip', clip: { code: 'win_3', clip: 'win', n: 3 } },
    ]);
  });
});
