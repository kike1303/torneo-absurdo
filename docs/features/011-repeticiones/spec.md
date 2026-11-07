---
id: "011"
title: Repeticiones y verificación en el navegador
type: spec
status: approved
risk: high
author: architect
reviewers: [product-owner, security-reviewer]
approvals:
  - by: product-owner
    date: 2026-11-02
  - by: security-reviewer
    date: 2026-11-02
linear: TA-44
depends_on: ["010", "003", "002"]
created: 2026-11-02
updated: 2026-11-07
---

# Spec: Repeticiones y verificación en el navegador

> Intent: [intent.md](./intent.md) · ADRs: [0001](../../decisions/0001-motor-propio-determinista.md), [0002](../../decisions/0002-commit-reveal-semilla.md), [0004](../../decisions/0004-personajes-homenaje.md), [0007](../../decisions/0007-i18n-desde-cero.md), [0012](../../decisions/0012-logs-y-cabeceras-de-seguridad.md) A4, B4.1 y B6 · Specs: [010](../010-scheduler-de-peleas/spec.md) (RF2, RF6, RF7, RF26, RF27, RF34, RF55, RF56 y preocupaciones 1, 2 y 13), [003](../003-motor-de-pelea/spec.md) (RF19), [002](../002-spike-renderer/spec.md) (RF-A17)

## Resumen
Una lista de peleas terminadas (`GET /api/fights`, paginada por `id`, sin índice ni migración) lleva a una página con el reproductor existente, que carga el log con `load` y usa `play`, `pause` y `seek`: **`packages/renderer` no cambia**. El navegador comprueba por su cuenta lo mismo que RF7 de 010, con un módulo puro y un `.wasm` **generado y commiteado**, cuya frescura se comprueba byte a byte en `just check` y en CI. No hay arte nuevo, no se toca `crates/engine`, ni el ciclo de vida, ni el ledger. La CSP solo suma `'wasm-unsafe-eval'` (ADR-0012 B4.1).

## Entrega
Un solo PR, con un `plan.md` de frontend-engineer. Separar el server de la web no reduce riesgo: la ruta nueva es pequeña y no tiene otro consumidor. El plan lo ordena por commits: artefacto y `verify.ts`, ruta, páginas y e2e. `devops-engineer` revisa `justfile`, `ci.yml` y `check-workflows`; `backend-engineer`, la ruta. Lo que sí tiene que pasar antes de implementar es el gate de `risk: high` del owner.

## Requisitos funcionales

### El `.wasm` en el navegador
- **RF1. Artefacto generado.** `just gen-wasm` escribe en `apps/web/src/lib/features/replay/engine-wasm/` los cuatro archivos de `wasm-bindgen --target web --omit-default-module-path`: `engine_wasm.js`, `engine_wasm.d.ts`, `engine_wasm_bg.wasm` y `engine_wasm_bg.wasm.d.ts`. Se commitean y nunca se editan a mano. La receta compila con `cargo build -p engine-wasm --target wasm32-unknown-unknown --release --locked` en `target/wasm-artifact`, con `RUSTFLAGS="--remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}/registry/src=/cargo --remap-path-prefix=$PWD=/src"`. Sin ese remap, el binario lleva la ruta del home de quien lo generó, y el repo es público; por eso el valor por defecto de `CARGO_HOME`, que puede no estar definida. El CLI es `wasm-bindgen-cli` en la versión exacta de `wasm-bindgen` en `Cargo.lock`, con `cargo install --locked` en `target/wasm-tools/<versión>/`, y la receta lo instala si falta. El directorio entra en `.gitattributes` (`-text`), `.prettierignore` y los ignores de ESLint. → la prueba de RF2.
- **RF2. Frescura byte a byte.** `_check-wasm-artifact`, dentro de `check-rust` y después de `_check-wasm`, genera en un directorio temporal y compara con `scripts/compare-trees.mjs`, que hoy lleva "protocol-ts" y `just gen` fijos en sus mensajes y pasa a recibir el nombre del árbol y su comando. Si difiere, falla nombrando `just gen-wasm`. Corre de verdad en `x86_64-unknown-linux-gnu`, la referencia, y en `aarch64-apple-darwin`, que hoy da los mismos bytes. En otro host escribe que se omite y sale con 0, y con `CI` definido falla. El filtro `rust` de `ci.yml` suma ese directorio y las recetas ya cuentan por `justfile`, para que un PR que solo cambia el binario corra la comprobación. `check-workflows` falla si el filtro lo pierde.
  **También en el release:** `ci-ok` protege el PR, no el tag, y `release.yml` no mira el estado de CI del commit. Un job nuevo, `wasm-artifact`, corre `_check-wasm-artifact`, y **no va dentro de `build`**: compila y corre `cargo install`, con scripts de build de crates que `just crate-age` y `cargo deny` no ven (preocupación 4), y `build` tiene `packages: write` y el token de GHCR en disco. Su forma:
  - `permissions: { contents: read }`, en la tabla de `check-workflows`. Sin `rust-cache` ni otra caché restaurada, sin secretos, sin `docker/login-action` y con `persist-credentials: false`.
  - **Sin `if` de job: corre siempre, y cada uno de sus pasos lleva `if: github.event_name == 'push'`,** igual que los de `build`. `release.yml` solo se dispara en `push` por un tag `v*` o en `workflow_dispatch`, así que esa condición significa "en un tag". En un rollback, sus pasos se omiten y el job termina en `success`, sin compilar ni instalar nada. Corre antes de `Validate the tag`, sin problema: ningún `run:` usa el nombre del tag. El checkout no lleva `ref:`, así que comprueba el mismo `github.sha` que `build` construye.
  - `build` declara `needs: wasm-artifact` y **no lleva `if` de job**. `deploy` y `verify` no cambian. Como `wasm-artifact` nunca queda `skipped`, la condición implícita de los `needs` deja pasar el rollback hasta `verify`, y en un tag un fallo o una cancelación detienen `build`, `deploy` y `verify`.
  - `check-workflows` exige esa forma: sus permisos; sin `if` de job en `wasm-artifact` ni en `build`; `needs: wasm-artifact` en `build`; `_check-wasm-artifact` en un paso con esa condición exacta y sin `continue-on-error`; y sin caché ni login en el job.

  → tests de `check-workflows`, un caso negativo por regla, también con el filtro; tocar un byte falla `_check-wasm-artifact`.
- **RF3. El artefacto se comporta.** Un Vitest de `apps/web` carga el `.wasm` commiteado con `initSync` y reproduce los 7 dorados de `crates/engine/tests/golden/` byte a byte, da `ruleset.unknown` con una versión desconocida y comprueba que los bytes no contienen `/Users/` ni `/home/`. Con `WebAssembly.Module.exports` e `imports` fija la superficie: las únicas funciones propias son `simulate` y `ruleset_bounds`, y solo importa del glue (hoy, `__wbg_Error_*`). `scripts/check-engine.mjs` suma que `crates/engine-wasm/src` no tenga bloques `extern` y que `js-sys` y `web-sys` no estén en su grafo; hoy no están. Corre en `check-web`, sin Rust. → ese test y los de `check-engine`.
- **RF4. Cotas del ruleset.** `engine-wasm` suma una segunda función, `ruleset_bounds(ruleset_version: u16) -> Result<String, JsError>`, que devuelve `{"time_limit_ms":N,"max_duration_ms":M}` o el código `ruleset.unknown`. Enmienda RF19 de 003 ("exporta una sola función"): sin ella, el navegador no puede comprobar las cotas de RF7 de 010 sin copiar datos del motor. No toca `crates/engine`: usa `Ruleset::time_limit_ms()` y `end_ms()`, con la misma suma que el verificador nativo. → `crates/engine-wasm/tests/codes.rs`; igualdad con `for_version` en nativo.
- **RF5. Carga.** El `.wasm` se importa con `?url`, así que Vite lo emite como archivo del mismo origen en `/_app/immutable/assets/`, y lo carga el `fetch` del glue (`init({ module_or_path })`), cubierto por `connect-src 'self'`. Nunca `data:`. Solo `lib/features/replay/engine-loader.ts` importa de `engine-wasm/`, y solo con `import()` (regla de ESLint). `scripts/check-renderer-split.mjs` suma tres reglas sobre el build: ninguna entrada ni nodo de ruta alcanza ese directorio por imports estáticos; el nodo de `fights/[id=fight_id]` sí lo alcanza por uno dinámico, para que el chequeo nunca pase vacío; y hay exactamente un `.wasm`, byte a byte igual al commiteado, y ningún `data:application/wasm`. La carga tiene un plazo de 30 s. → ese script en `just e2e`; un e2e cuenta las peticiones de `.wasm` y de Pixi al abrir `/` y `/fights`: cero.
- **RF6. CSP.** `script-src` suma `'wasm-unsafe-eval'`, y nada más, en `apps/web/vite.config.ts`, `CSP_B1` de `apps/web/e2e/security.spec.ts` y `b1=` de `ci.yml` y `release.yml`, en el mismo PR (ADR-0012 B6). `check-kit-config.mjs` ya lo admite: no cambia su allowlist. `csp.spec.ts` cubre `/es/fights` y una pelea con verificación terminada, en Chromium y WebKit, sin violaciones. El PR justifica la fuente, como pide B6.5. Un navegador que no conozca esa fuente bloquea la compilación, y eso es `cannot_check` (RF12), no un error de la página.

