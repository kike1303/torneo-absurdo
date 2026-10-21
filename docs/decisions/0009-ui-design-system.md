---
id: "0009"
title: Sistema de diseño, theme y componentes
type: adr
status: in-review
risk: low
author: architect
reviewers: [frontend-engineer, owner]
approvals: []
supersedes: null
linear: TA-5
depends_on: ["0003", "0007"]
created: 2026-10-20
updated: 2026-10-20
---

# ADR-0009: Sistema de diseño, theme y componentes

## Contexto
- La web es SvelteKit 5 con SSR (adapter-node) y el renderer es PixiJS, agnóstico de framework, en `packages/renderer` (ADR-0003). Cero strings literales y Paraglide con rutas `/es` y `/en` (ADR-0007).
- La feature 001 necesita cerradas las bases: tokens, tema sin parpadeo, primitivas, fronteras entre carpetas y cómo se verifica la accesibilidad. La identidad visual (paleta final, tipografías, logo) todavía no existe.
- Uso principal en el celular; presupuesto cero (nada de CDNs ni servicios de pago).

## Decisión

### D1. Tailwind v4, CSS-first
- `tailwindcss` v4 con `@tailwindcss/vite`. **No existe `tailwind.config.*`.**
- `src/app.css`: `@import "tailwindcss"; @import "tw-animate-css"; @import "./lib/theme/tokens.css";`.
- `lib/theme/tokens.css` define los tokens en dos niveles, en **OKLCH**:
  1. **Primitivas** (`--palette-<tono>-<50…950>`): solo se usan dentro de `tokens.css`.
  2. **Semánticos** de shadcn (`--background`, `--foreground`, `--card`, `--card-foreground`, `--popover`, `--popover-foreground`, `--primary`, `--primary-foreground`, `--secondary`, `--secondary-foreground`, `--muted`, `--muted-foreground`, `--accent`, `--accent-foreground`, `--destructive`, `--destructive-foreground`, `--border`, `--input`, `--ring`, `--radius`) más los de la arena para el renderer (`--arena-bg`, `--arena-hp`, `--arena-hp-lost`, `--arena-crit`, `--arena-text`). Se definen en `:root` (claro) y en `[data-theme="dark"]`.
  - Se exponen con `@theme inline { --color-background: var(--background); … }`.
- Variante oscura: `@custom-variant dark (&:where([data-theme="dark"], [data-theme="dark"] *));` (el mismo atributo que pone el SSR; D3).
- Tokens nuevos en `@theme`: `--font-sans`, `--font-display`; `--duration-fast: 120ms`, `--duration-base: 200ms`, `--duration-slow: 320ms`; `--ease-out-arena`; `--shadow-card`, `--shadow-glow`; `--radius-sm|md|lg` derivados de `--radius`.
- Se heredan de Tailwind sin cambios: la escala de espacio (`--spacing`), los breakpoints, la escala tipográfica y `z-index`.
- **"Nada de colores sueltos" se verifica con `scripts/check-tokens.mjs`** (corre en `just check` y en CI) sobre `src/lib/components/**` y `src/routes/**`: falla con valores arbitrarios de color (`-[#`, `-[rgb`, `-[oklch`, `-[hsl`), con clases de paleta cruda (`(bg|text|border|ring|fill|stroke|from|to|via)-(red|blue|…|zinc)-\d+`) y con `--palette-*` fuera de `tokens.css`.

### D2. shadcn-svelte y Bits UI
- shadcn-svelte **1.x** (CLI `shadcn-svelte@1`), con la versión de `bits-ui` (≥1) que pida ese CLI; `tw-animate-css` en lugar de `tailwindcss-animate`. Versiones exactas en `package.json` (sin `^`).
- `components.json` con los alias `components: $lib/components`, `ui: $lib/components/ui`, `utils: $lib/utils`, `hooks: $lib/hooks`. `cn()` en `lib/utils.ts` con `clsx` y `tailwind-merge`.
- **Actualizar una primitiva**: en una rama, `shadcn-svelte@1 add <componente> --overwrite`, revisar el diff y reaplicar nuestros cambios. Cada archivo modificado lleva un comentario de cabecera `<!-- MODIFICADO: <qué y por qué> -->`, que se conserva.

