// Fighters the test arena can pick (002 plan-a2 decision 10). Data only: no renderer here, so
// the controls and the page can use it without loading Pixi. Phase B adds the local ones: any
// other id is a fighter converted into content/_local/sprites/ (local-fighters.ts).
export const ARENA_FIGHTERS = ['dummy_a', 'dummy_b'] as const;
export type PlaceholderFighterId = (typeof ARENA_FIGHTERS)[number];
/** A placeholder or the id of a local fighter (`^[a-z][a-z0-9_]{0,47}$`). */
export type ArenaFighterId = string;

export function isPlaceholderFighter(value: string): value is PlaceholderFighterId {
  return (ARENA_FIGHTERS as readonly string[]).includes(value);
}
