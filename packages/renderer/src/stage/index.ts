// The Pixi layer (plan-a2 decisions 5 and 6): translates a `Scene` to Pixi objects. It keeps no
// state between frames other than its reusable objects.
import { Container, Graphics, type Application, type Ticker } from 'pixi.js';
import type { SpriteManifest } from '../manifest/types.ts';
import type { RendererTheme, Stage, StageSetup } from '../player/types.ts';
import type { HudLayout } from '../scene/hud.ts';
import {
  GROUND_Y,
  STAGE_WIDTH,
  type DamageNumberEffect,
  type FlashEffect,
  type ParticleEffect,
  type Scene,
} from '../scene/types.ts';
import { letterbox, type Letterbox } from '../scene/viewport.ts';
import { createApp } from './app.ts';
import { createBannerLayer, type BannerLayer } from './banners.ts';
import { createEffectLayer, type EffectLayer } from './effects.ts';
import { createFighterLayer, type FighterLayer } from './fighters.ts';
import { loadFonts } from './fonts.ts';
import { createHudLayer, type HudLayer } from './hud.ts';
import {
  EMPTY_TEXTURES,
  disableWorkers,
  loadTextures,
  unloadTextures,
  type TextureSet,
} from './textures.ts';

/** Alpha of the ground line, in the text color of the theme. */
const GROUND_ALPHA = 0.25;

interface Layers {
  readonly app: Application;
  readonly world: Container;
  readonly ground: Graphics;
  readonly fighters: FighterLayer;
  readonly effects: EffectLayer;
  readonly hud: HudLayer;
  readonly banners: BannerLayer;
}

export function createPixiStage(container: HTMLElement): Stage {
  let layers: Layers | null = null;
  let theme: RendererTheme | null = null;
  let textures: TextureSet = EMPTY_TEXTURES;
  let box: Letterbox = letterbox({ width: 0, height: 0 });
  let hudLayout: HudLayout | null = null;
  let lastScene: Scene | null = null;
  let observer: ResizeObserver | null = null;
  let destroyed = false;
  let onTick: ((ticker: Ticker) => void) | null = null;

  // Scratch lists reused every frame: no allocation per frame for the split of the effects.
  const particles: ParticleEffect[] = [];
  const numbers: DamageNumberEffect[] = [];
  const flashes: FlashEffect[] = [];
  let formatNumber: (n: number) => string = String;

  const paintGround = (l: Layers, current: RendererTheme): void => {
    l.ground
      .clear()
      .rect(0, GROUND_Y, STAGE_WIDTH, 2)
      .fill({ color: current.colors.text, alpha: GROUND_ALPHA });
  };

  const resize = (): void => {
    const l = layers;
    if (l === null) return;
    const width = Math.max(1, container.clientWidth);
    const height = Math.max(1, container.clientHeight);
    l.app.renderer.resize(width, height);
    box = letterbox({ width, height });
    hudLayout = l.hud.layout({ width, height });
    l.banners.layout(hudLayout);
    l.effects.resize(box.scale, l.app.renderer.resolution);
    if (lastScene !== null) draw(l, lastScene);
  };

  function draw(l: Layers, scene: Scene): void {
    const current = theme;
    if (current === null) return;
    const camera = scene.camera;
    // Logical p → CSS px: box + box.scale × (center + offset + zoom × (p − center)).
    l.world.pivot.set(camera.centerX, camera.centerY);
    l.world.position.set(
      box.x + box.scale * (camera.centerX + camera.offsetX),
      box.y + box.scale * (camera.centerY + camera.offsetY),
    );
    l.world.scale.set(box.scale * camera.zoom);

    particles.length = 0;
    numbers.length = 0;
    flashes.length = 0;
    for (const effect of scene.effects) {
      if (effect.kind === 'particle') particles.push(effect);
      else if (effect.kind === 'damage') numbers.push(effect);
      else flashes.push(effect);
    }
    l.fighters.apply(scene, textures, flashes);
    l.effects.apply(particles, numbers, current, formatNumber);
    l.hud.apply(scene.hud);
    l.banners.apply(scene.banners);
  }

  return {
    async init(setup: StageSetup) {
      theme = setup.theme;
      formatNumber = setup.formatNumber;
      disableWorkers();
      const app = await createApp(container.clientWidth, container.clientHeight, setup.theme);
      if (destroyed) {
        app.destroy(true, { children: true });
        return;
      }
      // No Text exists before the theme fonts load (ADR-0009 D7).
      await loadFonts(setup.theme);
      if (destroyed) {
        app.destroy(true, { children: true });
        return;
      }
      const world = new Container();
      const ground = new Graphics();
      const fighters = createFighterLayer();
      const effects = createEffectLayer(setup.theme);
      world.addChild(
        ground,
        fighters.fighters,
        fighters.flashes,
        effects.particles,
        effects.numbers,
      );
      const hud = createHudLayer(setup.theme, setup.labels);
      const banners = createBannerLayer(setup.theme, setup.labels);
      app.stage.addChild(world, hud.container, banners.container);
      layers = { app, world, ground, fighters, effects, hud, banners };
      paintGround(layers, setup.theme);

      container.appendChild(app.canvas);
      app.canvas.style.display = 'block';
      resize();
      observer = new ResizeObserver(resize);
      observer.observe(container);
      onTick = (ticker) => setup.onFrame(ticker.elapsedMS);
      app.ticker.add(onTick);
    },
    async loadAssets(manifests: ReadonlyMap<string, SpriteManifest>, resolveAsset) {
      const next = await loadTextures(manifests, resolveAsset);
      if (destroyed) {
        await unloadTextures(next);
        return;
      }
      const previous = textures;
      textures = next;
      lastScene = null;
      await unloadTextures(previous);
    },
    apply(scene) {
      lastScene = scene;
      if (layers !== null) draw(layers, scene);
    },
    setTheme(next) {
      theme = next;
      const l = layers;
      if (l === null) return;
      l.app.renderer.background.color = next.colors.bg;
      paintGround(l, next);
      l.hud.setTheme(next);
      l.banners.setTheme(next);
      if (lastScene !== null) draw(l, lastScene);
    },
    destroy() {
      if (destroyed) return;
      destroyed = true;
      observer?.disconnect();
      const l = layers;
      layers = null;
      if (l !== null) {
        if (onTick !== null) l.app.ticker.remove(onTick);
        // Children only: the shared `Texture.WHITE` and the Assets cache are not ours to destroy
        // here; the sheets are unloaded below.
        l.app.destroy(true, { children: true });
      }
      void unloadTextures(textures);
      textures = EMPTY_TEXTURES;
    },
  };
}
