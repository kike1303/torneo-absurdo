import { describe, expect, it } from 'vitest';
import shopFile from '../../../../../../content/catalog/shop.json';
import namesEn from '../../../../../../content/catalog/names.en.json';
import namesEs from '../../../../../../content/catalog/names.es.json';

// A suit of the shop (024) shows by its name in /shop and in /botarga, never by its slug: each one
// needs its name in both languages before it is sold (ADR-0007).
describe('the suits of the shop', () => {
  it('have a name in every locale', () => {
    const skins = shopFile.items.filter((item) => item.kind === 'skin').map((item) => item.id);
    for (const names of [namesEs, namesEn]) {
      const named = names.skins as Record<string, string>;
      expect(skins.filter((id) => !(named[id] ?? '').trim())).toEqual([]);
    }
  });
});
