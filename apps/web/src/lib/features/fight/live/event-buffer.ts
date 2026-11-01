import type { FightEvent } from '@torneo/protocol';

// The events of one fight, as they arrive (010 DP-B9, plan-d decision 3). `fight_sync` brings none,
// so it never touches this. Pages are placed by `from_index` and can overlap what is already here,
// whatever their size.

/** The engine's guard (`max_events`): more than this is a protocol error. */
export const MAX_EVENTS = 512;

export type PlaceResult = 'ok' | 'gap' | 'overflow';

export interface EventBuffer {
  readonly events: readonly FightEvent[];
  place(fromIndex: number, events: readonly FightEvent[]): PlaceResult;
}

export function createEventBuffer(max = MAX_EVENTS): EventBuffer {
  const events: FightEvent[] = [];
  return {
    events,
    place(fromIndex, page) {
      // A gap cannot happen in one connection: the cursor of the server is contiguous.
      if (fromIndex > events.length) return 'gap';
      const fresh = page.slice(events.length - fromIndex);
      if (events.length + fresh.length > max) return 'overflow';
      events.push(...fresh);
      return 'ok';
    },
  };
}
