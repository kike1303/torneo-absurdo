---
id: "001"
title: Fundaciones del monorepo
type: spec
status: done
risk: high
author: architect
reviewers: [product-owner, security-reviewer, devops-engineer, owner, frontend-engineer]
approvals:
  - by: product-owner
    date: 2026-10-21
  - by: security-reviewer
    date: 2026-10-21
  - by: devops-engineer
    date: 2026-10-21
  - by: owner
    date: 2026-10-21
  - by: security-reviewer
    date: 2026-10-22
  - by: frontend-engineer
    date: 2026-10-22
  - by: security-reviewer
    date: 2026-10-23
  - by: devops-engineer
    date: 2026-10-23
  - by: security-reviewer
    date: 2026-10-23
  - by: owner
    date: 2026-10-23
  - by: security-reviewer
    date: 2026-10-24
  - by: owner
    date: 2026-10-24
linear: TA-1
depends_on: []
created: 2026-10-21
updated: 2026-10-26
---

# Spec: Fundaciones del monorepo

> Intent: [intent.md](./intent.md) · ADRs: [0003](../../decisions/0003-stack-tecnologico.md), [0006](../../decisions/0006-despliegue-y-ci.md), [0007](../../decisions/0007-i18n-desde-cero.md), [0008](../../decisions/0008-autenticacion.md) (solo D3, D4 "sin CORS" y D6), [0009](../../decisions/0009-ui-design-system.md) · [Aviso de privacidad](../../legal/aviso-de-privacidad.md)

## Resumen
Se construye la base del monorepo en **tres fases con un PR cada una**: **A**, la web SvelteKit con i18n, tema y design system; **B**, el server axum con `/api/health`, el WebSocket de eco, `crates/protocol` con tipos TS generados por `ts-rs` y Postgres local; **C**, las imágenes, el compose de producción, la CI, el release con gate manual y el hook `pre-push`. No se despliega a producción. La spec fija además qué registran los logs de `web` y del server (para el aviso de privacidad), la cookie de idioma y cómo el eco convive con ADR-0008 D6.

Dependencias entre fases (las del intent): A no depende de nada; B depende de A; C depende de A y B.

**La fase C puede ir en dos PRs**, y lo decide su plan según el tamaño del diff: **C1**, imágenes, compose de producción, `.dockerignore` y `prod-local` (RF-C1 a RF-C7); **C2**, `ci.yml`, `release.yml`, `policy`, Dependabot, el hook y los docs (RF-C8 a RF-C18). C2 depende de C1, porque el job `images` construye los Dockerfiles de C1. El cierre (RF-C19) va después del último PR de C.

**Esta spec no habilita el primer despliegue a producción.** Ese despliegue queda bloqueado, además de por las verificaciones de ADR-0006, por los límites anónimos de ADR-0008 D6 y D7 en `/ws` (RF-C18, RF-C19 y "Áreas de preocupación" 1).

## Requisitos funcionales

Cada requisito dice cómo se verifica (→). "CI" significa que lo prueba un job de `ci.yml`; "PR" significa que el autor pega la evidencia en la descripción del PR de esa fase.

### Fase A — web, i18n y design system

**Monorepo JS**
- **RF-A1.** Raíz con `package.json` (`private: true`, `packageManager` con la versión exacta de pnpm ≥ 10 **y su hash**, `pnpm@x.y.z+sha512.…`), `pnpm-workspace.yaml` (`apps/web`, `packages/*`), `.node-version` con una versión exacta de Node 22 **≥ 22.17** (el mínimo de SvelteKit 3; enmienda 2026-10-22) y `.npmrc` con `save-exact=true`. pnpm no ejecuta scripts de instalación de dependencias salvo una lista explícita (`onlyBuiltDependencies`, vacía si se puede) y no instala versiones publicadas hace menos de 24 h (`minimumReleaseAge: 1440` en `pnpm-workspace.yaml`). → CI (`pnpm install --frozen-lockfile` en el job `web`) y revisión del PR.
- **RF-A2.** `apps/web` es **SvelteKit 3** (`@sveltejs/kit` 3.1.x) con Svelte 5 (≥ 5.57.1), Vite 8 (≥ 8.0.12), `@sveltejs/vite-plugin-svelte` 7, `@sveltejs/adapter-node` 6 y TypeScript `strict` en **6.0.x** (Kit 3 pide `^6` y `typescript-eslint` solo admite `< 6.1`, así que la 7.x queda fuera). Lo que eso fija (enmienda 2026-10-22):
  - **No existe `svelte.config.js`.** Toda la configuración de Kit (`adapter`, `csp`, `csrf`) va en las opciones del plugin `sveltekit({ … })` de `vite.config.ts`.
  - **Alias `#lib`** con *subpath imports* de Node en el `package.json` de `apps/web`: `"imports": { "#lib": "./src/lib/index.ts", "#lib/*": "./src/lib/*" }` (la primera entrada solo si existe `src/lib/index.ts`). Los imports llevan extensión (`#lib/utils.js`). `$lib` no existe.
  - `tsconfig.json` extiende `$app/tsconfig`, con `include` y `exclude` propios.
  - Variables de entorno de `web` declaradas en `src/env.ts` con `defineEnvVars` y leídas de `$app/env/private` o `$app/env/public`. Los módulos `$env/*` y `$app/environment` (alias obsoletos en Kit 3) no se usan.

  **Ninguna ruta declara `prerender = true`** (ADR-0009 D3). → `svelte-check` y `vite build` pasan; un chequeo de `just check` busca `prerender` en `src/routes/**` y falla si aparece con `true`; ESLint falla con imports de `$lib`, `$lib/**`, `$env/**` y `$app/environment` (RF-A14). Kit 3 además rompe el build si se prerenderiza una página cuya plantilla contiene `%sveltekit.nonce%`, que es nuestro caso (RF-A7).

**i18n (ADR-0007)**
- **RF-A3.** Paraglide JS 2 con mensajes en `apps/web/messages/{es,en}.json`, `baseLocale = en`. Todas las páginas viven bajo `/es/…` y `/en/…`. Una petición a `/` o a una ruta sin prefijo responde **307** a la misma ruta con prefijo, eligiendo el idioma por: cookie `locale` válida → `Accept-Language` → `en`. Quedan fuera del enrutamiento por idioma `/healthz`, `/api/**` y `/ws`. `<html lang>` refleja el idioma de la ruta. La redirección nunca es abierta: el `Location` siempre es una ruta relativa que empieza con `/es/` o `/en/`. → e2e, incluidos `GET //evil.example/` y `GET /\evil.example`, cuyo `Location` empieza con `/es/` o `/en/`.
- **RF-A4.** **Cookie de idioma**: nombre `locale`, valores `es | en`, atributos `Path=/; Max-Age=31536000; SameSite=Lax`, más `Secure` cuando el origen es `https`; sin `HttpOnly` (la escribe el cliente, igual que `theme`). Se escribe **solo** cuando el usuario elige idioma en el selector; visitar `/en/…` por un enlace no la cambia. Un valor que no sea `es` o `en` se ignora. → e2e y unit.
- **RF-A5.** El selector de idioma, en la cabecera de la app, navega a la misma ruta y query en el otro idioma y escribe la cookie. Tras elegir `es`, `GET /` redirige a `/es/` aunque `Accept-Language` pida `en`. → e2e (criterio A2 del intent).

**Tema (ADR-0009 D3)**
- **RF-A6.** `hooks.server.ts` resuelve el tema con la precedencia de D3 (en 001 no hay perfil: cookie `theme` → `dark`). La cookie se valida contra `dark | light | system`; **cualquier otro valor se trata como ausente** antes de llegar a `transformPageChunk` (el valor nunca se inserta en el HTML sin validar). Reemplaza `%theme%` en `data-theme`, `color-scheme` y `theme-color` según D3. → unit (tabla de cookies, incluida una con `"><script>`) y e2e (test 5 de ADR-0009).
- **RF-A7.** El script del tema `system` vive **estático en `app.html`** con `nonce="%sveltekit.nonce%"`, pesa ≤ 1 KB y no hace nada si `data-theme` no es `system`. El hook **no** inyecta `<script>`; solo las `<meta>` y el atributo dependen del hook (nota de frontend-engineer en ADR-0009). → revisión del PR y e2e (test 5 con CSP activa).
- **RF-A8.** `lib/theme/theme.svelte.ts` (runa) cambia el tema en vivo sin recargar y escribe la cookie `theme` (`Path=/; Max-Age=31536000; SameSite=Lax`, `Secure` en `https`). No se usa `mode-watcher`. → e2e.
- **RF-A9.** **Override del catálogo**: `?theme=dark|light` solo tiene efecto si `ENABLE_DEV_CATALOG === '1'` **y** `event.route.id` empieza con `/dev/` (la ruta ya sin prefijo de idioma; no se compara `url.pathname`, que es `/es/dev/…`). No escribe cookie. → e2e (test 6 de ADR-0009, incluido `/es/?theme=light` sin efecto).

**CSP y cabeceras**
- **RF-A10.** La opción `csp` del plugin `sveltekit()` (antes `kit.csp`; enmienda 2026-10-22) en `mode: 'auto'` con: `default-src 'self'`; `script-src 'self'` (más el nonce que agrega SvelteKit); `style-src 'self' 'unsafe-inline'` (Bits UI posiciona con estilos en línea); `img-src 'self' data: blob:`; `font-src 'self'`; `connect-src 'self'`; `object-src 'none'`; `base-uri 'self'`; `form-action 'self'`; `frame-ancestors 'none'`. `hooks.server.ts` agrega `X-Content-Type-Options: nosniff`, `Referrer-Policy: strict-origin-when-cross-origin` y `Strict-Transport-Security: max-age=31536000` (sin `includeSubDomains` ni `preload`, para no afectar a otros subdominios del owner; los navegadores lo ignoran sobre `http`). **CSRF de SvelteKit 3** (enmienda 2026-10-22): Kit 3 quitó `csrf.checkOrigin` y su chequeo de formularios está siempre activo; la única forma de apagarlo es `csrf.trustedOrigins: ['*']`. Por eso `csrf.trustedOrigins` **no se escribe** en la configuración (queda `[]`) y **`paths.origin` no se define**: es una constante de build, y la misma imagen tiene que servir a cualquier host (RF-C4, RF-C7). Sin `paths.origin`, Kit compara el `Origin` de la petición con el origen que arma adapter-node (`https://` + `Host`; ver RF-C4). Ese chequeo solo cubre `POST`/`PUT`/`PATCH`/`DELETE` con `Content-Type` de formulario o sin `Content-Type`; el resto lo cubre el guard de `Origin` propio (RF-A23). → e2e: en `/es/`, `/en/` y el catálogo no hay eventos `securitypolicyviolation`, el script del tema corre y las tres cabeceras están presentes; un chequeo de `just check` falla si `vite.config.ts` contiene `trustedOrigins`, `checkOrigin` o `paths.origin` (o `origin:` dentro de `paths`).
- **RF-A23.** **Guard de `Origin` en `web`** (enmienda 2026-10-22; misma regla que ADR-0008 D4 para peticiones del navegador). Va numerado al final para no mover los demás RF, pero vive aquí. Es un `handle` de `hooks.server.ts` y es el **primero que puede responder**: solo lo envuelve el handle de cabeceras de RF-A10, que no ejecuta nada antes de `resolve`. Va antes de la redirección por idioma, de Paraglide y del guard de `/dev/**`.
  - Aplica a **todo método distinto de `GET`, `HEAD` y `OPTIONS`**, en cualquier ruta (incluidas `/healthz` y las excluidas del idioma) y con **cualquier** `Content-Type`.
  - Pasa si el header `Origin` es **exactamente** `PUBLIC_ORIGIN` (comparación de cadenas, sin normalizar). Si `Origin` **falta**, pasa solo con `Sec-Fetch-Site: same-origin`. En cualquier otro caso (incluido `Origin: null`) responde **403** con `{"error":"auth.csrf_origin"}` (el código de ADR-0008 D4), **sin llamar a `resolve`** y sin escribir logs.
  - Nunca lee `X-Forwarded-*`, `Host` ni `event.url.origin` para esta decisión: el origen de confianza es solo `PUBLIC_ORIGIN`.
  - `PUBLIC_ORIGIN` se declara en `src/env.ts` (privada, dinámica) con un validador que exige un origen `http(s)://host[:puerto]` sin ruta, query ni barra final (la misma regla que RF-B8). Es opcional solo durante el build (`building` de `$app/env`); si falta o es inválida en runtime, `web` **no arranca**. Su valor es el mismo que el del server (RF-C4).
  - Kit 3 aplica su propio chequeo de formularios **antes** de `handle` (RF-A10); este guard no lo reemplaza, lo extiende a JSON y a cualquier otro tipo, y ancla la comparación a una variable en vez de al `Host`.
  → unit con una tabla: método (`GET`, `HEAD`, `OPTIONS`, `POST`, `PUT`, `PATCH`, `DELETE`) × `Origin` (igual, ajeno, `null`, con barra final, con otro esquema, ausente) × `Sec-Fetch-Site` (`same-origin`, `same-site`, `cross-site`, `none`, ausente) × `Content-Type` (formulario, JSON, ausente), y un caso que comprueba que no se llama a `resolve`; unit del validador de `PUBLIC_ORIGIN`. e2e contra `node build` (RF-C10): `POST /es/` con `Content-Type: application/json` y `Origin: https://evil.example` → 403 `auth.csrf_origin` (Kit no revisa JSON, así que esta respuesta prueba el guard propio); `POST /es/` sin `Origin` ni `Sec-Fetch-Site` → 403; `POST /healthz` con JSON y `Origin` = `PUBLIC_ORIGIN` → 405 (pasa el guard y Kit responde que no hay handler); stdout y stderr no reciben ninguna línea durante esas peticiones. Docker (RF-C2): sin `PUBLIC_ORIGIN`, el contenedor de `web` sale con código distinto de 0.

**Design system (ADR-0009)**
- **RF-A11.** Tailwind v4 CSS-first según D1, con la **paleta neutra provisional**: todos los tokens semánticos de shadcn y los `--arena-*`, en los dos temas, en OKLCH; `lib/theme/theme-color.ts` con los `--background` de cada tema. → test 3 de ADR-0009.
- **RF-A12.** shadcn-svelte 1.x según D2, con estas primitivas: `button`, `input`, `label`, `card`, `badge`, `dialog`, `dropdown-menu`, `separator` y `skeleton`. Cambios obligatorios, cada uno con la cabecera `<!-- MODIFICADO: … -->`:
  - `button` variante `destructive`: `text-white` → `text-destructive-foreground`.
  - Overlay de `dialog`: `bg-black/50` → `bg-background/80` (sin token nuevo).
  - **Anillo de foco**: se reemplazan `ring-[3px] ring-ring/50` y `focus-visible:border-ring` por el de D6, `focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background`. Se pinta `--ring` a opacidad completa, que es el par que mide el test 3 (≥ 3:1).
  - Textos internos en inglés (`Close`, `sr-only`) → `ui/_a11y-messages.ts` (D8).
  - Objetivos táctiles ≥ 44×44 px en los tamaños por defecto (D6).
  → tests 1, 3, 4 y 9 de ADR-0009 y el test de foco de abajo.
- **RF-A13.** `scripts/check-tokens.mjs` según D1, con una regla más, coherente con D6: falla con un modificador de opacidad sobre el anillo (`ring-ring/<n>`). → test 1 de ADR-0009 (con un fixture para esta regla).
- **RF-A14.** ESLint flat config según D4, con `@typescript-eslint/no-restricted-imports` (también en `<script lang="ts">`). Los patrones cubren alias **e imports relativos** (`#lib/features/**` y `**/features/**`; `#lib/components/<dominio>/**` y `**/components/<dominio>/**`), con el alias `#lib` de Kit 3 (ADR-0009, enmienda 2026-10-22). Además: `no-restricted-globals` para `fetch` en componentes; `no-console` en todo `src/` salvo `src/lib/server/log.ts`; y, en todo `src/`, `no-restricted-imports` de `$lib`, `$lib/**`, `$env/**` y `$app/environment`, con un mensaje que apunta a `#lib` y a `$app/env/*`. → test 2 de ADR-0009, con fixtures de imports relativos, de `console.log` y de un import de `$lib/utils`.
- **RF-A15.** `scripts/check-i18n.mjs` (en `just check`) falla si: una llave existe en un idioma y no en el otro; un mensaje está vacío; o un `.svelte` fuera de `src/routes/dev/**` tiene texto visible literal (nodos de texto con letras, y los atributos `aria-label`, `title`, `alt` y `placeholder` con texto literal). Usa el parser de `svelte/compiler`, no expresiones regulares sobre el markup. → unit con fixtures (criterio A1 del intent: "falla si falta una llave").
- **RF-A16.** Catálogo `src/routes/dev/components/` según D5. **El guard vive en `handle` de `hooks.server.ts`**: si `event.route.id` es `/dev` o empieza con `/dev/` y `ENABLE_DEV_CATALOG !== '1'` (declarada en `src/env.ts` como privada, dinámica y opcional, y leída de `$app/env/private`; enmienda 2026-10-22), responde con un `Response` 404 directo **sin llamar a `resolve`** ni lanzar `error(404)`, así que no corre ningún `load`, `+server.ts` ni form action de `/dev/**`, no pasa por `handleError` y no escribe logs. Va después del middleware de Paraglide (el `route.id` ya no lleva prefijo de idioma) y antes de todo lo demás, salvo el guard de `Origin` (RF-A23), que va primero. `src/routes/dev/+layout.server.ts` repite la comprobación como segunda capa; su `error(404)` sí pasa por `handleError` en Kit 3 (con `kind: 'app'`), que no lo registra (RF-A19). No basta solo con el layout: SvelteKit corre los `load` de servidor en paralelo (el de la página corre aunque el layout lance 404, salvo `await parent()`), y el layout no cubre `+server.ts` ni las actions. El guard cubre **todo** `/dev/**`, incluidas las rutas de prueba de RF-A20 y RF-B7. → test 6 de ADR-0009 y un e2e contra una instancia **sin** la variable: `/es/dev/components`, `/es/dev/status` y `/es/dev/error-probe` dan 404, y el proceso no escribe ninguna línea en stdout ni stderr durante esas peticiones.

**Páginas**
- **RF-A17.** Página de inicio en `/es/` y `/en/` (contenido provisional, todo en mensajes) con la cabecera de la app (selector de idioma y de tema), y `+error.svelte` traducido para 404 y 500. Son las "páginas clave existentes" de los tests 4 y 8 de ADR-0009. → e2e.
- **RF-A18.** `GET /healthz` (`src/routes/healthz/+server.ts`) responde 200 sin llamar al API, sin redirección por idioma y sin escribir logs. → e2e (criterio A4 del intent).

**Logs de `web` (aviso de privacidad, "Pendiente" punto 8)**
- **RF-A19.** Todo log de `web` sale por `src/lib/server/log.ts`, como JSON por línea a stdout/stderr. `web` **no registra peticiones**: solo el arranque, los errores de `handleError` y los de `process.on('unhandledRejection' | 'uncaughtException')` instalados en el `init` de `hooks.server.ts`. El handler de `uncaughtException` registra y después llama a `process.exit(1)` (Docker reinicia el contenedor); no deja el proceso vivo en un estado indefinido. Campos permitidos: `ts`, `level`, `event` (código), `error_id` (UUID aleatorio, también devuelto en la página de error), `route_id` (`event.route.id`, la plantilla, nunca la URL), `status`, `error_name`, `error_message` y `stack`, estos dos últimos pasados por un saneador que quita direcciones IPv4 e IPv6, el query string de cualquier URL y el valor que siga al nombre de un header sensible (`cookie`, `authorization`, `x-forwarded-for`, `x-internal-secret`, `x-client-ip`). **Nunca** se registran `getClientAddress()`, `X-Forwarded-For`, cookies, headers de la petición, la URL ni el cuerpo.
  **`handleError` en Kit 3** (enmienda 2026-10-22). Kit 3 pasa **todos** los errores por `handleError`, incluidos los esperados (`error(...)`) y los 404/405/413 del framework, y le entrega un discriminante `kind`:
  - Solo se registra `kind: 'unknown'` (el error lo lanzó nuestro código o algo que llama). Con `'app'`, `'framework'` y `'validation'` no se escribe nada: registrarlos sería registrar peticiones (cualquiera genera 404 a voluntad).
  - El `error_id` se genera **solo** para los errores que se registran, así que todo `error_id` que ve un usuario existe en los logs. Los demás casos no lo llevan.
  - El objeto que devuelve nunca copia `error.message` ni el error crudo: `message` es un código fijo de una lista cerrada, y `status` es el que calcula Kit (no se sobrescribe).
  - **Nunca lanza ni rechaza**: todo su cuerpo va en `try/catch` y, si algo falla, devuelve un objeto mínimo sin registrar el error original. Si `handleError` falla, Kit 3 escribe el error original con `console.error`, sin pasar por el saneador.
  - Excepción conocida, fuera de `log.ts`: adapter-node 6 escribe una línea con `console.error` y responde 400 cuando no puede armar el origen de la petición (sin `Host`). Con `PROTOCOL_HEADER`, `HOST_HEADER` y `PORT_HEADER` sin definir (RF-C4, RF-C6), el texto de esa línea es fijo y no lleva datos del cliente.
  → e2e del criterio A5 (ver Estrategia de pruebas); unit del saneador; unit de `handleError` con los cuatro `kind` (solo `unknown` escribe una línea), con un error cuyo `message` es un *getter* que lanza (no lanza y no escribe el error original) y con un error 404 de `error(...)` (no escribe nada). e2e: `GET /es/no-existe` → 404 sin ninguna línea en stdout ni stderr.
- **RF-A20.** Ruta de prueba `src/routes/dev/error-probe/+page.server.ts` que lanza un error cuyo mensaje incluye una URL con query y una IP de canario; solo existe detrás del guard de RF-A16. → la usa el e2e del criterio A5.

**Comandos y docs**
- **RF-A21.** `justfile` en la raíz con, al menos: `dev` (en A, solo la web), `check`, `test`, `e2e` y `fmt`. `just check` corre, en este orden y fallando en el primero que falle: Prettier (check), ESLint, `svelte-check`, Vitest, `check-tokens.mjs`, `check-i18n.mjs` y el chequeo de `prerender` y de la configuración CSRF de Kit (RF-A2, RF-A10). `just e2e` corre Playwright (no forma parte de `just check`, para que el `pre-push` sea rápido; lo corre la CI). → CI y PR.
- **RF-A22.** El PR de la fase A actualiza "Comandos" de `CLAUDE.md` y, en el aviso de privacidad, la fila "Preferencia de idioma" de la sección 6 (nombre `locale`, 1 año) y el punto 5 de "Pendiente". → revisión del PR (security-reviewer).

### Fase B — server, protocol y Postgres

**Workspace Rust**
- **RF-B1.** `Cargo.toml` de workspace con miembros explícitos `apps/server` y `crates/protocol` (no globs: `crates/engine` y `crates/engine-wasm` siguen vacíos). `rust-toolchain.toml` con una versión estable fija y el target `x86_64-unknown-linux-musl`. Perfil `release` según ADR-0006 D8 (`lto = "thin"`, `codegen-units = 1`, `strip = true`). → CI.
- **RF-B2.** `deny.toml` con la sintaxis actual: `[graph] targets = ["x86_64-unknown-linux-musl"]` y `[bans] deny = [{ crate = "openssl-sys" }, { crate = "openssl" }, { crate = "aws-lc-sys" }]`. sqlx con `runtime-tokio` + `tls-rustls-ring`; `mimalloc` como `#[global_allocator]` (ADR-0006 D8). → CI (`cargo deny check bans`).

**Protocol y tipos TS**
- **RF-B3.** `crates/protocol` es una librería pura: sin I/O, sin `tokio`, sin reloj, y sin texto para humanos (ADR-0007). Dependencias normales: `serde` (y `serde_json` solo en tests). `ts-rs` es **opcional**, detrás de la feature `ts` (`#[cfg_attr(feature = "ts", derive(TS))]`), para que el server y el futuro `engine-wasm` no la compilen. → `cargo tree -p server -e normal` no contiene `ts-rs` (chequeo en `just check`).
- **RF-B4.** Convenciones del protocolo (se copian a "Tipos compartidos" de `docs/architecture.md`):
  - Enums con datos: `#[serde(tag = "type", rename_all = "snake_case")]`. Enums de códigos: **`#[serde(rename = "dominio.motivo")]` en cada variante**, sin `rename_all` (enmienda 2026-10-23 b): `rename_all = "snake_case"` daría `origin_forbidden` sin el dominio. `ts-rs` respeta el `rename` y genera la unión de cadenas literales.
  - Códigos de error como `dominio.motivo` (`ws.origin_forbidden`), definidos como enums, no como `String` libre. Cada enum de códigos es dueño de un solo prefijo de dominio (`ApiErrorCode` → `http.*`, `WsErrorCode` → `ws.*`).
  - **Nada de enteros de 64 bits o más** (`u64`, `i64`, `u128`, `i128`, `usize`) en tipos exportados: `ts-rs` los mapea a `bigint` y `serde_json` los serializa como número, que JS pierde por encima de 2^53. Si hace falta, se serializan como cadena con `#[ts(type = "string")]` y un serializador explícito. → `just gen` falla si algún archivo generado contiene `bigint`.
