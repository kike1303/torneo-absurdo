// The two fighters and their flash copies (plan-a2 decision 6). Only properties change per frame.
import { Container, Sprite } from 'pixi.js';
import type { FighterScene, FlashEffect, Scene } from '../scene/types.ts';
import { SIDES, type Side } from '../timeline/types.ts';
import { NEUTRAL_WHITE } from './neutral.ts';
import type { TextureSet } from './textures.ts';

export interface FighterLayer {
  readonly fighters: Container;
  readonly flashes: Container;
  apply(scene: Scene, textures: TextureSet, flashes: readonly FlashEffect[]): void;
}

export function createFighterLayer(): FighterLayer {
  const fighters = new Container();
  const flashes = new Container();
  const body: Record<Side, Sprite> = { a: new Sprite(), b: new Sprite() };
  const flash: Record<Side, Sprite> = { a: new Sprite(), b: new Sprite() };
  for (const side of SIDES) {
    fighters.addChild(body[side]);
    // An additive copy tinted white lightens the struck sprite (decision 7).
    flash[side].blendMode = 'add';
    flash[side].tint = NEUTRAL_WHITE;
    flash[side].visible = false;
    flashes.addChild(flash[side]);
  }

  const place = (sprite: Sprite, f: FighterScene, textures: TextureSet): boolean => {
    const set = textures.byAsset.get(f.asset);
    const texture = set?.frames[f.frame];
    const anchor = set?.anchors[f.frame];
    if (texture === undefined || anchor === undefined) {
      sprite.visible = false;
      return false;
    }
    if (sprite.texture !== texture) sprite.texture = texture;
    sprite.anchor.set(anchor.x, anchor.y);
    sprite.position.set(f.x, f.y);
    // `b` is drawn mirrored around its pivot (RF-A15).
    sprite.scale.set(f.mirror ? -f.scale : f.scale, f.scale);
    sprite.visible = true;
    return true;
  };

  return {
    fighters,
    flashes,
    apply(scene, textures, active) {
      for (const side of SIDES) {
        const f = scene.fighters[side];
        const shown = place(body[side], f, textures);
        let alpha = 0;
        for (const effect of active)
          if (effect.side === side) alpha = Math.max(alpha, effect.alpha);
        const copy = flash[side];
        if (shown && alpha > 0) {
          place(copy, f, textures);
          copy.alpha = alpha;
        } else {
          copy.visible = false;
        }
      }
    },
  };
}
