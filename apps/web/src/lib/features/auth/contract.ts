import type { Locale as ProtocolLocale, Theme } from '@torneo/protocol';
import type { Locale as ParaglideLocale } from '#lib/paraglide/runtime.js';
import type { ThemeChoice } from '#lib/theme/resolve-theme.js';

// Type assertions in both directions (008 plan-d decision 6, notes of #54): the `Locale` and
// `Theme` of the protocol are exactly the web's. A value added on one side only fails
// `svelte-check`. No runtime code.

type Equal<A, B> = [A] extends [B] ? ([B] extends [A] ? true : false) : false;
type Assert<T extends true> = T;

export type LocalesMatch = Assert<Equal<ProtocolLocale, ParaglideLocale>>;
export type ThemesMatch = Assert<Equal<Theme, ThemeChoice>>;