- **RF-B5.** Tipos de 001 en `crates/protocol`:
  - `HealthResponse`: `{"status":"ok","version":"<APP_VERSION>"}` o `{"status":"db_unavailable"}` (enum con `tag = "status"`).
  - `ClientMessage` y `ServerMessage` del WS, con una sola variante `Echo { payload: String }` cada uno (`{"type":"echo","payload":"…"}`).
  - `ApiErrorCode` con, por ahora, `http.not_found` y `http.method_not_allowed`; `WsErrorCode` con `ws.origin_forbidden`, `ws.capacity`, `ws.invalid_message`, `ws.echo_disabled`, `ws.rate_limited`, `ws.pong_timeout` y `ws.idle_timeout`.
  - `ErrorCode`, unión **sin etiquetar** de los dos: `#[serde(untagged)] enum ErrorCode { Http(ApiErrorCode), Ws(WsErrorCode) }`, que en TS es `ApiErrorCode | WsErrorCode`. `ApiError { error: ErrorCode }` es el cuerpo de **toda** respuesta HTTP de error del server, incluidas las del handshake de `/ws` (403 `ws.origin_forbidden`, 503 `ws.capacity`), así que ningún código existe dos veces (enmienda 2026-10-23 b).
  - **Los conjuntos de cadenas no se solapan**: ninguna cadena serializada de `ApiErrorCode` es igual a una de `WsErrorCode` (lo garantiza el prefijo de RF-B4). Con `untagged`, serde prueba las variantes en orden, y un solapamiento haría ambigua la deserialización.
  - **Los códigos de dominios nuevos van en un enum y una variante nueva** de `ErrorCode` (por ejemplo, `AuthErrorCode` con `auth.csrf_origin` y `ErrorCode::Auth`), nunca dentro de `ApiErrorCode` ni de `WsErrorCode`.
  → tests de serialización (forma JSON exacta de cada caso, también a través de `ErrorCode` y `ApiError`, ida y vuelta); unit de no solapamiento: cada enum de códigos expone `ALL: &[Self]`, un `match` exhaustivo en el test obliga a que `ALL` liste todas las variantes, y el test comprueba que cada cadena empieza con el prefijo de su enum y que la unión de los conjuntos no tiene repetidos.
- **RF-B6.** `just gen` genera los tipos con `ts-rs` (`cargo test -p protocol --features ts`, con `TS_RS_EXPORT_DIR` apuntando a `packages/protocol-ts/src/generated/`), escribe el barril `packages/protocol-ts/src/index.ts`, formatea con Prettier y antepone a cada archivo un comentario de "archivo generado, no editar". Los archivos generados **se commitean**. `packages/protocol-ts` es el paquete pnpm `@torneo/protocol`, exporta el TS fuente (sin paso de build) y lo consume `apps/web`. → CI: `just gen` seguido de `git diff --exit-code -- packages/protocol-ts` en el job `rust`.
- **RF-B7.** La web usa los tipos: `src/lib/features/system/health.ts` y `src/lib/features/system/echo.ts` usan `HealthResponse`, `ClientMessage` y `ServerMessage` de `@torneo/protocol` y mapean cada código a una llave de Paraglide: una `ws_close_reason_*` por cada `WsErrorCode` (incluida `ws_close_reason_rate_limited` para `ws.rate_limited`; enmienda 2026-10-23 b), con un `switch` exhaustivo que no compila si falta alguna. La página `src/routes/dev/status/+page.svelte` (detrás del guard de RF-A16) muestra el estado del server y hace un eco de ida y vuelta. → `svelte-check` y Vitest de `health.ts` con fixtures de los dos casos de `HealthResponse` (criterio B2 del intent).

**Server**
- **RF-B8.** Configuración por variables de entorno (tabla en "Diseño"). Si falta una obligatoria o es inválida, el server registra `config.invalid` con el **nombre** de la variable (nunca su valor) y sale con código 1. `PUBLIC_ORIGIN` debe ser un origen (`esquema://host[:puerto]`, sin ruta ni barra final). → unit e integración.
- **RF-B9.** Arranque (ADR-0006 D3 y D10): conecta a la BD reintentando cada 2 s hasta 60 s; si no lo logra, sale con código 1. Después corre `sqlx::migrate!` y **solo entonces** abre el puerto. Pool con `max_connections = 8`. Hay una migración inicial (`…_baseline.sql`) sin tablas de dominio, que solo prueba la cadena de migraciones. → integración: tras arrancar, `_sqlx_migrations` contiene la baseline; con una BD inalcanzable, la función de espera devuelve error en el plazo configurado (el test la llama con un plazo corto) y `main` lo convierte en código 1.
- **RF-B10.** `GET /api/health` (ADR-0006 D3): `SELECT 1` con timeout de 1 s. El resultado de la BD se **cachea 1 s** y hay como mucho una consulta en vuelo (las peticiones concurrentes esperan la misma), para que una ráfaga externa no ocupe el pool de 8 conexiones ni tumbe el healthcheck. Responde 200 con `HealthResponse::Ok { version }` o 503 con `HealthResponse::DbUnavailable`, con `Cache-Control: no-store`. Sin rate limit y sin sesión. `version` sale de `option_env!("APP_VERSION")` en compilación, con `dev` por defecto, y debe cumplir `^(v\d+\.\d+\.\d+|sha-[0-9a-f]{7}|dev)$`. → integración (criterio B3 del intent), con un proxy TCP del test entre el server y Postgres que el test corta y restaura (200 → 503 → 200 sin reiniciar el server; el test espera más que el TTL de la caché entre cada paso), y un caso con 50 peticiones concurrentes que generan una sola consulta (contador del proxy).
- **RF-B11.** Subcomando `server healthcheck`: `GET http://127.0.0.1:${PORT}/api/health` con timeout de 2 s (supone que `BIND_ADDR` es `0.0.0.0` o `127.0.0.1`; ver la tabla de variables); sale con 0 si recibe 200 y con 1 en cualquier otro caso (503, timeout, conexión rechazada). Cliente HTTP sin TLS (no arrastra rustls ni OpenSSL a este camino). → integración con el binario real: 0 con la BD arriba, 1 con el proxy cortado y 1 sin server.
- **RF-B12.** Rutas desconocidas: 404 con `ApiError { error: "http.not_found" }`. Ninguna respuesta lleva `Access-Control-Allow-*`, tampoco un preflight `OPTIONS` con `Origin` ajeno: no se monta `CorsLayer` (ADR-0008 D4). Todas las respuestas llevan `X-Content-Type-Options: nosniff` y `Strict-Transport-Security: max-age=31536000` (como en RF-A10). **Solo lectura hasta auth**: una capa en la raíz del router responde 405 `http.method_not_allowed` a todo método distinto de `GET`, `HEAD` y `OPTIONS`, en cualquier ruta. La feature de auth la reemplaza por el middleware de `Origin` de ADR-0008 D4, así que nadie agrega una mutación sin quitarla a conciencia ("Áreas de preocupación" 2). → integración (criterio B5 del intent), con `POST /api/health` → 405 y las dos cabeceras en una respuesta 200 y en una 404.
- **RF-B13.** **WebSocket `/ws`** (ver "Diseño → WebSocket"):
  - **Orden de los chequeos del handshake** (enmienda 2026-10-23 b): `Origin` → upgrade → semáforo. El primero que falla responde y los siguientes no corren. Así, una petición sin `Origin` válido nunca llega a los demás chequeos, y solo una petición de upgrade válida toma un permiso del tope.
    1. `Origin` debe ser exactamente `PUBLIC_ORIGIN`; si falta o es ajeno, 403 con `ApiError { error: "ws.origin_forbidden" }` y no hay upgrade (ADR-0008 D6).
    2. **Upgrade**: `WebSocketUpgrade` se extrae como `Result<_, WebSocketUpgradeRejection>`. Si la petición no es un upgrade válido (por ejemplo, `GET /ws` con `Origin` válido y sin `Connection: upgrade`/`Upgrade: websocket`), responde con **el estado que da el rechazo de axum** (un 4xx: 400, o 426 si la conexión no se puede actualizar), **cuerpo vacío** y sin `Content-Type`, para no emitir el texto en inglés del rechazo (ADR-0007: el API solo emite códigos). No se agrega un código nuevo. Lleva las cabeceras de RF-B12 (`nosniff` y HSTS) y genera la línea normal de RF-B15 con `route: "/ws"`.
    3. Tope global de sockets `WS_MAX_CONNECTIONS` (por defecto 2000), con un semáforo: si no hay permiso, 503 `ws.capacity`. El permiso vive lo que vive el socket.
  - Por socket: mensaje de 4 KB como máximo, **fijado en el protocolo** con `max_message_size` y `max_frame_size` de `WebSocketUpgrade` (no se comprueba después de recibir: los valores por defecto de tungstenite, 64 MB por mensaje y 16 MB por frame, permitirían un OOM-kill con 256 MB); al pasarlo, cierre `1009`, o corte de la conexión si tungstenite ya la invalidó. 10 mensajes por cada 10 s (cierre `1008` con razón `ws.rate_limited`; enmienda 2026-10-23 b); un frame binario cierra con `1003`; un texto que no es un `ClientMessage` válido cierra con `1008` y razón `ws.invalid_message`.
  - **Vida del socket** (no depende de auth): el server manda un ping cada **30 s**; si no recibe ningún frame del cliente (pong incluido) en **60 s**, cierra con `1008` y razón `ws.pong_timeout`, y suelta la conexión TCP si el cierre no se completa en 5 s. **Inactividad**: un socket que no envía ningún mensaje de datos aceptado en **120 s** se cierra con `1008` y razón `ws.idle_timeout`; en 001, con el eco apagado no hay mensaje aceptado, así que en producción todo socket vive como máximo 120 s. La feature del WS real redefine la inactividad (por ejemplo, "sin suscripciones"), porque un espectador escucha sin hablar. Los tres valores son campos de la configuración del WS con esos valores por defecto; **no** son variables de entorno, y los tests los acortan al construir el server.
  - **Eco**: si `ENABLE_WS_ECHO === "1"`, un `Echo { payload }` se responde con `ServerMessage::Echo` con el mismo `payload`. Con cualquier otro valor, o sin la variable, un `Echo` cierra con `1008` y razón `ws.echo_disabled`. **Defensa extra**: si `ENABLE_WS_ECHO === "1"` pero `PUBLIC_ORIGIN` empieza con `https://`, el server deja el eco apagado y registra `warn` `ws.echo_refused` al arrancar (dev, tests y e2e usan `http://localhost`); si lo enciende, registra `warn` `ws.echo_enabled`. Esto cubre una variable puesta a mano en la UI de Coolify, que `check-compose.mjs` no ve.
  - El server nunca registra `payload` ni el valor de `Origin`.
  → integración (criterio B4 del intent y los casos de cada límite, incluidos: `GET /ws` con `Origin` válido y sin cabeceras de upgrade → 4xx, cuerpo vacío, `nosniff`, HSTS y una línea de petición con `route: "/ws"`; `GET /ws` sin `Origin` y sin upgrade → 403 `ws.origin_forbidden`; con el tope en 0, una petición sin upgrade da el 4xx y no 503; un cliente que no lee el socket, y por eso no contesta pings, queda cerrado tras el plazo de pong; un cliente que contesta pings pero no manda nada se cierra con `ws.idle_timeout`; y el eco con `PUBLIC_ORIGIN=https://…` queda apagado).
- **RF-B14.** Apagado ordenado (ADR-0006 D10): con `SIGTERM` o `SIGINT`, deja de aceptar conexiones, cierra cada WS con `1012` y termina en ≤ 10 s. → integración: un cliente WS conectado recibe `1012` y el proceso sale con 0 antes de 10 s.

**Logs del server (ADR-0008 D3 y aviso, "Pendiente" punto 8)**
- **RF-B15.** `tracing` con salida JSON por línea a stdout; `RUST_LOG` por defecto `info`. Una línea por petición HTTP, con **solo** estos campos: `ts`, `level`, `method`, `route` (la plantilla de `MatchedPath`, o `unmatched`; **nunca** la URI ni el query string), `status`, `latency_ms` y `request_id` (UUID aleatorio por petición, sin relación con el usuario). No se registran headers, cuerpo, IP, `ip_hash`, user agent, `user_id` ni identificadores de sesión. Como defensa adicional, `SetSensitiveRequestHeadersLayer`/`SetSensitiveResponseHeadersLayer` marcan `Cookie`, `Set-Cookie`, `Authorization`, `X-Internal-Secret`, `X-Client-Ip` y `X-Forwarded-For`. Los errores de conexión a la BD se registran por tipo (`db.unavailable`), nunca con `DATABASE_URL` ni con las opciones de conexión. → integración con captura del subscriber (ver Estrategia de pruebas).
- **RF-B16.** `/api/health` se registra en `info` como cualquier otra petición (lo pide la verificación 1 de ADR-0006 y además acota el plazo de los logs, ver "Diseño → Logs"). → integración.

**Forma local y comandos**
- **RF-B17.** `infra/compose.dev.yml` con **solo** `postgres:17-alpine` (por digest), publicado en `127.0.0.1:${DEV_PG_PORT:-5432}`, con usuario, contraseña y BD de desarrollo (`torneo`), un volumen nombrado y `pg_isready` como healthcheck. `just dev` levanta ese compose con `--wait`, arranca el server (`cargo run -p server`) y la web (`pnpm --filter web dev`) en paralelo, y al recibir Ctrl-C detiene los dos procesos (Postgres sigue arriba; `just db-down` lo baja). Las migraciones se aplican solas al arrancar el server. El proxy de Vite reenvía `/api` y `/ws` (con `ws: true`) a `localhost:8080` sin reescribir la ruta (ADR-0006 D2). → PR (criterio B1 del intent: un comando, migraciones aplicadas, `/es/dev/status` muestra el server `ok` y el eco).
- **RF-B18.** `apps/server/.env.example` y `apps/web/.env.example` con valores de desarrollo (`PUBLIC_ORIGIN=http://localhost:5173` en los dos, porque `web` también la exige desde la enmienda 2026-10-22 (RF-A23); `ENABLE_WS_ECHO=1` y `DATABASE_URL` local en el server; `ENABLE_DEV_CATALOG=1` en la web). Nunca un secreto real. → revisión del PR.
- **RF-B19.** `just check` agrega: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `just db-up` (idempotente) y `cargo test --workspace`, la frescura de `just gen` y el chequeo de `ts-rs` fuera del grafo del server (RF-B3). Las recetas que corren tests fijan `DATABASE_URL` al Postgres de `compose.dev.yml` si no viene definida. **Es el mismo camino en local y en CI**: el job `rust` no declara `services:` y usa este `just db-up` (RF-C9). `just deny` corre `cargo deny check bans` (en CI siempre; en local, si está instalado). → CI y PR (criterio B6 del intent).
- **RF-B20.** El PR de la fase B actualiza "Comandos" de `CLAUDE.md`, "Tipos compartidos" de `docs/architecture.md` (con `ts-rs` y las convenciones de RF-B4) y, en el aviso de privacidad, `{{PLAZO_LOGS}}` (con el valor calculado en B, que la fase C confirma; ver "Diseño → Logs y plazo"), la fila "Registros técnicos del servidor" (sin IP, cookies ni identificadores de usuario), una aclaración de que los registros de `web` son solo de errores, sin datos del cliente, y pueden durar más que los del server, y el punto 8 de "Pendiente". → revisión del PR (security-reviewer).

### Fase C — infra y CI

**Imágenes (ADR-0006 D5 y D8)**
- **RF-C1.** `apps/server/Dockerfile` con `cargo-chef` (`planner` → `cook` → `build`). Las etapas de compilación usan `FROM --platform=$BUILDPLATFORM` y compilan **en cruzado** para `x86_64-unknown-linux-musl` con `cargo-zigbuild`, así una Mac ARM compila sin emular (el build corre nativo en arm64 dentro de la VM de Docker Desktop; Rosetta solo interviene al ejecutar). Detalles:
  - `zig` en versión fija, con **un checksum por arquitectura del builder** (`BUILDARCH` `amd64` en Actions, `arm64` en Mac); un `BUILDARCH` sin checksum hace fallar el build.
  - `cargo-chef` y `cargo-zigbuild` con `cargo install --locked --version <fija>`.
  - `cargo chef cook --zigbuild --release --target x86_64-unknown-linux-musl`.
  - `ARG APP_VERSION` se declara **después** del `cook`, para que cambiar la versión no invalide la capa de dependencias.
  - **Todas** las etapas `FROM` por digest, incluido el builder `rust:<versión>`, cuya versión es la misma de `rust-toolchain.toml` (si no, rustup descarga la toolchain en cada build en frío; `check-compose.mjs` compara las dos).
  - Usa `SQLX_OFFLINE=true` y termina en `gcr.io/distroless/static-debian13:nonroot` (enmienda 2026-10-24), con el binario en `/server`.
  → job `images`: construye, y `readelf -l` sobre el binario extraído no muestra `INTERP` (un binario musl es `static-pie` y el `ldd` de glibc puede responder "statically linked" en vez de "not a dynamic executable"; `readelf` cubre la verificación 8 de ADR-0006 sin depender de esa salida).
- **RF-C2.** `apps/web/Dockerfile`: etapa de build con `FROM --platform=$BUILDPLATFORM node:22-…@sha256:…` (el resultado de `adapter-node` es JS, sin código nativo), y final `gcr.io/distroless/nodejs22-debian13:nonroot` por digest (enmienda 2026-10-24). Los dos digests corresponden a Node ≥ 22.17 (mínimo de SvelteKit 3), y el de build usa la misma versión que `.node-version` (RF-A1). Todas las dependencias de `apps/web` van en `devDependencies` para que Vite las empaquete: la imagen final contiene `build/` y `package.json`, **sin `node_modules`**. → job `images`: `/app/node_modules` no existe en la imagen y un **smoke test** (`docker run` de la imagen con `read_only`, `PUBLIC_ORIGIN=http://localhost:3000` y el puerto atado a `127.0.0.1`; sin `ORIGIN`, que adapter-node 6 ya no lee) responde 200 en `/healthz` y en `/en/`, y el mismo `docker run` **sin** `PUBLIC_ORIGIN` termina con código distinto de 0 antes de abrir el puerto (RF-A23). Esto prueba que el bundle es autocontenido, cosa que los e2e no prueban porque corren con `node_modules` presente.
- **RF-C3.** `.dockerignore` en la raíz que excluye, al menos, `.git`, `**/target`, `**/node_modules`, `**/.env`, `**/.env.*` (con `!**/.env.example`), `private/` y `content/_local/`. Las formas con `**/` son obligatorias: en `.dockerignore`, `.env` sin prefijo solo excluye el de la raíz del contexto, y un `infra/.env` o `apps/server/.env` acabaría en una capa del builder y en la caché de buildx. → `check-compose.mjs` verifica esas entradas con su forma `**/`, y `scripts/check-dockerignore.sh` (job `images`) lo prueba con Docker: crea `infra/.env` y `apps/server/.env` de prueba (los borra con `trap`), construye `scripts/fixtures/context.Dockerfile` (`FROM scratch` + `COPY . /ctx`) con `--output type=local` a un directorio temporal, y comprueba que en la salida no existen `ctx/infra/.env`, `ctx/apps/server/.env`, `ctx/private` ni `ctx/.git`, y sí existe `ctx/apps/server/.env.example`. No necesita ninguna imagen base.

**Compose de producción (ADR-0006 D1–D4, D11)**
- **RF-C4.** `infra/docker-compose.yml` con **solo** `web` y `server`, con `image: ghcr.io/kike1303/torneo-absurdo/{server,web}:${IMAGE_TAG:?}` y sin `build:`; redes externas `torneo-edge` y `torneo-db` y el alias `torneo-server` (D1); las etiquetas Traefik `torneo-*` de D2, con `observability.accesslogs=false` en los cuatro routers y la regla `Host` construida con `${PUBLIC_HOST:?}`; healthchecks en forma exec con 15 s / 3 s / 3 / 30 s (D3); `web` con `depends_on: server: service_healthy`; `mem_limit` = `memswap_limit` (256 MB y 384 MB) y `NODE_OPTIONS=--max-old-space-size=256` (D4); `restart: unless-stopped`; logs **explícitos** `logging: { driver: json-file, options: { max-size: "10m", max-file: "3" } }` (el plazo de "Diseño → Logs" supone el envoltorio de `json-file`, y no se depende del driver por defecto del daemon del VPS); `stop_grace_period: 15s` en `server`; sin `ports:`. Además, como endurecimiento: `platform: linux/amd64`, `read_only: true` (con `tmpfs: /tmp:size=16m` en `web`; sin tamaño, el tmpfs puede crecer hasta la mitad de la RAM del host y lo escrito cuenta contra los 384 MB), `cap_drop: [ALL]` y `security_opt: ["no-new-privileges:true"]` en los dos. `web` recibe `PUBLIC_ORIGIN=https://${PUBLIC_HOST}` (el mismo valor que el server; lo usa el guard de RF-A23), `ADDRESS_HEADER=X-Forwarded-For`, `XFF_DEPTH=1` (ADR-0008 D5), `HOST=0.0.0.0` y `PORT=3000`. **Origen de la petición en `web`** (enmienda 2026-10-22): adapter-node 6 quitó `ORIGIN` y `paths.origin` no se define (RF-A10), así que `event.url.origin` es `https://` + `Host`. `web` **no** recibe `ORIGIN`, `PROTOCOL_HEADER`, `HOST_HEADER` ni `PORT_HEADER`: el esquema queda fijo en `https` (Traefik termina TLS) y el host sale del `Host` que Traefik reenvía, que su regla `Host(${PUBLIC_HOST})` ya restringe. **Detrás de Traefik** (enmienda 2026-10-23), ningún header que el cliente pueda falsificar cambia el origen. Quien llega directo al contenedor (`prod-local` en `127.0.0.1:3000`, u otro contenedor de `torneo-edge`) sí elige el `Host`, y por eso `web` nunca arma URLs absolutas con `event.url` ("Seguridad"). **Cabeceras en Traefik** (enmienda 2026-10-23): el router `torneo-web` lleva el middleware propio `torneo-web-headers` (prefijo de ADR-0006 D2), de tipo `headers`, con `contentTypeNosniff=true`, `referrerPolicy=strict-origin-when-cross-origin`, `stsSeconds=31536000`, `stsIncludeSubdomains=false` y `stsPreload=false`, los mismos valores que RF-A10. Así cubre las respuestas de `web` que no pasan por `handle`: los estáticos de `/_app/*` y de `static/`, que sirve `sirv`, y la 308 de Kit de `/healthz/` a `/healthz` (desviación 16 de `plan-a.md`). `hooks.server.ts` sigue poniendo las suyas (RF-A10), porque `prod-local` y los e2e no tienen Traefik. Cada cabecera debe salir **una sola vez**: si Traefik y `handle` la duplican, se corrige en el middleware y no se quita de `handle`. `torneo-web-http` solo redirige y no lleva este middleware. `server` recibe `DATABASE_URL=${DATABASE_URL:?}`, `PUBLIC_ORIGIN=https://${PUBLIC_HOST}` y `RUST_LOG=info`. → `check-compose.mjs` y job `images` (`docker compose config` sin `IMAGE_TAG` sale con error; criterio C7 del intent).
- **RF-C5.** `infra/local.env`, **commiteado**, con los valores de la forma local y ningún secreto: `PUBLIC_HOST=localhost` y el `DATABASE_URL` del Postgres local de `compose.local.yml` (usuario y contraseña de desarrollo). Un comentario al inicio dice que las variables de producción (`IMAGE_TAG`, `PUBLIC_HOST`, `DATABASE_URL`) viven **solo en Coolify** y que no se crea `infra/.env` con valores reales. No hay `infra/.env.example`: este archivo cumple esa función sin invitar a guardar el `DATABASE_URL` real en el disco. → revisión.
- **RF-C6.** `scripts/check-compose.mjs` (en `just check` y en el job `images`) analiza el YAML crudo de `infra/docker-compose.yml` y falla si: hay servicios distintos de `web` y `server` (esta regla no aplica a `compose.local.yml`, que agrega `postgres`); hay `build:` o `ports:`; una imagen no termina en `:${IMAGE_TAG:?}`; falta alguna etiqueta o valor de RF-C4 (incluidos `logging` y el tamaño del `tmpfs`); aparece `ENABLE_DEV_CATALOG` o `ENABLE_WS_ECHO`; aparece `ORIGIN`, `PROTOCOL_HEADER`, `HOST_HEADER` o `PORT_HEADER` en `web`; aparece `BIND_ADDR` en `server` (enmienda 2026-10-23 b: el contenedor usa el valor por defecto `0.0.0.0`, que es el que supone `server healthcheck`); falta el middleware `torneo-web-headers` en el router `torneo-web`, o le falta alguno de sus cinco valores de RF-C4, o `stsIncludeSubdomains` o `stsPreload` valen `true` (enmienda 2026-10-23); el `PUBLIC_ORIGIN` de `web` no es idéntico al de `server`; o `RUST_LOG` vale algo más verboso que `info`. Además falla si faltan en `.dockerignore` las entradas de RF-C3 con su forma `**/`, o si la versión del `FROM rust:` de `apps/server/Dockerfile` no coincide con `rust-toolchain.toml`. → unit con fixtures.