### D3. Tema con SSR y sin parpadeo
- Valores: `dark | light | system`. **Precedencia: preferencia del perfil (con sesión) → cookie `theme` → `dark`.** `system` solo aplica si el usuario lo elige; así "dark por defecto" no choca con `prefers-color-scheme`, que el navegador siempre reporta.
- Con sesión, la preferencia se guarda en el perfil (`users.theme`), igual que el idioma (ADR-0007). Al iniciar sesión, el valor del perfil reescribe la cookie.
- Cookie `theme`: `Path=/; Max-Age=31536000; SameSite=Lax`, sin `HttpOnly` (la lee el script del tema).
- `hooks.server.ts` resuelve el valor y, con `transformPageChunk`, reemplaza `%theme%` en `app.html`: `<html data-theme="%theme%">`, `<meta name="color-scheme">` y `<meta name="theme-color">`.
- Para `system`, el SSR emite `data-theme="system"` y un **script inline bloqueante** en `<head>` (≤ 1 KB, `nonce="%sveltekit.nonce%"`) que resuelve con `matchMedia('(prefers-color-scheme: dark)')` antes del primer pintado y escucha sus cambios. Si se activa `kit.csp`, se usa `mode: 'auto'`, que le da nonce a ese script.
- **No se usa `mode-watcher`**: usa `localStorage` y su propio script, que compiten con la cookie del SSR. En su lugar, `lib/theme/theme.svelte.ts` (runa con el valor elegido y el resuelto; escribe la cookie y, con sesión, el perfil).

### D4. Organización y fronteras
- `lib/components/ui/`: primitivas, sin lógica de negocio.
- `lib/components/<dominio>/` (`fight/`, `betting/`, `leaderboard/`, `profile/`): componen primitivas y reciben datos por props.
- `lib/features/<dominio>/`: estado y llamadas al API. Las rutas (`+page.svelte`, `+page.ts`) conectan features con componentes.
- `lib/components/fight/FightStage.svelte` es el **único** archivo que importa `packages/renderer`.
- Reglas (ESLint `no-restricted-imports` y `no-restricted-globals` por glob en la config flat):
  - `ui/` no importa de `$lib/features/**`, `$lib/components/<dominio>/**` ni de los mensajes de Paraglide (salvo `ui/_a11y-messages.ts`, ver D8).
  - `components/<dominio>/` no importa valores de `$lib/features/**` (sí `import type`) y no usa `fetch`.
  - Nadie fuera de `FightStage.svelte` importa el renderer.

### D5. Catálogo `/dev/components`
- Carpeta `src/routes/dev/components/`. Paraglide reescribe la URL con el prefijo (hook `reroute`), así que se sirve en `/es/dev/components` y `/en/dev/components`, como cualquier otra ruta.
- Guard en su `+layout.server.ts`: 404 salvo que `ENABLE_DEV_CATALOG === '1'` (`$env/dynamic/private`). La variable está en el `.env` de desarrollo y en el job e2e de CI, y **nunca** en el compose de producción. Elegí esto en vez de `dev` de `$app/environment` porque Playwright corre contra el build (`vite preview`), donde `dev` es `false`.
- Muestra cada componente en cada variante y estado (default, hover/focus, disabled, error), dentro de dos paneles lado a lado con `data-theme="light"` y `data-theme="dark"`. El idioma es el de la ruta; no se combinan idiomas en una misma vista.
- Los textos de demo del catálogo están exentos del lint de i18n (ignorados por glob en `src/routes/dev/**`).
- Es el objetivo de axe y de las capturas de Playwright a 375×812 y 1280×800 (D6).

