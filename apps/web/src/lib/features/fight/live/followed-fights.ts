import type { FightsCurrent } from '@torneo/protocol';

// Which `fight:{id}` to follow (010 plan-d decisions 5 and 6): the watched fight while it has no
// `end`, so a cut does not leave it without events once it is `last` (RF57), or while it is `current`.
// A `current` that waits for it to finish is not followed yet: its events would have no place. Never
// `next` nor a `last` it is not watching: the board already brings their summaries.
export function followedFights(
  board: FightsCurrent | null,
  watchedId: number | null,
  watchedHasEnd: boolean,
): number[] {
  const current = board?.current ?? null;
  const active =
    current !== null && (current.state === 'betting_closed' || current.state === 'live');
  if (watchedId === null) return active ? [current.id] : [];
  return !watchedHasEnd || (active && current.id === watchedId) ? [watchedId] : [];
}