### Ruta de lista
- **RF7. `GET /api/fights`.** Responde `FightsPage { fights: Vec<FightSummary>, next_before: Option<u32> }`, con 20 peleas `settled` como máximo, de `id` mayor a menor. `?before=<id>` da las de `id` menor; el valor usa la misma regla de `parse_id` de RF34, y uno inválido da 404 `fight.not_found`. El tamaño de página es fijo: no hay `limit`. `next_before` es el `id` de la última de la página si hay más, y `null` si no. Sin sesión, igual para todos. `Cache-Control: no-store`, también en sus 404, 429 y 503, y límite `FIGHT_READ_IP` compartido con el detalle. Una base caída da 503 `fight.unavailable`. → integración en `fights_api.rs`: orden, páginas, final de la lista, parámetro inválido y cabeceras.
- **RF8. Consulta acotada.** `store::settled_page(pool, before, lock_timeout)`: `WHERE state = 'settled' AND id < $1 ORDER BY id DESC LIMIT 21`, con el `lock_timeout` y el plazo de `by_id_bounded`. Sus columnas son las de `decode` con `NULL::bytea` en `revealed_seed` y `log` y `''::bytea` en `spec_a` y `spec_b`, así que no pueden traer semilla ni log, y Postgres no lee sus valores TOAST. **Sin índice ni migración:** recorre la llave primaria hacia atrás y solo descarta las filas sin liquidar, que son pocas (RF13 y RF16 de 010). Una fila que no decodifica o sin `summary()` se omite con `fights.list_row_invalid` en `warn`, sin tumbar la página, y `next_before` sale de las filas leídas. No se agregan campos a `ALLOWED_FIELDS`. → integración con una fila en cada estado: solo sale la `settled`; prueba de fuente sobre las columnas; unitaria de la omisión.
- **RF9. Sin fugas.** La ruta entra en `ROUTES` con `Effect::Read` y `Handler::FightsList`, y RF27 de 010 la recorre en cada estado y con cada identidad. Eso solo no basta: `request_of` es exhaustivo, pero `expected_status` termina en `_ => None`, y una lista vacía no prueba nada. Por eso `fights_no_leaks.rs` agrega `FightsList` con 200 para las tres identidades y quita ese comodín; antes de `Settled` afirma que `fights` no incluye la pelea, y en `Settled`, que sí la incluye, para que la búsqueda de canarios no pase en vacío; y repite el barrido con `?before=<id+1>`. RF41 de 010 se extiende: dos peleas con semillas distintas dan el mismo cuerpo hasta `live_started_at`. → esas pruebas.
- **RF10. Tipos.** `FightsPage` en `crates/protocol/src/fights.rs`, sin texto ni 64 bits, y `just gen`. El SSR lee con `getFightsPage(api, before: number | null)` y `getFightDetail(api, id: number)` en `lib/server/api/reads.ts`: reciben números, nunca una cadena de la URL (RF18). → `_check-gen`; `svelte-check`.

### Verificación
- **RF11. Módulo puro.** `lib/features/replay/verify.ts` exporta `verifyFight(detail, expectedId, deps): Promise<Verdict>`, sin DOM, sin Svelte y sin importar el glue. `deps` trae `sha256(bytes)` y `engine()`, una promesa de `{ simulate, rulesetBounds }`. Un módulo aparte construye los `deps` reales. `engine()` solo se llama con un `decided` que ya pasó el commit y RF2, y una `voided` o `cancelled` nunca descarga el `.wasm`. `verifyFight` nunca rechaza: toda excepción inesperada da `cannot_check(engine_failed)`, así que la vista no se queda en "comprobando". → Vitest en Node con el `.wasm` real.
- **RF12. Estados.** Un `Verdict` es uno de estos, y ninguno es parcial:

  | Estado | Cuándo | Equivale en RF7 de 010 a |
  |---|---|---|
  | `match` | Toda comprobación de su final pasó. Lleva `scope`: `fight`, `commit` o `snapshot` | `Ok` |
  | `mismatch` | Una comprobación falló, o falta un dato que el final exige. Lleva `check` (RF13) | `Err` |
  | `cannot_check` | El navegador no pudo hacer la prueba. Lleva `reason`: `ruleset_unknown`, `engine_unavailable`, `engine_failed`, `crypto_unavailable` o `unknown_final` | No existe |
  | `not_applicable` | `cancelled(fight_unrunnable)`: nunca hubo semilla, y el snapshot puede ser justo lo que falló | `Ok` |

  Mientras tanto, la vista muestra "comprobando". **La frontera:** lo que depende de los datos publicados es `mismatch`, y lo que depende del navegador es `cannot_check`. Un fallo técnico nunca se presenta como `match` ni como `mismatch`.
- **RF13. Qué compara.** Exactamente lo de `apps/server/tests/support/public_verifier.rs`, en su orden, sin omitir pasos:

  | Final | Paso | `check` si falla |
  |---|---|---|
  | Todos | `detail` tiene la forma de `FightDetail` (lo valida `load-detail.ts`: tipos, y `ruleset_version` entero de 0 a 65535; de `final` solo que sea nulo o un objeto con `kind` de texto, sin mirar si conoce `kind` o `reason`) y `fight.id` es el de la URL | `data` |
  | Todos | `state` es `settled` y `final` no es nulo | `state` |
  | `decided` | `seed` y `seed_commit` presentes, `seed` son 64 dígitos hexadecimales en minúscula y `SHA-256(seed) == seed_commit` | `commit` |
  | `decided` | `spec_a` y `spec_b` presentes, y RF2 por concatenación de los bytes UTF-8 == `snapshot_sha256` | `snapshot` |
  | `decided` | `log` presente y `simulate(seed, spec_a, spec_b, ruleset_version) === log`, cadena exacta. Un código `seed.*`, `spec.*` o `input.too_large` también es este fallo | `log` |
  | `decided` | `final` == `outcome` del `end` del log | `final` |
  | `decided` | `slug` del resumen == `slug` de su spec, y `slug`, `max_hp` y `ruleset_version` del resumen == los del log | `summary` |
  | `decided` | `time_limit_ms` y `max_duration_ms` == `rulesetBounds(ruleset_version)` | `bounds` |
  | `voided` | commit, como arriba, y `log` nulo | `commit` o `log` |
  | `voided` | RF2, salvo `snapshot_invalid` | `snapshot` |
  | `cancelled` | `seed`, `log` y `seed_commit` nulos | `commit` o `log` |
  | `cancelled` | RF2, salvo `fight_unrunnable` | `snapshot` |

  Fuera de esas ramas: `ruleset.unknown` da `cannot_check(ruleset_unknown)`, sin importar de qué función venga. Un rechazo de `engine()` o el plazo de RF5 dan `engine_unavailable`. Una trampa del `.wasm` o `log.encode_failed` dan `engine_failed`. Sin `crypto.subtle` (contexto no seguro) da `crypto_unavailable`, y no hay SHA-256 de repuesto: producción es siempre `https`, y `localhost` es contexto seguro. El `switch` sobre `final.kind` y `reason` es exhaustivo con `never`, y un valor desconocido en ejecución da `cannot_check(unknown_final)`. Una respuesta 200 que no es JSON no llega al veredicto: es el estado "no se pudo cargar" de RF18. → Vitest, un caso por fila, con el dato alterado y con el dato nulo, incluidos `final: null` y `state: "live"`.
- **RF14. Igual que la comprobación nativa.** `apps/server/tests/golden/fight-verify-vectors.json` lleva `FightDetail` de KO, decisión, empate, `voided` y `cancelled`, con los bytes reales del motor, specs canónicas y horas fijas, sin Postgres *(Enmienda plan: D5)*, cada uno también alterado, con el resultado esperado. Lo escribe y lo comprueba un test de `fights_public_verify.rs` con `verify`, y lo comprueba Vitest con `verifyFight`: `Ok` debe ser `match` o `not_applicable`, y `Err`, `mismatch`. `public_verifier.rs` pasa a exigir 64 dígitos en minúscula, como RF13: hoy acepta mayúsculas y daría una tercera diferencia *(Enmienda plan: D2)*. Hay dos excepciones, marcadas en el vector: la versión desconocida y el final desconocido, nativo `Err` (o no deserializa) y navegador `cannot_check`. Un vector lleva un `final.kind` y otro una `reason` desconocidos, para fijar que la validación de forma no los rechaza y llegan a `unknown_final` (RF22). El filtro `web` de `ci.yml` suma ese archivo. → los dos tests.
- **RF15. Cuándo corre.** Sola, al abrir la página y con los datos ya cargados, una vez por carga, en el hilo principal. No hay botón: pesa menos que el reproductor (`.wasm` de 144 KB, 62 KB con gzip) y tarda milisegundos. El módulo comprueba el mismo `FightDetail` que se reproduce, nunca otra lectura. Con `cannot_check`, según lo que puede ayudar: `engine_unavailable` ofrece reintentar; `ruleset_unknown` y `unknown_final` ofrecen recargar, porque es una versión vieja del sitio; `engine_failed` también, porque tras una trampa del `.wasm` el glue reutiliza la instancia y un reintento podría dar un `mismatch` falso; `crypto_unavailable` no ofrece nada. El resultado está en una región `aria-live="polite"`. No revela el ganador: ver RF20.
- **RF16. Qué dice.** Texto en en y es. El plan puede pulirlo con product-owner, sin prometer más:

  | Estado | es | en |
  |---|---|---|
  | comprobando | Comprobando esta pelea en tu navegador… | Checking this fight in your browser… |
  | `match`, `fight` | Coincide. La semilla es el número que decide cómo sale la pelea, y el sello es su huella: sirve para comprobarla sin revelarla. La semilla publicada corresponde al sello publicado y, con esta semilla y estos personajes, el motor da exactamente esta pelea, hasta el resultado. | It matches. The seed is the number that decides how the fight goes, and the seal is its fingerprint: it lets you check the seed without revealing it. The published seed corresponds to the published seal and, with this seed and these fighters, the engine produces exactly this fight, result included. |
  | `match`, `commit` | Coincide. La semilla es el número que habría decidido la pelea, y el sello es su huella. La semilla publicada corresponde al sello publicado. Esta pelea se anuló, así que no hay pelea que simular. | It matches. The seed is the number that would have decided the fight, and the seal is its fingerprint. The published seed corresponds to the published seal. This fight was voided, so there is no fight to simulate. |
  | `match`, `snapshot` | Coincide. Los personajes publicados corresponden a su huella publicada. Esta pelea no se disputó y no tuvo semilla. | It matches. The published fighters correspond to their published fingerprint. This fight was not held and had no seed. |
  | `mismatch` | No coincide. Los datos publicados no reproducen esta pelea. Puede ser un error nuestro o datos alterados: no lo sabemos desde aquí. | It does not match. The published data does not reproduce this fight. It may be our mistake or altered data: we cannot tell from here. |
  | `cannot_check`, técnico | No se pudo comprobar. Tu navegador no pudo hacer la prueba. Esto no dice nada sobre la pelea. | Could not check. Your browser could not run the test. This says nothing about the fight. |
  | `cannot_check`, `ruleset_unknown` | No se pudo comprobar. Esta pelea usa reglas (versión {n}) que esta versión del sitio no conoce. Recarga la página. Esto no dice nada sobre la pelea. | Could not check. This fight uses rules (version {n}) this version of the site does not know. Reload the page. This says nothing about the fight. |
  | `cannot_check`, `unknown_final` | No se pudo comprobar. Esta versión del sitio no conoce este tipo de final. Recarga la página. Esto no dice nada sobre la pelea. | Could not check. This version of the site does not know this kind of ending. Reload the page. This says nothing about the fight. |
  | `not_applicable` | Nada que comprobar: esta pelea no se disputó y nunca tuvo semilla. | Nothing to check: this fight was not held and never had a seed. |

  `match` solo afirma lo que el navegador comprobó, con datos de una misma respuesta. "Sello" es la palabra de `/` ("Semilla sellada"), y `seed_commit` es solo el nombre del campo.

  **El límite, visible siempre debajo de `match` con `fight` o `commit`, fuera de todo `<details>`:**
  - es: "El sello se publica antes de la pelea, pero esta página no puede probarlo por sí sola: quien lo haya visto o guardado antes de la pelea puede compararlo. Tampoco prueba que la semilla saliera al azar."
  - en: "The seal is published before the fight, but this page cannot prove that on its own: anyone who saw or saved it before the fight can compare it. It does not prove the seed came out at random either."

  `mismatch` agrega una línea por `check`, en lenguaje de espectador: `data` y `state` (la respuesta no describe una pelea terminada), `commit` (la semilla no corresponde al sello), `snapshot` (los personajes no corresponden a su huella), `log` (la pelea no sale igual), `final` (el resultado no es el de la pelea), `summary` y `bounds` (el resumen no coincide con la pelea o sus reglas).

  **Debajo de todo veredicto, en un `<details>` cerrado,** el resto del límite:
  - es: "Quien controla el sitio pudo probar varias semillas y sellar la que le convenía. Estos datos tampoco se pueden dar por originales si nadie guardó una copia. La comprobación corre con el código que te entrega este sitio: para no depender de él, el motor es abierto y puedes correrlo con estos datos."
  - en: "Whoever controls the site could have tried several seeds and sealed the one that suited them. Nor can this data be taken as the original unless someone saved a copy. The check runs on code this site gives you: to avoid relying on it, the engine is open and you can run it on this data."

  En una `cancelled`, que no tuvo semilla, se omite la primera frase, y la siguiente dice: "Estos datos no se pueden dar por originales si nadie guardó una copia." / "This data cannot be taken as the original unless someone saved a copy."

  Y también `seed`, `seed_commit` (salvo en una `cancelled`, donde están vacíos), `snapshot_sha256` y `ruleset_version`, completos y copiables (`copy-text.ts`), y un enlace a `/api/fights/{id}`. → `check-i18n`; revisión de product-owner y security-reviewer; un Vitest sobre las llaves `replay_verify_*` prohíbe, en en y es, "imposible", "garantizado", "a prueba de trampas", "comprometió", "antes de empezar" y sus equivalentes, y exige la frase del límite junto a `match`.