### D6. Mobile-first y accesibilidad
- Se diseña a 375 px y se escala con los breakpoints de Tailwind.
- Objetivo: **WCAG 2.2 AA**. Además, como objetivo propio, objetivos táctiles de **≥ 44×44 px** (equivale a 2.5.5 AAA; AA solo pide 24 px en 2.5.8). Única excepción: enlaces dentro de un párrafo, con mínimo de 24 px.
- Foco: todo interactivo usa `focus-visible:ring-2 ring-ring ring-offset-2 ring-offset-background`. Se prohíbe `outline-none` sin un anillo de reemplazo (lo revisa `check-tokens.mjs`).
- Movimiento: animaciones decorativas solo con `motion-safe:`. En `tokens.css`, `@media (prefers-reduced-motion: reduce)` pone todos los `--duration-*` en `0ms`.
- Destellos: nada parpadea más de 3 veces por segundo, con o sin reduced motion (WCAG 2.3.1).
- Tolerancia de texto: los layouts aguantan textos ~30 % más largos (el español); se usan propiedades lógicas (`ms-`, `me-`, `ps-`, `pe-`, `text-start`) para no cerrar la puerta a RTL.

### D7. Contrato del renderer
- `packages/renderer` exporta:
  - `type RendererTheme = { colors: { bg, hp, hpLost, crit, text: number }, fonts: { sans: string, display: string } }` (colores como `0xRRGGBB`, porque Pixi no entiende OKLCH).
  - `type RendererOptions = { theme: RendererTheme; reducedMotion: boolean }` y los métodos `setTheme(theme)` y `setReducedMotion(flag)`, aplicables en caliente.
- **Fuente única: los tokens CSS.** `lib/components/fight/renderer-theme.ts` lee los `--arena-*` con `getComputedStyle(document.documentElement)` y los convierte a sRGB con `culori` (solo `converter('rgb')`, importado por función). Lo descarté al revés (TS que genere el CSS) porque agrega un paso de build y deja dos formatos de verdad.
- `FightStage.svelte` vuelve a construir el tema cuando cambia el tema resuelto (D3) y llama a `setReducedMotion` cuando cambia `matchMedia('(prefers-reduced-motion: reduce)')`.
- **Con `reducedMotion: true`**: sin screen shake, sin flashes de pantalla completa, 0 partículas, tweens de ≤ 150 ms o instantáneos; el impacto se comunica con color y texto estáticos.
- **Regla dura**: reduced motion **nunca** cambia la temporización del `FightLog`; la pelea sigue sincronizada con el reloj del servidor y con los demás espectadores.
- El renderer no dibuja texto hasta que carga la fuente (`document.fonts.load()` de las familias del tema, o sus bitmap fonts).

### D8. Iconos, fuentes e i18n en componentes
- **Iconos**: `@lucide/svelte` con imports por icono (`@lucide/svelte/icons/<nombre>`). Los decorativos llevan `aria-hidden="true"`; los botones de solo icono exigen un `aria-label` de Paraglide (prop obligatoria).
- **Fuentes**: autoalojadas con `@fontsource-variable/*` (o woff2 en `static/fonts/`), sin CDN de terceros; `font-display: swap`; `preload` solo de la fuente principal; subset latin (cubre á, ñ, ¿, ¡); fallback local con `size-adjust` y `ascent-override` para evitar CLS; tokens `--font-sans` y `--font-display`. Las familias concretas llegan con la identidad visual; mientras tanto, la pila del sistema.
- **i18n**: las primitivas reciben todo texto visible por props o snippets. Los textos internos que shadcn trae en inglés ("Close", "Previous", "Next", `sr-only`) salen de `ui/_a11y-messages.ts`, que usa Paraglide, o pasan a ser props obligatorias. `lang` en `<html>` lo pone el hook de Paraglide. Los formatos van por helpers compartidos en `lib/i18n/format.ts` (`formatCoins`, `formatDate`, `formatRelative`) con `Intl`; los componentes no formatean por su cuenta.

