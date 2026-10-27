// The only file that knows where the engine goldens live (002 plan-c decision 5). They are read in
// place, never copied (003 plan-a decision 6); the arena's single copy is checked byte by byte.
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import type { FightLog } from '@torneo/protocol';

export const GOLDEN_DIR = fileURLToPath(
  new URL('../../../../crates/engine/tests/golden/', import.meta.url),
);

/** The goldens that end in a KO: timeline v1 can play them. */
export const KO_LOGS = [
  'log-ko-clutch',
  'log-ko-comeback',
  'log-ko-decisive-crit',
  'log-ko-knockdown',
] as const;

/** The goldens without a KO: `adapter.unsupported_ending` until timeline v2 (TA-19). */
export const UNSUPPORTED_LOGS = ['log-decision-a', 'log-decision-b', 'log-draw'] as const;

export type GoldenLogName = (typeof KO_LOGS)[number] | (typeof UNSUPPORTED_LOGS)[number];

/** The `log-*` names found in the directory (throws if it does not exist). */
export function goldenLogNames(): string[] {
  return readdirSync(GOLDEN_DIR)
    .filter((file) => /^log-.*\.json$/.test(file))
    .map((file) => file.replace(/\.json$/, ''))
    .sort();
}

export function readGoldenBytes(name: GoldenLogName): Buffer {
  return readFileSync(`${GOLDEN_DIR}${name}.json`);
}

export function readGoldenLog(name: GoldenLogName): FightLog {
  return JSON.parse(readGoldenBytes(name).toString('utf8')) as FightLog;
}