### Lista y reproductor
- **RF17. Lista: `/fights`.** Sin sesión. `+page.server.ts` lee `GET /api/fights`, con `no-store`, y solo acepta `before` si es un decimal canónico; si no, da la primera página. Cada fila enlaza a su pelea y muestra el cruce, la fecha (`live_started_at`, o `live_at` si no lo hay) y el tipo de final: "disputada", "cancelada" o "anulada". **Sin ganador ni KO, decisión o empate** (RF20). La paginación es un enlace "ver más antiguas" (`?before=`), que funciona sin JavaScript. Sin peleas, un estado vacío que explica que aún no terminó ninguna y enlaza a `/`. Un fallo del API da 200 con estado de error, nunca 500, como RF53 de 010. La fecha se dibuja en UTC, con la zona a la vista, en el SSR y en el primer render, y pasa a la hora local en `onMount`, sin error de hidratación. → e2e, también en `hydration.spec.ts`.
- **RF18. Página: `/fights/{id}`.** La ruta es `routes/fights/[id=fight_id]`, con un matcher nuevo (`src/params/fight_id.ts`) que aplica el decimal canónico de 1 a `i32::MAX` de RF34, antes de cualquier lectura. Kit decodifica el parámetro y `handleFetch` agrega el secreto interno, la IP y la cookie, así que sin él `/fights/..%2Fauth%2Fgoogle%2Fcallback%3Fcode%3Dx` haría que el SSR pidiera otra ruta del API como la víctima. Si no coincide, 404 sin tocar el API. El `+page.server.ts` lee `GET /api/fights/{id}` y de una pelea `settled` solo deja pasar al cliente `{ id, a: slug, b: slug, date }`: el final, el log, la semilla y los specs no van en el HTML. En el cliente, `onMount` pide `GET /api/fights/{id}` con `load-detail.ts`, y ese `FightDetail` alimenta al reproductor, al resultado y a la verificación, así el veredicto habla de lo que se muestra. Así el SSR da 404 reales y el navegador comprueba lo que recibió directo del server. `error(404)` si no existe. Un 503 o 429 del SSR o del cliente da un estado con reintento, nunca 500. Pixi solo por `import()` en `FightStage.svelte` (`check-renderer-split`). → test del matcher con `..%2Fme` decodificado, `12?x`, `01` y `2147483648`; e2e de esas URL, con el log del server sin ninguna petición; test de `load` de que su salida no tiene `final`, `seed`, `log` ni `spec_*`.
- **RF19. Reproductor y controles.** Usa `FightStage` con `source` (el `load` de siempre), `timelineFromFightLog` y los placeholders por lado. **Empieza en pausa, en `t = 0`**, para no gastar datos ni batería sin que se pida. Controles, en una fila de botones de 44 px como mínimo que cabe a 375 px, sin texto cortado: **reproducir o pausar, −10 s, +10 s, reiniciar e ir al final**. Con `finished`, el primero pasa a "ver otra vez". **Comportamiento:** los saltos conservan el estado, reproduciendo o en pausa. Desde `finished`, −10 s deja en pausa en ese punto. Reiniciar va a `t = 0` y siempre reproduce. +10 s nunca se desactiva antes de `finished`, porque un botón apagado delataría cuánto queda (RF20). Ir al final deja `finished` y muestra el resultado. Sin barra de progreso ni velocidad. El reloj de ronda (`RoundClock`, `roundClockMs`) se muestra igual que en vivo. `FightStage` suma dos métodos exportados, `seekBy(ms)` y `seekToEnd()`, sobre `seek`, y `onsnapshot` también sin `live`, hasta su destrucción. Si el host pasa `onsnapshot`, además, sin `live` vigila la pérdida de contexto (`watchContext`), no deja de leer en `finished`, y tras perder el contexto vuelve a ese `t`, en pausa: sin eso, RF21 no se cumple y reiniciar se queda sin datos. `/dev/arena` y el catálogo no pasan esa prop y no cambian *(Enmienda plan: D1)*. **`packages/renderer` no cambia.** → e2e: una por KO, una por decisión y un empate se ven hasta el final, se reinician y se adelantan, con el estado de cada control.
- **RF20. Spoilers y coherencia con RF55.** Una barra delataría un KO temprano, así que no hay barra. ±10 s no dice cuánto queda, salvo al llegar al final, que es lo que se pidió. El resultado (`FightResultCard`, con el `final` del `FightDetail`) aparece en una región `aria-live` cuando llega `snapshot().ending` o `finished`, y antes solo con un botón explícito "ver el resultado". Una vez visto, no se oculta al reiniciar. La verificación no revela nada, y con `mismatch` tampoco dice el ganador. El `<title>` lleva el cruce, sin resultado. El HTML no lleva el final (RF18). La respuesta del cliente sí, con el log: es público desde `Settled` y solo se ve abriendo DevTools. → e2e: antes de reproducir, ni la lista ni la página muestran el ganador.
- **RF21. Sin animación.** `prefers-reduced-motion`: igual que el vivo, por `FightStage`, y los controles siguen. `webgl_unavailable`, o el contexto perdido demasiadas veces: sin controles, con `fight_notice_no_animation`, el botón de ver el resultado y la verificación completa. No hay reproducción por texto: el intent pide el resultado, y duplicar un reloj para mostrar HP no aporta en una repetición. Un log que no parsea o que `timelineFromFightLog` rechaza: lo mismo, con una llave propia (`replay_notice_playback_error`: "No pudimos mostrar la animación." / "We couldn't show the animation."), porque `fight_notice_playback_error` promete que el resultado "aparecerá aquí", y aquí va detrás del botón. Si falla el `.wasm`, la pelea se sigue viendo (RF12). → e2e con `getContext` sin WebGL y con `.wasm` bloqueado.
- **RF22. Finales sin log.** `cancelled` y `voided` muestran el cruce, la fecha y, ya con el `FightDetail`, su motivo, sin reproductor, sin controles y con la verificación de RF13, en un tono que no parece un error:
  - `missed_window`: "No se disputó: se perdió su horario."
  - `fight_unrunnable`: "No se disputó: no se pudo preparar."
  - `snapshot_invalid`: "Se anuló: los datos de los personajes estaban dañados. Se publicó su semilla."
  - `engine_error`: "Se anuló: el motor no pudo terminarla. Se publicó su semilla."

  **`voided(secret_lost)` lo agrega TA-27**, con su enmienda de ADR-0002 (preocupación 13 de 010). Esta feature deja dos cosas: una `voided` de `reason` desconocida muestra "anulada" sin motivo y da `cannot_check(unknown_final)`, también con `seed` nula, sin romperse; y una `voided` conocida con `seed` nula da `mismatch(commit)`, como pide el criterio 4 de esa preocupación. Cuando TA-27 agregue la variante, el `switch` de RF13 deja de compilar y le obliga a dar su estado propio. → Vitest y e2e con datos simulados.