**Forma local con las imágenes de producción**
- **RF-C7.** `just prod-local [tag]`:
  1. Crea las redes si no existen, con los mismos parámetros que en el VPS (`torneo-edge`: `--subnet 10.250.0.0/28 --gateway 10.250.0.1 --ip-range 10.250.0.8/29`; `torneo-db`: bridge sin subred fija) y la etiqueta `--label torneo.local=1`, con `scripts/local-networks.sh`, idempotente. Si una red con ese nombre ya existe sin la etiqueta, la usa sin modificarla y lo avisa. Si la subred choca con una red local, falla con un mensaje que explica cómo resolverlo.
  2. Sin `tag`, construye las dos imágenes con `docker buildx build --platform linux/amd64 --load` e `IMAGE_TAG=local`; con `tag`, hace pull de GHCR.
  3. Exporta `IMAGE_TAG` en el entorno del comando y levanta `docker compose --env-file infra/local.env -f infra/docker-compose.yml -f infra/compose.local.yml up -d --wait`.
  **Interpolación**: Compose interpola cada archivo antes de combinarlos y solo con el entorno del shell y el `--env-file` (nunca con el `environment:` de un servicio), así que `${IMAGE_TAG:?}`, `${PUBLIC_HOST:?}` y `${DATABASE_URL:?}` del compose de producción se resuelven con la variable exportada por la receta y con `infra/local.env` (RF-C5). El `--env-file` explícito evita además que Compose cargue un `infra/.env` del desarrollador.
  `infra/compose.local.yml` solo **agrega**: el servicio `postgres` en `torneo-db` (mismo digest que el de dev, 512 MB) y `ports` en `127.0.0.1:3000` y `127.0.0.1:8080`. No fija variables que se interpolan, ni toca redes, imágenes o etiquetas. `just prod-local-down` baja el stack y **solo** borra las redes con la etiqueta `torneo.local=1`, así que nunca borra un `torneo-edge` del VPS (advertencia de devops en RF-C18). → PR: en Linux amd64 y en Mac ARM (Docker Desktop con "Use Rosetta for x86_64/amd64 emulation"), los tres contenedores quedan `healthy`, `curl 127.0.0.1:8080/api/health` da 200 con `version` y `curl 127.0.0.1:3000/healthz` da 200 (criterio C7 del intent). **Cabeceras** (enmienda 2026-10-23): `curl -I` a `127.0.0.1:3000/es/` muestra `X-Content-Type-Options`, `Referrer-Policy` y `Strict-Transport-Security` sin `includeSubDomains`, cada una una sola vez. El PR pega también el `curl -I` de `/_app/version.json` y de `/healthz/`. `prod-local` no tiene Traefik, así que ahí esas dos respuestas salen sin las cabeceras; esa es la línea base que cubre el middleware de RF-C4, y la prueba con Traefik va en RF-C18.

**CI (ADR-0006 D7)**
- **RF-C8.** `.github/workflows/ci.yml` en `pull_request` y `push` a `main`, con los jobs `changes`, `policy`, `rust`, `web`, `e2e`, `images` y `ci-ok`, la caché, `concurrency`, `timeout-minutes` (`policy` 5 min y `ci-ok` 5 min, además de los de D7) y los permisos de D7. Filtros: los de D7 **más** estas extensiones:
  - `rust`: `packages/protocol-ts/**` (un cambio a mano en los generados dispara la frescura de RF-B6) y `deny.toml` (un PR que quite `openssl-sys` de `bans.deny` corre `cargo deny`).
  - `web`: `scripts/**` (los tests de `check-i18n.mjs`, `check-tokens.mjs` y `check-compose.mjs`, con sus fixtures, corren en Vitest dentro de este job), `.npmrc` y la configuración de formato y lint de la raíz (`.prettierrc*`, `.prettierignore` y `eslint.config.*`, si viven ahí).
  - `images`: `scripts/check-compose.mjs`, `scripts/check-dockerignore.sh`, `scripts/fixtures/context.Dockerfile`, `Cargo.lock`, `rust-toolchain.toml` y `pnpm-lock.yaml` (un cambio de dependencias que rompa el build cruzado se ve en el PR y no en el release, y la caché de `main` de las imágenes sigue caliente para el release, RF-C13).
  `ci-ok` depende de todos (incluido `policy`), corre con `if: always()` y es el único check requerido. → PRs de prueba (criterio C1 del intent).
- **RF-C9.** Job `rust`: **sin `services:`**; corre `just check` en su parte Rust, que levanta Postgres con el mismo `just db-up` que en local (RF-B19; ubuntu-latest trae Docker, y el digest de `compose.dev.yml` lo actualiza Dependabot). Además, `cargo deny check bans` y la compilación de `server` para `x86_64-unknown-linux-musl` (perfil `dev`, con `musl-tools`), con la comprobación de `readelf -l` sin `INTERP` de RF-C1. El PR de C mide la duración del job con caché caliente frente al objetivo de ≤ 8 min (p50), porque la compilación musl duplica compilación y caché. → CI (criterio C2 del intent).
- **RF-C10.** Job `web`: la parte web de `just check`, incluidos los tests de los scripts de `scripts/` salvo `check-workflows.mjs` (que corre en `policy`). Job `e2e`: Playwright contra `node build`, con **dos instancias**: una con `ENABLE_DEV_CATALOG=1` para casi todos los tests y otra sin la variable, en otro puerto, para el test 6 de ADR-0009 y el e2e del guard de RF-A16. Cada instancia recibe `PUBLIC_ORIGIN=http://localhost:<su puerto>` (RF-A23). En Chromium; los tests de tema (test 5) y CSP corren también en WebKit. Sube las capturas del test 8 como artefacto. → CI.
- **RF-C11.** Job `images`: construye las dos imágenes con `push: false`, `provenance: false`, `sbom: false`, `platforms: linux/amd64`, `cache-from: type=gha,scope=<imagen>` y, **solo en `push` a `main`**, `cache-to: type=gha,mode=max,scope=<imagen>` (`mode=max` guarda la etapa `cook` de `cargo-chef`; el `scope` evita que las dos imágenes se pisen la caché). Comprueba RF-C1 (binario sin `INTERP`), RF-C2 (sin `node_modules` y smoke test de `web`), RF-C3 (`check-dockerignore.sh`), `check-compose.mjs` y que `docker compose -f infra/docker-compose.yml config` **sin** `IMAGE_TAG` falla. → CI (criterio C4 del intent).
- **RF-C12.** Job `policy` (siempre corre, sin `if:`). Instala solo lo mínimo: Node, las dependencias de los scripts (un parser de YAML, sin navegadores ni toolchain Rust) y `just` en versión fija verificada por checksum, para no comerse el objetivo de ≤ 2 min de un PR de docs. Corre `scripts/check-workflows.mjs`, que analiza el YAML de `.github/workflows/*.yml` **y de `.github/actions/**`** y falla si:
  - algún workflow usa `pull_request_target`, o `workflow_run`;
  - en cualquier workflow o action hay una referencia al contexto `secrets` distinta de `secrets.GITHUB_TOKEN`, en cualquier sintaxis (`secrets.X`, `secrets[...]`, `toJSON(secrets)`, `secrets: inherit`). En 001 ningún workflow necesita otro secreto; el modo `api` de `deploy` lo necesitaría y por eso requiere un nuevo OK del owner (RF-C14);
  - una action de terceros no está fijada por SHA de 40 caracteres;
  - un workflow no declara `permissions` a nivel raíz **igual a `contents: read`** (los permisos extra solo van por job: `packages: write` en `release.yml/build` y `pull-requests: read` en `ci.yml/changes`), o un job no tiene `timeout-minutes`;
  - un paso `actions/checkout` no lleva `persist-credentials: false`;
  - un `run:` interpola con `${{ }}` datos controlables (`inputs.*`, `github.event.*`, `github.head_ref`, `github.ref_name`): deben pasar por `env:`;
  - aparece `coolify` (sin distinguir mayúsculas) o `/api/v1/deploy` en cualquier workflow (en 001 no hay llamadas a Coolify, ver RF-C14; el mensaje de fallo de `DEPLOY_MODE=api` no contiene esa palabra);
  - aparece una fusión automática (`gh pr merge`, `enable-pull-request-automerge` o `dependabot/fetch-metadata`): los PRs de Dependabot nunca se fusionan solos;
  - el job `deploy` de `release.yml` no declara `environment: production`.
  También corre una prueba de `just hooks` en un clon temporal (RF-C17). → unit con fixtures (uno por regla, incluidas las cuatro sintaxis de `secrets`) y CI (criterio C3 del intent).

**Release (ADR-0006 D5 y D6)**
- **RF-C13.** `.github/workflows/release.yml`, disparado por tags `v*` y por `workflow_dispatch` con input `tag`; `concurrency: release` sin cancelar. Job `build` (`contents: read`, `packages: write`):
  - valida el tag contra `^v[0-9]+\.[0-9]+\.[0-9]+$` **antes** de construir, **en los dos disparadores**: `github.ref_name` en un tag e `inputs.tag` en `workflow_dispatch`, ambos leídos por `env:` y nunca interpolados con `${{ }}` dentro de `run:` (un nombre como `v1$(…)` es válido en git); si no cumple, falla sin publicar nada;
  - normaliza el repo a minúsculas (`${GITHUB_REPOSITORY,,}`);
  - en un tag: construye y publica `server` (con `APP_VERSION` = el tag) y `web`, cada una con `vX.Y.Z` y `sha-<7>` sobre el mismo digest, `provenance: false`, `sbom: false` y **solo** `cache-from: type=gha,scope=<imagen>`, que lee la caché de `main`. Sin `cache-to`: la caché que escribe la corrida de un tag queda en el ámbito de ese ref, el siguiente tag no la ve, y con `mode=max` pesa cientos de MB que desalojan las de `main` de la cuota de 10 GB;
  - comprueba con `docker buildx imagetools inspect --raw` que cada tag es un manifiesto de imagen y no un índice; si no, falla antes de la retención;
  - retención con `actions/delete-package-versions` y exactamente `package-name: torneo-absurdo/{server,web}`, `package-type: container`, `min-versions-to-keep: 10`;
  - en un `workflow_dispatch` (rollback) no construye: solo comprueba que el tag existe en GHCR.
  → tag de prueba (criterio C5 del intent).
- **RF-C14.** Job `deploy` con `environment: production`, sin checkout y sin secretos. **`DEPLOY_MODE` a prueba de fallos**: si `vars.DEPLOY_MODE == 'api'`, el job **falla** con un mensaje que dice que el modo `api` no está implementado y que requiere un nuevo OK del owner; con cualquier otro valor, incluido no definirla, es el modo `manual` y el job no hace nada más que registrar el tag aprobado. En 001 no existe ningún código que llame a la API de Coolify. → `policy` (RF-C12) y el tag de prueba: `deploy` queda esperando la aprobación de `production` (criterio C6 del intent; ver "Preguntas abiertas").
- **RF-C15.** Job `verify`: consulta `https://${{ vars.PUBLIC_HOST }}/api/health` hasta 5 min y falla si `version` no es el tag. Si `vars.PUBLIC_HOST` no está definida, falla en el acto. → revisión del PR (se ejercita en el primer despliegue, fuera de 001).
- **RF-C16.** `.github/dependabot.yml` semanal para `github-actions`, `cargo`, `npm`, `docker` (los digests de las imágenes base de los dos Dockerfiles, con `directories: ["/apps/server", "/apps/web"]`) y `docker-compose` (el digest de Postgres en `infra/compose.dev.yml` e `infra/compose.local.yml`; el ecosistema `docker` no lee archivos de Compose). Todos con `cooldown` de 3 días, contra versiones maliciosas recién publicadas. Los PRs de Dependabot pasan por la misma revisión que cualquier otro y nunca se fusionan solos (RF-C12). Un bump de `rust:` en el Dockerfile que no toque `rust-toolchain.toml` falla en `check-compose.mjs` (RF-C6), y se resuelve subiendo los dos a la vez. → revisión.

**Hook, docs y cierre**
- **RF-C17.** `scripts/git-hooks/pre-push` (POSIX `sh`, ejecutable) corre `just check` y bloquea el push si falla. `just hooks` ejecuta `git config core.hooksPath scripts/git-hooks` (aplica a todos los worktrees del clon). Sin dependencias nuevas. → job `policy`: en un clon temporal, `just hooks` deja `core.hooksPath` en `scripts/git-hooks`, y el hook es ejecutable e invoca `just check` (criterio C8 del intent).
- **RF-C18.** El PR de la fase C reescribe la sección "Despliegue" de `docs/architecture.md` (Postgres standalone, compose con `web` y `server`, redes, release con gate manual, `DEPLOY_MODE`) e incluye:
  - La **preparación del VPS** como procedimiento con los comandos exactos de ADR-0006: creación de `torneo-edge` con `--ip-range 10.250.0.8/29`, la unión de `coolify-proxy` con `10.250.0.2`, el swap, los logs de acceso de Traefik y la limpieza de Docker, más las dos advertencias de devops (no borrar `torneo-edge`; "Reset configuration to default" del proxy quita la red y la IP).
  - **Prerrequisitos del primer despliegue a producción**, como lista que bloquea: (1) los **límites anónimos de ADR-0008 D6 y D7** en `/ws` implementados y probados: 50 sockets simultáneos por `ip_key` y upgrade a 30/min con ráfaga 10 por IP, con `TRUSTED_PROXIES=10.250.0.2/32` (ADR-0006 D2); (2) las verificaciones de ADR-0006 que no se hicieron en el cierre de 001 (RF-C19), más `docker inspect` de `ReadonlyRootfs`, `CapDrop`, `SecurityOpt` y `LogConfig` de `web` y `server`, para confirmar que Coolify respeta el endurecimiento y los logs al reescribir el compose, y `curl -I` a `https://${PUBLIC_HOST}/es/`, `/_app/version.json` y `/healthz/` (la 308): las tres respuestas traen `X-Content-Type-Options: nosniff`, `Referrer-Policy: strict-origin-when-cross-origin` y `Strict-Transport-Security: max-age=31536000` sin `includeSubDomains` ni `preload`, cada una una sola vez (RF-C4, enmienda 2026-10-23); (3) si la configuración de GitHub no se hizo en 001, esa configuración.
  - En "Comandos" de `CLAUDE.md`: los comandos nuevos y, sobre el `pre-push`, que revisa el **árbol de trabajo** y no el commit que se empuja, que necesita Docker encendido desde la fase B, y que `git push --no-verify` es la salida excepcional (un worktree de una rama sin `scripts/git-hooks/` empuja sin chequeo; es aceptable).
  → revisión del PR (criterio C9 del intent; security-reviewer revisa los prerrequisitos).
- **RF-C19.** **Checklist de cierre de la fase C** (posterior al merge del último PR de C, lo ejecuta devops-engineer con el OK del owner; la evidencia va en el issue TA-1). Tiene una parte fija y una variante por cada pregunta del owner (ver "Preguntas abiertas"). Cada variante dice qué se verifica en 001 y qué pasa al issue del primer despliegue, para que ninguna deje un criterio del intent sin dueño.
  - **Siempre**:
    - Crear en Linear el issue **"Primer despliegue a producción"**, bloqueado por la feature de auth (límites anónimos de D6 y D7) y con los prerrequisitos de RF-C18 como checklist, incluidas las verificaciones que esta fase no cubre.
  - **Configuración de GitHub, variante "sí"** (es criterio de cierre; la recomendada):
    1. Antes de cualquier tag: environment `production` (owner como required reviewer; *deployment branches and tags* limitados a `v*` y `main`), "Require approval for all external contributors", ruleset de `main` (PR obligatorio, 0 aprobaciones, `ci-ok` requerido, sin force-push ni borrado), ruleset de tags `v*` (solo el owner los crea), y las alertas y actualizaciones de seguridad de Dependabot activadas (reemplazan a `cargo deny check advisories`, "Áreas de preocupación" 11).
    2. Tag de prueba `v0.0.1` → `build` publica y verifica las dos imágenes (RF-C13, criterio C5); `deploy` queda esperando la aprobación de `production` y el owner la **rechaza** (no hay nada que desplegar; criterio C6). Después del primer push, los paquetes de GHCR quedan públicos.
    3. Un `workflow_dispatch` de `release.yml` lanzado desde una rama distinta de `main` no puede usar `production` (primera mitad de la verificación 17 de ADR-0006).
    - Si la ejecuta un agente con `gh`: los pasos 1 y 3 se hacen y se comprueban con la API (`gh api` sobre environments, rulesets y la configuración del repo, con la salida en el issue). Hasta donde sé, la API de GitHub no permite cambiar la visibilidad de un paquete de GHCR, así que ese paso lo hace el owner en la UI. El token que use el agente es del owner y no se registra en ningún archivo ni log.
  - **Configuración de GitHub, variante "no"**:
    - **No se empuja ningún tag `v*`** hasta que exista `production` con required reviewer ("Áreas de preocupación" 8).
    - C6 se verifica de forma estática (RF-C12: `deploy` declara `environment: production` y ningún workflow contiene llamadas a Coolify).
    - C5 y la observación del gate de C6 pasan al issue del primer despliegue, junto con la configuración. Como eso cambia criterios de un intent aprobado, **el product-owner actualiza el intent y el architect lo vuelve a aprobar antes de cerrar la fase C** (nota 2 del product-owner).
  - **Preparación del VPS, variante "parte de 001"**: los pasos de RF-C18 aplicados y **solo la parte de host** de las verificaciones de ADR-0006, que es la que existe sin `web` ni `server` desplegados:
    - 13, primera mitad: el proxy corre sin `--accesslog` y con Traefik ≥ 3.1.
    - 15b, parcial: `docker network inspect torneo-edge` muestra el `--ip-range 10.250.0.8/29` y a `coolify-proxy` con `10.250.0.2` como único miembro, también después de reiniciar el proxy.
    - 4, parcial: `swapon --show` muestra el swap de 2 GB y `vm.swappiness` vale 10.
    - D10: la limpieza de Docker activada con "Delete Unused Volumes" apagado.
    - El resto de 13 (etiquetas de los routers y `/api/health` fuera del access log), 15a, 15c y la parte de contenedores de 4 pasan al issue del primer despliegue.
    - Si la ejecuta un agente por SSH: solo hace los pasos de RF-C18 y estas comprobaciones, pega las salidas en el issue y no copia al repo ni al issue direcciones, claves ni otros datos del VPS que no hagan falta para la evidencia.
  - **Preparación del VPS, variante "aparte"**: se crea en Linear la tarea de preparación del VPS para devops-engineer, que bloquea el issue del primer despliegue, y el owner deja constancia de que cubre la frase "durante la feature 001" de ADR-0006 ("Áreas de preocupación" 9).

## Requisitos no funcionales
- **Memoria (VPS de 1 vCPU / 4 GB, ADR-0006 D4).** En `just prod-local`, en reposo, el server usa < 64 MB y la web < 200 MB (`docker stats --no-stream`, evidencia en el PR de C). Peor caso de nuestros límites en el VPS: 1,152 MB con Postgres.
- **Velocidad de CI (ADR-0006 D7).** PR solo de `docs/**`: `ci-ok` verde en ≤ 2 min. PR con Rust y caché caliente: ≤ 8 min (p50). `images` con caché caliente: ≤ 8 min. Release completo: ≤ 20 min sin contar la aprobación.
- **Costo.** Cero: Actions sin límite en repo público, GHCR público, Dependabot gratis. No se agrega ningún servicio externo; la descarga de `zig` en el build es un artefacto fijado por versión y checksum, no un servicio.
- **Seguridad.**
  - Ningún secreto en el repo, en las imágenes (RF-C3) ni accesible a un `pull_request` (RF-C12).
  - Contenedores `nonroot`, sistema de archivos de solo lectura, sin capabilities (RF-C4).
  - En local, todo puerto publicado se ata a `127.0.0.1`.
  - Valores de cookie validados antes de usarse (RF-A4, RF-A6).
  - Los logs no contienen datos del cliente (RF-A19, RF-B15).
  - Las rutas de `/dev/**` no ejecutan nada sin `ENABLE_DEV_CATALOG` (RF-A16), y el server no acepta métodos de escritura hasta auth (RF-B12).
  - `web` rechaza todo método no seguro cuyo `Origin` no sea `PUBLIC_ORIGIN` (RF-A23). Detrás de Traefik, su origen no depende de headers que el cliente controle (RF-C4).
  - **URLs absolutas solo desde `PUBLIC_ORIGIN`** (enmienda 2026-10-23). `web` nunca construye URLs absolutas con `event.url.origin`, `url.host` ni `url.href` (ni con `request.url` o `new URL(x, base)` con una base de la petición). Usa `PUBLIC_ORIGIN`. El motivo es que quien llega directo al contenedor elige el `Host`, y `event.url` lo sigue: en `prod-local` (`127.0.0.1:3000`) y desde cualquier contenedor de `torneo-edge`. → ESLint en `just check` (`no-restricted-syntax` en `apps/web/eslint.config.js`, `src/**` salvo `*.test.ts`; decisión 20 de `plan-a.md`).
  - Las cabeceras de seguridad de RF-A10 (`nosniff`, `Referrer-Policy` y HSTS) llegan a **todas** las respuestas de `web` en producción, incluidas las que no pasan por `handle`. Las pone Traefik (RF-C4).
  - Ningún socket de `/ws` vive sin ping/pong ni más de 120 s sin mensajes aceptados (RF-B13); el primer despliegue espera a los límites anónimos de ADR-0008 D6 y D7 (RF-C18).
- **i18n.** Cero strings literales visibles fuera de `src/routes/dev/**` (RF-A15). El protocolo y el API solo emiten códigos (RF-B4, RF-B5).
- **Accesibilidad.** WCAG 2.2 AA; objetivos táctiles ≥ 44×44 px; anillo de foco visible en todo interactivo; reduced motion según ADR-0009 D6.
- **Mobile-first.** Diseño a 375 px; ninguna página de 001 tiene scroll horizontal a 375 px (se comprueba junto con el test 8).
- **Navegadores.** Las dos últimas versiones de Safari iOS, Chrome Android y los navegadores de escritorio actuales.
- **Plataformas de desarrollo.** macOS (Intel y Apple Silicon) y Linux. Windows no se soporta.

## Diseño

