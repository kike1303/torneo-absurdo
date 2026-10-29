// Pure scene (RF-A9): no Pixi and no DOM.
export * from './types.ts';
export {
  MAX_ACTION_MS,
  MAX_WINDUP_MS,
  RETURN_MS,
  frameAt,
  xAt,
  type AnimationTiming,
  type Hold,
  type Segment,
  type SegmentKind,
  type TimingTable,
} from './choreography.ts';
export {
  K,
  LOOKAHEAD_MS,
  LOOKBACK_MS,
  cueWindow,
  prepareScene,
  type HpStep,
  type ImpactKind,
  type PreparedCue,
  type PreparedFighter,
  type PreparedScene,
  type PreparedStrike,
} from './prepare.ts';
export {
  HUD_MIN_FONT_PX,
  TRAIL_DELAY_MS,
  TRAIL_EASE_MS,
  fitText,
  layoutHud,
  type HudLayout,
  type HudNameLayout,
  type MeasureText,
  type Rect,
  type Viewport,
} from './hud.ts';
export { BANNER_TWEEN_MS, FIGHT_BANNER_MS } from './banners.ts';
export { FIXED_CAMERA } from './camera.ts';
export { sceneAt, sceneAtTimeline } from './scene-at.ts';
export { letterbox, logicalFontPx, type Letterbox } from './viewport.ts';
