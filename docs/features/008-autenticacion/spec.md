---
id: "008"
title: Autenticación y sesiones
type: spec
status: approved
risk: high
author: architect
reviewers: [product-owner, security-reviewer, game-designer]
approvals:
  - by: product-owner
    date: 2026-10-28
  - by: game-designer
    date: 2026-10-28
  - by: security-reviewer
    date: 2026-10-28
  - by: owner
    date: 2026-10-28
linear: TA-26
depends_on: ["001"]
created: 2026-10-28
updated: 2026-10-30
---

# Spec: Autenticación y sesiones

> Intent: [intent.md](./intent.md) · ADRs: [0005](../../decisions/0005-economia-sin-dinero-real.md), [0006](../../decisions/0006-despliegue-y-ci.md), [0007](../../decisions/0007-i18n-desde-cero.md), [0008](../../decisions/0008-autenticacion.md) (con su "Nota (2026-10-28): reparto de tests por feature" y su "Enmienda 2026-10-28: errores del callback OAuth"), [0009](../../decisions/0009-ui-design-system.md), [0012](../../decisions/0012-logs-y-cabeceras-de-seguridad.md)

## Resumen
La feature implementa ADR-0008 dentro del alcance del intent, en **cuatro fases, con un PR y un plan (`plan-a.md` … `plan-d.md`) cada una**:
- **A.** Defensas sin BD de dominio: la IP del cliente con `TRUSTED_PROXIES`, los límites por IP del `/ws` anónimo, el rate limiter, el middleware de `Origin` de D4 (reemplaza la capa de solo lectura) y la parte server de D5 (secreto interno).
- **B.** Esquema, ledger y sesiones: usuarios, cuentas OAuth, flujos, sesiones, el ledger de doble entrada con el `grant` inicial, `auth_events`, el proveedor `dev`, login, logout y "cerrar todas", y el `/ws` con sesión.
- **C.** Los proveedores reales y la cuenta: Google y Discord, vincular y desvincular, el renombre, las preferencias, la CLI de admin y ban, y las sesiones de admin.
- **D.** La web y el e2e: `handleFetch` (D5), las pantallas de login, onboarding y cuenta en en/es, la topología del e2e con server y Postgres, CI y la documentación.

Dependencias: B depende de A, C de B y D de C. A se puede mergear sola: protege el `/ws` que ya existe. **Nada de esta spec despliega**: todo corre con `just dev`, `just check`, `just test` y `just e2e` en local.

**`B = 100`** (`DAILY_GRANT`) es el valor provisional que propuso game-designer y aprobó el owner el 2026-10-28 (transmitido por la sesión coordinadora). Su valor definitivo lo fija TA-27.

## Requisitos funcionales

Cada RF dice cómo se verifica (→). "T<n>" es el test mínimo número n de ADR-0008. "IT" es un test de integración de Rust con Postgres (`cargo test`), "UT" uno unitario y "E2E" uno de Playwright.

### Fase A: IP, límites y `Origin`

- **RF-A1. IP del cliente (D7).** `client_ip(peer, headers, internal)` devuelve la IP así:
  - Petición interna válida (RF-A6): `X-Client-Ip`, si es una IP válida; si no, el peer.
  - Peer dentro de `TRUSTED_PROXIES`: la entrada de `X-Forwarded-For` más a la derecha que no esté en `TRUSTED_PROXIES`. Se leen todas las líneas del header en orden. Una entrada mal formada termina la búsqueda y devuelve el peer.
  - Cualquier otro peer: el peer, sin leer `X-Forwarded-For`.
  - Las IPv6 mapeadas a IPv4 (`::ffff:a.b.c.d`) se tratan como IPv4.
  → UT con tabla de casos y T14 (parte del server).
- **RF-A2. `ip_key` (D7).** Es la IPv4 completa o el prefijo `/64` de una IPv6. Es un tipo propio (`IpKey`), nunca un `String`. No implementa `Display`, y su `Debug` imprime `IpKey(..)`. → UT (dos IPv6 del mismo `/64` dan la misma llave) y un UT de que `Debug` no muestra la dirección.
- **RF-A3. `TRUSTED_PROXIES`.** Es una lista separada por comas de direcciones `/32` o `/128`, con el prefijo explícito o sin él. Cualquier otro prefijo hace que el server no arranque (`config.invalid`, `var = TRUSTED_PROXIES`). Por defecto, vacía. Una dirección de loopback solo se acepta si `PUBLIC_ORIGIN` es `http` en loopback, porque el proxy del e2e es local (RF-D7). → UT de config.
- **RF-A4. Rate limiter (D7).** Buckets en memoria, con llave por `IpKey` y por `user_id`, implementados con `tower-governor` o con `governor` directamente si el extractor de dos llaves no encaja (la semántica es la misma).
  - Una petición pasa solo si pasa todos sus buckets. Si no, 429 `http.rate_limited` con `Retry-After` en segundos.
    - *Enmienda 2026-10-30 (ver la Enmienda 2026-10-30 de RF-B5): en las cinco rutas que postea o navega el navegador (`start`, `link`, `callback`, `logout` y `logout-all`), la cuota agotada responde 303 a `/login?error=auth.try_again`, sin `Retry-After`. Los buckets, los límites y el tope no cambian.*
  - Una tarea cada 60 s llama a `retain_recent` para que la memoria no crezca con llaves viejas.
  - **Tope fijo de llaves por limiter: 100 000.** Con el tope lleno, una llave **nueva** recibe 429 (`http.rate_limited`, o `ws.rate_limited` en el upgrade) sin entrar al limiter. **Nunca se desaloja una llave existente**: desalojarla le reiniciaría el bucket a quien lo agotó. El tope es una constante, no una variable de entorno.
  - Los límites son los de la tabla de D7 (la fase A monta los de auth y el upgrade anónimo de `/ws`, y la B los de usuario).
  → IT con reloj controlado: la petición 11 de `start` en un minuto desde la misma IP da 429, y dos IPv6 del mismo `/64` comparten bucket (T22, parte de IP). IT con un tope reducido: lleno el tope, una llave nueva da 429 y una existente sigue con su bucket. Bench del plan-a con el tope lleno: < 20 MB.
- **RF-A5. Middleware de `Origin` (D4).** Reemplaza `http/read_only.rs`.
  - Cubre todo el router: los métodos `POST`, `PUT`, `PATCH` y `DELETE`, y cualquier otro distinto de `GET`, `HEAD` y `OPTIONS`.
  - Petición del navegador: exactamente un `Origin`, igual a `PUBLIC_ORIGIN`. Sin `Origin`, se acepta solo con `Sec-Fetch-Site: same-origin`. Si no, 403 `auth.csrf_origin`.
  - Petición interna (RF-A6): `X-Client-Origin` es obligatorio e igual a `PUBLIC_ORIGIN`, y `X-Client-Fetch-Site`, si viene, vale `same-origin`.
  - El 405 de las rutas que existen y el 404 de las que no se conservan.
  → IT de T2 y T15, y el test de manifiesto de T1. Con un `GET` que muta, el test falla, y su única excepción declarada es `GET /api/auth/{provider}/callback`, que entra en la fase B.
- **RF-A6. Petición interna (D5, lado server).** Una petición es interna solo si `X-Internal-Secret` coincide con `INTERNAL_API_SECRET` en tiempo constante (`subtle`). Solo entonces se leen `X-Client-Ip`, `X-Client-Origin` y `X-Client-Fetch-Site`. Sin secreto válido, esos headers se ignoran. Un secreto incorrecto no da error propio: la petición se trata como externa. → IT de T3.
- **RF-A7. Límites por IP del `/ws` anónimo (D6, D7).**
  - Orden del handshake: `Origin` → upgrade válido → rate de upgrade (30/min, ráfaga 10 por `IpKey`; si se pasa, 429 `ws.rate_limited`) → tope de sockets anónimos por `IpKey` (50; el 51.º recibe 503 `ws.ip_capacity`) → tope global (503 `ws.capacity`).
  - Los contadores se liberan con RAII al cerrarse el socket.
  → IT con el socket 51 y con el upgrade número 11 en ráfaga.
- **RF-A8. Sin CORS (D4).** → T21 (IT con un preflight `OPTIONS` y un `Origin` ajeno).
- **RF-A9. Logs.** No se registra ningún campo nuevo. `X-Client-Origin`, `X-Client-Fetch-Site` y `Sec-Fetch-Site` se agregan a `SENSITIVE_REQUEST_HEADERS`. → Canario de ADR-0012 A8 en el test de captura de logs, con la IP, `X-Client-Origin` y `X-Client-Fetch-Site`.

### Fase B: esquema, ledger, sesiones, proveedor `dev` y `/ws` con sesión

- **RF-B1. Migración.** Crea las tablas de "Modelo de datos". Cada constraint y cada trigger que pueda fallar usa un nombre fijo, y los triggers usan `RAISE` con un `ERRCODE` fijo, sin valores en el mensaje (ADR-0012 A9). → `cargo test` corre las migraciones desde cero.
- **RF-B2. Ledger inmutable (ADR-0005).**
  - No hay columna de saldo. El saldo de un usuario es `SUM(ledger_postings.amount)` de su cuenta.
  - Triggers `BEFORE UPDATE OR DELETE` (por fila) y `BEFORE TRUNCATE` sobre `ledger_entries` y `ledger_postings` abortan la operación con `ERRCODE` `TA001`.
  - **Cuadre:** constraint triggers `AFTER INSERT` diferidos (`DEFERRABLE INITIALLY DEFERRED`) en **`ledger_entries` y en `ledger_postings`**. Al commit, cada asiento tocado tiene al menos dos movimientos y suman 0. Así, un asiento sin movimientos también falla (y no consume el `grant` del día).
  - `created_at` y `grant_day` los fija un trigger `BEFORE INSERT` (`now()` y `(now() AT TIME ZONE 'UTC')::date`), que pisa cualquier valor que mande la aplicación.
  → IT: un `UPDATE`, un `DELETE` y un `TRUNCATE` fallan, un asiento descuadrado, uno con 0 movimientos y uno con 1 no hacen commit, y un `grant_day` enviado por la app se ignora.
- **RF-B3. Primer login (intent, resultado 3).** En **una sola transacción**:
  1. `INSERT users` con un alias generado (RF-B7), con `ON CONFLICT (display_name_key) DO NOTHING` y hasta 10 intentos.
  2. `INSERT oauth_accounts … ON CONFLICT (provider, provider_user_id) DO NOTHING RETURNING`. Si no inserta, otro login concurrente ganó: se hace `ROLLBACK` y se repite como login de un usuario existente.
  3. La cuenta del ledger.
  4. El asiento `grant` de `DAILY_GRANT` monedas (cuenta emisora −B, usuario +B), con `ON CONFLICT DO NOTHING` sobre el índice del día.
  5. `auth_events` de `login` y de `grant`.
  6. La sesión.

  → IT: 20 primeros logins concurrentes con la misma identidad dan un usuario, un asiento y 20 sesiones válidas. Un fallo inyectado en el paso 3 no deja ni usuario ni asiento. El saldo inicial es igual a `DAILY_GRANT`.
