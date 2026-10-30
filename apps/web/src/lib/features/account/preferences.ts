import { deserialize } from '$app/forms';
import type { Locale } from '#lib/i18n/locale.js';
import type { ThemeChoice } from '#lib/theme/resolve-theme.js';

/**
 * Saves the theme in the profile through the `theme` action of `/<locale>/preferences` (008
 * RF-D4, plan-d decision 5). The theme switcher already applied it live and wrote the cookie;
 * `false` means the profile did not change (the next SSR still uses the cookie on this device).
 */
export async function saveThemePreference(locale: Locale, theme: ThemeChoice): Promise<boolean> {
  const body = new FormData();
  body.set('theme', theme);
  try {
    const response = await fetch(`/${locale}/preferences?/theme`, {
      method: 'POST',
      body,
      headers: { accept: 'application/json', 'x-sveltekit-action': 'true' },
    });
    return deserialize(await response.text()).type === 'success';
  } catch {
    return false;
  }
}
