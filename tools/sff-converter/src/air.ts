// AIR parser (002 RF-B3, plan-b decision 5): line by line, simple anchors and split(','), no
// backtracking regular expressions. Collision boxes, interpolation, blending, scale and angle
// are ignored and counted.
import { fail, type Note } from './errors.ts';
import { INPUT_LIMITS } from './limits.ts';

export interface AirFrame {
  /** -1: an empty frame. */
  readonly group: number;
  readonly image: number;
  readonly x: number;
  readonly y: number;
  /** -1: held forever. */
  readonly ticks: number;
  readonly flipH: boolean;
  readonly flipV: boolean;
}

export interface AirAction {
  readonly frames: readonly AirFrame[];
  /** Index of the first frame after `Loopstart`, or null. */
  readonly loopStart: number | null;
  /** Frame lines that could not be read (an error only if a mapped action has any). */
  readonly badLines: number;
}

export interface AirFile {
  readonly actions: ReadonlyMap<number, AirAction>;
  readonly notes: readonly Note[];
}

const INT = /^-?[0-9]{1,9}$/;

function int(text: string | undefined): number | null {
  const t = (text ?? '').trim();
  return INT.test(t) ? Number(t) : null;
}

interface Building {
  frames: AirFrame[];
  loopStart: number | null;
  badLines: number;
}

/** `[Begin Action N]`, case-insensitive, or null. */
function actionHeader(line: string): number | null | 'other' {
  if (!line.startsWith('[')) return null;
  const close = line.indexOf(']');
  if (close === -1) return 'other';
  const words = line.slice(1, close).trim().split(/\s+/);
  if (
    words.length !== 3 ||
    words[0]?.toLowerCase() !== 'begin' ||
    words[1]?.toLowerCase() !== 'action'
  ) {
    return 'other';
  }
  const n = int(words[2]);
  return n === null ? 'other' : n;
}

export function parseAir(input: Uint8Array | string): AirFile {
  const text = typeof input === 'string' ? input : Buffer.from(input).toString('latin1');
  if (text.length > INPUT_LIMITS.airBytes) fail('air.too_large', { length: text.length });
  const actions = new Map<number, AirAction>();
  const counts = new Map<string, number>();
  const count = (code: string) => counts.set(code, (counts.get(code) ?? 0) + 1);
  const duplicates: number[] = [];
  let current: Building | null = null;
  let currentId = 0;
  let total = 0;

  const close = () => {
    if (current === null) return;
    if (actions.has(currentId)) duplicates.push(currentId);
    else actions.set(currentId, { ...current });
    current = null;
  };

  const lines = text.split('\n');
  for (let n = 0; n < lines.length; n++) {
    const rawLine = lines[n] ?? '';
    if (rawLine.length > INPUT_LIMITS.airLineChars) fail('air.line_too_long', { line: n + 1 });
    const semicolon = rawLine.indexOf(';');
    const line = (semicolon === -1 ? rawLine : rawLine.slice(0, semicolon)).trim();
    if (line === '') continue;
    const header = actionHeader(line);
    if (header !== null) {
      close();
      if (header === 'other') continue;
      total++;
      if (total > INPUT_LIMITS.actions) fail('air.too_many_actions', { count: total });
      current = { frames: [], loopStart: null, badLines: 0 };
      currentId = header;
      continue;
    }
    if (current === null) continue;
    const building: Building = current;
    const lower = line.toLowerCase();
    if (lower.startsWith('clsn')) {
      count('air.clsn_ignored');
      continue;
    }
    if (lower.startsWith('interpolate')) {
      count('air.interpolate_ignored');
      continue;
    }
    if (lower === 'loopstart') {
      building.loopStart = building.frames.length;
      continue;
    }
    const fields = line.split(',');
    const group = int(fields[0]);
    const image = int(fields[1]);
    const x = int(fields[2]);
    const y = int(fields[3]);
    const ticks = int(fields[4]);
    if (
      group === null ||
      image === null ||
      x === null ||
      y === null ||
      ticks === null ||
      ticks < -1
    ) {
      building.badLines++;
      count('air.bad_line');
      continue;
    }
    const flip = (fields[5] ?? '').trim().toUpperCase();
    if (flip !== '' && flip !== 'H' && flip !== 'V' && flip !== 'HV' && flip !== 'VH') {
      building.badLines++;
      count('air.bad_line');
      continue;
    }
    if ((fields[6] ?? '').trim() !== '') count('air.blend_ignored');
    if (fields.slice(7).some((f) => f.trim() !== '')) count('air.transform_ignored');
    if (building.frames.length >= INPUT_LIMITS.framesPerAction) {
      fail('air.too_many_frames', { action: currentId });
    }
    building.frames.push({
      group,
      image,
      x,
      y,
      ticks,
      flipH: flip.includes('H'),
      flipV: flip.includes('V'),
    });
  }
  close();
  const notes: Note[] = [...counts].map(([code, n]) => ({ code, data: { count: n } }));
  for (const action of duplicates) notes.push({ code: 'air.duplicate_action', data: { action } });
  return { actions, notes };
}