- **RF23. Pelea sin liquidar.** Con `state` distinto de `settled`, también detenida, la URL responde 200 con "esta pelea todavía no termina", el cruce y un enlace a `/`, sin reproductor, sin pedir el detalle ni descargar nada. El SSR solo pasa `{ pending: true, a: slug, b: slug }`, y el matcher de RF18 también aplica. No se redirige, para que el enlace siga valiendo cuando termine, y no se actualiza sola. → test de `load` con los cuatro estados; e2e real en `fights.spec.ts` mientras la pelea está en `live`; RF27 de 010 para el API.
- **RF24. Datos incompletos.** `log`, `spec_a` o `spec_b` nulos en un `decided` (DP-10 de 010): sin reproductor si falta el log, con el aviso de RF21, el botón "ver el resultado" y `mismatch` del paso que corresponda. Un JSON de `log` inválido no llega a `simulate`: la comparación de cadenas ya lo detecta. Una respuesta sin la forma de `FightDetail` no da reproductor, resultado ni botón, porque nada en ella es fiable: solo `mismatch(data)` *(Enmienda plan: revisión de architect)*. La versión desconocida la cubre RF16: no parece una manipulación. → Vitest.
- **RF25. Nombres.** `fighterDisplay` y su respaldo de RF56 de 010: un personaje retirado de `content/` se ve con su slug y su pelea sigue reproduciéndose y verificándose, porque ambas salen del snapshot. → e2e con los personajes de los dorados, que no tienen lore; `a_fight_verifies_after_its_content_changed` de 010.
- **RF26. Cómo se llega y cómo se vuelve.** `FightHome` suma un enlace a `/fights`. `FightResultCard` recibe una prop opcional con el enlace a su repetición, y solo `FightHome` se la pasa, con `fight.state === 'settled'`: el resultado en vivo sale antes de `Settled` y ese enlace daría "todavía no", y en `/fights/{id}` la tarjeta se enlazaría a sí misma. El aviso de RF57 de 010 (`fight_notice_degraded`) suma un enlace a `/fights`. Las dos páginas nuevas leen una vez `GET /api/fights/current` en el SSR, y si `current` está en `live` muestran un aviso "hay una pelea en vivo" con enlace a `/`: no hace polling ni usa WS, así que puede quedar viejo, y el enlace sigue siendo válido. Si esa lectura falla, no se muestra el aviso. → e2e con `fake-ws` en `home.spec.ts`.

## Requisitos no funcionales
- **Carga:** `/` y `/fights` no cargan Pixi ni el `.wasm` (RF5). El `.wasm` medido: 144 KB y 62 KB con gzip. En Node, en un Mac reciente, compila en 2 ms y simula en menos de 4 ms. `check-renderer-split` imprime el tamaño, sin umbral.
- **Server:** una consulta acotada a 21 filas sin leer TOAST, y la lectura de detalle ya existente. Un visitante hace dos lecturas por página (SSR y cliente), una consulta por llave primaria: aceptable con 120/min por IP.
- **Disco:** sin cambios. Las peleas se conservan siempre (RF4 de 010 prohíbe borrar). Unas 52 000 al año caben en una paginación por cursor, que no se degrada con la profundidad.
- **Logs (ADR-0012 A4):** ni la semilla ni el contenido del log aparecen en un log, tampoco tras `Settled`. El SSR lee el detalle y no lo registra. `verify.ts` no escribe en consola.
- **i18n:** todo en en y es, con `check-i18n`. Fechas y números con `Intl`. Los códigos (`check`, `reason`) nunca se muestran.
- **Móvil y accesibilidad:** una columna a 375 px, con el pulgar. Veredicto y resultado en regiones `aria-live`, foco visible, `a11y.spec.ts` cubre las dos páginas.
- **Determinismo y límites:** `crates/engine` no cambia. El glue y el `.wasm` no conocen el log ni el DOM. Las entradas siguen limitadas a 8 KiB (`MAX_INPUT_BYTES`, dorados de 250 a 350 bytes). `apps/web` sigue sin lógica de dominio: compara lo que da el motor y no simula por su cuenta.

## Diseño

### Componentes afectados
| Componente | Cambio |
|---|---|
| `crates/engine-wasm` | `ruleset_bounds`, y sus tests |
| `crates/protocol` | `FightsPage`, `just gen` |
| `apps/server` | `fights/api.rs`, `fights/store.rs`, `http/routes.rs`. Tests: `fights_api`, `fights_no_leaks`, `fights_public_verify`, `routes`. **No se tocan `fights/hook.rs` ni `fights/bets_contract.rs`** (TA-27) |
| `packages/renderer` | **Sin cambios** |
| `apps/web` | `routes/fights/**`, `params/fight_id.ts`, `lib/features/replay/{verify,engine-loader,load-detail}.ts`, `engine-wasm/` generado, componentes de lista y veredicto, `FightStage` (RF19), `FightHome`, `FightResultCard`, mensajes, `vite.config.ts`, `reads.ts` |
| `apps/web/Dockerfile`, `.dockerignore`, `check-compose` | **Sin cambios.** `COPY apps/web apps/web` ya trae el artefacto |
| `scripts/` (`compare-trees`, `check-engine`, `check-renderer-split`, `check-workflows`), `justfile`, `ci.yml`, `release.yml` | RF2, RF3, RF5 y RF6; receta de e2e de "Estrategia de pruebas" |
| Docs | `docs/architecture.md`, `CLAUDE.md`, glosario, `apps/server/README.md` (`GET /api/fights`) y la enmienda de RF19 en la spec de 003 |

### Cómo obtiene cada entorno el mismo binario
| Entorno | De dónde sale | Rust necesario |
|---|---|---|
| `just dev` | El archivo commiteado, por Vite | No para el `.wasm` |
| `check-web` y job `web` | El commiteado: Vitest (RF3 y RF14), `svelte-check` y ESLint | No |
| `just check` y job `rust` | Lo regenera y lo compara con el commiteado (RF2) | Sí, ya está |
| `just e2e`, jobs `e2e` y `e2e-fights` | El commiteado, empaquetado por `pnpm --filter web build` | No para el `.wasm` |
| Imagen de release | El commiteado, en la etapa de Node. El job `wasm-artifact` corre antes RF2 sobre ese commit, sin permisos de escritura, y `build` depende de él | Solo en `wasm-artifact`; `build` no compila Rust para esto |

### Tipos
```rust
pub struct FightsPage { pub fights: Vec<FightSummary>, pub next_before: Option<u32> }
```
```ts
type Verdict =
  | { status: 'match'; scope: 'fight' | 'commit' | 'snapshot' }
  | { status: 'mismatch'; check: 'data' | 'state' | 'commit' | 'snapshot' | 'log' | 'final' | 'summary' | 'bounds' }
  | { status: 'cannot_check'; reason: 'ruleset_unknown' | 'engine_unavailable' | 'engine_failed' | 'crypto_unavailable' | 'unknown_final' }
  | { status: 'not_applicable' };
```

### Flujo
```
/fights  SSR ── GET /api/fights ─▶ filas, sin resultado
   │ clic
/fights/12  matcher ─▶ SSR ── GET /api/fights/12 ─▶ ¿existe? ¿settled? pasa solo cruce y fecha
   │ onMount
   ├─ fetch GET /api/fights/12 ─▶ FightDetail
   │      ├─ log ─▶ timelineFromFightLog ─▶ FightStage (import de Pixi), en pausa
   │      └─ verifyFight ─ SHA-256 commit y RF2
   │                     └─ decided: import() del glue + fetch del .wasm ─▶ simulate, bounds
   └─ el resultado del FightDetail, por snapshot().ending, finished o el botón
```

### UI
Una columna a 375 px; en escritorio, escenario y panel, como `/`. Estados de la página: cargando (`Skeleton`), lista para reproducir, no se pudo cargar con reintento, sin liquidar (RF23), final sin log (RF22) y sin animación (RF21). `data-*` de depuración solo con `?debug=1` y el catálogo de desarrollo, como en `/` (plan-d de 010, decisión 14).

