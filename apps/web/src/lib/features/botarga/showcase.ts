import type { Timeline } from '@torneo/renderer/timeline';

// What `/botarga` plays to show the botarga (021 "Web"): its idle while paused at the start, then
// one heavy strike that sends the sparring partner down and its victory, so the idle and the victory
// it picked are what is seen. Side `a` is the player's botarga; `b`, a sparring partner in the
// tournament variants. A timeline of the renderer like any other: `FightStage` validates it on load,
// and the e2e checks that the stage gets ready.

export const SHOWCASE_IDLE_MS = 2500;
const KO_GAP_MS = 400;
const WIN_GAP_MS = 1500;
const WIN_POSE_MS = 4000;
const MAX_HP = 100;

export const SHOWCASE: Timeline = {
  timeline_version: 3,
  duration_ms: SHOWCASE_IDLE_MS + KO_GAP_MS + WIN_GAP_MS + WIN_POSE_MS,
  fighters: {
    a: { asset: 'dummy_a', max_hp: MAX_HP },
    b: { asset: 'dummy_b', max_hp: MAX_HP },
  },
  cues: [
    { at_ms: 0, type: 'intro' },
    {
      at_ms: SHOWCASE_IDLE_MS,
      type: 'strike',
      actor: 'a',
      move: 'attack_heavy',
      result: 'crit',
      damage: MAX_HP,
      target_hp: 0,
    },
    { at_ms: SHOWCASE_IDLE_MS + KO_GAP_MS, type: 'ko', fighter: 'b' },
    { at_ms: SHOWCASE_IDLE_MS + KO_GAP_MS + WIN_GAP_MS, type: 'win', fighter: 'a' },
  ],
};