### Componentes afectados
```
/
├─ package.json · pnpm-workspace.yaml · .npmrc · .node-version        (A)
├─ Cargo.toml · Cargo.lock · rust-toolchain.toml · deny.toml          (B)
├─ justfile                                                           (A, B, C)
├─ .dockerignore                                                      (C)
├─ apps/web/                                                          (A, B)
│  ├─ package.json (`imports` con `#lib`) · vite.config.ts (config de Kit 3) · tsconfig.json
│  ├─ messages/{es,en}.json · project.inlang/
│  ├─ src/app.html (script del tema con nonce) · app.css
│  ├─ src/env.ts (`PUBLIC_ORIGIN`, `ENABLE_DEV_CATALOG`)
│  ├─ src/hooks.server.ts (guard de Origin, Paraglide, tema, cabeceras, handleError, init)
│  ├─ src/hooks.ts (reroute de Paraglide)
│  ├─ src/lib/theme/{tokens.css, theme-color.ts, theme.svelte.ts}
│  ├─ src/lib/components/ui/** · ui/_a11y-messages.ts
│  ├─ src/lib/features/system/{health.ts, echo.ts}                    (B)
│  ├─ src/lib/i18n/format.ts · src/lib/server/log.ts
│  ├─ src/routes/+layout.svelte · +page.svelte · +error.svelte
│  ├─ src/routes/healthz/+server.ts
│  ├─ src/routes/dev/{+layout.server.ts, components/, error-probe/, status/ (B)}
│  ├─ Dockerfile                                                      (C)
│  └─ .env.example
├─ apps/server/  (bin `server`: serve | healthcheck)                  (B)
│  ├─ src/{main.rs, config.rs, db.rs, http/, ws/, telemetry.rs, shutdown.rs}
│  ├─ migrations/…_baseline.sql · tests/ · .env.example
│  └─ Dockerfile                                                      (C)
├─ crates/protocol/  (serde; ts-rs con feature `ts`)                  (B)
├─ packages/protocol-ts/  (@torneo/protocol, generado y commiteado)   (B)
├─ scripts/
│  ├─ check-tokens.mjs · check-i18n.mjs (+ fixtures)                  (A)
│  ├─ check-compose.mjs · check-workflows.mjs (+ fixtures)            (C)
│  ├─ check-dockerignore.sh · fixtures/context.Dockerfile             (C)
│  ├─ local-networks.sh · git-hooks/pre-push                          (C)
├─ infra/
│  ├─ compose.dev.yml                                                 (B)
│  └─ docker-compose.yml · compose.local.yml · local.env              (C)
├─ .github/workflows/{ci.yml, release.yml} · .github/dependabot.yml   (C)
└─ docs/architecture.md · CLAUDE.md · docs/legal/aviso-de-privacidad.md (A, B, C)
```
`crates/engine`, `crates/engine-wasm`, `packages/renderer` y `content/` no se tocan.

### Variables de entorno

| Variable | Servicio | Obligatoria | Valor / por defecto | Dónde se fija |
|---|---|---|---|---|
| `DATABASE_URL` | server | sí (`:?` en el compose) | — | Coolify; dev `apps/server/.env`; `prod-local` `infra/local.env` |
| `PUBLIC_ORIGIN` | server | sí | `https://${PUBLIC_HOST}` | compose; dev `http://localhost:5173` |
| `PORT` | server | no | `8080` | — |
| `BIND_ADDR` | server | no | `0.0.0.0` | dev `127.0.0.1` en `apps/server/.env.example`; **nunca** en el compose de producción (`check-compose.mjs` la prohíbe en `server`, fase C). Se parsea como `IpAddr`; un valor inválido es `config.invalid` (RF-B8). `server healthcheck` llama a `127.0.0.1`, así que solo funciona con `0.0.0.0` o `127.0.0.1` (enmienda 2026-10-23 b). No se llama `HOST` porque zsh y algunos CI ya la definen, y para no confundirla con el `HOST` de `web` |
| `RUST_LOG` | server | no | `info` (nunca más verboso en prod, RF-C6) | compose |
| `WS_MAX_CONNECTIONS` | server | no | `2000` | — |
| `ENABLE_WS_ECHO` | server | no | ausente = eco apagado; solo `1` lo enciende | solo `.env` de dev y tests; nunca en el compose |
| `APP_VERSION` | server (build) | no | `dev` | `--build-arg` en Actions |
| `ENABLE_DEV_CATALOG` | web | no | ausente = `/dev/**` da 404 | solo `.env` de dev y job `e2e` |
| `PUBLIC_ORIGIN` | web | sí (en runtime; no arranca sin ella) | `https://${PUBLIC_HOST}`, igual que el server | compose; dev `http://localhost:5173`; e2e `http://localhost:<puerto>` (RF-A23) |
| `ADDRESS_HEADER`, `XFF_DEPTH`, `HOST`, `PORT`, `NODE_OPTIONS` | web | — | ver RF-C4 | compose |
| `ORIGIN`, `PROTOCOL_HEADER`, `HOST_HEADER`, `PORT_HEADER` | web | **prohibidas** | — | nunca: `ORIGIN` ya no existe en adapter-node 6 y las otras tres harían que el origen dependa de headers (RF-C4, RF-C6) |
| `IMAGE_TAG`, `PUBLIC_HOST` | compose | sí (`:?`) | — | Coolify; en `prod-local`, `IMAGE_TAG` la exporta la receta y `PUBLIC_HOST` viene de `infra/local.env` por `--env-file` (RF-C7) |

`INTERNAL_API_URL`, `TRUSTED_PROXIES`, `IP_HASH_KEY` y el secreto interno llegan con la feature de auth: en 001 el SSR no llama al API. Los plazos del WS (ping, pong e inactividad, RF-B13) no son variables de entorno.

### Modelo de datos / tipos (protocol)
- Postgres: solo la migración baseline (sin tablas de dominio). Las tablas de `docs/architecture.md` ("Datos") llegan con sus features.
- Tipos y convenciones: RF-B4 y RF-B5. Ejemplo de la forma generada (ilustrativo):
  ```ts
  export type HealthResponse = { status: "ok"; version: string } | { status: "db_unavailable" };
  export type ClientMessage = { type: "echo"; payload: string };
  ```
- **Elección de generador: `ts-rs`** (razones en "Respuestas" y "Alternativas").

### API / mensajes WS

| Ruta | Método | Respuesta | Notas |
|---|---|---|---|
| `/api/health` | GET | 200 `HealthResponse::Ok` · 503 `DbUnavailable` | `Cache-Control: no-store`; sin sesión ni rate limit; resultado de la BD cacheado 1 s |
| `/ws` | GET (upgrade) | 101 · 403 `ws.origin_forbidden` · 4xx sin cuerpo (no es upgrade) · 503 `ws.capacity` | chequeos en ese orden (RF-B13); ver abajo |
| cualquier otra | GET, HEAD, OPTIONS | 404 `http.not_found` | sin CORS |
| cualquiera | otro método | 405 `http.method_not_allowed` | solo lectura hasta auth (RF-B12) |
| (web) `/healthz` | GET | 200 | fuera de i18n; sin logs |

**WebSocket y ADR-0008 D6.** `/ws` existe en todas las builds y ya aplica todo lo de D6 que no depende de la sesión ni de la IP del cliente: `Origin` exacto en el handshake, 4 KB por mensaje fijados en el protocolo, 10 mensajes por 10 s y el tope global. Agrega lo que D6 no fija y no depende de auth: ping cada 30 s con cierre si no hay respuesta en 60 s, e inactividad de 120 s, para que un socket mudo no ocupe el tope para siempre. El eco **no** es un canal de texto libre: es una variante tipada de `ClientMessage`, apagada por defecto y encendida solo con `ENABLE_WS_ECHO=1`, que existe en el `.env` de desarrollo y en los tests, y que `check-compose.mjs` prohíbe en el compose de producción. Con la variable ausente, el único mensaje que existe se rechaza, así que en producción el cliente no puede mandar nada que el server acepte, lo cual es más estricto que D6 ("solo suscripción/desuscripción"). Cuando llegue el WS real, la feature correspondiente agrega `Subscribe`/`Unsubscribe` y puede borrar `Echo`. Los límites por IP y por usuario (D6 y D7) llegan con auth, porque necesitan `TRUSTED_PROXIES` y la sesión, y **el primer despliegue a producción espera a los anónimos** (RF-C18).

### Logs y plazo (aviso de privacidad, "Pendiente" punto 8)
- **Qué registra `tower-http::trace` sobre el usuario: nada que lo identifique.** Por petición: `ts`, `level`, `method`, `route` (plantilla), `status`, `latency_ms` y `request_id` aleatorio (RF-B15). No hay IP, `ip_hash`, user agent, `user_id`, sesión, headers, URI ni query string. Esto es más estricto que ADR-0008 D3, que solo pedía redactar headers y quitar el query de los callbacks: al usar `MatchedPath`, ni siquiera los identificadores de la ruta (p. ej. `fight_id`) llegan al log.
- **Contrato a futuro.** Una feature que quiera agregar un campo a la línea de petición (por ejemplo `user_id`) tiene que actualizar el aviso de privacidad en el mismo PR y pasar por security-reviewer. Propongo elevar este contrato a ADR (ver "Áreas de preocupación").
- **`web`** no registra peticiones; solo arranque y errores saneados (RF-A19). Como escribe poco, sus logs pueden durar más de 3 semanas; no es un problema mientras no contengan datos del cliente, que es lo que garantizan RF-A19 y su e2e. El aviso lo aclara (RF-B20). El saneador de `error_message` funciona por lista negra y basta para 001; para ADR-0012 propongo registrar el mensaje solo de errores propios con código y, para el resto, `error_name` más el stack.
- **Plazo típico (`{{PLAZO_LOGS}}`)**: los logs de Docker rotan por tamaño, 3 × 10 MB por contenedor con el driver `json-file` fijado en el compose (ADR-0006 D1, RF-C4), y se borran al recrear el contenedor en cada despliegue. Como el healthcheck de Docker pide `/api/health` cada 15 s (5,760 líneas al día) y cada línea ocupa ~200 B más ~80 B del envoltorio de `json-file`, el server escribe al menos ~1.6 MB/día aunque no haya tráfico: 30 MB duran **como máximo ~19 días**, y menos con tráfico real o con despliegues. Valor para el aviso: **"menos de 3 semanas"**. La medición va en dos pasos, cada uno en la fase donde existe lo que mide:
  - **Fase B** (cálculo): con `cargo run`, mide el tamaño real de la línea de `/api/health` (media de 100 líneas), le suma el envoltorio de `json-file` (~80 B), calcula el máximo de días y escribe `{{PLAZO_LOGS}}` con ese resultado (RF-B20).
  - **Fase C** (confirmación): con `just prod-local` en reposo durante 1 h, mide el crecimiento real del archivo de log del contenedor `server` (`docker inspect --format '{{.LogPath}}'` y su tamaño, o `docker logs server | wc -c` más el envoltorio si el archivo no es accesible, como en Docker Desktop), extrapola a un día y, si el máximo pasa de 21 días, ajusta el aviso en el mismo PR. La evidencia va en el PR.

### Cookies

| Cookie | Valores | Atributos | Duración | Quién la escribe |
|---|---|---|---|---|
| `locale` | `es`, `en` | `Path=/; SameSite=Lax`; `Secure` en https; sin `HttpOnly` | 1 año (`Max-Age=31536000`) | el selector de idioma, solo por elección explícita |
| `theme` | `dark`, `light`, `system` | igual | 1 año (ADR-0009 D3) | `theme.svelte.ts` |

Elegí `locale` y no el `PARAGLIDE_LOCALE` por defecto de Paraglide, que expone la librería en el aviso, y 1 año y no los 400 días por defecto, para igualar a `theme`. Las dos son de preferencia; si cuentan como "necesarias" lo confirma la revisión legal (aviso, "Pendiente" punto 5).

### Flujos

**Desarrollo (`just dev`)**
```
just dev ─▶ docker compose -f infra/compose.dev.yml up -d --wait   (postgres en 127.0.0.1:5432)
        ├─▶ cargo run -p server   → espera BD → migra → :8080
        └─▶ pnpm --filter web dev → :5173 ── proxy /api, /ws ──▶ :8080
navegador ─▶ http://localhost:5173/  ─307─▶ /es/ | /en/
```

**Imágenes de producción en local (`just prod-local`)**
```
local-networks.sh (torneo-edge 10.250.0.0/28, ip-range .8/29; torneo-db)
buildx --platform linux/amd64 (builder en $BUILDPLATFORM + zigbuild) ─▶ :local
compose -f docker-compose.yml -f compose.local.yml up --wait
   postgres(torneo-db) ◀── server(torneo-edge+torneo-db, 127.0.0.1:8080) ◀── web(torneo-edge, 127.0.0.1:3000)
```
En Mac ARM, el build no emula (compila en cruzado) y la ejecución de las imágenes amd64 usa Rosetta de Docker Desktop. Sin Traefik local: `/api` y `/ws` desde el navegador no pasan por `web`; la prueba local es de salud de los contenedores, y las etiquetas se verifican con `check-compose.mjs`.

**CI y release**
```
PR ─▶ changes ─┬─ rust (si rust)    ─┐
               ├─ web, e2e (si web)  ├─▶ ci-ok (único requerido)
               ├─ images (si images) │
policy (siempre) ────────────────────┘
tag vX.Y.Z ─▶ build (valida tag → push → imagetools → retención) ─▶ deploy [environment production, espera al owner]
            ─▶ verify (/api/health == tag)
```

### UI
- **Cabecera de la app** (todas las páginas): marca (mensaje), selector de idioma y selector de tema (`dropdown-menu` con `dark`, `light` y `system`; el elemento activo marcado con `aria-checked`). A 375 px los dos selectores caben en una fila con objetivos de 44 px; en escritorio, la misma fila alineada al final.
- **Inicio** (`/es/`, `/en/`): contenido provisional en mensajes. Sin estados de carga (no hay datos).
- **Error** (`+error.svelte`): título y texto por estado (404, 500), elegidos por `page.status` y nunca por `message`, y el `error_id` para soporte cuando existe (solo en los errores que se registran, RF-A19). Mismo layout.
- **`/dev/components`**: según ADR-0009 D5.
- **`/dev/status`** (B): estados `cargando` (skeleton), `ok` (badge con la versión), `db_unavailable` (badge destructivo), `error de red`, y el eco: `conectado`, `enviado`, `recibido` y `cerrado` con el código de razón traducido. Todos los textos son mensajes de Paraglide.

## Áreas de preocupación
1. **Eco y `/ws` frente a ADR-0008 D6 (seguridad).** El eco es tipado, está apagado por defecto, está prohibido en el compose de producción y el server lo rechaza con un `PUBLIC_ORIGIN` `https://` (RF-B13, RF-C6). `/ws` existe en las imágenes de producción sin límites por IP ni por usuario hasta la feature de auth: un solo host con un cliente que no es navegador puede abrir sockets hasta `WS_MAX_CONNECTIONS` y dejar fuera a todos. En 001 lo acoto con ping/pong e inactividad (RF-B13), que no dependen de auth y hacen que un socket mudo muera en ≤ 120 s, pero eso no basta sin un límite por IP, porque el atacante reconecta. Por eso, a pedido de security-reviewer, **el primer despliegue a producción queda bloqueado hasta que existan los límites anónimos de D6 y D7** (50 sockets por `ip_key`, upgrade a 30/min con ráfaga 10, con `TRUSTED_PROXIES`). Es una condición escrita en RF-C18 (prerrequisitos en `docs/architecture.md`) y en RF-C19 (issue del primer despliegue, bloqueado por auth). La reserva del 20 % de D6 llega con auth, porque distingue cuentas fuera de carencia.
2. **Sin middleware de `Origin`/CSRF (ADR-0008 D4) en 001.** El intent lo deja fuera. Hoy no hay ninguna ruta de mutación, así que no hay hueco, y **ningún PR puede agregar una ruta que no sea `GET`/`HEAD` antes de que exista ese middleware**. Ya no depende solo de la revisión: en el server, una capa responde 405 a todo método distinto de `GET`, `HEAD` y `OPTIONS` (RF-B12), y la feature de auth la reemplaza por el middleware de D4; en `web`, el chequeo CSRF de formularios de SvelteKit 3 está siempre activo (RF-A10) y, desde la enmienda 2026-10-22, un guard propio en `handle` aplica la regla de `Origin` de ADR-0008 D4 a todo método no seguro (RF-A23).
3. **Contrato de logs más estricto que ADR-0008 D3 y transversal.** No registrar URI ni identificadores (RF-B15) es una decisión que obliga a todas las features futuras. Propongo un **ADR-0012 ("Logs, cabeceras de seguridad y datos del cliente")** que también recoja la CSP (punto 4), HSTS y el saneador de `web` ("Diseño → Logs"). **Al aprobarse esta spec, el architect abre su issue en Linear** (nota 1 del product-owner), para que no quede solo como intención; tiene que estar aprobado antes de que 002 amplíe la CSP. Mientras tanto, la fuente es esta spec.
   - **Resuelta por [ADR-0012](../../decisions/0012-logs-y-cabeceras-de-seguridad.md)** (aprobado el 2026-10-26, parte A). El contrato de logs ya no vive aquí: manda el ADR. Una diferencia con RF-A19: `error_message` salió de la allowlist de `web` y entró `error_code` (A5, TA-14).
4. **CSP (RF-A10) es nueva y afecta a otras features.** El renderer (002, PixiJS) y `engine-wasm` necesitarán ampliarla (por ejemplo `'wasm-unsafe-eval'` en `script-src`, y quizá `worker-src` o `blob:`); `style-src 'unsafe-inline'` lo impone Bits UI. `connect-src 'self'` cubre `wss:` del mismo origen en los navegadores actuales; el e2e en WebKit lo comprueba con `/dev/status` en local (en CI no hay server). Si WebKit falla, se agrega el origen `wss://` explícito por variable en la feature del WS.
   - **Resuelta por [ADR-0012](../../decisions/0012-logs-y-cabeceras-de-seguridad.md)** (aprobado el 2026-10-26, parte B). La CSP base, lo prohibido y las ampliaciones permitidas a 002, `engine-wasm` y el WebSocket (con la prueba `wss://` en WebKit) los fija el ADR.
5. **Extensiones a los ADRs, sin contradecirlos** (para devops-engineer y frontend-engineer):
   - Filtros de D7 ampliados (RF-C8): `rust` + `packages/protocol-ts/**` y `deny.toml`; `web` + `scripts/**`, `.npmrc` y la configuración de formato y lint de la raíz; `images` + los scripts de compose y de contexto, `Cargo.lock`, `rust-toolchain.toml` y `pnpm-lock.yaml`. Un job `policy` siempre activo dentro de `ci-ok` (RF-C12), y `ci-ok` con `timeout-minutes: 5`.
   - Postgres de la CI con `just db-up` y no con `services:` (RF-C9): un solo camino en local y en CI.
   - `release.yml` solo lee la caché de `main` (`cache-from`), sin `cache-to` (RF-C13).
   - El binario estático se comprueba con `readelf -l` sin `INTERP` en vez de la salida exacta de `ldd` que cita la verificación 8 de ADR-0006 (RF-C1).
   - `check-workflows.mjs` más estricto que la verificación 9 de ADR-0006: cualquier referencia a `secrets` en cualquier workflow, permisos de raíz `contents: read`, `persist-credentials: false` y nada de `${{ }}` con datos controlables en `run:` (RF-C12).
   - Regla `ring-ring/<n>` en `check-tokens.mjs` (RF-A13) y overlay `bg-background/80` en vez de un token nuevo (RF-A12).
   - Endurecimiento del compose: `read_only`, `tmpfs` de 16 MB, `cap_drop`, `no-new-privileges`, `platform` y `logging` explícito (RF-C4).
   - **Guard de `/dev/**` en `handle`** (RF-A16), además del `+layout.server.ts` que sugiere ADR-0009 D5: el layout solo no impide que corran los `load` de página, los `+server.ts` ni las actions.
   - HSTS en `web` y en `server` (RF-A10, RF-B12) y 405 a todo método que no sea de lectura en el server (RF-B12), hasta que auth traiga el middleware de ADR-0008 D4.
   - Ping/pong e inactividad en `/ws` (RF-B13), que ADR-0008 D6 no fija.
   - `deploy` con `DEPLOY_MODE=api` **falla** en vez de implementar la llamada a Coolify (RF-C14): el owner eligió `manual` y pasar a `api` requiere su OK, así que no tiene sentido mantener sin probar código que maneja un token de equipo.
   - Playwright contra `node build` y no `vite preview` (ADR-0009 D5 lo daba como ejemplo; el objetivo, que `dev` sea `false`, se mantiene, y además se prueba `adapter-node` con `ADDRESS_HEADER`, que es lo que necesita el test de logs).
   - Las capturas del test 8 son artefactos para revisión, no comparación de píxeles: con la pila de fuentes del sistema, las capturas difieren entre macOS y Linux. La regresión visual se evalúa cuando haya fuentes propias.
6. **Imágenes amd64 en Mac ARM (riesgo técnico).** La compilación cruzada con `cargo-zigbuild` depende de que `ring` y `mimalloc` compilen con `zig cc` para musl, lo cual es habitual pero hay que probarlo en el PR de C. Si falla, el respaldo es el plan B de ADR-0006 D8 para el build, y en Mac ARM se usa `just prod-local <tag>` con imágenes de GHCR. Ejecutar amd64 en ARM requiere Rosetta en Docker Desktop; en Linux ARM, `binfmt`/QEMU (no soportado oficialmente).
7. **`just check` necesita Docker desde la fase B** (Postgres para los tests de integración). Lo acepto: Docker ya es requisito de `just dev`, y la CI usa el mismo `just db-up` (RF-C9). El `pre-push` no corre e2e, para que tarde poco; la CI es la red completa.
8. **Orden del tag de prueba y del environment (seguridad).** GitHub crea el environment `production` **sin protección** si un workflow lo referencia y no existe. En 001 no pasa nada (el modo `manual` no despliega), pero el tag de prueba debe ir **después** de crear `production` con required reviewer, o el criterio C6 no se puede observar. Por eso, en RF-C19, la configuración de GitHub va antes del tag.
9. **"Durante la feature 001" en ADR-0006.** Las decisiones del owner en ADR-0006 dicen que devops-engineer aplica los cambios del proxy compartido "durante la feature 001". Si el owner saca la preparación del VPS de 001 (recomendación del intent), esa frase queda desfasada y el owner debe dejar constancia. La spec funciona en los dos casos: el procedimiento queda escrito en `docs/architecture.md` (RF-C18) y su ejecución es un punto opcional del cierre (RF-C19).
10. **Plazo de logs ligado al healthcheck.** "Menos de 3 semanas" depende de que el healthcheck siga registrándose (RF-B16) y del tamaño de línea. Si alguien baja `/api/health` a `debug` o cambia el intervalo, el plazo crece y el aviso deja de ser cierto. Lo anoto en el comentario de la configuración de logs. También depende del driver `json-file`, que por eso queda fijado en el compose (RF-C4); si Coolify lo reescribe, lo detecta el `docker inspect` de `LogConfig` del primer despliegue (RF-C18).
11. **Cadena de suministro.** El código de shadcn se copia; las actions van por SHA (RF-C12); `zig` se descarga con un checksum por arquitectura, y `cargo-chef` y `cargo-zigbuild` van con `--locked --version` (RF-C1); todas las etapas `FROM` van por digest (RF-C1, RF-C2); `packageManager` lleva hash, pnpm no corre scripts de instalación salvo los permitidos y no instala versiones de menos de 24 h (RF-A1); Dependabot espera 3 días (`cooldown`) y nunca se fusiona solo (RF-C12, RF-C16). No agrego `cargo deny check advisories` a la CI, porque haría fallar PRs ajenos a la advertencia; lo cubren las alertas de Dependabot, que se activan en la configuración de GitHub (RF-C19) o, si esa configuración sale de 001, antes del primer despliegue (RF-C18).
12. **SvelteKit 3 (enmienda 2026-10-22).** Kit 3.0 salió el 2026-10-01 y adapter-node 6 el mismo día; el ecosistema todavía se está poniendo al día. Lo que queda abierto:
    - **Formularios del mismo origen sobre `http`.** Sin `paths.origin`, adapter-node supone `https`, así que en `node build` local y en el e2e (que sirven por `http`) el chequeo de Kit rechaza con 403 un formulario del mismo origen. En 001 no hay form actions y `vite dev` no aplica el chequeo, así que no afecta. La primera feature que agregue una action decide cómo probarla: por ejemplo, `PROTOCOL_HEADER=x-forwarded-proto` **solo** en la instancia de e2e, con ese header puesto por Playwright, y nunca en el compose (RF-C6 lo prohíbe allí).
    - **Compatibilidad sin probar.** Lo verifiqué leyendo los paquetes, no ejecutándolos: `shadcn-svelte@1.7.0` detecta `#lib/*` en `imports` y lo usa como alias por defecto; la regla de ESLint de Paraglide 2.29.1 reconoce cualquier import que termine en `/paraglide/messages.js`; ninguno de los dos fija versión de Kit. El plan de la fase A empieza con una prueba corta (`init` de shadcn con `#lib`, un `add button`, `svelte-check` y `vite build`) y, si algo falla, el plan vuelve a revisión antes de seguir.
    - **Archivos `server` de Paraglide.** En Kit 3, todo archivo cuyo nombre tenga un segmento `server` es solo de servidor; Paraglide genera `src/lib/paraglide/server.js`, que solo debe importarse desde `hooks.server.ts`. Si un componente lo importa, el build falla, que es lo correcto.
    - **TypeScript 6.0.x fijo.** La 7.x ya existe y no la admiten ni Kit 3 (`^6`) ni `typescript-eslint` (`< 6.1`). Dependabot la va a proponer; ese PR se rechaza hasta que las dos la acepten.
    - **Extensión a ADR-0008 D4.** El guard de `Origin` de `web` (RF-A23) es nuevo. ADR-0008 queda enmendado para nombrarlo, en lugar de `csrf.checkOrigin`, que ya no existe.
13. **Cabeceras de seguridad en dos capas (enmienda 2026-10-23).** RF-A10 las pone en `handle` y RF-C4 en Traefik, porque `sirv` y la 308 de Kit responden antes de `handle`. Tiene dos riesgos. (a) Si los dos valores divergen, el navegador recibe dos `Strict-Transport-Security` distintos; por eso RF-C6 fija los valores del middleware y RF-C18 exige que cada cabecera salga una sola vez. (b) La única prueba en vivo es la del primer despliegue (RF-C18), porque `prod-local`, los e2e y el job `images` no tienen Traefik. Lo acepto: la configuración se verifica de forma estática en `just check` y en `images` (RF-C6), y el primer despliegue ya está bloqueado por otros prerrequisitos. Ningún HSTS lleva `includeSubDomains`, para no afectar a otros subdominios del owner.

## Respuestas a preguntas abiertas del intent
- **P: Generador de tipos (`ts-rs`, `specta` o `typeshare`).** → R: **`ts-rs`**, detrás de la feature `ts` (RF-B3, RF-B6). Funciona con derive sobre los tipos ya compilados, respeta los atributos de serde que necesitamos (enums internamente etiquetados para `HealthResponse`, `ClientMessage`, `ServerMessage` y luego `FightEvent`), su salida es determinista para el chequeo de frescura y no agrega runtime. `typeshare` lee el código fuente sin expandir macros y solo admite enums con datos etiquetados en forma adyacente (`tag` + `content`), lo que nos impondría la forma del protocolo. `specta` lleva tiempo en release candidate con API inestable y está orientado a Tauri/rspc.
- **P: Herramienta del hook `pre-push`.** → R: un **script POSIX** en `scripts/git-hooks/pre-push`, instalado con `git config core.hooksPath` desde `just hooks` (RF-C17). Es un solo hook que llama a `just check`; `lefthook` agregaría un binario y un archivo de configuración sin aportar nada, y `husky` ataría los hooks a `pnpm install`.
- **P: `Origin` en el WS de eco y compatibilidad con ADR-0008 D6.** → R: el eco **ya valida `Origin`** y aplica los límites por socket y el tope global; además es una variante tipada **apagada por defecto** (`ENABLE_WS_ECHO=1` solo en dev y tests, prohibida en el compose de producción). Ver RF-B13 y "Diseño → WebSocket". Los límites por IP y por usuario llegan con auth.
- **P: Forma local con Postgres.** → R: **las dos cosas, para dos usos.** `infra/compose.dev.yml` (solo Postgres) para `just dev` con recarga en caliente (RF-B17), y el **override** `infra/compose.local.yml`, que solo agrega Postgres, variables y puertos locales sobre el compose de producción sin modificarlo, para `just prod-local` (RF-C7). Postgres no entra al compose que lee Coolify (ADR-0006 D1).
- **P: Imágenes amd64 y redes externas en local, también en Mac ARM.** → R: las redes externas se **crean** en local con los mismos parámetros que en el VPS (incluido `--ip-range`), con un script idempotente, en vez de sobrescribir `external` (que depende de cómo Compose combina archivos). Las imágenes se compilan en cruzado desde `$BUILDPLATFORM` con `cargo-zigbuild` (server) o son JS puro (web), así que en una Mac ARM se construyen sin emular; para ejecutarlas se usa Rosetta de Docker Desktop. `platform: linux/amd64` queda explícito en el compose (RF-C1, RF-C2, RF-C4, RF-C7; riesgo en "Áreas de preocupación" 6).
- **P: Qué registra `tower-http::trace` sobre el usuario y plazo típico de los logs.** → R: **nada que lo identifique**: método, plantilla de ruta, estado, latencia y un `request_id` aleatorio, sin IP, headers, URI, query, user agent ni `user_id` (RF-B15). Plazo: rotación de 3 × 10 MB que, con el healthcheck cada 15 s, dura **menos de 3 semanas** (~19 días como máximo) y se reinicia en cada despliegue. Valor de `{{PLAZO_LOGS}}`: "menos de 3 semanas". Ver "Diseño → Logs y plazo".
- **P: Nombre y duración de la cookie de idioma (ADR-0007).** → R: **`locale`, 1 año** (`Max-Age=31536000`), `Path=/; SameSite=Lax`, `Secure` en https, sin `HttpOnly`, escrita solo por elección explícita (RF-A4). Valor de `{{DURACION_COOKIE_IDIOMA}}`: "1 año".
- **P: Configuración de GitHub como criterio de cierre.** → se arrastra a "Preguntas abiertas" (owner); la spec funciona con cualquier respuesta (RF-C19).
- **P: Preparación del VPS como tarea aparte.** → se arrastra a "Preguntas abiertas" (owner); la spec funciona con cualquier respuesta (RF-C18, RF-C19).

## Decisiones del owner (2026-10-21)
- **Pregunta 1 → "sí".** La configuración de GitHub es criterio de cierre de la fase C y la ejecuta un agente con `gh` según la variante "sí" de RF-C19. La visibilidad pública de los paquetes de GHCR la cambia el owner en la UI.
- **Pregunta 2 → "aparte".** La preparación del VPS es una tarea propia en Linear, que bloquea el issue del primer despliegue. La ejecuta el operador del VPS con un runbook privado (fuera de este repo) escrito por devops-engineer, que después verifica la evidencia. Ningún agente de este repo accede al VPS. Esta tarea cubre la frase "durante la feature 001" de ADR-0006.
- **Ambientes:** por ahora solo local (`just dev` y `just prod-local`) y producción. Sin staging hasta que haya usuarios reales.

## Preguntas abiertas
Las dos preguntas del owner quedaron resueltas arriba; se conservan para trazabilidad.

1. **¿La configuración de GitHub es criterio de cierre de la fase C?** (environment `production`, aprobación para forks, rulesets de `main` y de tags, alertas de Dependabot y paquetes de GHCR públicos).
   - *Recomendación del architect (igual que en el intent): sí*, como checklist posterior al merge (RF-C19), porque sin ruleset `ci-ok` no es requerido de verdad y sin `production` no se puede observar el gate de `deploy`. El owner se inclina por el "sí", con la configuración hecha por un agente con `gh`; RF-C19 dice qué parte puede hacer el agente y qué parte queda en la UI.
   - **Si sí:** C5 y C6 se verifican con el tag de prueba `v0.0.1`, que se empuja **después** de proteger `production` (RF-C19).
   - **Si no:** no se empuja ningún tag `v*` en 001. C6 se verifica de forma estática (RF-C12) y C5 y la observación del gate pasan al issue del primer despliegue. Descarto la otra salida que dejaba abierta la versión anterior (empujar el tag con un `production` sin protección), porque deja el criterio C6 sin observar y crea un environment desprotegido. Como C5 y C6 salen de 001, el intent se actualiza y se vuelve a aprobar antes de cerrar la fase C (nota 2 del product-owner).
2. **¿La preparación del VPS es una tarea aparte?**
   - *Recomendación del architect: sí*, una tarea en Linear para devops-engineer que bloquee el primer despliegue, porque no se verifica en local ni en la CI. El owner se inclina por dar acceso SSH a un agente para hacerla dentro de 001; RF-C19 acota qué hace ese agente y qué evidencia deja.
   - **En los dos casos**, el procedimiento queda escrito en "Despliegue" de `docs/architecture.md` (RF-C18) y el primer despliegue espera a los prerrequisitos de RF-C18.
   - **Si es parte de 001:** se agrega al checklist de cierre (RF-C19) con **solo la parte de host** de las verificaciones 4, 13 y 15 de ADR-0006. El resto necesita `web` y `server` desplegados, y el intent deja el aislamiento de red en "No entra", así que pasa al issue del primer despliegue.
   - **Si es aparte:** se crea la tarea y el owner deja constancia de que la frase "durante la feature 001" de ADR-0006 queda cubierta por ella ("Áreas de preocupación" 9).

## Estrategia de pruebas

**Trazabilidad intent → requisitos → prueba**

| Criterio del intent | RF | Prueba | Nivel |
|---|---|---|---|
| A1 `just check` limpio; falla sin una llave | A15, A21 | fixtures de `check-i18n.mjs`; CI | unit · CI |
| A2 idioma por cookie en `/es` y `/en` | A3–A5 | elegir `es`, ir a `/`, ver `/es/`; cookie con atributos; valor inválido ignorado | e2e |
| A3 tests 1–6, 8, 9 de ADR-0009 | A6–A16 | los tests del ADR, más foco, scroll horizontal y el guard de `/dev/**` sin logs | unit · e2e |
| A4 `/healthz` 200 | A18 | `GET /healthz` sin redirección | e2e |
| A5 logs de `web` sin datos del cliente | A19, A20 | canarios (abajo) | e2e |
| B1 un comando local | B17 | evidencia en el PR | manual |
| B2 tipo de protocol en la web | B5–B7 | frescura de `gen`; `svelte-check`; Vitest de `health.ts` | CI · unit |
| B3 health 200/503 y healthcheck 0/1 | B10, B11 | proxy TCP cortable; binario real | integración |
| B4 eco | B13 | ida y vuelta con `ENABLE_WS_ECHO=1`; límites, ping/pong e inactividad | integración |
| B5 sin CORS; logs sin headers | B12, B15 | preflight ajeno; canarios | integración |
| B6 `just check` con Rust | B19 | CI | CI |
| C1 filtros y `ci-ok` | C8 | tres PRs de prueba (solo docs; `crates/protocol`; un test roto a propósito) | CI |
| C2 `cargo deny` y estático | B2, C9 | job `rust` | CI |
| C3 `pull_request_target` y `secrets.*` | C12 | fixtures de `check-workflows.mjs` | unit · CI |
| C4 `images` sin push | C11 | job `images` | CI |
| C5 tag publica manifiesto de imagen | C13, C19 | tag `v0.0.1` + `imagetools inspect --raw` (variante "sí" de la pregunta 1; con "no", pasa al primer despliegue y se actualiza el intent) | release |
| C6 gate `manual` sin Coolify | C12, C14, C19 | `policy` siempre; tag de prueba esperando aprobación en la variante "sí" | release · CI |
| C7 compose sin `IMAGE_TAG` falla; local levanta | C3–C7, C11 | `compose config`; `check-dockerignore.sh`; smoke test de `web`; `just prod-local` en amd64 y Mac ARM | CI · manual |
| C8 `just hooks` | C17 | clon temporal en `policy` | CI |
| C9 docs | A22, B20, C18 | revisión | PR |

**Detalle por nivel**
- **Unit (Vitest):** resolución del tema y validación de cookies (incluido un valor con `"><script>`); validación de `locale`; el guard de `/dev/**` de `handle` (`route.id` `/dev`, `/dev/…`, `/developers` y `null`); el guard de `Origin` y el validador de `PUBLIC_ORIGIN` (RF-A23); `handleError` por `kind` (RF-A19); saneador de logs (IPs v4 y v6, query strings, valores de headers sensibles); mapeo de `HealthResponse` y de los códigos de WS a llaves; fixtures de `check-tokens.mjs` (test 1 de ADR-0009 + `ring-ring/50`), de ESLint (test 2 + imports relativos + `console.log`), de `check-i18n.mjs` (llave faltante, mensaje vacío, texto literal, `aria-label` literal; pasa en `src/routes/dev/**`), de `check-compose.mjs` (incluidos `.dockerignore` sin `**/`, `logging` ausente, `tmpfs` sin tamaño, `rust:` distinto de `rust-toolchain.toml`, y `compose.local.yml` con `postgres` aceptado) y de `check-workflows.mjs` (un caso que falla por regla, incluidas las cuatro sintaxis de `secrets`, y uno válido); contraste (test 3).
- **Unit (Rust):** serialización exacta de cada tipo de `protocol`; parseo de la configuración (`PUBLIC_ORIGIN` con ruta o barra final → inválido; `ENABLE_WS_ECHO=1` con `PUBLIC_ORIGIN` `https://` → eco apagado); formato de `APP_VERSION`.
- **Integración (Rust, con el Postgres de `just db-up` en local y en CI):**
  - `/api/health` 200 → 503 → 200 con un proxy TCP del test entre el server y Postgres; 50 peticiones concurrentes → una sola consulta;
  - `server healthcheck` con el binario real: 0, 1 (proxy cortado) y 1 (sin server);
  - espera de la BD que falla en el plazo y `main` → código 1; migración baseline aplicada antes de abrir el puerto;
  - `/ws`: `Origin` ausente o ajeno → 403; eco encendido → vuelve igual; eco apagado → `1008 ws.echo_disabled`; 4 KB + 1 byte → `1009`; 11 mensajes en 10 s → `1008 ws.rate_limited`; frame binario → `1003`; JSON inválido → `1008 ws.invalid_message`; con `WS_MAX_CONNECTIONS=2`, el tercer upgrade → 503 `ws.capacity`; con plazos acortados, un cliente que no lee el socket (no contesta pings) queda cerrado tras el plazo de pong, y uno que contesta pings sin mandar mensajes se cierra con `1008 ws.idle_timeout`;
  - `SIGTERM` con un socket abierto → `1012` y salida en ≤ 10 s;
  - sin CORS: `OPTIONS` con `Origin: https://evil.example` y `GET` → ninguna respuesta con `Access-Control-Allow-*`;
  - `POST /api/health` y `DELETE /cualquier` → 405 `http.method_not_allowed`; `nosniff` y HSTS en una 200 y en una 404;
  - **canarios de logs**: peticiones con `Cookie`, `Authorization`, `X-Internal-Secret`, `X-Client-Ip` y `X-Forwarded-For` de valores únicos, a una ruta existente, a una inexistente con query string y al handshake de `/ws` con un `Origin` ajeno y un `payload` de canario; con el subscriber capturado en el filtro de producción (`info`) y con `debug` global, ninguna línea contiene un canario, la query ni el `payload`; y un arranque con `DATABASE_URL` de contraseña canario e inalcanzable no la registra.