## Áreas de preocupación
1. **Un binario que nadie lee en el diff, en un repo público.** Una revisión no puede ver qué hay en el `.wasm`: su garantía es RF2, y por eso el filtro `rust` y su test son obligatorios. El `ci-ok` requerido lo hace bloqueante en el PR, pero no en el tag: dos PR que pasan por separado pueden dejar `main` con un binario viejo, y por eso RF2 también corre en el job `wasm-artifact` del release, del que `build` depende. RF5 cubre lo que Vite hace con el archivo.
2. **Reproducibilidad entre máquinas: medida, no garantizada.** Con `rustc 1.99.1` y el remap de RF1, un Mac ARM y `linux/amd64` dieron los mismos bytes, en rutas distintas: el `.wasm` de `cargo` y los cuatro archivos de `wasm-bindgen`. `linux/arm64` dio otro `.wasm` de `cargo`, y no sabemos por qué. Por eso hay allowlist de hosts, y la referencia es la de CI. Sin el remap, el binario llevaba `/Users/<usuario>/.cargo/...`. Si una subida de toolchain separa al Mac, se quita de la lista: `just check` lo omite allí y `just gen-wasm` se corre en el contenedor `rust` de `apps/server/Dockerfile` con `--platform linux/amd64`. Lo prueba el primer PR de esa subida.
3. **El `.wasm` se regenera con más cambios de los que parece.** Cualquier cambio en `crates/engine`, `engine-wasm`, `protocol`, en crates de su grafo (`serde`, `serde_json`, `rand_chacha`) o en la toolchain falla `_check-wasm-artifact` hasta correr `just gen-wasm`. También un PR de Dependabot, igual que ya pasa con `rust:`. Cada regeneración suma unos 62 KB al historial. Se acepta: esos cambios son raros, y tras la primera entrada de RF6 aún más.
4. **Proveedor del CLI.** `wasm-bindgen-cli` se instala fuera de `Cargo.lock`, así que `just crate-age` y `cargo deny` no ven sus dependencias. Lo acota: versión exacta igual a la librería, `--locked` con el lock que publicó (anterior a la versión ya envejecida en nuestro lock), corre en desarrollo y en CI, y en el release solo en `wasm-artifact`, con `contents: read`, sin caché y sin el token de GHCR: una dependencia comprometida no alcanza a publicar nada. Lo que produce queda fijado en el commit. Instalarlo tardó 51 s en un Mac y 290 s con emulación amd64, así que el job `rust` puede sumar unos minutos sin caché. Su límite sube de 20 a 25 min, y el PR anota lo medido *(Enmienda plan: D6)*. Usa la red una vez por `target/`. Eso no choca con "Sin red" de 010, que es sobre los tests: `cargo` ya descarga crates igual.
5. **ADR-0012 B4.1 es global.** `'wasm-unsafe-eval'` queda también en `/`, donde no se usa. Ya está aceptado por el ADR y no se reabre. No es una CSP por ruta (descartada allí). El gate del owner aplica porque el PR relaja.
6. **Lo que comprueba el navegador tiene dos límites.** El primero es el de las preocupaciones 1 y 2 de 010: no demuestra una semilla al azar, ni detecta una base reescrita si nadie archivó. El segundo es propio de esta feature: el código que hace la prueba lo entrega el mismo sitio, así que una web comprometida podría decir "coincide" siempre. RF16 lo dice, y da el camino: los datos crudos y un motor abierto. No se puede cerrar desde aquí. Por lo mismo, el navegador compara datos de una sola respuesta: no comprueba que el sello sea anterior a la pelea, y `match` no lo afirma.
7. **Choque entre R3 de 010 y "un fallo técnico no es un no coincide".** Se resuelve con la frontera de RF12: falta de datos publicados es `mismatch`, que es lo que dice R3, y fallo del navegador es `cannot_check`, que es lo que dice el intent. Las excepciones conscientes frente al verificador nativo son la versión y el final desconocidos (RF14), porque un build viejo no es evidencia de nada. Un servidor malicioso podría forzarlas, pero solo cambia "no coincide" por un "no se pudo comprobar" visible, y nunca da `match`. `ruleset_version` además está dentro de RF2.
8. **`no coincide` en masa.** Mientras la tabla de RF6 siga vacía, regenerar los dorados de la versión 1 hace que las peleas locales anteriores den `mismatch`: es lo esperado, y se resuelve con `just db-reset`. En producción, el mismo síntoma sería un incumplimiento de RF10 de 003, y el prerrequisito 7 de `docs/architecture.md` lo evita. `FightSummary` no lleva versión del binario, y no hace falta: `ruleset_version` y RF6 atan el comportamiento.
9. **Cursor por `id`, no por `settled_at`.** La lista ordena por anuncio, y la fecha mostrada también. Una pelea detenida y liquidada tarde aparece en su lugar por horario, no arriba, y quien ya pasó esa página no la ve hasta recargar. A cambio, no hay índice, ni migración, ni nada que toque la inmutabilidad, y el cursor es estable. La revisión de `ORDER BY settled_at` de la preocupación de TA-8 (prerrequisito 10) no cambia.
10. **Trabajo en paralelo con TA-27.** Los dos tocan `crates/protocol/src/fights.rs`, `routes.rs`, los mensajes y `fights_no_leaks.rs`, en sitios distintos: el segundo en fusionar resuelve conflictos pequeños. Si TA-27 agrega `SecretLost`, el `switch` de RF13 deja de compilar a propósito. Esta spec no da semántica a esa razón.
11. **Límite de lectura en el e2e.** Todos los proyectos salen de una IP, y `FIGHT_READ_IP` es de 120/min con ráfaga de 30. La configuración nueva corre con `workers: 1`. Si aun así da 429, el plan lo resuelve con menos visitas por prueba, no subiendo el límite.
12. **Tiempo del e2e.** `e2e-fights` tiene un límite de 15 min y ya usa hasta 5,5. Suma unos segundos dentro del flujo real y la configuración nueva, con el objetivo de 3 min. Si se pasa, se reduce a Chromium y WebKit móviles sin bajar la cobertura de `csp.spec.ts`.

## Respuestas a preguntas abiertas del intent
1. **Spoilers** → RF17 y RF20. La lista esconde al ganador y el tipo de resultado, y la página no lo muestra antes de verlo o pedirlo. Sin barra, coherente con RF55. "Tipo de final" del intent se lee como `disputada`, `cancelada` o `anulada`, porque "empate" o "KO" ya es un spoiler. **Decisión de producto: la revisa product-owner.**
2. **Controles** → RF19. Reproducir o pausar, ±10 s, reiniciar e ir al final, sin barra ni velocidad. Sin cambios en el renderer. Una velocidad se podría sumar luego escalando el reloj que inyecta el host.
3. **Alcance de la verificación** → RF13. Todo RF7, final incluido.
4. **Cómo se presenta** → RF15 y RF16. Sola al abrir la página. 62 KB con gzip, solo en esta página. Semilla y sello en un `<details>`, copiables. El texto explica los dos y deja el límite a la vista.
5. **Finales sin log** → RF13 y RF22. Los comprueba, sin `.wasm`. `secret_lost` lo agrega TA-27, y esta feature solo lo tolera sin romperse.
6. **URL sin liquidar** → RF23. "Todavía no termina", con enlace al vivo. Sin redirección.
7. **Datos incompletos o antiguos** → RF12, RF16, RF24 y RF25.
8. **Tamaño de la lista** → RF7 y RF8. 20 por página, cursor por `id`, sin índice, y se conservan siempre. Basta una ruta nueva y `GET /api/fights/{id}`.
9. **Cómo se llega y se vuelve** → RF26. Desde `/`, desde el resultado y desde el aviso sin conexión, y con un aviso de pelea en vivo en las páginas nuevas.

## Estrategia de pruebas
| Criterio del intent | Prueba | Nivel |
|---|---|---|
| Lista | Orden, páginas, filtro por estado, estado vacío | integración, e2e general |
| Sin fugas | RF27 de 010 con la ruta nueva; RF9, con su caso positivo en `Settled`; RF23 en `fights.spec.ts`; test de `load` y matcher | integración, unit, e2e |
| Reproducción | KO, decisión y empate sembrados: hasta el final, reiniciar, ±10 s | e2e |
| Sin apagones | Sin WebGL y con `.wasm` bloqueado por `page.route` | e2e |
| Verificación | RF13 y RF14, con el `.wasm` real. En e2e, una pelea real da "coincide", y log, semilla, commit, `final: null` y `state: "live"` alterados en la respuesta dan "no coincide" | unit, integración, e2e |
| Vigencia | `a_fight_verifies_after_its_content_changed` y personajes sin lore | integración, e2e |
| Finales sin log | `cancelled` y `voided` sembradas; `reason` desconocida con `seed` nula por `page.route` | e2e |
| Logs | Tras la corrida, las líneas de `support/logs.ts` no contienen la semilla ni fragmentos del log sembrado | e2e |
| CSP y carga | `csp.spec.ts`; RF5 | e2e, build |
| Calidad | en y es, 375 px, Chromium y WebKit, sin sesión, `a11y`, `check-i18n`, `just check`, `just e2e`. WebKit en Linux no tiene WebGL (preocupación 10 de 010): en CI comprueba la rama sin animación y anota `limited`, y los controles en WebKit se comprueban fuera de CI, con el resultado en el PR *(Enmienda plan: D3)* | e2e |

**Cómo se organiza el e2e, con el mínimo de tiempo:**
- **General (`_e2e-run`, sin peleas):** estado vacío de `/fights`, 404, enlaces de `/` con `fake-ws`, y que `/` y `/fights` no piden Pixi ni `.wasm`.
- **Peleas reales (`fights.spec.ts`), sin más peleas:** mientras una está en `live`, su URL da "todavía no". Al liquidarse, en cada proyecto, abrir el KO y el empate da "coincide" e ir al final da el resultado publicado. Es el caso real del intent.
- **Sembradas (`playwright.replays.config.ts`, nueva, `workers: 1`, los 4 proyectos):** corre dentro de `_e2e-fights-run`, después de la anterior y con la base recién creada, así `ci.yml` no cambia y `home.spec.ts` sigue sin `last`. Su `globalSetup` llama a un `seedReplays` nuevo de `db.mjs`, con el server ya migrado, porque Playwright levanta `webServer` antes (el plan lo confirma: aquí no está instalado). Cada fila va **en una sola transacción, recorriendo las transiciones legales de RF4 y RF9**: `INSERT` en `scheduled`, secreto, `betting_open`, `betting_closed`, `live` y `settled`, y borra el secreto. Los triggers lo dejan, y el runner no ve estados intermedios. Datos reales: specs y logs de los dorados, sin el salto de línea final. Filas: KO, `log-decision-a`, empate, `voided(engine_error)`, `cancelled(missed_window)`, `cancelled(fight_unrunnable)` y una `scheduled` lejana para RF23. `CONTENT_DIR` va sin personajes.
- **Manipulación:** `page.route('**/api/fights/*')` intercepta el `fetch` del cliente (RF18), y cambia un byte de `log`, `seed` o `seed_commit`, o pone `final: null` o `state: "live"`. Para la versión desconocida también recalcula `snapshot_sha256`, porque `ruleset_version` está en RF2.
- `csp.spec.ts` lleva el caso B4.1, como pide ADR-0012, y solo corre en la configuración sembrada (`grep`).

Unitarias: `verify.ts` (RF11 a RF14), `load` de las páginas, artefacto (RF3). Integración: `fights_api.rs`, `fights_no_leaks.rs`, `fights_public_verify.rs`, `engine-wasm/tests/codes.rs`.

