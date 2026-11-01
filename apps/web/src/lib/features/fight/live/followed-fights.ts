import type { FightsCurrent } from '@torneo/protocol';

// Which `fight:{id}` to follow (010 plan-d decisions 5 and 6): `current` from `pre_live` on, and the
// watched fight while it has no `end`, so a cut does not leave it without events once it is `last`
// (RF57). Never `next` nor a `last` it is not watching: the board already brings their summaries.
export function followedFights(
  board: FightsCurrent | null,
  watchedId: number | null,
  watchedHasEnd: boolean,
): number[] {
  const ids: number[] = [];
  const current = board?.current ?? null;
  if (current !== null && (current.state === 'betting_closed' || current.state === 'live')) {
    ids.push(current.id);
  }
  if (watchedId !== null && !watchedHasEnd && !ids.includes(watchedId)) ids.push(watchedId);
  return ids;
}
