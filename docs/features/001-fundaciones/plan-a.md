---
id: "001"
title: Fundaciones del monorepo — fase A
type: plan
status: done
risk: high
author: frontend-engineer
reviewers: [architect, security-reviewer]
approvals:
  - by: architect
    date: 2026-10-22
  - by: security-reviewer
    date: 2026-10-22
  - by: owner
    date: 2026-10-22
linear: TA-1
depends_on: []
created: 2026-10-21
updated: 2026-10-22
---

# Plan: Fundaciones del monorepo — fase A (desde spec.md 2026-10-22, enmienda SvelteKit 3)

> [intent](./intent.md) · [spec](./spec.md) · ADRs [0003](../../decisions/0003-stack-tecnologico.md), [0007](../../decisions/0007-i18n-desde-cero.md), [0008](../../decisions/0008-autenticacion.md) (D3, solo lo que la spec aplica a `web`), [0009](../../decisions/0009-ui-design-system.md) · [Aviso de privacidad](../../legal/aviso-de-privacidad.md)

**Alcance:** RF-A1 a RF-A23 y los RNF que aplican a `web` (seguridad de cookies, CSP y cabeceras, logs sin datos del cliente, i18n, WCAG 2.2 AA, 375 px, navegadores). Rama `ta-1-fundaciones-fase-a`, un solo PR. Las fases B y C tienen sus propios planes.

**Por qué security-reviewer:** este plan fija la CSP y las cabeceras (RF-A10), el guard de `Origin` de `web` (RF-A23), los logs de `web` (RF-A19, aviso de privacidad, "Pendiente" punto 8) y dos cookies (`locale` y `theme`).

## Decisiones de implementación

Son decisiones que la spec deja a la implementación, o desvíos menores que el registro de npm y de shadcn-svelte obligan a tomar. Cada una dice por qué.

1. **SvelteKit 3 (3.1.0) con adapter-node 6.0.0, desde el inicio.** Es la decisión del owner del 2026-10-22, recogida en la enmienda de la spec (RF-A1, RF-A2, RF-A10, RF-A14, RF-A16, RF-A19, RF-A23) y en las de ADR-0008 y ADR-0009. No hay migración posterior. Lo que Kit 3 cambia para este plan:
   - **Sin `svelte.config.js`.** Toda la configuración de Kit va en las opciones de `sveltekit({ … })` dentro de `vite.config.ts`; aquí solo `adapter` y `csp` (decisión 17).
   - **`#lib` en vez de `$lib`**, con *subpath imports* en `apps/web/package.json` e imports con extensión (`#lib/utils.js`) (decisión 18).
   - **Variables de entorno** en `src/env.ts` con `defineEnvVars`, leídas de `$app/env/private` (decisión 19). Nada de `$env/*` ni `$app/environment`.
   - **`handleError` recibe todos los errores**, con `kind` (decisión 4).
   - **CSRF:** el chequeo de formularios de Kit está siempre activo en el build (no en `vite dev`) y compara contra `https://` + `Host`; el guard propio de RF-A23 extiende la regla de ADR-0008 D4 a todo método no seguro (decisión 3, handle 2).
   - **Node ≥ 22.17** (`engines` de Kit 3.1.0) y **TypeScript 6.0.x** (peer `^6.0.0` de Kit y `<6.1.0` de typescript-eslint).
   - **Archivos `server`.** En Kit 3, todo archivo con un segmento `server` en el nombre es solo de servidor (`/[/.]server\.[^/]+$/`). `#lib/paraglide/server.js` (de donde sale `paraglideMiddleware`) solo se importa desde `src/lib/server/hooks/paraglide.ts`. Comprobé en `@inlang/paraglide-js@2.29.1` que `runtime.js` no importa `server.js`, así que `hooks.ts` (`reroute` con `deLocalizeUrl`) y los componentes no lo arrastran al cliente.
   - **Ecosistema joven.** Kit 3.0 salió el 2026-10-01. Nadie ejecutó todavía shadcn-svelte 1.7.0 ni Paraglide 2.29.1 con Kit 3; por eso el paso 2 es una prueba corta con punto de corte.
