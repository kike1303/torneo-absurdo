import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { HOUSE_LOOKS, HOUSE_UNIFORM, wardrobeCatalog } from './botarga-model.js';

const uniform = (id: string, file: string) =>
  fileURLToPath(new URL(`../../../../../../content/models/uniform/${id}/${file}`, import.meta.url));

describe('the house uniform (TA-91)', () => {
  it('is in content/models/uniform, out of the wardrobe a player picks from', () => {
    expect(HOUSE_UNIFORM).toEqual(['tournament_shirt', 'tournament_pants']);
    for (const id of HOUSE_UNIFORM) {
      expect(existsSync(uniform(id, `${id}.glb`)), id).toBe(true);
      expect(existsSync(uniform(id, 'accessory.json')), id).toBe(true);
    }
    const { accessories } = wardrobeCatalog();
    for (const id of HOUSE_UNIFORM) expect(accessories).not.toContain(id);
  });

  it('dresses both fighters of a house fight', () => {
    expect(HOUSE_LOOKS.a?.wear).toEqual(HOUSE_UNIFORM);
    expect(HOUSE_LOOKS.b?.wear).toEqual(HOUSE_UNIFORM);
  });
});