- **e2e (Playwright contra `node build`):**
  - Tests 4, 5, 6, 8 y 9 de ADR-0009. El 5 corre con la CSP activa, en Chromium y WebKit.
  - Idioma (A2): redirección de `/` según cookie y `Accept-Language`, atributos de la cookie y valor inválido ignorado; `//evil.example/` y `/\evil.example` redirigen a `/es/…` o `/en/…`, nunca a otro host.
  - `/healthz`.
  - Sin violaciones de CSP en `/es/`, `/en/` y el catálogo; cabeceras de RF-A10 presentes.
  - **Guard de `Origin`** (RF-A23): `POST` JSON con `Origin` ajeno → 403 `auth.csrf_origin`; `POST` sin `Origin` ni `Sec-Fetch-Site` → 403; `POST /healthz` JSON con `Origin` = `PUBLIC_ORIGIN` → 405; sin líneas en stdout ni stderr.
  - **`handleError`** (RF-A19): `GET /es/no-existe` → 404 sin líneas en stdout ni stderr.
  - **Guard de `/dev/**`** (instancia sin `ENABLE_DEV_CATALOG`): `/es/dev/components`, `/es/dev/status` y `/es/dev/error-probe` → 404, y stdout y stderr no reciben ninguna línea durante esas peticiones.
  - **Foco**: recorrer el catálogo con Tab y comprobar que cada elemento enfocado tiene `box-shadow` (anillo) distinto de `none`.
  - Sin scroll horizontal a 375 px en las páginas de 001.
  - **Canarios de logs de `web` (A5)**: con `ADDRESS_HEADER=X-Forwarded-For` y `XFF_DEPTH=1`, peticiones a `/es/` y a `/es/dev/error-probe` con `Cookie`, `Authorization`, `X-Internal-Secret`, `X-Client-Ip` y `X-Forwarded-For: 203.0.113.77` (la IP que devolvería `getClientAddress()`). Se captura stdout y stderr del proceso y ninguna línea contiene esos valores, la IP de canario del mensaje del error ni el query string. La línea de error sí trae `error_id` y `route_id`.
- **Docker (job `images`):** `readelf -l` sin `INTERP` en el binario del server; `/app/node_modules` ausente y smoke test de `web` (`/healthz` y `/en/` → 200 con `PUBLIC_ORIGIN`; sin ella, el contenedor sale con código distinto de 0); `check-dockerignore.sh` con `infra/.env` y `apps/server/.env` de prueba fuera del contexto.
- **CI y release:** los PRs de prueba de C1 se documentan en el PR de la fase C (enlaces a las corridas), y el tag de C5/C6 en el issue TA-1 si aplica la variante "sí" (RF-C19). Un PR de prueba que modifica a mano `packages/protocol-ts` falla en `rust` por frescura, y uno que toca `deny.toml` corre `rust`.
- **Manual con evidencia en el PR:** `just dev` (B1); `just prod-local` en Linux amd64 y en Mac ARM con `docker stats` (C7 y memoria); tamaño de la línea de `/api/health` (B) y crecimiento del log en 1 h de `prod-local` (C) para el plazo; duración del job `rust` con caché caliente (C).

## Alternativas consideradas
- **`typeshare`** y **`specta`** como generadores: ver "Respuestas". Escribir los tipos TS a mano: dos fuentes de verdad.
- **`lefthook` o `husky`** para el hook: ver "Respuestas".
- **Eco solo con una feature de Cargo** (`ws-echo`, fuera de la imagen de release): el binario de los tests y el de producción serían distintos y la unificación de features en el workspace puede encenderla sin querer. La variable en runtime se prueba en sus dos estados con el mismo binario.
- **Sin eco en 001**: el intent exige el criterio B4.
- **Eco de texto libre con `Origin` y límites**: sigue siendo "mensajes arbitrarios" en producción, lo que el intent prohíbe.
- **Postgres dentro del compose de producción con `profiles`**: el compose lo lee Coolify y Postgres es un recurso standalone (ADR-0006 D1); no quiero depender de cómo Coolify trata los perfiles.
- **Override que cambia `external: true` a `false`**: depende de las reglas de combinación de Compose; crear las redes es explícito e igual al VPS.
- **Imágenes multi-arquitectura (amd64 + arm64)**: duplica el tiempo de build y ADR-0006 D5 fija amd64 con un manifiesto por tag.
- **Imágenes arm64 nativas solo para local**: no serían "las imágenes de producción".
- **Logs con la URI completa y el query redactado**: más diagnóstico, pero mete identificadores de la ruta en el log y obliga a declararlos.
- **`user_id` en la línea de petición**: útil para soporte, pero lo vuelve dato personal ligado a la actividad; si una feature lo necesita, lo justifica y actualiza el aviso.
- **Cookie `PARAGLIDE_LOCALE` de 400 días** (valor por defecto de Paraglide): ver "Diseño → Cookies".
- **Capturas con comparación de píxeles**: ver "Áreas de preocupación" 5.
- **Implementar `DEPLOY_MODE=api` ya**: ver "Áreas de preocupación" 5.
- **Postgres de la CI como `services:` y `just db-up` sin efecto con `CI=true`** (opción b de devops-engineer): dos caminos distintos para lo mismo y un digest que Dependabot no ve en el workflow. Elegí `just db-up` también en CI (RF-C9).
- **Guard de `/dev/**` solo en `+layout.server.ts`**: no impide que corran los `load` de página, los `+server.ts` ni las actions (RF-A16).
- **`infra/.env.example`** para la forma local: invita a crear un `infra/.env` con el `DATABASE_URL` real. `infra/local.env` commiteado con valores de desarrollo y `--env-file` explícito (RF-C5, RF-C7).
- **Inactividad del WS medida en frames (pongs incluidos)**: equivale al ping/pong y no acota un socket que solo contesta pings. Se mide en mensajes de datos aceptados (RF-B13).
- **SvelteKit 2.70.3 con una migración posterior a Kit 3 (TA-9)** (enmienda 2026-10-22): era el plan original de la fase A. El owner eligió Kit 3 desde el inicio, para no migrar dos veces ni salir a producción con una rama de Kit que ya no recibe versiones.
- **`paths.origin` fijado al compilar** (enmienda 2026-10-22): ancla el CSRF de Kit a un valor fijo, pero mete el host dentro de la imagen. La imagen de `prod-local` dejaría de ser la de producción y cambiar de dominio obligaría a recompilar. El origen de confianza va en `PUBLIC_ORIGIN`, en runtime (RF-A23).
- **`PROTOCOL_HEADER=x-forwarded-proto` y `HOST_HEADER=x-forwarded-host` en producción** (enmienda 2026-10-22): harían que el origen dependa de headers. Hoy Traefik los reescribe, pero un cambio de su configuración los dejaría pasar desde el cliente. Con `https` fijo y `Host` restringido por la regla del router no hace falta confiar en ellos.
- **Confiar solo en el CSRF de Kit 3** (enmienda 2026-10-22): no cubre JSON ni otros tipos de contenido y compara contra el `Host`, no contra una variable. ADR-0008 D4 pide la regla completa para toda petición no segura.
- **Rechazar el arranque (código 1) con `ENABLE_WS_ECHO=1` y `PUBLIC_ORIGIN` `https://`**: una variable suelta en Coolify tumbaría producción. Dejar el eco apagado con un `warn` falla cerrado sin caída (RF-B13).

## Enmienda 2026-10-22: SvelteKit 3

**Decisión del owner (2026-10-22):** la fase A usa **SvelteKit 3** desde el inicio, en vez de Kit 2.70.3 con una migración posterior. Por eso esta spec deja de depender de **TA-9** ("Migrar web a SvelteKit 3"): no hay nada que migrar. TA-9 queda sin objeto y deja de bloquear a TA-8 ("Primer despliegue a producción"); cerrarla en Linear le toca al owner. El resto de la spec no cambia, y las aprobaciones de 2026-10-21 siguen valiendo para todo lo que esta enmienda no toca.

Lo verifiqué en `@sveltejs/kit@3.1.0`, `@sveltejs/adapter-node@6.0.0` (código y `CHANGELOG`) y en la guía oficial de migración a Kit 3.

| Cambio de Kit 3 | Cómo queda en la spec |
|---|---|
| Sin `svelte.config.js`: la configuración va en `sveltekit({ … })` de `vite.config.ts`; `kit.csp` pasa a ser `csp` | RF-A2, RF-A10. El nonce (`%sveltekit.nonce%`), `transformPageChunk` y `reroute` no cambian. Kit 3 además rompe el build si se prerenderiza una página con `%sveltekit.nonce%` en la plantilla, lo que refuerza RF-A2 |
| `$lib` → `#lib` (*subpath imports* en `package.json`, imports con extensión) | RF-A2, RF-A14; ADR-0009 D2, D4 y test 2 enmendados |
| `csrf.checkOrigin` eliminado: el chequeo de formularios está siempre activo y solo se apaga con `trustedOrigins: ['*']` | RF-A10: no se escribe `trustedOrigins` ni `paths.origin`, y lo verifica un chequeo. Como ese chequeo solo cubre formularios y compara contra el `Host`, se agrega el guard de `Origin` propio (RF-A23), alineado con ADR-0008 D4 (enmendado) |
| adapter-node 6 sin `ORIGIN`; `paths.origin` es de build | RF-C2, RF-C4, RF-C6, RF-C10 y tabla de variables: `web` recibe `PUBLIC_ORIGIN` (runtime, para RF-A23); el origen de la petición es `https://` + `Host`, sin `PROTOCOL_HEADER`, `HOST_HEADER` ni `PORT_HEADER`. `ADDRESS_HEADER` y `XFF_DEPTH` siguen existiendo igual (ADR-0008 D5 no cambia) |
| `handleError` recibe **todos** los errores, con `kind`; si lanza, Kit escribe el error crudo con `console.error` | RF-A19: solo se registra `kind: 'unknown'`, `error_id` solo en esos, nunca lanza; e2e de 404 sin logs. "Diseño → UI": `error_id` solo cuando existe |
| `$env/*` y `$app/environment` obsoletos; variables en `src/env.ts` con `defineEnvVars` y `$app/env/*` | RF-A2, RF-A16, RF-A23; ESLint los prohíbe (RF-A14); ADR-0009 D5 enmendado |
| Node ≥ 22.17, TypeScript 6, Vite 8, `vite-plugin-svelte` 7 | RF-A1, RF-A2, RF-C2; "Áreas de preocupación" 12 |

**Requisito nuevo:** RF-A23 (guard de `Origin` en `web`). **Riesgos:** "Áreas de preocupación" 12. **Alternativas:** cuatro nuevas en "Alternativas consideradas".

**Efecto en `plan-a.md`:** su decisión 1 (Kit 2.70.3 y TA-9) y las partes que dependen de ella (`svelte.config.js`, `$lib`, el filtro de `handleError` por `status`, `$env/dynamic/private`, el orden de los `handle` sin el guard de `Origin`) quedan desfasadas. frontend-engineer actualiza el plan cuando esta enmienda esté aprobada, y el architect lo vuelve a revisar.

**Revisión de la enmienda:** security-reviewer (RF-A10, RF-A19, RF-A23, RF-C4 y RF-C6) y frontend-engineer (RF-A2, RF-A14, RF-A16 y la viabilidad con Paraglide y shadcn-svelte). El owner ya dio su OK a la dirección; vuelve a confirmar porque la spec es `risk: high`.


## Enmienda 2026-10-23: menor

Surge de la revisión de security-reviewer a la enmienda de Kit 3 (hallazgo 1), de la revisión del plan para Kit 3 (decisión 20 de `plan-a.md`) y de la revisión de QA del PR #1 (desviación 16 de `plan-a.md`). No cambia el alcance ni el diseño. Las aprobaciones anteriores siguen valiendo para todo lo que esta enmienda no toca.

| Cambio | Dónde |
|---|---|
| Regla nueva: `web` nunca construye URLs absolutas con `event.url.origin`, `url.host` ni `url.href`; usa `PUBLIC_ORIGIN`. Ya la aplica ESLint desde la fase A | "Requisitos no funcionales → Seguridad" |
| "Ningún header que el cliente pueda falsificar cambia el origen" vale solo **detrás de Traefik**. Quien llega directo al contenedor elige el `Host` | RF-C4 y "Seguridad" |
| Traefik pone `X-Content-Type-Options`, `Referrer-Policy` y HSTS (sin `includeSubDomains` ni `preload`) en todas las respuestas de `web`, incluidos `/_app/*` y la 308 de `/healthz/`, con el middleware `torneo-web-headers` | RF-C4 (requisito), RF-C6 (chequeo estático), RF-C7 (`curl -I` de línea base en `prod-local`), RF-C18 (`curl -I` detrás de Traefik, prerrequisito del primer despliegue), "Áreas de preocupación" 13 |

**Dónde se verifica.** Se pidió verificar las cabeceras con `curl -I` en `prod-local`, pero `prod-local` no tiene Traefik (ver "Flujos"): ahí `web` se publica directo en `127.0.0.1:3000` y `/_app/*` y `/healthz/` salen sin esas cabeceras. Por eso la verificación queda en tres partes. RF-C6 comprueba las etiquetas en `just check` y en `images`. RF-C7 toma en `prod-local` el `curl -I` de línea base. RF-C18 hace el `curl -I` real detrás de Traefik en el primer despliegue. RF-C10 y RF-C11 no cambian, porque ni `e2e` ni `images` corren con Traefik. Agregar Traefik a `compose.local.yml` daría una prueba en vivo antes de producción, pero agranda la fase C; queda como alternativa para devops-engineer si la prefiere.