## Trabajo de cierre
- `docs/architecture.md`: el `.wasm` commiteado y `just gen-wasm`, `ruleset_bounds` (hoy dice que exporta solo `simulate`), `GET /api/fights`, la CSP y la frase de repeticiones de "Ciclo de vida".
- Spec de 003: enmienda de RF19 por RF4 y RF3 (dos funciones, con la superficie ya comprobada). Glosario: `Veredicto`, y ajustar `Repetición` ("a partir de su log").
- `CLAUDE.md`: `just gen-wasm`, `_check-wasm-artifact` en la lista de `just check`, y `playwright.replays.config.ts` en `just e2e`.
- `apps/server/README.md`: `GET /api/fights`.
- `docs/architecture.md`, prerrequisitos del primer despliegue: un rollback de prueba llega hasta `verify`. No se puede probar en este PR: `workflow_dispatch` solo existe desde la rama por defecto y exige un tag ya publicado, y `check-workflows` fija la forma mientras tanto *(Enmienda plan: D4)*.
- No hay tag ni deploy (TA-8 sigue pospuesto).

## Alternativas consideradas
- **Compilar el `.wasm` en una etapa Rust de `apps/web/Dockerfile`.** No deja un binario en el repo, pero no basta: `svelte-check`, Vitest, `just dev` y los jobs `web`, `e2e` y `e2e-fights` también lo necesitan, así que todos pasarían a requerir Rust, el target y el CLI, que hoy no tienen. Además, el `Dockerfile` copiaría `crates/`, `Cargo.*` y `rust-toolchain.toml`, con una etapa nueva que `check-compose` tendría que vigilar, y compilaría en cada build de imagen. Y seguiría sin ser el mismo binario en todos lados, salvo pasarlo entre jobs. Commitear lleva la complejidad a una sola comprobación.
- **Generar en `pnpm build` o en `_web-prepare`.** Igual que lo anterior, con Rust como requisito de todo lo de web.
- **`wasm-pack`.** Otra herramienta más sobre el mismo `wasm-bindgen`, sin nada que necesitemos.
- **Sin `wasm-bindgen`, exportando funciones a mano.** Choca con `unsafe_code = "forbid"` (RF19 de 003).
- **Comprobar la frescura por hash de las entradas, sin compilar.** No necesita CLI, pero no prueba que el binario salga de esas fuentes, que es justo lo que importa de un binario sin leer.
- **SHA-256 en el `.wasm` o en TS.** Funcionaría sin contexto seguro, y obligaría a descargar el `.wasm` para peleas sin log, o a mantener un hash propio, por un caso que no existe en producción.
- **Un Web Worker.** Exige `worker-src 'self'` (B4.3), y la simulación tarda milisegundos.
- **Pasar el `FightDetail` completo por el SSR.** Una lectura menos, pero mete hasta 50 KB en el HTML, y la respuesta no se podría interceptar en el e2e. También comprobaría lo que pasó por `web`, no lo que dio el server.
- **Cursor por `settled_at` con índice parcial.** Da "lo último en terminar" exacto, pero suma una migración y un índice por un caso raro (preocupación 9). Es solo una adición, si un día hace falta.
- **Barra de progreso con la cota de 90 s, o con el fin real.** La primera salta y se lee mal (ya descartada en 010). La segunda es el spoiler del intent.
- **Velocidad 2×.** Se podría hacer sin tocar el renderer, con el reloj del host, pero suma estado y controles sin que el intent lo pida.
- **Reproducción por texto sin WebGL.** Duplicaría un reloj de reproducción en `apps/web`, sin que el intent lo pida.
- **Mostrar el resultado en la lista con un interruptor.** Serviría al apostador, que todavía no existe (TA-27). Se puede sumar con esa feature, sin rehacer nada.
- **Un `settled_at` en `FightSummary`.** Cambia un tipo de WS por una fecha que `live_started_at` ya da.

## Cambios por revisión (2026-11-02)
| Hallazgo | Qué cambió | Dónde |
|---|---|---|
| product-owner 1 | Límite reescrito, con en y es. Lo de "probar varias y sellar" va en `<details>` | RF16 |
| product-owner 2 | "Sello" en vez de "comprometió", y explica semilla y sello. **No se usa "sellada antes de empezar"**: lo decidió quien coordina, porque el navegador no lo comprueba | RF16 |
| product-owner 3 | `unknown_final` tiene su texto y ofrece recargar | RF15, RF16 |
| product-owner 4 | Log inválido o ausente: botón de ver el resultado y llave propia | RF21, RF24 |
| product-owner 5 | Comportamiento de cada control, y +10 s sin apagarse antes del final | RF19 |
| product-owner 6 | La tarjeta no se enlaza a sí misma | RF26 |
| product-owner, nota | Sin cambios: queda para una fase posterior, con datos de uso | — |
| security 1 | Matcher `fight_id`, y lecturas que reciben números | RF10, RF18, RF23 |
| security 2 | `expected_status` sin comodín, caso positivo en `Settled` y barrido con `before` | RF9 |
| security 3 | Pasos `data` y `state`, `verifyFight` nunca rechaza, segunda excepción de RF14 | RF11, RF13, RF14, preocupación 7 |
| security 4 | `match` solo afirma lo comprobado, y el límite queda a la vista | RF16, preocupación 6 |
| security 5 | RF2 en el release, y el `.wasm` emitido igual al commiteado | RF2, RF5, preocupación 1 |
| security, menores | Recargar tras una trampa; superficie fijada y `check-engine`; `compare-trees` con nombre; `${CARGO_HOME:-…}`; `id` de la URL y resultado del `FightDetail` | RF15, RF3, RF2, RF1, RF13, RF18 |
| security, preocupación 4 | Ronda 1, sin cambios: el propio revisor acepta ese riesgo tal como está descrito. Ronda 2 corrige dónde corre, abajo | Preocupación 4 |
| security, rondas 2 y 3 | `wasm-artifact` aparte, sin `if` de job y con cada paso condicionado a `push`; `build` con `needs` y sin `if`, y `deploy` y `verify` sin cambios, para que un rollback siga desplegando | RF2, preocupaciones 1 y 4, "Cómo obtiene cada entorno" |
| security, ronda 2, menor | La forma no rechaza un `final.kind` o `reason` desconocidos, con vectores | RF13, RF14 |
| product-owner, ronda 2 | "Su huella publicada" en `snapshot`, y una `cancelled` sin frase sobre semillas ni `seed` y `seed_commit` | RF16 |

## Enmienda (plan, 2026-11-02)
Sale de la revisión de [plan.md](./plan.md), cuyos puntos conservan aquí su número. Cada cambio está también en su RF o sección, con la marca *Enmienda plan*. La spec sigue aprobada: esta enmienda no reabre el resto y no toca ningún ADR.

| Punto | Qué cambia | Dónde | Por qué | Revisa |
|---|---|---|---|---|
| D1 | `FightStage` suma tres ajustes más, solo si el host pasa `onsnapshot` | RF19 | Sin `watchContext` fuera de `live`, RF21 no se cumple; sin seguir leyendo tras `finished`, reiniciar se queda sin datos | — |
| D2 | El verificador nativo de pruebas exige la semilla en minúscula | RF14 | Evita una tercera diferencia sin declarar. Solo endurece | security-reviewer |
| D3 | WebKit en CI cubre la rama sin animación, y sus controles se comprueban fuera de CI | "Estrategia de pruebas" | WebKit en Linux no tiene WebGL | product-owner |
| D4 | El rollback real pasa a los prerrequisitos del primer despliegue | "Trabajo de cierre" | No se puede ejecutar antes. **Cambia lo que anotó security-reviewer** ("probarlo con el PR") | security-reviewer |
| D5 | Vectores construidos con los bytes del motor, sin Postgres | RF14 | Un archivo estable, sin `id` ni hora de una corrida. `every_kind_of_final_verifies_with_public_data` sigue cubriendo la lectura real | — |
| D6 | Límite del job `rust`: 25 min | Preocupación 4 | Una corrida de `main` ya tarda 14 min 10 s | devops-engineer, en el PR |
| Architect | Una respuesta sin forma solo da `mismatch(data)` | RF24 | Ni la spec ni el plan decían qué se dibuja en ese caso | security-reviewer |

## Enmienda (merge de main, 2026-11-06)
Al traer `main` antes del PR cambiaron cuatro premisas. La spec sigue aprobada y ningún ADR cambia.

| Qué cambia | Dónde | Por qué |
|---|---|---|
| La corrida sembrada (`playwright.replays.config.ts`) y todo `_e2e-fights-run` corren **solo en local**, con `just e2e`, y su salida va en el PR. Ya no hay job `e2e-fights` | "Estrategia de pruebas", preocupación 12, tabla de RF2 | TA-56 lo sacó de CI: sin GPU, el 3D por software lo volvía inestable. El caso B4.1 de `csp.spec.ts` (evidencia de ADR-0012 B6) queda también en local |
| La repetición usa la vista 3D por defecto de `FightStage` (feature 013), sin cambios en `packages/renderer`. "Pixi solo por `import()`" se lee como "el renderer (Pixi o Three) y el modelo `.glb`" | RF5, RF18 | TA-50 hizo del 3D la vista por defecto |
| El motor conoce el ruleset 2 (feature 016), así que el "ruleset desconocido" de los vectores, los tests y el e2e pasa a ser el 3. El verificador acepta cualquier versión de `Ruleset::for_version` | RF14 | El server sigue programando solo peleas v1 |
| *(2026-11-07)* `voided(secret_lost)` deja de ser un final desconocido. `verifyFight` le da `not_applicable` solo si no hay semilla ni log, hay commit y la huella coincide, como el resultado propio de RF21 de 012; si no, `mismatch`. `/fights/{id}` muestra su aviso y no muestra la comprobación ni la semilla, salvo un `mismatch` o un `cannot_check`. El "motivo desconocido" de los vectores pasa a ser `meteor` | RF13, RF14, RF22 | TA-27 (012) agregó `secret_lost`, y su plan le deja a quien fusione después el estado en la repetición |

## Revisión
### architect — 2026-11-02 — Enmienda (revisión del plan)
- Ver "Enmienda (plan, 2026-11-02)". La revisé contra `plan.md`, `FightStage.svelte`, `public_verifier.rs`, `release.yml` y `check-workflows.mjs`, sin compilar. Sin cambios en ningún ADR. D2, D4 y la fila de architect son para security-reviewer, y D3, para product-owner: quien coordina les pasa esas filas. Yo no apruebo esta enmienda.

