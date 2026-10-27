// Particles and damage numbers from pools (plan-a2 decision 6): no objects per frame, and a
// number's text changes only when its slot shows another cue.
import { Container, Sprite, Text, Texture } from 'pixi.js';
import { MAX_PARTICLES } from '../fx/particles.ts';
import type { RendererTheme } from '../player/types.ts';
import type { DamageNumberEffect, ParticleEffect } from '../scene/types.ts';
import { roleColor } from './colors.ts';

/** Damage numbers alive at once never pass this (≤ 900 ms each, ≥ 600 ms between strikes). */
const NUMBER_POOL = 8;
/** CSS px of a damage number at scale 1; crits are bigger through the effect's scale. */
const NUMBER_CSS_PX = 18;

export interface EffectLayer {
  readonly particles: Container;
  readonly numbers: Container;
  /** Font size in logical px and its raster resolution, so text stays ≥ 14 CSS px and sharp. */
  resize(stageScale: number, resolution: number): void;
  apply(
    particles: readonly ParticleEffect[],
    numbers: readonly DamageNumberEffect[],
    theme: RendererTheme,
    formatNumber: (n: number) => string,
  ): void;
}

interface NumberSlot {
  readonly text: Text;
  cue: number;
}

export function createEffectLayer(theme: RendererTheme): EffectLayer {
  const particles = new Container();
  const numbers = new Container();
  const dots: Sprite[] = [];
  for (let i = 0; i < MAX_PARTICLES; i++) {
    const dot = new Sprite(Texture.WHITE);
    dot.anchor.set(0.5);
    dot.visible = false;
    particles.addChild(dot);
    dots.push(dot);
  }
  const slots: NumberSlot[] = [];
  for (let i = 0; i < NUMBER_POOL; i++) {
    const text = new Text({
      text: '',
      style: { fontFamily: theme.fonts.display, fontSize: NUMBER_CSS_PX, fontWeight: '700' },
    });
    text.anchor.set(0.5);
    text.visible = false;
    numbers.addChild(text);
    slots.push({ text, cue: -1 });
  }

  return {
    particles,
    numbers,
    resize(stageScale, resolution) {
      const size = stageScale > 0 ? NUMBER_CSS_PX / stageScale : NUMBER_CSS_PX;
      for (const slot of slots) {
        slot.text.style.fontSize = size;
        slot.text.resolution = resolution * Math.max(1, stageScale);
      }
    },
    apply(activeParticles, activeNumbers, current, formatNumber) {
      for (let i = 0; i < dots.length; i++) {
        const dot = dots[i] as Sprite;
        const p = activeParticles[i];
        if (p === undefined) {
          dot.visible = false;
          continue;
        }
        dot.position.set(p.x, p.y);
        dot.width = p.size;
        dot.height = p.size;
        dot.tint = roleColor(current, p.color);
        dot.alpha = p.alpha;
        dot.visible = true;
      }

      // Keep the slot of every number that is still alive, then give free slots to new ones.
      const used = new Set<NumberSlot>();
      const pending: DamageNumberEffect[] = [];
      for (const n of activeNumbers) {
        const slot = slots.find((s) => s.cue === n.cue && !used.has(s));
        if (slot === undefined) pending.push(n);
        else used.add(slot);
      }
      for (const n of pending) {
        const slot = slots.find((s) => !used.has(s));
        if (slot === undefined) break;
        slot.cue = n.cue;
        slot.text.text = formatNumber(n.value);
        used.add(slot);
      }
      for (const slot of slots) {
        const n = used.has(slot) ? activeNumbers.find((a) => a.cue === slot.cue) : undefined;
        if (n === undefined) {
          slot.text.visible = false;
          continue;
        }
        slot.text.position.set(n.x, n.y);
        slot.text.scale.set(n.scale);
        slot.text.alpha = n.alpha;
        const fill = roleColor(current, n.color);
        if (slot.text.style.fill !== fill) slot.text.style.fill = fill;
        slot.text.visible = true;
      }
    },
  };
}
