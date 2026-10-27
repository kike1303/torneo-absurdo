// Banners `fight` (end of the intro) and `ko` (from the KO on). The text comes from the host
// (`labels.banners`, RF-A11); here only codes and tween values. With reduced motion, no tween.
import type { PreparedScene } from './prepare.ts';
import type { BannerScene } from './types.ts';

/** `fight` shows during the last FIGHT_BANNER_MS of the intro. */
export const FIGHT_BANNER_MS = 1200;
export const BANNER_TWEEN_MS = 200;

function tweenIn(tau: number, reducedMotion: boolean): { alpha: number; scale: number } {
  if (reducedMotion || tau >= BANNER_TWEEN_MS) return { alpha: 1, scale: 1 };
  const p = Math.max(0, tau) / BANNER_TWEEN_MS;
  return { alpha: p, scale: 1.4 - 0.4 * p };
}

export function bannersAt(
  prepared: PreparedScene,
  t: number,
  reducedMotion: boolean,
): BannerScene[] {
  const out: BannerScene[] = [];
  const first = prepared.firstStrikeAt;
  if (first !== null && t < first) {
    const start = Math.max(0, first - FIGHT_BANNER_MS);
    if (t >= start) {
      const tween = tweenIn(t - start, reducedMotion);
      const left = first - t;
      const alpha =
        !reducedMotion && left < BANNER_TWEEN_MS
          ? Math.min(tween.alpha, left / BANNER_TWEEN_MS)
          : tween.alpha;
      out.push({ code: 'fight', alpha, scale: tween.scale });
    }
  }
  const ko = prepared.ko;
  if (ko !== null && t >= ko.at) {
    out.push({ code: 'ko', ...tweenIn(t - ko.at, reducedMotion) });
  }
  return out;
}