2. **Paraglide solo decide el idioma por URL.** Estrategia `['url', 'baseLocale']`, con `urlPatterns` explícitos para que `en` también lleve prefijo (por defecto, Paraglide 2 deja el idioma base sin prefijo). La detección de `/` sin prefijo (cookie `locale` → `Accept-Language` → `en`) y la escritura de la cookie son código nuestro (`src/lib/i18n/locale.ts` y `src/lib/server/hooks/locale-redirect.ts`). Así: la cookie se llama `locale` con los atributos de RF-A4; Paraglide nunca escribe `PARAGLIDE_LOCALE`; visitar `/en/…` no puede tocar la cookie; y el `Location` de la redirección es relativo (`Response.redirect` de Paraglide lo haría absoluto, y RF-A3 lo pide relativo).
   - **`LocaleSwitcher` nunca llama a `setLocale`** de Paraglide, en ningún caso: cambia de idioma con un enlace a `localizedHref()` y escribe la cookie con `writeLocaleCookie()`. Lo hago cumplir con ESLint: `no-restricted-imports` con `importNames: ['setLocale']` sobre `#lib/paraglide/runtime.js` (en `paths`, que compara el nombre exacto) y su forma relativa (en `patterns`, decisión 18), con un caso en el test 2.
   - **`paraglideMiddleware` también redirige** (security-reviewer, punto 4): con `Sec-Fetch-Dest: document` responde 307 con `Location: decision.redirectUrl.href`, absoluto y con el origen que adapter-node 6 arma como `https://` + `Host` (RF-C4). El handle `paraglide` envuelve al middleware y, si la respuesta es 3xx, reescribe **siempre** su `Location` a `pathname + search` de esa URL, pasado por la misma función que quita las `/` y `\` iniciales de sobra. No espero a que el e2e lo encuentre: la reescritura es incondicional y el e2e la prueba.
3. **Orden de los `handle`** (`sequence` en `hooks.server.ts`):
   1. `securityHeaders`: llama a `resolve` y agrega `nosniff`, `Referrer-Policy` y HSTS a toda respuesta, incluidas las 307, las 403 del guard de `Origin` y las 404 del guard de `/dev/**`. No hace nada antes de `resolve` ni escribe logs. **Tolera cabeceras inmutables:** si `headers.set` lanza `TypeError`, copia la respuesta (`new Response(res.body, res)`) y pone las cabeceras en la copia, como `withVary` de Paraglide; así una respuesta de `fetch` reenviada nunca se convierte en un 500.
   2. `originGuard` (RF-A23): el **primer handle que puede responder**. Para todo método distinto de `GET`, `HEAD` y `OPTIONS`, en cualquier ruta (incluidas `/healthz`, `/api/**` y `/ws`) y con cualquier `Content-Type`: pasa si `Origin` es exactamente `PUBLIC_ORIGIN` (comparación de cadenas, sin normalizar), o si `Origin` falta y `Sec-Fetch-Site` es `same-origin`. En cualquier otro caso, incluido `Origin: null`, responde `403` con `{"error":"auth.csrf_origin"}` (`Content-Type: application/json`), sin llamar a `resolve` y sin escribir logs. Nunca lee `Host`, `X-Forwarded-*` ni `event.url`. Se construye con una fábrica pura, `createOriginGuard(trustedOrigin)`, y `hooks.server.ts` le pasa `PUBLIC_ORIGIN` de `$app/env/private`; así el unit no necesita el módulo de entorno. Corre también en `vite dev`, donde el chequeo de Kit no corre.
      - **Falla cerrado** (Minor 2 de architect, ronda Kit 3): si `trustedOrigin` es `undefined` o `''`, rechaza con 403 todo método no seguro, aunque `Sec-Fetch-Site` sea `same-origin`. Un `Origin` vacío nunca coincide con un `PUBLIC_ORIGIN` vacío.
      - **Comparación exacta, sin normalizar** (Minor 3 de security-reviewer, ronda Kit 3): `https://TORNEO.example` o `https://torneo.example:443` contra `PUBLIC_ORIGIN=https://torneo.example` dan 403. El unit lo fija para que nadie agregue una normalización.
   3. `localeRedirect`: si la ruta no está excluida y no empieza con `/es/` o `/en/` (ni es `/es` o `/en` exactos), responde 307 y no llama a `resolve`.
      - **Exclusiones por segmento completo:** `/healthz`, `/api` y `/ws`, cada una exacta o seguida de `/` (`/api` y `/api/x` sí; `/apix`, `/healthzz` y `/wsx` no), igual que `/developers` en el guard.
      - **`Location`** = `/<locale>/` + `event.url.pathname` sin `/` ni `\` iniciales + `event.url.search`. Solo usa esos dos campos, ya serializados por el parser WHATWG; nunca `request.url` crudo, ninguna cabecera ni `new URL(path, base)`.
      - **Cabeceras de la 307:** `Vary: Cookie, Accept-Language` y `Cache-Control: private, no-store`, porque el destino depende de esas cabeceras.
   4. `paraglide`: `paraglideMiddleware`, que se salta en las rutas excluidas (para que nunca redirija `/healthz`) **(cambiado: ahora corre también en las rutas excluidas, con una URL efectiva que lleva el idioma; ver desviación 14)**, reemplaza `%paraglide.lang%` y reescribe a relativo el `Location` de sus propias redirecciones (decisión 2).
   5. `devGuard` (RF-A16): si `route.id` es `/dev` o empieza con `/dev/` y `ENABLE_DEV_CATALOG !== '1'` (de `$app/env/private`), devuelve un **`Response` 404 directo** (`new Response(null, { status: 404 })`), sin `resolve` y sin `throw error(404)`, para no renderizar el árbol de errores ni pasar por `handleError`. La segunda capa, `dev/+layout.server.ts`, sí lanza `error(404)`; en Kit 3 eso llega a `handleError` con `kind: 'app'` y no se registra (decisión 4).
   6. `theme` (RF-A6 y RF-A9): resuelve el tema y reemplaza `%theme%` y `%theme.meta%` en `transformPageChunk` **solo en la parte del documento anterior a `</head>` (que incluye la etiqueta `<html>`), y solo la primera aparición de cada marcador**. Un handle con estado por petición marca cuándo ya pasó `</head>`; desde ahí, los fragmentos siguientes salen sin tocar. Un texto futuro con el marcador en el `<body>` no recibe `<meta>` inyectadas. Unit con un HTML que trae `%theme.meta%` también en el `<body>`.

   RF-A23 pide que `originGuard` sea el primero que puede responder, envuelto solo por `securityHeaders`, y antes de la redirección por idioma, de Paraglide y del guard de `/dev/**`. RF-A16 pide que `devGuard` vaya "después de Paraglide y antes de todo lo demás, salvo el guard de `Origin`". `securityHeaders` envuelve a todos, pero solo toca la respuesta que ya existe; no ejecuta nada de la ruta. `localeRedirect` va antes de `paraglide` porque una ruta sin prefijo nunca llega a `/dev/**` sin redirigirse primero. Kit aplica su propio chequeo de formularios **antes** de `handle` (RF-A10); `originGuard` no lo reemplaza.
4. **`handleError` por `kind`, síncrono y sin lanzar** (RF-A19 enmendado). Kit 3 pasa por `handleError` todos los errores, incluidos `error(...)` y los 404/405 del framework, con `kind` (`'app'`, `'framework'`, `'validation'` o `'unknown'`).
   - **Solo `kind: 'unknown'` se registra.** Con `'app'`, `'framework'` y `'validation'` no escribe nada: cualquiera genera 404 a voluntad, y registrarlos sería registrar peticiones.
   - **`error_id` solo para lo que se registra.** Se genera con `crypto.randomUUID()` solo en `kind: 'unknown'`, y se devuelve como `errorId`. Así, todo `error_id` que ve un usuario existe en los logs.
   - **`message` es un código fijo** de una lista cerrada (`'not_found'` si el `status` que calculó Kit es 404, `'internal'` en el resto), en todos los `kind`, y nunca `error.message` ni el error crudo: RF-A20 lanza a propósito un mensaje con una IP y un query de canario. No devuelve `status`, así que queda el que calcula Kit (el `merge` de Kit 3 hereda lo que el hook omite).
   - **Tipos.** En Kit 3, `App.Error` ya declara `status: number` y `message: string`, y redeclarar `message` con otro tipo es un error de TypeScript. Por eso `app.d.ts` solo agrega `errorId?: string`, y la lista cerrada la impone el tipo de retorno de la función (`ErrorCode = 'not_found' | 'internal'`). `+error.svelte` elige el texto de Paraglide por `page.status`, nunca muestra `message`, y muestra `errorId` solo si existe.
   - **Síncrono** (Minor 2 de security-reviewer, ronda Kit 3). En Kit 3.1.0, si `handleError` devuelve una `Promise` durante el render, Kit escribe un `console.warn` y descarta el resultado, y si la promesa rechaza escribe el error crudo con `console.error`, sin saneador. La función no lleva `async` ni devuelve promesas, y un unit lo verifica (paso 5).
   - **Nunca lanza.** Todo su cuerpo va en `try/catch`; si algo falla (por ejemplo, un `message` que es un *getter* que lanza), devuelve `{ message: 'internal' }` sin registrar el error original. Si `handleError` lanzara, Kit escribiría el error original con `console.error`.
5. **`cn()` con `clsx` + `tailwind-merge`, como fija ADR-0009 D2.** El registro actual de shadcn-svelte cambió `utils.ts` para importar el paquete `cn` (`cn@^0.3.0`, de shadcn-ui; el nombre en npm existe desde 2013). Mantengo el ADR: escribo `src/lib/utils.ts` a mano, con los tipos `WithElementRef` y `WithoutChild*` del registro, y no agrego `cn`. Cambiar a `cn` requiere modificar el ADR.
6. **`@lucide/svelte` 0.561.0**, que es el rango que pide el registro (`^0.561.0`). La 1.x existe, pero cambiar de mayor fuera de lo que pide el registro no aporta nada en 001; Dependabot (fase C) lo propondrá.
7. **El plugin de mensajes de inlang va instalado y no por CDN.** Por defecto, `project.inlang/settings.json` carga `@inlang/plugin-message-format` desde jsDelivr al compilar. Lo instalo fijado como `devDependency` y `modules` apunta a `"./node_modules/@inlang/plugin-message-format/dist/index.js"` (relativo a `apps/web`, el padre de `project.inlang`). *Verificado por security-reviewer:* `importLocalPlugins` de `@inlang/sdk@4.0.4` lee del disco todo módulo que no empiece con `http`, y el `dist/index.js` de la 4.5.5 no tiene imports.
   - **No hay alternativa por CDN.** Si la ruta local fallara en el paso 4, paro y el plan vuelve a revisión. Un JS remoto en el build se salta el lockfile, el hash y `minimumReleaseAge`.
   - **Chequeo:** `check-i18n.mjs` falla si alguna entrada de `modules` en `project.inlang/settings.json` empieza con `http` (sin distinguir mayúsculas), con un fixture que lo prueba. Así, un `paraglide init`, `sv add` o `--overwrite` que reescriba `settings.json` rompe `just check`.
8. **Más primitivas modificadas de las que lista RF-A12.** El registro actual trae los mismos problemas en otros archivos, y `check-tokens.mjs` y el test de foco fallarían si no se corrigen. Esta es la lista cerrada **por archivo** de lo que lleva `<!-- MODIFICADO: … -->`; la revisión del diff del paso 10 se hace contra ella, y cualquier archivo de `ui/` modificado que no esté aquí se anota en "Desviaciones":

   | Archivo | Cambio |
   |---|---|
   | `button/button.svelte` | `destructive`: `text-white` → `text-destructive-foreground` (RF-A12); se quitan `dark:bg-destructive/60` (bajaría el contraste real por debajo de lo que mide el test 3) y `focus-visible:ring-destructive/20` (pinta el foco con otro color que `--ring`). Anillo de D6 en la base. `size` `default` → `h-11`; `icon` → `size-11` |
   | `badge/badge.svelte` | `destructive`: `text-white` → `text-destructive-foreground`; `ring-[3px] ring-ring/50` → anillo de D6 |
   | `input/input.svelte` | `ring-[3px] ring-ring/50` y `focus-visible:border-ring` → anillo de D6; `h-9` → `h-11` |
   | `dialog/dialog-overlay.svelte` | `bg-black/50` → `bg-background/80` (RF-A12) |
   | `dialog/dialog-content.svelte` | botón de cerrar: `focus:` → `focus-visible:` con el anillo de D6, de 16 px a `size-11`, texto `Close` → `_a11y-messages.ts`; `duration-200` → `duration-(--duration-base)` |
   | `dropdown-menu/dropdown-menu-item.svelte` | `min-h-11` |
   | `dropdown-menu/dropdown-menu-checkbox-item.svelte` | `min-h-11` (lo usa `ThemeSwitcher`, con `aria-checked`) |
   | `dropdown-menu/dropdown-menu-radio-item.svelte` | `min-h-11` |
   | `dropdown-menu/dropdown-menu-sub-trigger.svelte` | `min-h-11` |
   | `dialog/dialog-footer.svelte` | botón de cerrar opcional: texto `Close` → `_a11y-messages.ts` (añadido en la implementación) |
   | `dropdown-menu/dropdown-menu-content.svelte`, `dropdown-menu/dropdown-menu-sub-content.svelte` | `duration-100` → `duration-(--duration-base)` (añadido en la implementación) |

   **Ajustes al registro actual (estilo `vega`, desviación 3):** el `button` traía `h-8`, `size-8`, `ring-3` y `focus-visible:ring-ring/50`, y su `destructive` era `bg-destructive/10 text-destructive`; queda con `h-11`/`size-11` (y `lg` en `h-12`/`size-12`, para que no sea menor que el tamaño por defecto), el anillo de D6 y `bg-destructive text-destructive-foreground`, que es el par que mide el test 3. Su variante `outline` y la del `badge` dibujan el límite con `border-input` en vez de `border-border` (ADR-0009 test 3). El overlay traía `bg-black/10`, no `bg-black/50`. Los tamaños `xs`, `sm`, `icon-xs` e `icon-sm` del botón quedan como en el registro: no son los de por defecto.

   Si al correr `add` aparece en otro archivo un `sr-only` en inglés, un color suelto o un `ring-ring/<n>`, se corrige igual y se suma a la tabla en el mismo commit.
9. **`theme-color` en hex.** `<meta name="theme-color">` con OKLCH no lo leen todos los navegadores que soportamos. `theme-color.ts` guarda el equivalente sRGB en hex de cada `--background`, y el test 3 convierte el token con `culori` y compara con tolerancia de ±1 por canal.
10. **El script del tema `system` en `app.html`** (≤ 1 KB, `nonce="%sveltekit.nonce%"`): si `data-theme` es `system`, lo reescribe con `dark` o `light` según `matchMedia` y escucha los cambios; en cada cambio vuelve a leer la cookie `theme` y no hace nada si ya no es `system`. Así no hace falta otro atributo en `<html>` cuando la runa cambia a `dark` o `light`.
11. **Captura de los logs en los e2e.** Playwright arranca las dos instancias de `node build` con un envoltorio (`e2e/support/serve-with-log.mjs`) que escribe cada fragmento de stdout y stderr con **`appendFileSync`** en `test-results/logs/<instancia>.log`, para que el archivo no quede atrás del proceso. `webServer.stdout: 'pipe'` no sirve, porque solo reimprime en la consola del runner. Hay dos niveles de comprobación:
    - **Durante los tests:** cada test lee el archivo antes y después de sus peticiones, para atar una línea concreta a una petición (el `error_id` de A19).
    - **Al final (`globalTeardown`, `e2e/support/global-teardown.ts`):** detiene los dos servidores, espera a que salgan y revisa **los archivos completos**. Así no hay falsos negativos por escrituras asíncronas que lleguen tarde (en `:4174` se esperan cero líneas de petición y no habría ninguna que confirme el vaciado). Falla si alguna línea contiene un canario, la IP `203.0.113.77` o el query, o si no cumple la lista blanca de la decisión 12.
12. **Lista blanca de líneas de log.** Toda línea de los dos archivos es JSON con solo los campos de RF-A19, salvo **una**: la de adapter-node, que debe cumplir exactamente `^Listening on http://[^\s]+:\d+$`, aparecer una sola vez y antes de cualquier línea producida por una petición. No contiene datos del cliente y no se puede desactivar. Cualquier otra salida que no venga de `log.ts` hace fallar el teardown: un aviso de Node, un `console.error` de Kit o un log de arranque con variables de entorno. **El log de arranque de `init` solo lleva un código** (por ejemplo `event: "web_started"`) y nunca valores de variables de entorno.
13. **Esqueleto a mano y CLIs solo en local.** `pnpm dlx` no usa el lockfile y sus dependencias transitivas no siguen las versiones fijadas. Por eso `apps/web` se crea **a mano**, sin `sv create`. `shadcn-svelte@1.7.0` es el único CLI por `pnpm dlx`: corre solo en local, en los pasos 2 y 10, con `--no-deps` y la revisión del diff; nunca en CI ni en ninguna receta de `justfile`. `init` solo se ejecuta en el paso 2, en una copia desechable, para ver qué alias propone; su salida no se commitea.
14. **`strictDepBuilds: true`** en `pnpm-workspace.yaml`, junto con `onlyBuiltDependencies: []`. Una dependencia nueva con scripts de instalación rompe `pnpm install` en vez de quedar ignorada en silencio. Si alguna los necesita de verdad, se agrega a `onlyBuiltDependencies` con su motivo en "Desviaciones" y vuelve a security-reviewer.
15. **Herramientas en la raíz.** Prettier y su configuración viven en la raíz (formatean `apps/web` y `scripts/`). ESLint vive en `apps/web/eslint.config.js` (solo lintea `apps/web`). Los scripts de `scripts/` se prueban con Vitest desde la raíz (`scripts/vitest.config.mjs`) y usan `svelte/compiler` de la dependencia de la raíz, con la misma versión que `apps/web`.
16. **Límites para ajustar la CSP.** Si el e2e de `securitypolicyviolation` encuentra una violación (en Chromium o WebKit), el ajuste **nunca** es agregar `'unsafe-inline'`, `'unsafe-eval'` ni `'strict-dynamic'` a `script-src`. Cualquier cambio de `script-src` respecto de RF-A10 detiene la implementación, se anota en "Desviaciones" y vuelve a revisión de security-reviewer. Los ajustes de `style-src` dentro de lo que ya permite RF-A10 sí se hacen en el PR, también anotados.
17. **Configuración de Kit en `vite.config.ts`** (RF-A2, RF-A10). No existe `svelte.config.js`. El plugin queda así: `sveltekit({ adapter: adapter(), csp: { mode: 'auto', directives: { … RF-A10 … } } })`, junto a `tailwindcss()` y `paraglideVitePlugin(paraglideOptions)`, más el bloque `test` de Vitest.
    - **No se escriben** `csrf`, `trustedOrigins`, `checkOrigin` ni `paths.origin`. `checkOrigin` ya no existe en Kit 3 (su presencia rompe la configuración), `trustedOrigins: ['*']` es la única forma de apagar el chequeo de formularios, y `paths.origin` metería el host en la imagen.
    - **`scripts/check-kit-config.mjs`** (en `just check`) falla si `apps/web/vite.config.ts` contiene `trustedOrigins`, `checkOrigin` o `paths.origin` (o una clave `origin:` dentro de `paths`), o si existe `apps/web/svelte.config.js`. Va con fixtures.
    - `svelte-kit sync` de Kit 3 lee la configuración de `vite.config.ts`, así que `_web-prepare` no cambia.
    - `%sveltekit.nonce%` en `app.html` y `transformPageChunk` funcionan igual que en Kit 2 (ADR-0009, enmienda D3). Kit 3 además rompe el build si se prerenderiza una página con el nonce en la plantilla, que refuerza `check-prerender.mjs`.
18. **Alias `#lib`** (RF-A2, RF-A14; ADR-0009, enmienda D2 y D4).
    - `apps/web/package.json` lleva `"imports": { "#lib/*": "./src/lib/*" }`. No hay `src/lib/index.ts`, así que no se declara la entrada `#lib` sola.
    - Todos los imports llevan extensión: `#lib/utils.js`, `#lib/components/ui/button/index.js`, `#lib/paraglide/messages.js`.
    - `tsconfig.json` extiende `$app/tsconfig`, con `include` y `exclude` propios, `strict` y `noUncheckedIndexedAccess`.
    - **ESLint y `#`.** `no-restricted-imports` interpreta los `group` de `patterns` con la sintaxis de `.gitignore`, donde una línea que empieza con `#` es un comentario: el patrón `#lib/features/**` **nunca coincide** y no da error. Lo comprobé con ESLint 9 y typescript-eslint 8.72.0: `#lib/features/**` no atrapa nada; `\#lib/features/**` y `regex: '^#lib/features/'` sí. Por eso los patrones del alias se escriben con `regex` (anclado con `^#lib/`), y los nombres exactos (`#lib/paraglide/runtime.js` para `setLocale`) van en `paths`, que no usa esa sintaxis. El test 2 tiene un caso por cada patrón `#lib` que solo ese patrón atrapa (por ejemplo, desactivando la forma relativa en ese caso), para que un patrón muerto no quede tapado por su gemelo relativo.
    - Prohibidos en todo `src/`: `$lib`, `$lib/**`, `$env/**` y `$app/environment`, con un mensaje que apunta a `#lib` y a `$app/env/*` (RF-A14). Esos sí van en `group`, porque no empiezan con `#`.
    - **`ignores` globales** (Minor 5 de architect, ronda Kit 3): `eslint.config.js` ignora `src/lib/paraglide/**` (generado), `.svelte-kit/**` y `build/**`. El test 2 tiene además un caso que **pasa** con `import type` de `#lib/…` donde la regla lo permite (`allowTypeImports`), para que un `regex` demasiado ancho no prohíba también los tipos.
    - **Versión de ESLint** (Minor 4 de architect, ronda Kit 3): el hallazgo del `#` se comprobó con ESLint 9 y el plan fija la 10.13.0. Al terminar el paso 9 se anota en "Desviaciones" con qué versión real se verificó el comportamiento (el test 2 lo comprueba con la versión instalada).
19. **Entorno de `web` en `src/env.ts`** (RF-A2, RF-A16, RF-A23).
    - `defineEnvVars` (de `@sveltejs/kit/env`) declara dos variables privadas y dinámicas: `ENABLE_DEV_CATALOG` (opcional; solo `'1'` la enciende) y `PUBLIC_ORIGIN`. En Kit 3 no hay semántica de prefijo `PUBLIC_`: lo que decide si una variable es pública es `public: true`, que aquí no se usa.
    - `PUBLIC_ORIGIN` es opcional solo durante el build (`building` de `$app/env`). En runtime, si falta o no es un origen `http(s)://host[:puerto]` sin ruta, query, fragmento, credenciales ni barra final (la regla de RF-B8), Kit no arranca la app.
    - **Forma serializada del navegador** (Minor 2 de security-reviewer, ronda Kit 3): además de la regla de RF-B8, el validador rechaza todo valor con `new URL(v).origin !== v`. Así quedan fuera las mayúsculas en el host, el puerto por defecto (`:443` en `https`, `:80` en `http`), la barra final, la ruta y las credenciales. Motivo: el guard compara cadenas sin normalizar, y el navegador manda el `Origin` en minúsculas y sin puerto por defecto; un `PUBLIC_ORIGIN=https://Torneo.example` rechazaría en silencio toda mutación legítima.
    - **Los validadores viven en `src/lib/env/validators.ts`**, puros y sin imports de Kit, para probarlos en Vitest sin el módulo de entorno. `src/env.ts` solo los conecta.
    - **El mensaje de error de un validador nunca incluye el valor** (Minor 3 de security-reviewer, ronda Kit 3): Kit imprime esos mensajes tal cual al arrancar, y cuando `web` reciba secretos (ADR-0008 D5) terminarían en el `json-file` de Docker. El mensaje nombra la variable y la regla (`PUBLIC_ORIGIN must be an http(s) origin without path`), y un unit comprueba, para cada entrada inválida, que el mensaje no la contiene.
    - Se leen de `$app/env/private`, que Kit protege para que no llegue al cliente.
20. **`PUBLIC_ORIGIN` es la única fuente de URLs absolutas** (Minor 1 de security-reviewer, ronda Kit 3). Sin `paths.origin`, `event.url.origin` es `https://` + `Host`, y quien llega directo al contenedor (por ejemplo en `prod-local`) elige el `Host`. `web` **nunca** construye URLs absolutas con `event.url.origin`, `url.host`, `url.href` ni `request.url`; si alguna vez necesita una (canónica, `hreflang`, correo), usa `PUBLIC_ORIGIN`. En 001 ningún código arma URLs absolutas: las redirecciones son relativas (decisión 2) y el guard compara contra `PUBLIC_ORIGIN`. Lo hago cumplir con ESLint en `src/**/*.{ts,svelte}` con `no-restricted-syntax` (Minor 3 de architect y Minor 1 de security-reviewer, ronda Kit 3). Los selectores:
    - `origin`, `host` y `href` sobre `<algo>.url` (`MemberExpression` cuyo objeto es una propiedad `url`) **o** sobre un `Identifier` llamado `url` (cubre `const { url } = event; url.origin`);
    - `request.url`;
    - `new URL(…, …)` con segundo argumento (`NewExpression[callee.name='URL'][arguments.length>=2]`), que arma una URL absoluta con la base que se le pase;
    - `.toString()` sobre `<algo>.url` o sobre `url`, y `String(<algo>.url)` / `String(url)`;
    - plantillas con `<algo>.url` o `url` como expresión (`` `${event.url}` ``).

    El test 2 tiene un caso por forma. `pathname` y `search` siguen permitidos, que es lo único que usan `localeRedirect` y la reescritura del handle `paraglide` (decisión 2). Si algún selector da falsos positivos en código legítimo, se anota en "Desviaciones"; la regla sigue valiendo y se aplica en revisión.

## Versiones fijadas

Verificadas contra el registro de npm el **2026-10-22 04:58 UTC** (`npm view <paquete> time`). Para la enmienda de Kit 3 volví a consultar el mismo día `@sveltejs/kit`, `@sveltejs/adapter-node`, `bits-ui`, `vite` y los peers de `typescript-eslint`, `svelte-check`, `@sveltejs/vite-plugin-svelte` y `@inlang/paraglide-js`. Todas tienen más de 24 h publicadas, así que pasan `minimumReleaseAge: 1440`. Todas van exactas (`save-exact=true`). **Antes de instalar, el paso 1 vuelve a comprobar las fechas**; si alguna cambió, se anota en "Desviaciones".

| Paquete | Versión | Publicada | Notas |
|---|---|---|---|
| pnpm (`packageManager`) | `10.34.7` | 2026-10-12 | ≥ 10.16 por `minimumReleaseAge` (nota 2 de devops). Hash: `pnpm@10.34.7+sha512.ba98abc585539933375b5971f4bc6d6c73a1b4b7edc4a879e7d102fe5423d1e02309d67c60a11f04955093a96e521ad9c3b138cdd654ca5be563f378b3ae48bd`, calculado a partir de `dist.integrity` del registro; corepack lo valida en el primer uso. Descarto 11.x y 12.x: RF-A1 nombra `onlyBuiltDependencies` y no verifiqué si sigue existiendo en esas versiones |
| Node (`.node-version`) | `22.23.4` | — | Versión exacta ≥ 22.17 (RF-A1; `engines` de Kit 3.1.0). `engines.node: ">=22.17"` en la raíz y `engine-strict=true` en `.npmrc`. Si al escribir el paso 1 hay una 22.x más nueva, se usa esa y se anota |
| `@sveltejs/kit` | `3.1.0` | 2026-10-14 | Decisión 1. Peers: Vite `^8.0.12`, Svelte `^5.57.1`, TS `^6.0.0`, `vite-plugin-svelte` `^7.0.0`; `engines.node >=22.17` |
| `@sveltejs/adapter-node` | `6.0.0` | 2026-10-01 | Peer `@sveltejs/kit ^3.0.0-next.0`. Sin `ORIGIN`; `ADDRESS_HEADER` y `XFF_DEPTH` igual que en la 5 |
| `@sveltejs/vite-plugin-svelte` | `7.3.1` | 2026-09-23 | Kit 3.1.0 pide `^7.0.0` |
| `svelte` | `5.57.2` | 2026-10-06 | ≥ 5.57.1 (Kit 3). También en la raíz (parser de los scripts) |
| `vite` | `8.3.4` | 2026-10-08 | ≥ 8.0.12 (Kit 3). La 8.3.5 tiene menos de 24 h |
| `typescript` | `6.0.3` | 2026-04-16 | Kit 3 pide `^6.0.0` y typescript-eslint `<6.1.0`; la 7.x queda fuera por los dos |
| `svelte-check` | `4.7.7` | 2026-10-16 | Peer TS `^5 \|\| ^6` |
| `@inlang/paraglide-js` | `2.29.1` | 2026-10-19 | |
| `@inlang/plugin-message-format` | `4.5.5` | 2026-10-20 | Decisión 7 |
| `tailwindcss` | `4.3.3` | 2026-07-16 | La versión con la que frontend-engineer verificó D1 y D6 en ADR-0009 |
| `@tailwindcss/vite` | `4.3.3` | 2026-07-16 | |
| `tw-animate-css` | `1.4.0` | 2025-09-24 | |
| `shadcn-svelte` (CLI, solo con `pnpm dlx`) | `1.7.0` | 2026-09-16 | No entra como dependencia |
| `bits-ui` | `2.20.0` | 2026-10-20 | El registro pide `^2.14.4`. Fijado: la 2.20.1 salió el 2026-10-22 19:01 UTC y no pasa `minimumReleaseAge`. No depende de Kit (peers: Svelte `^5.33.0` y `@internationalized/date`) |
| `@internationalized/date` | `3.12.4` | 2026-09-01 | Peer de `bits-ui` |
| `tailwind-variants` | `3.3.1` | 2026-08-03 | El registro pide `^3.2.2` |
| `clsx` | `2.1.1` | 2024-04-23 | Decisión 5 |
| `tailwind-merge` | `3.7.0` | 2026-09-12 | Decisión 5 |
| `@lucide/svelte` | `0.561.0` | 2025-12-12 | Decisión 6 |
| `culori` | `4.0.2` | 2025-06-27 | Test 3 (y luego `renderer-theme.ts`, D7) |
| `@types/culori` | `4.0.1` | 2025-09-02 | |
| `eslint` | `10.13.0` | 2026-10-16 | |
| `@eslint/js` | `10.0.1` | 2026-02-06 | |
| `typescript-eslint` | `8.72.0` | 2026-10-19 | Peer `eslint ^10`, TS `<6.1.0` |
| `eslint-plugin-svelte` | `3.23.1` | 2026-10-09 | |
| `svelte-eslint-parser` | `1.9.0` | 2026-10-09 | |
| `globals` | `17.13.0` | 2026-10-01 | |
| `eslint-config-prettier` | `10.1.8` | 2025-07-18 | |
| `prettier` | `3.9.10` | 2026-10-10 | Raíz |
| `prettier-plugin-svelte` | `4.1.1` | 2026-06-15 | Raíz |
| `prettier-plugin-tailwindcss` | `0.8.1` | 2026-07-15 | Raíz |
| `vitest` | `5.0.5` | 2026-10-15 | Raíz y `apps/web` |
| `@playwright/test` | `1.64.1` | 2026-10-14 | |
| `@axe-core/playwright` | `4.14.0` | 2026-10-15 | |
| `@types/node` | `22.20.5` | 2026-10-01 | Igual a la línea de Node |

**No verificado:** (a) que `shadcn-svelte@1.7.0` funcione con Kit 3 y `#lib` (el architect leyó en su código que detecta `#lib/*` en `imports` y lo propone; no lo ejecutó nadie; `init` solo en la copia desechable del paso 2, decisión 13); (b) que pnpm 10.34.7 instale todo con `onlyBuiltDependencies: []` y `strictDepBuilds: true` (Vite 8 trae `rolldown` y Tailwind 4 trae `@tailwindcss/oxide`, que usan binarios por `optionalDependencies`; security-reviewer comprobó que ni ellos ni Kit ni Paraglide tienen scripts de instalación); (c) que Vite resuelva `#lib/x.js` a `src/lib/x.ts` desde archivos `.ts` y `.svelte`, y que Paraglide 2.29.1 compile y funcione con Kit 3 (su plugin de Vite y su regla de ESLint no fijan versión de Kit). Todo se comprueba en los pasos 2 y 4.

