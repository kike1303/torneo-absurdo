import { THEME_COLOR } from './theme-color.js';

export const THEME_CHOICES = ['dark', 'light', 'system'] as const;
export type ThemeChoice = (typeof THEME_CHOICES)[number];
export type ResolvedTheme = 'dark' | 'light';

export const THEME_COOKIE = 'theme';
export const THEME_COOKIE_MAX_AGE = 31_536_000;
/** ADR-0009 D3: without profile or cookie, dark. */
export const DEFAULT_THEME: ThemeChoice = 'dark';

/** Only `dark | light | system`; any other value is treated as absent (RF-A6). */
export function parseTheme(value: unknown): ThemeChoice | undefined {
  return typeof value === 'string' && (THEME_CHOICES as readonly string[]).includes(value)
    ? (value as ThemeChoice)
    : undefined;
}

export interface ResolveThemeInput {
  /** The profile's preference with a session (008 RF-D4); `null` or absent defers to the cookie. */
  profile?: ThemeChoice | null;
  cookie: string | undefined;
  /** `?theme=` value; only `dark | light`, and only on `/dev/**` with the catalog on (RF-A9). */
  override: string | null;
  enableDevCatalog: string | undefined;
  routeId: string | null;
}

/** ADR-0009 D3, 008 RF-D4: catalog override → profile (with a session) → cookie → `dark`. */
export function resolveTheme(input: ResolveThemeInput): ThemeChoice {
  if (input.enableDevCatalog === '1' && input.routeId?.startsWith('/dev/')) {
    if (input.override === 'dark' || input.override === 'light') return input.override;
  }
  return parseTheme(input.profile) ?? parseTheme(input.cookie) ?? DEFAULT_THEME;
}

/** `<meta>` tags for `%theme.meta%` (ADR-0009 D3). Values come from a closed list only. */
export function themeMeta(choice: ThemeChoice): string {
  if (choice === 'system') {
    return [
      '<meta name="color-scheme" content="light dark" />',
      `<meta name="theme-color" media="(prefers-color-scheme: light)" content="${THEME_COLOR.light}" />`,
      `<meta name="theme-color" media="(prefers-color-scheme: dark)" content="${THEME_COLOR.dark}" />`,
    ].join('\n    ');
  }
  return [
    `<meta name="color-scheme" content="${choice}" />`,
    `<meta name="theme-color" content="${THEME_COLOR[choice]}" />`,
  ].join('\n    ');
}

export function serializeThemeCookie(choice: ThemeChoice, protocol: string): string {
  const secure = protocol === 'https:' ? '; Secure' : '';
  return `${THEME_COOKIE}=${choice}; Path=/; Max-Age=${THEME_COOKIE_MAX_AGE}; SameSite=Lax${secure}`;
}