**Efecto en los planes.** `plan-a.md` no cambia: la regla de ESLint y las cabeceras de `handle` ya están implementadas. El plan de la fase C incluye el middleware, el chequeo de RF-C6 y las dos verificaciones con `curl -I`.

**Revisión de la enmienda:** security-reviewer (regla de URLs absolutas, RF-C4 y los valores de HSTS) y devops-engineer (middleware de Traefik, RF-C6, RF-C7 y RF-C18).

## Enmienda 2026-10-23 (b): fase B

Surge de la revisión del architect a `plan-b.md` (entrada `architect — 2026-10-23`, respuestas 1 a 4). No cambia el alcance. Las aprobaciones anteriores siguen valiendo para todo lo que esta enmienda no toca.

| Cambio | Dónde |
|---|---|
| `ws.rate_limited` nuevo en `WsErrorCode`: el cierre por tasa es `Close(1008, "ws.rate_limited")` y la web lo traduce con `ws_close_reason_rate_limited` | RF-B5, RF-B7, RF-B13, "Estrategia de pruebas" |
| `ApiError { error: ApiErrorCode }` → `ApiError { error: ErrorCode }`, con `ErrorCode` = unión sin etiquetar `ApiErrorCode \| WsErrorCode`. Los conjuntos de cadenas no se solapan (unit), y cada dominio nuevo (auth) va en un enum y una variante nueva | RF-B4, RF-B5 |
| Los enums de códigos usan `#[serde(rename = "dominio.motivo")]` por variante, no `rename_all` | RF-B4 |
| `BIND_ADDR` nueva en el server: opcional, `IpAddr`, por defecto `0.0.0.0`. `server healthcheck` supone `0.0.0.0` o `127.0.0.1`. `check-compose.mjs` la prohíbe en `server` (fase C) | "Variables de entorno", RF-B11, RF-C6 |
| Orden de los chequeos de `/ws`: `Origin` → upgrade → semáforo. `GET /ws` sin upgrade responde con el estado de axum (4xx), cuerpo vacío, sin `Content-Type` y con `nosniff` y HSTS | RF-B13, "API / mensajes WS" |

**Seguridad de los cambios.** El cuerpo vacío en el rechazo del upgrade evita texto en inglés en el API (ADR-0007), y no filtra nada: el estado es el mismo que da axum. Poner el semáforo después del upgrade evita que peticiones que no son WebSocket gasten o sondeen el tope. Con `BIND_ADDR`, un bind en una IP que no sea de loopback ni `0.0.0.0` haría fallar el healthcheck del contenedor, y por eso el compose no la acepta. En dev, `127.0.0.1` evita publicar el 8080 en la LAN (RNF "Seguridad").

**Efecto en los planes.** `plan-b.md` incorpora las respuestas 1 a 4 (su paso 2 espera la aprobación de esta enmienda) y corrige el orden de su decisión 10, que hoy pone el semáforo antes del upgrade. El plan de la fase C agrega la regla de `BIND_ADDR` a `check-compose.mjs`.

**Nota (2026-10-23, revisión de `plan-b.md` ronda 2).** La variable que aquí se llamaba `HOST` pasa a llamarse `BIND_ADDR`, a propuesta de backend-engineer. zsh define `HOST` con el nombre de la máquina, algunos perfiles y entornos de CI la exportan, y `web` ya usa `HOST` (adapter-node) con otro dueño. Con `HOST`, un nombre de máquina heredado haría fallar el arranque con `config.invalid`, y otra IP rompería el healthcheck. Solo cambia el nombre: el tipo, el valor por defecto, la condición del healthcheck y la prohibición en el compose son las mismas. Toca la tabla de variables, RF-B11 y RF-C6. `HOST` de `web` (RF-C4) no cambia.

**Revisión de la enmienda:** security-reviewer (orden del handshake, rechazo sin cuerpo, `BIND_ADDR` y su efecto en el healthcheck, `ErrorCode` sin etiquetar). El cambio de nombre de la nota anterior también pasa por security-reviewer.

## Enmienda 2026-10-24: distroless debian13

Surge de `plan-c.md` ("Bloqueo de inicio", decisión 2). No cambia el alcance ni el diseño. Las aprobaciones anteriores siguen valiendo para todo lo que esta enmienda no toca.

| Cambio | Dónde |
|---|---|
| La base final del server pasa de `gcr.io/distroless/static-debian12:nonroot` a `gcr.io/distroless/static-debian13:nonroot` | RF-C1 |
| La base final de `web` pasa de `gcr.io/distroless/nodejs22-debian12:nonroot` a `gcr.io/distroless/nodejs22-debian13:nonroot` | RF-C2 |

**Motivo.** Las imágenes `-debian12` de distroless ya no reciben parches. Lo verifiqué el 2026-10-24:
- El README de `GoogleContainerTools/distroless` publica solo imágenes `-debian13` y dice: "Any other tags are considered deprecated and are no longer updated".
- La última subida de `nodejs22-debian12` en `gcr.io` es del 2026-02-14, y la de `static-debian12` del 2026-08-21. `nodejs22-debian13` y `static-debian13` tienen subidas del 2026-10-23.

Una base sin parches contradice el propósito de "actualizadas por Dependabot" de ADR-0006 D5. Dependabot mueve digests dentro de la misma etiqueta y no cambia de `-debian12` a `-debian13`.

**Lo que no cambia.**
- Las dos siguen siendo distroless `nonroot` (UID 65532), sin shell, fijadas por digest de índice y con la antigüedad mínima de 3 días.
- `nodejs22-debian13` mantiene `/nodejs/bin/node` como entrypoint, que es la ruta del healthcheck de ADR-0006 D3. Su digest debe corresponder a la versión de `.node-version`, igual que antes (RF-C2).
- `check-compose.mjs` sigue exigiendo que la base final de `web` sea `nodejs<major de .node-version>-`, sin fijar la versión de Debian.
- El binario del server es estático (musl, D8) y no depende de la libc de la base. Los builders (`rust:…-bookworm`, `node:…-bookworm-slim`) no cambian: solo compilan, y su versión de Debian no llega a la imagen final.

**Mantenimiento.** El salto a la siguiente versión de Debian no lo hace Dependabot. Cuando distroless deprecie `-debian13`, se cambia a mano en una enmienda como esta.

**ADR-0006.** D5 nombra `-debian12`. Le agregué una nota fechada en "Notas", sin cambiar su `status` ni sus approvals. El cuerpo de D5 se corrige cuando el owner dé el OK.

**Revisión de la enmienda:** security-reviewer (bases de las imágenes finales) y el owner, porque toca el texto de un ADR. devops-engineer propuso el cambio.

## Revisión

### product-owner — 2026-10-21 — APPROVE
- Verifiqué: comparé la spec con el intent aprobado, punto por punto. La tabla de trazabilidad cubre los 20 criterios: A1–A5, B1–B6 y C1–C9. Revisé en el cuerpo de cada uno que su requisito diga cómo se prueba (unit, integración, e2e, CI, release o manual con evidencia en el PR), y todos se pueden verificar sin el VPS. Las cuatro preguntas técnicas del intent tienen respuesta y razones: `ts-rs` detrás de la feature `ts`, hook con script POSIX y `core.hooksPath`, eco tipado, apagado por defecto, con `Origin` y límites, y logs del server sin datos del cliente con el plazo "menos de 3 semanas". También respondió lo que el intent le dejó decidir: la forma local con Postgres, las redes externas y amd64 en Mac ARM, y la cookie `locale` de 1 año. Las dos preguntas del owner pasan a "Preguntas abiertas" de forma explícita, con las dos ramas resueltas en RF-C19, y ninguna bloquea A ni B. Las dependencias A → B → C coinciden con el intent. Respeta lo que pide el intent: mobile-first a 375 px, que se prueba con el test 8 y el scroll horizontal; es bilingüe sin strings literales, con RF-A15 usando el parser de Svelte; no tiene features de producto, no despliega, no usa servicios de pago y no toca dinero ni contenido, así que no choca con ADR-0004 ni con ADR-0005; y la i18n cumple ADR-0007, con el protocolo y el API emitiendo solo códigos. "No entra" y "Fuera de alcance" están intactos: auth, CSRF, `TRUSTED_PROXIES`, rate limit por IP, renderer y test 7, identidad visual y primer despliegue. El frontmatter es válido, con `risk: high` y owner y security-reviewer como revisores.
- Notas (no bloquean):
  1. **Agregados que no pidió el intent**: la CSP y las cabeceras (RF-A10), el endurecimiento del compose (RF-C4), Dependabot (RF-C16) y el job `verify` (RF-C15). Los acepto porque son base, no cuestan nada y no son producto. Pero la CSP condiciona a 002 y al futuro `engine-wasm`, así que pido que el ADR-0012 propuesto en "Áreas de preocupación" 3 se abra en Linear al aprobarse esta spec, y que no quede solo como intención.
  2. **Si el owner responde "no" a la pregunta 1**, C5 y C6 salen de 001. Eso cambia criterios de un intent aprobado. En ese caso hay que actualizar el intent y que lo apruebe de nuevo, no basta con la spec.
  3. **Tamaño de fases**: las tres son grandes (unos 20 RF cada una) pero coherentes. La C mezcla imágenes y `prod-local` con CI, release y política. Sugiero que el plan de C la parta en dos PRs (imágenes, compose y `prod-local` por un lado; `ci.yml`, `release.yml`, `policy` y el hook por otro) si el diff pasa de lo que se puede revisar con calma.

### security-reviewer — 2026-10-21 — REQUEST_CHANGES
**🤖 security-reviewer**

- Verifiqué:
  - **Frontmatter**: es válido según `docs/process.md`. Trae `risk: high`, `linear: TA-1` y `reviewers` con security-reviewer y owner, que es lo que corresponde a `risk: high`.
  - **ADR-0008**: la spec respeta lo que toma de este ADR. Con D3, los headers redactados son un superconjunto de la lista del ADR (agrega `X-Forwarded-For`) y no hay query en los logs. Con D4, no se monta `CorsLayer` y la integración prueba el preflight ajeno. Con D6, el `Origin` exacto en el handshake, los 4 KB, los 10 mensajes cada 10 s y el tope `WS_MAX_CONNECTIONS` coinciden con el ADR. Los límites por IP y por usuario quedan para auth, y lo declara.
  - **ADR-0006**: la spec respeta D1 a D11 en lo que toca a seguridad. Hay `${IMAGE_TAG:?}`, `observability.accesslogs=false` en los cuatro routers, imágenes distroless `nonroot` por digest, `provenance: false`, prohibición de `pull_request_target`/`workflow_run`, actions fijadas por SHA, `permissions` por workflow, `DEPLOY_MODE` que falla cerrado (con `api` el job falla) y ningún código que llame a Coolify.
  - **ADR-0009 D3**: la cookie `theme` se valida antes de `transformPageChunk`, con un test con `"><script>`. El script del tema es estático y lleva nonce. No hay prerender, y el override `?theme` depende de `ENABLE_DEV_CATALOG` y de `route.id`.
  - **Aviso de privacidad**:
    - La fila "Registros técnicos del servidor" ("ruta, método, resultado y fecha, sin IP ni cookies") coincide con RF-B15.
    - `web` no registra peticiones.
    - Los canarios cubren el filtro `info` y también `debug` global, así que también atrapan el span de `TraceLayer` con `uri` y lo que registren sqlx o hyper.
    - "Menos de 3 semanas" es coherente con 3 × 10 MB y el healthcheck cada 15 s, y RF-B20 obliga a medirlo.
    - La cookie `locale` de 1 año llena `{{DURACION_COOKIE_IDIOMA}}`.
  - **Puntos que pidió el architect**:
    1. **`/ws` sin límites por IP**: no basta como "riesgo residual". El primer despliegue debe esperar esos límites (punto 1).
    2. **Ninguna ruta no-GET**: la regla es correcta, pero hoy depende solo de la revisión (punto 4).
    3. **Eco tras `ENABLE_WS_ECHO=1`**: el diseño es correcto. Un eco tipado y apagado por defecto es más estricto que D6. Hay una defensa extra opcional (punto 5).
    4. **Contrato de logs**: cumple el aviso. Solo hay ajustes menores (puntos 6 y 7).
    5. **CSP**: es correcta. Falta HSTS (punto 8).
    6. **Environment `production`**: las dos ramas de la pregunta abierta 1 prohíben empujar un tag `v*` antes de proteger `production`. Además, `deploy` en modo `manual` no hace nada. Me sirve.
    7. **Endurecimiento del compose**: es correcto. Falta corregir `.dockerignore` (punto 3).
    8. **`zig` con checksum**: está bien. Hay que extenderlo al resto de la cadena del build (punto 9).
    9. **Dependabot**: está bien. Hay ajustes menores (punto 9).
    10. **Ningún `secrets.*` en `ci.yml`**: la regla tiene huecos de sintaxis (punto 10).

**Cambios pedidos (Important, bloquean):**

1. **`/ws` puede quedar en producción sin límite por IP y sin timeout de inactividad, y nada impide desplegarlo así.** Área de preocupación 1, RF-B13.
   - *Escenario:* tras el primer despliegue, un solo host abre 2000 upgrades con `Origin` correcto (lo pone cualquier cliente que no sea un navegador) y no manda nada. Como ningún mensaje cuenta para el límite por socket y no hay idle ni ping/pong, los sockets viven para siempre. El tope global se llena y todos los espectadores reciben `ws.capacity`. Cuando exista el WS real, eso es un DoS del main event desde una sola IP. La reserva del 20 % de D6 tampoco existe todavía.
   - *Cambio:* convertir el "riesgo residual" en una condición escrita.
     - **El primer despliegue a producción queda bloqueado hasta que existan los límites anónimos de D6 y D7** (50 sockets por `ip_key`, upgrade a 30/min con ráfaga 10, con `TRUSTED_PROXIES`). Ponerlo en RF-C18, dentro del procedimiento de "Despliegue" de `docs/architecture.md`, y en RF-C19 como prerrequisito del issue de primer despliegue.
     - Ya en 001, agregar a RF-B13 un ping del server cada 30 s con cierre si no llega el pong en 60 s, y un timeout de inactividad. Son baratos y no dependen de auth.
     - Agregar un test de integración: un socket sin pong se cierra.

2. **El guard de `/dev/**` en `+layout.server.ts` no cubre todo `/dev/**`, como afirma RF-A16.**
   - SvelteKit corre los `load` de servidor **en paralelo**. El `load` de la página corre aunque el layout lance `error(404)`, salvo que haga `await parent()`.
   - Además, el layout no aplica a `+server.ts` ni a las form actions, que corren antes de los `load`.
   - *Escenario:* en producción, sin `ENABLE_DEV_CATALOG`, `GET /es/dev/error-probe` responde 404, pero su `+page.server.ts` se ejecuta igual. Lanza el error, pasa por `handleError` y escribe una línea con stack por petición, que cualquiera puede repetir en bucle.
   - Hoy el impacto es chico. Lo grave es que la spec garantiza una propiedad de seguridad que el mecanismo no da. La siguiente ruta de prueba (un `+server.ts` o una action en `/dev/`) quedaría expuesta, y el test 6 de ADR-0009 seguiría en verde, porque solo mira el 404 de `/es/dev/components`.
   - *Cambio:* hacer el guard en `handle` de `hooks.server.ts`: si `event.route.id` empieza con `/dev` y `ENABLE_DEV_CATALOG !== '1'`, devolver 404 antes de `resolve`. Se puede mantener el del layout como segunda capa.
   - *Test e2e*: sin la variable, `/es/dev/error-probe` da 404 y **no escribe ninguna línea** en stdout ni stderr.
   - Es una extensión de ADR-0009 D5 y hay que anotarla en el área de preocupación 5.

3. **RF-C3: los patrones de `.dockerignore` solo excluyen los `.env` de la raíz.**
   - En `.dockerignore`, `.env` y `.env.*` se comparan contra la ruta relativa a la raíz del contexto. A diferencia de `.gitignore`, no alcanzan subdirectorios.
   - *Escenario:* el owner tiene `infra/.env` con el `DATABASE_URL` real (RF-C5 invita a crearlo) o `apps/server/.env`, y corre `just prod-local` o el plan B de ADR-0006 D8. El `COPY . .` del builder mete el archivo en una capa de la etapa de build y en la caché de buildx. Si el cache se exporta (`cache-to … mode=max`), el secreto viaja con él.
   - *Cambio:* usar `**/.env`, `**/.env.*` y `!**/.env.example` (y también `**/node_modules`, que ya está). `check-compose.mjs` debe verificar las formas con `**/`. Un fixture con `infra/.env` debe quedar fuera del contexto: se comprueba con `docker build` de un Dockerfile de prueba que hace `COPY . /ctx` seguido de `test ! -e /ctx/infra/.env`, o con `docker buildx … --print` si existe la opción.

**No bloqueantes (Minor):**

4. **"Ninguna ruta no-GET" depende solo de la revisión.** Propongo aplicarla en código. Una capa en el router de 001 que responda 405 a todo método distinto de `GET`, `HEAD` y `OPTIONS`, más un test de integración (`POST /api/health` da 405). La feature de auth la reemplaza por el middleware de D4, así que nadie agrega una mutación sin quitarla a conciencia. En `web`, escribir en RF-A10 que `csrf.checkOrigin` no se desactiva.

5. **Defensa extra para el eco.** `check-compose.mjs` no ve las variables que alguien agregue en la UI de Coolify. Propongo que el server se niegue a encender el eco si `PUBLIC_ORIGIN` es `https://` (dev, tests y e2e usan `http://localhost`) y que registre un `warn` `ws.echo_enabled` al arrancar. El impacto de un eco encendido es bajo, porque solo refleja al mismo socket.
   - Precisar en RF-B13 que los 4 KB se fijan **en el protocolo**, con `max_message_size` y `max_frame_size` de `WebSocketUpgrade`. Si se comprueban después de recibir, valen los valores por defecto de tungstenite (64 MB por mensaje y 16 MB por frame): unos pocos sockets con mensajes grandes provocan un OOM-kill con 256 MB.

6. **Logs de `web`.**
   - El saneador funciona por lista negra. `error_message` puede traer la ruta de una URL (identificadores) u otros datos que no siguen a un nombre de header. Basta para 001. Para el ADR-0012 propongo registrar el mensaje solo de errores propios con código, y para el resto, `error_name` más el stack.
   - En RF-A19, el handler de `uncaughtException` debe registrar y después `process.exit(1)`. Si no sale, el proceso sigue en un estado indefinido.
   - Aclarar en el aviso o en la spec que `web` escribe poco, así que sus logs pueden durar más de 3 semanas. No es un problema mientras no contengan datos personales, que es lo que garantizan RF-A19 y el e2e.

7. **`/api/health` sin rate limit hace un `SELECT 1` por petición.**
   - *Escenario:* una ráfaga externa ocupa el pool de 8 conexiones, `/api/health` empieza a dar 503 y Docker marca `server` como unhealthy. Traefik deja de enrutar contenedores unhealthy, así que el resultado es una caída total mientras dure la ráfaga.
   - Propongo cachear el resultado de la BD durante 1 a 2 s, con una sola consulta en vuelo. Es compatible con ADR-0006 D3 y no cambia los tests de 200 → 503 → 200.

8. **Cabeceras.**
   - Falta `Strict-Transport-Security`. Hay que agregarlo antes de que exista `__Host-session`, sea con un middleware `torneo-headers` en las etiquetas o en `hooks.server.ts` y axum, con `max-age` de al menos 1 año. En 001 puede ir sin `preload`.
   - Agregar un test de RF-A3: `GET //evil.example/` y `GET /\evil.example` redirigen a un `Location` que empieza con `/es/` o `/en/`. No debe ser una redirección abierta.

9. **Cadena de suministro.**
   - Fijar también `cargo-chef` y `cargo-zigbuild` (`cargo install --locked --version …`) y **todas** las etapas `FROM` por digest, incluidos los builders de Rust y `node:22`.
   - `packageManager` con hash (`pnpm@x.y.z+sha512…`).
   - Considerar `minimumReleaseAge` en pnpm y `cooldown` en `dependabot.yml`, contra versiones maliciosas recién publicadas.
   - Dependabot nunca se fusiona solo.
   - Activar las alertas de Dependabot en la configuración de GitHub de RF-C19, porque son el reemplazo declarado de `cargo deny check advisories`.

10. **`check-workflows.mjs` (RF-C12).**
    - Buscar `secrets.` no detecta `toJSON(secrets)`, `secrets[...]` ni `secrets: inherit`. Hay que rechazar cualquier referencia al contexto `secrets` salvo `secrets.GITHUB_TOKEN`.
    - Escanear también `.github/actions/**`.
    - Exigir que los `permissions` de raíz sean `contents: read`, no basta con que existan.
    - Exigir `persist-credentials: false` en los checkouts.
    - En `release.yml`, `inputs.tag` y `github.ref_name` deben pasar por `env:` y validarse con la regex **también en `workflow_dispatch`**, nunca interpolarse con `${{ }}` dentro de `run:`. Hoy solo puede dispararlo el owner, pero un nombre de tag como `v1$(…)` es válido en git.

Cuando estén resueltos los puntos 1 a 3, apruebo sin otra ronda completa: verifico solo esos puntos.

### devops-engineer — 2026-10-21 — REQUEST_CHANGES
- Verifiqué: el frontmatter contra `docs/process.md` (válido). Contrasté la fase C y lo que toca `infra/`, `.github/`, el `justfile`, Docker y la toolchain con ADR-0006 D1–D11, sus "Números fijados", las verificaciones 1–17 y las "Decisiones del owner". También revisé RF-B1, RF-B2, RF-B17 y RF-B19 porque la fase C depende de ellos. Revisé el "No entra" del intent y el remoto `kike1303/torneo-absurdo`, que coincide con la imagen de RF-C4. Todo lo de abajo se lee en el texto de la spec; no ejecuté builds. Hay cosas que están bien y no requieren cambios. `compose.dev.yml` está atado a `127.0.0.1`. El endurecimiento del compose es compatible con distroless `nonroot` y con puertos > 1024. `deny.toml` usa la sintaxis actual (`[graph] targets`, `{ crate = ... }`), que corrige el `name` del ADR. `DEPLOY_MODE=api` falla de forma segura: el gate de `production` va primero, el fallo después, y no hay token. `policy` dentro de `ci-ok` cuadra con el "un saltado cuenta como éxito" de D7. El filtro `rust` con `packages/protocol-ts/**` está bien. `core.hooksPath` relativo se resuelve en la raíz de cada worktree. La espera de aprobación del environment no cuenta para `timeout-minutes`, así que `deploy` = 10 min alcanza. La cuenta del plazo de logs es correcta (5,760 × ~280 B ≈ 1.6 MB/día, 30 MB ≈ 18.6 días).
- Cambios pedidos (bloqueantes):
  1. **`just check` y el Postgres de la CI se contradicen (RF-B19 contra RF-C9).** `just check` llama a `just db-up`, que levanta `compose.dev.yml` en `127.0.0.1:5432`. Al mismo tiempo, el job `rust` declara un `services: postgres`, que para publicarse al runner necesita `ports: 5432:5432`. Tal como está escrito, los dos chocan por el puerto, o hay dos Postgres y no queda claro a cuál apunta `DATABASE_URL`. Hay que elegir una de dos. (a) La CI no usa `services:` y corre el mismo `just db-up`, que ubuntu-latest trae Docker; esta es mi recomendación, porque es un solo camino, local y CI iguales, y el digest lo actualiza Dependabot. (b) Se mantiene el servicio y `db-up` no hace nada si `CI=true`. Hay que decirlo en RF-C9.
  2. **`PUBLIC_HOST` no se puede fijar desde `compose.local.yml` (RF-C7).** Compose interpola cada archivo **antes** de combinarlos, y solo con el entorno del shell y el `--env-file`/`.env` del proyecto, nunca con el `environment:` de un servicio. Como el compose de producción usa `${PUBLIC_HOST:?}` e `${IMAGE_TAG:?}`, `just prod-local` falla tal como lo describe RF-C7 ("compose.local.yml agrega ... `PUBLIC_HOST=localhost`"). La tabla de variables ya dice "local la receta", así que RF-C7 está mal. Corrección: la receta exporta `IMAGE_TAG` y `PUBLIC_HOST` (y `DATABASE_URL` si el compose de producción la interpola con `:?`), o pasa un `--env-file` explícito. `compose.local.yml` solo fija valores de `environment:` que no se interpolan. Ojo: con `-f infra/docker-compose.yml` el directorio del proyecto es `infra/` y Compose carga solo `infra/.env` si existe. Conviene `--env-file` explícito para no mezclarlo con un `.env` del desarrollador.
  3. **Filtros incompletos (RF-C8).**
     - `deny.toml` no está en el filtro `rust`. Un PR que quite `openssl-sys` de `bans.deny` no corre `cargo deny` y pasa en verde. Es justo la regla que debería proteger.
     - `scripts/**` no está en ningún filtro. Un cambio a `check-i18n.mjs`, `check-tokens.mjs` o `check-compose.mjs`, o a sus fixtures, no corre sus tests: ninguno corre en `policy`, solo `check-workflows.mjs`.
     - Faltan en `web` los archivos de configuración de la raíz: `.npmrc`, `.prettierrc*`/`.prettierignore` y `eslint.config.*` si viven en la raíz.
     - Propuesta: `rust` agrega `deny.toml`; `web` agrega `scripts/**`, `.npmrc` y la configuración de formato y lint de la raíz; `images` agrega `scripts/check-compose.mjs`. Hay que anotarlo como extensión de D7 en "Áreas de preocupación" 5.
  4. **Medición del plazo de logs en la fase equivocada.** "Diseño → Logs y plazo" dice que "el PR de la fase B mide ... 1 h de `just prod-local`". Pero `just prod-local`, las imágenes y el healthcheck de Docker que genera las 5,760 líneas son de la fase C. Hay dos opciones. (a) B calcula con el tamaño real de la línea de `/api/health` sacado de `cargo run` y C lo confirma con `just prod-local` y ajusta el aviso. (b) Todo se mide en C, y RF-B20 deja `{{PLAZO_LOGS}}` como provisional. Además hay que fijar `logging.driver: json-file` explícito en RF-C4: la cuenta supone el envoltorio de `json-file` y no conviene depender del driver por defecto del daemon del VPS.
  5. **Las dos variantes de las preguntas del owner no cierran (RF-C19).**
     - **GitHub, variante "no":** "Preguntas abiertas" 1 dice que no se empuja ningún tag `v*` y que C5 y C6 pasan a la tarea del primer despliegue. Pero RF-C19 pone el tag `v0.0.1` como **"Siempre"**. Hay que condicionar el punto del tag a la variante "sí", o hacer explícito el caso "se acepta un `production` sin protección".
     - **VPS, variante "parte de 001":** pide anotar "las verificaciones 13 y 15 de ADR-0006", pero las dos necesitan `web` y `server` desplegados (13: `docker inspect` de los routers y `/api/health` en el access log; 15a y 15c: `getent` desde `web` y redes de `web`/`server`). 001 no despliega, y el intent pone el aislamiento de red en "No entra". En 001 solo se puede verificar la parte del host: proxy sin `--accesslog` y con Traefik ≥ 3.1 (13, primera mitad); `torneo-edge` con `--ip-range`, con `coolify-proxy` en `10.250.0.2` como único miembro, también tras reiniciar el proxy (15b parcial); `swapon --show` (parte de la 4); y la limpieza de Docker con "Delete Unused Volumes" apagado. Lo demás va al primer despliegue.