## Archivos que cambian

| Archivo | Cambio |
|---|---|
| `package.json` | nuevo: `private`, `packageManager` con hash, `engines`, scripts mínimos, devDeps de la raíz (Prettier y plugins, Vitest, Svelte) |
| `pnpm-workspace.yaml` | nuevo: `apps/web`, `packages/*`; `minimumReleaseAge: 1440`; `onlyBuiltDependencies: []`; `strictDepBuilds: true` (decisión 14) |
| `pnpm-lock.yaml` | nuevo (generado) |
| `.npmrc` | nuevo: `save-exact=true`, `engine-strict=true` |
| `.node-version` | nuevo: `22.23.4` (exacta, ≥ 22.17) |
| `.prettierrc.json`, `.prettierignore` | nuevos. Se ignoran `**/*.md`, `docs/`, `content/`, `.claude/`, `private/`, el lockfile, `.svelte-kit`, `build`, `src/lib/paraglide` y `packages/protocol-ts/src/generated` (B) |
| `.gitignore` | modificado: `apps/web/src/lib/paraglide/` (compilado) |
| `justfile` | nuevo: `default`, `install`, `dev`, `build`, `fmt`, `check`, `test`, `e2e`, `e2e-install` y las recetas internas `_web-prepare` y `_check-web` |
| `scripts/check-tokens.mjs` | nuevo: ADR-0009 D1 completo + regla `ring-ring/<n>` (RF-A13). Exporta `checkTokens(files)` para los tests |
| `scripts/check-i18n.mjs` | nuevo: RF-A15 con `svelte/compiler`, más el chequeo de `modules` sin `http` en `project.inlang/settings.json` (decisión 7). Exporta `checkI18n(opts)` |
| `scripts/check-prerender.mjs` | nuevo: RF-A2. Falla con `prerender = true` o `'auto'` en `apps/web/src/routes/**` |
| `scripts/check-kit-config.mjs` | nuevo: RF-A10 y decisión 17. Falla si `apps/web/vite.config.ts` contiene `trustedOrigins`, `checkOrigin` o `paths.origin` (o `origin:` dentro de `paths`), o si existe `apps/web/svelte.config.js` |
| `scripts/vitest.config.mjs`, `scripts/__tests__/*.test.mjs`, `scripts/__fixtures__/{check-tokens,check-i18n,check-prerender,check-kit-config}/**` | nuevos: un fixture por caso del test 1 de ADR-0009, más `ring-ring/50`; llave faltante, mensaje vacío, texto literal, `aria-label` literal, `settings.json` con un módulo `https://…` y caso válido en `src/routes/dev/**`; `vite.config.ts` con `trustedOrigins`, con `checkOrigin`, con `paths: { origin }` y uno válido |
| `apps/web/package.json` | nuevo: `"imports": { "#lib/*": "./src/lib/*" }` (decisión 18), todas las dependencias en `devDependencies` (RF-C2 lo necesitará) y scripts `dev`, `build`, `preview`, `check`, `lint`, `test`, `e2e` |
| `apps/web/vite.config.ts` | nuevo: `sveltekit({ adapter, csp })` con la CSP de RF-A10 y sin `csrf` ni `paths` (decisión 17), `tailwindcss()`, `paraglideVitePlugin(paraglideOptions)`; bloque `test` de Vitest (entorno `node`). No hay `svelte.config.js` |
| `apps/web/paraglide.options.js` | nuevo: `project`, `outdir`, `strategy: ['url','baseLocale']` y `urlPatterns` (decisión 2); lo comparten Vite y `_web-prepare` |
| `apps/web/project.inlang/settings.json` | nuevo: `baseLocale: "en"`, `locales: ["en","es"]`, `modules: ["./node_modules/@inlang/plugin-message-format/dist/index.js"]` (decisión 7) |
| `apps/web/messages/{en,es}.json` | nuevos: cabecera, selectores, inicio, error 404/500, `_a11y-messages`, nombres de idioma, nombres de tema |
| `apps/web/tsconfig.json` | nuevo: `strict`, `noUncheckedIndexedAccess`, extiende `$app/tsconfig`, con `include` y `exclude` propios (decisión 18) |
| `apps/web/eslint.config.js` | nuevo: flat config de RF-A14 (D4 con `#lib` por `regex` + `no-restricted-globals` + `no-console` + `$lib`, `$env/**` y `$app/environment` prohibidos), más `setLocale` prohibido (decisión 2), URLs absolutas prohibidas (decisión 20) e `ignores` de `src/lib/paraglide/**`, `.svelte-kit` y `build` (decisión 18) |
| `apps/web/components.json` | nuevo: alias de ADR-0009 D2 enmendado: `lib: #lib`, `components: #lib/components`, `ui: #lib/components/ui`, `utils: #lib/utils`, `hooks: #lib/hooks` |
| `apps/web/.env.example` | nuevo: `PUBLIC_ORIGIN=http://localhost:5173` y `ENABLE_DEV_CATALOG=1`, con un comentario: solo dev y e2e (RF-B18) |
| `apps/web/src/env.ts` | nuevo: `defineEnvVars` con `PUBLIC_ORIGIN` y `ENABLE_DEV_CATALOG`, privadas y dinámicas (decisión 19) |
| `apps/web/src/lib/env/validators.ts` | nuevo: validadores puros de `PUBLIC_ORIGIN` y `ENABLE_DEV_CATALOG`; sus mensajes nunca incluyen el valor (decisión 19) |
| `apps/web/playwright.config.ts` | nuevo: proyectos `chromium` y `webkit` (este último solo para los tests de tema y CSP), dos `webServer` (`:4173` con catálogo, `:4174` sin él), cada uno con `PUBLIC_ORIGIN=http://localhost:<su puerto>` (RF-C10), `globalTeardown` (decisión 11), capturas como artefactos |
| `apps/web/src/app.html` | nuevo: `<html lang="%paraglide.lang%" data-theme="%theme%">`, `%theme.meta%` (meta `color-scheme` y `theme-color`) dentro del `<head>`, script estático con nonce (RF-A7). Los marcadores de tema solo aparecen antes de `</head>` |
| `apps/web/src/app.css` | nuevo: los tres `@import` de D1 |
| `apps/web/src/app.d.ts` | nuevo: `App.Error { errorId?: string }`; `status` y `message` ya los declara Kit 3 (decisión 4) |
| `apps/web/src/hooks.server.ts` | nuevo: `init` (log de arranque con solo un código, sin variables de entorno, y `process.on`), `handle = sequence(...)` (decisión 3, con `originGuard` primero tras `securityHeaders`), `handleError` síncrono (decisión 4) |
| `apps/web/src/hooks.ts` | nuevo: `reroute` con `deLocalizeUrl` |
| `apps/web/src/lib/server/log.ts` | nuevo: único escritor de logs; JSON por línea; lista blanca de campos de RF-A19 |
| `apps/web/src/lib/server/sanitize.ts` | nuevo: quita IPv4 e IPv6, el query de las URLs y el valor que sigue a un header sensible |
| `apps/web/src/lib/server/hooks/{security-headers,origin-guard,locale-redirect,paraglide,dev-guard,theme,handle-error,process-handlers}.ts` | nuevos: un archivo por pieza, para probarlas sin servidor. `paraglide.ts` es el único que importa `#lib/paraglide/server.js` (decisión 1) |
| `apps/web/src/lib/theme/tokens.css` | nuevo: paleta neutra provisional en OKLCH, semánticos de shadcn + `destructive-foreground` + `--arena-*`, `@theme inline`, `@custom-variant dark`, duraciones fuera de `@theme` y bloque de reduced motion de D6 |
| `apps/web/src/lib/theme/theme-color.ts` | nuevo: hex de `--background` por tema (decisión 9) |
| `apps/web/src/lib/theme/resolve-theme.ts` | nuevo: validación `dark \| light \| system`, precedencia de D3 y override de RF-A9 (puro, sin `event`) |
| `apps/web/src/lib/theme/theme.svelte.ts` | nuevo: runa `{ choice, resolved }`, `setTheme()` (cookie, `data-theme`, `<meta>`) |
| `apps/web/src/lib/i18n/locale.ts` | nuevo: `isLocale`, `pickLocale(cookie, acceptLanguage)`, `writeLocaleCookie()`, `localizedHref()` |
| `apps/web/src/lib/i18n/format.ts` | nuevo: `formatCoins`, `formatDate`, `formatRelative` con `Intl` (ADR-0009 D8) |
| `apps/web/src/lib/utils.ts` | nuevo: `cn` con `clsx` + `tailwind-merge` y los tipos del registro |
| `apps/web/src/lib/components/ui/{button,input,label,card,badge,dialog,dropdown-menu,separator,skeleton}/**` | nuevos (CLI) y modificados según RF-A12 y la decisión 8 |
| `apps/web/src/lib/components/ui/_a11y-messages.ts` | nuevo: "Close" y los `sr-only` desde Paraglide |
| `apps/web/src/lib/components/layout/{AppHeader,LocaleSwitcher,ThemeSwitcher}.svelte` | nuevos: cabecera de "Diseño → UI" |
| `apps/web/src/routes/+layout.svelte`, `+page.svelte`, `+error.svelte` | nuevos: RF-A17 |
| `apps/web/src/routes/healthz/+server.ts` | nuevo: RF-A18 |
| `apps/web/src/routes/dev/+layout.server.ts` | nuevo: segunda capa del guard |
| `apps/web/src/routes/dev/components/+page.svelte` (+ secciones en `_sections/`) | nuevo: catálogo de D5, con cabecera, selectores y todas las primitivas |
| `apps/web/src/routes/dev/error-probe/{+page.server.ts,+page.svelte}` | nuevos: RF-A20 (el `.svelte` existe para que la ruta sea válida; nunca se renderiza) |
| `apps/web/src/**/*.test.ts` | nuevos: unit de Vitest (ver "Pruebas") |
| `apps/web/tests/eslint/boundaries.test.ts` (+ casos en el mismo archivo) | nuevo: test 2 con `ESLint#lintText` y rutas virtuales |
| `apps/web/e2e/*.spec.ts`, `apps/web/e2e/support/{serve-with-log.mjs,logs.ts,raw-http.ts,global-teardown.ts}` | nuevos: e2e; `logs.ts` tiene la lista blanca de líneas que usan los tests y el teardown |
| `CLAUDE.md` | modificado: sección "Comandos" (RF-A22) |
| `docs/legal/aviso-de-privacidad.md` | modificado: fila "Preferencia de idioma" (`locale`, 1 año) y punto 5 de "Pendiente" (RF-A22) |

No se tocan `apps/server`, `crates/*`, `packages/renderer`, `packages/protocol-ts`, `content/`, `infra/` ni `.github/`.

## Orden de trabajo

Cada paso termina con un comando que lo verifica. Un commit por paso o por grupo de pasos, con el formato `feat(web): … (TA-1)`.

1. **Raíz del monorepo (RF-A1).** Vuelvo a comprobar las fechas de la tabla de versiones. Escribo `package.json`, `pnpm-workspace.yaml`, `.npmrc`, `.node-version`, `.prettierrc.json`, `.prettierignore` y el cambio a `.gitignore`. → `node --version` da ≥ 22.17; `corepack enable && pnpm --version` da `10.34.7`; `pnpm install` en limpio pasa con `strictDepBuilds: true` y no avisa de scripts ignorados (`pnpm ignored-builds` vacío).
2. **Esqueleto de SvelteKit 3 (RF-A2) y prueba corta de shadcn-svelte con `#lib`** ("Áreas de preocupación" 12 de la spec). Creo `apps/web` **a mano**, sin `sv create` (decisión 13): `package.json` con `imports`, `vite.config.ts` con `sveltekit({ adapter, csp })` (decisión 17), `tsconfig.json` que extiende `$app/tsconfig`, `src/env.ts` con `PUBLIC_ORIGIN` obligatoria en runtime y `ENABLE_DEV_CATALOG` (decisión 19; los validadores completos llegan en el paso 6) y una página. Después:
   - En una **copia desechable** de `apps/web` (directorio temporal, fuera del repo), `pnpm dlx shadcn-svelte@1.7.0 init` para ver qué alias propone. Se espera `#lib` en los cinco; la copia se borra.
   - En el árbol real, `components.json` escrito a mano (tabla de archivos), `utils.ts` a mano (decisión 5) y `pnpm dlx shadcn-svelte@1.7.0 add --no-deps button`. El botón importa `#lib/utils.js`, sin ningún `$lib`.
   - La página importa el botón con `#lib/components/ui/button/index.js` desde un `.svelte` y `cn` con `#lib/utils.js` desde un `.ts`, y un `+page.server.ts` lee `ENABLE_DEV_CATALOG` de `$app/env/private`.
   → `svelte-check --fail-on-warnings` con 0 errores; `pnpm --filter web build` pasa; `PUBLIC_ORIGIN=http://localhost:3000 node apps/web/build` sirve la página con el botón y el nonce en el `<script>` de `app.html`; `node apps/web/build` **sin** `PUBLIC_ORIGIN` termina con código distinto de 0; `git diff --exit-code package.json apps/web/package.json pnpm-lock.yaml` vacío tras el `add`; `grep -rn '\$lib' apps/web/src` vacío. **Punto de corte:** si algo de esto falla (el CLI no acepta `#lib`, Vite no resuelve `#lib/x.js` a `.ts`, `svelte-check` no entiende `$app/tsconfig`, o la validación de entorno no detiene el arranque), paro, lo anoto en "Desviaciones" y el plan vuelve a revisión antes de seguir. No hay respaldo preaprobado.
