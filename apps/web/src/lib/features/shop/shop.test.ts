import type { ShopItem } from '@torneo/protocol';
import { describe, expect, it } from 'vitest';
import {
  itemParam,
  levelOfTier,
  parseBuyForm,
  parseItemParam,
  shopSections,
  tryOn,
} from './shop.js';

const item = (over: Partial<ShopItem> & Pick<ShopItem, 'kind' | 'id'>): ShopItem => ({
  price: 400,
  owned: false,
  in_draw: false,
  tier: null,
  level_ok: true,
  ...over,
});

describe('shopSections', () => {
  it('groups by kind, names each item and says what stands in the way', () => {
    const sections = shopSections(
      {
        balance: 1000,
        items: [
          item({ kind: 'accessory', id: 'oven_mitts', price: 750, tier: 1 }),
          item({ kind: 'cosmetic', id: 'idle_2', owned: true }),
          item({ kind: 'cosmetic', id: 'win_1', price: 750, in_draw: true }),
          item({ kind: 'accessory', id: 'tinfoil_crown', price: 1500, tier: 3, level_ok: false }),
        ],
      },
      'es',
    );
    // Animations first, then accessories; no suits, no section.
    expect(sections.map((s) => s.kind)).toEqual(['cosmetic', 'accessory']);
    const [clips, accessories] = sections;
    expect(clips?.entries.map((e) => [e.id, e.state, e.label])).toEqual([
      ['idle_2', 'owned', { kind: 'clip', clip: { code: 'idle_2', clip: 'idle', n: 2 } }],
      ['win_1', 'in_draw', { kind: 'clip', clip: { code: 'win_1', clip: 'win', n: 1 } }],
    ]);
    const mitts = accessories?.entries[0];
    expect(mitts).toMatchObject({ state: 'buyable', price: 750, level: null });
    expect(mitts?.label.kind).toBe('name');
    expect(mitts?.deltas).toEqual([
      { stat: 'attack', delta: 8 },
      { stat: 'speed', delta: -8 },
    ]);
    // Out of reach: the level that opens tier 3.
    expect(accessories?.entries[1]).toMatchObject({ state: 'level', level: 40 });
  });

  it('names a suit without a name by its slug', () => {
    const [skins] = shopSections(
      { balance: 0, items: [item({ kind: 'skin', id: 'tournament_suit', price: 7500 })] },
      'en',
    );
    expect(skins?.entries[0]?.label).toEqual({ kind: 'name', name: 'tournament_suit' });
  });
});

describe('levelOfTier', () => {
  it('is the inverse of the tier of a level', () => {
    expect([1, 2, 3, 4, 5].map(levelOfTier)).toEqual([1, 20, 40, 60, 80]);
  });
});

describe('the picked item', () => {
  it('reads only an item of a known kind and form', () => {
    expect(parseItemParam('accessory:oven_mitts')).toEqual({ kind: 'accessory', id: 'oven_mitts' });
    expect(parseItemParam(itemParam({ kind: 'skin', id: 'tournament_suit' }))).toEqual({
      kind: 'skin',
      id: 'tournament_suit',
    });
    for (const bad of [null, '', 'oven_mitts', 'hat:oven_mitts', 'accessory:', 'skin:../x']) {
      expect(parseItemParam(bad)).toBeNull();
    }
  });

  it('is tried on over the saved build, an accessory in the place of its slot', () => {
    const build = {
      cosmetics: { idle: null, victory: 'win_2' as const },
      accessories: ['propeller_beanie', 'oven_mitts'],
      skin: null,
    };
    expect(tryOn(build, null)).toEqual({
      cosmetics: build.cosmetics,
      wear: build.accessories,
      skin: null,
    });
    // Another head takes the place of the beanie.
    expect(tryOn(build, { kind: 'accessory', id: 'tinfoil_crown' }).wear).toEqual([
      'oven_mitts',
      'tinfoil_crown',
    ]);
    expect(tryOn(build, { kind: 'cosmetic', id: 'idle_3' }).cosmetics).toEqual({
      idle: 'idle_3',
      victory: 'win_2',
    });
    expect(tryOn(build, { kind: 'cosmetic', id: 'win_1' }).cosmetics.victory).toBe('win_1');
    expect(tryOn(build, { kind: 'skin', id: 'tournament_suit' }).skin).toBe('tournament_suit');
  });
});

describe('parseBuyForm', () => {
  const key = '0f8c1e2a-3b4d-4e5f-8a9b-0c1d2e3f4a5b';
  const form = (entries: Record<string, string>) => {
    const data = new FormData();
    for (const [k, v] of Object.entries(entries)) data.append(k, v);
    return data;
  };

  it('reads the purchase, with the price that was shown', () => {
    expect(
      parseBuyForm(
        form({ kind: 'cosmetic', item_id: 'idle_2', expected_price: '400', client_key: key }),
      ),
    ).toEqual({ kind: 'cosmetic', item_id: 'idle_2', expected_price: 400, client_key: key });
  });

  it('refuses what is not one', () => {
    const good = { kind: 'cosmetic', item_id: 'idle_2', expected_price: '400', client_key: key };
    for (const change of [
      { kind: 'coins' },
      { item_id: 'Idle 2' },
      { expected_price: '0' },
      { expected_price: '-400' },
      { expected_price: '4e2' },
      { client_key: key.toUpperCase() },
      { client_key: '' },
    ]) {
      expect(parseBuyForm(form({ ...good, ...change }))).toBeNull();
    }
  });
});