- Cambios sugeridos (no bloqueantes):
  6. **`cache-to` en `release.yml` (RF-C13).** Las cachés de Actions que escribe una corrida de un tag quedan en el ámbito de ese ref. El tag siguiente no las ve; solo ve las de la rama por defecto. `mode=max` con un target de Rust en `release` y LTO pesa cientos de MB, que ocupan la cuota de 10 GB y desalojan por LRU las de `main` (rust-cache, pnpm, Playwright). Propongo dos cambios. En release, solo `cache-from: scope=<imagen>`, que lee la de `main`. Y para que esa caché siga caliente, que el filtro `images` incluya `Cargo.lock`, `rust-toolchain.toml` y `pnpm-lock.yaml`. Eso además detecta en el PR un cambio de dependencias que rompa el build cruzado, que es el riesgo 6, en vez de descubrirlo en el release.
  7. **Dockerfile del server (RF-C1).**
     - La descarga de `zig` necesita **un checksum por arquitectura** del builder (`BUILDARCH` amd64 en Actions, arm64 en Mac). En Mac el build corre nativo en arm64 dentro de la VM de Docker Desktop; Rosetta solo interviene al ejecutar.
     - Hay que usar `cargo chef cook --zigbuild --release --target x86_64-unknown-linux-musl`.
     - `ARG APP_VERSION` va **después** del `cook`, para que el cambio de versión no invalide la capa de dependencias.
     - El `FROM rust:<versión>` debe ser el mismo que `rust-toolchain.toml`, o rustup descarga la toolchain en cada build en frío.
     - `ring` 0.17 y `libmimalloc-sys` compilan con `cc`, y `zig cc` para `x86_64-musl` es el caso habitual. El respaldo del riesgo 6 es razonable.
  8. **Chequeo de binario estático.** Los binarios de `x86_64-unknown-linux-musl` son `static-pie` por defecto, y el `ldd` de glibc puede responder "statically linked" en vez de "not a dynamic executable". Es más robusto comprobar que no hay `INTERP` (`readelf -l` sin `PT_INTERP`) o aceptar las dos salidas. Aplica a RF-C1 y RF-C9.
  9. **Smoke test de la imagen `web` (RF-C2, RF-C11).** Que `/app/node_modules` no exista no prueba que el bundle sea autocontenido: los e2e corren con `node_modules` presente. El job `images` debería hacer `docker run` de la imagen y `curl` a `/healthz` y a `/en/`, que debe dar 200.
  10. **Compose y endurecimiento.**
      - `tmpfs: /tmp` en `web` necesita un tamaño (`/tmp:size=16m`): sin él, el tamaño por defecto es la mitad de la RAM del host y lo que se escribe ahí cuenta contra los 384 MB.
      - En la verificación del primer despliegue hay que agregar `docker inspect` de `ReadonlyRootfs`, `CapDrop` y `SecurityOpt`, para confirmar que Coolify los respeta al reescribir el compose.
      - `check-compose.mjs` debe aplicar "solo `web` y `server`" a `docker-compose.yml`, no a `compose.local.yml`.
  11. **Redes locales.** `scripts/local-networks.sh` debería crearlas con una etiqueta (`--label torneo.local=1`), y `just prod-local-down` solo debería borrar las que la tengan. Así, si alguien corre la receta donde no debe, no borra el `torneo-edge` del VPS (advertencia de devops en RF-C18).
  12. **Dependabot (RF-C16).** El digest de Postgres en `compose.dev.yml` y `compose.local.yml` necesita el ecosistema `docker-compose`, porque `docker` solo actualiza Dockerfiles. Para los dos Dockerfiles hay que usar `directories`.
  13. **Workflows.**
      - `ci-ok` también necesita `timeout-minutes`, porque si no lo tiene `check-workflows.mjs` falla. Propongo 5.
      - La regla "aparece `coolify`" debe ser sin distinción de mayúsculas, y el mensaje de fallo de `DEPLOY_MODE=api` no debe contener esa palabra, o `policy` se rompe sola.
      - `policy` necesita `just` y Node con un parser de YAML. Hay que instalar solo lo mínimo para no comerse el objetivo de ≤ 2 min en PRs de docs.
  14. **Coste del job `rust`.** La compilación musl en perfil `dev` (RF-C9) duplica la compilación y el tamaño de la caché, lo que deja justo el p50 de ≤ 8 min. Es lo que pide la verificación 8 y lo acepto, pero hay que medirlo en el PR de C.
  15. **`pre-push`.** Hay que documentar en `CLAUDE.md` que el hook revisa el árbol de trabajo y no el commit que se empuja, que necesita Docker encendido desde la fase B, y que `--no-verify` es la salida excepcional. Un worktree de una rama sin `scripts/git-hooks/` empuja sin chequeo; es aceptable.

### architect — 2026-10-21 — RESPUESTA
- Forma: `status: in-review`. No toqué `approvals` ni las revisiones anteriores. Todos los cambios están en el cuerpo; esta lista solo los mapea.

**product-owner (notas)**
1. **ADR-0012 en Linear** → "Áreas de preocupación" 3: abro su issue al aprobarse la spec, y tiene que estar aprobado antes de que 002 amplíe la CSP. Amplié su alcance con HSTS y el saneador de `web`.
2. **Intent si salen C5 y C6** → "Preguntas abiertas" 1 y RF-C19, variante "no": el product-owner actualiza el intent y el architect lo vuelve a aprobar antes de cerrar la fase C. Quité la salida de "empujar el tag con `production` sin proteger".
3. **Partir la fase C** → "Resumen": C1 (RF-C1 a RF-C7) y C2 (RF-C8 a RF-C18), a decisión del plan; C2 depende de C1 y el cierre va después del último PR.

**security-reviewer (Important)**
1. **`/ws` y primer despliegue** → RF-B13: ping cada 30 s, cierre `ws.pong_timeout` sin respuesta en 60 s, e inactividad de 120 s sin mensajes aceptados (`ws.idle_timeout`); los plazos son configuración, no variables de entorno. Códigos nuevos en RF-B5. Tests de integración: el cliente que no contesta pings se cierra, y el que solo contesta pings también. El bloqueo del primer despliegue hasta tener los límites anónimos de D6 y D7 está en el "Resumen", en RF-C18 (prerrequisitos en `docs/architecture.md`), en RF-C19 (issue bloqueado por auth) y en "Áreas de preocupación" 1.
2. **Guard de `/dev/**`** → RF-A16: en `handle`, 404 sin llamar a `resolve`, después de Paraglide; el layout queda como segunda capa. Hay un e2e contra una instancia sin la variable que no escribe ninguna línea (RF-C10 con dos instancias, Estrategia de pruebas) y un unit del guard. Anotado como extensión de ADR-0009 D5 en "Áreas de preocupación" 5. También corregí la referencia a `/dev/status`, que es RF-B7 y no RF-B5.
3. **`.dockerignore`** → RF-C3: `**/.env`, `**/.env.*`, `!**/.env.example`, `**/node_modules` y `**/target`. `check-compose.mjs` exige las formas con `**/`. `scripts/check-dockerignore.sh` construye un `FROM scratch` con `--output type=local` y comprueba que `infra/.env` y `apps/server/.env` quedan fuera (job `images`). Además, RF-C5 deja de invitar a crear `infra/.env` (ver devops 2).

**security-reviewer (Minor)**
4. **Solo lectura** → RF-B12: capa de 405 `http.method_not_allowed` para todo método distinto de `GET`, `HEAD` y `OPTIONS`, con test de `POST /api/health`. En RF-A10, la protección CSRF de SvelteKit no se desactiva. "Áreas de preocupación" 2 actualizada.
5. **Eco y 4 KB** → RF-B13: con `PUBLIC_ORIGIN` `https://` el eco queda apagado, con `warn` `ws.echo_refused`, y si se enciende registra `warn` `ws.echo_enabled`. Elegí no salir con código 1 para que una variable suelta en Coolify no tumbe producción (en Alternativas). Los 4 KB se fijan con `max_message_size` y `max_frame_size`.
6. **Logs de `web`** → RF-A19: `process.exit(1)` tras `uncaughtException`. "Diseño → Logs": la duración mayor de los logs de `web` y la propuesta del saneador para ADR-0012. RF-B20: el aviso lo aclara.
7. **Health** → RF-B10: caché de 1 s con una sola consulta en vuelo y un test de 50 peticiones concurrentes.
8. **Cabeceras** → HSTS `max-age=31536000`, sin `includeSubDomains` ni `preload`, en `web` (RF-A10) y en `server` (RF-B12). El test de redirección abierta está en RF-A3 y en el e2e.
9. **Cadena de suministro** → RF-C1 (`--locked --version`, todas las etapas `FROM` por digest), RF-C2, RF-A1 (`packageManager` con hash y `minimumReleaseAge: 1440`), RF-C16 (`cooldown` de 3 días), RF-C12 (prohibida la fusión automática) y RF-C19 (alertas de Dependabot). "Áreas de preocupación" 11 actualizada.
10. **`check-workflows.mjs`** → RF-C12: cualquier sintaxis del contexto `secrets` en cualquier workflow, `.github/actions/**`, raíz `contents: read`, `persist-credentials: false` y nada de `${{ }}` con datos controlables en `run:`. RF-C13: el tag se valida por `env:` en los dos disparadores.

**devops-engineer (bloqueantes)**
1. **Postgres en CI** → opción (a). RF-C9 sin `services:`, con `just db-up`; RF-B19 dice que es el mismo camino y fija el `DATABASE_URL` por defecto. La (b) va en Alternativas.
2. **Interpolación** → RF-C7: la receta exporta `IMAGE_TAG` y usa `--env-file infra/local.env`; `compose.local.yml` ya no fija variables interpoladas. RF-C5 cambia `infra/.env.example` por `infra/local.env`, commiteado y sin secretos. RF-C4 usa `DATABASE_URL=${DATABASE_URL:?}`. La tabla de variables está actualizada.
3. **Filtros** → RF-C8: `rust` + `deny.toml`; `web` + `scripts/**`, `.npmrc` y la configuración de Prettier y ESLint de la raíz; `images` + los scripts de compose y de contexto. Los tests de `scripts/` corren en `web` (RF-C10). Anotado como extensión de D7 en "Áreas de preocupación" 5.
4. **Plazo de logs** → opción (a) en "Diseño → Logs y plazo": B calcula con la línea real de `cargo run` y C lo confirma con 1 h de `prod-local` y ajusta el aviso. `logging.driver: json-file` explícito en RF-C4 y verificado por `check-compose.mjs`.
5. **Variantes de RF-C19** → reescrito con una parte fija (issue del primer despliegue) y una variante por pregunta. El tag `v0.0.1` solo aparece en la variante "sí" de GitHub. Con "no", no hay tag, C6 se verifica de forma estática y C5 pasa al primer despliegue con el intent actualizado. La variante VPS "parte de 001" se limita a la parte de host (13 primera mitad, 15b parcial, 4 parcial y limpieza de Docker), y el resto pasa al issue del primer despliegue. Las dos variantes de cada pregunta siguen abiertas para el owner. Cubren sus inclinaciones: si un agente hace la configuración con `gh`, la visibilidad de GHCR queda en la UI y el token del owner no se registra; si un agente tiene SSH, su alcance y su evidencia quedan acotados.

**devops-engineer (no bloqueantes)**
6. **Caché del release** → RF-C13 solo con `cache-from`; el filtro `images` agrega `Cargo.lock`, `rust-toolchain.toml` y `pnpm-lock.yaml` (RF-C8).
7. **Dockerfile del server** → RF-C1: un checksum de `zig` por `BUILDARCH`, `cargo chef cook --zigbuild --release --target …`, `ARG APP_VERSION` después del `cook`, y `rust:` igual a `rust-toolchain.toml`, comprobado por `check-compose.mjs` (RF-C6, RF-C16).
8. **Binario estático** → `readelf -l` sin `INTERP` en RF-C1 y RF-C9; anotado como extensión en "Áreas de preocupación" 5.
9. **Smoke test de `web`** → RF-C2 y RF-C11: `docker run` y 200 en `/healthz` y `/en/`.
10. **Compose** → `tmpfs: /tmp:size=16m` (RF-C4). El `docker inspect` de `ReadonlyRootfs`, `CapDrop`, `SecurityOpt` y `LogConfig` va en los prerrequisitos del primer despliegue (RF-C18). "Solo `web` y `server`" aplica solo a `docker-compose.yml` (RF-C6).
11. **Redes locales** → RF-C7: `--label torneo.local=1`; `prod-local-down` solo borra las que tienen la etiqueta, y una red existente sin etiqueta se usa sin modificarla.
12. **Dependabot** → RF-C16: ecosistema `docker-compose` y `directories` para los Dockerfiles.
13. **Workflows** → `ci-ok` con 5 min (RF-C8); `coolify` sin distinguir mayúsculas y fuera del mensaje de fallo de `api` (RF-C12); `policy` instala solo Node, el parser de YAML y `just` (RF-C12).
14. **Coste de `rust`** → RF-C9: el PR de C mide la duración con caché caliente; también va en la evidencia manual.
15. **`pre-push`** → RF-C18: `CLAUDE.md` documenta que revisa el árbol de trabajo, que necesita Docker y que existe `--no-verify`.

**Descartado:** nada de las tres revisiones. Lo único que ajusté fue el Minor 5 de security: el server deja el eco apagado con un `warn` en vez de negarse a arrancar.

### security-reviewer — 2026-10-21 — APPROVE (ronda 2)
**🤖 security-reviewer**

- Verifiqué: leí el cuerpo de la spec, no solo el mapa de la respuesta del architect. Revisé los Important 1–3 en cada lugar donde la respuesta dice que están resueltos. También revisé que los Minor 4–10 no abran un hueco nuevo, y miré con más cuidado el eco con `https://`.
  1. **`/ws` y el primer despliegue**: resuelto.
     - RF-B13 trae lo que pedí: un ping cada 30 s; cierre `ws.pong_timeout` si no llega ningún frame en 60 s; corte de la conexión TCP si el cierre no termina en 5 s; e inactividad de 120 s medida en mensajes de datos aceptados. Así, un socket que solo contesta pings también se cierra. Los plazos no son variables de entorno, así que nadie los alarga desde Coolify.
     - Con el eco apagado, ningún socket de producción vive más de 120 s.
     - Los códigos nuevos están en RF-B5. La "Estrategia de pruebas" cubre los dos casos: un cliente que no lee el socket y uno que solo contesta pings.
     - El bloqueo del primer despliegue ahora es una condición escrita y no un "riesgo residual". Está en el Resumen, en RF-C18 (prerrequisito 1, con `TRUSTED_PROXIES=10.250.0.2/32`), en RF-C19 (issue bloqueado por auth) y en "Áreas de preocupación" 1. El escenario de las 2000 conexiones desde un solo host queda acotado en 001 y no llega a producción sin los límites por IP.
  2. **Guard de `/dev/**`**: resuelto.
     - RF-A16 lo pone en `handle`, después de Paraglide. Compara `route.id`, no `url.pathname`, y responde 404 sin llamar a `resolve`. Por eso no corre ningún `load`, `+server.ts` ni form action, y tampoco `handleError`.
     - El layout queda como segunda capa.
     - Las pruebas cubren lo necesario: un unit con `/dev`, `/dev/…`, `/developers` y `null`, y un e2e contra una segunda instancia sin la variable (RF-C10). Ese e2e pide 404 en `components`, `status` y `error-probe`, y que stdout y stderr no reciban ninguna línea. Es la prueba que faltaba: el test 6 solo miraba el 404.
     - Está anotado como extensión de ADR-0009 D5.
  3. **`.dockerignore`**: resuelto.
     - RF-C3 usa `**/.env`, `**/.env.*`, `!**/.env.example`, `**/target` y `**/node_modules`. `check-compose.mjs` exige las formas con `**/`.
     - `check-dockerignore.sh` construye un `FROM scratch` con `COPY . /ctx` y `--output type=local`. Comprueba que `infra/.env`, `apps/server/.env`, `private` y `.git` quedan fuera, y que `.env.example` sí entra, lo que también prueba que el orden de la negación es correcto.
     - Se cerró el origen del escenario: RF-C5 y RF-C7 cambian `infra/.env.example` por `infra/local.env`, commiteado y sin secretos, y el `--env-file` es explícito.
  - **Eco con `https://` (Minor 5)**: no es Important.
    - Lo que pedí era que el server se negara a **encender** el eco, no que se negara a arrancar. Dejarlo apagado con `warn` `ws.echo_refused` falla cerrado, igual que si la variable no estuviera.
    - El caso tiene unit e integración.
    - El impacto residual también es bajo. Para que el eco se encienda en producción, alguien tendría que poner `ENABLE_WS_ECHO=1` **y además** un `PUBLIC_ORIGIN` que no sea `https://` en la UI de Coolify. Con eso, el handshake exigiría un `Origin` `http://`, que ningún navegador manda desde el sitio `https`. El WS real dejaría de funcionar para todos, así que se notaría enseguida, y el eco solo refleja al mismo socket.
  - **Resto de los Minor**: no introducen nada Important.
    - La capa de 405 deja pasar `GET`, `HEAD` y `OPTIONS`, así que el upgrade de `/ws` sigue funcionando.
    - HSTS va sin `includeSubDomains`.
    - La caché de `/api/health`, de 1 s y con una sola consulta en vuelo, no expone nada.
    - `process.exit(1)` deja el reinicio a Docker.
    - `check-workflows.mjs` cubre las cuatro sintaxis de `secrets`, `.github/actions/**`, `persist-credentials: false` y la prohibición de `${{ }}` con datos controlables. En RF-C13, el tag se valida por `env:` en los dos disparadores.
    - En RF-C19, el token del owner que usa un agente con `gh` no se registra, y el agente con SSH tiene alcance y evidencia acotados.

**Sugerencias (Minor, no bloquean; pueden ir en el plan de la fase B):**
- **Eco**: en vez de una lista negra por prefijo (`empieza con https://`), encenderlo solo si `PUBLIC_ORIGIN`, ya parseado, es `http` con host `localhost` o `127.0.0.1`. Esto cierra variantes como `HTTPS://…` o un host público con `http`, sin depender de que el `Origin` exacto rompa el sitio.
- **Frames de control**: aclarar en RF-B13 si los pings que manda el cliente cuentan para el límite de 10 mensajes por 10 s. El tamaño de un frame de control ya está acotado, pero hoy un cliente puede mandar pings sin límite y el server contesta cada uno.

### devops-engineer — 2026-10-21 — APPROVE (ronda 2)
- Verifiqué: leí el cuerpo completo de la spec y no solo la RESPUESTA del architect. Revisé los bloqueantes 1–5 en RF-B19, RF-C4, RF-C5, RF-C7, RF-C8, RF-C9, RF-C19, "Diseño → Logs y plazo", "Áreas de preocupación" 5 y Alternativas. Revisé los no bloqueantes 6–15 en RF-A1, RF-B10, RF-B12, RF-C1, RF-C2, RF-C3, RF-C6, RF-C11, RF-C12, RF-C13, RF-C16 y RF-C18. Lo contrasté con ADR-0006 D3, D5, D7, D8, D10 y sus verificaciones 4, 8, 13 y 15, y con el `.gitignore` actual. No ejecuté builds. No edité el archivo.
- **Bloqueantes: los cinco están resueltos y son correctos.**
  1. **Postgres en CI.** RF-C9 no usa `services:` y RF-B19 usa el mismo `just db-up`, con un `DATABASE_URL` por defecto. Hay un solo camino y Dependabot ve el digest.
  2. **Interpolación.** RF-C7 exporta `IMAGE_TAG` y pasa `--env-file infra/local.env`. Con `--env-file` explícito, Compose no carga `infra/.env`. `compose.local.yml` ya no fija variables que se interpolan. `infra/local.env` no choca con el `.gitignore`, porque `.env` y `.env.*` no lo cubren, y solo lleva credenciales de desarrollo. RF-C4 usa `${DATABASE_URL:?}`. La tabla de variables está actualizada.
  3. **Filtros.** `rust` agrega `deny.toml` y `packages/protocol-ts/**`. `web` agrega `scripts/**`, `.npmrc` y la configuración de Prettier y ESLint. `images` agrega los scripts, `Cargo.lock`, `rust-toolchain.toml` y `pnpm-lock.yaml`. Todo está anotado como extensión de D7.
  4. **Plazo de logs.** La fase B calcula y la fase C confirma, y la fase C ajusta el aviso si pasa de 21 días. `json-file` con 3 × 10 MB queda explícito en RF-C4 y lo exige `check-compose.mjs`. `LogConfig` está en los prerrequisitos de RF-C18.
  5. **RF-C19.** La parte fija y las variantes cierran. El tag `v0.0.1` solo existe en la variante "sí". En la variante "no", C6 se verifica de forma estática y C5 se mueve con una reaprobación del intent. La variante VPS se limita a la parte del host: 13 primera mitad, 15b parcial, 4 parcial y la limpieza de Docker de D10. Lo demás pasa al primer despliegue.
- **Los puntos que pediste revisar son técnicamente correctos:**
  - **`check-dockerignore.sh` con `FROM scratch`.** `COPY . /ctx` más `--output type=local` funciona con el driver `docker` por defecto y no descarga ninguna imagen base. Las rutas `ctx/...` de la salida son las correctas.
  - **`readelf -l` sin `INTERP`.** Es correcto para `static-pie`. También lo es si `zig` produce un estático no PIE, porque la comprobación no depende del tipo ELF. `readelf` viene de serie en ubuntu-latest.
  - **`cargo chef cook --zigbuild --release --target x86_64-unknown-linux-musl`.** Es correcto. Poner `ARG APP_VERSION` después del `cook` está bien.
  - **`minimumReleaseAge: 1440` y `cooldown` de 3 días.** La unidad (minutos) y el archivo (`pnpm-workspace.yaml`) son los correctos. `cooldown` de Dependabot existe como opción.
  - **405 con una capa en la raíz del router.** El diseño es correcto, y el test `DELETE /cualquier` → 405 detecta si la capa no cubre el fallback.
  - **Caché de 1 s y una sola consulta en vuelo en `/api/health`.** Es correcta y compatible con `Cache-Control: no-store` y con el healthcheck cada 15 s.