## Alternativas descartadas
- **Tailwind v3 con `tailwind.config.js`**: v4 es el default actual de shadcn-svelte y deja los tokens en CSS.
- **Skeleton**: trae su propio sistema de temas y opina mucho del look; chocaría con la identidad propia.
- **DaisyUI**: clases semánticas cómodas, pero sin primitivas accesibles (dialog, combobox) al nivel de Bits UI.
- **Melt UI o Bits UI directo, sin shadcn**: más trabajo para llegar al mismo punto; shadcn solo copia código encima de Bits UI.
- **Storybook o Histoire desde el día uno**: otra toolchain y más minutos de CI; `/dev/components` cubre catálogo, axe y capturas.
- **`mode-watcher`**: ver D3.
- **Tokens en TS que generen CSS**: ver D7.

## Consecuencias
- Más fácil: cambiar la identidad visual es cambiar valores en `tokens.css`; el renderer la hereda sin tocar código.
- Más difícil: las primitivas copiadas se actualizan a mano (D2); el script del tema es código propio que hay que mantener.
- Vigilar: el tamaño de `culori` en el bundle de la página de pelea, y que la lista de clases de paleta de `check-tokens.mjs` siga a Tailwind.

**Números fijados:** 375 px de diseño base · capturas a 375×812 y 1280×800 · táctil ≥ 44 px (24 px en enlaces dentro de texto) · contraste ≥ 4.5:1 para texto y ≥ 3:1 para `--ring` y `--border` contra el fondo · duraciones 120/200/320 ms y 0 ms con reduced motion · ≤ 3 destellos/s · tweens del renderer ≤ 150 ms con reduced motion · script del tema ≤ 1 KB · cookie `theme` de 1 año.

**Tests mínimos (CI):**
1. `check-tokens.mjs` falla con un color arbitrario, una clase de paleta cruda o `outline-none` sin anillo (fixtures).
2. ESLint falla con las importaciones prohibidas de D4 (fixtures).
3. **Contraste** (Vitest): parsea `tokens.css` y verifica en ambos temas los pares `foreground/background`, `card-foreground/card`, `popover-foreground/popover`, `primary-foreground/primary`, `secondary-foreground/secondary`, `muted-foreground/background`, `accent-foreground/accent`, `destructive-foreground/destructive` (≥ 4.5:1), y `ring/background`, `border/background` (≥ 3:1).
4. **axe** (`@axe-core/playwright`): 0 violaciones `serious` o `critical` en `/es/dev/components` y en las páginas clave existentes, en ambos temas.
5. **Sin parpadeo**: con cookie `theme=dark`, `theme=light` y `theme=system` + `colorScheme` emulado, el HTML del SSR (o el estado antes del primer pintado) ya trae el `data-theme` resuelto correcto.
6. **Catálogo fuera de producción**: el build sin `ENABLE_DEV_CATALOG` responde 404 en `/es/dev/components`.
7. **Renderer**: con `reducedMotion: true`, los eventos del `FightLog` se emiten en los mismos tiempos que con `false` (test unitario en `packages/renderer`).
8. Capturas de Playwright del catálogo a 375×812 y 1280×800, en ambos temas.

## Pendiente (owner)
- **OK del owner** para aceptar el ADR.
- **Identidad visual**: no es la feature 004 (esa es el pipeline de assets). Será una **feature nueva**, cuyo intent escribe product-owner, junto con el estilo de los personajes; el owner decide su prioridad. Mientras tanto, 001 entrega una **paleta neutra provisional** con todos los tokens semánticos y `--arena-*` de D1, para que cambiar de identidad sea solo cambiar valores.
- Las familias tipográficas que se elijan deben tener licencia libre (OFL o similar) para autoalojarlas sin costo.

