// What `FightStage` needs to play a fight in progress (010 plan-c decision 11): the goteo of
// /dev/arena today, the WS of the home page in phase D.
import type { Timeline } from '@torneo/renderer/timeline';

export interface LiveSession {
  /** The playback clock: negative before the fight is due on screen. */
  readonly clock: () => number;
  /** The timeline at the moment of opening. */
  readonly timeline: Timeline;
  /**
   * Calls `push` with each newer timeline, and right away with the current one if it is newer than
   * `timeline`: an `end` released before subscribing is not lost. Returns the unsubscribe function.
   */
  subscribe(push: (timeline: Timeline) => void): () => void;
}
