// Banners `fight` and `ko` (RF-A13): host text, theme fonts, outside the camera.
import { Container, Text } from 'pixi.js';
import type { RendererLabels, RendererTheme } from '../player/types.ts';
import type { HudLayout } from '../scene/hud.ts';
import type { BannerCode, BannerScene } from '../scene/types.ts';

const CODES: readonly BannerCode[] = ['fight', 'ko'];

export interface BannerLayer {
  readonly container: Container;
  layout(layout: HudLayout): void;
  apply(banners: readonly BannerScene[]): void;
  setTheme(theme: RendererTheme): void;
}

export function createBannerLayer(theme: RendererTheme, labels: RendererLabels): BannerLayer {
  const container = new Container();
  const texts = {} as Record<BannerCode, Text>;
  for (const code of CODES) {
    const text = new Text({
      text: labels.banners[code],
      style: { fontFamily: theme.fonts.display, fontSize: 28, fontWeight: '800' },
    });
    text.anchor.set(0.5);
    text.visible = false;
    container.addChild(text);
    texts[code] = text;
  }
  const paint = (current: RendererTheme): void => {
    texts.fight.style.fill = current.colors.text;
    texts.ko.style.fill = current.colors.crit;
  };
  paint(theme);

  return {
    container,
    layout(hud) {
      for (const code of CODES) {
        texts[code].style.fontSize = hud.banner.fontPx;
        texts[code].position.set(hud.banner.x, hud.banner.y);
      }
    },
    apply(banners) {
      for (const code of CODES) {
        const banner = banners.find((b) => b.code === code);
        const text = texts[code];
        if (banner === undefined || banner.alpha <= 0) {
          text.visible = false;
          continue;
        }
        text.alpha = banner.alpha;
        text.scale.set(banner.scale);
        text.visible = true;
      }
    },
    setTheme: paint,
  };
}