## Revisión
### frontend-engineer — 2026-10-20 — REQUEST_CHANGES
- Verifiqué: frontmatter contra el esquema de `docs/process.md`; consistencia con ADR-0003 (SvelteKit SSR con adapter-node, PixiJS agnóstico en `packages/renderer`) y ADR-0007 (Paraglide, rutas `/es` y `/en`, cero strings literales, `Intl`); que la organización de carpetas coincida con mi definición de agente y con el intent 001 (tema que persiste, `/dev/components`) y el intent 002 (`prefers-reduced-motion` en el renderer); y la viabilidad técnica de Tailwind v4, shadcn-svelte, Bits UI y Svelte 5. No contradice ningún ADR aprobado. La dirección es correcta, pero como ADR todavía no se puede verificar: deja abiertas decisiones que la feature 001 necesita cerradas.
- Cambios pedidos:
  1. **Frontmatter y forma.** Falta `approvals: []` (y `linear` y `depends_on: ["0003", "0007"]`, como en el esquema). Renombra `## Propuesta` a `## Decisión` y agrega `## Consecuencias` y `## Alternativas descartadas` (por ejemplo Skeleton, DaisyUI, Melt UI directo, Storybook o Histoire desde el día uno), igual que los demás ADRs.
  2. **Tailwind v4 CSS-first.** Hay que dejar explícito que no se usa `tailwind.config.*`: se usa `@tailwindcss/vite`, los tokens se declaran en CSS (`lib/theme/tokens.css`, importado desde `app.css`) en dos niveles. El primero son las primitivas (paleta, escala). El segundo son los semánticos de shadcn (`--background`, `--foreground`, `--primary`, `--muted`, `--destructive`, `--ring`, `--radius`…), definidos en `:root` y en el selector del tema oscuro y expuestos con `@theme inline { --color-background: var(--background); … }`. Hay que definir la variante oscura con `@custom-variant dark (…)` sobre la misma clase o atributo que pone el SSR, y fijar el formato de color (OKLCH, el default de shadcn para v4). Además, las categorías "espacio" y "movimiento" deben decir qué tokens nuevos se agregan (`--ease-*`, `--duration-*`, `--font-*`, `--shadow-*`) y cuáles se heredan de Tailwind. Así "nada de colores sueltos" se vuelve verificable (por ejemplo con una regla de lint o de revisión que prohíba valores arbitrarios `[#…]` y clases de paleta cruda en los componentes).
  3. **shadcn-svelte y Bits UI.** Hay que fijar las versiones compatibles con Svelte 5 y Tailwind v4 (shadcn-svelte 1.x con Bits UI 1.x o superior, `tw-animate-css` en lugar de `tailwindcss-animate`) y el `components.json` con los alias `$lib/components/ui` y `$lib/utils` (`cn` con `clsx` y `tailwind-merge`). También cómo se actualizan las primitivas copiadas: se vuelve a correr el CLI y se revisa el diff, y nuestras modificaciones quedan documentadas en el propio archivo.
  4. **Tema con SSR sin parpadeo.** Hay que especificar el mecanismo. Propuesta: cookie `theme=dark|light|system`, que `hooks.server.ts` lee para inyectar la clase o `data-theme` en `<html>` con `transformPageChunk`, junto con `color-scheme` y `<meta name="theme-color">`. Para `system`, el servidor no conoce la preferencia (el client hint `Sec-CH-Prefers-Color-Scheme` solo funciona en Chromium), así que hace falta un script inline bloqueante en `<head>` de `app.html`. Si se activa CSP en SvelteKit (`kit.csp`), ese script necesita hash o nonce. También hay que decidir si se usa `mode-watcher` (el que trae shadcn-svelte) o uno propio coordinado con la cookie. Además hay una ambigüedad que resolver: "dark por defecto" y "respeta `prefers-color-scheme`" chocan, porque el navegador siempre reporta una preferencia (light por omisión). Hay que definir la precedencia (por ejemplo, elección del usuario, después `dark`, con `system` como opción explícita, o elección del usuario y después sistema) y si la preferencia se guarda en el perfil con sesión, como el idioma en ADR-0007.
  5. **Fronteras entre carpetas.** Hay que agregar las reglas de dependencia: `ui/` no importa de `features/` ni de componentes de dominio, ni usa mensajes de Paraglide salvo las etiquetas de accesibilidad internas (punto 9). Los componentes de dominio no hacen fetch: reciben datos de `lib/features/<dominio>/`. Y hay que fijar dónde vive el wrapper Svelte del renderer (propongo `lib/components/fight/FightStage.svelte`). Si es viable, que se haga cumplir con `no-restricted-imports` de ESLint.
  6. **`/dev/components`.** Hay que precisar cómo queda fuera de producción: un guard en `+layout.server.ts` con `dev` de `$app/environment` que responde 404, o excluir la ruta del build. También hay que precisar cómo convive con el prefijo de idioma (`/es/dev/components`), si muestra cada componente en tema × idioma × variante, y si sus textos de demo están exentos del lint de i18n. Propongo usarla como objetivo de Playwright más axe y de capturas en 375 px y desktop.
  7. **Mobile-first y accesibilidad verificables.** Hay que decir "WCAG 2.2 AA" y aclarar que 44 px es más estricto que AA (2.5.8 pide 24 px; 44 px corresponde a 2.5.5 AAA), lo cual está bien como objetivo propio. Hay que definir cómo se mide: `@axe-core/playwright` en `/dev/components` y en las páginas clave, en ambos temas, además de una prueba automática de contraste de los pares de tokens semánticos (foreground sobre background, en ambos temas). También hay que exigir un token de anillo de foco (`--ring`) con `focus-visible` y las variantes `motion-safe:` y `motion-reduce:`, o duraciones a 0 bajo `prefers-reduced-motion`.
  8. **Cómo consume el renderer el tema y el reduced-motion.** PixiJS dibuja en canvas y no lee variables CSS, y además el renderer debe seguir siendo agnóstico de framework. Hay que definir un contrato: `packages/renderer` exporta un tipo `RendererTheme` (colores ya convertidos a número o hex, porque Pixi no entiende OKLCH, más las familias de fuente) y opciones como `{ theme, reducedMotion }`, con `setTheme()` y `setReducedMotion()` en caliente. La web los construye desde los tokens (`getComputedStyle` con una conversión, o una fuente única en TS que genere el CSS; hay que elegir una) y los actualiza cuando cambia el tema o el `matchMedia('(prefers-reduced-motion: reduce)')`. Hay que definir qué significa "reducir efectos": sin screen shake, sin destellos ni flashes (y en general nada que parpadee más de 3 veces por segundo, WCAG 2.3.1), sin partículas o con muy pocas, y tweens cortos. Una regla dura: **no cambia la temporización del `FightLog`**, para que la pelea siga sincronizada con los demás espectadores. Y que el renderer espere a `document.fonts.ready` (o a sus bitmap fonts) antes de dibujar texto.
  9. **Huecos que hay que cubrir.**
     - **Iconos**: elegir un set. Propongo `@lucide/svelte`, el default de shadcn-svelte, con imports individuales para tree-shaking. Los iconos decorativos llevan `aria-hidden`, y los botones de solo icono, un `aria-label` de Paraglide.
     - **Fuentes**: autoalojadas (`@fontsource-variable/*` o woff2 en `static/`), sin CDN de terceros, con `font-display: swap`, `preload` de la fuente principal, subset latin (cubre á, ñ, ¿, ¡), fallback con `size-adjust` para evitar CLS, y tokens `--font-sans` y `--font-display` en `@theme`. Esto se decide aquí aunque la familia concreta llegue con la identidad visual.
     - **i18n en componentes**: las primitivas reciben el texto por props o snippets y nunca lo tienen fijo. Los textos internos que shadcn trae en inglés ("Close", "Previous", "Next", textos `sr-only`) se reemplazan por mensajes de Paraglide o por props obligatorias. `lang` en `<html>` lo pone el hook de Paraglide. Los layouts deben tolerar que el español sea ~30 % más largo. Se usan propiedades lógicas (`ms-`, `ps-`, `text-start`) para no cerrar la puerta a RTL. Y hay helpers `Intl` compartidos (`formatCoins`, `formatDate`) en `lib/i18n/` en vez de formatear dentro de cada componente.
  10. **Pendiente.** Hay que aclarar la referencia a la "feature 004": hoy 004 es el pipeline de assets, no la identidad visual. Hay que decir si la identidad será una feature nueva y que 001 entrega una paleta neutra provisional con todos los tokens semánticos, para que cambiar de identidad sea solo cambiar valores.