- **RF-B4. Login de un usuario existente.** Busca `oauth_accounts`. Con `banned_at` presente, 403 `auth.banned` sin crear sesión (D10). Si no, crea la sesión y escribe `auth_events` de `login`. Nunca crea asientos. → IT.
- **RF-B5. Flujo OAuth (D2).**
  - `POST /api/auth/{provider}/start`: un formulario con `next` opcional y, solo para `dev`, `dev_identity`.
  - Crea la fila en `oauth_flows`, pone la cookie del flujo (RF-B9) y responde 303 a la URL de autorización.
  - `GET /api/auth/{provider}/callback`: consume el flujo con `DELETE … RETURNING`, compara `state` en tiempo constante, canjea el `code` con PKCE `S256`, lee `userinfo`, descarta los tokens del proveedor y responde 303 a `next`. La respuesta lleva `Cache-Control: no-store` y la cookie del flujo siempre se expira.
  - **Errores del callback** (Enmienda 2026-10-28 de ADR-0008): 303 a `/login?error=<code>`. No se crea ni rota una sesión, la cookie del flujo se expira, la respuesta lleva `Cache-Control: no-store` y `next` nunca se usa. `<code>` sale de un enum cerrado de `crates/protocol` (`CallbackErrorCode`), y el server no copia al `Location` nada de la petición ni del proveedor. Los códigos son `auth.invalid_state`, `auth.banned`, `auth.provider_error` (el proveedor falló o devolvió algo inválido), `auth.try_again`, `auth.already_linked` y `auth.provider_already_linked` (estos dos, en la fase C).
  - `next` se valida con el regex de D2.
  - El proveedor `{provider}` que no existe o no está habilitado da 404.
  - **Enmienda 2026-10-30: respuestas de navegación sin JSON (pregunta abierta 2 de plan-d).** Escrita por architect como autor de la spec, tras su revisión de plan-d. Firmada por security-reviewer el 2026-10-30 (ver "Revisión → security-reviewer — 2026-10-30 — APPROVE (ronda 5)"). La aprobación del owner la cubre su delegación del 2026-10-29 (ver "Revisión → owner"). La misma enmienda toca RF-A4, RF-B11, RF-C2, la tabla de API y el Área 13.
    - **Por qué.** `start`, `link`, `logout` y `logout-all` son envíos de formulario que la web manda directo a `/api`, porque solo el server puede poner la cookie del flujo o expirar la de sesión, y `callback` es una navegación top-level que vuelve del proveedor. Todo JSON que respondan lo ve el usuario como texto crudo, el mismo problema que corrigieron la Enmienda 2026-10-28 de ADR-0008 y la desviación 16 de plan-b. Además, `callback` está detrás de la capa `AUTH_IP` (`http/routes.rs`), así que su 429 sale de la capa y no del handler: hasta ahora no era cierto que el callback "mapea todo error a 303".
    - **Alcance.** `POST /api/auth/{provider}/start`, `POST /api/auth/link/{provider}`, `GET /api/auth/{provider}/callback`, `POST /api/auth/logout` y `POST /api/auth/logout-all`. Ninguna otra ruta cambia: `unlink` y las de `/api/me` las llama la web desde form actions y siguen en JSON.
    - **429 y 503 → 303 a `/login?error=auth.try_again`.** Todo 429 de esas rutas (el bucket que sea: `AUTH_IP`, los de mutación por IP o por usuario y el tope de llaves de RF-A4) y todo 503 (la BD caída o un timeout) responde **303** con `Location: /login?error=auth.try_again`. El `Location` se arma en el mismo y único lugar que el del callback, a partir de `CallbackErrorCode`, y nunca copia nada de la petición. Además:
      - sin `Retry-After` y sin cuerpo JSON;
      - con `Cache-Control: no-store` (`private, no-store` en `logout` y `logout-all`, como hoy);
      - nunca usa `next`;
      - en la rama de la cuota, no toca la BD: no crea ni consume una fila de `oauth_flows` ni de `sessions`.
    - **Las cookies en esas ramas.** `start` y `link` no ponen la cookie del flujo. `callback` la expira, como en todo error de RF-B5 ("la cookie del flujo siempre se expira"), y la fila de `oauth_flows`, si existía, vence por su TTL. `logout` y `logout-all` no revocan ni expiran la cookie de sesión, porque la sesión sigue viva y el usuario puede reintentar (igual que la desviación 16 de plan-b).
    - **Lo que sigue en JSON.** El 404 de un proveedor desconocido o apagado y el 403 `auth.csrf_origin`. Solo los produce una petición armada a mano, no un formulario de nuestras páginas.
    - **Sin cambios de tipos.** `auth.try_again` ya está en `CallbackErrorCode`, así que `crates/protocol` y `packages/protocol-ts` no cambian. La cuota sigue siendo la misma, y solo cambia la forma de la respuesta.
    → IT por ruta, en rojo primero: con la cuota agotada (reloj controlado), 303 a `/login?error=auth.try_again` sin `Retry-After`, sin cuerpo JSON, con su `Cache-Control` y sin filas nuevas en `oauth_flows` ni en `sessions`; con la BD cortada (`PgProxy`), 303 igual en las rutas que la leen; en `logout` con la cuota agotada, la sesión sigue válida después. E2E: agotar la cuota de `start` con un solo cliente de test termina en `/login` con el texto de `auth.try_again`, sin JSON.
  → T4 adaptado (cada rechazo da 303 a `/login?error=auth.invalid_state`, sin `Set-Cookie` de sesión y con la cookie del flujo expirada), T5 e IT de cada código de la lista.
- **RF-B6. Proveedor `dev`** (intent, Restricciones).
  - Existe solo con la feature de cargo `dev-auth`, con `PUBLIC_ORIGIN` `http` en loopback **y con `BIND_ADDR` de loopback** (`127.0.0.1` o `::1`). Con la feature activada y otro origen, el server no arranca (`config.invalid`, `var = DEV_AUTH`). Con otro `BIND_ADDR` (incluido el valor por defecto, `0.0.0.0`), tampoco (`config.invalid`, `var = BIND_ADDR`). `Origin` es defensa CSRF, no autenticación: sin este gate, cualquiera en la red local podría pedir una sesión de cualquier `dev_identity`.
  - Implementa en el mismo proceso `GET /api/auth/dev/oauth/authorize`, `POST /api/auth/dev/oauth/token` y `GET /api/auth/dev/oauth/userinfo`. El cliente OAuth los llama por HTTP a `127.0.0.1`, con el mismo código que usa para Google y Discord, y valida el `code_challenge` (S256).
  - La identidad es `dev_identity` (`^[a-z0-9_-]{1,32}$`). Llega desde `start`, viaja en `login_hint` y se guarda como `provider_user_id`. El `provider` es `dev`.
  - El access token que devuelve contiene un canario fijo (RF-B17).
  → UT de config (origen y `BIND_ADDR`), IT del flujo completo y un chequeo de CI de que el binario de release no contiene la cadena `/api/auth/dev/` (RF-D9).
