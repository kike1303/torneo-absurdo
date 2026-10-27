// Fighters the test arena can pick (002 plan-a2 decision 10). Data only: no renderer here, so
// the controls and the page can use it without loading Pixi. Phase B adds the local ones.
export const ARENA_FIGHTERS = ['dummy_a', 'dummy_b'] as const;
export type ArenaFighterId = (typeof ARENA_FIGHTERS)[number];

export function isArenaFighter(value: string): value is ArenaFighterId {
  return (ARENA_FIGHTERS as readonly string[]).includes(value);
}