### architect — 2026-10-20 — RESPUESTA
1. **Frontmatter y forma** → `approvals: []`, `supersedes: null`, `linear: null`, `depends_on: ["0003", "0007"]`; secciones Contexto, Decisión, Alternativas descartadas (Tailwind v3, Skeleton, DaisyUI, Melt/Bits directo, Storybook/Histoire, mode-watcher, tokens en TS) y Consecuencias. Agregué a `owner` como revisor, porque aceptar un ADR es un gate humano.
2. **Tailwind v4 CSS-first** → D1. `@tailwindcss/vite`, sin `tailwind.config.*`; `tokens.css` en dos niveles con OKLCH; `@theme inline`; `@custom-variant dark` sobre `data-theme`; tokens nuevos y heredados enumerados. El "nada de colores sueltos" se verifica con `scripts/check-tokens.mjs`. Elegí un script en vez de una regla de ESLint porque las clases viven en atributos de Svelte, donde ESLint no las ve bien.
3. **shadcn-svelte y Bits UI** → D2. shadcn-svelte 1.x, Bits UI ≥1 según el CLI, `tw-animate-css`, versiones exactas, alias de `components.json`, `cn()`, y un procedimiento de actualización con cabecera `MODIFICADO`.
4. **Tema con SSR** → D3. Cookie `dark|light|system`, `transformPageChunk` con `data-theme`, `color-scheme` y `theme-color`; script inline bloqueante con nonce para `system` (`kit.csp` en `mode: 'auto'`); sin `mode-watcher` (razón en D3). Precedencia: perfil → cookie → `dark`, con `system` solo como opción explícita. Se guarda en el perfil, como el idioma.
5. **Fronteras** → D4. Reglas de importación con `no-restricted-imports`/`no-restricted-globals`; los componentes de dominio no hacen fetch; `FightStage.svelte` en `lib/components/fight/` es el único que importa el renderer.
6. **`/dev/components`** → D5. Me aparté de la sugerencia de usar `dev` de `$app/environment`: elegí `ENABLE_DEV_CATALOG`, porque Playwright corre contra el build, donde `dev` es falso. 404 en producción (test 6). Vive bajo el prefijo de idioma vía `reroute`; muestra variante × estado × ambos temas; textos de demo exentos del lint de i18n; es el objetivo de axe y de las capturas.
7. **Mobile-first y a11y** → D6. WCAG 2.2 AA; los 44 px quedan como objetivo propio (2.5.5 AAA), con 24 px para enlaces dentro de texto; anillo `--ring` con `focus-visible`; `motion-safe:` y duraciones en 0 bajo reduced motion. axe y prueba de contraste de pares semánticos en ambos temas (tests 3 y 4).
8. **Renderer** → D7. `RendererTheme` con colores `0xRRGGBB` y fuentes, `RendererOptions`, `setTheme`/`setReducedMotion`. La fuente única son los tokens CSS (`getComputedStyle` + `culori`). Definí qué es "reducir efectos", la regla de 2.3.1 para todos, que **la temporización del `FightLog` no cambia** (test 7) y la espera de fuentes.
9. **Huecos** → D8. `@lucide/svelte` por icono con reglas de `aria`; fuentes autoalojadas con `swap`, `preload`, subset latin, fallback con `size-adjust` y tokens; i18n en primitivas (`_a11y-messages.ts` o props obligatorias), `lang` por Paraglide, +30 % de texto, propiedades lógicas y `lib/i18n/format.ts`.
10. **Pendiente** → "Pendiente (owner)". Corregí la referencia: la identidad visual será una feature nueva (no la 004), y 001 entrega una paleta neutra provisional con todos los tokens semánticos y `--arena-*`.
