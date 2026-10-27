// Builds small valid timelines for the choreography tests; target_hp is computed here.
import type { SpriteManifest } from '../../src/manifest/index.ts';
import {
  validateTimeline,
  type Cue,
  type Move,
  type Side,
  type StrikeResult,
  type Timeline,
  type TimelineVersion,
} from '../../src/timeline/index.ts';
import { MANIFESTS } from './placeholders.ts';

export interface BuilderOptions {
  readonly maxHp?: number;
  readonly manifests?: ReadonlyMap<string, SpriteManifest>;
  readonly assets?: Readonly<Record<Side, string>>;
  /** `timeline_version` of the KO timelines of `finish` (1 by default). */
  readonly version?: TimelineVersion;
}

export class TimelineBuilder {
  private readonly cues: Cue[] = [{ at_ms: 0, type: 'intro' }];
  private readonly hp: Record<Side, number>;
  private readonly maxHp: number;
  readonly manifests: ReadonlyMap<string, SpriteManifest>;
  private readonly assets: Readonly<Record<Side, string>>;
  private readonly version: TimelineVersion;

  constructor(options: BuilderOptions = {}) {
    this.maxHp = options.maxHp ?? 1000;
    this.hp = { a: this.maxHp, b: this.maxHp };
    this.manifests = options.manifests ?? MANIFESTS;
    this.assets = options.assets ?? { a: 'dummy_a', b: 'dummy_b' };
    this.version = options.version ?? 1;
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
    return this.validate(this.version, at + 1500 + 2000);
  }

  /**
   * Ends by time (v2): `time_up` at `at`, then the closing 1 500 ms later, `win` of `winner` (a
   * decision) or `draw` when it is null, and 2 500 ms of pose, like the engine goldens.
   */
  finishByTime(at: number, winner: Side | null): Timeline {
    this.cues.push({ at_ms: at, type: 'time_up' });
    this.cues.push(
      winner === null
        ? { at_ms: at + 1500, type: 'draw' }
        : { at_ms: at + 1500, type: 'win', fighter: winner },
    );
    return this.validate(2, at + 4000);
  }

  private validate(version: TimelineVersion, duration: number): Timeline {
    const json = {
      timeline_version: version,
      duration_ms: duration,
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