- **RF-B7. `display_name` (D1).** Un módulo `profile::name` normaliza (NFC y recorte), mide en grafemas (3–24), rechaza los rangos `Cc`, bidi y de ancho cero de D1, calcula `display_name_key` (skeleton de UTS #39 sobre NFKC con casefold completo) y compara contra los reservados.
  - Los reservados están en `content/names/reserved.txt`. Las listas del alias, en `content/names/nouns.txt` y `content/names/adjectives.txt`. Se incrustan con `include_str!`.
  - **Enmienda 2026-10-29** (ver la enmienda del mismo nombre en ADR-0008). security-reviewer firmó el 2026-10-29 (https://github.com/kike1303/torneo-absurdo/pull/54#issuecomment-6502944943). architect firmó el 2026-10-29 (https://github.com/kike1303/torneo-absurdo/pull/54#issuecomment-6504044430). security-reviewer revalidó el texto con el invariante de dos formas el 2026-10-29 (https://github.com/kike1303/torneo-absurdo/pull/54#issuecomment-6504090300). Se rechazan además U+2028, U+2029, todos los `Default_Ignorable_Code_Point` y los blancos U+2800, U+13441, U+13442 y U+16FE4. Los reservados se comparan por dos formas: la llave de las letras y los dígitos del nombre (sin signos, espacios ni acentos), y las letras y los dígitos de su llave, llaveados otra vez. Un nombre es reservado si cualquiera de sus formas coincide con cualquiera de las formas de un reservado. **Invariante:** la regla solo agrega rechazos a la comparación por `display_name_key`. La llave del `UNIQUE` no cambia.
  - **Idioma y orden:** español, con el orden sustantivo + adjetivo, y **solo adjetivos invariables en género** (p. ej. `veloz`, `feliz`, `valiente`, `gris`), para que nunca haya un error de concordancia. Cada palabra va capitalizada y sin espacios ni tildes (`TigreVeloz`). Todos ven el mismo alias en en y en es, porque es dato del usuario y no pasa por i18n.
  - **Formato y espacio:** sustantivo (≤ 10 caracteres) + adjetivo (≤ 10) + número de 10 a 999, elegido con `OsRng`. Mínimos: 200 sustantivos y 100 adjetivos, es decir, ≥ 200 × 100 × 990 ≈ 19,8 M combinaciones (≥ 4 M). Pasa por la misma validación.
  - **Test de las listas:** cuenta las palabras y comprueba el producto ≥ 4 M. Comprueba también que no haya palabras duplicadas, que cada combinación cumpla el largo de 3 a 24 y que ninguna palabra (por `display_name_key`) sea un reservado.
  - **Contenido:** las listas las escribe o aprueba **game-designer** con el checklist de ADR-0004 (ningún nombre de franquicia ni de personaje protegido) y revisa que no haya palabras ni combinaciones ofensivas. Queda como paso de revisión del PR de la fase B.
  → T20 (tabla de casos y alias forzado a chocar: reintenta y, tras 10 choques, 503 `auth.try_again` sin crear el usuario) y T19 con el proveedor `dev`.
- **RF-B8. Sesiones (D3).**
  - El token es de 256 bits de `OsRng`, en base64url. En la BD se guarda solo `SHA-256(token)`.
  - Inactividad de 7 días y expiración absoluta de 30 (12 h si el usuario era `admin` al crear la sesión). `last_seen_at` se actualiza como máximo cada 5 min.
  - Un login con una sesión previa en la petición la borra (rotación).
  - Toda respuesta que lee la sesión lleva `Cache-Control: private, no-store`.
  - Una sesión inválida responde 401 `auth.unauthenticated` y expira la cookie: no existe, venció cualquiera de las dos expiraciones o el usuario está baneado.
  → T10 (rotación) y T11.
- **RF-B9. Cookies y su nombre.** Un único `CookiePolicy`, derivado de la config al arrancar, define los nombres y los atributos.
  - **Producción:** `__Host-session` y `__Host-oauth`, con `Secure; HttpOnly; SameSite=Lax; Path=/`, sin `Domain`. Es la única variante que existe sin la feature `dev-auth`.
  - **Loopback de desarrollo** (feature `dev-auth` y origen `http` en loopback): `ta_dev_session` y `ta_dev_oauth`, con los mismos atributos salvo `Secure`.
  - El server lee **un solo** nombre por cookie, el de la política activa.
  → UT con el binario sin la feature: la política es la de producción y no se puede construir otra. IT: con la política de producción, una cookie `ta_dev_session` válida se ignora.
- **RF-B10. Borrado de sesiones (D3).** Una sola función, `sessions::revoke(tx, filtro)`, hace `DELETE FROM sessions … RETURNING id` y `pg_notify('session_revoked', id)` en la misma transacción.
  - Es la única vía: logout, logout-all, rotación, ban, revocación de admin, desvincular y limpieza de expiradas.
  - El server escucha con `PgListener` y cierra en el acto los sockets de cada `id` notificado.
  - Usa **`try_recv()`**, no `recv()`: `recv()` reconecta en silencio y pierde las notificaciones del hueco. `try_recv()` devuelve `Ok(None)` al perder la conexión. En ese punto, y al reconectar, el server revalida contra la BD **todos** los sockets con sesión.
  → T10 (incluido el cierre del socket sin esperar la revalidación). IT: `pg_terminate_backend` sobre la conexión del listener, `revoke` durante el hueco, y el socket se cierra tras la reconexión, antes de la revalidación de 5 min. Además, un chequeo de CI que falla si `DELETE FROM sessions` aparece fuera de esa función.
- **RF-B11. Logout (D10).** `POST /api/auth/logout` y `POST /api/auth/logout-all` revocan la sesión o todas, expiran la cookie y responden 303 a `/`. Sin sesión, también 303: el logout es idempotente. → T10.
  - *Enmienda 2026-10-30 (ver la Enmienda 2026-10-30 de RF-B5): con la cuota agotada o la BD caída, 303 a `/login?error=auth.try_again`, sin revocar ni expirar la cookie.*
- **RF-B12. `GET /api/me`.**
  - Devuelve `display_name`, `role`, `balance`, `providers` (lista de `provider`), `locale`, `theme` y `display_name_is_generated`.
  - No devuelve `user_id`, el id de la sesión ni ningún dato del proveedor.
  - Sin sesión, 401.
  → IT.
- **RF-B13. `GET /api/auth/providers`.** Lista los proveedores habilitados (con credenciales, más `dev` si existe) para que la web dibuje los botones. → IT.
- **RF-B14. `/ws` con sesión (D6).**
  - La cookie de sesión se lee en el upgrade; nunca hay token en la query.
  - **Orden contra la carrera con la revocación:** el socket se registra en `session_id → sockets` **antes** de validar la sesión contra la BD. Si la validación falla, se quita del registro y se cierra. Así, un NOTIFY que llega entre la validación y el registro no se pierde: o encuentra el socket registrado, o la validación ya ve la sesión borrada.
  - Con sesión cuentan 5 sockets por usuario y 200 por `IpKey`, y el upgrade se limita a 10/min por usuario y 120/min por IP. El socket 6.º o el 201.º da 503 `ws.user_capacity` o `ws.ip_capacity`. Los sockets con sesión no cuentan contra los 50 anónimos.
  - Cada socket autenticado revalida su sesión cada 5 min (y toca `last_seen_at` con la regla de RF-B8). Si ya no es válida, cierra con `1008` y `auth.unauthenticated`.
  - **Enmienda 2026-10-29 (desviación 17 de plan-b, revisión de #54):** la segunda validación del upgrade (la que sigue al registro) tiene un timeout propio (5 s). Si da un error de la BD o se pasa del timeout, el socket se cierra con `1008` y `auth.try_again` (falla cerrado), y un cliente puede reconectar. Una sesión que de verdad terminó sigue cerrando con `auth.unauthenticated`. La revalidación periódica corre al lado del loop del socket, con su propio timeout, y un error o un timeout ahí deja el socket abierto: la vía principal es el `NOTIFY`. Revisada por architect el 2026-10-29 (https://github.com/kike1303/torneo-absurdo/pull/54#issuecomment-6504991749); el comportamiento lo aprobó security-reviewer en la ronda 4 de #54 (https://github.com/kike1303/torneo-absurdo/pull/54#issuecomment-6504090300).
  - El registro `session_id → sockets` y `user_id → sockets` vive en memoria.
  → T18 (mensaje de más de 4 KB, suscripción número 9, socket con sesión 201 desde la misma `/64`), IT del 6.º socket e IT de la carrera: con un punto de inyección entre el registro y la validación, un `revoke` en ese hueco cierra el socket.
- **RF-B15. Canales (D6).**
  - `ClientMessage::Subscribe { channel }` y `Unsubscribe { channel }`, donde `channel` es texto en el cable y el server lo convierte a `protocol::Channel`.
  - En esta feature, `Channel` solo tiene `SelfChannel` (`"self"`); los canales públicos llegan con TA-13.
  - Un canal desconocido, mal formado o con id responde `ServerMessage::Error { code: ws.forbidden_channel }` sin abrir la suscripción. Un anónimo que pide `self` recibe lo mismo. La suscripción 9.ª responde `ws.too_many_subscriptions`.
  - `self` entrega `SelfEvent::ProfileChanged { display_name }` cuando el usuario se renombra en otra pestaña (fase C). El primer evento real de saldo llega con TA-27.
  → T17 (con un `SelfHub::publish` de B en el test hasta que exista el renombre, y con el renombre en C).
- **RF-B16. Señales y retención (D8).**
  - `auth_events` guarda `ip_hash = HMAC-SHA256(ip_key, IP_HASH_KEY)` y nunca la IP.
  - Una tarea diaria (y una pasada al arrancar) borra los `auth_events` de más de 30 días, los `oauth_flows` vencidos y las sesiones vencidas (estas, con `sessions::revoke`).
  → T28, parte del `/64`, e IT de la retención. `auth_events` no tiene el trigger de RF-B2, así que el test inserta un `created_at` viejo directamente por SQL.
- **RF-B17. Logs (ADR-0012).**
  - Las rutas `/api/auth/*/callback` y `/api/auth/dev/oauth/*` se registran sin query.
  - Ningún evento nuevo lleva un valor de A4.
  - El test de captura del subscriber corre un login completo con el proveedor `dev` y busca los canarios: el token de sesión, el valor de la cookie del flujo, `code`, `state`, `code_verifier`, el access token canario, `token_hash` (hex y bytes), `X-Internal-Secret`, `X-Client-Origin`, `X-Client-Fetch-Site`, la IP, `ip_key`, `ip_hash`, `user_id`, `provider_user_id`, `display_name` y el `sessions.id` (viaja en el NOTIFY y lo maneja el registro de sockets).
  → T13 extendido (A8).
- **RF-B18. Secretos y config nueva.**
  - `IP_HASH_KEY` e `INTERNAL_API_SECRET` son obligatorios siempre: 32 bytes como mínimo después de decodificar el base64url. Fuera de loopback, además, tienen que ser distintos del valor del `.env.example`. En `just dev`, los `.env.example` de server y web traen el mismo valor de desarrollo, para que las form actions de `web` firmen con el secreto que conoce el server.
  - `DAILY_GRANT` es obligatorio y entero, con **`1 ≤ DAILY_GRANT ≤ 10 000`**. Fuera de ese rango, `config.invalid` con `var = DAILY_GRANT`: un error de tipeo inflaría la economía desde el primer login, y los asientos no se pueden deshacer. La paridad que exige `R = 0.5 B` entero la valida TA-27 cuando exista `R`. El `Debug` de `Config` redacta cada secreto.
  → UT de config (incluidos `0`, `10 001` y un valor no entero de `DAILY_GRANT`) e IT de arranque.

### Fase C: proveedores reales, cuenta, CLI de admin y ban

- **RF-C1. Google y Discord (D1, D2).**
  - Crate `oauth2` con un cliente HTTP `reqwest`, `rustls` con el proveedor `ring` (`cargo deny` prohíbe `aws-lc`) y raíces `webpki-roots`. *(Texto original; el cliente lo reemplaza la Enmienda 2026-10-29 de abajo.)*
  - Scopes `openid` (Google) e `identify` (Discord), y `redirect_uri` `${PUBLIC_ORIGIN}/api/auth/{provider}/callback`.
  - La identidad sale de `userinfo` (Google `sub`, Discord `id`).
  - Un proveedor se habilita solo si tiene `*_CLIENT_ID` y `*_CLIENT_SECRET`. Fuera de loopback, los dos son obligatorios.
  - En el test, la URL base de cada proveedor es inyectable.
  - **Enmienda 2026-10-29 (fase C, preguntas 1 y 2 de plan-c).** Escrita por architect como autor de la spec, tras su revisión de plan-c. security-reviewer la firmó el 2026-10-29 (es la primera conexión TLS saliente): ver "security-reviewer — 2026-10-29 — APPROVE (ronda 4)" en la sección Revisión de esta spec. La misma enmienda toca RF-C3, RF-C5, la tabla de API y los RNF de i18n y de costo.
    - **Cliente HTTP.** No se usa `reqwest`. El cliente es el propio de la fase B (`hyper::client::conn::http1`, desviación 1 de plan-b) con `tokio-rustls` encima (`default-features = false`, features `ring` y `tls12`), `rustls` como dependencia directa sin `logging` y con el proveedor `ring`, y raíces `webpki-roots`. Ofrece solo `http/1.1` por ALPN. `https` va solo a nombres DNS, con validación `webpki` y SNI; `http` sigue limitado a una IP de loopback (el proveedor `dev` y los proveedores falsos de los tests). Se conservan las reglas de B: sin redirecciones, timeout total de 10 s (DNS, conexión, TLS y respuesta), respuesta de 64 KB como máximo y `User-Agent` genérico. No entran `reqwest`, `hyper-rustls`, `native-tls`, `openssl` ni `aws-lc`. Motivo: es la capa con menos superficie, y `rustls`, `ring` y `webpki-roots` ya están en el lock por `sqlx`.
    - **Credenciales fuera de loopback.** "Los dos son obligatorios" se lee así: fuera de loopback (el predicado único de la fase B), el server no arranca sin las cuatro variables `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, `DISCORD_CLIENT_ID` y `DISCORD_CLIENT_SECRET` (`config.invalid` con la variable que falta). `just prod-local` y el job `images` de CI, que corren la imagen de release fuera de loopback, pasan valores de relleno que no son credenciales reales. Ahí los botones de Google y Discord aparecen y no funcionan contra el proveedor real; se acepta, porque en esos entornos no hay login posible.
  → T19 con un servidor HTTP de prueba que imita a cada proveedor (sin red): la URL de autorización pide solo su scope y el alias no coincide con ningún campo devuelto, aunque el falso devuelva `name`, `email`, `username` y `global_name`.
- **RF-C2. Vincular (D1).** `POST /api/auth/link/{provider}` con sesión, mismo flujo con `purpose = link`. En el callback, la sesión actual tiene que ser la de `link_user_id`.
  - Si `(provider, id)` es de otro usuario, el callback responde 303 a `/login?error=auth.already_linked` (RF-B5), sin fusión.
  - Si el usuario ya tiene ese proveedor, 303 a `/login?error=auth.provider_already_linked` (`UNIQUE (user_id, provider)`).
  - Al vincular, el token se rota y se escribe `auth_events` de `link`.
  - **Enmienda 2026-10-29: cuerpo rechazado (la pidió security-reviewer en la ronda 4, ver Revisión).** `link` toma `Result<Form<_>, FormRejection>`, como `start` en la fase B, y convierte todo rechazo del formulario, incluido un cuerpo de más de 4 KB, en 303 a `/login?error=auth.invalid_state`. El texto plano de axum nunca llega al cliente (ADR-0007). → IT con un formulario de 4097 bytes: 303 a `/login?error=auth.invalid_state` y sin el texto de axum.
  - **Enmienda 2026-10-30: `link` sin JSON (ver la Enmienda 2026-10-30 de RF-B5; firmada por security-reviewer en la ronda 5 de "Revisión").** Con la cuota agotada o la BD caída, `link` responde 303 a `/login?error=auth.try_again`, sin poner la cookie del flujo. **Sin sesión** (cookie ausente, vencida o revocada), responde 303 a `/login` **sin `error`**, y conserva el `Set-Cookie` que expira la cookie de sesión. `auth.invalid_state` le daría un texto engañoso a quien solo tiene la sesión vencida: el formulario de login ya es la respuesta. El 404 de un proveedor desconocido o apagado y el 403 `auth.csrf_origin` siguen en JSON. → IT: sin cookie y con una cookie revocada, 303 a `/login`, con `Location` exacto, sin query y con la cookie de sesión expirada; con la cuota agotada y con la BD cortada, 303 a `/login?error=auth.try_again` sin cookie del flujo.
  → T4 (flujo `link` de otra sesión), T6 e IT.
- **RF-C3. Desvincular (D1).** `POST /api/auth/unlink/{provider}` exige que quede otro proveedor (si no, 409 `auth.last_provider`). Revoca todas las demás sesiones del usuario y escribe `auth_events` de `unlink`. → IT.
  - **Enmienda 2026-10-29 (fase C, pregunta 5 de plan-c; firmada por security-reviewer en la ronda 4, ver Revisión).** Si el usuario no tiene vinculado ese proveedor, 404 `http.not_found`. "Que quede otro proveedor" cuenta solo los proveedores que siguen vinculados **y** están habilitados en la config del proceso: uno apagado no deja entrar al usuario. Desvincular un proveedor apagado se permite, siempre que quede uno habilitado. El conjunto habilitado es config estática, así que no abre una carrera. → IT de los dos casos.
  - **Enmienda 2026-10-29: cuerpo (la pidió security-reviewer en la ronda 4, ver Revisión).** `unlink` no lee el cuerpo (como `logout`), así que `DefaultBodyLimit` no tiene nada que cortar y no hay rechazo posible; nunca responde con el texto de axum. Si alguna vez lee un formulario, lo mapea igual que `link` (RF-C2). → IT con un formulario de 4097 bytes: ninguna respuesta lleva el texto de axum.
- **RF-C4. Renombre.** `PUT /api/me/display-name` con `{ "display_name": … }`. Responde 200 con el perfil, 422 `profile.display_name_invalid`, 422 `profile.display_name_reserved` o 409 `profile.display_name_taken` (por la violación del índice, sin un "¿existe?" previo). Publica `SelfEvent::ProfileChanged`. → T20 (concurrencia: de dos renombres simultáneos, exactamente uno gana) y A9 (RF-C9).
- **RF-C5. Preferencias (ADR-0009 D3, ADR-0007).** `PUT /api/me/preferences` con `{ "locale"?: "en"|"es", "theme"?: "dark"|"light"|"system" }`. `null` borra la preferencia. Ver la UI en RF-D4. → IT.
  - **Enmienda 2026-10-29: cuerpo rechazado (fase C, pregunta 3 de plan-c; vale también para RF-C4; firmada por security-reviewer en la ronda 4, ver Revisión).** Toda ruta de la fase C que lee un cuerpo JSON (`PUT /api/me/display-name` y `PUT /api/me/preferences`) responde **422 `http.invalid_body`** ante cualquier rechazo del cuerpo: JSON mal formado, `Content-Type` que no es JSON, un campo desconocido (p. ej. `role` o `banned_at`; RF-C8 se cumple igual, porque no cambian), un tipo equivocado, un valor fuera del enum (`"locale": "fr"`) y un cuerpo de más de 4 KB. El mapeo cubre todas las variantes del rechazo de `Json` de axum y el del límite de tamaño, así que el texto plano de axum nunca llega al cliente. Un `display_name` que es un string pero no pasa la validación sigue dando `profile.display_name_invalid` o `profile.display_name_reserved`. `http.invalid_body` es un `ApiErrorCode` de `crates/protocol`; su texto en/es llega en la fase D. → IT por variante (incluido el cuerpo de más de 4 KB): 422, `{ "error": "http.invalid_body" }` y nada más en el cuerpo.
- **RF-C6. CLI de admin (D9, D10).**
  - Subcomandos `server admin grant|revoke|ban|unban <user_id>`. `ban` exige `--reason <texto>`.
  - Cada uno, en una transacción: cambia `role` o `banned_at`, revoca todas las sesiones con `sessions::revoke` (la notificación cierra los sockets del proceso que sirve) y escribe una fila en `admin_audit` (`actor = 'cli'`, `admin_id` nulo, `action`, `target`, `payload` con el motivo).
  - Termina con 0, o con 1 y un evento de log con código si el usuario no existe.
  - El único SQL que escribe `role` o `banned_at` vive en el módulo de la CLI.
  - La CLI es una fuente de logs nueva (A8): sus eventos no llevan el `user_id` ni el motivo.
  → T9, el criterio de ban del intent (con el usuario baneado, sus sesiones dan 401, su socket se cierra y el login da `auth.banned`; tras `unban` vuelve a entrar) y un chequeo de CI del SQL. Test de captura de `server admin ban <user_id> --reason <canario>`, con éxito y con un usuario inexistente: ni el `user_id` ni el motivo aparecen en los logs.
- **RF-C7. Sesiones de admin (D9).** La expiración absoluta es de 12 h si el usuario es `admin` al crear la sesión. El extractor `RecentAdmin` exige `role = admin` y `authenticated_at` de hace menos de 60 min; si no, 403 `auth.reauth_required`. En esta feature no hay rutas de admin, así que el extractor solo se prueba con una ruta de test. → IT.
- **RF-C8. Ninguna ruta escribe `role` ni `banned_at`.** → T9: un test manda `role` y `banned_at` en el body de cada ruta de perfil y comprueba que no cambian.
- **RF-C9. A9.** El Postgres de dev, de test y del e2e corre con la configuración de ADR-0012 A9. → Test que provoca un renombre que choca y lee el log del contenedor de Postgres de test (`docker logs`): el alias no aparece. El plan-c fija cómo lo corre `just check`.
- **RF-C10. Apps OAuth de desarrollo.** Un documento (`apps/server/README.md`, sección "Login real en local") explica cómo registrar apps de desarrollo de Google y Discord con `redirect_uri` `http://localhost:5173/api/auth/{google,discord}/callback`, y qué variables poner en `apps/server/.env`. → Revisión del PR.

### Fase D: web, e2e y CI

- **RF-D1. `handleFetch` (D5).**
  - Solo si el origen de la URL es exactamente el de `INTERNAL_API_URL`, agrega la cookie de sesión de la política (RF-D2), `X-Internal-Secret`, `X-Client-Ip` (`getClientAddress()`) y `X-Client-Origin` y `X-Client-Fetch-Site` (copia literal; si no venían, no se envían).
  - Cualquier otro `fetch` sale sin nada de eso.
  - `web` valida `INTERNAL_API_URL` e `INTERNAL_API_SECRET` al arrancar con la regla de RF-B18, y los mensajes de error nunca llevan el valor.
  → T16 (UT) y T14 (parte web, UT).
- **RF-D2. Cookie que reenvía la web.** `__Host-session` siempre, y `ta_dev_session` solo si `PUBLIC_ORIGIN` es `http` en loopback. El server ignora la que no corresponde a su política. → UT.
- **RF-D3. `locals.user`.** `hooks.server.ts` llama a `GET /api/me` por `INTERNAL_API_URL` cuando hay cookie de sesión. Con 401, `locals.user = null`. Toda página con `locals.user` lleva `Cache-Control: private, no-store`. Ningún `load` ni `GET` llama a una ruta de mutación (D5). → UT y E2E (un chequeo estático en `check-kit-config` o en un test que busca `fetch` con métodos de mutación en `load`).
- **RF-D4. Preferencias.** Precedencia del tema, según ADR-0009 D3: perfil (con sesión) → cookie `theme` → `dark`. La del idioma, para una ruta sin prefijo: perfil → cookie `locale` → `Accept-Language` → `en`.
  - En el login, un valor del perfil reescribe la cookie.
  - Con sesión, cambiar el tema o el idioma escribe la cookie y el perfil, este por una form action con `PUT /api/me/preferences`.
  - Sin preferencia en el perfil (`null`), manda la cookie.
  → UT del resolvedor y E2E.
- **RF-D5. Pantallas (mobile-first, 375 px, en/es).**
  - `/[locale]/login`: un botón por proveedor de `GET /api/auth/providers`, cada uno un `<form method="post" action="/api/auth/{p}/start">` con `next`. Con `dev`, un campo `dev_identity`. Los errores llegan por `?error=<code>` (RF-D6).
  - `/[locale]/onboarding`: muestra el alias generado y ofrece cambiarlo (form action) u omitir. Se llega por `next` en el primer login, cuando `display_name_is_generated` es verdadero.
  - `/[locale]/account`: alias (editar), saldo, proveedores vinculados (vincular o desvincular), "Cerrar sesión" y "Cerrar todas las sesiones".
  - El encabezado muestra "Entrar" o el alias y el saldo.
  - Cada pantalla tiene sus estados de cargando, error (por código) y vacío.
  - Todo texto vive en `messages/{en,es}.json`. El alias es dato.
  → E2E y `check-i18n`.
- **RF-D6. Errores del callback en la UI.** `/[locale]/login` lee `error` y lo resuelve a una llave i18n (`auth_error.<code>`) **solo** si es uno de los códigos de `CallbackErrorCode`, importados de `packages/protocol-ts`. Un valor desconocido muestra un texto genérico y nunca se refleja (ni en la página, ni en un atributo, ni en un log). Con sesión, la página muestra el error y un enlace a la cuenta. → UT del resolvedor y E2E: un callback con `state` inválido termina en `/login` con el texto traducido, y `?error=<canario>` no deja el canario en el HTML.
- **RF-D7. Topología del e2e.**
  - `just e2e` corre `db-up`, crea la base `torneo_e2e` desde cero, compila el server con `--features dev-auth` y levanta en `127.0.0.1` el server, la web y un proxy de borde (`apps/web/e2e/support/edge.mjs`) que imita a Traefik.
  - El proxy enruta `/api` y `/ws` al server y lo demás a la web, y agrega `X-Forwarded-For`.
  - El server tiene `PUBLIC_ORIGIN = http://localhost:4173` (el origen del proxy) y `TRUSTED_PROXIES = 127.0.0.1/32`.
  - La instancia sin catálogo sigue solo con web.
  - Nada sale de loopback ni usa la red.
  → `just e2e`.
- **RF-D8. E2E de login.** En Chromium y WebKit, con emulación móvil (375 px y táctil): login con `dev`, onboarding con renombre, recarga con sesión, logout, otra vez login con la misma identidad (mismo alias), y en/es. Un usuario baneado por la CLI ve que su sesión se cierra. → E2E.
- **RF-D9. CI.**
  - El filtro `changes` dispara `e2e` también con cambios en `apps/server`, `crates/protocol` y `content/names`.
  - El job `e2e` tiene Postgres y el binario del server con `dev-auth`.
  - Un paso comprueba que el binario de la imagen de release no contiene `/api/auth/dev/` ni `ta_dev_session`.
  - Chequeo estructural en `check-compose.mjs`: `dev-auth` no está en `default` de `apps/server/Cargo.toml`, y la línea de `cargo build` del `Dockerfile` no lleva `--features` ni `--all-features`.
  - Todo pasa `check-workflows` (permisos, actions fijadas por SHA y sin `${{ }}` en `run:`).
  → CI.
- **RF-D10. Documentación.**
  - `CLAUDE.md`: `just e2e` ahora necesita Docker, y explica la feature `dev-auth`.
  - `docs/architecture.md`: las tablas, el ledger y `LISTEN/NOTIFY`.
  - `.env.example` de server y web, solo con marcadores.
  → Revisión del PR.

## Requisitos no funcionales
- **Memoria (VPS de 1 vCPU / 4 GB, server de 256 MB por ADR-0006):**
  - El rate limiter se limpia cada 60 s, y su tamaño está acotado por llaves activas. Con 100 k llaves activas, la memoria tiene que quedar por debajo de 20 MB (lo mide un bench en el plan-a).
  - El registro de sockets está acotado por `WS_MAX_CONNECTIONS`.
  - El pool de Postgres sigue en 8 conexiones, más una para `PgListener`.
- **CPU:** no hay hashing costoso en caliente (SHA-256 y HMAC). La validación UTS #39 corre solo en el renombre y al generar el alias.
- **Determinismo:** no toca el motor (ADR-0001). `OsRng` solo para tokens, `state`, `code_verifier` y el alias, todo fuera de `crates/engine`.
- **i18n (ADR-0007):** el server responde solo códigos. Códigos nuevos: `auth.csrf_origin`, `auth.invalid_state`, `auth.banned`, `auth.provider_error`, `auth.try_again`, `auth.unauthenticated`, `auth.already_linked`, `auth.provider_already_linked`, `auth.last_provider`, `auth.reauth_required`, `profile.display_name_invalid`, `profile.display_name_reserved`, `profile.display_name_taken`, `http.rate_limited`, `ws.rate_limited`, `ws.ip_capacity`, `ws.user_capacity`, `ws.forbidden_channel` y `ws.too_many_subscriptions`. Cada uno tiene su texto en en y es.
  - **Enmienda 2026-10-29 (firmada por security-reviewer en la ronda 4, ver Revisión):** se suma `http.invalid_body` (RF-C5), también con texto en en y es (fase D).
- **Accesibilidad:** los botones de proveedor son `<button>` con texto visible, y los errores de formulario van con `aria-describedby`. Objetivo táctil ≥ 44 px.
- **Seguridad:** todo ADR-0008 dentro del alcance, ADR-0012 (A4, A8 y A9) y el repo público (sin secretos).
- **Costo:** sin servicios de pago. Las dependencias nuevas (`oauth2`, `reqwest` con `rustls` y `ring`, `governor`/`tower-governor`, `sha2`, `hmac`, `subtle`, `rand`, `base64`, `unicode-normalization`, `unicode-segmentation`, `unicode-security`, un crate de casefold completo) pasan `just crate-age` y `just deny`.
  - **Enmienda 2026-10-29 (firmada por security-reviewer en la ronda 4, ver Revisión):** sale `reqwest` con `rustls` y `ring`, y entra `tokio-rustls` (`default-features = false`, `ring` y `tls12`) como único crate nuevo de producción de la fase C. `rustls` (sin `logging`) y `webpki-roots` pasan a dependencias directas, en las versiones que ya están en el lock. Sin `aws-lc`, con una sola versión de `rustls`, verificado con `just deny` y `cargo tree -d -p server`.

## Diseño

### Componentes afectados
| Lugar | Cambio | Fase |
|---|---|---|
| `apps/server/src/net/` (nuevo) | `client_ip`, `IpKey`, `TrustedProxies` | A |
| `apps/server/src/http/origin.rs` (nuevo) y `read_only.rs` (se borra) | middleware D4 y detección de petición interna | A |
| `apps/server/src/rate/` (nuevo) | buckets y limpieza | A |
| `apps/server/src/ws/` | límites por IP y por usuario, registro de sockets, canales, revalidación | A, B |
| `apps/server/migrations/` | una migración por fase que crea tablas (B y C) | B, C |
| `apps/server/src/auth/` (nuevo) | flujos, proveedores, `dev` (tras `cfg(feature = "dev-auth")`), callback, login, link y unlink | B, C |
| `apps/server/src/sessions.rs` (nuevo) | token, validación, `revoke`, listener | B |
| `apps/server/src/ledger/` (nuevo) | cuentas, asientos, saldo | B |
| `apps/server/src/profile/` (nuevo) | `name`, alias, `GET /api/me`, renombre, preferencias | B, C |
| `apps/server/src/cli.rs` y `admin.rs` | `admin grant|revoke|ban|unban` | C |
| `apps/server/src/jobs.rs` (nuevo) | retención diaria | B |
| `apps/server/src/config.rs` | `TRUSTED_PROXIES`, `INTERNAL_API_SECRET`, `IP_HASH_KEY`, `DAILY_GRANT`, proveedores, `CookiePolicy` | A, B, C |
| `crates/protocol` | códigos nuevos, `Channel`, `ClientMessage::{Subscribe, Unsubscribe}`, `ServerMessage::{Subscribed, Unsubscribed, Error, SelfEvent}`, `Me`, `Provider` | A, B, C |
| `packages/protocol-ts` | `just gen` | A, B, C |
| `content/names/` | `adjectives.txt`, `nouns.txt`, `reserved.txt` | B |
| `apps/server/Dockerfile` y `.dockerignore` | `COPY content/names` y build sin `dev-auth` | B |
| `apps/web/src/hooks.server.ts`, `lib/server/api.ts`, `env.ts` | `handleFetch`, `locals.user`, preferencias | D |
| `apps/web/src/routes/[locale]/{login,onboarding,account}` y `messages/` | pantallas e i18n | D |
| `apps/web/e2e/`, `playwright.config.ts`, `justfile`, `.github/workflows/ci.yml` | topología del e2e y CI | D |
| `CLAUDE.md`, `docs/architecture.md`, `.env.example` | documentación | D |

No se tocan `crates/engine`, `crates/sim`, `crates/engine-wasm` ni `packages/renderer`.

### Modelo de datos
```sql
users (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  display_name text NOT NULL,
  display_name_key text NOT NULL CONSTRAINT users_display_name_key_uq UNIQUE,
  display_name_is_generated boolean NOT NULL DEFAULT true,
  role text NOT NULL DEFAULT 'user' CHECK (role IN ('user','admin')),
  locale text NULL CHECK (locale IN ('en','es')),
  theme text NULL CHECK (theme IN ('dark','light','system')),
  created_at timestamptz NOT NULL DEFAULT now(),
  banned_at timestamptz NULL
)
oauth_accounts (
  provider text NOT NULL CHECK (provider IN ('google','discord','dev')),
  provider_user_id text NOT NULL,
  user_id uuid NOT NULL REFERENCES users ON DELETE RESTRICT,
  created_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (provider, provider_user_id),
  CONSTRAINT oauth_accounts_user_provider_uq UNIQUE (user_id, provider)
)
oauth_flows (
  id_hash bytea PRIMARY KEY,             -- SHA-256 del id de la cookie
  provider text NOT NULL, purpose text NOT NULL CHECK (purpose IN ('login','link')),
  link_user_id uuid NULL REFERENCES users, state text NOT NULL, code_verifier text NOT NULL,
  next text NOT NULL, dev_identity text NULL, expires_at timestamptz NOT NULL,
  CHECK ((purpose = 'link') = (link_user_id IS NOT NULL))
)
sessions (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),   -- handle interno (registro de sockets, NOTIFY)
  token_hash bytea NOT NULL UNIQUE, user_id uuid NOT NULL REFERENCES users,
  created_at, authenticated_at, last_seen_at, expires_at timestamptz NOT NULL,
  ip_hash bytea NOT NULL, user_agent_family text NULL
)
ledger_accounts (
  id uuid PRIMARY KEY,
  kind text NOT NULL CONSTRAINT ledger_accounts_kind_ck CHECK (kind IN ('user','issuance')),
  user_id uuid NULL UNIQUE REFERENCES users,
  CONSTRAINT ledger_accounts_user_ck CHECK ((kind = 'user') = (user_id IS NOT NULL))
)                                         -- la migración crea la única cuenta 'issuance'
ledger_entries (                          -- el asiento
  id uuid PRIMARY KEY,
  kind text NOT NULL CONSTRAINT ledger_entries_kind_ck CHECK (kind IN ('grant')),  -- TA-27 agrega tipos
  user_id uuid NULL REFERENCES users,       -- nulo solo para asientos sin usuario (p. ej. rake de TA-27)
  grant_day date NOT NULL,                  -- lo fija el trigger
  created_at timestamptz NOT NULL,          -- lo fija el trigger
  CONSTRAINT ledger_entries_user_ck CHECK (kind NOT IN ('grant') OR user_id IS NOT NULL)
)
CREATE UNIQUE INDEX ledger_entries_grant_day_uq ON ledger_entries (user_id, grant_day) WHERE kind = 'grant';
ledger_postings (                         -- los movimientos del asiento (doble entrada)
  entry_id uuid NOT NULL REFERENCES ledger_entries, account_id uuid NOT NULL REFERENCES ledger_accounts,
  amount bigint NOT NULL CHECK (amount <> 0), PRIMARY KEY (entry_id, account_id)
)
auth_events (
  id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, user_id uuid NOT NULL REFERENCES users,
  kind text NOT NULL CHECK (kind IN ('login','link','unlink','grant')),
  provider text NULL, ledger_entry_id uuid NULL REFERENCES ledger_entries,
  ip_hash bytea NOT NULL, created_at timestamptz NOT NULL DEFAULT now()
)
admin_audit (                             -- fase C; solo inserción (mismo trigger que el ledger)
  id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, actor text NOT NULL CHECK (actor IN ('cli')),
  admin_id uuid NULL REFERENCES users, action text NOT NULL, target text NOT NULL,
  payload jsonb NOT NULL, at timestamptz NOT NULL DEFAULT now()
)
```
- Los montos son `bigint` (`Coins` enteros). El saldo es un `SUM`. Si algún día hace falta un saldo materializado, lo decide un ADR con su verificación (`docs/architecture.md` lo deja abierto).
- **Puntos de extensión para TA-27.** Son extensiones previstas, no reescrituras: TA-27 las hace en su propia migración y no toca datos ni la migración de esta feature.
  - `ledger_entries`: reemplaza `ledger_entries_kind_ck` con los tipos nuevos (`bailout`, `bet_stake`, `bet_payout`, `refund`, `rake`, `admin_adjustment`) y `ledger_entries_user_ck` para exigir usuario también en `bailout` y los demás asientos de un usuario. Agrega columnas nulas (`bet_id`, `fight_id`) y el índice único de `bailout`.
  - `ledger_accounts`: reemplaza `ledger_accounts_kind_ck` con los tipos de cuenta sin usuario que necesite (p. ej. `sink` para el rake que se destruye, ADR-0005, y `escrow` por pelea para los stakes del pozo), y `ledger_accounts_user_ck` se ajusta para que solo `user` lleve `user_id`.
  - Los triggers de inmutabilidad y cuadre no cambian.
- `display_name_is_generated` pasa a `false` con el primer renombre.

### API
| Método y ruta | Sesión | Respuesta | Fase |
|---|---|---|---|
| `POST /api/auth/{provider}/start` | no | 303 al proveedor y cookie del flujo; 429 o 503: 303 a `/login?error=auth.try_again` (Enmienda 2026-10-30) | B (dev), C |
| `GET /api/auth/{provider}/callback` | opcional | 303 a `next` y cookie de sesión; si falla, 303 a `/login?error=<code>`, incluido el 429 de la capa (Enmienda 2026-10-30) | B, C |
| `GET /api/auth/providers` | no | `["google","discord","dev"]` (los habilitados) | B |
| `POST /api/auth/link/{provider}` | sí | 303 al proveedor; sin sesión, 303 a `/login`; 429 o 503: 303 a `/login?error=auth.try_again` (Enmienda 2026-10-30) | C |
| `POST /api/auth/unlink/{provider}` | sí | 303 a `/` (desde form); 404 `http.not_found` si no está vinculado (Enmienda 2026-10-29) | C |
| `POST /api/auth/logout`, `/logout-all` | opcional | 303 a `/`; 429 o 503: 303 a `/login?error=auth.try_again` (desviación 16 de plan-b y Enmienda 2026-10-30) | B |
| `GET /api/me` | sí | `Me` | B |
| `PUT /api/me/display-name` | sí | `Me`, error de `profile.*` o 422 `http.invalid_body` (Enmienda 2026-10-29) | C |
| `PUT /api/me/preferences` | sí | `Me` o 422 `http.invalid_body` (Enmienda 2026-10-29) | C |
| `GET /api/auth/dev/oauth/{authorize,userinfo}`, `POST …/token` | no | OAuth de prueba | B, solo `dev-auth` |

Todos los errores son `{ "error": "<code>" }` (como `http.*` hoy). Toda mutación pasa por RF-A5.

### Flujo del primer login (proveedor `dev`)
```
navegador ──POST /api/auth/dev/start (Origin ok, dev_identity, next)──▶ server
          ◀─303 /api/auth/dev/oauth/authorize?state&code_challenge&login_hint, Set-Cookie flujo
          ──GET authorize─▶ server(dev) ──303 /api/auth/dev/callback?code&state──▶
          ──GET callback (cookie flujo)──▶ server: DELETE…RETURNING flujo · compara state
                server ──POST token (code, verifier)──▶ server(dev)  [HTTP a 127.0.0.1]
                server ──GET userinfo──▶ server(dev) → provider_user_id
                BEGIN users·oauth_accounts·ledger_accounts·grant·auth_events·sessions COMMIT
          ◀─303 next (/es/onboarding), Set-Cookie sesión, expira cookie flujo
```

### Revocación entre procesos
```
CLI (docker exec server admin ban) ── tx: UPDATE banned_at · sessions::revoke → pg_notify ── COMMIT
server (serve) ── PgListener 'session_revoked' ──▶ registro: cierra sockets de cada id
                 └─ al reconectar el listener: revalida todos los sockets con sesión
```

### UI
Mobile-first a 375 px, con los tokens y las primitivas de ADR-0009 y sin colores literales. Las pantallas y estados están en RF-D5. En desktop, el contenido se centra en una columna de 28 rem como máximo. El login sin JavaScript funciona porque es un formulario HTML. El onboarding y la cuenta usan form actions con mejora progresiva.

## Áreas de preocupación
1. **La CLI es otro proceso.** `docker exec server admin ban` no ve el registro de sockets en memoria del proceso que sirve, y D3 pide cerrar los sockets "en el acto". Se resuelve con `LISTEN/NOTIFY` (RF-B10). Si el listener está caído, la revalidación de 5 min queda como respaldo y el listener al reconectar revalida todo. El payload es el `id` interno de la sesión, que no es secreto ni dato de A4. security-reviewer debe confirmar que `pg_notify` no deja el payload en los logs de Postgres con la configuración de A9.
2. **`locale` y `theme`: esta spec se aparta de lo que recomendé en el intent.** Recomendé "solo columnas", pero ADR-0009 D3 (aprobado) ya decidió: "con sesión, la preferencia se guarda en el perfil… al iniciar sesión, el valor del perfil reescribe la cookie". Solo columnas contradiría ese ADR, así que la spec implementa la sincronización (RF-C5 y RF-D4).
3. **Errores del callback en una navegación top-level.** Antes, ADR-0008 D2 y T4 fijaban un 400 con JSON, que en el celular se veía crudo. La "Enmienda 2026-10-28" de ADR-0008, aprobada por el owner y pendiente de security-reviewer, lo cambia por un 303 a `/login?error=<code>` con las condiciones de seguridad: lista cerrada, sin reflejar, sin sesión y con la cookie del flujo expirada. El server lo hace en RF-B5 (fase B) y la web en RF-D6 (fase D). Riesgo que queda: entre la fase B y la D, `/login` todavía no existe y el 303 termina en un 404 de la web. No se despliega nada, así que es aceptable.
4. **CI del e2e.** Hoy el job `e2e` corre en la imagen de Playwright sin Docker, Rust ni Postgres, y los `services:` de GitHub Actions no aceptan `command`, así que no reciben los flags de A9. Opciones que decide el plan-d: (a) un service container de Postgres con los flags puestos por `ALTER SYSTEM` y `pg_reload_conf()` antes de los tests, y el binario del server compilado en otro job y pasado como artifact; o (b) correr el e2e fuera del contenedor, lo que devuelve la dependencia del mirror de apt que 001 evitó. El plan tiene que dar la duración estimada y mantenerse bajo el timeout de 20 min.
5. **`TRUSTED_PROXIES` con loopback.** El e2e necesita `127.0.0.1/32`. RF-A3 lo permite solo con un origen de loopback, para que un error de configuración en producción no convierta a cualquier proceso local en proxy de confianza.
6. **`dev` en el `CHECK` de `oauth_accounts.provider`.** La migración es la misma en todos los entornos, así que la BD de producción acepta `dev`. No es explotable: el binario de release no tiene el proveedor y nadie puede insertar esa fila sin acceso a la BD. Separar la migración por entorno costaría más de lo que protege. Lo dejo explícito para security-reviewer.
7. **Memoria del rate limiter frente a la rotación de `/64`.** Desde un `/48` o un `/32` de IPv6, un atacante crea llaves nuevas más rápido de lo que `retain_recent` las limpia. Por diseño, el tope fijo de RF-A4 acota la memoria. El costo: con el tope lleno, un usuario legítimo con una IP nueva recibe 429 hasta que `retain_recent` libere llaves. Es preferible a un OOM que tire todos los sockets.
8. **`rustls` y `cargo deny`.** El proveedor por defecto de `rustls` es `aws-lc-rs`, que `just deny` prohíbe. El plan-c fija `ring` y `webpki-roots` (la imagen distroless no trae un bundle de CA que mantengamos). Con la Enmienda 2026-10-29 de RF-C1, la pieza TLS es `tokio-rustls` sin features por defecto (las suyas traen `aws_lc_rs`), así que el riesgo de unificación de features se revisa con `cargo tree -d` al agregarla.
9. **El contexto de build del Dockerfile.** Hoy solo copia `apps/server` y `crates`. `include_str!` de `content/names/` obliga a copiarlo, y `content/` tiene todos los derechos reservados. Va dentro del binario igual que va en el repo público, así que no cambia nada de licencia, pero `check-dockerignore` tiene que seguir dejando fuera `content/_local/`.
10. **`B = 100`.** `DAILY_GRANT` no tiene valor por defecto en el código. La fase B agrega `DAILY_GRANT=100` a `apps/server/.env.example`. Cuando TA-27 fije el valor definitivo, solo cambian los ejemplos y la configuración.
11. **Sesión anónima y CGNAT.** Los 50 sockets anónimos por IP pueden quedarse cortos en el main event detrás de un CGNAT grande. Es el número que fijó ADR-0008, así que esta spec no lo cambia. La prueba de carga es de TA-6 y TA-8.
12. **WebKit y las cookies `Secure` en `http://localhost`.** Por eso existe la política de loopback (RF-B9). `just prod-local` usa el binario de release con `__Host-` sobre `http://127.0.0.1:3000`, donde WebKit puede no guardar la sesión. En `prod-local` el login solo se garantiza en Chromium, y queda escrito en `CLAUDE.md`.
13. **Navegaciones del navegador frente a las respuestas JSON y la CSP.** Dos reglas chocaban con el login real. (a) RF-A4 fija un 429 JSON con `Retry-After`, y `start`, `link`, `callback`, `logout` y `logout-all` son navegaciones de formulario o top-level donde ese JSON se ve crudo. La Enmienda 2026-10-30 de RF-B5 lo resuelve con un 303 a `/login?error=auth.try_again` solo en esas cinco rutas, sin cambiar la cuota. (b) ADR-0012 B1 fija `form-action 'self'`, y el 303 de `start` y de `link` a Google o a Discord sigue al envío de un formulario, así que Chromium lo bloquea. La "Enmienda 2026-10-30" de ADR-0012 (B4.6) suma exactamente las fuentes de los dos endpoints de autorización, con su ruta. La de RF-B5 la firmó security-reviewer el 2026-10-30 (ronda 5 de "Revisión"). La de ADR-0012 la firmaron security-reviewer y devops-engineer el 2026-10-30 (ver la "Revisión" de ese ADR) y está en vigor; la aplica el PR #56. WebKit quedó verificado en el e2e de la fase D (plan-d, D35): también aplica `form-action` a las redirecciones, así que sin B4.6 bloqueaba los dos botones, y con B4.6 los deja pasar en los tres proyectos que corren `e2e/auth/` (Chromium de escritorio, Chromium a 375 px y WebKit a 375 px), mientras el formulario inyectado a un webhook de Discord sigue bloqueado. El riesgo que queda es el de la cadena real de cada proveedor (otro host, por ejemplo el IdP de una cuenta de Google Workspace con SSO, falla cerrado y sería otra enmienda): el login real en móvil se prueba a mano en TA-6.

## Respuestas a preguntas abiertas del intent
- **P: ¿Cómo inicia sesión un desarrollador en local?** → R: con las dos vías. El proveedor `dev` (RF-B6) es obligatorio para `just dev` y el e2e, y la guía de apps OAuth de desarrollo (RF-C10) permite probar Google y Discord reales en `just dev`. Decidido por el architect; security-reviewer confirma al revisar esta spec.
- **P: ¿Cookies `__Host-` en `http://localhost` con WebKit?** → R: la política de cookies de RF-B9. Producción solo con `__Host-`, y nombres sin prefijo ni `Secure` solo con la feature `dev-auth` y un origen `http` en loopback, verificado con el binario de release. No hay https local: cuesta certificados y configuración en cada máquina, y no prueba nada que la política no pruebe.
- **P: ¿`locale` y `theme` sincronizados o solo columnas?** → R: sincronizados, porque así lo pide ADR-0009 D3 (Área de preocupación 2).
- **`B` provisional** → `DAILY_GRANT = 100`, propuesto por game-designer y aprobado por el owner el 2026-10-28 (transmitido por la sesión coordinadora).
- **Errores del callback** → 303 a `/login?error=<code>`, por la Enmienda 2026-10-28 de ADR-0008 (aprobada por el owner, transmitida por la sesión coordinadora): RF-B5 y RF-D6.
- **Ban** (decisión del owner) → RF-C6.
- **Reparto de tests** (decisión del owner) → la "Nota (2026-10-28)" de ADR-0008. Esta spec cubre: T1–T6, T9–T11, T13–T17, T19–T22, de T18 el mensaje, la suscripción y los sockets por IP, y de T28 el caso del `/64`.

## Estrategia de pruebas
| Test mínimo de ADR-0008 | Nivel | Fase |
|---|---|---|
| T1 manifiesto de rutas | IT (recorre el router) | A, extendido en B y C |
| T2, T3, T15, T21 | IT | A |
| T14 | UT server (A) y UT web (D) | A, D |
| T22 | IT (IP en A, usuario en B) | A, B |
| T4, T5, T10, T11, T13, T17, T18 parcial, T28 `/64` | IT | B |
| T19 | IT con `dev` (B) y con proveedores falsos (C) | B, C |
| T20 | UT de tabla y alias (B), IT de concurrencia del renombre (C) | B, C |
| T6, T9 | IT | C |
| T16 | UT web | D |
- **Propiedades (proptest, ya disponible en el workspace si el plan lo confirma):**
  - `display_name_key` es idempotente.
  - Todo nombre que pasa la validación mide de 3 a 24 grafemas.
  - `client_ip` nunca devuelve una IP en `TRUSTED_PROXIES`.
- **Concurrencia:** 20 primeros logins concurrentes, dos renombres simultáneos y logout con un socket abierto.
- **Ledger:** inmutabilidad (trigger), cuadre (trigger diferido), saldo = suma, un `grant` por día y ningún asiento al vincular, desvincular o rotar.
- **Logs:** el test de captura (RF-B17) y, para A9, el log del Postgres de test.
- **E2E:** RF-D8 en Chromium y WebKit con emulación móvil, más los e2e existentes sin regresión.
- **CI:** el chequeo de `DELETE FROM sessions`, el del SQL de `role` y `banned_at`, el del binario de release sin `dev-auth` y `check-workflows`.
- Cada fase termina con `just check` y, en D, con `just e2e`, con la salida pegada en el PR.

## Alternativas consideradas
- **Https local con certificados (mkcert o Caddy) en lugar de la política de loopback:** es más fiel a producción, pero exige un paso por máquina y por CI, y WebKit en Linux requiere confiar en la CA. La política de loopback, con su test sobre el binario de release, da la misma garantía con menos piezas.
- **Proveedor `dev` que salta OAuth (crea la sesión directo):** es más simple, pero no ejercita `state`, PKCE, el callback ni los logs del camino real, y el intent lo exige.
- **Cerrar los sockets desde la CLI llamando a un endpoint interno del server:** es un endpoint más que proteger. `LISTEN/NOTIFY` usa la BD, que los dos procesos ya comparten.
- **Saldo materializado en `users`:** contradice "nunca se actualiza un saldo" en su forma más simple. El `SUM` alcanza para pocos usuarios.
- **`grant_day` calculado por la aplicación:** D8 lo prohíbe ("se calcula en la BD").
- **Una sola fase:** el PR sería demasiado grande para revisarlo bien en un tema `risk: high`. La fase A, además, protege el `/ws` de hoy sin esperar al resto.

## Revisión
### product-owner — 2026-10-28 — APPROVE
- Verifiqué cada criterio de éxito del intent aprobado contra un RF:
  - Login, renombre, logout y reingreso con `dev` en en/es: RF-B6 y RF-D8.
  - Emulación móvil en Chromium y WebKit: RF-D8.
  - `grant` inicial del día por `DAILY_GRANT`, que es obligatorio y > 0: RF-B3 y RF-B18. Los 20 logins concurrentes y el fallo a mitad de la transacción: RF-B3.
  - Sin columna de saldo, triggers de inmutabilidad y ningún asiento al vincular, desvincular o rotar: RF-B2 y la estrategia de pruebas del ledger.
  - Tests de ADR-0008 que caen en este alcance, contrastados con la "Nota (2026-10-28)" del ADR: T1–T6, T9–T11, T13–T17, T19–T22, T18 parcial y el `/64` de T28.
  - `/ws` anónimo (socket 51, upgrade 11 en ráfaga) y `X-Forwarded-For` desde un peer no confiable: RF-A1 y RF-A7.
  - La capa de solo lectura desaparece: RF-A5.
  - Lista completa de canarios: RF-B17. A9: RF-C9.
  - Gate del proveedor `dev` y cookies: RF-B6, RF-B9 y RF-D9.
  - Secretos en server y web: RF-B18 y RF-D1.
  - i18n: RF-D5 y RNF.
  - El e2e se dispara con cambios en `apps/server`: RF-D9.
  - Ban por CLI con `admin_audit`, 401, cierre del socket y `auth.banned`: RF-C6 y RF-C8.
- Las tres preguntas abiertas del intent están respondidas. La sincronización de `locale`/`theme` no es scope creep: la exige ADR-0009 D3 (líneas 52–53), que ya está aprobado. Las tres decisiones del owner (`grant` del día, reparto de tests y ban) están reflejadas.
- Sin scope creep: no hay apuestas, rescate, temporadas ni canales públicos, y `SelfEvent::ProfileChanged` es lo mínimo para que T17 pruebe algo real. Respeta ADR-0005 (monedas solo por asiento), ADR-0007 (solo códigos) y ADR-0004 (no aplica). Mobile-first a 375 px, objetivos táctiles de 44 px y login sin JS.
- Notas (no bloquean):
  1. **RF-D6 (JSON crudo en el callback).** Para el espectador en el celular es un callejón sin salida, sobre todo `auth.banned` y un `state` vencido al volver del proveedor. Aceptable mientras nada se despliegue. Si el owner no aprueba la enmienda (Área 3), debe quedar como bloqueante de la apertura al público en TA-6/TA-8.
  2. **Prueba móvil manual con Google y Discord reales** (criterio del intent): no es de esta spec. Que el plan-d la deje anotada en el issue de deploy para que no se pierda.
- Decisiones solo del owner (no las tomo yo):
  - Aprobar `B = 100` (`DAILY_GRANT`), propuesto por game-designer.
  - Aprobar o rechazar la enmienda de ADR-0008 que cambia el error del callback de 400 JSON a 303 `/login?error=<code>` (Área 3). Mi recomendación como product-owner: aprobarla.

### game-designer — 2026-10-28 — REQUEST_CHANGES
- Verifiqué: el ledger contra ADR-0005 (doble entrada, solo inserción con triggers, sin columna de saldo, montos `bigint` enteros) y contra ADR-0008 D8 (`grant_day` fijado por la BD, `UNIQUE (user_id, grant_day) WHERE kind = 'grant'`, el primer login en una transacción con el `grant` del día como monedas iniciales). `DAILY_GRANT` es configuración obligatoria y sin valor por defecto en el código. Cada asiento guarda su monto en `ledger_postings`, así que TA-27 puede cambiar `B` sin migrar asientos. El valor 100 es coherente con `R = 0.5 B = 50` entero y con los ejemplos de D8. La cuenta `issuance` en negativo deja medir la masa monetaria emitida, y eso sirve para vigilar la inflación.
- Cambios pedidos:
  1. **"Sin reescribir nada" no se cumple para el rake ni para el pozo.** `ledger_entries.user_id` es `NOT NULL` y `ledger_accounts.kind` solo acepta `user` e `issuance`. El rake (ADR-0005: "se destruye") y el pozo parimutuel no tienen usuario, así que TA-27 tendría que relajar un `NOT NULL` y reemplazar un `CHECK`. *Cambio:* (a) `ledger_entries.user_id` nulo, con un `CHECK` con nombre que lo exija para `grant` (y que TA-27 extienda a `bailout`), o el compromiso explícito de que todo asiento de TA-27 lleva usuario; (b) decir que TA-27 agrega tipos de cuenta (p. ej. `sink` para el rake y `escrow` para los stakes de una pelea) reemplazando el `CHECK` con nombre de `ledger_accounts.kind` en su propia migración. Esa migración es una extensión prevista, no una reescritura.
  2. **Rango de `DAILY_GRANT`.** Hoy solo se exige "> 0". Un error de tipeo (100000) infla la economía desde el primer login y no se puede deshacer, porque los asientos son inmutables. *Cambio:* un tope de cordura en la validación (p. ej. `1 ≤ DAILY_GRANT ≤ 10 000`, con `config.invalid` y `var = DAILY_GRANT`) y su UT. La paridad que exige `R = 0.5 B` entero la valida TA-27 cuando exista `R`.
  3. **Listas del alias (`content/names/`).** Hoy no existen, y la spec no dice (a) en qué idioma están ni qué orden siguen: "adjetivo + sustantivo" es el orden del inglés, y en español el adjetivo concuerda en género con el sustantivo. Para mostrarle el mismo alias a usuarios en/es, propongo una sola lista en inglés, o palabras invariables con orden fijo; (b) cuántas palabras tiene como mínimo cada lista. Con 10 reintentos y 503 `auth.try_again` al chocar, el espacio debe ser grande: ≥ 200 × 200 × 100 = 4 M combinaciones, y el test de RF-B7 debe comprobarlo; (c) quién revisa el contenido. Que lo escriba o apruebe game-designer con el checklist de ADR-0004 (ningún nombre de franquicia ni de personaje protegido) y sin combinaciones ofensivas. Basta con agregarlo al test de RF-B7 contra `reserved.txt` y a una revisión del PR.

### game-designer — 2026-10-28 — APPROVE (ronda 2)
- Verifiqué mis tres puntos. (1) `ledger_entries.user_id` es nulo, con `ledger_entries_user_ck` que lo exige para `grant`. Los `CHECK` tienen nombre (`ledger_entries_kind_ck`, `ledger_accounts_kind_ck`, `ledger_accounts_user_ck`), y "Puntos de extensión para TA-27" fija que `bailout`, apuestas, `refund`, `rake`, `sink` y `escrow` entran en una migración propia, sin tocar datos ni triggers. (2) `1 ≤ DAILY_GRANT ≤ 10 000` con `config.invalid` y UT de `0`, `10 001` y un valor no entero. La paridad de `R` queda para TA-27. (3) El alias es sustantivo + adjetivo invariable + número de 10 a 999, en español. El largo máximo es 10 + 10 + 3 = 23 ≤ 24. El espacio es ≥ 19,8 M combinaciones, con un test de ≥ 4 M, de duplicados, de largo y de reservados, y la revisión de contenido (ADR-0004 y palabras ofensivas) queda a mi cargo en el PR de la fase B.
- `B = 100` sigue pendiente del owner. Solo cambia los `.env.example`, así que no bloquea esta aprobación.

### security-reviewer — 2026-10-28 — REQUEST_CHANGES
**🤖 security-reviewer**

- Verifiqué la spec contra ADR-0008 (D1 a D10, la "Nota (2026-10-28)" y los tests de 008), ADR-0005, ADR-0012 (A4.13, A8, A9), el intent aprobado y el código actual (`config.rs`: `BIND_ADDR` por defecto `0.0.0.0`, `decide_echo`; `.env.example` con `BIND_ADDR=127.0.0.1`). Está bien resuelto:
  - La política de cookies única (RF-B9, con su IT de que `ta_dev_session` se ignora en producción).
  - `TRUSTED_PROXIES` con loopback solo con origen de loopback (RF-A3).
  - El primer login en una transacción con `ROLLBACK` ante el choque de `oauth_accounts` (RF-B3).
  - `grant_day` y `created_at` fijados por trigger.
  - Los secretos (RF-B18) y la lista de canarios (RF-B17).
  - El `id` de sesión como payload de NOTIFY y no el token.
  - Área 1 confirmada: con A9 (`log_statement = none`, `log_min_error_statement = panic`, `terse`), ni el `pg_notify` ni su payload llegan al log de Postgres.
  - Área 6 aceptada: `dev` en el `CHECK` no es explotable sin acceso a la BD.

  Los hallazgos 1 a 6 son **Important** y bloquean.
- Cambios pedidos:
  1. **El gate de `dev-auth` mira solo `PUBLIC_ORIGIN`, no dónde escucha el server.** *Escenario:* un desarrollador corre el binario con `dev-auth` sin `BIND_ADDR` (por defecto es `0.0.0.0`) en una red compartida. Desde otra máquina, `curl -X POST -H 'Origin: http://localhost:5173' …/api/auth/dev/start -d dev_identity=<la del admin>` sigue el flujo y obtiene una sesión de esa identidad, admin incluido si se le hizo `admin grant`. `Origin` es defensa CSRF, no autenticación. *Cambio:* con `dev-auth`, el server no arranca si `BIND_ADDR` no es de loopback (`config.invalid`, `var = BIND_ADDR`), con su UT. Además, RF-D9 suma a la búsqueda de cadenas un chequeo estructural: `dev-auth` no está en `default` de `apps/server/Cargo.toml`, y la línea de `cargo build` del `Dockerfile` no lleva `--features` ni `--all-features` (en `check-compose.mjs`).
  2. **`PgListener::recv()` reconecta en silencio.** sqlx reconecta dentro de `recv()` y pierde las notificaciones del hueco sin avisar, así que "revalidar al reconectar" (RF-B10) nunca se dispararía. *Escenario:* se reinicia la conexión del listener, la CLI banea en ese hueco y el socket del baneado sigue vivo hasta 5 min. *Cambio:* usar `try_recv()` (devuelve `Ok(None)` al perder la conexión) y revalidar todos los sockets con sesión en ese punto. IT: `pg_terminate_backend` sobre la conexión del listener, `revoke` durante el hueco y el socket se cierra tras la reconexión, antes de la revalidación de 5 min.
  3. **Carrera entre el upgrade y la revocación.** *Escenario:* el upgrade valida la sesión, la revocación hace commit y su NOTIFY se procesa antes de que el socket entre en el registro, y el socket se registra después. Queda abierto con una sesión borrada hasta 5 min. Un cliente que reconecta en bucle tras un logout o un ban gana esa carrera de forma repetible. *Cambio:* registrar el socket en `session_id → sockets` **antes** de validar la sesión (o volver a validarla después de registrarlo), y escribirlo en RF-B14.
  4. **La memoria del rate limiter no está acotada por diseño.** RF-A4 y el Área 7 dejan el tope de llaves condicionado al bench. *Escenario:* desde un `/48` o un `/32` de IPv6 (65 k o 4 G de `/64`), el atacante crea llaves nuevas más rápido que `retain_recent` cada 60 s, y el server de 256 MB cae por OOM con todos los sockets adentro. *Cambio:* un tope fijo de llaves por limiter en esta spec. Al llegarse, una llave nueva recibe 429 `http.rate_limited` (o `ws.rate_limited`) y **nunca se desaloja una llave existente**, porque desalojar le reiniciaría el bucket a quien lo agotó. El bench del plan-a mide con el tope lleno y comprueba < 20 MB.
  5. **El cuadre del ledger no cubre un asiento sin movimientos.** Si el trigger diferido está en `ledger_postings`, un `ledger_entries` sin filas en `ledger_postings` no lo dispara: el asiento queda sin movimientos y consume el `grant` del día. *Cambio:* constraint triggers diferidos en `ledger_entries` y en `ledger_postings`. IT: un asiento con 0 movimientos y otro con 1 no hacen commit.
  6. **Faltan dos canarios (A4.13 y A8).**
     - (a) El `sessions.id` es la llave de una fila de sesión, viaja en el NOTIFY y el registro lo maneja: va en el test de captura de RF-B17.
     - (b) La CLI de admin es una fuente nueva: un test de captura de `server admin ban <user_id> --reason <canario>`, con éxito y con un usuario inexistente, comprueba que ni el `user_id` ni el motivo aparecen en los logs.
- No bloquean:
  - En loopback, un `INTERNAL_API_SECRET` efímero solo lo conoce el server. `web` no puede firmar y sus form actions darían 403. Conviene que en `just dev` venga fijo en los dos `.env` o que el plan lo resuelva.
  - Los triggers de solo inserción no frenan al dueño de la tabla (`ALTER TABLE … DISABLE TRIGGER`). Un rol de BD separado para la app endurece esto, y queda para un ADR de devops.
- Solo el owner decide:
  - `B = 100`.
  - La enmienda del Área 3 (303 a `/login?error=<code>`). Seguridad no se opone, si `error` solo admite un código de una lista cerrada que la web resuelve a una llave i18n (un valor desconocido se muestra genérico y nunca se refleja), no se crea sesión y la cookie del flujo se expira.

### security-reviewer — 2026-10-28 — APPROVE (ronda 2)
**🤖 security-reviewer**

- Verifiqué los 6 hallazgos en el texto revisado:
  1. El gate de `dev-auth` exige `BIND_ADDR` de loopback (RF-B6) y suma el chequeo estructural de `Cargo.toml` y del `Dockerfile` (RF-D9).
  2. `try_recv()` con revalidación total al perder la conexión y al reconectar, más su IT con `pg_terminate_backend` (RF-B10).
  3. El socket se registra antes de validar la sesión (RF-B14).
  4. El tope fijo de 100 000 llaves sin desalojo, con su IT y el bench con el tope lleno (RF-A4, Área 7).
  5. Constraint triggers diferidos en `ledger_entries` y `ledger_postings`, con los casos de 0 y 1 movimientos (RF-B2).
  6. `sessions.id` en los canarios y el test de captura de la CLI (RF-B17, RF-C6).
- Cambio no pedido (RF-B18: secretos siempre obligatorios y el mismo valor de desarrollo en los dos `.env.example`): no contradice el intent. El intent pide rechazar el valor público fuera de loopback, y en producción `PUBLIC_ORIGIN` es `https://…` por el compose, así que el valor de ejemplo siempre se rechaza. Con un `PUBLIC_ORIGIN` de loopback, ese valor solo permitiría falsificar `X-Client-Ip` (la llave de rate limit y el `ip_hash`) en un entorno local, no saltarse D4. Además arregla el 403 de las form actions que señalé.
- Nota para el plan-b, no bloquea: RF-B18 dice "fuera de loopback" sin definirlo. Que server y `web` usen el **mismo** predicado que RF-B9 y RF-A3 (`PUBLIC_ORIGIN` `http` en loopback), con un UT en cada app: el valor del `.env.example` con un origen `https` o que no es de loopback no arranca.
- Owner: `B = 100` es configuración en un rango validado; si el owner elige otro valor, solo cambian los ejemplos. La enmienda del Área 3 tiene resueltos los dos caminos: si se aprueba, entra en C o D con las condiciones de seguridad; si no, bloquea la apertura en TA-6 y TA-8. Ninguno de los dos bloquea esta aprobación.

### security-reviewer — 2026-10-28 — APPROVE (ronda 3: enmienda del callback)
**🤖 security-reviewer**

- Verifiqué RF-B5, RF-C2, RF-D6, la tabla de API y el Área 3 contra la "Enmienda 2026-10-28" de ADR-0008:
  - **Redirección abierta:** no hay. El `Location` es una ruta fija, el código sale de un enum de `crates/protocol` y `next` nunca se usa.
  - **Reflejo:** no hay. La web resuelve el código a una llave i18n solo si está en `CallbackErrorCode`, y su E2E comprueba que un `?error=<canario>` no deja el canario en el HTML.
  - **Cookie del flujo:** se consume y se expira en todo error, así que no se puede reintentar un flujo. No se crea ni se rota ninguna sesión.
  - **Vinculación fallida con sesión:** vuelve a `/login` sin tocar la sesión vigente, y no hay fusión.
- No hay oráculo útil: `auth.already_linked` y `auth.banned` solo los ve quien acaba de autenticarse como esa identidad en el proveedor. Un tercero puede armar `/login?error=auth.banned` y mostrarle a la víctima un mensaje de la lista cerrada, sin sesión ni cambio de estado. Es un engaño de bajo impacto, y lo acepto.
- Nota para el plan-d, no bloquea: el canario de `?error=` también debe buscarse en los logs de `web` (A8), no solo en el HTML. Y un `error` que mande el proveedor (p. ej. `access_denied`, cuando el usuario cancela) se mapea a un código de la lista, sin copiar `error_description`.

### owner

- **APPROVE** (2026-10-28, en chat): aprueba la spec (`risk: high`), `B = 100`, la enmienda del callback y la nota de ADR-0008 sobre el reparto de tests. Sigue plan-a.
- **APPROVE** (2026-10-29, en chat): aprueba la Enmienda 2026-10-29 (RF-C1, RF-C2, RF-C3, RF-C5 y RNF). Sigue la implementación de la fase C.
- **Delegación** (2026-10-29, en chat), textual: "el merge tambien lo puedes hacer y puedes continuar con todo lo que falte sin pedirme permiso, haz todo de una vez, solo ve siguiendo los procesos de los agentes y ya. No importa por ahora que sea de alto riesgo, esto todavía esta en desarrollo y no hay nada montado asi que no pasa nada. Intenta hacer todo sin pedirme las aprobaciones directas a mi". Mientras no haya nada desplegado, las aprobaciones del owner en 008 (enmiendas de la spec y de ADRs `risk: high`, merges) quedan cubiertas por las firmas de los agentes revisores que pide `docs/process.md`. No cubre un tag `v*` ni un deploy.

### security-reviewer — 2026-10-29 — APPROVE (ronda 4: Enmienda 2026-10-29 de RF-C1, RF-C3, RF-C5 y RNF)
**🤖 security-reviewer**

- Verifiqué el diff de la enmienda contra ADR-0008 (D1, D2, D7), ADR-0012 (A3, A8), plan-c (decisiones 3, 4, 5, 7 y 8) y el código de `eb832be` (`auth/start.rs`, `http/routes.rs`, `telemetry.rs`, `deny.toml` y `Cargo.lock`):
  - **Cliente TLS (RF-C1).** `tokio-rustls` sin features por defecto quita sus `aws_lc_rs` y `logging`. `rustls 0.23.46` del lock ya está sin `log` ni `aws-lc-rs`, y `deny.toml` prohíbe `aws-lc-sys`, `openssl` y `openssl-sys`, así que una unificación de features que traiga `aws-lc` rompe `just deny`. Validación `webpki` con SNI y nombre del host, sin verificador `dangerous` en producción y raíces de `webpki-roots` solamente. Las URLs de los proveedores son constantes del código y la inyección es solo por `Settings` (tests), así que ni el `client_secret` ni el `code` pueden ir a un host que elija la config o un atacante. Con DNS controlado, el atacante no pasa la validación del certificado. Sin redirecciones, 10 s en total (con 3 s por intento en plan-c), 64 KB y solo `as_code()` en el log: ni el texto de rustls ni los nombres del certificado llegan al log ni al cliente. Solo `http/1.1` por ALPN, sin h2. El camino `http` sigue limitado a una IP de loopback, y en el binario de release ninguna URL constante es `http`.
  - **Credenciales obligatorias fuera de loopback.** Falla cerrado. Los rellenos de `prod-local` y de `images` no son credenciales: si llegaran a producción, el login fallaría con `auth.provider_error` y no se abriría ningún acceso. Los de `prod-local` son aleatorios y van a `infra/local.secrets.env` (ignorado). El `Debug` redactado de `ClientSecret` y de `Config` evita que el secreto termine en un log.
  - **RF-C3.** El 404 por un proveedor no vinculado solo le dice al usuario lo que ya sabe de su propia cuenta: no es un oráculo. Contar solo los proveedores vinculados **y** habilitados evita dejar una cuenta sin forma de entrar. `SELECT … FOR UPDATE` sobre `users` pone en serie dos desvincular concurrentes (Google y Discord a la vez: el segundo ve 0 y da 409) y un vincular contra un desvincular. El conjunto habilitado es config estática del proceso, así que no hay carrera.
  - **RF-C5.** Un solo código para todo rechazo, sin texto de axum ni de serde (`` unknown variant `fr` `` copia el valor), `deny_unknown_fields` y un IT por variante con 4097 bytes. RF-C8 no depende de eso. Además, el formatter de `telemetry.rs` no escribe campos de targets de terceros, así que el `body_text` del evento `axum::rejection` tampoco llega al log.
- **Punto abierto del architect (cuerpos de más de 4 KB en `link` y `unlink`).** Sí, el texto de axum tiene que quedar bloqueado también ahí, por ADR-0007 (el server responde solo códigos) y por coherencia del contrato. El impacto de seguridad es bajo: el texto le vuelve solo a quien mandó el cuerpo, como `text/plain` con `nosniff`, y no es un hallazgo. Condición para plan-c, que no bloquea esta firma:
  - `link` toma `Result<Form<_>, FormRejection>` igual que `start` en B, y todo rechazo (incluido el de 4 KB) da 303 a `/login?error=auth.invalid_state`.
  - `unlink` no extrae el cuerpo (como `logout`), así que `DefaultBodyLimit` no tiene nada que cortar y no hay rechazo posible. Si alguna vez lee un formulario, lo mapea igual que `link`.
  - Un IT por ruta con un formulario de 4097 bytes: ninguna respuesta lleva el texto de axum.
  - Conviene que architect lo escriba en una línea de RF-C2 y RF-C3, y que cambie el "Pendiente de la firma de security-reviewer" de la enmienda por un enlace a esta ronda.
- No bloquean:
  - Sumar `rustls` y `tokio_rustls` a `CAPPED_TARGETS` de `telemetry.rs`, por si un bump o un cambio de features vuelve a encender `logging`. El contenido ya no llegaría al log, porque de un target de terceros `JsonLine` escribe solo `target` y `"external"`, pero el tope evita el volumen de un handshake por evento en `trace`.
  - `webpki-roots` queda fijado en el binario. Dependabot lo sube, y un CA que se retira solo se refleja tras rebuild y deploy. Se acepta, porque la imagen distroless no tiene otro bundle.
  - **Fuera del alcance de la enmienda, para el owner** (es diseño de ADR-0008 D1): con una sesión robada o un navegador desatendido, alguien puede vincular **su** Google a la cuenta (vincular cuenta como reautenticación) y después desvincular el Discord de la víctima. `unlink` revoca las demás sesiones y la víctima ya no tiene con qué entrar: es una toma de cuenta persistente. Las monedas son ficticias y el requisito previo es fuerte (la cookie es `__Host-`, `HttpOnly` y `Secure`), así que no bloquea esta fase. Propongo un issue: `unlink` exige una autenticación reciente hecha con un proveedor que ya estaba vinculado antes de la sesión actual, o un período de espera para desvincular después de vincular algo nuevo.
- **Sobre `docker exec server admin ban <user_id> --reason …` (contexto de plan-c):** no hace falta que plan-c lo resuelva en el código. Quien ve los argumentos en `docker events`, en `ps` o en el historial del shell ya es root o del grupo `docker` en el VPS, y eso le da acceso a la BD. Basta una línea en la sección "Admin y ban por CLI" de `apps/server/README.md`: el motivo queda en el historial del shell y en los eventos de Docker, así que nunca lleva datos personales.

### security-reviewer — 2026-10-30 — APPROVE (ronda 5: Enmienda 2026-10-30 de RF-B5, con RF-A4, RF-B11, RF-C2, tabla de API y Área 13)
**🤖 security-reviewer**

- Verifiqué:
  - El `git diff` de la spec contra RF-A4, RF-B5 y su enmienda del callback, RF-B11, RF-C2 y su enmienda del 2026-10-29, ADR-0008 (Enmienda 2026-10-28 y D2, D3, D4, D7 y D10), la desviación 16 de plan-b, `apps/server/src/auth/callback.rs` (el `Location` sale de `CallbackErrorCode::as_str()`) y las preguntas abiertas 1 y 2 de plan-d. También revisé la redacción de la delegación del owner del 2026-10-29: su texto cubre las enmiendas de la spec.
  - **La cuota no se debilita.** El bucket se consume y la respuesta sigue siendo un rechazo. Solo cambia la forma. El 303 lleva a `/login`, una ruta de `web` que no consume los buckets de auth, así que no hay bucle de reintentos. Un cliente con script recibe el mismo "no" sin `Retry-After`, y la rama de la cuota no toca la BD, así que inundar no cuesta más que antes. El tope de llaves (sin desalojo) no cambia.
  - **Sin reflexión.** El `Location` se arma en el único lugar y desde el enum cerrado, nunca usa `next` ni copia nada de la petición. No hay open redirect, ni inyección en el `Location`, ni un oráculo nuevo: 303 frente a 429 no le dice al atacante nada que no supiera.
  - **Cookies.**
    - `start` y `link` no emiten la cookie del flujo en las ramas de rechazo, así que no queda flujo a medias que fijar.
    - `callback` expira la cookie del flujo y no consume la fila. Sin la cookie, la fila no sirve (el `state` va atado a la cookie, D2) y vence por TTL. El `code` del proveedor no se canjea.
    - `logout` y `logout-all` no expiran la cookie de una sesión viva. Es lo correcto: expirarla sin revocar daría una falsa sensación de cierre, con la sesión todavía válida para quien tenga el token. Con la desviación 16, `/login` muestra el error y el enlace a la cuenta, así que el usuario no cree que cerró sesión.
  - **503 tras un commit** (timeout después de que la transacción confirmó). En `callback` y `link` puede quedar una sesión sin cookie o un vínculo hecho con el texto `auth.try_again`. Las dos cosas son inocuas: la sesión huérfana no tiene token en el navegador, y el reintento de `link` da `auth.provider_already_linked`. En `logout` puede quedar la sesión revocada con la cookie viva, que ya no autentica. Ninguna rama emite una cookie de sesión en el error, como pide RF-B5.
  - **`link` sin sesión → 303 a `/login` sin `error`.** Ni crea flujo ni vincula nada. Expirar la cookie de sesión vencida o revocada es correcto. El `Location` exacto y sin query evita un parámetro controlable.
  - **Lo que sigue en JSON.** El 404 (proveedor desconocido o apagado) y el 403 `auth.csrf_origin` solo los produce una petición armada a mano, y no revelan nada nuevo. Que el 403 de CSRF quede JSON no debilita D4.
  - **Alcance cerrado a cinco rutas.** `unlink`, `/api/me` y el upgrade de `/ws` no cambian. Los tests pedidos (IT por ruta en rojo primero, con reloj controlado y `PgProxy`, sin filas nuevas en `oauth_flows` ni en `sessions`, y la sesión válida tras un `logout` con la cuota agotada) verifican cada afirmación de la enmienda.
  - El frontmatter (`updated: 2026-10-30`). Sigo el precedente de las rondas 3 y 4: la firma de una enmienda queda en esta sección y `approvals` no cambia.
- Hallazgos de seguridad confirmados: ninguno.
- No bloquean (para plan-d y su PR):
  1. **`link` sin sesión, en la IT.** La enmienda dice "sin cookie del flujo" solo para la cuota y la BD. *Cambio:* la IT de "sin cookie y con una cookie revocada" también comprueba que no hay `Set-Cookie` del flujo ni fila nueva en `oauth_flows`. Así queda fijado que el chequeo de sesión corre antes de crear el flujo.
     - *Escenario:* si un refactor creara el flujo antes de mirar la sesión, un `link` sin sesión dejaría una cookie del flujo de `link` sin `user_id`, y el callback la trataría como un login.
  2. **El 429 del callback sale de la capa `AUTH_IP`, no del handler.** Esa capa ahora expira la cookie del flujo. *Cambio:* que use la misma función que el handler para el nombre de la cookie (`__Host-` o la política de loopback de RF-B9) y sus atributos, y una IT que compare el `Set-Cookie` de expiración de la capa con el del handler. Además, que la capa no registre la URI ni la query (ADR-0008 D3, ADR-0012 A). Hoy no lo hace ningún log, pero es código nuevo en el camino de `?code=&state=`.
     - *Escenario:* un nombre de cookie distinto deja viva la cookie del flujo; un log de la URI escribe `code` y `state` en el log.
  3. **El e2e que agota la cuota de `start` desde loopback** comparte el bucket `AUTH_IP` con el resto del e2e. No es de seguridad, pero un orden distinto de los specs puede dar falsos rojos. Que corra aislado (`test.describe.configure({ mode: 'serial' })` al final, o un server propio).
