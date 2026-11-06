import { describe, expect, it } from 'vitest';
import draw from './fixtures/golden/draw.timeline.json' with { type: 'json' };
import { validateTimeline } from '../src/timeline/validate.ts';
import { MANIFESTS } from './support/placeholders.ts';

type Json = { timeline_version: number; cues: Record<string, unknown>[] };

/** The draw golden (adapter output, v3) with every strike's ability set to `ability`. */
function withAbility(version: number, ability: unknown): Json {
  const base = structuredClone(draw) as Json;
  return {
    ...base,
    timeline_version: version,
    cues: base.cues.map((cue) => (cue.type === 'strike' ? { ...cue, ability } : cue)),
  };
}

const firstStrike = (json: Json) => json.cues.findIndex((cue) => cue.type === 'strike');

describe('timeline v3 (015)', () => {
  it('accepts the ability of a strike and keeps it', () => {
    const result = validateTimeline(withAbility(3, 'pipe_geyser'), MANIFESTS);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const strike = result.timeline.cues.find((cue) => cue.type === 'strike');
    expect(strike).toMatchObject({ ability: 'pipe_geyser' });
  });

  it('accepts a strike without ability', () => {
    const json = structuredClone(draw) as Json;
    for (const cue of json.cues) delete cue.ability;
    expect(validateTimeline(json, MANIFESTS).ok).toBe(true);
  });

  it.each(['', 'Pipe', 'with space', 'x'.repeat(65), 7, null])('rejects ability %j', (ability) => {
    const json = withAbility(3, ability);
    expect(validateTimeline(json, MANIFESTS)).toEqual({
      ok: false,
      errors: expect.arrayContaining([
        { code: 'timeline.invalid_value', path: `/cues/${firstStrike(json)}/ability` },
      ]) as unknown,
    });
  });

  it.each([1, 2])('rejects an ability in v%i as an unknown field', (version) => {
    const json = withAbility(version, 'pipe_geyser');
    const result = validateTimeline(json, MANIFESTS);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.errors).toContainEqual({
      code: 'timeline.unknown_field',
      path: `/cues/${firstStrike(json)}/ability`,
    });
  });
});
