// Engine logs that /dev/arena can play with `?log=<name>` (002 RF-C3, plan-c decision 7; the
// endings by time in 005 RF15; all seven goldens in 006 RF10). Each one is a byte-exact copy of an
// engine golden, in its own lazy chunk. Its types come from the adapter, so this file never
// imports the log and the timeline shapes at once (decision 8).
import {
  timelineFromFightLog,
  type AdaptedTimeline,
  type AdapterLog,
} from './timeline-from-fight-log.js';

type FighterSide = keyof AdaptedTimeline['fighters'];

export const ENGINE_LOGS = {
  'ko-comeback': () => import('./fixtures/log-ko-comeback.json'),
  'ko-clutch': () => import('./fixtures/log-ko-clutch.json'),
  'ko-decisive-crit': () => import('./fixtures/log-ko-decisive-crit.json'),
  'ko-knockdown': () => import('./fixtures/log-ko-knockdown.json'),
  'decision-a': () => import('./fixtures/log-decision-a.json'),
  'decision-b': () => import('./fixtures/log-decision-b.json'),
  draw: () => import('./fixtures/log-draw.json'),
} as const;

export type EngineLogName = keyof typeof ENGINE_LOGS;

/** The name of `?log=`, only from the exact allow-list; anything else is null (the script). */
export function parseEngineLog(raw: string | null): EngineLogName | null {
  return raw !== null && Object.hasOwn(ENGINE_LOGS, raw) ? (raw as EngineLogName) : null;
}

/**
 * The timeline of an engine log, with the given assets. A log the adapter rejects throws the
 * error `FightStage` already shows (`timeline_invalid`); nothing goes to the console (ADR-0012).
 */
export async function loadEngineLogTimeline(
  name: EngineLogName,
  assets: Readonly<Record<FighterSide, string>>,
): Promise<AdaptedTimeline> {
  // JSON imports type `type` as `string`: the cast stays here, the adapter checks the content.
  const log = (await ENGINE_LOGS[name]()).default as unknown as AdapterLog;
  const result = timelineFromFightLog(log, { a: { asset: assets.a }, b: { asset: assets.b } });
  if (!result.ok)
    throw Object.assign(new Error('timeline_invalid'), { code: 'timeline_invalid' as const });
  return result.timeline;
}