3. **`justfile` mínimo (RF-A21).** `install`, `dev`, `build`, `fmt`, `test`, `e2e`, `e2e-install`, `_web-prepare` (`svelte-kit sync` y compilación de Paraglide) y `check` con lo que ya existe (incluido `check-kit-config.mjs`, aunque sus fixtures llegan en el paso 8). → `just check` pasa. **Paralelizable con el paso 4.**
4. **i18n (RF-A3, RF-A4).** `project.inlang`, `paraglide.options.js`, mensajes, `hooks.ts` (`reroute`), `lib/i18n/locale.ts` y los handles `localeRedirect` y `paraglide`. → unit de `pickLocale` (cookie válida, inválida, `Accept-Language` con pesos, ausente) y de `reroute`/`deLocalizeUrl` (`/es/dev/components` → `/dev/components`; `/healthz` y `/api/x` sin cambios). Unit de `localeRedirect`: `Location` armado solo con `pathname` y `search` (una ruta con `%0d%0a` sale codificada y nunca como salto de línea en la cabecera), `//evil.example/` y `/\evil.example` → `Location` que empieza con `/es/` o `/en/`, `Vary: Cookie, Accept-Language` y `Cache-Control: private, no-store`, y exclusiones por segmento (`/api` y `/api/x` excluidas; `/apix`, `/healthzz` y `/wsx` redirigen). Unit de `localizedHref()` con `//…` y `/\…`. Unit del handle `paraglide`: un 3xx del middleware con `Location` absoluto sale como `pathname + search`. `curl -sI localhost:5173/` da 307 con `Location: /en/`. Si la ruta local del plugin de inlang no compila, paro aquí (decisión 7). **Punto de corte de Paraglide con Kit 3** (Minor 1 de architect, ronda Kit 3): si Paraglide 2.29.1 no compila con Kit 3.1.0 (plugin de Vite o `paraglide-js compile`), o si `reroute`/`deLocalizeUrl` o `paraglideMiddleware` no funcionan con Kit 3 (por ejemplo, `route.id` sigue con el prefijo de idioma o el middleware no reemplaza `%paraglide.lang%`), paro, lo anoto en "Desviaciones" y el plan vuelve a revisión. No hay respaldo preaprobado.
5. **Logs (RF-A19).** `log.ts`, `sanitize.ts`, `handleError` (decisión 4), `init` con `process.on` y `process.exit(1)` tras `uncaughtException`. → unit del saneador (IPv4, IPv6 comprimida y con zona, `?query` en `http(s)://` y en rutas relativas, `cookie: …`, `Authorization: Bearer …`, `x-forwarded-for=…`, mayúsculas y minúsculas). Unit de `handleError`: con los cuatro `kind`, solo `'unknown'` escribe una línea, con solo los campos permitidos y un `error_id` igual al `errorId` devuelto; `'app'` (un `error(404)`), `'framework'` y `'validation'` no escriben nada y no devuelven `errorId`; en todos, `message` es `'not_found'` o `'internal'`, nunca el texto del error, y no se devuelve `status`; con un error cuyo `message` es un *getter* que lanza, no lanza, devuelve `{ message: 'internal' }` y no escribe el error original. **Síncrono:** para cada `kind`, el valor devuelto no es una `Promise` ni un *thenable* (`typeof result?.then !== 'function'`), y `handleError` no es una `AsyncFunction` (`handleError.constructor.name !== 'AsyncFunction'`). Unit del log de arranque: sin valores de variables de entorno.
6. **Cabeceras y CSP (RF-A10), guard de `Origin` (RF-A23), `/healthz` (RF-A18) y guard de `/dev/**` (RF-A16).** `securityHeaders`, la opción `csp` de `sveltekit()`, `originGuard`, `src/env.ts` completo con `validators.ts`, `healthz/+server.ts`, `devGuard` y `dev/+layout.server.ts`. → unit de `originGuard` con la tabla de RF-A23: método (`GET`, `HEAD`, `OPTIONS`, `POST`, `PUT`, `PATCH`, `DELETE`) × `Origin` (igual, ajeno, `null`, con barra final, con otro esquema, ausente) × `Sec-Fetch-Site` (`same-origin`, `same-site`, `cross-site`, `none`, ausente) × `Content-Type` (formulario, JSON, ausente); en los rechazos, 403 con cuerpo `{"error":"auth.csrf_origin"}` y `resolve` sin llamar (un espía); además, un `Host` y un `X-Forwarded-Host` iguales a `Origin` no cambian el resultado; un `Origin` con mayúsculas (`https://TORNEO.example`) o con puerto por defecto (`https://torneo.example:443`) da 403; y con `createOriginGuard(undefined)` y `createOriginGuard('')` todo método no seguro da 403, también con `Sec-Fetch-Site: same-origin` y con `Origin: ''`. Unit de los validadores: `PUBLIC_ORIGIN` válido (`http://localhost:5173`, `https://torneo.example`) e inválido (con ruta, con barra final, con query, con fragmento, con credenciales, `ftp://`, vacío, con mayúsculas en el host, `https://…:443` y `http://…:80`), y para cada inválido el mensaje del error **no contiene** la entrada; `ENABLE_DEV_CATALOG` solo enciende con `'1'`. Unit del guard de `/dev/**` (`/dev`, `/dev/components`, `/developers`, `null`, con y sin la variable; con la variable apagada devuelve un `Response` 404 sin llamar a `resolve` ni a `handleError`) y de `securityHeaders` (incluida una respuesta con cabeceras inmutables, que sale copiada con las cabeceras y no como 500); `curl` a `/healthz` da 200 sin `Location`.
7. **Tokens y tema (RF-A6 a RF-A9, RF-A11).** `tokens.css` con la paleta neutra, `theme-color.ts`, `resolve-theme.ts`, handle `theme`, script en `app.html` y `theme.svelte.ts`. Ajusto los valores OKLCH hasta que pase el test 3. Ya sé que la paleta "neutral" de shadcn no llega a 3:1 en `--ring` ni en `--input` contra el fondo claro, ni a 4.5:1 en `destructive-foreground`/`destructive`. → test 3 (contraste y `theme-color`), unit de `resolve-theme` (tabla de cookies, incluida `"><script>`; override solo con la variable y `route.id` bajo `/dev/`) y unit del handle `theme` (los marcadores solo se reemplazan antes de `</head>` y una vez).
8. **Scripts de calidad (RF-A13, RF-A15, RF-A2, RF-A10).** `check-tokens.mjs`, `check-i18n.mjs`, `check-prerender.mjs` y `check-kit-config.mjs`, con fixtures y tests. → `pnpm exec vitest run --config scripts/vitest.config.mjs` en verde, y cada script falla sobre su fixture de error con un mensaje que nombra archivo y línea. **Paralelizable con los pasos 5 a 7**, porque no depende de `apps/web/src`.
9. **ESLint (RF-A14).** `eslint.config.js` y `tests/eslint/boundaries.test.ts`, con los patrones `#lib` como `regex` (decisión 18). → test 2 en verde. Casos que fallan: valor del renderer fuera de `FightStage.svelte`; valor de `#lib/features/system/health.js` y de `../../features/x.js` en un componente de dominio; `import type` de features en `ui/`; mensajes de Paraglide (`#lib/paraglide/messages.js` y relativo) en `ui/` fuera de `_a11y-messages.ts`; `fetch` en un componente; `console.log` fuera de `log.ts`; `import { setLocale }` de `#lib/paraglide/runtime.js` en cualquier archivo; `import { cn } from '$lib/utils'`, `import … from '$lib'`, `$env/dynamic/private` y `$app/environment` en cualquier archivo de `src/`; y una por cada forma de URL absoluta de la decisión 20: `event.url.origin`, `url.host` tras `const { url } = event`, `request.url`, `new URL('/x', event.url)`, `event.url.toString()`, `String(event.url)` y `` `${event.url}` ``. Un caso por cada patrón `#lib` que **solo** ese patrón atrapa, para detectar un patrón muerto (decisión 18). Casos que pasan: `import type { RendererTheme }` en `renderer-theme.ts`, `import type` de `#lib/features/…` en un componente de dominio (decisión 18, `ignores` e `import type`), `$app/env/private` en `hooks.server.ts`, `event.url.pathname` en `locale-redirect.ts` y `new URL(value)` con un solo argumento. Al terminar, anoto en "Desviaciones" la versión real de ESLint con la que pasó el test 2 (decisión 18).
10. **Primitivas (RF-A12).** `components.json` y `utils.ts` ya existen desde el paso 2, igual que `button`. En local, `pnpm dlx shadcn-svelte@1.7.0 add --no-deps input label card badge dialog dropdown-menu separator skeleton`, con las versiones de la tabla ya instaladas y el lockfile fijado. Revisión del diff de todas las primitivas (`button` incluido) contra la tabla por archivo de la decisión 8, los cambios de RF-A12 y `_a11y-messages.ts`. Si el CLI dejó algún `$lib`, se cambia a `#lib` y el archivo lleva la cabecera `MODIFICADO` (ADR-0009, enmienda D2) y entra en la tabla de la decisión 8. → `git diff --exit-code package.json apps/web/package.json pnpm-lock.yaml` sale vacío; `pnpm why cn` (en la raíz y con `--filter web`) no devuelve nada; `grep -rn '\$lib' apps/web/src` vacío; `just check` en verde: `check-tokens` no encuentra nada en `ui/`, y `check-i18n` no encuentra "Close".
11. **Cabecera, páginas y catálogo (RF-A5, RF-A17, RF-A20, D5).** `AppHeader`, `LocaleSwitcher` (`dropdown-menu` con un item por idioma, como enlace a `localizedHref()`; al elegir, escribe la cookie con `writeLocaleCookie()` y navega; **nunca llama a `setLocale`**, decisión 2), `ThemeSwitcher` (`dropdown-menu` con `aria-checked`), `+layout.svelte`, inicio, `+error.svelte` (título y texto por estado y el `error_id`), catálogo con cada primitiva en cada variante y estado, y `error-probe`. → `just dev` y revisión manual a 375 px y a 1280 px, en `es` y `en`, en los dos temas.
12. **e2e (Playwright).** `playwright.config.ts`, `serve-with-log.mjs` con `appendFileSync`, `global-teardown.ts` con la lista blanca de líneas (decisiones 11 y 12), un cliente HTTP crudo (`node:http`) para las rutas `//evil.example/` y `/\evil.example`, que `fetch` normalizaría, y para fijar `Host`, `Origin`, `Sec-Fetch-Dest` y `Sec-Fetch-Site`, y las specs de la sección "Pruebas". Cada instancia recibe `PUBLIC_ORIGIN=http://localhost:<su puerto>`. → `just e2e` en verde en Chromium y WebKit, teardown incluido.
13. **Capturas y ajuste visual.** Corro el test 8, reviso las capturas contra "Diseño → UI" de la spec (una fila con los dos selectores a 375 px, alineada al final en escritorio, sin scroll horizontal) y ajusto. → capturas en `test-results/screenshots/`, que se adjuntan al PR.
14. **Docs (RF-A22).** "Comandos" de `CLAUDE.md` (`just install`, `just dev`, `just check`, `just test`, `just e2e`, `just e2e-install`, `just fmt`; Node ≥ 22.17 y corepack; `PUBLIC_ORIGIN` en `apps/web/.env`) y el aviso de privacidad (fila de idioma: `locale`, 1 año; el punto 5 de "Pendiente" queda solo con la consulta legal sobre si las cookies de preferencia son "necesarias"). → revisión del PR.
15. **Cierre.** `just check` y `just e2e` desde un clon limpio (`git clean -xdf` en un worktree temporal y `just install`). Corro `pnpm audit --audit-level moderate`. Pego en el PR las salidas de los tres, con las capturas y la evidencia de los canarios.

## `just check` en la fase A

```
set shell := ["bash", "-euo", "pipefail", "-c"]

check: _check-web          # B lo cambia a: check: _check-web _check-rust

_check-web: _web-prepare
    pnpm exec prettier --check .
    pnpm --filter web exec eslint .
    pnpm --filter web exec svelte-check --fail-on-warnings
    pnpm --filter web exec vitest run
    pnpm exec vitest run --config scripts/vitest.config.mjs
    node scripts/check-tokens.mjs
    node scripts/check-i18n.mjs
    node scripts/check-prerender.mjs
    node scripts/check-kit-config.mjs
```

- El orden es el de RF-A21 (el chequeo de `prerender` y el de la configuración CSRF de Kit van al final) y `just` corta en la primera línea que falla. Vitest corre dos veces, una por `apps/web` (unit, test 2 y test 3) y otra por `scripts/` (tests 1 y fixtures de i18n y prerender), y las dos cuentan como el paso "Vitest" de RF-A21.
- `_web-prepare` (`svelte-kit sync` y compilación de Paraglide con `paraglide.options.js`) va primero, porque ESLint y `svelte-check` necesitan los tipos generados. No cambia archivos versionados.
- **No entran:** Playwright (`just e2e`, RF-A21) ni `pnpm install --frozen-lockfile` (lo corre la CI de la fase C).
- **Puntos de extensión:** B agrega `_check-rust` (fmt, clippy, `db-up`, tests, frescura de `just gen`) y lo suma a `check`. C agrega `hooks` y las recetas de imágenes, y **sí cambia `check`**: según RF-C6, `check-compose.mjs` corre dentro de `just check` (además de en el job `images`), así que C le suma esa línea. Solo `check-workflows.mjs` corre únicamente en el job `policy` (RF-C12), y C tendrá que excluir sus tests de `scripts/vitest.config.mjs` en el job `web` (RF-C10). Las recetas de A no dependen de Docker ni de Rust.
- `just fmt` = `prettier --write .`; `just test` = las dos corridas de Vitest; `just e2e` = `_web-prepare`, `pnpm --filter web build` y `playwright test`; `just e2e-install` = `playwright install chromium webkit`.

## Riesgos

