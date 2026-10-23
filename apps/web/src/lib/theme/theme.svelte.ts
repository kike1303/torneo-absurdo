import {
  parseTheme,
  serializeThemeCookie,
  type ResolvedTheme,
  type ThemeChoice,
} from './resolve-theme.js';
import { THEME_COLOR } from './theme-color.js';

const DARK_QUERY = '(prefers-color-scheme: dark)';

let choice = $state<ThemeChoice>('dark');
let systemDark = $state(true);
let listening = false;

function onSystemChange(event: MediaQueryListEvent): void {
  systemDark = event.matches;
  if (choice === 'system') apply();
}

/** Theme state (RF-A8, ADR-0009 D3). `choice` is what the user picked; `resolved` is painted. */
export const theme = {
  get choice(): ThemeChoice {
    return choice;
  },
  get resolved(): ResolvedTheme {
    if (choice === 'system') return systemDark ? 'dark' : 'light';
    return choice;
  },
};

/** Syncs the runa with the theme the SSR used. Call once in the root layout (browser only). */
export function initTheme(initial: ThemeChoice): void {
  choice = parseTheme(initial) ?? 'dark';
  const query = matchMedia(DARK_QUERY);
  systemDark = query.matches;
  if (!listening) {
    query.addEventListener('change', onSystemChange);
    listening = true;
  }
}

/** Changes the theme live, without reloading, and writes the `theme` cookie. */
export function setTheme(next: ThemeChoice): void {
  const parsed = parseTheme(next);
  if (!parsed) return;
  choice = parsed;
  document.cookie = serializeThemeCookie(parsed, location.protocol);
  apply();
}

function apply(): void {
  const root = document.documentElement;
  root.dataset.theme = theme.resolved;
  const head = document.head;
  head.querySelectorAll('meta[name="theme-color"]').forEach((meta) => meta.remove());
  let scheme = head.querySelector<HTMLMetaElement>('meta[name="color-scheme"]');
  if (!scheme) {
    scheme = document.createElement('meta');
    scheme.name = 'color-scheme';
    head.prepend(scheme);
  }
  if (choice === 'system') {
    scheme.content = 'light dark';
    for (const mode of ['light', 'dark'] as const) {
      const meta = document.createElement('meta');
      meta.name = 'theme-color';
      meta.media = `(prefers-color-scheme: ${mode})`;
      meta.content = THEME_COLOR[mode];
      scheme.after(meta);
    }
  } else {
    scheme.content = choice;
    const meta = document.createElement('meta');
    meta.name = 'theme-color';
    meta.content = THEME_COLOR[choice];
    scheme.after(meta);
  }
}
