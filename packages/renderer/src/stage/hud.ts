// HUD in CSS px of the container, outside the camera (RF-A13, plan-a2 decision 8). The bars are
// sprites of `Texture.WHITE`: moving and tinting them costs nothing per frame.
import { CanvasTextMetrics, Container, Sprite, Text, TextStyle, Texture } from 'pixi.js';
import type { RendererLabels, RendererTheme } from '../player/types.ts';
import { layoutHud, type HudLayout, type MeasureText, type Viewport } from '../scene/hud.ts';
import type { HudScene } from '../scene/types.ts';
import { SIDES, type Side } from '../timeline/types.ts';

/** Alpha of the empty track behind the bars, in the text color of the theme. */
const TRACK_ALPHA = 0.18;

export interface HudLayer {
  readonly container: Container;
  layout(viewport: Viewport): HudLayout;
  apply(hud: HudScene): void;
  setTheme(theme: RendererTheme): void;
}

function bar(): Sprite {
  const sprite = new Sprite(Texture.WHITE);
  sprite.visible = false;
  return sprite;
}

export function createHudLayer(theme: RendererTheme, labels: RendererLabels): HudLayer {
  const container = new Container();
  const track: Record<Side, Sprite> = { a: bar(), b: bar() };
  const trail: Record<Side, Sprite> = { a: bar(), b: bar() };
  const hp: Record<Side, Sprite> = { a: bar(), b: bar() };
  const names: Record<Side, Text> = {
    a: new Text({ text: '', style: { fontFamily: theme.fonts.sans, fontSize: 14 } }),
    b: new Text({ text: '', style: { fontFamily: theme.fonts.sans, fontSize: 14 } }),
  };
  names.b.anchor.set(1, 0);
  for (const side of SIDES) container.addChild(track[side], trail[side], hp[side], names[side]);

  let current = theme;
  let layout: HudLayout | null = null;

  const paint = (): void => {
    for (const side of SIDES) {
      track[side].tint = current.colors.text;
      track[side].alpha = TRACK_ALPHA;
      trail[side].tint = current.colors.hpLost;
      hp[side].tint = current.colors.hp;
      names[side].style.fill = current.colors.text;
    }
  };
  paint();

  const measure: MeasureText = (text, px) =>
    CanvasTextMetrics.measureText(
      text,
      new TextStyle({ fontFamily: current.fonts.sans, fontSize: px }),
    ).width;

  /** The bars drain towards the outer edges: the remaining HP sits next to the center. */
  const fill = (sprite: Sprite, side: Side, fraction: number): void => {
    if (layout === null) return;
    const rect = layout.bars[side];
    const width = rect.width * Math.max(0, Math.min(1, fraction));
    sprite.position.set(side === 'a' ? rect.x + rect.width - width : rect.x, rect.y);
    sprite.width = width;
    sprite.height = rect.height;
    sprite.visible = width > 0;
  };

  return {
    container,
    layout(viewport) {
      const next = layoutHud(viewport, measure, labels.names);
      layout = next;
      for (const side of SIDES) {
        fill(track[side], side, 1);
        const name = names[side];
        name.style.fontSize = next.fontPx;
        name.text = next.names[side].text;
        name.position.set(next.names[side].x, next.names[side].y);
      }
      return next;
    },
    apply(hud) {
      for (const side of SIDES) {
        const f = hud[side];
        fill(trail[side], side, f.trail / f.maxHp);
        fill(hp[side], side, f.hp / f.maxHp);
      }
    },
    setTheme(theme) {
      current = theme;
      paint();
    },
  };
}
