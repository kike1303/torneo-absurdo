// sRGB hex of `--background` per theme, for `<meta name="theme-color">` (plan-a decision 9).
// The only `.ts` exempt from check-tokens (ADR-0009 D1); test 3 checks it against tokens.css.
export const THEME_COLOR = {
  light: '#ffffff',
  dark: '#0a0a0a',
} as const;
