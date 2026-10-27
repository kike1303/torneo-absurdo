import type { ArenaFighterId } from '#lib/features/arena/fighters.js';
import { m } from '#lib/paraglide/messages.js';

/** Translated name of an arena fighter (the renderer only draws what the host gives it). */
export function fighterName(id: ArenaFighterId): string {
  switch (id) {
    case 'dummy_a':
      return m.arena_fighter_dummy_a();
    case 'dummy_b':
      return m.arena_fighter_dummy_b();
  }
}
