---
id: "008"
title: "Autenticación y sesiones, fase B: esquema, ledger, sesiones, proveedor dev y /ws con sesión"
type: plan
status: approved
risk: high
author: backend-engineer
reviewers: [architect]
approvals:
  - by: architect
    date: 2026-10-28
linear: TA-26
depends_on: ["001"]
created: 2026-10-28
updated: 2026-10-28
---

# Plan: Autenticación y sesiones, fase B (desde spec.md 2026-10-28)

> [intent](./intent.md) · [spec](./spec.md) · [plan-a](./plan-a.md) · ADRs [0001](../../decisions/0001-motor-propio-determinista.md), [0004](../../decisions/0004-personajes-homenaje.md), [0005](../../decisions/0005-economia-sin-dinero-real.md), [0006](../../decisions/0006-despliegue-y-ci.md) (D1, D2, D4), [0007](../../decisions/0007-i18n-desde-cero.md), [0008](../../decisions/0008-autenticacion.md) (D1, D2, D3, D5, D6, D7, D8, D10, la "Nota (2026-10-28)" y la "Enmienda 2026-10-28"), [0012](../../decisions/0012-logs-y-cabeceras-de-seguridad.md) (A4, A8, A9)

**Alcance.** RF-B1 a RF-B18 de la spec, más los RNF de memoria, CPU, determinismo, i18n (solo códigos del server) y costo que aplican a B. De la estrategia de pruebas: T1 (extendido), T4 adaptado, T5, T10, T11, T13 extendido, T17, T19 y T20 con el proveedor `dev` (sin la concurrencia del renombre, que es de C), T22 (parte de usuario), de T18 el mensaje de más de 4 KB, la suscripción 9.ª y el socket con sesión 201 desde la misma `/64`, y de T28 el `ip_hash` del `/64`. También pliega las notas no bloqueantes de la revisión del PR #53 que tocan B (decisión 3) y la nota de seguridad sobre "fuera de loopback" (decisión 5).

**Fuera de alcance.** Google y Discord, vincular y desvincular, renombre, preferencias, CLI de admin y ban (fase C). `handleFetch`, pantallas, e2e, CI y docs (fase D). No toca `crates/engine`, `crates/engine-wasm`, `crates/sim`, `crates/content` ni `packages/renderer`. Ningún workflow cambia (decisión 22).

**Revisores del PR de la fase B.** Además de architect (este plan):
- **security-reviewer:** `risk: high`, y la fase toca D2, D3, D5, D6, D8 y A9.
- **game-designer:** las listas del alias (`content/names/`, checklist de ADR-0004 y sin palabras ofensivas) y el ledger con el `grant` de `DAILY_GRANT` (RF-B2, RF-B3).
- **frontend-engineer:** `apps/web` cambia en tres puntos chicos (decisiones 4 y 5): los casos nuevos del `switch` de `echo.ts`, `isLoopbackHttpOrigin` en `validators.ts` y el `.env.example` de web.
- **devops-engineer:** `infra/docker-compose.yml`, `infra/local.env`, `scripts/check-compose.mjs`, `apps/server/Dockerfile`, `.dockerignore` y el `justfile` (decisiones 6, 18 y 22).
- **qa:** como en A, la verificación de mutaciones.

## Para el architect: lo que tienes que decidir
1. **`oauth2` y el cliente HTTP.** *Resuelta por el architect (cambio pedido 3): `oauth2` sin `reqwest`, con `state` y `code_verifier` de `OsRng` (decisión 1).* D2 nombra el crate `oauth2`, y el RNF de costo nombra `reqwest`. `oauth2 5.0` con `default-features = false` todavía trae `chrono` y `rand 0.8` (una tercera versión de `rand` en el lock). Mi propuesta: `oauth2` sin features y su trait `AsyncHttpClient` implementado sobre el cliente de `hyper-util` (`client-legacy`, ya en el lock), sin `reqwest`. En B solo hace falta HTTP a `127.0.0.1`. La fase C agrega `hyper-rustls` con `ring` y `webpki-roots`, que ya están en el lock por `sqlx`. Si `just crate-age`, `just deny` o `cargo tree -d` muestran que `oauth2` cuesta demasiado, la alternativa es escribir el flujo a mano (URL de autorización, `POST` al token y `GET` a `userinfo`, unas 150 líneas con PKCE sobre `sha2` y `base64`). Eso se aparta de D2 y necesita tu visto bueno.
2. **El tipo de `Me.balance`.** El contrato de `crates/protocol` prohíbe enteros de 64 bits en los tipos exportados, porque `ts-rs` los exporta como `bigint`. Los montos del ledger son `bigint` en la BD. Propongo `balance: i64` con `#[ts(type = "number")]` y una guarda en el server: si `|saldo| > 2^53 − 1`, el server responde 500 sin cuerpo propio y nunca trunca. El script de `just gen` necesita una excepción declarada para ese campo. La alternativa es `i32`, que trunca a 2 147 483 647 monedas y obliga a migrar el tipo si algún día se acerca.
3. **Cookie de sesión inválida en el upgrade del `/ws`.** RF-B8 da 401 en HTTP, pero el navegador no puede leer el cuerpo de un handshake rechazado. Propongo que el socket entre como **anónimo** si la cookie no valida en el handshake: cuenta en los límites anónimos y no puede suscribirse a `self`. Si la sesión cae entre el registro y la segunda validación (RF-B14), el socket se cierra con `1008` y `auth.unauthenticated`.
4. **Secretos de `just prod-local`.** *Resuelta por el architect (cambio pedido 1):* no van fijos en git. `just prod-local` los genera en el primer uso en un archivo ignorado (decisión 6).
5. **`DAILY_GRANT` en el compose.** Propongo el literal `DAILY_GRANT: '100'` en `infra/docker-compose.yml`, para que el valor de la economía quede revisado en git. `check-compose` exigiría la llave con un entero de 1 a 10 000, no un valor exacto, así TA-27 lo cambia sin tocar el script. `IP_HASH_KEY` e `INTERNAL_API_SECRET` vienen de Coolify (`${…:?}`).
6. **Prueba de A9 sobre el log de Postgres.** *Resuelta por el architect (cambio pedido 2), con la violación de unicidad y el control positivo (decisión 13).* La estrategia de pruebas pide revisar el log del Postgres de test. Los IT no ven el log del contenedor. Propongo un paso `_check-pg-log` en `check-rust`, justo después de `cargo test`. El paso lee `docker compose -f infra/compose.dev.yml logs --since <inicio> postgres` y falla si aparece el prefijo canario que un IT escribe a propósito en un `NOTIFY`, en un `INSERT` que el trigger rechaza y en un asiento descuadrado. Necesita Docker, igual que el resto de `check-rust`.
7. **El proveedor `dev` frente al middleware de `Origin`.** El cliente OAuth llama a `POST /api/auth/dev/oauth/token` desde el mismo proceso, sin `Origin`, y `GET /api/auth/dev/oauth/authorize` guarda un `code` en memoria (un `GET` que muta). Propongo, solo con la feature `dev-auth`: `GET …/authorize` como segunda excepción declarada de `SAFE_METHOD_EXCEPTIONS`, y `POST …/token` exento del chequeo de `Origin` con una lista de exenciones explícita y probada (D4 habla de mutaciones del navegador, y el token se autentica con `code` y `code_verifier`). El UT sin la feature comprueba que las dos listas tienen solo la excepción de producción.
8. **Validación de secretos en `web`.** RF-B18 dice que los dos `.env.example` traen el mismo valor de desarrollo. RF-D1 dice que `web` valida `INTERNAL_API_SECRET` al arrancar en la fase D. Propongo que B agregue el valor al `.env.example` de web y el predicado `isLoopbackHttpOrigin` con su UT (decisión 5), pero **sin** conectarlo a `env.ts` hasta D. Si `web` lo exigiera ya, el smoke test de la imagen de web en el job `images` fallaría, porque el compose de producción no le pasa el secreto hasta D.

## Decisiones de implementación

### 1. Dependencias
Se fijan con `=` en `[workspace.dependencies]`, como las demás:
- `sha2 = "=0.11.1"` y `hmac = "=0.13.0"`: ya están en el lock por `sqlx` y comparten `digest 0.11`.
- `rand_core = { version = "=0.9.5", default-features = false, features = ["os_rng"] }`: ya está en el lock (es la línea de `rand_chacha 0.9`). Da `OsRng`. El índice al azar del alias usa muestreo por rechazo sobre `u32` (sin sesgo, con UT).
- `unicode-normalization = "=0.1.26"` (ya está), `unicode-segmentation`, `unicode-security` y un crate de casefold completo (`caseless`, si pasa `crate-age` y `deny`; si no, `unicode-casefold`). Las versiones exactas se eligen en el paso 1.
- `oauth2 = { version = "=5.0.0", default-features = false }`, más `hyper-util` con `client-legacy` (ya en el lock) como implementación de `AsyncHttpClient`. Sin `reqwest`, `curl` ni `ureq` (pregunta 1).
- **`state` y `code_verifier` salen de `OsRng`**, nunca del `rand 0.8` que trae `oauth2`. Se construyen con `CsrfToken::new` y `PkceCodeVerifier::new` desde 16 y 32 bytes de `OsRng` en base64url (el `code_verifier` de 43 caracteres cumple RFC 7636). Un UT de fuente falla si `new_random` aparece en `apps/server/src`. `rand 0.8` queda en el lock, pero ningún secreto lo usa.
- **Verificación previa de `oauth2` (2026-10-28, lectura de crates.io con `User-Agent` genérico y un proyecto de prueba fuera del repo).** `oauth2 5.0.0` se publicó el 2025-01-21 y no está retirado. Sin sus features por defecto, `cargo tree --target all` no contiene `openssl`, `openssl-sys`, `native-tls`, `aws-lc-rs`, `aws-lc-sys` ni `ring`. Los crates que agrega al workspace son `chrono 0.4.45` (2026-06-04), `iana-time-zone 0.1.65` y sus crates de plataforma (`core-foundation-sys`, `android_system_properties`, `iana-time-zone-haiku`, `windows-*`), `rand 0.8.8` (2026-08-25), `rand_chacha 0.3.1`, `rand_core 0.6.4`, `thiserror 1.0.69` y `serde_path_to_error`. La versión más reciente del árbol resuelto por separado es de 2026-10-23, con más de 3 días. **Pasa la regla de `crate-age` y la de `deny`**, así que no hace falta la enmienda de D2. El paso 1 lo repite sobre el lock real del workspace, y si algo falla se detiene y lo decide el owner (respuesta del architect a la pregunta 1).
- `axum` gana las features `form` y `query` (el formulario de `start` y la query del callback). Si `form` trae `serde_urlencoded` como crate nuevo, pasa por el mismo chequeo.
- Ninguna trae OpenSSL ni `aws-lc`.

