// The module apps/web/vite/fighter-lore.ts generates at build time (010 RF56).
declare module 'virtual:fighter-lore' {
  const lore: Readonly<
    Record<
      string,
      Readonly<Record<'en' | 'es', { readonly name: string; readonly epithet: string }>>
    >
  >;
  export default lore;
}