### architect — 2026-11-02 — atendí la tercera ronda
- Apliqué en RF2 el arreglo de security-reviewer y quité el `if` de `build` que agregué en la ronda anterior: no bastaba, porque un `skipped` en la cadena de `needs` habría omitido `deploy` y `verify` en un rollback. No ejecuté el workflow. No la apruebo yo.

### architect — 2026-11-02 — atendí la segunda ronda
- Ver las tres últimas filas de "Cambios por revisión". Revisé `release.yml` y la tabla de permisos de `check-workflows`, sin ejecutar el workflow. Sumé algo que el pedido no decía: un `needs` omitido omite a `build`, así que necesita su propio `if`, y hay que probarlo en un rollback real o con su caso en `check-workflows`. Product-owner ya había aprobado: sus dos retoques no cambian lo aprobado, pero aprobar otra vez o no es decisión suya. No la apruebo yo.

### architect — 2026-11-02 — atendí las dos revisiones
- Ver "Cambios por revisión". Comprobé contra el código: `release.yml`, `compare-trees.mjs`, el comodín de `expected_status` y el texto de `fight_notice_playback_error`. En el `.wasm` medido, las funciones propias son `simulate`, y el grafo no tiene `js-sys` ni `web-sys`. Sin compilar nada nuevo ni correr pruebas. No la apruebo yo: pide otra ronda de los dos revisores.

### architect — 2026-11-02 — autor
- La escribí contra el código de esta rama, sin implementar nada. Medí en una carpeta temporal, con `rustc 1.99.1` y `wasm-bindgen-cli 0.2.129`: tamaño del `.wasm`, bytes de `cargo` en macOS ARM, `linux/amd64` y `linux/arm64` con el remap y sin él, los de `wasm-bindgen` en macOS ARM y `linux/amd64`, y `simulate` en Node con dos dorados. No medí el tiempo del job `rust` en CI, ni corrí nada en un navegador. Pendiente: product-owner y security-reviewer. Yo no la apruebo.

### product-owner — 2026-11-02 — REQUEST_CHANGES
- Verifiqué: los 10 criterios y las 9 preguntas abiertas del intent tienen respuesta y prueba; respeta el alcance del MVP y ADR-0004, 0005 y 0007. Revisé el texto de RF16 y RF22 contra la preocupación 1 de 010, y `FightHome` y los mensajes actuales de `apps/web`.
- De acuerdo, sin cambios: la lista con solo "disputada", "cancelada" o "anulada" protege a quien la quiere ver sin saber, y el apostador no existe hasta TA-27. Los cinco controles caben a 375 px y bastan para ver, reiniciar y adelantar. Empezar en pausa y sin barra es coherente con RF55.
- Cambios pedidos:
  1. **RF16, el límite honesto está mal dicho.** "No demuestra que la semilla no se eligiera antes de comprometerla" no se entiende, y leído al pie de la letra describe lo normal: toda semilla se elige antes de su commit. El riesgo real es otro (preocupación 1 de 010, "una semilla elegida es indistinguible de una al azar"). Usar: "Esto no demuestra que la semilla saliera al azar: quien controla el sitio pudo probar varias y sellar la que le convenía. Tampoco que estos datos sean los originales, si nadie guardó una copia." Y dar su versión en inglés, que hoy falta en la spec, igual que el resto de esa frase.
  2. **RF16, `match` de `fight` y lenguaje.** Es el texto que más gente va a leer y omite justo lo que da credibilidad: que la semilla ya estaba fijada antes de la pelea. Además, "semilla" no se explica, y "comprometió" es un calco de *commit* que en español significa otra cosa. Usar "sellada", también en `match` de `commit` y en la línea de `mismatch`. Para `fight`: "Coincide. La semilla (el número que decide cómo sale la pelea) es la que quedó sellada antes de empezar, y con ella tu navegador volvió a simular la pelea: salió lo mismo, hasta el resultado." / "It matches. The seed (the number that decides how the fight goes) is the one sealed before it started, and with it your browser simulated the fight again: same thing, result included."
  3. **RF15 y RF16, `unknown_final`.** Cae en el texto técnico ("tu navegador no pudo hacer la prueba") con botón de reintento, pero el navegador no falló y reintentar nunca sirve: es un final que esta versión del sitio no conoce. Tratarlo como `ruleset_unknown`: ofrecer recargar, con un texto que diga eso y no nombre "reglas".
  4. **RF20, RF21 y RF24, spoiler al fallar la animación.** "Con el resultado" no dice si se muestra solo, y `fight_notice_playback_error` promete "El resultado aparecerá aquí", que en una repetición es falso si va detrás del botón. Definirlo: el log inválido o ausente muestra el botón "ver el resultado", como sin WebGL, con una llave propia que no diga eso.
  5. **RF19, comportamiento de los controles.** Definir que los saltos conservan reproducir o pausa, qué estado deja reiniciar, y que +10 s nunca se desactiva antes del final: un botón apagado delataría cuánto queda (RF20).
  6. **RF26, enlace a sí misma.** `FightResultCard` también se usa en `/fights/{id}` (RF20), donde esa pelea está `settled`: con esta regla se enlazaría a su propia página. El enlace no va allí.
- Nota para una fase posterior, no bloquea: sin resultado ni filtros, la lista no ayuda a encontrar las peleas buenas. Conviene mirarlo con datos de uso, junto al interruptor de resultado de TA-27.

### product-owner — 2026-11-02 — APPROVE (segunda ronda)
- Verifiqué solo lo que pedí y "Cambios por revisión": mis puntos 1, 3, 4, 5 y 6 quedaron resueltos en RF15, RF16, RF19, RF21, RF24 y RF26. El nuevo RF18, que saca el final del HTML, también ayuda contra los spoilers.
- Punto 2: acepto que no se use "sellada antes de empezar". El navegador no comprueba esa anterioridad, y mi texto prometía de más. El `match` de `fight` y el límite siempre visible dicen lo mismo sin afirmarlo: se publica antes, esta página no lo prueba y quien lo guardó puede compararlo. "Semilla" y "sello" quedan explicados, y coinciden con "Semilla sellada" de `/`. Se entiende; es largo, pero aceptable, porque "Coincide." se lee primero.
- Para pulir en el plan, no bloquea (RF16 lo permite):
  - `match` de `snapshot` y la línea `snapshot` de `mismatch` usan "su sello" sin explicarlo, y en esa pelea "sello" no es el de la semilla. Mejor "su huella publicada".
  - El `<details>` va bajo todo veredicto, pero en una `cancelled` no tiene sentido hablar de semillas, ni listar `seed` y `seed_commit` vacíos. Allí, omitir esa frase y esos campos.

### product-owner — 2026-11-02 — APPROVE (enmienda: D3 y RF24)
- Verifiqué solo esas dos filas, contra RF18, RF24 y D3 de `plan.md`. Solo leí; no corrí pruebas.
- D3: lo apruebo. Chromium cubre los controles en CI, y WebKit, la rama sin animación. Condición para cerrar el PR, porque el espectador está en el celular: la comprobación fuera de CI se hace en WebKit con WebGL y a 375 px, y su resultado va en el PR. Después del PR, ningún CI cubre esos controles en WebKit: riesgo aceptado.
- RF24: lo apruebo. Sin reproductor, resultado ni botón no se muestra nada que no sea fiable, y el cruce y la fecha siguen saliendo del SSR. Al implementar, el `<details>` de RF16 no lista campos de esa respuesta.

### security-reviewer — 2026-11-02 — REQUEST_CHANGES
- Verifiqué, contra el código de esta rama: `settled_page` no puede traer semilla ni log (`summary()` no usa `revealed_seed`, `log` ni `spec_*`, y `FightSummary` no los tiene); el cursor y `next_before` solo devuelven ids de filas `settled` y los ids ajenos ya son públicos por RF34, así que no hay canal lateral nuevo; RF23 y RF18 no pasan datos de más al HTML. `FIGHT_READ_IP` con `IpKey` (`X-Client-Ip` desde `handleFetch`), `no-store` en el 429 y consulta acotada: suficiente, sin cambios. CSP: cumple B3, B4.1, B6 y el trabajo 10 de ADR-0012, y `check-kit-config` ya admite la fuente. `check-engine` sigue valiendo para `ruleset_bounds` (lee todo `src/`). `.wasm`: `rust` corre `just check-rust` y falla cerrado con `CI`; `compare-trees` detecta archivos sobrantes y faltantes. `simulate`: sin una coincidencia falsa con `ruleset_version` fuera de `u16` (trunca, pero RF2 y `summary` lo atrapan). XSS: sin `{@html}` previsto, Svelte escapa.
- Cambios pedidos (**Important**, bloquean):
  1. **RF18 y RF23: `params.id` llega sin validar al cliente interno.** RF17 valida `before`; RF18 no valida `id`, y Kit decodifica el parámetro (`routing.js`). `getFightDetail` concatena en `apiRequest` y `handleFetch` agrega `X-Internal-Secret`, la IP y la cookie. Explotación: un enlace a `/fights/..%2Fauth%2Fgoogle%2Fcallback%3Fcode%3Dx%26state%3Dy` hace que el SSR pida `/api/auth/google/callback?...`, una ruta `Effect::Mutate`, como la víctima y con el secreto interno. También sirve para cualquier otro `GET`. Pedir: un matcher de Kit (`[id=fight_id]`) o el mismo decimal canónico de RF34, antes de cualquier lectura, y si no, 404 sin tocar el API. `getFightDetail` recibe un número, no una cadena. → test de `load` con `..%2Fme`, `12%3Fx`, `01` y `2147483648`, sin llamada a `fetch`.
  2. **RF9: `expected_status` no es exhaustivo, como dice la spec.** Termina en `_ => None` (`fights_no_leaks.rs:200`), así que `Handler::FightsList` compila sin caso y el barrido da por limpio el cuerpo de un 401, un 404 o un 429, sin haber llegado al handler. Y antes de `Settled` la lista está vacía, así que el barrido no prueba nada aunque responda 200. Pedir: agregar `FightsList` con 200 para las tres identidades, quitar el comodín, y en `Settled` afirmar que `fights` sí incluye la pelea, para que la búsqueda de canarios no pase en vacío. También un barrido con `?before=<id+1>`.
  3. **RF13 omite dos pasos del verificador nativo, pese a "sin omitir pasos".** `verify` empieza con `state != Settled` y `final` nulo, ambos `Err`. La tabla no los tiene, `check` no tiene valor para ellos y el `switch` sobre `final.kind` revienta con `null`, o queda en "comprobando". Explotación: una respuesta alterada con `final: null` o `state: "live"` evita un "no coincide"; rompe además RF14, cuya única excepción declarada es la versión desconocida. Pedir: esos dos pasos, con un `check` nuevo (`state`) o con `final`; validar la forma del JSON en `load-detail.ts` (tipos y `ruleset_version` entero de 0 a 65535), con `mismatch` si no cumple; y que ninguna excepción de `verifyFight` deje el estado en "comprobando". Declarar `unknown_final` como segunda excepción de RF14 y de la preocupación 7, porque también baja un "no coincide" a un "no se pudo comprobar".
  4. **RF16: dos `match` afirman lo que el navegador no comprobó, y lo mismo el texto pedido por product-owner.** El navegador compara `seed` con un `seed_commit` y los specs con un `snapshot_sha256` **de la misma respuesta**. Explotación: quien reescribe la fila con semilla, commit y log coherentes (preocupación 2 de 010) obtiene "es la que se comprometió antes de la pelea" (o "quedó sellada antes de empezar"), en el texto principal, y lo falso solo se matiza dentro de un `<details>` cerrado. Pedir: decir solo lo comprobado, por ejemplo "La semilla coincide con el sello publicado para esta pelea, y con ella tu navegador volvió a simular la pelea…", y "Los personajes publicados coinciden con su sello". El límite de `<details>`, con la redacción de product-owner, suma: "Que el sello sea anterior a la pelea solo se comprueba si alguien lo guardó antes de empezar", y conserva la frase sobre el código que entrega este sitio. Lo mismo en en. Agregar el Vitest a las llaves.
  5. **El `.wasm` de la imagen de release no pasa por RF2.** La preocupación 1 apoya la garantía en `ci-ok`, pero ese check protege el PR, no el tag: `release.yml` no corre `_check-wasm-artifact`, no comprueba el estado de CI del commit ni que esté en `main`, y solo valida el formato del tag. Explotación: dos PR que tocan `crates/engine` pasan por separado y dejan `main` con un binario viejo, o se etiqueta un commit de otra rama; la imagen publica un `.wasm` que no sale de sus fuentes, sin ningún chequeo, y el fallo en `main` no lo detiene. Pedir: `_check-wasm-artifact` en el job `build` de `release.yml`, antes del push, con su regla en `check-workflows`. Y en `check-renderer-split`, que el `.wasm` emitido sea byte a byte el commiteado, para cubrir lo que hace Vite con él.