Orden obligatorio, igual que en A: editar los `Cargo.toml`, `cargo update -w`, **`just crate-age` antes de compilar**, bajar lo que marque con `cargo update -p <crate>@<v> --precise <anterior>`, `just deny` y `cargo tree -d -p server` para ver los duplicados nuevos. Si `oauth2` agrega `rand 0.8` y `chrono`, se para y se decide la pregunta 1 antes de seguir.

**Ninguna petición de red lleva datos personales.** El cliente OAuth manda `User-Agent: torneo-absurdo-server` (genérico, sin versión ni host), no sigue redirecciones (evita SSRF), tiene un timeout de 10 s y corta las respuestas en 64 KB. Lo prueba un UT del adaptador con un server de prueba local.

### 2. Protocolo (`crates/protocol`)
- `AuthErrorCode` gana `InvalidState`, `Banned`, `ProviderError`, `TryAgain` y `Unauthenticated`. `AlreadyLinked`, `ProviderAlreadyLinked`, `LastProvider` y `ReauthRequired` llegan en C.
- `CallbackErrorCode`, un enum cerrado con su propio `as_str` y su test de conjunto, con los cuatro códigos de B: `auth.invalid_state`, `auth.banned`, `auth.provider_error` y `auth.try_again`. C agrega los dos de vinculación. Es el **único** tipo que puede ir en el `Location` del error del callback (Enmienda).
- `WsErrorCode` gana `UserCapacity`, `ForbiddenChannel` y `TooManySubscriptions`.
- `Channel` (solo `SelfChannel`, `"self"`), `ClientMessage::{Subscribe, Unsubscribe} { channel: String }`, `ServerMessage::{Subscribed, Unsubscribed} { channel }`, `ServerMessage::Error { code: ErrorCode }`, `ServerMessage::SelfEvent(SelfEvent)` y `SelfEvent::ProfileChanged { display_name }`.
- `Me { display_name, role, balance, providers, locale, theme, display_name_is_generated }`, `Role`, `Provider` (`google`, `discord`, `dev`), `Locale` y `Theme`. `balance` según la pregunta 2.
- El doc comment de `ErrorCode` se reescribe a ≤ 100 columnas y su ejemplo deja de decir "por ejemplo `auth.*`" (ya existe): la regla queda en abstracto, un dominio nuevo es un enum nuevo (nota de #53).
- Los tests del archivo se extienden como en A: `ALL`, conteos, prefijos sin solaparse, round-trip y la forma exacta de cada mensaje. Después, `just gen` y se commitea `packages/protocol-ts`.

### 3. Notas de la revisión de #53 que se cierran en B
1. **`routes::register` sin entradas mudas.** `RouteSpec` gana `handler: Handler`, un enum con una variante por ruta. `route_for(&RouteSpec, &AppState) -> MethodRouter<AppState>` es un `match` exhaustivo, sin `Option`: una entrada sin handler no compila. Se borra el UT `every_entry_has_a_handler`, que deja de tener sentido. `route_for` también monta las capas por ruta (rate limit, límite de cuerpo).
2. **`OriginPolicy` desde `Config`.** `AppState::from_config(config, settings, pool, port)` arma la política de `Origin`, `NetState`, `CookiePolicy`, los limiters y el proveedor `dev` desde la config. `AppState::new` desaparece. Los IT arman la config con `Config::from_lookup` sobre una tabla (`tests/support/config.rs`). `WsState::public_origin()` se queda solo para el handshake.
3. **Test de la carrera del tope, determinista.** Se quita el `recv_timeout(200 ms)`. Dentro del hook (con el lock compartido tomado), el test comprueba con `admission.try_write()` que ni una limpieza ni la admisión de una llave nueva podrían entrar (`#[cfg(test)] fn exclusive_would_block(&self) -> bool`), sin hilos. Después del chequeo corre, en secuencia, la limpieza y la llave nueva, y comprueba `Full` y `len() == 1`.
4. **`ErrorCode`:** ver decisión 2.
5. **`.env.example` del server:** el comentario de `TRUSTED_PROXIES` agrega que en producción el compose lo fija en `10.250.0.2/32` (ADR-0006 D2) y que no se configura en Coolify.
6. **`INTERNAL_API_SECRET` en la allowlist de `check-compose`:** decisión 6.
7. **`plan-a.md`:** se reordenan las desviaciones 7 y 8, y se corrige el riesgo de "Contención del mutex de admisión": desde la desviación 8, las llaves existentes toman el lock compartido y solo la admisión y la limpieza lo toman exclusivo. Es un cambio de doc en el commit del paso 3, sin cambiar ninguna decisión.
8. **6to4, Teredo y NAT64 como `/64` simples, y la latencia del lock exclusivo bajo ráfagas de `/64` nuevas.** Aceptados por la spec y acotados por el tope de llaves. B no los cambia. Quedan en Riesgos.

Las notas de la fase D de esa misma lista (el 503 que el navegador ve como 1006, el canario de `?error=` en los logs de web y el mapeo de los errores del proveedor) no se tocan aquí. Las notas de `just hooks` y de `core.hooksPath` son del clon local, no del repo.

### 4. `apps/web` en B (mínimo)
- `echo.ts`: los casos nuevos de `WsErrorCode` (`ws.user_capacity`, `ws.forbidden_channel` y `ws.too_many_subscriptions`) con sus llaves y textos en/es, y el caso `auth.unauthenticated` como motivo de cierre (`ws_close_reason_unauthenticated`), con sus filas en `echo.test.ts`. Sin el caso, `svelte-check` falla con `just gen`, igual que en A.
- `validators.ts`: `isLoopbackHttpOrigin(origin: string): boolean` y su UT (decisión 5). No se conecta a `env.ts` (pregunta 8).
- `.env.example`: `INTERNAL_API_SECRET` con el mismo valor de desarrollo que el del server (RF-B18). `web` todavía no lo lee.

### 5. "Fuera de loopback": un solo predicado (nota de seguridad de la spec)
- **Definición:** un origen está "en loopback" si y solo si `is_loopback_http_origin(PUBLIC_ORIGIN)` (de A): esquema `http` y host `localhost`, `127.0.0.1` o `[::1]`. "Fuera de loopback" es su negación. `https://localhost` y `http://127.0.0.2` quedan fuera. Lo usan RF-A3 (proxies de loopback), RF-B6 (proveedor `dev`), RF-B9 (política de cookies de desarrollo) y RF-B18 (secretos distintos del ejemplo). Ningún módulo reimplementa la regla: los cuatro llaman a la misma función, y un UT de fuente falla si `Host::Domain` o `Ipv4Addr::LOCALHOST` aparecen en `apps/server/src` fuera de `config.rs` (las constantes de origen de los tests no cuentan).
- **La misma tabla en server y web.** `scripts/__fixtures__/loopback-origins.json` lista pares `[origen, esperado]` (los seis del UT de A, más `https://localhost:5173`, `http://LOCALHOST:5173`, que `parse_public_origin` ya rechaza, y `http://[::ffff:127.0.0.1]:5173`). El UT de Rust la lee con `include_str!` desde `CARGO_MANIFEST_DIR`, y el de Vitest de `validators.ts` la importa. Si una de las dos implementaciones diverge, falla su test.
- **RF-B18.** Los valores de desarrollo de `INTERNAL_API_SECRET` e `IP_HASH_KEY` son constantes de `config.rs` (`DEV_INTERNAL_API_SECRET`, `DEV_IP_HASH_KEY`). Un UT comprueba que el `.env.example` del server trae esas dos constantes, y otro (Vitest) que el de web trae la misma de `INTERNAL_API_SECRET`. Fuera de loopback, un secreto igual a la constante da `config.invalid` con su `var`. La comparación es por bytes del texto, en tiempo constante (`subtle`), aunque la constante sea pública: así nadie copia el patrón a un secreto real.

### 6. Config (RF-B6, RF-B9, RF-B18)
En `config.rs`, en este orden, después de `INTERNAL_API_SECRET`:
- `INTERNAL_API_SECRET`: pasa a **obligatorio** (como dijo la decisión 17 de A). Base64url de ≥ 32 bytes, y fuera de loopback distinto del valor de desarrollo.
- `IP_HASH_KEY`: obligatorio, misma regla. Se guarda como `IpHashKey(Box<[u8]>)` con los bytes **decodificados** (es la llave del HMAC) y `Debug` redactado.
- `DAILY_GRANT`: obligatorio, entero decimal sin signo ni espacios, `1 ≤ n ≤ 10 000`. Si no, `config.invalid` con `var = DAILY_GRANT`. Se guarda como `Coins(i64)`.
- **Gate de `dev-auth`** (solo con la feature): si el origen está fuera de loopback, `config.invalid` con `var = DEV_AUTH`. Si `BIND_ADDR` no es `127.0.0.1` ni `::1` (incluido el valor por defecto, `0.0.0.0`), `config.invalid` con `var = BIND_ADDR`. Va al final, después de validar `PUBLIC_ORIGIN` y `BIND_ADDR`.
- `CookiePolicy` se deriva al final y queda en `Config`. **Sin la feature, el tipo solo tiene la variante de producción**: el constructor de desarrollo está detrás de `#[cfg(feature = "dev-auth")]`, así que no existe en el binario de release (RF-B9).
- `Debug` de `Config` redacta los tres secretos (`<redacted>`) y muestra `daily_grant`.
- `apps/server/.env.example`: los dos secretos con los valores de desarrollo, `DAILY_GRANT=100` (Área 10), `BIND_ADDR=127.0.0.1` (ya está, y ahora lo exige el gate) y la nota de `TRUSTED_PROXIES` (decisión 3.5). `just dev` arranca con `--features dev-auth` (decisión 22). Quien tenga un `apps/server/.env` viejo recibe `config.invalid` con `var = INTERNAL_API_SECRET`, que dice qué falta sin mostrar valores.
- **Compose y `check-compose`** (revisa devops-engineer): `server.environment` gana `INTERNAL_API_SECRET: ${INTERNAL_API_SECRET:?INTERNAL_API_SECRET is required}`, `IP_HASH_KEY: ${IP_HASH_KEY:?IP_HASH_KEY is required}` y `DAILY_GRANT: '100'` (pregunta 5). `ENV.server` de `check-compose.mjs` exige las dos interpolaciones requeridas y un `DAILY_GRANT` entero de 1 a 10 000. El fixture `valid` se actualiza y `check-compose.test.mjs` suma un caso por llave que falta, una interpolación sin `:?` y un `DAILY_GRANT` fuera de rango. **Secretos de `prod-local`** (cambio pedido 1, revisa devops-engineer): `infra/local.env` **no** gana secretos. `scripts/local-secrets.sh`, llamado por `just prod-local` (y por `_prod-local-up`), crea en el primer uso `infra/local.secrets.env` con `INTERNAL_API_SECRET` e `IP_HASH_KEY`, cada uno de 32 bytes de `/dev/urandom` en base64url sin relleno, con permisos `600`. Si el archivo existe, no lo toca. La receta lo pasa con un segundo `--env-file`. `.gitignore` y `.dockerignore` lo excluyen, `check-dockerignore` gana el caso, y un test en `scripts/__tests__` comprueba que el script nunca escribe fuera de `infra/local.secrets.env` y que dos corridas no cambian el contenido. Así ningún valor que acepte el binario de producción vive en git. El `web` del compose no cambia hasta D.

### 7. Migración y ledger (RF-B1, RF-B2)
- `apps/server/migrations/20261028000000_auth.sql` crea las tablas del "Modelo de datos" de la spec, salvo `admin_audit` (fase C), con todos los constraints con nombre fijo.
- La cuenta `issuance` se crea en la misma migración con un UUID fijo (`ledger::ISSUANCE_ACCOUNT`).
- **Triggers**, todos con `RAISE EXCEPTION USING ERRCODE = …` y un mensaje fijo sin valores (A9):
  - `ledger_immutable()`: `BEFORE UPDATE OR DELETE … FOR EACH ROW` y `BEFORE TRUNCATE … FOR EACH STATEMENT` sobre `ledger_entries` y `ledger_postings`, con `ERRCODE` `TA001`.
  - `ledger_entry_defaults()`: `BEFORE INSERT` en `ledger_entries`, pisa `created_at = now()` y `grant_day = (now() AT TIME ZONE 'UTC')::date`.
  - `ledger_entry_balanced()`: `CONSTRAINT TRIGGER … AFTER INSERT … DEFERRABLE INITIALLY DEFERRED FOR EACH ROW` en `ledger_entries` **y** en `ledger_postings`. Al commit comprueba, para el asiento tocado, `count(*) >= 2` y `sum(amount) = 0`. Si no, `ERRCODE` `TA002`.
- `sqlx` sin macros comprobadas en compilación (`SQLX_OFFLINE` no tiene `.sqlx/`): todo SQL usa `sqlx::query`/`query_as` con `.bind()`, nunca `format!`. Un UT de fuente falla si aparece `AssertSqlSafe` fuera de `tests/support/db.rs`.
- `ledger/mod.rs`: `Coins`, `create_user_account(tx, user_id)`, `grant(tx, user_id, amount) -> Option<EntryId>` (`INSERT … ON CONFLICT (user_id, grant_day) WHERE kind = 'grant' DO NOTHING RETURNING id`, y los dos movimientos solo si insertó) y `balance(pool, user_id) -> Coins` (`SUM`). No hay ninguna función que haga `UPDATE` ni `DELETE` de esas tablas.
- `db::error_kind` reconoce `TA001` y `TA002` como códigos (`ledger_immutable`, `ledger_unbalanced`), para que un fallo se registre con su código y nunca con el texto de Postgres.

### 8. `display_name` y alias (RF-B7)
- `profile/name.rs`: `validate(input) -> Result<DisplayName, NameError>`, en este orden: NFC, recorte, rechazo de `Cc` y de los rangos bidi y de ancho cero de D1, de 3 a 24 grafemas (`unicode-segmentation`), `display_name_key` = skeleton de UTS #39 (`unicode-security`) sobre casefold completo sobre NFKC, y comparación contra los reservados por llave. `NameError` mapea a `profile.*` en C. En B solo lo usa el alias.
- `profile/alias.rs`: `AliasSource` (trait), con `OsAlias` en producción (sustantivo + adjetivo + número de 10 a 999 con `OsRng`). Los IT inyectan una fuente fija por `Settings` para forzar choques (T20).
- `content/names/{nouns,adjectives,reserved}.txt`, una palabra por línea, incrustadas con `include_str!`. **Borrador mío** con ≥ 200 sustantivos y ≥ 100 adjetivos invariables en género, capitalizados, sin tildes ni espacios, de ≤ 10 caracteres. **game-designer las aprueba o reescribe** en el PR con el checklist de ADR-0004 y la revisión de palabras y combinaciones ofensivas. Hasta su APPROVE, el PR no se mergea.
- UT de las listas: conteos, producto ≥ 4 M, sin duplicados (también por `display_name_key`), cada combinación extrema (la palabra más corta y la más larga de cada lista con 10 y con 999) cumple de 3 a 24, y ninguna palabra es un reservado.
- **Dockerfile y `.dockerignore`** (revisa devops): las etapas `planner` y `build` copian `content/names`. `.dockerignore` deja pasar `content/names/**` y sigue excluyendo `content/_local/` y el resto de `content/`. `just check-dockerignore` gana el caso de que `content/_local/` no entra en el contexto.

### 9. Sesiones y cookies (RF-B8, RF-B9, RF-B10)
- `sessions.rs`:
  - `issue(tx, user_id, role, ip_hash, ua_family) -> SessionToken`: 32 bytes de `OsRng` en base64url. En la BD solo `SHA-256(token)`. `expires_at = now() + 30 d` (o `12 h` si `role = admin`).
  - `validate(pool, &token) -> Option<SessionInfo>`: una consulta por `token_hash` con `JOIN users`. Es válida si `banned_at IS NULL`, `expires_at > now()` y `last_seen_at > now() − 7 d`. Toca `last_seen_at` con `UPDATE … WHERE id = $1 AND last_seen_at < now() − interval '5 min'`. Todo el tiempo sale de `now()` de Postgres, sin reloj en la app, y los IT envejecen filas con SQL.
  - `revoke(tx, filter) -> Vec<SessionId>`: la **única** función con `DELETE FROM sessions … RETURNING id`, más `SELECT pg_notify('session_revoked', id::text)` por cada id, en la misma transacción. `RevokeFilter::{One(SessionId), AllOfUser(UserId), Token(TokenHash), Expired}`. Un UT de fuente falla si `DELETE FROM sessions` (sin distinguir mayúsculas ni espacios) aparece en `apps/server/src` fuera de `sessions.rs`.
- `SessionInfo` y `SessionToken` tienen `Debug` redactado. `SessionId` es un newtype del UUID sin `Display` (viaja en el `NOTIFY`, y RF-B17 lo trata como canario).
- `user_agent_family`: un conjunto cerrado (`chrome`, `firefox`, `safari`, `edge`, `other`) calculado con reglas de subcadena. Nunca se guarda el `User-Agent`.
- `ip_hash`: `IpKey::hash(&IpHashKey) -> [u8; 32]`, dentro de `net/`, sobre un byte de familia más los bytes de la llave. Los bytes de `IpKey` siguen sin salir del módulo. UT de T28: dos IPv6 del mismo `/64` dan el mismo hash.
- `http/cookies.rs`: `CookiePolicy { session_name, flow_name, secure }`. El parser lee el header `Cookie` y busca **solo** el nombre de la política activa. Con el nombre repetido, no hay sesión (ambiguo). `set_session`, `set_flow` y `expire(name)` escriben atributos fijos (`HttpOnly; SameSite=Lax; Path=/`, más `Secure` en producción, sin `Domain`, y `Max-Age` igual a la expiración absoluta o a 600 s en el flujo). Sin `axum-extra`.
- **Extractor `CurrentSession`** (opcional y obligatorio): lee la cookie, valida y, si es inválida, responde 401 `auth.unauthenticated` con la cookie expirada. Toda respuesta que lo usa lleva `Cache-Control: private, no-store`, por una capa en esas rutas.

### 10. Flujo OAuth, proveedor `dev` y login (RF-B3 a RF-B6, RF-B11 a RF-B13)
- **Manifiesto de rutas.** `POST /api/auth/{provider}/start` (`Mutate`), `GET /api/auth/{provider}/callback` (`Mutate`, la excepción), `GET /api/auth/providers` (`Read`), `POST /api/auth/logout` y `/logout-all` (`Mutate`), `GET /api/me` (`Read`). Con `dev-auth`, `GET /api/auth/dev/oauth/authorize` (`Mutate`, excepción de la pregunta 7), `POST …/token` (`Mutate`, exento de `Origin`) y `GET …/userinfo` (`Read`), todos `#[cfg(feature = "dev-auth")]`.
- **Exenciones de `Origin`: una tabla cerrada** (pregunta 7, nota para security-reviewer). `ORIGIN_EXEMPT: &[(Method, &str)]`, vacía sin la feature y con exactamente `(POST, "/api/auth/dev/oauth/token")` con ella. El middleware compara método y **plantilla exacta** de la ruta (`MatchedPath`), nunca un prefijo como `/api/auth/dev/`. Un UT de cada variante fija el contenido de la tabla, y un IT con la feature comprueba que `POST` a otra ruta `dev` (por ejemplo `…/authorize`, que solo acepta `GET`) sin `Origin` sigue dando 403 `auth.csrf_origin`. Una ruta `dev` nueva no queda exenta sin editar la tabla y su test. Igual con `SAFE_METHOD_EXCEPTIONS`.
- **Rate limit.** `AUTH_IP` en `start` y `callback`. La cuota "otras mutaciones" (usuario 60/min, IP 240/min) en `logout` y `logout-all`. `KeyedLimiter` se vuelve genérico sobre la llave (`KeyedLimiter<K>`, con `CappedStore<K>`), con un limiter por `UserId` y otro por `IpKey`. Las cuotas sin ráfaga en D7 usan `per_minute(n)`, que da una ráfaga de `n`. El IT de "la 6.ª petición en ráfaga da 429 con `Retry-After: 6`" se repite sobre `start` (decisión 3 de A).
- **Cuerpo.** `DefaultBodyLimit::max(4096)` en `start`, `token`, `logout` y `logout-all`.
- **`start`:** `Form { next?, dev_identity? }`. `next` pasa por `auth::next::sanitize`, una función escrita a mano equivalente a `^/(?![/\\])[^\x00-\x1f]*$` (el crate `regex` no tiene lookahead), con un UT de tabla y una propiedad de proptest. Si no cumple, `/`. `dev_identity` solo con `dev` y con `^[a-z0-9_-]{1,32}$`; si no, 303 a `/login?error=auth.invalid_state`. Crea la fila de `oauth_flows` (`id` y `state` de 128 bits y `code_verifier` de `OsRng`, `expires_at = now() + 10 min`), borra de paso las filas vencidas, pone la cookie del flujo y responde 303 a la URL de autorización. Un `{provider}` que no existe o no está habilitado da 404 `http.not_found`.
- **`callback`:** `DELETE FROM oauth_flows WHERE id_hash = $1 RETURNING …`, `state` comparado con `subtle`, `provider` igual al de la ruta, no vencido. Canje del `code` con PKCE, `userinfo`, y se descartan los tokens. Después, el login (abajo) y 303 a `next`. Cada fallo da 303 a `/login?error=<CallbackErrorCode>`: sin sesión, con la cookie del flujo expirada, con `Cache-Control: no-store` y sin usar `next`. El `Location` solo se arma desde `CallbackErrorCode::as_str()`, y un UT de fuente impide formatear otra cosa en él.
- **Proveedor `dev`** (`auth/dev.rs`, `#[cfg(feature = "dev-auth")]`): `authorize` valida `client_id`, `redirect_uri` exacta, `code_challenge_method = S256` y `login_hint`, guarda `code → (identidad, challenge, expira en 60 s)` en un mapa en memoria con tope de 1000 entradas, y redirige al callback. `token` consume el `code` una vez y comprueba `S256(code_verifier)`. Su access token es `ta-dev-access-token-canary.` más 16 bytes al azar. `userinfo` devuelve `{"sub": <dev_identity>}`. El cliente OAuth lo llama en `http://<BIND_ADDR>:<puerto>`, con el puerto real del listener, que `serve()` pasa a `AppState::from_config` tras el bind.
- **Login** (`auth/login.rs`), en **una** transacción:
  - Existente (RF-B4): con `banned_at`, `auth.banned` sin sesión.
  - Primer login (RF-B3): `INSERT users … ON CONFLICT (display_name_key) DO NOTHING RETURNING` con hasta 10 alias, después `oauth_accounts … ON CONFLICT DO NOTHING RETURNING` (si no inserta, `ROLLBACK` y se reintenta una vez como existente), la cuenta del ledger, el `grant` de `DAILY_GRANT`, `auth_events` de `login` y de `grant` (con `ip_hash`), y la sesión.
  - Si la petición traía una sesión, la rota con `revoke(One)` en la misma transacción.
  - Tras 10 choques de alias, `auth.try_again` y `ROLLBACK`.
- **Logout** (RF-B11): `revoke(Token)` o `revoke(AllOfUser)`, la cookie expirada y 303 a `/`, con o sin sesión.
- **`GET /api/me`** (RF-B12): `Me`, sin `user_id`, sin id de sesión y sin nada del proveedor. **`GET /api/auth/providers`** (RF-B13): `Vec<Provider>`, en B `["dev"]` con la feature y `[]` sin ella.

### 11. `/ws` con sesión (RF-B14, RF-B15)
- **Handshake.** El orden de A, con la sesión intercalada: `Origin` → upgrade válido → cookie (primera validación, `validate`) → si hay sesión, rate de upgrade por usuario (10/min) y por IP con sesión (120/min); si no, el rate anónimo de A → tope de sockets (5 por usuario y 200 por `IpKey` con sesión, o 50 anónimos por `IpKey`) → tope global. Los sockets con sesión no cuentan contra los anónimos. **Cookie inválida** (pregunta 3, nota para security-reviewer): el socket entra como anónimo. Pasa por el rate y el tope anónimos, **no** toma ningún cupo con sesión (ni por usuario, ni por `IpKey` con sesión, ni bucket de usuario), no se registra en `SessionId → sockets`, y `self` le responde `ws.forbidden_channel`, así que no recibe nada de `SelfHub`. Un IT lo comprueba: con 5 sockets válidos del usuario A, un sexto con la cookie de A vencida entra como anónimo, no recibe el `publish` de A y los contadores con sesión no cambian.
- **Topes.** `IpSlots` pasa a `Slots<K>` genérico: una instancia para `IpKey` anónimas (50), otra para `IpKey` con sesión (200) y otra para `UserId` (5). Los tres son RAII, como en A.
- **Registro** (`ws/registry.rs`): `SessionId → {socket: CancellationToken + motivo}` y `UserId → sockets`, en memoria y acotado por `WS_MAX_CONNECTIONS`. Ya dentro de `on_upgrade`: **registrar → `ValidationGate::before_validate()` → segunda validación por `SessionId`**. Si falla, se quita del registro y se cierra con `1008` y `auth.unauthenticated`. `ValidationGate` es un trait con una implementación nula en producción. El IT de la carrera inyecta una que espera a que el test haga el `revoke` (con `tokio::sync::Notify`, sin `sleep`).
- **Listener** (`sessions/listener.rs`): un `PgListener` sobre su **propio** pool de 1 conexión (8 + 1, RNF), con `application_name = 'torneo-listener'`. Bucle con `try_recv()`:
  - `Ok(Some(n))`: cierra los sockets de ese id.
  - `Ok(None)`: se perdió la conexión. Revalida en lote todos los sockets con sesión (`SELECT id FROM sessions WHERE id = ANY($1) AND …válida`) y cierra los que faltan.
  - `Err`: espera de 1 a 30 s con backoff, reconecta y revalida otra vez.

  Escribe un evento `sessions.listener_lost` en `warn`, solo con `event`.
- **Revalidación periódica.** Cada socket con sesión revalida cada 5 min (`WsConfig.revalidate_interval`, que los IT acortan) con `validate` por id, que toca `last_seen_at` con la regla de 5 min. Si no es válida, `1008` y `auth.unauthenticated`. `session.rs` generaliza `policy()` a `ErrorCode`.
- **Canales.** `Subscribe { channel }` se convierte a `Channel`. Uno desconocido, mal formado o con id (`user:…`) da `ServerMessage::Error { code: ws.forbidden_channel }`, y `self` desde un anónimo da lo mismo. La 9.ª suscripción da `ws.too_many_subscriptions`. Los mensajes de suscripción cuentan en la ventana de 10 por 10 s de A. `SelfHub` (`ws/self_hub.rs`): `UserId → senders` y `publish(UserId, SelfEvent)`, que los IT usan para T17 hasta que exista el renombre (C).

### 12. Retención (RF-B16)
`jobs.rs`: una pasada al arrancar (después de migrar) y luego cada 24 h. Cada pasada, en su transacción, borra los `auth_events` de más de 30 días, los `oauth_flows` vencidos y las sesiones vencidas (estas por `sessions::revoke(Expired)`, que notifica). Se cancela con la señal de apagado, como el sweeper de A. No registra nada salvo un fallo (`jobs.retention_failed` con `error_kind`).

### 13. Logs (RF-B17, A8, A9)
- Las rutas nuevas se registran por su plantilla (`MatchedPath`): `/api/auth/{provider}/callback` y `/api/auth/dev/oauth/authorize` salen sin query por construcción. Un UT del `request_log` lo fija con una URI con query canaria.
- `ALLOWED_FIELDS` no gana ningún campo. Los eventos nuevos (`sessions.listener_lost` y `jobs.retention_failed`) solo usan `event` y `error_kind`.
- `tests/logs.rs` (con `--features dev-auth`, en los tres niveles de `RUST_LOG` de A) corre un login completo con `dev`, `GET /api/me`, un socket con sesión, un logout y un callback con `state` inválido. Busca los canarios de RF-B17: token de sesión, valor de la cookie del flujo, `code`, `state`, `code_verifier`, access token canario, `token_hash` (hex y base64), `X-Internal-Secret`, `X-Client-Origin`, `X-Client-Fetch-Site`, la IP, `ip_key`, `ip_hash` (hex), `user_id`, `provider_user_id` (el `dev_identity` canario), `display_name` y el `sessions.id`. Los ids y hashes se leen de la BD de test después de la corrida.
- **A9: `_check-pg-log`** (pregunta 6 y cambio pedido 2). `tests/pg_log.rs` usa un prefijo canario por corrida (`pglog-canary-<uuid>`, escrito en un archivo del directorio `target/`) en cuatro casos:
  - un `pg_notify('session_revoked', …)` con el canario;
  - un `UPDATE` del ledger que el trigger rechaza, con el canario en el valor;
  - un asiento descuadrado con el canario;
  - **una violación de unicidad con el canario en la llave**: un `INSERT INTO users` con un `display_name_key` repetido igual al canario, sin `ON CONFLICT`. Es el caso que A9 existe para tapar (`Key (…)=(…)`).

  `_check-pg-log` lee `docker compose -f infra/compose.dev.yml logs --since <inicio de cargo test> postgres` y falla en dos casos:
  - si el canario aparece;
  - **control positivo**: si el log del intervalo no trae ninguna línea `ERROR` del contenedor. El IT fuerza una que A9 deja pasar a propósito: un error de sintaxis sin valores (`SELEC 1`). Con `log_error_verbosity=terse` y `log_min_error_statement=panic`, Postgres registra la línea `ERROR` sin la sentencia completa, y el paso exige encontrarla.

  Así el paso no pasa en vacío si `--since` o el nombre del servicio están mal.

### 14. Propiedades (proptest)
- `next::sanitize`: el resultado siempre empieza por `/`, nunca por `//` ni por `/\`, y no tiene bytes de control.
- `display_name_key` es idempotente (`key(key(x)) == key(x)` sobre la salida normalizada) y todo nombre que pasa `validate` mide de 3 a 24 grafemas.
- `Coins`: `grant` + movimientos suman 0 para cualquier `1 ≤ B ≤ 10 000`.

Ninguna toca el motor: el determinismo de `crates/engine` no cambia (ADR-0001). `OsRng` solo aparece en `sessions`, `auth` y `profile::alias`. Un UT de fuente prohíbe `OsRng` en el código de `crates/` (el motor usa `rand_core` solo a través de `rand_chacha`).

### 15. Feature `dev-auth` y `justfile` (revisa devops)
- `apps/server/Cargo.toml`: `[features] dev-auth = []`, fuera de `default`.
- `just dev` corre `cargo run -p server --features dev-auth --locked`.
- `check-rust`:
  - suma `cargo clippy -p server --features dev-auth --all-targets --locked -- -D warnings`;
  - suma `cargo test -p server --features dev-auth --locked` después del `cargo test --workspace` (que sigue sin la feature y prueba la política de producción);
  - suma `_check-pg-log`.
- Los IT que necesitan `dev` llevan `#![cfg(feature = "dev-auth")]`, y los que prueban su ausencia, `#![cfg(not(feature = "dev-auth"))]`.
- El chequeo de que el binario de release no contiene `/api/auth/dev/` ni `ta_dev_session` es de RF-D9 (fase D). En B lo cubre un UT sin la feature: `ROUTES` no tiene rutas `/api/auth/dev/`, `CookiePolicy` solo tiene la variante de producción y las listas de excepciones solo tienen la del callback.

## Archivos que cambian
| Archivo | Cambio |
|---|---|
| `Cargo.toml`, `Cargo.lock` | modificado: dependencias de la decisión 1 (pasa `just crate-age` y `just deny`) |
| `apps/server/Cargo.toml` | modificado: dependencias, `axum` con `form` y `query`, feature `dev-auth` |
| `apps/server/migrations/20261028000000_auth.sql` | nuevo: tablas, constraints, triggers y la cuenta `issuance` (decisión 7) |
| `apps/server/src/config.rs` | modificado: secretos obligatorios, `IP_HASH_KEY`, `DAILY_GRANT`, gate de `dev-auth`, `CookiePolicy`, constantes de desarrollo, tabla compartida (decisiones 5 y 6) |
| `apps/server/src/app.rs` | modificado: `AppState::from_config`, pool, puerto, listener, jobs (decisión 3.2) |
| `apps/server/src/db.rs` | modificado: códigos `TA001` y `TA002`, pool del listener |
| `apps/server/src/net/ip_key.rs` | modificado: `IpKey::hash` (T28) |
| `apps/server/src/rate/` | modificado: `KeyedLimiter<K>`, cuotas nuevas, test determinista (decisiones 3.3 y 10) |
| `apps/server/src/http/routes.rs` | modificado: `Handler`, `route_for` exhaustivo, rutas de B, excepciones `dev` (decisiones 3.1 y 10) |
| `apps/server/src/http/origin.rs` | modificado: `OriginPolicy` desde `Config`, exención de `token` con `dev-auth` (pregunta 7) |
| `apps/server/src/http/cookies.rs` | nuevo: `CookiePolicy`, parser y `Set-Cookie` (decisión 9) |
| `apps/server/src/http/mod.rs` | modificado: capas por ruta, `Cache-Control` |
| `apps/server/src/sessions.rs`, `sessions/listener.rs` | nuevos: token, `validate`, `revoke`, listener (decisiones 9 y 11) |
| `apps/server/src/ledger/mod.rs` | nuevo: cuentas, `grant`, saldo (decisión 7) |
| `apps/server/src/profile/{mod,name,alias,me}.rs` | nuevos: validación, alias, `GET /api/me` (decisión 8) |
| `apps/server/src/auth/{mod,start,callback,login,next,logout,providers,client}.rs` | nuevos: flujo OAuth, login, logout, cliente HTTP (decisión 10) |
| `apps/server/src/auth/dev.rs` | nuevo, solo con `dev-auth`: el proveedor `dev` |
| `apps/server/src/ws/{mod,handshake,session,ip_slots,registry,self_hub,channels}.rs` | modificado y nuevos: sesión en el handshake, `Slots<K>`, registro, `ValidationGate`, canales, `SelfHub` (decisión 11) |
| `apps/server/src/jobs.rs` | nuevo: retención (decisión 12) |
| `apps/server/src/lib.rs` | modificado: módulos nuevos |
| `apps/server/.env.example` | modificado: secretos de desarrollo, `DAILY_GRANT`, nota de `TRUSTED_PROXIES` |
| `apps/server/Dockerfile`, `.dockerignore` | modificado: `COPY content/names` (decisión 8, revisa devops) |
| `apps/server/tests/support/{app,config,db,session}.rs` | modificado y nuevos: config por tabla, pool, usuarios y sesiones de prueba |
| `apps/server/tests/{ledger,auth_flow,sessions,me,ws_session,listener,retention,names}.rs` | nuevos: los IT de las Pruebas |
| `apps/server/tests/{http,routes,ws,client_ip,logs,startup}.rs` | modificado: `from_config`, rutas nuevas, canarios y arranque |
| `crates/protocol/src/{error,ws,me,lib}.rs` | modificado y nuevo: decisión 2 |
| `packages/protocol-ts/src/**` | regenerado con `just gen` |
| `content/names/{nouns,adjectives,reserved}.txt` | nuevos: borrador para game-designer (decisión 8) |
| `scripts/__fixtures__/loopback-origins.json` | nuevo: tabla compartida (decisión 5) |
| `scripts/check-compose.mjs`, su fixture `valid` y `check-compose.test.mjs` | modificado: secretos y `DAILY_GRANT` (decisión 6) |
| `infra/docker-compose.yml` | modificado: variables nuevas del server (decisión 6, pregunta 5) |
| `scripts/local-secrets.sh`, su test en `scripts/__tests__`, `.gitignore`, `.dockerignore` | nuevo y modificados: secretos de `prod-local` generados e ignorados (decisión 6, revisa devops) |
| `apps/server/tests/pg_log.rs` | nuevo: canarios de A9 y control positivo (decisión 13) |
| `apps/web/src/lib/features/system/echo.ts`, `echo.test.ts`, `messages/{en,es}.json` | modificado: códigos nuevos (decisión 4) |
| `apps/web/src/lib/env/validators.ts`, `validators.test.ts`, `apps/web/.env.example` | modificado: `isLoopbackHttpOrigin` y el secreto de desarrollo (decisiones 4 y 5) |
| `justfile` | modificado: `dev` con `dev-auth`, `check-rust` con la feature y `_check-pg-log`, `prod-local` con `local-secrets.sh` y el segundo `--env-file` (decisiones 6 y 15) |
| `docs/features/008-autenticacion/plan-a.md` | modificado: orden de las desviaciones y riesgo del lock (decisión 3.7) |

## Orden de trabajo
1. **Dependencias** (decisión 1): `Cargo.toml`, `cargo update -w`, `just crate-age` **antes de compilar**, bajar lo marcado, `just deny` y `cargo tree -d`. Si `oauth2` trae lo que dice la pregunta 1, se para aquí. Commit `build(server): add auth phase B dependencies (TA-26)`.
2. **Protocolo y web** (decisiones 2 y 4): primero los tests de `crates/protocol` en rojo, después los tipos, `just gen`, `echo.ts`, mensajes y `echo.test.ts`. Commit `feat(protocol): auth, channel and Me types for phase B (TA-26)`. **Paralelizable** con el paso 3.
3. **Notas de #53** (decisión 3): primero el test determinista de la carrera y el del manifiesto exhaustivo. Después `Handler`, `AppState::from_config` y `tests/support/config.rs`, el doc comment, el `.env.example` y `plan-a.md`. Los IT de A siguen en verde sin cambiar sus aserciones. Commit `refactor(server): exhaustive route manifest and state from config (TA-26)`.
4. **Config y "fuera de loopback"** (decisiones 5 y 6): primero los UT de config (`DAILY_GRANT` con `0`, `10 001`, `1.5`, `+5` y vacío; secretos cortos, iguales al ejemplo dentro y fuera de loopback; gate de `dev-auth` por origen y por `BIND_ADDR`), la tabla compartida y el Vitest de `isLoopbackHttpOrigin`. Después la implementación, los `.env.example`, el compose, `local.env` y `check-compose`. Commit `feat(server): required secrets, DAILY_GRANT and the dev-auth gate (TA-26)`.
5. **Migración y ledger** (decisión 7): primero `tests/ledger.rs` en rojo (`UPDATE`, `DELETE` y `TRUNCATE` dan `TA001`; un asiento descuadrado, uno con 0 y uno con 1 movimiento no hacen commit; el `grant_day` de la app se ignora; un `grant` por día; la suma total se conserva). Después la migración y `ledger/`. Commit `feat(server): immutable double-entry ledger (TA-26)`.
6. **Nombres y alias** (decisión 8): UT de tabla de T20 y de las listas en rojo, después `profile/name.rs`, `alias.rs`, las listas y el Dockerfile. Commit `feat(server): display name validation and generated alias (TA-26)`. **Paralelizable** con el paso 5.
7. **Sesiones y cookies** (decisión 9): UT de `CookiePolicy` (con y sin la feature), del parser y de la fuente (`DELETE FROM sessions`). IT de T11 (inactividad y absoluta envejecidas por SQL), de baneado y de la cookie `ta_dev_session` ignorada con la política de producción. Después `sessions.rs`, `cookies.rs` y el extractor. Commit `feat(server): sessions, cookies and single revoke path (TA-26)`.
8. **Flujo OAuth y login** (decisión 10), el paso más riesgoso: primero los IT de `auth_flow.rs` en rojo (flujo `dev` completo, T4 adaptado, T5, cada `CallbackErrorCode`, T19 con `dev`, RF-B3 con 20 logins concurrentes y el fallo inyectado, RF-B4, T10 de rotación y logout, T20 con el alias forzado, `start` con 429 en la 6.ª, 404 de un proveedor desconocido, `/api/me` y `/api/auth/providers`). Después `auth/`, `profile/me.rs` y las rutas. Commit `feat(server): OAuth flow, dev provider, login and logout (TA-26)`.
9. **`/ws` con sesión** (decisión 11): primero los IT en rojo (6.º socket por usuario, 201.º por `/64` con sesión, T17, suscripción 9.ª, T22 por usuario, la carrera con `ValidationGate`, la revalidación con un intervalo corto, el cierre por logout sin esperar la revalidación y el `pg_terminate_backend` del listener). Después `registry`, `Slots<K>`, `SelfHub`, canales y el listener. Commit `feat(server): authenticated /ws, revocation and the self channel (TA-26)`.
10. **Retención** (decisión 12): IT con filas envejecidas por SQL, después `jobs.rs`. **Paralelizable** con el paso 9.
11. **Logs** (decisión 13): canarios de RF-B17 en `tests/logs.rs` en los tres niveles y `_check-pg-log`. Commit `test(server): auth log canaries and Postgres log check (TA-26)`.
12. **`justfile`** (decisión 15), `just fmt`, `just check` completo, `just deny`, y la salida en el PR.

## Riesgos
- **Un asiento mal hecho no se puede deshacer** (ADR-0005). → Triggers de inmutabilidad y cuadre en la BD, no en la app. `DAILY_GRANT` acotado a 1–10 000 y obligatorio. game-designer revisa el `grant`. Los IT del paso 5 van antes de cualquier código que inserte asientos.
- **Carrera entre el registro del socket y la revocación** (RF-B14). → Registrar antes de validar, y un IT determinista con `ValidationGate` y `Notify`, sin `sleep`.
- **El listener pierde notificaciones al reconectar.** → `try_recv()` y no `recv()`, revalidación en lote al perder la conexión y al recuperarla, y el IT con `pg_terminate_backend`. La revalidación de 5 min queda de respaldo.
- **El proveedor `dev` llega a producción.** → Feature fuera de `default`, gate de origen y de `BIND_ADDR`, `CookiePolicy` sin variante de desarrollo en el binario de release, y UT sin la feature. El chequeo del binario de release es de la fase D (RF-D9).
- **Un `GET` que muta o una exención de `Origin` de más.** → Las dos excepciones de `dev` existen solo con la feature y las fija un UT de cada variante. `check_manifest` sigue fallando con cualquier otra.
- **Redirección abierta o reflejo en el callback.** → `next` sale solo de `oauth_flows`, `sanitize` tiene tabla y propiedad, y el `Location` del error sale solo de `CallbackErrorCode`.
- **Filtrar un secreto o un identificador en los logs** (A4). → `Debug` redactado en todos los tipos nuevos, ningún campo nuevo en `ALLOWED_FIELDS`, canarios de RF-B17 y `_check-pg-log`.
- **El pool se agota con el listener.** → Pool propio de 1 conexión para el listener, así el de la app sigue en 8.
- **Primeros logins concurrentes.** → `ON CONFLICT` en los dos índices únicos, `ROLLBACK` y un reintento como existente, con el IT de 20 concurrentes.
- **Las listas del alias no pasan la revisión.** → Son datos (`content/names/`), así que reescribirlas no cambia código. El UT de las listas valida tamaño y formato antes de la revisión.
- **6to4, Teredo y NAT64 como `/64` simples, y la latencia de la admisión exclusiva bajo ráfagas de `/64` nuevas** (notas de #53). → Aceptados por la spec y acotados por el tope de llaves. Los mide la prueba de carga de TA-6 y TA-8.
- **Romper `just dev` con un `.env` viejo.** → `config.invalid` nombra la variable que falta, y el PR lo avisa.
- **`prod-local` con WebKit** (Área 12). → No cambia en B. Lo documenta la fase D.

## Alternativas descartadas
- **`reqwest` ya en la fase B.** Trae `hyper-rustls` y más features de las que B usa (solo HTTP a loopback), y su línea 0.13 usa `aws-lc-rs` por defecto. `hyper-util` ya está en el lock (pregunta 1).
- **Macros `sqlx::query!` con `.sqlx/`.** Obligan a una BD o a un caché en el build del Dockerfile (`SQLX_OFFLINE`). `query` con `bind` es igual de parametrizado y no cambia el build.
- **Un regex con lookahead para `next`** (`fancy-regex`). Es una dependencia nueva para una regla de cuatro condiciones.
- **`axum-extra` para cookies.** Su `CookieJar` lee cualquier nombre y escribe atributos por llamada. El parser propio solo conoce los nombres de la política, como exige RF-B9.
- **Un `user_agent` completo en `sessions`.** Es un dato más identificable que la familia y la spec pide `user_agent_family`.
- **El reloj de la app para las expiraciones.** Postgres ya tiene `now()`, y los IT envejecen filas con SQL sin pausar el reloj de tokio.
- **El registro de sockets después de validar.** Abre la ventana de RF-B14.
- **`recv()` en el listener.** Reconecta en silencio y pierde notificaciones (RF-B10).
- **Una migración por entorno para `dev` en el `CHECK`** (Área 6). Cuesta más de lo que protege.

## Pruebas (cómo se demuestra que funciona)
- [ ] `just check` en verde, con la salida pegada en el PR (incluye `cargo test` con y sin `dev-auth`, clippy de las dos variantes y `_check-pg-log`).
- [ ] `just crate-age`, `just deny` y `cargo tree -d -p server` sin versiones nuevas de crates con más de una versión, salvo las que se justifiquen en el PR (`rand 0.8`, `rand_chacha 0.3`, `rand_core 0.6` y `thiserror 1` por `oauth2`). UT de fuente de `new_random`.
- [ ] **`prod-local`:** el test de `scripts/local-secrets.sh` (idempotente, solo escribe su archivo), `check-dockerignore` con `infra/local.secrets.env` fuera, y `git check-ignore infra/local.secrets.env`.
- [ ] **A9:** `tests/pg_log.rs` con los cuatro canarios y `_check-pg-log` con su control positivo.
- [ ] **RF-B1, RF-B2:** `tests/ledger.rs` (paso 5), con las migraciones corridas desde cero en una BD de test.
- [ ] **RF-B3:** 20 primeros logins concurrentes con la misma identidad dan 1 usuario, 1 asiento y 20 sesiones válidas. El fallo inyectado en el paso 3 (un trigger de test que rechaza el `INSERT` en `ledger_accounts`) no deja usuario ni asiento. El saldo inicial es `DAILY_GRANT`.
- [ ] **RF-B4:** login existente sin asientos nuevos. Un baneado recibe 303 a `/login?error=auth.banned` sin sesión.
- [ ] **RF-B5 (T4 adaptado, T5):** sin cookie del flujo, sin `state`, con `state` reusado, de otra cookie, vencido y de otro `provider`: 303 a `/login?error=auth.invalid_state`, sin `Set-Cookie` de sesión, con la cookie del flujo expirada, `Cache-Control: no-store` y sin usar `next`. `next` igual a `https://x`, `//x`, `/\x` y con bytes de control: 303 a `/`. Un IT por cada `CallbackErrorCode` de B, y ninguna parte de la petición aparece en el `Location`.
- [ ] **RF-B6:** UT de config del gate (origen y `BIND_ADDR`) e IT del flujo `dev` completo, con un `code_verifier` que no coincide y un `code` reusado que fallan.
- [ ] **RF-B7 (T20 y T19):** UT de tabla (largo, `Cc`, bidi, ancho cero, reservado, mayúsculas, homoglifo con `о` cirílica y reservado con homoglifos), UT de las listas y propiedades. IT con el alias forzado a chocar: reintenta y crea un alias libre. Con 10 choques, 303 a `/login?error=auth.try_again` sin usuario. T19 con `dev`: el `display_name` no coincide con el `dev_identity` ni con nada de `userinfo`.
- [ ] **RF-B8 (T10, T11):** la rotación borra la sesión previa y cierra su socket en el acto. `last_seen_at` de más de 7 días o `expires_at` vencido da 401 `auth.unauthenticated` con la cookie expirada. Un admin creado por SQL recibe una sesión de 12 h. `last_seen_at` no se toca dos veces en 5 min. `Cache-Control: private, no-store` en `/api/me` y en el logout.
- [ ] **RF-B9:** UT de `CookiePolicy` con y sin la feature. IT: con la política de producción, una `ta_dev_session` válida se ignora, y una cookie con el nombre repetido no da sesión.
- [ ] **RF-B10:** UT de fuente del único `DELETE FROM sessions`. IT de `pg_terminate_backend` sobre `torneo-listener`, un `revoke` en el hueco, y el socket cerrado antes de la revalidación (con el intervalo por defecto de 5 min, en menos de 5 s).
- [ ] **RF-B11:** logout y logout-all dan 303 a `/` con y sin sesión, y el token viejo da 401.
- [ ] **RF-B12, RF-B13:** `/api/me` con la forma exacta y sin `user_id` (el JSON no tiene ninguna llave fuera de las de `Me`). Sin sesión, 401. `/api/auth/providers` con y sin la feature.
- [ ] **RF-B14 (T18 parcial):** 6.º socket del mismo usuario: 503 `ws.user_capacity`. 201.º socket con sesión desde la misma `/64` (41 usuarios de prueba con 5 sockets, con el reloj manual de A): 503 `ws.ip_capacity`. Los sockets con sesión no consumen los 50 anónimos. La carrera con `ValidationGate`. La revalidación con un intervalo de 200 ms cierra con `1008` y `auth.unauthenticated`. Mensaje de más de 4 KB (ya de A, con sesión).
- [ ] **RF-B15 (T17):** el socket de A con `self` recibe el `SelfHub::publish` de A y no el de B. `user:{id_B}`, nombres desconocidos, mal formados y `self` desde un anónimo responden `ws.forbidden_channel` sin abrir la suscripción. La 9.ª responde `ws.too_many_subscriptions`.
- [ ] **RF-B16 (T28 del `/64`):** UT de `IpKey::hash`. IT de retención con `auth_events`, `oauth_flows` y sesiones envejecidas por SQL, y el `NOTIFY` de las sesiones borradas.
- [ ] **RF-B17 (T13 extendido):** `tests/logs.rs` en `info`, `debug` y `debug,sqlx=trace,tungstenite=trace`, con todos los canarios de la decisión 13.
- [ ] **RF-B18:** UT de config de los dos secretos (cortos, no base64url, iguales al ejemplo dentro y fuera de loopback) y de `DAILY_GRANT`. IT de arranque en `tests/startup.rs`: código 1 y solo `var` en el log. UT de que los `.env.example` traen las constantes.
- [ ] **T1 extendido:** `tests/routes.rs` recorre las rutas de B (y las de `dev` con la feature), y el manifiesto no compila sin handler (decisión 3.1).
- [ ] **T22 (usuario):** el mismo usuario desde dos IPs comparte su bucket de upgrade del `/ws` (el 11.º desde la segunda IP da 429).
- [ ] **"Fuera de loopback":** la tabla compartida pasa en Rust y en Vitest, y el UT de fuente de `Host::Domain` y `Ipv4Addr::LOCALHOST`.
- [ ] **Compose:** `check-compose.test.mjs` con los casos nuevos, y `node scripts/check-compose.mjs` sobre `infra/`.
- [ ] **Web:** `echo.test.ts` con las filas nuevas, el Vitest de `isLoopbackHttpOrigin`, `svelte-check` y `check-i18n`.
- [ ] **Docker:** `just check-dockerignore` con `content/_local/` fuera del contexto. El job `images` construye la imagen con `content/names`.

## Estimación de CI
Sin cambios en los workflows: el job `rust` corre `just check-rust`, que absorbe los pasos nuevos. Suma unos 3 a 5 min: la compilación y los tests con `dev-auth` (unos 90 s), clippy de la segunda variante (unos 40 s), los IT con Postgres (login concurrente, listener y `/ws` con 200 sockets, todos con reloj manual o `Notify`) y `_check-pg-log` (segundos). El job `web` suma segundos y `images`, la capa de `content/names`. Todo queda bajo los timeouts actuales.

## Desviaciones durante la implementación
1. **Cliente HTTP del flujo OAuth.** Usa `hyper::client::conn::http1` (la feature `client` ya estaba) en lugar del cliente `client-legacy` de `hyper-util`. B solo hace una petición por conexión a una IP de loopback, y así no se activa ninguna feature nueva. Las reglas (solo `http` a loopback, sin redirecciones, 10 s, 64 KB, `User-Agent` genérico) son las del plan, con sus UT.
2. **La dirección del propio server.** Los endpoints `token` y `userinfo` del proveedor `dev` se llaman en la dirección local de la conexión aceptada (extensión `LocalAddr`, que `serve_listener` inserta junto a `ConnectInfo`), no en un puerto que `serve()` pase a `AppState::from_config`. Funciona igual en el binario y en los IT. `from_config` recibe además la sonda de salud (los IT de A usan sondas falsas), y queda `AppState::with_ws` para los IT que bajan los topes por debajo de lo que permite la config.
3. **`ServerMessage::SelfEvent { event }`** es una variante con campo, no `SelfEvent(SelfEvent)`: con el tag interno de serde una variante newtype choca con el tag de `SelfEvent`. En el cable queda `{"type":"self_event","event":{"kind":"profile_changed",…}}`.
4. **La excepción de `Me.balance`** la vigila un UT de `crates/protocol`, que exige un solo `ts(type = …)` en todo el crate. Con `#[ts(type = "number")]` el TS generado no tiene `bigint`, así que `gen-protocol-ts.mjs` no cambia.
5. **UT de fuente de "fuera de loopback".** Prohíbe `Host::Domain` y la comparación con `"localhost"` fuera de `config.rs`, pero no `Ipv4Addr::LOCALHOST`: `healthcheck.rs` se conecta a esa dirección y no decide nada sobre el origen.
6. **`.dockerignore`.** No se reduce `content/` a `content/names`: `check-compose` prohíbe cualquier re-inclusión distinta de `!**/.env.example`, y es una regla de seguridad. `content/_local/` sigue excluido (con su caso en `check-dockerignore`, que ahora además exige `content/names/nouns.txt` en el contexto), y la imagen copia solo `content/names`, en la etapa `build` (las de `planner` y `cook` no compilan el crate).
7. **`display_name_key` se itera hasta un punto fijo.** Una pasada no es idempotente (el skeleton de `%` es `º/₀`, que NFKC cambia otra vez; lo encontró la propiedad de proptest con `"% 0"`). La spec exige la idempotencia, así que `key_of` repite la pasada hasta que no cambia (máximo 16). Es determinista y solo junta más nombres confundibles. El archivo `proptest-regressions/` se commitea, como recomienda proptest.
8. **Propiedad de `Coins`.** Se quitó: era una tautología. La cubren los IT del ledger con los extremos de `DAILY_GRANT` (1 y 10 000) y el 100, más la conservación del total.
9. **Tests de A con el secreto opcional.** `INTERNAL_API_SECRET` es obligatorio desde B, así que los casos "sin secreto configurado" de A pasan a "otro secreto configurado" (el valor ajeno se ignora igual). Los UT de config y los IT que usan un origen fuera de loopback, y el de fallo de bind con `192.0.2.1`, corren solo sin `dev-auth` (con la feature el server ni arranca), y con la feature hay un IT de arranque del gate.
10. **Lectura de la sesión.** Son dos funciones de `http/session.rs` (`current` y `unauthenticated`), no un extractor de axum. El comportamiento es el del plan: 401 `auth.unauthenticated`, la cookie expirada y `Cache-Control: private, no-store`.
11. **Orden de los IT de sesión.** Los de la capa de BD (T11, ban, toque de 5 min, `revoke` con su `NOTIFY`) se escribieron en el paso 7. Los HTTP (RF-B9 con la política de producción, en `tests/me.rs`, y el 401 de `/api/me`) llegaron en el paso 8, junto con la primera ruta que lee la sesión.
12. **La 9.ª suscripción.** Con `self` como único canal, en B no se puede abrir una 9.ª. El tope de 8 está implementado y su UT lo prueba con un tope de 0. El IT con nueve canales reales llega con los canales públicos de TA-13.
13. **Un evento de log más:** `auth.login_failed` en `warn`, solo con `event` y `error_kind`, cuando la transacción del login falla en la BD (el usuario ve `auth.try_again`). Ningún campo nuevo en `ALLOWED_FIELDS`.
14. **Canarios del login en `tests/logs_auth.rs`** (con `dev-auth`), no dentro de `tests/logs.rs`, para no tocar los conteos de A. Ese IT también prueba que las rutas salen como plantillas y que ninguna línea lleva `?`. Reemplaza el UT de `request_log` con una query canaria: armar un router de prueba ahí violaría el UT de fuente de `.route(`, y el IT cubre las rutas reales.
15. **IT del flujo con el limiter real.** Los helpers del flujo usan un reloj manual y rellenan `AUTH_IP` antes de `start` y de `callback`, en lugar de cambiar la cuota. El 429 de la 6.ª petición tiene su propio IT.
16. **Logout con la BD caída:** 503 `auth.try_again` en lugar del 303. Responder 303 diría que la sesión se cerró cuando puede seguir viva.
17. **Listener y retención.** Al perder la conexión, el listener revalida, suelta el `PgListener` y reconecta de forma explícita (`LISTEN` otra vez), y vuelve a revalidar. No depende de la reconexión implícita de `try_recv()`. Los lanza `serve_listener`, con un token que se cancela al terminar, así los IT en proceso también los tienen. Una revalidación periódica que falla en la BD deja el socket abierto: la vía principal sigue siendo el `NOTIFY`.
18. **Errores del proveedor `dev`.** `authorize` responde 400 `auth.provider_error` a una petición inválida. `token` y `userinfo` responden los códigos de RFC 6749 (`invalid_grant`, `invalid_token`), que es lo que `oauth2` interpreta. Son códigos, no texto.
19. **`sqlx` gana la feature `uuid`.** No agrega crates al lock (solo nombres de dependencias opcionales), y `just crate-age` y `just deny` pasan.

## Revisión
### architect — 2026-10-28 — REQUEST_CHANGES
- Verifiqué el plan contra los RF-B1 a RF-B18 de la spec, contra ADR-0008 (D1 a D3, D5 a D8, D10, la "Nota (2026-10-28)" y la "Enmienda 2026-10-28" del callback), contra ADR-0005, ADR-0012 (A4, A8, A9) y ADR-0001, y contra el código de `main` después de #53. Lo que está bien:
  - Cada RF tiene su decisión, sus archivos y su prueba.
  - El orden escribe los tests del ledger antes de cualquier código que inserte asientos.
  - Los triggers de cuadre están en las dos tablas, con el caso de 0 y de 1 movimientos.
  - El único `revoke` emite `pg_notify` en la misma transacción.
  - El socket se registra antes de la segunda validación, con un IT determinista (`ValidationGate` con `Notify`).
  - El listener usa `try_recv()` y tiene su IT con `pg_terminate_backend`.
  - El `Location` del error sale solo de `CallbackErrorCode`, y `next` solo de `oauth_flows`.
  - `CookiePolicy` no tiene la variante de desarrollo sin la feature.
  - El `sessions.id` está entre los canarios.
  - Las notas de #53 están plegadas (decisión 3).
  - El motor no se toca.
- Cambios pedidos:
  1. **Secretos de `prod-local` (pregunta 4): no van fijos en git.** `infra/local.env` está commiteado, y su origen (`https://localhost`) queda "fuera de loopback". Unos valores fijos y públicos en ese archivo los aceptaría también el binario de producción, porque RF-B18 solo rechaza los del `.env.example`. Copiar ese archivo a Coolify por error dejaría `IP_HASH_KEY` e `INTERNAL_API_SECRET` públicos en producción. *Cambio:* `just prod-local` genera los dos secretos en el primer uso (32 bytes de `/dev/urandom` en base64url) en un archivo ignorado por git (p. ej. `infra/local.secrets.env`), y los pasa con un segundo `--env-file`. `check-dockerignore` y `.gitignore` lo cubren, con un caso en `scripts/__tests__` si aplica. Revisa devops-engineer.
  2. **`_check-pg-log` (pregunta 6): sí, con dos agregados.**
     - (a) Un IT tiene que provocar a propósito una **violación de unicidad** con el canario en la llave: un `INSERT` en `users` con un `display_name_key` repetido, sin `ON CONFLICT`. Es el caso que A9 existe para tapar (`Key (…)=(…)`). Hoy el plan solo prueba el `NOTIFY`, el trigger y el descuadre.
     - (b) Un control positivo: el paso falla también si el log del intervalo no tiene **ninguna** línea del contenedor (p. ej. no encuentra la línea de checkpoint o de arranque). Así no pasa en vacío si `--since` o el nombre del servicio están mal.
  3. **`oauth2` (pregunta 1): sí, el crate, pero con el `state` y el `code_verifier` de `OsRng`.** RF-B5 y D2 piden 128 bits de CSPRNG del server. Se construyen con `CsrfToken::new` y `PkceCodeVerifier::new` desde bytes de `OsRng`, nunca con `new_random*`, que usa el `rand 0.8` del crate. Un UT de fuente falla si aparece `new_random` en `apps/server/src`. Así la tercera versión de `rand` queda en el lock pero no se usa para ningún secreto.
- Respuestas a las demás preguntas. Ninguna cambia el plan:
  - **1 (resto).** Uso `oauth2` sin features y `AsyncHttpClient` sobre `hyper-util`, sin `reqwest`. Cumple D2 ("crate `oauth2`"), y `chrono` y `rand 0.8` son aceptables si pasan `crate-age` y `deny`. **Si no pasan**, escribir el flujo a mano se aparta de D2: necesita una enmienda de ADR-0008, que escribo yo, aprueba security-reviewer y **decide el owner**. El paso 1 se detiene ahí.
  - **2.** `balance: i64` con `#[ts(type = "number")]`, la excepción declarada en `gen-protocol-ts.mjs` solo para ese campo y la guarda de `|saldo| ≤ 2^53 − 1` con 500: sí. `i32` truncaría o forzaría una migración del contrato. La guarda lleva su UT.
  - **3.** Cookie inválida en el upgrade, el socket entra como anónimo: sí. Cuenta en los límites anónimos, `self` le da `ws.forbidden_channel`, y quien informa al cliente es la UI con el 401 de `/api/me`. Rechazar el handshake solo daría un 1006 y un bucle de reconexión. **Para security-reviewer:** confirmar que un socket con una cookie inválida no recibe nada de `self` ni consume los cupos con sesión.
  - **5.** `DAILY_GRANT: '100'` literal en el compose, y `check-compose` que solo valida el rango de 1 a 10 000: sí. El valor de la economía queda revisado en git y TA-27 lo cambia sin tocar el script.
  - **7.** Excepciones de `dev` (el `GET …/authorize` que muta y el `POST …/token` sin `Origin`) solo con la feature, y un UT de cada variante: sí. El token se autentica con el `code` de un solo uso y el `code_verifier`, el server solo escucha en loopback por el gate de `BIND_ADDR`, y ninguna de las dos existe en el binario de release. **Para security-reviewer:** revisar que la lista de exenciones de `Origin` sea una tabla cerrada y no un prefijo (`/api/auth/dev/`), para que una ruta `dev` nueva no quede exenta por accidente.
  - **8.** El validador de secretos de `web` queda sin conectar a `env.ts` hasta D: sí. Conectarlo ahora rompería el smoke de `images`, y `web` no usa el secreto hasta D.
- Decisiones del owner: ninguna en este plan, salvo la enmienda de D2 si `oauth2` no pasa `crate-age` o `deny` (pregunta 1).

### architect — 2026-10-28 — APPROVE (ronda 2)
- Verifiqué los tres cambios pedidos y las dos notas para security-reviewer en el cuerpo del plan:
  1. Los secretos de `prod-local` se generan en `infra/local.secrets.env`, ignorado por git y por `.dockerignore`, con `scripts/local-secrets.sh` idempotente, su test y un segundo `--env-file`. Revisa devops-engineer.
  2. A9 tiene la violación de unicidad forzada con el canario en `display_name_key` y el control positivo: el chequeo falla si el log del intervalo no trae ninguna línea `ERROR`, forzada con un `SELEC 1` sin valores.
  3. `state` y `code_verifier` salen de `OsRng` por `CsrfToken::new` y `PkceCodeVerifier::new`, con un UT de fuente contra `new_random`.
  4. El socket con cookie inválida entra como anónimo: no recibe nada de `self` y no consume cupos con sesión.
  5. Las exenciones de `dev` son una tabla cerrada de método y plantilla exacta, vacía sin la feature.
- `oauth2 =5.0.0` sin features por defecto pasa `crate-age` y no trae OpenSSL, `native-tls`, `aws-lc` ni `ring`. Cumple D2 y no hace falta ninguna enmienda ni ninguna decisión del owner. Los duplicados (`rand 0.8`, `rand_chacha 0.3`, `rand_core 0.6`, `thiserror 1`) pasan `just deny`: `deny.toml` tiene `multiple-versions = "warn"` y solo prohíbe varias versiones de `tungstenite`. Quedan justificados en el PR, y ningún secreto sale del `rand 0.8`.