- **Kit 3 es joven (decisión 1).** Salió el 2026-10-01; shadcn-svelte 1.7.0 y Paraglide 2.29.1 no se probaron con él. → El paso 2 es una prueba corta con punto de corte y sin respaldo preaprobado: si falla, el plan vuelve a revisión. `pnpm audit` va como evidencia en el PR.
- **Patrones de ESLint muertos con `#`.** Un `group` que empieza con `#` es un comentario en la sintaxis de `.gitignore` y nunca coincide, sin error. → Patrones `#lib` como `regex` y un caso del test 2 por patrón que solo ese patrón atrapa (decisión 18).
- **`handleError` asíncrono o que lanza.** En Kit 3, cualquiera de los dos acaba en un `console.warn` o en el error crudo por `console.error`, sin saneador. → Decisión 4 y su unit (no es `AsyncFunction`, no devuelve *thenable*, no lanza con un *getter* que lanza); el teardown de la lista blanca lo atraparía si se escapa.
- **Formularios del mismo origen sobre `http`.** Sin `paths.origin`, Kit compara contra `https://` + `Host`, así que en `node build` local y en el e2e un `POST` de formulario del mismo origen recibe 403 del chequeo de Kit antes de `handle`. En 001 no hay form actions. → Los e2e que necesitan llegar a `handle` con un `POST` (A16, A23) usan `Content-Type: application/json`, que Kit no revisa.
- **`urlPatterns` de Paraglide y rutas excluidas.** Si `deLocalizeUrl` o `paraglideMiddleware` tratan `/healthz` o `/api/**` como rutas con idioma, habría redirecciones o un `route.id` incorrecto (y entonces el guard por `route.id` fallaría). → Unit de `reroute` en el paso 4, `paraglideMiddleware` saltado en las rutas excluidas y el e2e de `/healthz` y del guard.
- **Fugas a los logs desde código ajeno.** Kit, adapter-node o Vite pueden escribir por `console` (avisos y errores no capturados). → `handleError` propio, síncrono y que nunca lanza (decisión 4; Kit no imprime el error si el hook existe y no falla), `process.on` instalados en `init` y el e2e de canarios, que mira **todo** stdout y stderr del proceso y no solo lo que pasa por `log.ts`. El `globalTeardown` revisa los archivos completos contra la lista blanca de líneas; la única excepción es una línea `Listening on` exacta, una vez y antes de cualquier petición (decisiones 11 y 12).
- **Saneador por lista negra.** Un mensaje de error puede traer datos que no siguen ese patrón (por ejemplo, identificadores en la ruta de una URL). → Es lo que acepta la spec para 001 ("Diseño → Logs"); los unit cubren IPv6 comprimida, URLs relativas y mayúsculas. La propuesta de ADR-0012 lo reemplaza.
- **Redirección abierta o inyección de cabeceras.** `//evil.example/` o `/\evil.example` podrían terminar en un `Location` que el navegador lea como otro host, y `paraglideMiddleware` responde con un `Location` absoluto armado desde `Host`. → `localeRedirect` arma el `Location` solo con `event.url.pathname` (sin barras ni `\` iniciales) y `event.url.search`, y nunca usa `new URL(path, base)`; el handle `paraglide` reescribe a relativo las redirecciones del middleware (decisión 2). Unit con `%0d%0a`, `//` y `/\`; e2e con el cliente HTTP crudo, con y sin `Sec-Fetch-Dest: document` y con `Host: evil.example`.
- **CSP y estilos en línea.** Si SvelteKit agrega un nonce a `style-src`, los navegadores ignoran `'unsafe-inline'` y Bits UI deja de posicionar los menús. → El e2e escucha `securitypolicyviolation` con un `dropdown-menu` y un `dialog` abiertos en el catálogo, en Chromium y WebKit. Si falla, se ajusta la opción `csp` de `sveltekit()` dentro de los límites de la decisión 16 (nunca `'unsafe-inline'`, `'unsafe-eval'` ni `'strict-dynamic'` en `script-src`) y se anota.
- **Contraste y paleta.** Llevar `--input` y `--ring` a ≥ 3:1 oscurece los bordes de los controles en el tema claro; es lo que pide WCAG 1.4.11 y no un error. → Test 3 automático; la identidad visual final llega con otra feature.
- **Desfase del registro de shadcn-svelte** (`cn`, `@lucide/svelte` 0.x y más primitivas con colores o anillos sueltos de los que nombra la spec). → Decisiones 5, 6 y 8; `check-tokens` y el test de foco lo detectan si se escapa algo.
- **Node local.** El owner tiene Node 18 por `nvm`, y `nvm` no lee `.node-version`; Kit 3 pide ≥ 22.17. → `engines` + `engine-strict` hacen fallar `pnpm install` con un mensaje claro; `CLAUDE.md` lo documenta.
- **`minimumReleaseAge`, `onlyBuiltDependencies: []` y `strictDepBuilds: true`.** Una dependencia con script de instalación o una versión nueva bloquean `pnpm install` (a propósito). → El paso 1 lo detecta; solo se agrega a la lista lo imprescindible, con su motivo en "Desviaciones" y nueva revisión de security-reviewer.
- **CLIs por `pnpm dlx` fuera del lockfile.** → Esqueleto a mano (decisión 13); `shadcn-svelte` solo en local, con `--no-deps`, y el paso 10 verifica que `package.json` y el lockfile no cambian y que `cn` no entra.
- **Medir "antes del primer pintado" (test 5).** → `page.addInitScript` registra un `requestAnimationFrame` que guarda `data-theme` en el primer frame; el script del `<head>` corre antes del primer pintado, así que el valor nunca puede ser `system`.
- **Tamaño del PR.** Unos 23 RF y 20 decisiones. → Commits por paso, y el orden de lectura del PR sigue el de este plan.

## Alternativas descartadas

- **SvelteKit 2.70.3 con una migración posterior a Kit 3:** el owner eligió Kit 3 desde el inicio para no migrar dos veces ni salir a producción con una rama de Kit que ya no recibe versiones (decisión 1; "Alternativas consideradas" de la spec).
- **`handleError` asíncrono:** en Kit 3.1.0, sin `experimental.async`, descarta el resultado durante el render y escribe un aviso; si rechaza, vuelca el error crudo (decisión 4).
- **Patrones `#lib/**` en `group` sin escapar:** nunca coinciden (decisión 18). Escapar con `\#` también funciona, pero `regex` deja la intención más clara.
- **Validar `PUBLIC_ORIGIN` con una librería de esquemas (Zod, Valibot):** una dependencia más para una sola regla; un validador a mano con su unit basta (decisión 19).
- **Estrategia de cookie de Paraglide (`PARAGLIDE_LOCALE`, `setLocale`)**: escribe la cookie con sus propios atributos y en momentos que no controlamos (RF-A4 pide "solo por elección explícita"). Ver la decisión 2 y "Diseño → Cookies" de la spec.
- **`webServer.stdout: 'pipe'` para leer los logs:** decisión 11.
- **Paquete `cn` del registro nuevo de shadcn-svelte:** decisión 5.
- **Plugin de inlang desde jsDelivr, aunque sea con versión exacta:** ejecuta JS remoto en el build sin lockfile, hash ni `minimumReleaseAge` (decisión 7).
- **`sv create` para el esqueleto:** `pnpm dlx` no respeta el lockfile (decisión 13).
- **`throw error(404)` en el guard:** renderiza el árbol de errores y pasa por `handleError`; el guard devuelve un `Response` 404 directo.
- **ESLint en la raíz para `scripts/`:** los scripts son pocos y tienen tests; Prettier en la raíz basta. ESLint queda con un solo `eslint.config.js`, en `apps/web`.
- **Guard por `url.pathname`:** RF-A9 y RF-A16 piden `route.id`, porque el pathname lleva el prefijo de idioma.
- **Comparación de píxeles en el test 8:** la spec la descarta ("Áreas de preocupación" 5).

## Pruebas (cómo se demuestra que funciona)

- [ ] `just check` en verde (salida pegada en el PR), y `just e2e` en verde en Chromium y WebKit.

| RF | Prueba | Nivel |
|---|---|---|
| A1 | `pnpm install --frozen-lockfile` en limpio con `strictDepBuilds: true`; `packageManager` con hash; `pnpm ignored-builds` vacío; `pnpm audit --audit-level moderate` en el PR; revisión del PR. En CI desde la fase C | manual · PR |
| A2 | Fixtures de `check-prerender.mjs` (`true` y `'auto'` fallan; `false` y la ausencia pasan); `svelte-check` y `vite build` en verde; test 2 con `$lib`, `$lib/**`, `$env/**` y `$app/environment` (A14); prueba corta del paso 2 | unit · `just check` · PR |
| A3 | e2e: `/` y `/dev/components` sin prefijo → 307 con `Location` `/es/…` o `/en/…` según cookie → `Accept-Language` → `en`; el query se conserva; `//evil.example/` y `/\evil.example` (cliente crudo) → `Location` empieza con `/es/` o `/en/`; `<html lang>` según la ruta; `/healthz`, `/api/x` y `/ws` sin redirección y `/apix` con redirección; la 307 trae `Vary: Cookie, Accept-Language` y `Cache-Control: private, no-store`. **Camino de Paraglide:** los mismos casos más `/es`, `/en`, `/es//evil.example/` y `/EN/`, con `Sec-Fetch-Dest: document` y `Host: evil.example` (cliente crudo): ningún `Location` es absoluto ni apunta a otro host. Unit de `pickLocale`, `reroute`, `localeRedirect` (`%0d%0a`, `//`, `/\`, exclusiones por segmento), `localizedHref()` (`//…`, `/\…`) y la reescritura del handle `paraglide` | unit · e2e |
| A4 | e2e: tras elegir en el selector, la cookie `locale` trae `Path=/`, `Max-Age=31536000`, `SameSite=Lax`, sin `HttpOnly` (y `Secure` solo en https; se comprueba en el unit de `writeLocaleCookie` con `location.protocol` simulado); visitar `/en/` por un enlace no cambia la cookie; `locale=xx` se ignora | unit · e2e |
| A5 | e2e: elegir `es`, ir a `/` con `Accept-Language: en` → `/es/`; el cambio conserva ruta y query | e2e |
| A6 | Unit de `resolve-theme` (tabla: ausente, `dark`, `light`, `system`, `DARK`, `"><script>`, vacía) → nunca sale un valor fuera de la lista; e2e test 5 | unit · e2e |
| A7 | e2e test 5 con la CSP activa en Chromium y WebKit; un unit comprueba que el script de `app.html` pesa ≤ 1024 bytes | unit · e2e |
| A8 | e2e: cambiar el tema en el selector cambia `data-theme`, `color-scheme` y `theme-color` sin recargar, y escribe `theme` con sus atributos | e2e |
| A9 | e2e test 6: `/es/dev/components?theme=light` cambia el tema en la instancia con catálogo; `/es/?theme=light` no tiene efecto; no se escribe cookie | e2e |
| A10 | e2e: sin `securitypolicyviolation` en `/es/`, `/en/` y el catálogo (con un menú y un diálogo abiertos); `nosniff`, `Referrer-Policy` y HSTS en una 200, una 307, una 403 del guard de `Origin` y una 404. Unit de `securityHeaders` con cabeceras inmutables. Fixtures de `check-kit-config.mjs` (`trustedOrigins`, `checkOrigin`, `paths: { origin }` y `svelte.config.js` fallan; la configuración real pasa) | unit · e2e · `just check` |
| A11 | Test 3 (Vitest): todos los pares de ADR-0009 en los dos temas y `theme-color.ts` contra `--background` | unit |
| A12 | Tests 1, 3, 4 y 9; e2e de foco (Tab por el catálogo; cada elemento enfocado tiene `box-shadow` distinto de `none`); e2e de 44 px (los interactivos del catálogo en tamaño por defecto y los de la cabecera miden ≥ 44×44) | unit · e2e |
| A13 | Test 1 con un fixture por caso de ADR-0009 y `ring-ring/50` | unit |
| A14 | Test 2 con los casos del paso 9 (imports relativos y `console.log` incluidos) | unit |
| A15 | Fixtures de `check-i18n.mjs`: llave solo en `es`, mensaje vacío, texto literal, `aria-label`/`title`/`alt`/`placeholder` literales → fallan; lo mismo en `src/routes/dev/**` → pasa (criterio A1 del intent) | unit |
| A16 | Unit del guard (`Response` 404 directo, sin `resolve` ni `handleError`); e2e test 6 contra `:4174` (sin la variable): `GET` a `/es/dev/components` y `/es/dev/error-probe`, `GET /es/dev/components/__data.json` y un `POST` a `/es/dev/error-probe` con `Content-Type: application/json` y `Origin` = `PUBLIC_ORIGIN` de esa instancia (pasa el chequeo de Kit, que no revisa JSON, y el guard de RF-A23, y llega al de `/dev/**`) → 404, y el log de esa instancia no crece durante las peticiones ni tiene líneas de petición en el teardown. `/es/dev/status` también se pide, pero **en la fase A esa ruta todavía no existe** (llega en B): su 404 es por ruta inexistente y no cuenta como cobertura del guard | unit · e2e |
| A17 | e2e: inicio en `/es/` y `/en/`; 404 traducido con título y texto; test 4 (axe, 0 `serious`/`critical`) en el inicio y en el 404 con `theme=light` y `theme=dark`, y en el catálogo con `?theme=light` y `?theme=dark` | e2e |
| A18 | e2e: `GET /healthz` → 200 sin `Location` y sin líneas nuevas en el log | e2e |
| A19 | e2e de canarios (A5 del intent) contra `:4173` con `ADDRESS_HEADER=X-Forwarded-For` y `XFF_DEPTH=1`: peticiones a `/es/` y a `/es/dev/error-probe?q=<canario>` con `Cookie`, `Authorization`, `X-Internal-Secret`, `X-Client-Ip` y `X-Forwarded-For: 203.0.113.77`. Ninguna línea nueva contiene esos valores, la IP de canario del mensaje ni el query; la línea de error trae `error_id` (el mismo que muestra la página) y `route_id` `/dev/error-probe`, y solo los campos permitidos. **El HTML de la página de error tampoco contiene los canarios** (`message` es un código fijo). `GET /es/no-existe` → 404 sin líneas nuevas y sin `error_id` en la página. En el `globalTeardown`, los archivos completos de las dos instancias pasan la lista blanca de líneas y no contienen ningún canario. Unit del saneador y unit de `handleError` por `kind`, síncrono y sin lanzar (paso 5) | unit · e2e |
| A20 | Lo usa el e2e de A19; el guard lo cubre en A16 | e2e |
| A21 | `just check` falla en el primer paso roto (lo pruebo una vez rompiendo a propósito una llave de `es.json`: falla `check-i18n`; pego la salida en el PR) | PR |
| A22 | Revisión de `CLAUDE.md` y del aviso | PR |
| A23 | Unit de `originGuard` con la tabla completa y el espía de `resolve`, y unit de los validadores, incluido que el mensaje no contiene la entrada (paso 6). e2e contra `node build` (cliente crudo): `POST /es/` con `Content-Type: application/json` y `Origin: https://evil.example` → 403 con cuerpo exactamente `{"error":"auth.csrf_origin"}`; `POST /es/` con `Content-Type: application/json`, sin `Origin` ni `Sec-Fetch-Site` → 403 con el mismo cuerpo (con JSON, Kit no lo rechaza antes, así que el 403 solo puede venir del guard); `POST /healthz` con `Content-Type: application/json` y `Origin` = `PUBLIC_ORIGIN` → 405; ninguna línea nueva en el log durante esas peticiones. El arranque sin `PUBLIC_ORIGIN` termina con código distinto de 0 (paso 2; en la fase C, el smoke test de RF-C2) | unit · e2e |

**Tests de ADR-0009 que cubre esta fase:** 1 (A13), 2 (A14, con los fixtures `#lib` de la enmienda), 3 (A11), 4 (A17), 5 (A6/A7), 6 (A9/A16), 8 (capturas) y 9 (reduced motion: con `reducedMotion: 'reduce'`, el `transition-duration` y el `animation-duration` calculados de un botón y de un diálogo abierto del catálogo son ≤ 0.01 ms). El 7 es del renderer y no entra.

- [ ] **Capturas (test 8 y RNF de 375 px):** catálogo a 375×812 y 1280×800 con `?theme=light` y `?theme=dark`; además, inicio en `es` y `en` y el 404, con los dos tamaños y los dos temas. En cada una, `document.documentElement.scrollWidth <= clientWidth`. Son artefactos para revisión, no comparación de píxeles.
- [ ] **Evidencia en el PR:** salidas de `just check`, `just e2e` y `pnpm audit --audit-level moderate`, la prueba de A21, la prueba corta del paso 2 (alias que propuso `init`, `svelte-check`, `vite build` y el arranque sin `PUBLIC_ORIGIN`), el diff vacío y el `pnpm why cn` vacío del paso 10, las capturas y el extracto del log de canarios (los dos archivos completos que revisó el teardown).

## Estimación de CI

Sin cambios: la fase A no agrega workflows. Los jobs `web` y `e2e` llegan con la fase C (RF-C10), que reutiliza `just check` (con la línea de `check-compose.mjs` que C le suma, RF-C6) y `just e2e` tal como quedan aquí. Como referencia para C, en local `just check` debería tardar menos de 1 min y `just e2e` unos 2 a 3 min con los navegadores ya instalados.

## Desviaciones durante la implementación

1. **Paso 1, `pnpm ignored-builds`.** Con pnpm 10.34.7 el comando imprime "Cannot identify as no node_modules found" aunque `node_modules` existe. La comprobación equivalente es que `node_modules/.modules.yaml` no tiene la clave `ignoredBuilds`, y que `strictDepBuilds: true` habría hecho fallar `pnpm install` si alguna dependencia tuviera scripts. Las fechas de la tabla de versiones se volvieron a comprobar el 2026-10-23 00:03 UTC y no cambiaron; Node 22.23.4 sigue siendo la última 22.x.
2. **Paso 2, peer opcional de `runed`.** `bits-ui@2.20.0` depende de `runed@0.35.1`, que declara `@sveltejs/kit ^2.21.0` como peer **opcional**. pnpm avisa (`unmet peer`), pero el único módulo de `runed` que importa Kit es `useSearchParams` (`$app/environment`, `$app/navigation`, `$app/state`), y `bits-ui` no lo usa. Si una primitiva futura lo usara, el build fallaría en vez de fallar en silencio.
3. **Paso 2, CLI de shadcn-svelte 1.7.0.**
   - `add` **no tiene `--no-deps`**; solo `--no-deps-install` (escribe las dependencias en `package.json` sin instalar). Se usa `add --no-deps-install -y`, y la verificación de que `package.json` y el lockfile no cambian se mantiene (con `button`, `package.json` quedó idéntico; si un `add` futuro lo cambia, se revierte con git antes del commit).
   - `init` exige elegir un *preset* de forma interactiva (`--preset` espera un código de shadcn-svelte.com/create, no un nombre). En la copia desechable se aceptaron los valores por defecto con `expect`. Propuso `#lib` en los cinco alias, más `style: "vega"`, `iconLibrary`, `menuColor` y `menuAccent`, que no se copian a nuestro `components.json` (no hacen falta para `add`).
   - En una carpeta sin `packageManager`, `corepack pnpm dlx` resuelve pnpm 12.15.0. Para la copia desechable se usó `pnpm@10.34.7` explícito, y en el repo los shims de corepack respetan `packageManager`.
   - **El registro cambió de estilo.** `init` en limpio añade `cn@^0.5.0`, `@lucide/svelte@^1.61.0`, `@fontsource-variable/inter` y `shadcn-svelte/tailwind.css`, y el `button` actual (estilo `vega`) trae `h-8`, `focus-visible:ring-3`, `focus-visible:ring-ring/50` y `hover:bg-[color-mix(…)]`, distintos de lo que supone la tabla de la decisión 8. Nada de eso entra: `utils.ts` sigue a mano (decisión 5), no se añaden dependencias, y la revisión del paso 10 ajusta cada primitiva a los tokens, al anillo de D6 y a 44 px, ampliando la tabla de la decisión 8 con lo que haga falta.
4. **Paso 4, barra final.** Kit 3 normaliza la barra final **antes** de `handle` (por defecto `never`): `/es/` respondía 308 a `../es`, sin pasar por nuestros handles (y por tanto sin las cabeceras de RF-A10). Como RF-A3 redirige a `/<locale>/`, el `+layout.ts` raíz declara `trailingSlash = 'ignore'`: `/es/` y `/es` se sirven igual, sin redirección. No hay otros archivos `+layout.ts`/`+page.ts` que lo cambien.
5. **Paso 4, otros detalles de Kit 3 y Paraglide.** `sequence` y los tipos `Handle`, `HandleServerError` y `Reroute` se importan de `@sveltejs/kit/hooks`. `event.request` es `readonly` en Kit 3, así que el handle `paraglide` no reemplaza la petición por la delocalizada (no hace falta: el enrutado ya lo hace `reroute`). Con `urlPatterns` propios, el runtime de Paraglide importa `@inlang/paraglide-js/urlpattern-polyfill`, que queda empaquetado en el build. `_web-prepare` compila Paraglide con `compile(options)` de `@inlang/paraglide-js` y `paraglide.options.js`, porque el CLI no acepta `urlPatterns`. La prueba de Paraglide con Kit 3 pasó: compila con el plugin local, `<html lang>` sigue a la ruta, `reroute` deja `/healthz` y `/api/x` sin cambios y el `Location` de las redirecciones contra `node build` con `Host: evil.example` y `Sec-Fetch-Dest: document` siempre es relativo.
6. **Paso 5, saneador.** Además de los `?` pegados a una URL o ruta, el saneador quita cualquier `?nombre=…` aunque vaya tras un espacio, y el fragmento (`#…`) que siga al query. Es más estricto que lo pedido.
7. **Paso 7, tema.** El `+layout.server.ts` raíz devuelve `locals.theme` (el tema con el que se pintó el SSR) para que la runa arranque sincronizada; `App.Locals` declara `theme`. La paleta neutra usa primitivas `--palette-neutral-*`, `-red-*`, `-green-*`, `-amber-*` y `-slate-*` con pasos de 50.
8. **Paso 9, ESLint.**
   - **Versión real:** el comportamiento del `#` como comentario de `.gitignore` se verificó con ESLint **10.13.0** y typescript-eslint **8.72.0**: `group: ['#lib/features/**']` no coincide; `\\#lib/…` y `regex: '^#lib/features/'` sí. El test 2 lo cubre con la configuración sin formas relativas (`createConfig({ relativePatterns: false })`).
   - `eslint.config.js` exporta `createConfig()` además del default, para que el test 2 pueda desactivar las formas relativas.
   - Se desactiva `svelte/no-navigation-without-resolve` (del preset recomendado de eslint-plugin-svelte 3.23): nuestras rutas llevan prefijo de idioma y se arman con `localizedHref()`, no son `route.id`, y las primitivas solo reenvían `href`.
   - Los selectores de la decisión 20 no se aplican a `src/**/*.test.ts`, que leen fixtures con `new URL(ruta, import.meta.url)`. En código de la app, el selector sobre un `Identifier` llamado `url` obligó a renombrar una variable del validador de `PUBLIC_ORIGIN` (falso positivo esperado, sin cambiar la regla).
   - Paquete del renderer en las reglas: `@torneo/renderer` (y `**/packages/renderer/**`), que todavía no existe; el nombre se confirma cuando llegue el paquete.
9. **Paso 10, `add` de shadcn-svelte.** Como `dialog` depende de `button`, el CLI pide confirmar la sobrescritura; se usó `add --no-deps-install -y -o` (el botón aún no tenía cambios propios). El CLI escribió `"@lucide/svelte": "^1.61.0"` en `apps/web/package.json`; se revirtió con `git checkout` y los iconos que usan las primitivas (`check`, `chevron-right`, `minus`, `x`) existen en 0.561.0. Tras el revert, `git diff --exit-code package.json apps/web/package.json pnpm-lock.yaml` sale vacío y `pnpm why cn` (raíz y `--filter web`) no devuelve nada.
10. **Paso 11, cabecera.** El enlace de cada idioma lleva `data-sveltekit-reload`: el cambio de idioma recarga la página entera, porque `<html lang>` y los mensajes del layout salen del SSR. `ThemeSwitcher` recibe el tema del SSR por prop y solo lee la runa después de montarse (la runa es estado de módulo y en el servidor se compartiría entre peticiones). Se añadió un enlace "saltar al contenido" y los mensajes `skip_to_content`, `header_label` y `error_page_title`. A 375 px los dos selectores comparten fila y la marca pasa a la línea de arriba.
11. **Paso 12, e2e.** `workers: 1` y `fullyParallel: false`, porque los tests leen los logs de instancias compartidas. El proyecto `webkit` corre `theme.spec.ts` y `csp.spec.ts`. Un unit (`tests/e2e-log-whitelist.test.ts`) cubre la lista blanca que usa el teardown, para que no pueda quedar muerta sin que se note.
12. **Paso 15, Prettier y archivos de inlang.** La compilación de Paraglide crea `apps/web/project.inlang/{.gitignore,.meta.json,README.md,cache,.lix}`; inlang los ignora con su propio `.gitignore` anidado, que Prettier no lee. En el clon limpio `prettier --check` fallaba por `.meta.json`; `.prettierignore` ignora ahora todo `project.inlang/` salvo `settings.json`. Lo detectó la corrida desde el clon limpio.
13. **Revisión de QA, H1 (saneador).** Una IPv4 incrustada en IPv6 (`::ffff:203.0.113.77`, `64:ff9b::…`, la forma completa `0:0:0:0:0:ffff:…`) dejaba restos (`.0.113.77`), porque la regex de IPv6 se comía el prefijo antes de que corriera la de IPv4. Ahora esas formas se reconocen enteras antes que las demás, y los unit comprueban que no sobrevive ningún fragmento de la dirección.
14. **Revisión de QA, H2 (cambia la decisión 3, handle 4).** Los 404 de las rutas excluidas (`/api/**`, `/ws`, `/healthz/…`) salían con `<html lang="%paraglide.lang%">`, porque el handle `paraglide` se saltaba el middleware y nadie reemplazaba el marcador. Ahora el middleware corre también en las rutas excluidas, con una URL efectiva (`effectiveRequestUrl`) que lleva el prefijo del idioma elegido por la cookie `locale` válida o, si no, `en`. Así `lang` y los mensajes de esa página coinciden, y el middleware no puede redirigir esas rutas porque la URL ya trae el idioma (unit con `Sec-Fetch-Dest: document` en `/healthz`; el `Location` se sigue reescribiendo a relativo). e2e con GET y POST JSON sobre `/api`, `/api/x`, `/ws`, `/healthz/x` y `/healthz//`, y axe sobre el 404 de `/api/x`.
15. **Revisión de QA, H4.** El e2e "Paraglide's own redirects are relative" no ejercitaba a Paraglide: con `['url', 'baseLocale']` el middleware nunca redirige (sin prefijo responde antes `localeRedirect`; con prefijo, la URL decide). Se reescribió para fijar, caso por caso, el estado, el `Location` exacto y que toda 307 sale de `localeRedirect` (sus cabeceras `Cache-Control` y `Vary`), sin `continue`. La reescritura incondicional del `Location` en el handle `paraglide` se mantiene como defensa.
16. **Revisión de QA, H3 (cabeceras fuera de `handle`).** Hay respuestas que no pasan por `handle` y por tanto salen **sin** `nosniff`, `Referrer-Policy` ni HSTS (RF-A10):
    - la 308 de normalización de la barra final de Kit en endpoints (`/healthz/` → `/healthz`; el `trailingSlash = 'ignore'` del `+layout.ts` raíz solo cubre páginas);
    - los estáticos de `/_app/*` (y de `static/`), que adapter-node sirve con `sirv` antes de Kit.

    No contienen datos de la app ni ejecutan código de rutas; se acepta en la fase A. **En la fase C, el proxy (Traefik) también pone estas cabeceras** en el router de `web`, para cubrir estos casos; el plan de C debe incluirlo y verificarlo con un `curl` a `/_app/version.json` y a `/healthz/`.
17. **Revisión de QA, nota de credenciales.** El saneador también quita la parte `usuario:contraseña@` de cualquier URL (`postgres://user:pass@host` → `postgres://[redacted]@host`), con su unit.
18. **Nits de las revisiones del PR #1 (TA-10).**
    - **Saneador.** El valor de un header sensible se redacta también cuando viene en un arreglo (de una o varias líneas) o en una entrada de `Map` (`=>`). Las credenciales se cortan en el último `@`, aunque la contraseña tenga `/` o `@` sin codificar; la autoridad vacía (`file:///…`) no se toca. Se quitan los fragmentos `#…` sin query, salvo después de un `.` (`Foo.#bar` en las trazas). La IPv4 pegada a una letra o a `_` se redacta. La entrada se corta a 8 KB, y se descarta también el último token, para que el corte no deje un trozo de IP o de credencial. Las regex de IPv6 se anclan con `(?<![0-9a-f:])` y el esquema de las credenciales se acota, porque eran cuadráticas. Un unit comprueba que 160 KB de entrada adversaria tardan menos de 50 ms.
    - **ESLint.** `ui/` no puede importar nada fuera de `components/ui/` (`^#lib/components/(?!ui/)`), en lugar de una lista cerrada de dominios. La forma relativa depende de la profundidad del archivo, así que hay un objeto por nivel hasta `UI_MAX_DEPTH`, y un test falla si una primitiva queda más abajo. La regla de URLs absolutas (decisión 20) atrapa también la desestructuración de `origin`, `host` y `href`, `new URL(<x>.url)` y las lecturas de `Host`, `Forwarded` y `X-Forwarded-*`. El `new URL(event.url)` de `paraglide.ts` lleva un `eslint-disable` con su motivo: solo alimenta la detección de idioma, y `relativizeLocation` reescribe cualquier `Location`.
    - **Orden de los handle.** Dos e2e: `POST /` y `POST /es/dev/components` con `Origin` ajeno dan 403 (no 307 ni 404). Fallan si el guard se mueve detrás de `localeRedirect` o `devGuard`.
    - **`check-i18n`.** Detecta los literales que una expresión muestra tal cual: `{"…"}`, plantillas, las dos ramas de `?:`, `&&`/`||`/`??` y `+`, en el texto y en `aria-label`/`title`/`alt`/`placeholder`. Los literales que se pasan a funciones o a comparaciones siguen permitidos.
    - **Scripts de `apps/web`.** `pnpm run sync` (`scripts/compile-paraglide.js` y `svelte-kit sync`) corre antes de `check`, `lint`, `test`, `dev` y `build`, y `e2e` hace el build primero. `just _web-prepare` llama a ese mismo script, así que hay un único punto de entrada y `pnpm --filter web check` funciona en un clon limpio.
    - **Segunda ronda (revisiones del PR #2).**
      - El valor de un header sensible se lee por su estructura, en una sola pasada lineal. Un string entre comillas se toma completo, con sus escapes; un arreglo, hasta el `]` que lo cierra, aunque haya `]` dentro de un elemento; cualquier otro valor, hasta el fin de línea. Si un string o un arreglo no cierra, se redacta hasta el final.
      - El ancla de IPv6 pasa a `(?<![0-9a-f]|[0-9a-f]:)`, así que `peer:2001:db8::1` vuelve a redactarse.
      - El truncado retrocede hasta el último separador (`[\s"'<>,;]`), no solo hasta el último espacio.
      - Cada patrón se mide con 8 KB de entrada adversaria.
      - La regla de `ui/` atrapa también las rutas relativas que suben hasta `lib/` y vuelven por `components/<x>/`, con una regex que no depende de la profundidad.
      - La regla de URLs absolutas agrega `hostname`, `port` y `protocol`, el acceso con corchetes, `toJSON()`, `URL.parse`, `'' + url`, `new Request(url)`, `headers.get` con plantilla y `Object.fromEntries(headers).host`.
      - `check-i18n` detecta los literales dentro de arreglos.
      - Quedan sin cambiar dos casos en que se redacta de más: una URL con puerto y una `@` en la ruta (`http://localhost:5173/…/@x`) se toma como credencial, y `C#9` pierde el `#9`. Corregirlos sin conocer la URL haría que dejaran de redactarse contraseñas o fragmentos poco comunes, y aquí se prefiere redactar de más.

## Revisión

### architect — 2026-10-21 — APPROVE

- **Verifiqué:** comparé el plan con la spec aprobada, de RF-A1 a RF-A22, con los RNF de `web` y con la "Estrategia de pruebas". Lo revisé también contra ADR-0003, ADR-0007 y ADR-0009 (D1 a D8) y contra `docs/process.md`. El frontmatter es válido: `type: plan`, `risk: high`, los reviewers son architect y security-reviewer, y el autor es frontend-engineer, no yo. Contrasté contra el registro real de npm y de shadcn-svelte las afirmaciones que sostienen las decisiones 1 y 5. `@sveltejs/kit` `latest` es 3.1.0 (3.0.0 salió el 2026-10-01). La 2.70.3 es del 2026-08-18 y sus peers aceptan Vite `^8`, TS `^6` y `vite-plugin-svelte` `^7`. `registry/utils.json` de shadcn-svelte exporta `cn` desde el paquete `cn@^0.3.0`, que está en npm desde 2013. Revisé también qué paquetes y archivos traen `button`, `input`, `badge`, `dialog` y `dropdown-menu` en el registro.

**Puntos que pidió el orquestador**

1. **Kit 2.70.3 en vez de Kit 3.1.0: lo acepto.** Kit 3 obligaría a reabrir una spec `risk: high` ya aprobada por el owner:
   - RF-A10 nombra `csrf.checkOrigin`.
   - RF-C2 y RF-C4 usan `ORIGIN`.
   - ADR-0009 D2 y D4 fijan alias y patrones de ESLint sobre `$lib`.

   La fase A no despliega, y el primer despliegue ya está bloqueado por los límites anónimos de ADR-0008 D6 y D7 (RF-C18). ADR-0003 no fija la versión mayor, así que no hay conflicto con el ADR.

   **El issue de migración sí hace falta, y debe bloquear el issue "Primer despliegue a producción".** No basta con que vaya "antes" sin más. Pido tres cosas:
   - (a) Abrirlo cuando se apruebe este plan y no en el paso 15, para que `plan-c` pueda citarlo y lo agregue a los prerrequisitos de RF-C18.
   - (b) Que diga que la migración enmienda la spec (RF-A10, RF-C2, RF-C4 y el smoke test de RF-C2) y ADR-0009 D2/D4 (`$lib` → `#lib`). Por tocar un ADR, necesita el OK del owner.
   - (c) Programarlo después de la fase C. Así C se escribe contra la spec vigente y la migración toca todo de una vez.

2. **Locale con código propio: lo acepto, y la spec lo obliga.** El `paraglideMiddleware` redirige con `Response.redirect`, que deja el `Location` absoluto, y RF-A3 lo pide relativo. La estrategia de cookie de Paraglide no garantiza la regla de RF-A4 de escribir la cookie "solo por elección explícita" con nombre `locale` y 1 año ("Diseño → Cookies"). ADR-0007 se cumple: Paraglide sigue manejando los mensajes y el idioma por URL, y la precedencia es cookie → `Accept-Language` → `en`. Es código sensible porque puede abrir una redirección: lo cubren el unit de `pickLocale`, el e2e con cliente HTTP crudo y la regla de no usar `new URL(path, base)`. Condición: `LocaleSwitcher` no puede llamar a `setLocale` de Paraglide en ningún caso.

3. **Registro de shadcn-svelte.**
   - **`cn` frente a `clsx` + `tailwind-merge`:** acepto la decisión 5. ADR-0009 D2 manda, y lo confirmé en el registro. Cambiar al paquete `cn` necesitaría enmendar el ADR, y no vale la pena en 001.
   - **Riesgo que el plan no cierra:** `add` instala las `devDependencies` de cada registro con rango (`bits-ui@^2.14.4`, `tailwind-variants@^3.2.2`, `@lucide/svelte@^0.561.0`). Eso podría mover versiones de la tabla o, en una corrida futura con `init` o `utils`, meter `cn`. En el paso 10, corre `add` con `--no-deps`, o con el lockfile ya fijado. Su verificación debe incluir: `git diff package.json pnpm-lock.yaml` vacío y `pnpm why cn` sin resultado.
   - **Inconsistencia menor:** "No verificado (a)" habla de `init`, pero el paso 10 no lo ejecuta (escribe `components.json` y `utils.ts` a mano). Deja solo `add`.
   - **Primitivas `MODIFICADO`:** la lista de la decisión 8 es correcta y amplía RF-A12 sin contradecirlo. Para que la revisión del diff no dependa de la memoria, pide la lista por archivo:
     - `dialog-overlay.svelte`: `bg-black/50`.
     - `dialog-content.svelte`: botón de cerrar, `focus-visible:`, `size-11` y la duración.
     - `button.svelte`, `badge.svelte`, `input.svelte`.
     - En `dropdown-menu`, qué archivos llevan `min-h-11`. Deberían ser `-item`, `-checkbox-item`, `-radio-item` y `-sub-trigger`, porque `ThemeSwitcher` usa items con `aria-checked`.

     No bloquea: el e2e de 44 px y el de foco atraparían lo que se escape.

4. **Orden de los `handle`: correcto.** Es `securityHeaders` → `localeRedirect` → `paraglide` → `devGuard` → `theme`.
   - `securityHeaders` solo actúa después de `resolve`. Por eso no contradice "después de Paraglide y antes de todo lo demás" (RF-A16) y además cubre la 307 y la 404 del guard.
   - `route.id` ya existe en `handle`, resuelto con `reroute`/`deLocalizeUrl`, así que el guard compara la ruta sin prefijo.
   - `sequence` encadena los dos `transformPageChunk` (Paraglide y tema).
   - Detalle para el unit del guard: debe responder con un `Response` 404 directo y no con `throw error(404)`, para no renderizar el árbol de errores ni pasar por `handleError`.

5. **`just check`: cumple RF-A21** (orden, corte en el primer fallo, `_web-prepare` primero, Playwright fuera). La partición `_check-web`/`_check-rust` encaja con RF-B19 y con el job `web` de RF-C10. **Hay un error de hecho que hay que corregir:** el plan dice que C "no cambia `check`" y que `check-compose.mjs` corre solo en su job. RF-C6 dice que `check-compose.mjs` corre **en `just check`** y en `images`. Solo `check-workflows.mjs` va únicamente en `policy`, y C tendrá que excluir sus tests de `scripts/vitest.config.mjs` en el job `web` (RF-C10). No afecta lo que A implementa. Es una línea y se corrige antes del merge del PR de A, para que `plan-c` no herede el error.

6. **Cada RF-A tiene prueba:** la tabla cubre de A1 a A22, con el nivel que fija la spec. A1 y A21 quedan en PR hasta que la CI llegue con C, lo que es coherente con la spec. Una nota: en la fase A, `/es/dev/status` todavía no existe (llega en B), así que su caso en el e2e de A16 da 404 por ruta inexistente y no por el guard. No pasa nada, porque `components` y `error-probe` sí prueban el guard. Basta con anotarlo para que nadie lo lea como cobertura.

7. **Pasos:** son 15 y casi todos tienen un comando de verificación. Hay un punto de corte explícito en el paso 2 con un respaldo concreto (Vite 7.3.7 / TS 5.9.3), y se marca qué pasos se pueden hacer en paralelo. El paso 11 solo se verifica a mano, pero el 12 lo automatiza enseguida. Lo acepto.

8. **Nombre `plan-a.md`: lo acepto.** La spec aprobada define tres fases con un PR cada una y dice que "las fases B y C tienen sus propios planes" (y C puede partirse en C1/C2). Un solo `plan.md` no se podría revisar ni aprobar por fase. Para que no quede como una excepción sin dueño, hay que registrar la convención fuera de este PR:
   - En `docs/process.md`, "Estructura de una feature": `plan-<fase>.md` cuando la spec define fases. Es un cambio de proceso y necesita el OK del owner.
   - En `write-plan` y `feature-pipeline`, que hoy dicen `plan.md`.
   - En la regla de Linear: TA-1 pasa a *In Progress* cuando se aprueba `plan-a`.

**Fuera de lo pedido:** no encontré contradicciones con ADR-0007 ni con ADR-0009. El dominio no se toca: no cambian `crates/*`, `packages/protocol-ts` ni `content/`, y la UI no lleva texto literal (`_a11y-messages.ts` y `check-i18n` con el parser de Svelte).

### security-reviewer — 2026-10-21 — APPROVE
**🤖 security-reviewer**

- Verifiqué:
  - **Trazabilidad.** Leí el plan entero y lo comparé con la spec aprobada: RF-A1 a RF-A22, el RNF de seguridad, "Diseño → Cookies" y "Diseño → Logs", y mis rondas 1 y 2. También lo comparé con ADR-0008 D3 (de esa parte, a `web` solo le toca la redacción de logs), con ADR-0009 D2, D3 y D5 y con el aviso de privacidad (sección 6 y puntos 5 y 8 de "Pendiente"). El frontmatter es válido, con `risk: high` y security-reviewer entre los revisores. No contradice ningún ADR aprobado.
  - **Kit 2.70.3.** Consulté los avisos de GitHub (`gh api /advisories`) de `@sveltejs/kit` y `@sveltejs/adapter-node`. El último es GHSA-29g2-3rmr-qm68 (ReDoS en `Accept`), se corrigió en 2.70.2 y no afecta a la 2.70.3. Hoy no hay ningún aviso abierto que solo arregle Kit 3.
  - **Hash de pnpm.** El hash de `packageManager` coincide byte a byte con el `dist.integrity` de `pnpm@10.34.7`, pasado de base64 a hex.
  - **Fechas de publicación.** Las de `@inlang/plugin-message-format@4.5.5` (2026-10-20 22:48 UTC), `bits-ui@2.20.0`, `@inlang/paraglide-js@2.29.1`, `typescript-eslint@8.72.0`, `vite@8.3.4` y `svelte@5.57.2` superan las 24 h a las 05:08 UTC de hoy.
  - **Scripts de instalación.** `@tailwindcss/oxide@4.3.3`, `rolldown`, Kit y Paraglide no tienen scripts de instalación. `esbuild` sí tiene, pero en Vite 8 solo es una dependencia opcional (*peer*).
  - **Plugin de inlang.** Revisé el código de `@inlang/sdk@4.0.4`, que es el que trae Paraglide 2.29.1. `importLocalPlugins` lee del disco cualquier módulo que no empiece con `http`, con la ruta relativa al padre de `project.inlang` (`apps/web`). El `dist/index.js` del plugin 4.5.5 viene empaquetado y no tiene imports. **Las rutas locales funcionan**, así que la decisión 7 se puede marcar como verificada.
  - **Telemetría.** Ni el SDK ni Paraglide la tienen (busqué `posthog` y `telemetry`).
  - **`paraglideMiddleware`.** Revisé su código: sin estrategia `cookie` no escribe cookies. Además puede responder él mismo con un 307 con `Location` absoluto (punto 4).
  - **Orden de los `handle`.** `securityHeaders` y `localeRedirect` no ejecutan código de la ruta ni escriben logs. El guard decide por `route.id`, que Kit ya resolvió con `reroute` antes de `handle`, y cubre `__data.json`. Lo acepto como lectura válida de "antes de todo lo demás" en RF-A16.
  - **Redirección propia.** `/<locale>/` es un prefijo fijo, y después van la ruta sin `/` ni `\` iniciales y el query. Para `//evil.example/` y `/\evil.example`, el parser WHATWG convierte `\` en `/`, así que las dos llegan como `//…` y el recorte las cubre. Con `pathname` y `search` ya codificados no hay inyección de cabeceras.
  - **CSP.** Kit no agrega nonce a `style-src` si la directiva lleva `'unsafe-inline'`. Aun así, el e2e de `securitypolicyviolation` con un menú y un diálogo abiertos, en Chromium y WebKit, es la prueba correcta.
  - **Cookies.** `theme` y `locale` se validan contra una lista cerrada antes de usarse. Con eso también quedan neutralizadas las cookies que pudiera plantar un subdominio hermano.
  - **HSTS.** Llega a las respuestas 200, 307 y 404, sin `includeSubDomains`.
  - **`handleError`.** Registrar solo `status >= 500` es correcto y cumple RF-A19.

**Hallazgos (todos Minor, no bloquean):**

1. **Kit 2 sin releases (decisión 1): es aceptable porque 001 no despliega, pero el bloqueo tiene que ser vinculante.** Hoy depende de que "la fase C lo anote", y RF-C18 no lo incluye.
   - *Escenario:* sale un aviso que solo se corrige en Kit 3, por ejemplo en la negociación de contenido o en el CSRF. El primer despliegue se hace con su checklist de RF-C18/C19, que no menciona Kit, y `web` sale vulnerable.
   - *Cambio:* en el paso 15, el issue "Migrar web a SvelteKit 3" se crea como **bloqueante** del issue "Primer despliegue a producción". Si ese issue todavía no existe, se crea ya y la fase C lo reutiliza. La migración vuelve a verificar RF-A10 (CSRF con la API nueva), el filtro de `handleError` (en Kit 3 entran todos los errores) y el guard.
   - Agregar `pnpm audit --audit-level moderate` (o la consulta de avisos de arriba) como evidencia en el PR de A.

2. **Plugin de inlang: quitar la alternativa de jsDelivr (decisión 7).**
   - Ya está verificado que las rutas locales funcionan, así que la alternativa sobra. Además es peligrosa: ejecuta en el build un JS remoto sin comprobar su integridad. Se salta el lockfile, el hash y `minimumReleaseAge`, y contradice la postura de cadena de suministro de la spec ("Áreas de preocupación" 11, el checksum de `zig`).
   - *Escenario:* en la fase C, el build de la imagen de `web` descarga el plugin. Si jsDelivr o su réplica de npm sirve un archivo alterado, ese código corre con acceso completo al build y puede inyectarse en el bundle publicado en GHCR.
   - *Cambio:* `modules` debe apuntar a `"./node_modules/@inlang/plugin-message-format/dist/index.js"`, que es relativo a `apps/web`. Agregar un chequeo en `check-i18n.mjs` (o un unit) que falle si alguna entrada de `modules` empieza con `http`, porque `paraglide init`, `sv add` o un `--overwrite` podrían reescribir `settings.json`. Si la ruta local fallara, se para y se vuelve a revisión; no hay alternativa preaprobada.

3. **E2e de canarios: hoy la ventana "antes y después" puede dar falsos negativos.** Además, la excepción de `Listening on` queda más ancha de lo necesario.
   - En macOS, Node escribe a pipes de forma asíncrona, y el envoltorio copia a un archivo también de forma asíncrona. En la instancia `:4174` se espera cero líneas, así que no hay ninguna línea que confirme que todo se vació antes de leer. Si algo se escribe después de leer el archivo, el test pasa igual.
   - *Cambio:*
     - (a) `serve-with-log.mjs` escribe cada fragmento con `appendFileSync`.
     - (b) Un `globalTeardown` detiene los dos servidores y después revisa **los archivos completos**. Ninguna línea puede contener un canario, la IP `203.0.113.77` ni el query.
     - (c) Una lista blanca de formas de línea: cada línea es JSON con solo los campos de RF-A19, salvo **una** línea que cumpla exactamente `^Listening on http://[^\s]+:\d+$`, que aparezca una sola vez y antes de cualquier petición.
   - Así la excepción queda acotada a esa línea, y cualquier otra salida que no sea de `log.ts` falla: un aviso de Node, un `console.error` de Kit o un log de arranque con variables de entorno.
   - *Agregar también:* el log de arranque de `init` no incluye valores de variables de entorno, solo un código. El e2e del guard incluye un `POST` del mismo origen a `/es/dev/error-probe` y un `GET /es/dev/components/__data.json`, porque la spec justifica el guard justamente por las actions y los `+server.ts`.

4. **`paraglideMiddleware` es una segunda fuente de redirecciones, y ningún test la ejercita.**
   - El middleware responde 307 con `Location: decision.redirectUrl.href`, una URL **absoluta** con el origen de `request.url`. adapter-node arma ese origen con `Host` cuando no hay `ORIGIN`, como en dev, en el e2e y en el smoke test. Solo redirige si llega `Sec-Fetch-Dest: document`, que todo navegador manda y el cliente HTTP crudo del e2e no.
   - En producción `ORIGIN` es fijo y no hay caché compartida, así que no es explotable.
   - Pero RF-A3 promete que el `Location` siempre es relativo, y el camino `/es`, `/en` o `/EN/…` del middleware queda sin probar.
   - *Cambio:* repetir los casos de RF-A3, más `/es`, `/en`, `/es//evil.example/` y `/EN/`, con `Sec-Fetch-Dest: document` y `Host: evil.example`. La afirmación es que ningún `Location` es absoluto ni apunta a otro host. Si alguno lo es, el handle `paraglide` lo reescribe a `pathname + search`.

5. **Respuesta 307 propia.**
   - Debe llevar `Vary: Cookie, Accept-Language` y `Cache-Control: private, no-store`, porque su destino depende de esas cabeceras. Paraglide pone `Vary` en las suyas y la nuestra no.
   - Hay que construir el `Location` solo con `event.url.pathname` y `event.url.search`, ya serializados, y nunca con `request.url` crudo ni con cabeceras. Un unit lo comprueba con `%0d%0a` en la ruta.
   - `localizedHref()` necesita el mismo unit con rutas `//…` y `/\…`.
   - Las exclusiones (`/healthz`, `/api`, `/ws`) se comparan por segmento completo (`/api` o `/api/…`), como en el caso `/developers` del guard.

6. **`handleError` y el cliente.** `App.Error.message` debe ser un código fijo y nunca `error.message`: hoy RF-A20 lanza un mensaje con una IP y un query de canario. El e2e de A19 debe afirmar también que el HTML de la página de error no contiene esos canarios. El `error_id` de los 404 que no se registran es aceptable.

7. **CSP.** Me sirve como está. Si el e2e en WebKit encuentra una violación, el ajuste no puede ser agregar `'unsafe-inline'` a `script-src`, `'unsafe-eval'` ni `'strict-dynamic'`. Cualquier cambio de `script-src` vuelve a revisión, y queda anotado en "Desviaciones".

8. **Cadena de suministro.**
   - Agregar `strictDepBuilds: true` en `pnpm-workspace.yaml`. Con eso, una dependencia nueva con scripts de instalación rompe `pnpm install`, en vez de quedar ignorada en silencio.
   - `pnpm dlx` no usa el lockfile, y sus dependencias transitivas no siguen las versiones fijadas. Por eso conviene crear el esqueleto **a mano**, sin `sv create`, y correr `shadcn-svelte@1.7.0` solo en local, con la revisión del diff que ya prevé el paso 10. Nunca en CI.
   - La decisión 5, no usar el paquete `cn` sino escribir `utils.ts` a mano, es la correcta.

9. **Cookie `theme`.** No tiene problemas. Como defensa en profundidad, el handle `theme` debería reemplazar `%theme%` y `%theme.meta%` solo en el fragmento del `<head>`, o solo la primera vez. Así, un texto futuro de usuario que contenga el marcador no recibe `<meta>` inyectadas. El valor ya está validado, así que el impacto es nulo.

10. **HSTS.** Es correcto. `securityHeaders` debe tolerar respuestas con cabeceras inmutables: copiar la respuesta si `headers.set` lanza `TypeError`, como hace `withVary` de Paraglide. Si no lo hace, una respuesta así se convierte en un 500 que pasa por `handleError`.

### frontend-engineer — 2026-10-21 — RESPUESTA

Incorporé todas las notas de las dos revisiones al cuerpo del plan. No cambié `status` ni `approvals`. No descarté ninguna nota. En un caso fui más allá de lo pedido (security, punto 4), y lo explico abajo.

**architect**

| Punto | Cambio en el plan |
|---|---|
| 1 (issue de migración: abrirlo ya, que enmiende spec y ADR-0009 D2/D4, después de C, bloqueante) | Decisión 1 y riesgo "Kit 2": el issue ya existe, **TA-9**, y **bloquea a TA-8** ("Primer despliegue a producción"). El plan lo cita y no crea issues; anoto qué debe cubrir TA-9 (después de C, enmiendas a RF-A10, RF-C2, RF-C4, smoke test de RF-C2 y ADR-0009 D2/D4 con OK del owner). El paso 15 ya no abre issues, solo comprueba el bloqueo |
| 2 (`LocaleSwitcher` nunca llama a `setLocale`) | Decisión 2 y paso 11. Además lo hago cumplir con `no-restricted-imports` sobre `setLocale` (tabla de archivos: `eslint.config.js`), con un caso en el test 2 (paso 9) |
| 3 (`add --no-deps`, diff vacío, `pnpm why cn`) | Paso 10: `add --no-deps` en local, con el lockfile fijado; verificación con `git diff --exit-code package.json apps/web/package.json pnpm-lock.yaml` y `pnpm why cn` vacíos. También en "Riesgos" y en "Evidencia en el PR" |
| 3 (quitar `init` de "No verificado (a)") | "No verificado (a)" ahora habla solo de `add --no-deps`, y aclara que `init` no se usa |
| 3 (lista de `MODIFICADO` por archivo) | Decisión 8 convertida en tabla por archivo: `button`, `badge`, `input`, `dialog-overlay` (`bg-black/50` → `bg-background/80`), `dialog-content` y, en `dropdown-menu`, `-item`, `-checkbox-item`, `-radio-item` y `-sub-trigger` con `min-h-11`. El paso 10 revisa el diff contra esa tabla |
| 4 (guard con `Response` 404 directo) | Decisión 3, handle 4; unit del paso 6; fila A16; "Alternativas descartadas" (`throw error(404)`) |
| 5 (**obligatorio**: C sí cambia `check`) | "`just check` en la fase A", puntos de extensión: C suma `check-compose.mjs` a `just check` (RF-C6); solo `check-workflows.mjs` va únicamente en `policy`, y C excluye sus tests de `scripts/vitest.config.mjs` en el job `web` (RF-C10). También corregí "Estimación de CI" |
| 6 (`/es/dev/status` no existe en A) | Fila A16: se sigue pidiendo, pero queda anotado que en A da 404 por ruta inexistente y no cuenta como cobertura del guard |
| 8 (convención `plan-<fase>.md`) | Fuera de este archivo, como pide el architect: queda para un cambio de `docs/process.md`, `write-plan` y `feature-pipeline` con OK del owner. Lo dejo al orquestador |

**security-reviewer**

| Punto | Cambio en el plan |
|---|---|
| 1 (bloqueo vinculante y `pnpm audit`) | Decisión 1 y riesgo "Kit 2": TA-9 bloquea a TA-8, y TA-9 vuelve a verificar el CSRF, el filtro de `handleError` y el guard. `pnpm audit --audit-level moderate` en el paso 15, en la fila A1 y en "Evidencia en el PR" |
| 2 (sin jsDelivr y chequeo de `http`) | Decisión 7: ruta local verificada (`./node_modules/@inlang/plugin-message-format/dist/index.js`), sin alternativa por CDN; si falla, se para y vuelve a revisión (paso 4). `check-i18n.mjs` falla si una entrada de `modules` empieza con `http`, con fixture (tabla de archivos). Nueva entrada en "Alternativas descartadas" |
| 3 (canarios: `appendFileSync`, `globalTeardown`, lista blanca; log de arranque; `POST` y `__data.json`) | Decisiones 11 y 12: `appendFileSync`, `global-teardown.ts` que para los servidores y revisa los archivos completos, y lista blanca con una sola línea `^Listening on http://[^\s]+:\d+$`, una vez y antes de cualquier petición. El log de arranque de `init` solo lleva un código (decisión 12, tabla de archivos y unit del paso 5). Fila A16: `GET …/__data.json` y `POST` del mismo origen a `/es/dev/error-probe`. Filas A16 y A19 y riesgo de fugas actualizados |
| 4 (redirecciones de `paraglideMiddleware`) | Fila A3: los casos de RF-A3, más `/es`, `/en`, `/es//evil.example/` y `/EN/`, con `Sec-Fetch-Dest: document` y `Host: evil.example`. **Voy más allá de lo pedido:** en vez de reescribir el `Location` solo "si alguno es absoluto", el handle `paraglide` lo reescribe **siempre** a `pathname + search` (decisión 2, decisión 3 handle 3, unit del paso 4). Motivo: security ya leyó en el código del middleware que el `Location` es absoluto (`redirectUrl.href`), así que el condicional se cumpliría siempre. Hacerlo incondicional evita depender de que el e2e lo descubra |
| 5 (307 con `Vary`/`no-store`, `Location` con `pathname`+`search`, units, exclusiones por segmento) | Decisión 3, handle 2: `Vary: Cookie, Accept-Language`, `Cache-Control: private, no-store`, `Location` solo con `event.url.pathname` y `event.url.search`, y exclusiones por segmento completo. Unit del paso 4 con `%0d%0a`, `//` y `/\`, y `localizedHref()` con `//…` y `/\…`. Fila A3 y riesgo "Redirección abierta" actualizados |
| 6 (`App.Error.message` como código fijo) | Decisión 4: `'not_found'` o `'internal'`, nunca `error.message`; `app.d.ts` con ese tipo; unit de `handleError` (paso 5); fila A19 afirma que el HTML de la página de error no contiene los canarios |
| 7 (restricciones de `script-src`) | Decisión 16 nueva: nunca `'unsafe-inline'`, `'unsafe-eval'` ni `'strict-dynamic'` en `script-src`; cualquier cambio de `script-src` detiene la implementación, se anota en "Desviaciones" y vuelve a revisión. Riesgo "CSP" actualizado |
| 8 (`strictDepBuilds`, esqueleto a mano, shadcn solo en local) | Decisión 14 (`strictDepBuilds: true`) y decisión 13 (esqueleto a mano sin `sv create`; `shadcn-svelte` solo en local con `--no-deps`, nunca en CI ni en el `justfile`). Tabla de archivos (`pnpm-workspace.yaml`), pasos 1, 2 y 10, "No verificado (b)", fila A1, riesgos y alternativas |
| 9 (`%theme%` solo en el `<head>`) | Decisión 3, handle 5: los marcadores se reemplazan solo antes de `</head>` (lo que incluye `<html>`, donde va `data-theme`) y solo la primera vez. Unit en el paso 7; tabla de archivos (`app.html`) |
| 10 (`securityHeaders` con cabeceras inmutables) | Decisión 3, handle 1: si `headers.set` lanza `TypeError`, copia la respuesta. Unit en el paso 6 y fila A10 |

### frontend-engineer — 2026-10-22 — RESPUESTA (Kit 3)

Actualicé el plan a la enmienda SvelteKit 3 de la spec (2026-10-22) y a las de ADR-0008 y ADR-0009. `status: in-review` y `approvals` vacío: el plan cambió y necesita otra vez la revisión de architect y security-reviewer. Las revisiones de arriba se conservan como historia; las referencias a la migración posterior quedaron sin objeto y salieron del cuerpo del plan.

| Tema | Cambio en el plan |
|---|---|
| Kit 3 desde el inicio | Decisión 1 reescrita (Kit 3.1.0, adapter-node 6.0.0, Node ≥ 22.17, TS 6.0.x, archivos `server`, ecosistema joven). Tabla de versiones: kit 3.1.0, adapter-node 6.0.0, bits-ui 2.20.0 (la 2.20.1 tiene menos de 24 h), Node `22.23.4` exacta y `engines >=22.17`. "Alternativas descartadas": Kit 2.70.3 con migración posterior |
| Configuración en `vite.config.ts` | Decisión 17: `sveltekit({ adapter, csp })`, sin `svelte.config.js`, sin `csrf` ni `paths.origin`; nuevo `scripts/check-kit-config.mjs` en `just check` con fixtures (RF-A10, RF-A21) |
| `#lib` | Decisión 18: `imports` en `apps/web/package.json`, imports con extensión, `tsconfig` que extiende `$app/tsconfig`, `components.json` con los alias de D2 enmendado. **Hallazgo:** en `no-restricted-imports`, un `group` que empieza con `#` es un comentario de `.gitignore` y nunca coincide (lo comprobé con ESLint 9 y typescript-eslint 8.72.0); los patrones `#lib` van como `regex` y el test 2 tiene un caso por patrón que solo ese patrón atrapa |
| `handleError` | Decisión 4 reescrita: solo `kind: 'unknown'` se registra, `error_id` solo en esos, `message` como código fijo en todos los `kind`, sin tocar `status`, **síncrono** y sin lanzar. `app.d.ts` solo agrega `errorId?` porque Kit 3 ya declara `status` y `message`. Unit por `kind`, del *getter* que lanza y de que no es `AsyncFunction` ni devuelve *thenable* (paso 5) |
| Guard RF-A23 | Decisión 3: `originGuard` es el handle 2, justo después de `securityHeaders` y antes de `localeRedirect`, `paraglide`, `devGuard` y `theme`; fábrica pura `createOriginGuard(PUBLIC_ORIGIN)`. Unit con la tabla de la spec (paso 6) y fila A23 |
| `PUBLIC_ORIGIN` y `src/env.ts` | Decisión 19: `defineEnvVars` con `PUBLIC_ORIGIN` y `ENABLE_DEV_CATALOG`, privadas y dinámicas, leídas de `$app/env/private`; validadores puros en `src/lib/env/validators.ts`. `.env.example` y `playwright.config.ts` con `PUBLIC_ORIGIN` |
| Paso 2 | Ahora es la prueba corta de "Áreas de preocupación" 12: `init` de shadcn-svelte en una copia desechable para ver los alias, `add --no-deps button` en el árbol real con `#lib`, `svelte-check`, `vite build`, arranque con y sin `PUBLIC_ORIGIN`. Si falla, el plan vuelve a revisión; no hay respaldo preaprobado |
| Minor 1 de security (URLs absolutas) | Decisión 20: `web` nunca arma URLs absolutas con `event.url.origin`/`host`/`href` ni `request.url`; usa `PUBLIC_ORIGIN`. ESLint lo hace cumplir (`no-restricted-syntax`), con caso en el test 2 |
| Minor 2 de security (`handleError` síncrono) | Decisión 4 y unit del paso 5 |
| Minor 3 de security (mensajes de validadores) | Decisión 19: el mensaje nombra la variable y la regla, nunca el valor; unit que lo comprueba para cada entrada inválida (paso 6) |
| Minor 4 de security (e2e de RF-A23) | Fila A23: los `POST` llevan `Content-Type: application/json` y se verifica el cuerpo `{"error":"auth.csrf_origin"}` |
| Efecto lateral en A16 | Sin `paths.origin`, Kit rechaza sobre `http` un `POST` de formulario del mismo origen antes de `handle`. El `POST` del e2e de A16 pasa a JSON con `Origin` = `PUBLIC_ORIGIN`, para que de verdad llegue al guard de `/dev/**`. Nuevo riesgo "Formularios del mismo origen sobre `http`" |

### architect — 2026-10-22 — APPROVE (Kit 3)

- **Verifiqué:** leí el `git diff` de `plan-a.md` y el plan completo, incluida la respuesta de frontend-engineer (Kit 3). Lo comparé con la enmienda de la spec del commit `7e17a37` (RF-A1, A2, A10, A14, A16, A19, A21, A23, RF-B18, RF-C2, C4 y C10, y "Áreas de preocupación" 2 y 12), con la enmienda de ADR-0008 (D4 sin `checkOrigin`, guard propio y `trustedOrigins` vacío), con la de ADR-0009 (D2 y D4 con `#lib`, D3 con `csp` en `vite.config.ts`, D5 con `$app/env/private` y test 2) y con las revisiones de security-reviewer y frontend-engineer de esa enmienda. El frontmatter es válido. No descargué ni ejecuté nada.
- **Decisión 1:** Kit 3.1.0, adapter-node 6.0.0, Node ≥ 22.17 y TS 6.0.x. Es coherente con RF-A1 y RF-A2.
- **Decisión 3, orden de los `handle`:** `securityHeaders` → `originGuard` → `localeRedirect` → `paraglide` → `devGuard` → `theme`. Cumple RF-A23 y RF-A16, y un `POST` ajeno a `/` recibe 403 y no 307. La fábrica `createOriginGuard` nunca lee `Host`, `X-Forwarded-*` ni `event.url`.
- **Decisión 4, `handleError`:** coincide con RF-A19 enmendado. Solo registra `kind: 'unknown'`, el `message` es un código fijo, es síncrono y nunca lanza. El `app.d.ts` solo agrega `errorId?`.
- **Decisiones 17 a 20:** son correctas y en algunos puntos más estrictas que la spec. El hallazgo del `#` como comentario de `.gitignore` es real, y resolverlo con un `regex` anclado es correcto.
- **Paso 2:** es la prueba corta que pide "Áreas de preocupación" 12, con un punto de corte sin respaldo preaprobado.

**Hallazgos (Minor, no bloquean):**
1. **Paso 4:** nombrar el punto de corte para Paraglide con Kit 3. Si no compila o si `reroute`/`paraglideMiddleware` no funcionan, se para y el plan vuelve a revisión.
2. **`createOriginGuard`:** debe fallar cerrado cuando el origen está vacío o falta, rechazando todo método no seguro. Dos casos de unit: `undefined` y `''`.
3. **Selector de ESLint de la decisión 20:** se salta con destructuring (`const { url } = event`). Hay que ampliarlo a un `Identifier` `url` y agregar el caso al test 2.
4. **Versión de ESLint:** la prueba del `#` se hizo con ESLint 9 y el plan fija la 10. Hay que anotar la versión real en "Desviaciones" al terminar el paso 9.
5. **Test 2 y `ignores`:** el test 2 necesita un caso que pase con `import type` de `#lib/…`. `eslint.config.js` debe ignorar `src/lib/paraglide/**`, `.svelte-kit` y `build`.

**Pendiente del architect, fuera del plan:** llevar a la spec la regla de URLs absolutas (solo `PUBLIC_ORIGIN`) y la precisión "detrás de Traefik" en RF-C4, como enmienda menor.

### security-reviewer — 2026-10-22 — APPROVE (Kit 3)
**🤖 security-reviewer**

Verifiqué: comparé el `git diff` de `plan-a.md` y la respuesta `frontend-engineer — 2026-10-22 — RESPUESTA (Kit 3)` con RF-A10, RF-A16, RF-A19, RF-A23, RF-B8 y RF-C4 de la spec enmendada, y con mi aprobación de la enmienda del commit `7e17a37` (Minor 1 a 6). Revisé el plan completo en estos puntos:
- decisiones 1 a 4, 7, 11 a 14 y 16 a 20;
- la tabla de archivos y los pasos 2, 4, 5, 6, 9, 12 y 15;
- los riesgos;
- las filas A1, A3, A10, A16, A19 y A23.

En esta ronda no volví a abrir el código de Kit 3.1.0. Me apoyo en lo que leí en la ronda anterior: `csrf.js`, `respond.js`, `errors.js`, `env.js` y `handler.js`.

**Resultado de cada punto que pediste:**
- **Minor 1, URLs absolutas.** Aplicado en la decisión 20: `PUBLIC_ORIGIN` es la única fuente de URLs absolutas, con `no-restricted-syntax` y un caso en el test 2. Los selectores tienen huecos (hallazgo 1).
- **Minor 2, `handleError` síncrono.** Aplicado en la decisión 4 y en el paso 5:
  - un unit comprueba que no es `AsyncFunction` y que no devuelve un *thenable*;
  - un `try/catch` cubre todo el cuerpo y devuelve `{ message: 'internal' }` cuando falla;
  - un unit con un *getter* que lanza;
  - un riesgo dedicado.
- **Minor 3, validadores.** Aplicado en la decisión 19 y en el paso 6: el mensaje nombra la variable y la regla, nunca el valor, y un unit lo comprueba para cada entrada inválida. Los validadores son puros y están en `src/lib/env/validators.ts`.
- **Minor 4, e2e de RF-A23.** Aplicado en la fila A23: los dos `POST` llevan `Content-Type: application/json` y se verifica el cuerpo exacto `{"error":"auth.csrf_origin"}`. Con JSON, Kit no los rechaza antes, así que el 403 solo puede venir del guard.
- **`originGuard`** (decisión 3, handle 2):
  - Es el primero que puede responder y solo lo envuelve `securityHeaders`, que no hace nada antes de `resolve`. Va antes de `localeRedirect`, `paraglide`, `devGuard` y `theme`.
  - Revisa todo método fuera de `GET`, `HEAD` y `OPTIONS`, en cualquier ruta y con cualquier `Content-Type`.
  - Compara `Origin` con `PUBLIC_ORIGIN` de forma exacta. Si falta `Origin`, solo acepta `Sec-Fetch-Site: same-origin`. Rechaza `Origin: null`.
  - No llama a `resolve`, no escribe logs y nunca lee `Host`, `X-Forwarded-*` ni `event.url`.
  - Se construye con una fábrica pura. La tabla del unit cubre `Host` y `X-Forwarded-Host` falsificados y usa un espía de `resolve`.
  - La 403 lleva las cabeceras de A10.
  - Cumple RF-A23 y ADR-0008 D4.
- **`POST` del e2e de A16.** Va en JSON con `Origin` igual al `PUBLIC_ORIGIN` de `:4174`, así que pasa el chequeo de Kit y el guard de `Origin` y de verdad llega a `devGuard`. Antes, el 403 de Kit habría hecho pasar el test sin ejercitar el guard de `/dev/**`. El nuevo riesgo "Formularios del mismo origen sobre `http`" está bien acotado, porque en 001 no hay actions.
- **`handleError` solo con `kind: 'unknown'`.** Es correcto:
  - `'app'`, `'framework'` y `'validation'` se pueden provocar a voluntad, y registrarlos sería registrar peticiones.
  - `error_id` solo existe en lo que se registra.
  - `message` es un código fijo en todos los `kind`.
  - El `error(404)` de la segunda capa de `/dev` no se registra.
- **`src/env.ts`:**
  - Las dos variables son privadas y dinámicas y se leen de `$app/env/private`.
  - No se usa `public: true`.
  - `PUBLIC_ORIGIN` solo es opcional durante el build.
  - El paso 2 prueba que `node build` no arranca sin ella.
  - `.env.example` no tiene secretos.
- **Mis Minor anteriores siguen en el plan:**
  - **Canarios:** decisiones 11 y 12. Usan `appendFileSync`, un `globalTeardown` que revisa los archivos completos y una lista blanca con una sola línea `Listening on`. El log de arranque lleva solo un código.
  - **Redirecciones:** decisiones 2 y 3 y fila A3.
    - El `Location` se arma solo con `pathname` y `search`.
    - La 307 lleva `Vary` y `no-store`.
    - Las exclusiones se comparan por segmento completo.
    - La reescritura del `Location` de Paraglide es incondicional.
    - El e2e usa `Host: evil.example` y `Sec-Fetch-Dest: document`.
  - **Cadena de suministro:** `strictDepBuilds: true` y `onlyBuiltDependencies: []` (decisión 14). El esqueleto se crea a mano y `shadcn-svelte` solo corre en local (decisión 13).
  - **Sin jsDelivr:** la decisión 7 usa una ruta local, sin alternativa por CDN, y `check-i18n.mjs` falla si una entrada de `modules` empieza con `http`.
  - **Otros que también siguen:** CSP sin `'unsafe-inline'`, `'unsafe-eval'` ni `'strict-dynamic'` en `script-src` (decisión 16), `pnpm audit` en el paso 15 y `check-kit-config.mjs` contra `trustedOrigins`, `checkOrigin`, `paths.origin` y `svelte.config.js` (decisión 17).

**Hallazgos (todos Minor, no bloquean):**

1. **El selector de ESLint de la decisión 20 es más estrecho que la regla.** Solo atrapa `<x>.url.origin|host|href` cuando `url` es una propiedad, y `request.url`. Se escapan:
   - `const { url } = event; url.origin`;
   - `String(event.url)`, `event.url.toString()` y `` `${event.url}` ``;
   - `new URL(path, event.url)`, que da una URL absoluta con el `Host` del cliente.
   - *Escenario:* en una feature futura, el `next` del login o un enlace canónico se arma con `new URL('/es/x', event.url).href`. ESLint no lo marca, y una petición directa al contenedor con `Host: evil.example` (en `prod-local` o desde `torneo-edge`) genera enlaces al dominio del atacante.
   - *Cambio:* ampliar `no-restricted-syntax` en `src/**/*.ts` a:
     - `MemberExpression` sobre un `Identifier` llamado `url` con `origin`, `host` o `href`;
     - `NewExpression[callee.name='URL']` con segundo argumento;
     - `.toString()` y `String()` sobre `*.url`;
     - plantillas con `*.url`.

     Agregar un caso por forma al test 2. Si algún selector da demasiados falsos positivos, se anota en "Desviaciones". La regla de la decisión 20 sigue valiendo de todos modos y se aplica en revisión.
2. **El validador de `PUBLIC_ORIGIN` debe exigir la forma en que el navegador serializa `Origin`.** El guard compara cadenas sin normalizar, lo cual es correcto. Pero el navegador manda el host en minúsculas y sin puerto por defecto. Un `PUBLIC_ORIGIN=https://Torneo.example` o `https://torneo.example:443` pasaría la regla de RF-B8, y el guard rechazaría todas las mutaciones legítimas. No es una vía de explotación, porque el fallo es cerrado, pero rompe en silencio la primera feature con `POST`.
   - *Cambio:* el validador rechaza un valor si `new URL(v).origin !== v`. Eso cubre las mayúsculas, el puerto por defecto, la barra final, la ruta y las credenciales. Agregar al unit del paso 6 casos con mayúsculas, `:443` y `:80`. El mensaje sigue sin incluir el valor.
3. **Nit, unit de `originGuard`.** Agregar a la tabla un `Origin` con mayúsculas o con puerto por defecto (`https://TORNEO.example`, `https://torneo.example:443`) que debe dar 403. Así queda fijado que la comparación es exacta y que nadie le agregue una normalización.

### Orquestador — 2026-10-22 — nota
Los Minor de la ronda Kit 3 que no bloquean (architect 1–5 y security-reviewer 1–3) se incorporan **al inicio de la implementación**: el autor los pasa al cuerpo del plan en el primer commit de la rama y las revisiones del PR los verifican.

### frontend-engineer — 2026-10-22 — RESPUESTA (Minor de la ronda Kit 3)

Incorporados al cuerpo del plan en el primer commit de la rama, sin cambiar `status` ni `approvals`:

| Minor | Dónde |
|---|---|
| architect 1 (punto de corte de Paraglide con Kit 3) | Paso 4 |
| architect 2 (`createOriginGuard` falla cerrado con `undefined` y `''`) | Decisión 3, handle 2; unit del paso 6 |
| architect 3 (destructuring `const { url } = event`) | Decisión 20; casos del paso 9 |
| architect 4 (versión real de ESLint en "Desviaciones") | Decisión 18; paso 9 |
| architect 5 (`import type` de `#lib/…` que pasa; `ignores`) | Decisión 18; paso 9; tabla de archivos (`eslint.config.js`) |
| security 1 (selectores de URL absoluta más amplios) | Decisión 20; casos del paso 9 |
| security 2 (`new URL(v).origin !== v` en el validador) | Decisión 19; unit del paso 6 |
| security 3 (`Origin` con mayúsculas o puerto por defecto → 403) | Decisión 3, handle 2; unit del paso 6 |