- **No bloqueantes. Son errores reales pero pequeños; el plan de la fase C debe resolverlos y los verifico en su revisión:**
  1. **`check-dockerignore.sh` puede borrar archivos del desarrollador (RF-C3).** Según el texto, el script crea `apps/server/.env` e `infra/.env` y los borra con `trap`. `apps/server/.env` es el `.env` de desarrollo de la tabla de variables, así que correr el script en local lo sobrescribe y lo borra. Corrección: crear solo los archivos que no existen y que el `trap` borre solo lo que creó el script. Si el archivo ya existe, se usa como sujeto de la prueba. Además, la salida va a `mktemp -d`, fuera del repo.
     - Hay un hueco más. BuildKit da prioridad a un `<Dockerfile>.dockerignore` junto al Dockerfile sobre el `.dockerignore` de la raíz. Un `apps/server/Dockerfile.dockerignore` anularía la raíz y la prueba con `fixtures/context.Dockerfile` no lo vería. `check-compose.mjs` debería fallar si existe `apps/*/Dockerfile.dockerignore`.
  2. **Versión de pnpm (RF-A1).** `minimumReleaseAge` necesita pnpm ≥ 10.16. Una versión anterior ignora la opción sin avisar. Hay que cambiar "≥ 10" por "≥ 10.16".
  3. **`cooldown` en Dependabot (RF-C16).** La sintaxis es `cooldown: { default-days: 3 }`. Solo aplica a las actualizaciones de versión, no a las de seguridad, y eso es lo que queremos. El plan debe confirmar que se acepta en los cinco ecosistemas, en especial `docker-compose` y `github-actions`. Si alguno no la admite, `dependabot.yml` no pasa la validación.
  4. **Filtro `rust` y Node en el job `rust`.** Es una consecuencia del bloqueante 1: ahora `rust` depende de `infra/compose.dev.yml`, así que ese archivo debe estar en el filtro `rust`. Si no, un bump del digest de Postgres no corre los tests de integración.
     - Además, `just gen` formatea con Prettier (RF-B6), así que el job `rust` necesita Node y `pnpm install` para el chequeo de frescura. Hay que decirlo en RF-C9.
     - Opcional: agregar `**/Cargo.toml` al filtro `images`. Un cambio de features sin cambio en `Cargo.lock` cambia la receta de `cargo-chef` y el release encontraría la caché fría.
  5. **El entorno del shell gana a `--env-file` (RF-C7).** Si el `justfile` exporta `DATABASE_URL` de forma global (RF-B19) o el desarrollador la tiene en su shell, Compose interpola `localhost:5432` y el `server` del contenedor nunca queda `healthy`. La receta debe correr Compose con `env -u DATABASE_URL -u PUBLIC_HOST`, o el `DATABASE_URL` por defecto se limita a las recetas de tests.
     - En RF-C11, el `compose config` sin `IMAGE_TAG` debe pasar `--env-file infra/local.env` y comprobar que el error nombra a `IMAGE_TAG`. Sin eso falla igual por `PUBLIC_HOST` y no prueba nada sobre `IMAGE_TAG`.
  6. **Test de "una sola consulta" (RF-B10).** Un proxy TCP cuenta conexiones, no consultas, y el pool reutiliza conexiones. El contador debe vivir en el server (un contador de prueba en el estado del health) o el proxy debe contar mensajes `Execute` y `Query` del protocolo de Postgres. El TTL debería ser configurable en los tests, como los plazos del WS, para evitar fallos intermitentes en el límite de 1 s.
  7. **405 (RF-B12).** RFC 9110 exige `Allow` en un 405: hay que poner `Allow: GET, HEAD, OPTIONS`. Además, un `OPTIONS` sobre `/api/health` lo responde el `MethodRouter` de axum con 405 y cuerpo vacío. Hay que definir el fallback de método para que todo 405 lleve `ApiError { error: "http.method_not_allowed" }`.
  8. **Dockerfile del server (RF-C1).**
     - Hay que copiar `rust-toolchain.toml` antes del `cook`. La receta de `cargo-chef` no lo incluye, y sin él rustup no instala el target musl ni fija el canal.
     - El paso `build` debe usar `cargo zigbuild` con el mismo target, perfil y features que el `cook`. Si no, la capa de dependencias no se reutiliza.
     - Hay que fijar una versión de `cargo-chef` que tenga `--zigbuild`.
  9. **Envoltorio de `json-file` (Diseño → Logs).** `json-file` escapa las comillas de la línea JSON (`"` pasa a `\"`), así que lo que se agrega por línea es más de ~80 B. La fase B debe medir la línea ya escapada. El error va en la dirección segura (menos días), así que "menos de 3 semanas" sigue siendo cierto.

### architect — 2026-10-22 — ENMIENDA (SvelteKit 3)
- Cambié lo que resume "Enmienda 2026-10-22: SvelteKit 3": RF-A1, RF-A2, RF-A10, RF-A14, RF-A16, RF-A19, RF-B18, RF-C2, RF-C4, RF-C6 y RF-C10, la tabla de variables, "Componentes afectados", "Diseño → UI", "Áreas de preocupación" 2 y 12, "Alternativas consideradas" y "Estrategia de pruebas". Agregué RF-A23. No toqué las aprobaciones ni las revisiones anteriores.
- `status: in-review`. Pido revisión de la enmienda a security-reviewer y frontend-engineer.

### security-reviewer — 2026-10-22 — APPROVE (enmienda Kit 3)
**🤖 security-reviewer**

- Verifiqué: el `git diff` de los tres documentos contra ADR-0008 D4 y D5. También leí el código de `@sveltejs/kit@3.1.0` y `@sveltejs/adapter-node@6.0.0`:
  - `csrf.js` y `respond.js`: el chequeo de Kit corre antes de `handle`. Solo cubre `POST`/`PUT`/`PATCH`/`DELETE` con `Content-Type` de formulario o sin él. Compara contra `paths.origin || url.origin` y se apaga solo con `trustedOrigins` que incluya `'*'`.
  - `config/options.js`: `checkOrigin` es una opción `removed`, así que su presencia rompe la configuración.
  - `errors.js`: los cuatro valores de `kind`, el `merge` con el *fallback* y que, si el hook lanza o rechaza, Kit escribe el error original con `console.error`.
  - `instance.js`: el `handleError` por defecto.
  - `core/env.js` y `exports/internal/env.js`: la validación de `defineEnvVars` al arrancar, `set_building()` en el build y que `public` vale `false` por defecto, sin semántica de prefijo `PUBLIC_`.
  - `vite/utils.js`: `/[/.]server\.[^/]+$/` hace que `src/lib/paraglide/server.js` sea solo de servidor, y `$app/env/private` está protegido por `guard.js`.
  - `handler.js` de adapter-node: el origen es `https://` + `Host` cuando no hay `*_HEADER`. Sin `PROTOCOL_HEADER` el texto del error es fijo. `getRequest` responde 400 sin log. `ADDRESS_HEADER` y `XFF_DEPTH` no cambian.
  - `static.js`: solo atiende `GET` y `HEAD`, el resto pasa por `handle`.
  - `prerender_template_nonce`: existe.
  - **Conclusión:** RF-A23 aplica la regla de D4 a peticiones del navegador. Pide `Origin` exacto a `PUBLIC_ORIGIN`, o `Sec-Fetch-Site: same-origin` si falta. Rechaza `Origin: null`, cualquier `Content-Type` y todo método fuera de una lista permitida, sin `resolve` y sin logs. Nada que Kit atienda antes de `handle` deja pasar una mutación.
  - `paths.origin` queda prohibido y verificado por un chequeo.
  - El riesgo de formularios sobre `http` está bien acotado: en 001 no hay actions, y `PROTOCOL_HEADER` solo en e2e es aceptable porque RF-C6 lo prohíbe en el compose.

- Hallazgos (Minor, no bloquean):
  1. **`Host` y `event.url.origin` (RF-C4).** La frase "ningún header que el cliente pueda falsificar cambia el origen" solo es cierta detrás de Traefik. Quien llega directo al contenedor elige el `Host`, y `event.url.origin` lo sigue. Eso pasa en `prod-local` (`127.0.0.1:3000`) o desde cualquier contenedor de `torneo-edge`.
     - *Escenario:* una feature futura arma una URL absoluta con `event.url.origin`, por ejemplo un enlace `hreflang` o canónico, el `next` del login o un correo. Una petición con `Host: evil.example` genera enlaces al dominio del atacante.
     - Hoy no hay impacto: el guard usa `PUBLIC_ORIGIN` y no hay caché compartida.
     - *Cambio:* agregar en "Seguridad" de la spec la regla "`web` nunca construye URLs absolutas con `event.url.origin`, `url.host` ni `url.href`; usa `PUBLIC_ORIGIN`". También precisar la frase de RF-C4 con "detrás de Traefik".
  2. **`handleError` síncrono (RF-A19).** En Kit 3.1.0, un `handleError` que devuelve una `Promise` durante el render, sin `experimental.async`, escribe un `console.warn` y descarta el resultado. Si la promesa rechaza, Kit escribe el error crudo con `console.error`.
     - *Escenario:* alguien lo declara `async` y el e2e "sin líneas en stdout" falla, o un rechazo vuelca a stderr la URL con query y la IP de canario sin pasar por el saneador.
     - *Cambio:* RF-A19 exige que `handleError` sea síncrono, sin `async` y sin devolver promesas, y el test unitario lo verifica.
  3. **Mensajes de los validadores de entorno.** Kit imprime al arrancar los mensajes de los validadores de `defineEnvVars` tal cual (`env_invalid`).
     - *Escenario:* cuando `web` reciba `X-Internal-Secret` o `IP_HASH_KEY` (ADR-0008 D5), un validador que lance algo como `` `invalid value: ${v}` `` deja el secreto en el `json-file` de Docker.
     - *Cambio:* aplicar en `src/env.ts` la regla de RF-B8: el mensaje lleva la regla, nunca el valor. El test unitario del validador de `PUBLIC_ORIGIN` comprueba que el mensaje no contiene el valor de entrada.
  4. **Un e2e de RF-A23 no prueba el guard.** `POST /es/` sin `Origin` ni `Sec-Fetch-Site`, si va sin `Content-Type`, lo rechaza antes el chequeo de Kit con el texto "Cross-site POST form submissions are forbidden". El test pasaría aunque el guard no existiera.
     - *Cambio:* enviar `Content-Type: application/json` y verificar que el cuerpo es `{"error":"auth.csrf_origin"}`. Lo mismo para el caso con `Origin` ajeno.
  5. **Precisión en la enmienda de ADR-0008.** Dice que el chequeo de Kit "está siempre activo", pero Kit lo omite en `vite dev` (`!__SVELTEKIT_DEV__`).
     - *Cambio:* decir "en el build (no en `vite dev`)" y aclarar que el guard de RF-A23 sí corre en dev. La spec ya lo dice en "Áreas de preocupación" 12.
  6. **Frontmatter de los ADRs (proceso).** ADR-0009 agrega `security-reviewer` a `reviewers` pero sigue en `approved` sin su aprobación, y ADR-0008 tampoco la lleva para la enmienda. Hay que registrar esta aprobación en `approvals` de los dos ADRs y en la spec al cerrar la ronda. Le toca a quien edite, no a mí.

### frontend-engineer — 2026-10-22 — APPROVE (enmienda Kit 3)

**Verifiqué:**
- Leí el `git diff` de `spec.md`, ADR-0008 y ADR-0009 y lo comparé con `plan-a.md`.
- Revisé el registro de npm hoy. `@sveltejs/kit@3.1.0` (2026-10-14) pide Vite `^8.0.12`, Svelte `^5.57.1`, TS `^6.0.0` y `vite-plugin-svelte` `^7.0.0`, con `engines.node >=22.17`. `adapter-node@6.0.0` (2026-10-01) pide `kit ^3.0.0-next.0`. `typescript-eslint@8.72.0` pide TS `<6.1.0`. `svelte-check@4.7.7` acepta TS `^6`. `paraglide-js@2.29.1` no fija versión de Kit. `bits-ui@2.20.0` no depende de Kit (sus peers son Svelte `^5.33.0` e `@internationalized/date`).
- Leí el código de Kit 3.1.0 y adapter-node 6.0.0 que security-reviewer ya había bajado al scratchpad. No descargué nada.
  - `errors.js`: confirma los cuatro `kind`, el `merge` con el *fallback* y el aviso cuando `handleError` devuelve una `Promise`.
  - `ambient.d.ts`: confirma que `App.Error` ya declara `status: number` y `message: string`.
  - `cli.js`: confirma que `svelte-kit sync` lee `vite.config.ts`.
  - `write_tsconfig`: confirma `$app/tsconfig` y `#lib/*`.
  - adapter-node 6 sigue escribiendo `Listening on http://…`, así que la lista blanca de logs del plan sigue valiendo.
- Paraglide 2.29.1: `runtime.js` no importa `server.js`. Con eso, la regla de Kit 3 de que todo archivo con segmento `server` es solo de servidor solo afecta a `paraglideMiddleware`, que se importa desde los hooks.
- Leí la documentación de entorno de Kit 3: `defineEnvVars`, `building` y que no hay semántica de prefijo `PUBLIC_`. Por eso `PUBLIC_ORIGIN` puede ser privada sin conflicto.
- Corrí ESLint 9 con typescript-eslint 8.72.0 sobre la prueba de patrones `#lib`/`$lib` que ya estaba en el scratchpad.

**Hallazgos (todos Minor; no bloquean la spec y los resolví en el plan):**
1. **Un patrón `#lib/…` en `group` nunca coincide.** `no-restricted-imports` lee los `group` con sintaxis de `.gitignore`, donde una línea que empieza con `#` es un comentario. Con el patrón `#lib/features/**`, el import `#lib/features/system/health.js` no lo atrapó ese patrón sino su gemelo relativo `**/features/**`. Sí funcionan `\#lib/…` y `regex: '^#lib/…'`. La enmienda D4 de ADR-0009 dice "cada patrón con `#lib/…`"; sigue siendo correcta, pero la implementación tiene que escribirlos como `regex` y el test 2 necesita un caso que solo atrape el patrón `#lib`.
2. **`App.Error` en Kit 3 ya declara `status` y `message: string`.** El `app.d.ts` del plan anterior (`message: 'not_found' | 'internal'`) no compilaría. La lista cerrada de RF-A19 se cumple con el tipo de retorno de `handleError`, no ampliando `App.Error`.
3. **Efecto lateral en la prueba de RF-A16.** Sin `paths.origin`, sobre `http` Kit rechaza antes de `handle` un `POST` de formulario del mismo origen. El e2e de A16 del plan ("`POST` del mismo origen para pasar el CSRF de Kit") ya no llegaría al guard de `/dev/**`. Pasa a `Content-Type: application/json` con `Origin` igual a `PUBLIC_ORIGIN`. Es el mismo fenómeno que ya registra "Áreas de preocupación" 12.
4. **`init` de shadcn y la decisión 13 se contradicen.** "Áreas de preocupación" 12 pide `init` de shadcn-svelte en la prueba corta, y la decisión 13 del plan evita `init` porque escribe `components.json` y un `utils.ts` que trae el paquete `cn`. Lo resolví corriendo `init` en una copia desechable solo para ver qué alias propone, sin commitear su salida.
5. El resto de lo pedido es viable tal como está escrito:
   - `csp` en `vite.config.ts` sin `svelte.config.js`.
   - `components.json` con `#lib`.
   - `src/env.ts` con `$app/env/private`.
   - `handleError` por `kind`.
   - `server.js` de Paraglide solo en los hooks.
   - Node ≥ 22.17 y TS 6.0.x.
   - bits-ui fijado en 2.20.0: la 2.20.1 salió el 2026-10-22 a las 19:01 UTC y no pasa `minimumReleaseAge`.

### architect — 2026-10-23 — ENMIENDA (menor)
- Cambié lo que resume "Enmienda 2026-10-23: menor": "Requisitos no funcionales → Seguridad", RF-C4, RF-C6, RF-C7 y RF-C18. Agregué "Áreas de preocupación" 13. No toqué las aprobaciones ni las revisiones anteriores.
- `status: in-review`. Pido revisión de la enmienda a **security-reviewer** y **devops-engineer**. A devops-engineer le pido además que confirme dos cosas: que el middleware `headers` de Traefik reemplaza las cabeceras que ya trae la respuesta y no las duplica, y si prefiere sumar Traefik a `compose.local.yml` para verificar en vivo antes de producción.

### security-reviewer — 2026-10-23 — APPROVE (enmienda menor)

**🤖 security-reviewer**

Verifiqué: el diff completo de `docs/features/001-fundaciones/spec.md` (la tabla de la enmienda, "Seguridad", RF-C4, RF-C6, RF-C7, RF-C18 y "Áreas de preocupación" 13), los valores de RF-A10 y RF-B12, los routers de ADR-0006 D2 y los selectores `absoluteUrlSelectors` de `apps/web/eslint.config.js`.

**Sin hallazgos Important.** Las tres partes cierran lo que se pidió y no abren superficie nueva.

1. **Regla de URLs absolutas.** Corrige el hallazgo 1 de mi revisión anterior. Antes la spec decía que "ningún header falsificable cambia el origen", sin matices. Ahora esa garantía vale solo detrás de Traefik, y la defensa no depende de la topología: el código nunca arma URLs absolutas con la petición. Escenario cubierto: alguien desde otro contenedor de `torneo-edge` (o desde `prod-local`) manda `Host: evil.example`, y un enlace absoluto, un `Location` o una URL canónica apuntaría a su dominio. ESLint ya aplica la regla en `just check`. El guard de RF-A23 compara contra `PUBLIC_ORIGIN` y no contra `event.url`, así que el `Host` falsificado tampoco lo evade.
2. **"Detrás de Traefik".** La redacción es correcta y no debilita RF-A23 ni el chequeo CSRF de Kit. Sin Traefik, el atacante controla `Host` y además `Origin`, pero ahí no hay víctima con navegador ni cookie que explotar.
3. **Cabeceras en Traefik.** Los valores son los mismos de RF-A10: `nosniff`, `strict-origin-when-cross-origin` y HSTS de 1 año sin `includeSubDomains` ni `preload`. RF-C6 los fija de forma estática y exige que `stsIncludeSubdomains` y `stsPreload` no valgan `true`, lo que evita un HSTS que afecte a otros subdominios del owner. No poner el middleware en `torneo-web-http` es correcto, porque los navegadores ignoran HSTS sobre `http`. `torneo-api` no lo necesita, porque RF-B12 ya pone `nosniff` y HSTS desde axum. Acepto el riesgo (b) del área 13 (la única prueba en vivo es la del primer despliegue), porque ese despliegue ya está bloqueado por la lista de RF-C18.

**Nits (no bloquean):**
- **N1. La regla no nombra los headers.** La regla de "Seguridad" nombra `event.url.origin`, `url.host`, `url.href`, `request.url` y `new URL(x, base)`, pero no `request.headers.get('host')` ni `X-Forwarded-Host`, que es la misma fuente. ESLint tampoco los detecta. Escenario: alguien arma un `Location` con `` `https://${request.headers.get('host')}/...` `` y pasa el lint. Sugiero agregar a la regla "ni con los headers `Host` o `X-Forwarded-*`" y un selector `CallExpression[callee.property.name='get'][arguments.0.value=/^(host|x-forwarded-host)$/i]` cuando se toque `eslint.config.js`.
- **N2. HSTS solo sobre TLS.** El middleware `headers` de Traefik solo emite `Strict-Transport-Security` cuando la petición llegó por TLS. Con el entrypoint `https` eso se cumple. Conviene que el `curl -I` de RF-C18 se haga contra `https://` y no contra el puerto interno, como ya está escrito. Se lo dejo a devops-engineer junto con su pregunta sobre si Traefik duplica o reemplaza las cabeceras.

### devops-engineer — 2026-10-23 — APPROVE (enmienda menor)

Verifiqué:
- `git diff` de la spec: la sección de la enmienda, RF-C4, RF-C6, RF-C7, RF-C18, "Seguridad" y "Áreas de preocupación" 13.
- `apps/web/src/lib/server/hooks/security-headers.ts`: pone tres cabeceras con `headers.set`, con los mismos valores que RF-C4.
- D2 de ADR-0006.

El comportamiento de Traefik lo saco de lo que conozco del middleware `headers` de Traefik v3.

1. **Traefik reemplaza, no duplica.** `stsSeconds`, `contentTypeNosniff` y `referrerPolicy` (la capa "secure") y `customResponseHeaders` escriben con `Header.Set`, y Go normaliza el nombre de la cabecera. Cada cabecera sale una sola vez. El riesgo real no es duplicar sino que los valores de `handle` y de Traefik se separen sin que se note, porque Traefik pisa el suyo.
   - *Sugerencia para el plan de C:* que `check-compose.mjs` compare las etiquetas con `SECURITY_HEADERS` de `security-headers.ts`, para tener una sola fuente de verdad.
   - *Sugerencia para RF-C6:* que falle si aparece `forceSTSHeader`.
   - *Corrección de redacción para 13(a):* lo que puede pasar es una divergencia que no se ve, no una cabecera duplicada.
2. **No hace falta Traefik en `compose.local.yml`.** Exigiría TLS local, un `certresolver` falso y sobrescribir etiquetas, lo que rompe la regla de "solo agrega". RF-C6, con el chequeo estático en `just check` y en `images`, y RF-C18, con la prueba real contra el proxy de Coolify antes del primer despliegue, cubren el riesgo.

Sin errores que pidan cambios.

### architect — 2026-10-23 — ENMIENDA (b): fase B
- Cambié lo que resume "Enmienda 2026-10-23 (b): fase B": RF-B4, RF-B5, RF-B7, RF-B11, RF-B13, RF-C6, la tabla de variables de entorno, la de "API / mensajes WS" y "Estrategia de pruebas". No toqué las aprobaciones ni las revisiones anteriores.
- `status: in-review`. Pido revisión de la enmienda a **security-reviewer**.

### security-reviewer — 2026-10-23 — APPROVE (enmienda b)

**🤖 security-reviewer**

**Verifiqué:** el bloque de la enmienda (b) en `git diff`:
- `ErrorCode` sin etiquetar: cada enum tiene un solo prefijo y hay un test de no solapamiento que no compila si falta una variante.
- `ws.rate_limited`.
- `HOST` como `IpAddr`: el compose no tiene `ports:`, y si el valor está mal falla cerrado.
- El orden `Origin` → upgrade → semáforo: no corre nada caro antes del semáforo.
- `GET /ws` sin upgrade: el cuerpo va vacío, lleva `nosniff` y HSTS y se registra en el log sin `Origin`.

**Hallazgos:** ninguno Important. Hay tres nits:
1. **El semáforo, con `try_acquire_owned` antes de responder 101.** El permiso se pasa al closure de `on_upgrade`. Con `acquire().await` las peticiones se encolan sin límite, y eso permite un DoS por memoria.
2. **El 4xx del rechazo.** Hay que decir "el 4xx que dé axum", porque `HEAD /ws` responde 405, y el test debe aceptar cualquier 4xx.
3. **El nombre `HOST`.** Puede llegar exportado con el nombre de la máquina. En ese caso falla cerrado, pero conviene considerar `BIND_ADDR`, a criterio del architect.

### security-reviewer — 2026-10-23 — APPROVE (BIND_ADDR)

**🤖 security-reviewer**

**Verifiqué:** el diff de la spec en RF-B11, RF-C6, la tabla de variables y la nota de la enmienda (b).
- Solo cambia el nombre. El tipo, el valor por defecto, la condición del healthcheck y la prohibición en el compose siguen igual.
- El riesgo baja: un `HOST` que exporte el shell ya no tumba el arranque.
- Un `HOST` que quede suelto en `server` no tiene efecto.
- `check-compose.mjs` (fase C) debe prohibir `BIND_ADDR` en `server` y tener un fixture que lo pruebe.

### architect — 2026-10-24 — ENMIENDA: distroless debian13
- Cambié RF-C1 y RF-C2 (`-debian12` → `-debian13`) y agregué la sección "Enmienda 2026-10-24: distroless debian13". No toqué las aprobaciones ni las revisiones anteriores.
- `status: in-review`. Pido revisión de la enmienda a **security-reviewer** y el OK del **owner**.

### security-reviewer — 2026-10-24 — APPROVE (enmienda debian13)

**🤖 security-reviewer**

**Verifiqué:** el diff de RF-C1, RF-C2, la sección "Enmienda 2026-10-24: distroless debian13" y la nota de ADR-0006.
- Solo cambia la versión de Debian de las bases finales. Siguen siendo `:nonroot` (UID 65532), sin shell, fijadas por digest de índice y con la antigüedad mínima de 3 días.
- La ruta del healthcheck no cambia: `nodejs22-debian13` usa `/nodejs/bin/node` como entrypoint (plan-c, decisión 2), y el healthcheck del server es `/server healthcheck`, en forma exec. No necesita shell.
- El server es estático (musl, D8). Los builders `-bookworm` no llegan a la imagen final.
- El riesgo baja: con `-debian12` sin parches, un CVE de la base (por ejemplo, en OpenSSL o en la glibc de `nodejs22`) quedaba abierto en producción sin que Dependabot lo arreglara.
- La nota de ADR-0006 no cambia el `status` ni las approvals. El cuerpo de D5 sigue diciendo `-debian12` hasta el OK del owner. Es correcto.
- **Sugerencia (no bloquea, ya existía):** como el digest manda sobre la etiqueta, un digest de la variante `latest` (root) escrito como `:nonroot@sha256:…` pasaría sin error. El job `images` debería comprobar `docker inspect -f '{{.Config.User}}'` = `65532` (o `nonroot`) en las dos imágenes finales.