- Minor, no bloquean:
  - Tras una trampa del `.wasm`, el glue reutiliza la instancia (`init` regresa la existente), y el reintento de RF15 corre sobre memoria sin desenrollar: puede dar un `mismatch` falso, contra RF12. `engine_failed` debe ofrecer recargar, no reintentar.
  - RF3: fijar con `WebAssembly.Module.exports` e `imports` lo exportado (`simulate`, `ruleset_bounds`) y lo importado, y sumar a `check-engine` que `src/` no tenga bloques `extern` ni `js-sys` o `web-sys` en el grafo. Hoy nada hace cumplir "una sola función" de RF19 de 003 ni "no conoce el DOM" de los RNF.
  - `compare-trees.mjs` tiene "protocol-ts" y `just gen` fijos en sus mensajes: RF2 pide nombrar `just gen-wasm`, así que debe recibir el nombre, sin confundir.
  - RF1: `$CARGO_HOME` puede no estar definida; usar `${CARGO_HOME:-$HOME/.cargo}`, o el remap no aplica y filtra la ruta del home.
  - RF18: comprobar que `detail.fight.id` es el de la URL, y dibujar `FightResultCard` con el `final` del `FightDetail` verificado, no con el del SSR, para que el veredicto hable de lo que se muestra.
  - Preocupación 4 (CLI fuera de `crate-age` y `cargo deny`): riesgo residual aceptable como está descrito.

**🤖 security-reviewer**

### security-reviewer — 2026-11-02 — REQUEST_CHANGES (segunda ronda)
- Verifiqué, solo mis hallazgos y "Cambios por revisión": 1 (matcher antes de leer, lecturas con números), 2 (`expected_status` sin comodín, caso positivo en `Settled`, barrido con `before`), 3 (pasos `data` y `state`, `verifyFight` que nunca rechaza, excepciones de RF14 y tipo `Verdict`) y 4 (`match` solo dice lo comprobado y el límite está a la vista) quedan resueltos, igual que los menores. De 5 queda bien el `.wasm` emitido igual al commiteado (RF5), y el motivo de RF2.
- Cambios pedidos:
  1. **Important. RF2 pone la comprobación en un job con permiso de escritura en GHCR.** El job `build` de `release.yml` tiene `packages: write` y `docker/login-action` en la línea 68, que deja el token en `~/.docker/config.json`. `_check-wasm-artifact` compila y corre `cargo install wasm-bindgen-cli`, con scripts de build de crates que, según la preocupación 4, `just crate-age` y `cargo deny` no ven. Explotación: una dependencia comprometida de ese CLI lee el token en el tag, o deja un proceso que espera al login si el paso va antes, y publica una imagen de `web` con otro `.wasm` u otro JS, justo lo que esta comprobación quiere evitar. Pedir: un job aparte (por ejemplo `wasm-artifact`) con solo `contents: read`, sin caché restaurada y solo en un tag, del que `build` dependa con `needs`, sin romper el rollback, que lo omite. `check-workflows` exige esa forma y su fila en la tabla de permisos. Corregir también la preocupación 4 ("solo corre en desarrollo y en CI") y la fila del release de "Cómo obtiene cada entorno".
- Minor, no bloquea: la validación de forma de RF13 no debe rechazar un `final.kind` o `reason` desconocidos, o RF22 nunca llegaría a `unknown_final`. Un vector de RF14 lo fija.

**🤖 security-reviewer**

### security-reviewer — 2026-11-02 — REQUEST_CHANGES (tercera ronda)
- Verifiqué, solo lo pedido: la forma de `wasm-artifact` (permisos, sin caché ni login, mismo `github.sha`, sin el nombre del tag en un `run:`); el `if` de `build`, que en un tag solo pasa con `success`; la preocupación 4, ya correcta y aceptable; y RF13 y RF14 (`data` y `state`, `final` desconocido que llega a `unknown_final` y excepciones marcadas), sin observaciones. No ejecuté el workflow.
- Cambios pedidos:
  1. **Important. RF2: el `if` de `build` no basta, y un rollback no desplegaría.** "`deploy` y `verify` siguen dependiendo de `build` sin cambios" es el problema: la condición implícita `success()` de un job mira toda su cadena de `needs`, no solo el directo (comportamiento conocido de GitHub Actions, que no comprobé aquí en un run real). Escenario: en un incidente, el owner lanza un `workflow_dispatch` con el tag anterior; `wasm-artifact` queda `skipped`, `build` pasa con su `if` y `deploy` y `verify` se omiten, sin fallo visible, y el `.wasm` o la versión mala sigue en producción. Pedir lo más simple y que falla cerrado: que `wasm-artifact` **no tenga `if`** y corra siempre, con cada uno de sus pasos en `if: github.event_name == 'push'`, igual que los de `build`. Así `build` lleva `needs: wasm-artifact` sin `if`, nunca hay un `skipped`, y `deploy` y `verify` no cambian. `check-workflows` exige esa forma: sin `if` de job en los tres, y `_check-wasm-artifact` en un paso con esa condición exacta y sin `continue-on-error`. Si se prefiere mantener el `if`, `deploy` y `verify` necesitan el suyo con `needs.build.result` y `needs.deploy.result`, y también su regla.

**🤖 security-reviewer**

### security-reviewer — 2026-11-02 — APPROVE (cuarta ronda)
- Verifiqué, solo el cambio en RF2 y contra `release.yml`: `wasm-artifact` sin `if` de job nunca queda `skipped`, así que la condición implícita de `needs` deja pasar el rollback hasta `verify`; `github.event_name == 'push'` solo es falso en `workflow_dispatch`, donde todos los pasos que construyen o publican de `build` ya llevan esa misma condición, así que un rollback omite la comprobación sin publicar nada; en un tag, un fallo o una cancelación detienen `build`, `deploy` y `verify` sin una expresión propia, y `check-workflows` fija la forma. No ejecuté el workflow: el comportamiento de un rollback real queda para probarlo con el PR. Con esto quedan cerrados mis hallazgos de las rondas anteriores. Sigue pendiente el gate de `risk: high` del owner antes de implementar.

**🤖 security-reviewer**

### security-reviewer — 2026-11-02 — APPROVE (quinta ronda: enmienda del plan)
- Verifiqué, solo D2, D4 y RF24, contra `public_verifier.rs`, `fights/mod.rs`, `release.yml` y `plan.md`, sin ejecutar nada.
- **D2:** de acuerdo. `hex_decode` acepta mayúsculas y `+a`, y `fights::hex` solo emite minúsculas: solo endurece un verificador de pruebas, y un vector lo fija.
- **D4:** de acuerdo, y reemplaza mi nota de la cuarta ronda ("probarlo con el PR"). Sin un tag publicado no hay nada que revertir, ni producción que quede expuesta; `check-workflows` fija la forma hasta entonces, y el prerrequisito bloquea el primer despliegue.
- **RF24:** de acuerdo. Una respuesta sin la forma no da reproductor, resultado ni botón, y `unavailable` sigue sin veredicto, así que un fallo de red no parece manipulación.
- Minor, no bloquea: `fight.id` distinto del de la URL y `state` distinto de `settled` pasan `parseFightDetail`, que no los mira. Darles el mismo trato en la vista (solo el `mismatch`, sin reproductor ni resultado), con un Vitest. No hay fuga, porque el server no manda el log antes de `Settled`: solo afecta a una respuesta alterada.

**🤖 security-reviewer**
