// HUD, banners and damage numbers of the 3D arena, in DOM over the canvas (013). Same layout as the
// Pixi HUD (`layoutHud`, CSS px of the container) and the same theme roles; text comes from the
// host. Each frame only writes styles that changed.
import { Vector3, type PerspectiveCamera } from 'three';
import type { RendererLabels, RendererTheme } from '../player/types.ts';
import { layoutHud, type HudLayout, type MeasureText, type Viewport } from '../scene/hud.ts';
import type {
  BannerCode,
  BannerScene,
  ColorRole,
  DamageNumberEffect,
  HudScene,
} from '../scene/types.ts';
import { SIDES, type Side } from '../timeline/types.ts';
import { arenaPoint } from './world.ts';

/** Alpha of the empty track behind the bars, in the text color of the theme (as in Pixi). */
const TRACK_ALPHA = 0.18;
const BANNERS: readonly BannerCode[] = ['fight', 'ko', 'time'];
/** Damage numbers on screen at once; older ones are hidden. */
const NUMBER_POOL = 8;
const NUMBER_FONT_RATIO = 0.07;

export const cssColor = (hex: number): string => `#${hex.toString(16).padStart(6, '0')}`;

function colorOf(theme: RendererTheme, role: ColorRole): string {
  return cssColor(theme.colors[role]);
}

function div(parent: HTMLElement, style: Partial<CSSStyleDeclaration>): HTMLDivElement {
  const el = document.createElement('div');
  Object.assign(el.style, { position: 'absolute', ...style });
  parent.appendChild(el);
  return el;
}

export interface Overlay {
  readonly element: HTMLDivElement;
  layout(viewport: Viewport): void;
  apply(
    hud: HudScene,
    banners: readonly BannerScene[],
    numbers: readonly DamageNumberEffect[],
    camera: PerspectiveCamera,
    formatNumber: (n: number) => string,
    /** The turn of the fight axis (018): the numbers follow the fighters. */
    angle?: number,
  ): void;
  setTheme(theme: RendererTheme): void;
  destroy(): void;
}

export function createOverlay(
  container: HTMLElement,
  theme: RendererTheme,
  labels: RendererLabels,
): Overlay {
  const element = document.createElement('div');
  Object.assign(element.style, {
    position: 'absolute',
    inset: '0',
    pointerEvents: 'none',
    overflow: 'hidden',
    userSelect: 'none',
  });
  element.setAttribute('aria-hidden', 'true');
  container.appendChild(element);

  const bar = () => div(element, { left: '0', top: '0', width: '0', height: '0' });
  const track = { a: bar(), b: bar() };
  const trail = { a: bar(), b: bar() };
  const hp = { a: bar(), b: bar() };
  const names = {
    a: div(element, { whiteSpace: 'nowrap', lineHeight: '1' }),
    b: div(element, { whiteSpace: 'nowrap', lineHeight: '1' }),
  };
  const banners = {} as Record<BannerCode, HTMLDivElement>;
  for (const code of BANNERS) {
    const el = div(element, {
      left: '50%',
      top: '50%',
      fontWeight: '800',
      whiteSpace: 'nowrap',
      lineHeight: '1',
      display: 'none',
    });
    el.textContent = labels.banners[code];
    banners[code] = el;
  }
  const numberEls = Array.from({ length: NUMBER_POOL }, () =>
    div(element, { left: '0', top: '0', fontWeight: '800', lineHeight: '1', display: 'none' }),
  );

  let current = theme;
  let layout: HudLayout | null = null;
  let viewport: Viewport = { width: 0, height: 0 };
  const canvas = document.createElement('canvas').getContext('2d');
  const measure: MeasureText = (text, px) => {
    if (canvas === null) return text.length * px * 0.55;
    canvas.font = `${px}px ${current.fonts.sans}`;
    return canvas.measureText(text).width;
  };
  const point = new Vector3();

  const paint = (): void => {
    for (const side of SIDES) {
      track[side].style.background = colorOf(current, 'text');
      track[side].style.opacity = String(TRACK_ALPHA);
      trail[side].style.background = colorOf(current, 'hpLost');
      hp[side].style.background = colorOf(current, 'hp');
      names[side].style.color = colorOf(current, 'text');
      names[side].style.fontFamily = current.fonts.sans;
    }
    banners.fight.style.color = colorOf(current, 'text');
    banners.ko.style.color = colorOf(current, 'crit');
    // The bell is not a blow: no crit color (005 spec P1).
    banners.time.style.color = colorOf(current, 'text');
    for (const code of BANNERS) banners[code].style.fontFamily = current.fonts.display;
    for (const el of numberEls) el.style.fontFamily = current.fonts.display;
  };
  paint();

  const fill = (el: HTMLDivElement, side: Side, fraction: number): void => {
    if (layout === null) return;
    const rect = layout.bars[side];
    const width = rect.width * Math.max(0, Math.min(1, fraction));
    const left = side === 'a' ? rect.x + rect.width - width : rect.x;
    el.style.transform = `translate(${left}px, ${rect.y}px)`;
    el.style.width = `${width}px`;
    el.style.height = `${rect.height}px`;
  };

  return {
    element,
    layout(next) {
      viewport = next;
      layout = layoutHud(next, measure, labels.names);
      for (const side of SIDES) {
        fill(track[side], side, 1);
        const name = layout.names[side];
        const el = names[side];
        el.textContent = name.text;
        el.style.fontSize = `${layout.fontPx}px`;
        el.style.top = `${name.y}px`;
        if (side === 'a') el.style.left = `${name.x}px`;
        else el.style.right = `${next.width - name.x}px`;
      }
      for (const code of BANNERS) banners[code].style.fontSize = `${layout.banner.fontPx}px`;
    },
    apply(hud, sceneBanners, numbers, camera, formatNumber, angle = 0) {
      for (const side of SIDES) {
        const f = hud[side];
        fill(trail[side], side, f.trail / f.maxHp);
        fill(hp[side], side, f.hp / f.maxHp);
      }
      for (const code of BANNERS) {
        const banner = sceneBanners.find((b) => b.code === code);
        const el = banners[code];
        if (banner === undefined || banner.alpha <= 0) {
          el.style.display = 'none';
          continue;
        }
        el.style.display = 'block';
        el.style.opacity = String(banner.alpha);
        el.style.transform = `translate(-50%, -50%) scale(${banner.scale})`;
      }
      const fontPx = Math.max(14, Math.round(viewport.height * NUMBER_FONT_RATIO));
      numberEls.forEach((el, i) => {
        const n = numbers[numbers.length - 1 - i];
        if (n === undefined) {
          el.style.display = 'none';
          return;
        }
        const at = arenaPoint(n.x, n.y, angle);
        point.set(at.x, at.y, at.z).project(camera);
        const x = ((point.x + 1) / 2) * viewport.width;
        // Never over the HUD: on a short stage the head is close to the bars and the names.
        const hudBottom = layout === null ? 0 : layout.names.a.y + layout.fontPx;
        const y = Math.max(hudBottom + fontPx / 2, ((1 - point.y) / 2) * viewport.height);
        el.textContent = formatNumber(n.value);
        el.style.display = 'block';
        el.style.color = colorOf(current, n.color);
        el.style.fontSize = `${fontPx}px`;
        el.style.opacity = String(n.alpha);
        el.style.transform = `translate(${x}px, ${y}px) translate(-50%, -50%) scale(${n.scale})`;
      });
    },
    setTheme(next) {
      current = next;
      paint();
    },
    destroy() {
      element.remove();
    },
  };
}
