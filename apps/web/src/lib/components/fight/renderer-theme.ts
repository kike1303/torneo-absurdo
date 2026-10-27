// RendererTheme from the CSS tokens (ADR-0009 D7): the `--arena-*` colors in OKLCH, converted
// to sRGB with culori/fn and clamped to its gamut, as 0xRRGGBB because Pixi needs that.
import type { RendererTheme } from '@torneo/renderer';
import { clampRgb, modeOklch, modeRgb, useMode } from 'culori/fn';

useMode(modeOklch);
const toRgb = useMode(modeRgb);

/** Reads a CSS custom property, already resolved (`getComputedStyle(...).getPropertyValue`). */
export type ReadToken = (name: string) => string;

const FALLBACK_FONT = 'sans-serif';

export function tokenToHex(value: string): number {
  const rgb = toRgb(value.trim());
  if (rgb === undefined) throw new Error(`renderer_theme.invalid_color`);
  const { r, g, b } = clampRgb(rgb);
  const byte = (c: number) => Math.max(0, Math.min(255, Math.round(c * 255)));
  return (byte(r) << 16) | (byte(g) << 8) | byte(b);
}

export function buildRendererTheme(read: ReadToken): RendererTheme {
  const color = (name: string) => tokenToHex(read(name));
  const font = (name: string) => read(name).trim() || FALLBACK_FONT;
  return {
    colors: {
      bg: color('--arena-bg'),
      hp: color('--arena-hp'),
      hpLost: color('--arena-hp-lost'),
      crit: color('--arena-crit'),
      text: color('--arena-text'),
    },
    fonts: { sans: font('--font-sans'), display: font('--font-display') },
  };
}

/** The theme painted now, read from `<html>` (browser only). */
export function currentRendererTheme(): RendererTheme {
  const style = getComputedStyle(document.documentElement);
  return buildRendererTheme((name) => style.getPropertyValue(name));
}
