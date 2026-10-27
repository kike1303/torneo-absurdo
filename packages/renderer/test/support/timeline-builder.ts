// Builds small valid timelines for the choreography tests; target_hp is computed here.
import type { SpriteManifest } from '../../src/manifest/index.ts';
import {
  validateTimeline,
  type Cue,
  type Move,
  type Side,
  type StrikeResult,
  type Timeline,
} from '../../src/timeline/index.ts';
import { MANIFESTS } from './placeholders.ts';

export interface BuilderOptions {
  readonly maxHp?: number;
  readonly manifests?: ReadonlyMap<string, SpriteManifest>;
  readonly assets?: Readonly<Record<Side, string>>;
}

export class TimelineBuilder {
  private readonly cues: Cue[] = [{ at_ms: 0, type: 'intro' }];
  private readonly hp: Record<Side, number>;
  private readonly maxHp: number;
  readonly manifests: ReadonlyMap<string, SpriteManifest>;
  private readonly assets: Readonly<Record<Side, string>>;

  constructor(options: BuilderOptions = {}) {
    this.maxHp = options.maxHp ?? 1000;
    this.hp = { a: this.maxHp, b: this.maxHp };
    this.manifests = options.manifests ?? MANIFESTS;
    this.assets = options.assets ?? { a: 'dummy_a', b: 'dummy_b' };
  }

  strike(at: number, actor: Side, move: Move, result: StrikeResult, damage = 0): this {
    const target: Side = actor === 'a' ? 'b' : 'a';
    const dmg = result === 'dodge' ? 0 : damage;
    this.hp[target] -= dmg;
    this.cues.push({
      at_ms: at,
      type: 'strike',
      actor,
      move,
      result,
      damage: dmg,
      target_hp: this.hp[target],
    });
    return this;
  }

  knockdown(at: number, fighter: Side, duration: number): this {
    this.cues.push({ at_ms: at, type: 'knockdown', fighter, duration_ms: duration });
    return this;
  }

  /** Ends with a finishing strike (when HP is left), `ko` and `win`, and validates. */
  finish(at: number, winner: Side = 'a'): Timeline {
    const loser: Side = winner === 'a' ? 'b' : 'a';
    if (this.hp[loser] > 0) this.strike(at, winner, 'special', 'crit', this.hp[loser]);
    this.cues.push({ at_ms: at, type: 'ko', fighter: loser });
    this.cues.push({ at_ms: at + 1500, type: 'win', fighter: winner });
    const json = {
      timeline_version: 1,
      duration_ms: at + 1500 + 2000,
      fighters: {
        a: { asset: this.assets.a, max_hp: this.maxHp },
        b: { asset: this.assets.b, max_hp: this.maxHp },
      },
      cues: this.cues,
    };
    const result = validateTimeline(json, this.manifests);
    if (!result.ok) throw new Error(JSON.stringify(result.errors.slice(0, 5)));
    return result.timeline;
  }
}

export const timeline = (options?: BuilderOptions) => new TimelineBuilder(options);
