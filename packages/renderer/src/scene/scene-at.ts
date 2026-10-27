// sceneAt: the scene as a pure function of time (RF-A9). It reads cues only through
// `cueWindow`, so its cost is bounded by K and not by the length of the timeline.
import { flashAlpha } from '../fx/flash.ts';
import { damageNumberAt } from '../fx/numbers.ts';
import { MAX_PARTICLES, particlesAt, type ParticleKind } from '../fx/particles.ts';
import { NO_OFFSET, shakeOffset, type Offset, type ShakeKind } from '../fx/shake.ts';
import { punchZoom } from '../fx/zoom.ts';
import type { ManifestsById } from '../timeline/validate.ts';
import { otherSide, type Side, type Timeline } from '../timeline/types.ts';
import { bannersAt } from './banners.ts';
import { cameraFrom } from './camera.ts';
import { frameAt, xAt, type Segment } from './choreography.ts';
import { hudFighterAt } from './hud.ts';
import { cueWindow, prepareScene, type PreparedCue, type PreparedScene } from './prepare.ts';
import {
  FACING,
  GROUND_Y,
  MARK_X,
  type DamageNumberEffect,
  type EffectScene,
  type FighterScene,
  type FlashEffect,
  type ParticleEffect,
  type Phase,
  type Scene,
  type SceneOptions,
} from './types.ts';

/** Height of the impact point above the ground, and of the damage numbers. */
const IMPACT_HEIGHT = 56;
const NUMBER_HEIGHT = 104;

function fighterAt(
  prepared: PreparedScene,
  window: readonly PreparedCue[],
  side: Side,
  t: number,
): FighterScene {
  const fighter = prepared.fighters[side];
  const base = {
    side,
    asset: fighter.asset,
    y: GROUND_Y,
    mirror: side === 'b',
    scale: fighter.scale,
  };
  const hold = fighter.hold;
  if (hold !== null && t >= hold.start) {
    const timing = fighter.timing[hold.animation];
    return { ...base, animation: timing.code, ...frameAt(timing, t - hold.start), x: hold.x };
  }
  let active: Segment | undefined;
  for (const pc of window) {
    for (const segment of pc.segments) {
      if (segment.side !== side || segment.start > t || t >= segment.end) continue;
      if (active === undefined || segment.start > active.start) active = segment;
    }
  }
  if (active === undefined) {
    // Idle at the mark, with a global phase so idle never depends on history.
    const timing = fighter.timing.idle;
    return { ...base, animation: timing.code, ...frameAt(timing, t), x: MARK_X[side] };
  }
  const timing = fighter.timing[active.animation];
  return {
    ...base,
    animation: timing.code,
    ...frameAt(timing, t - active.animStart),
    x: xAt(active, t),
  };
}

function phaseAt(prepared: PreparedScene, t: number): Phase {
  if (t >= prepared.durationMs) return 'finished';
  if (prepared.win !== null && t >= prepared.win.at) return 'win';
  if (prepared.ko !== null && t >= prepared.ko.at) return 'ko';
  if (prepared.firstStrikeAt !== null && t >= prepared.firstStrikeAt) return 'fight';
  return 'intro';
}

interface Fx {
  zoom: number;
  shake: Offset;
  particles: ParticleEffect[];
  others: EffectScene[];
}

function addShake(fx: Fx, seed: number, kind: ShakeKind, tau: number): void {
  const offset = shakeOffset(seed, kind, tau);
  if (offset !== NO_OFFSET) fx.shake = { x: fx.shake.x + offset.x, y: fx.shake.y + offset.y };
}

function effectsAt(window: readonly PreparedCue[], t: number, reducedMotion: boolean): Fx {
  const fx: Fx = { zoom: 1, shake: NO_OFFSET, particles: [], others: [] };
  for (const pc of window) {
    const cue = pc.cue;
    const tau = t - cue.at_ms;
    if (tau < 0) continue;

    if (cue.type === 'ko' && !reducedMotion) {
      addShake(fx, pc.shakeSeed, 'ko', tau);
      fx.zoom = Math.max(fx.zoom, punchZoom(tau));
      continue;
    }
    const strike = pc.strike;
    if (cue.type !== 'strike' || strike === null) continue;
    const defender = otherSide(cue.actor);
    const kind = strike.impactKind;
    const connects = kind === 'hit' || kind === 'crit' || kind === 'special';

    if (connects && !reducedMotion) {
      addShake(fx, strike.seeds.shake, kind, tau);
      const ox = (strike.attackerX + strike.defenderX) / 2;
      const color = kind === 'hit' ? 'text' : 'crit';
      for (const p of particlesAt(
        strike.seeds.particles,
        kind as ParticleKind,
        tau,
        ox,
        GROUND_Y - IMPACT_HEIGHT,
        FACING[cue.actor],
      )) {
        fx.particles.push({ kind: 'particle', ...p, color });
      }
      if (kind === 'special') fx.zoom = Math.max(fx.zoom, punchZoom(tau));
    }

    if (strike.flash) {
      const alpha = flashAlpha(tau);
      if (alpha > 0) {
        const flash: FlashEffect = { kind: 'flash', cue: pc.index, side: defender, alpha };
        fx.others.push(flash);
      }
    }

    if (cue.damage > 0) {
      const n = damageNumberAt(strike.seeds.numbers, tau, strike.crit, reducedMotion);
      if (n !== null) {
        const number: DamageNumberEffect = {
          kind: 'damage',
          cue: pc.index,
          value: cue.damage,
          crit: strike.crit,
          x: strike.defenderX + n.dx,
          y: GROUND_Y - NUMBER_HEIGHT + n.dy,
          alpha: n.alpha,
          scale: n.scale,
          color: strike.crit ? 'crit' : 'text',
        };
        fx.others.push(number);
      }
    }
  }
  // Hard cap: keep the newest particles (they were pushed in cue order).
  if (fx.particles.length > MAX_PARTICLES) fx.particles = fx.particles.slice(-MAX_PARTICLES);
  return fx;
}

export function sceneAt(prepared: PreparedScene, time: number, options: SceneOptions): Scene {
  // A time outside [0, duration] never reaches the scene: NaN and −∞ are the start, anything
  // past the end (+∞ included) is the end.
  const t = Number.isNaN(time) ? 0 : Math.min(prepared.durationMs, Math.max(0, time));
  const window = cueWindow(prepared, t);
  const fighters = {
    a: fighterAt(prepared, window, 'a', t),
    b: fighterAt(prepared, window, 'b', t),
  };
  const fx = effectsAt(window, t, options.reducedMotion);
  const win = prepared.win;
  return {
    t,
    phase: phaseAt(prepared, t),
    winner: win !== null && t >= win.at ? win.fighter : null,
    fighters,
    hud: {
      a: hudFighterAt(prepared.fighters.a, t),
      b: hudFighterAt(prepared.fighters.b, t),
    },
    camera: options.reducedMotion
      ? cameraFrom(1, NO_OFFSET, { a: fighters.a.x, b: fighters.b.x })
      : cameraFrom(fx.zoom, fx.shake, { a: fighters.a.x, b: fighters.b.x }),
    banners: bannersAt(prepared, t, options.reducedMotion),
    effects: [...fx.others, ...fx.particles],
  };
}

/** The signature of the spec (RF-A9): prepares and evaluates in one call. */
export function sceneAtTimeline(
  timeline: Timeline,
  manifests: ManifestsById,
  t: number,
  options: SceneOptions,
): Scene {
  return sceneAt(prepareScene(timeline, manifests), t, options);
}
