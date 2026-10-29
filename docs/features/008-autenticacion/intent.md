---
id: "008"
title: Autenticación y sesiones
type: intent
status: approved
risk: high
author: product-owner
reviewers: [architect, security-reviewer]
approvals:
  - by: architect
    date: 2026-10-28
  - by: security-reviewer
    date: 2026-10-28
linear: TA-26
depends_on: ["001"]
created: 2026-10-28
updated: 2026-10-28
---

# Intent: Autenticación y sesiones

## Problema
La Fase 1 (MVP) de la visión necesita usuarios: sin cuenta no hay `Coins`, ni apuestas, ni leaderboard, ni motivo para volver mañana. Hoy el server no sabe quién es nadie. La base de datos solo tiene la migración baseline, el API es de solo lectura (una capa responde 405 a toda mutación hasta que llegue auth) y `/ws` acepta sockets anónimos con el chequeo de `Origin`, el tope global y los límites por socket, pero **sin límites por IP**. Un solo cliente puede ocupar todo `WS_MAX_CONNECTIONS` y dejar fuera al resto de los espectadores.

ADR-0008 (aprobado) ya decidió el diseño: D1 a D10. Lo que falta es construirlo, y que el primer usuario real entre con monedas que salgan del ledger, sin que nadie edite un saldo.

## Resultado propuesto
1. **Login con Google o Discord** (D1, D2) desde el celular, en en/es, sin contraseñas ni email. Al primer login se crea el usuario con un alias generado (nunca el nombre del proveedor), y un onboarding invita a cambiarlo con la validación de D1.
2. **El ledger nace aquí.** Esta feature crea el esquema base del ledger: doble entrada, la cuenta del sistema que emite las monedas, el tipo de asiento `grant` con su `grant_day` calculado en la BD y el índice único parcial por usuario y día de D8. La feature de apuestas lo **extiende** (stakes, pagos, rescate, reembolsos, rake) sin reescribirlo. No hay columna de saldo: el saldo siempre se deriva, y los asientos solo se insertan.
3. **Monedas iniciales por el ledger.** El primer login crea, en **una sola transacción**, el usuario, su cuenta OAuth, su cuenta en el ledger, el asiento inicial (ADR-0005) y su `auth_events`. Si algo falla, no queda nada. Vincular, desvincular y rotar la sesión nunca crean asientos. El asiento inicial es el **`grant` del día UTC** y reusa su índice único (decisión del owner, ver Decisiones). No hay asiento de bienvenida aparte. Un usuario nuevo nunca queda sin monedas ni con dos `grant` el mismo día. El monto `B` es configuración (p. ej. `DAILY_GRANT`, entero > 0, validado al arrancar; 100 propuesto por game-designer, lo aprueba el owner en la spec), nunca un número escrito en el código, y cada asiento guarda su propio monto. Reclamar el bono los días siguientes es de TA-27.
4. **Sesión** con cookie, expiración por inactividad y absoluta, rotación en login y vinculación, logout y "cerrar todas" (D3, D10). Borrar una sesión cierra sus sockets en el acto.
5. **Vincular y desvincular** un segundo proveedor desde una sesión iniciada, sin fusiones automáticas (D1).
6. **Defensas del API**: el middleware de `Origin` de D4 reemplaza la capa de solo lectura; SSR con secreto interno (D5); IP del cliente con `TRUSTED_PROXIES` y rate limiting por usuario e IP (D7) en las rutas de auth y de perfil.
7. **`/ws` con sesión y límites por IP** (D6, D7). Entran **ya**, porque protegen el WS que existe hoy: 50 sockets anónimos por `ip_key`, upgrade anónimo a 30/min con ráfaga 10, e IP real detrás de `TRUSTED_PROXIES`. Con sesión: autenticación por cookie en el upgrade, 5 sockets por usuario y 200 por IP, revalidación cada 5 min y canal privado `self` (un socket anónimo no puede suscribirse).
8. **Rol `admin` y ban**: `server admin grant|revoke` por CLI, sesiones de admin de 12 h y reautenticación a los 60 min (D9). Ban y desban con `server admin ban|unban` por CLI (decisión del owner): cada uno escribe su fila en `admin_audit` (solo de inserción, nace aquí), y el ban revoca todas las sesiones del usuario y cierra sus sockets. Un usuario baneado no inicia sesión (`auth.banned`, D10).
9. **Señales**: `auth_events` de login y vinculación con `ip_hash` (nunca la IP en claro) y su retención con el piso de 30 días de D8. El término `temporada + 7 días` llega con la feature de temporadas.
10. **Login en local y en el e2e sin red** con un proveedor de desarrollo (`dev`) que nunca existe en producción (ver Restricciones).

## Usuarios y sistemas afectados
- Usuarios: espectadores y apostadores (login, alias, sesión en el celular), el owner como admin, y los desarrolladores que necesitan iniciar sesión en local.
- Sistemas: `apps/server` (auth, sesiones, middleware, WS, CLI, migraciones), `apps/web` (pantallas de login, onboarding y cuenta, `handleFetch`, guard de `Origin`), `crates/protocol` (códigos de error y canales, regenerando `packages/protocol-ts`), `content/` (listas de palabras del alias y reservados).
- E2e y CI: hoy `just e2e` levanta solo `node build` de web, y el job `e2e` de CI corre sin Rust ni Postgres y solo cuando cambia web. Con login, el e2e necesita el server (con el proveedor `dev`) y Postgres (Docker en local, service container en CI), y los cambios en `apps/server` también disparan `e2e`. Toca `justfile`, `apps/web/playwright.config.ts`, `.github/workflows/ci.yml` y `CLAUDE.md`, además del `Dockerfile` del server (el chequeo de que la imagen de release no lleva el proveedor `dev`).
- No toca: `crates/engine`, `crates/sim`, `packages/renderer`.

## Restricciones
- **ADR-0008 manda.** El intent no redecide D1 a D10, y la spec los implementa con sus números fijados. Los tests mínimos del ADR que caen en este alcance bloquean el merge (ver Criterios).
- **Desviación de ADR-0008, aceptada por el owner (2026-10-28).** El ADR dice que sus 28 tests mínimos bloquean el merge de la feature de auth. Los que prueban código de otras features pasan a ellas, y cada uno entra como **criterio bloqueante en el issue de Linear de su feature dueña**:
  - Apuestas, **TA-27**: tests 7, 8, 23, 24 y 26, la reserva de capacidad de D6 (parte de 18) y guardar `bets.ranked_stake` al crear cada apuesta.
  - El test 25 y la parte de carencia de 27 bloquean a la feature que escriba `season_stats` primero (TA-27 o TA-28).
  - Peleas en vivo, **TA-13**: test 12.
  - Temporadas, **TA-28**: la parte de temporada de 27, el test 28 salvo el caso del `/64` y el término `temporada + 7 días` de la retención de `ip_hash`.
  - La nota de ADR-0008 sobre la feature dueña de cada test la escribe el architect.
- **Economía:** `B` se lee de configuración al arrancar (entero > 0; si falta o no es válido, el server no arranca), con valor 100 **propuesto por game-designer; lo aprueba el owner en la spec**. Con 100, `R = 50` es entero y `R ≤ B`, coincide con los ejemplos de D8 y da topes de 500 (`5 B`) y 100 en carencia (`1 B`). Como cada asiento guarda su monto, cuando TA-27 cambie `B` solo cambian los asientos nuevos.
- **ADR-0005:** nada de dinero real. Las monedas iniciales entran **solo** como asiento del ledger. No hay columna de saldo, y en el código no hay `UPDATE` ni `DELETE` sobre los asientos (lo hace cumplir un chequeo en CI o un trigger).
- **Proveedor `dev` (para local y e2e), imposible en producción:**
  - Se compila solo con una feature de cargo que el `Dockerfile` de release no activa, y un chequeo de CI lo comprueba sobre la imagen o el compose.
  - En runtime exige además que `PUBLIC_ORIGIN` sea `http` en loopback. Con el proveedor activado y otro origen, el server **no arranca**: no basta con apagarlo en silencio, como hace hoy `ENABLE_WS_ECHO`.
  - Usa su propio valor de `provider` (`dev`), nunca `google` ni `discord`, así que ni activado puede suplantar una identidad real.
  - Pasa por el mismo `start`, flujo, `state`, PKCE y `callback` que los proveedores reales, para que los tests de flujo y de logs ejerciten el camino de producción.
  - El botón no va en el bundle de producción de `web`: aparece solo si el server lo lista.
- **Cookies en loopback:** un nombre de cookie sin `__Host-`, si hace falta en `http://localhost`, solo existe detrás del mismo gate (feature de compilación más origen en loopback). El server lee **un solo** nombre por cookie, y en producción solo `__Host-session` y `__Host-oauth`. Un test con el binario de release lo comprueba.
- **Secretos:** `IP_HASH_KEY`, el secreto interno de D5 y los secretos OAuth son obligatorios fuera de loopback y tienen 32 bytes como mínimo (los OAuth, los que emita el proveedor). `server` y `web` no arrancan sin ellos o con el valor público del `.env.example`, y el `Debug` de la configuración los redacta, como ya hace con `DATABASE_URL`.
- **ADR-0012:** ningún log lleva lo que lista A4. Cada fuente nueva agrega su canario en el mismo PR (A8). Las violaciones de unicidad del alias no escriben el valor en los logs de Postgres (A9).
- **ADR-0007:** el server responde solo códigos (`auth.*`, `profile.*`, `ws.*`); todo texto visible vive en en/es. El alias es dato del usuario y no pasa por i18n.
- **Mobile-first:** el login y el onboarding se completan con el pulgar en 375 px (ver en Criterios cómo se verifica).
- **Pocos usuarios y CGNAT:** los límites por IP no pueden bloquear a un grupo legítimo detrás de la misma IP móvil (por eso hay un bucket por usuario y otro por IP más alto).
- **Funciona en local:** todo corre con `just dev` y el Postgres local. `just check`, `just test` y `just e2e` pasan sin red ni cuentas reales de Google o Discord.
- **CI sin costo:** el e2e con server y Postgres cabe en el runner gratuito de GitHub, con caché del build de Rust.
- **Repo público:** sin secretos ni datos personales en git; los `.env.example` solo llevan marcadores.
- Sin dependencias de pago ni servicios externos fuera de los dos proveedores OAuth.

## Criterios de éxito
- [ ] En `just dev`, un desarrollador inicia sesión con el proveedor `dev`, ve su alias generado, lo cambia, cierra sesión y vuelve a entrar con la misma cuenta, en los dos idiomas.
- [ ] **Móvil, en CI:** el e2e corre el flujo completo de login, onboarding y logout con el proveedor `dev` en Chromium y WebKit con emulación móvil (viewport de 375 px, táctil). **Móvil, manual:** antes de abrir al público (fuera de esta feature, con el deploy), el owner prueba el ida y vuelta con Google y Discord reales en Safari de iOS y Chrome de Android, y deja constancia en el issue de deploy.
- [ ] Un usuario nuevo tiene, justo después del primer login, un solo asiento `grant` del día UTC por el `B` configurado, y su saldo es la suma de sus asientos. Con `DAILY_GRANT` ausente, 0 o no entero, el server no arranca. 20 primeros logins concurrentes con la misma identidad crean un solo usuario y un solo asiento inicial. Un fallo a mitad del primer login (p. ej. al insertar la cuenta OAuth) no deja ni usuario ni asiento.
- [ ] No hay columna de saldo, y un chequeo en CI (o un trigger probado) impide `UPDATE` y `DELETE` sobre los asientos. Vincular, desvincular y rotar la sesión no crean asientos.
- [ ] Pasan los tests mínimos de ADR-0008 que caen en este alcance: 1–6, 9–11, 13–17, 19–22, de 18 los casos de mensaje, suscripción y sockets por IP, y de 28 "dos IPv6 del mismo `/64` producen el mismo `ip_hash`". Los movidos figuran como criterio bloqueante en TA-27, TA-13 y TA-28 según el reparto de Restricciones.
- [ ] **Ban:** tras `server admin ban <user_id>`, las sesiones del usuario dan 401, sus sockets se cierran y un login nuevo responde `auth.banned`. Tras `unban` puede volver a entrar. Cada comando escribe exactamente una fila en `admin_audit`, y ningún endpoint HTTP escribe `banned_at`.
- [ ] El socket anónimo número 51 desde la misma `ip_key` y el upgrade anónimo que pasa de 30/min (ráfaga 10) se rechazan; un `X-Forwarded-For` falsificado desde un peer fuera de `TRUSTED_PROXIES` no cambia la llave.
- [ ] La capa de solo lectura desaparece y ninguna mutación pasa sin el middleware de `Origin`.
- [ ] **Canarios (ADR-0012 A4, A4.13 y A8):** en una corrida de login completa, ningún log contiene la cookie de sesión, `__Host-oauth`, `code`, `state`, `code_verifier`, el access token del proveedor (el proveedor `dev` devuelve un canario), `token_hash`, `X-Internal-Secret`, `X-Client-Origin`, `X-Client-Fetch-Site`, la IP, `ip_key`, `ip_hash`, `user_id`, `provider_user_id` ni `display_name`.
- [ ] **A9:** un renombre que choca con el índice único no escribe el alias en el log de Postgres.
- [ ] **Proveedor `dev` y cookies:** la imagen de release no contiene el proveedor `dev` (chequeo de CI); un build con la feature activada y un `PUBLIC_ORIGIN` que no es `http` en loopback no arranca; el binario de release solo acepta `__Host-session` y `__Host-oauth`.
- [ ] **Secretos:** `server` y `web` no arrancan fuera de loopback si falta `IP_HASH_KEY` o el secreto interno, si mide menos de 32 bytes o si es el valor del `.env.example`; el `Debug` de la configuración no los muestra.
- [ ] `check-i18n` pasa: todo código nuevo tiene su texto en en y es.
- [ ] `just check` y `just e2e` (Chromium y WebKit) pasan con el login incluido, sin red. En CI, un cambio en `apps/server` dispara el job `e2e`.

## Fuera de alcance
- **Apuestas y economía de juego** (TA-27, que extiende el ledger de aquí): `POST /api/bets`, reclamar el bono de los días siguientes (`POST /api/bonus/claim`), rescate, valores definitivos de `B`, `R`, `m` y `r`, tope por pelea, carencia y sus efectos, y la reserva de capacidad de D6 (sin apuestas, toda cuenta sigue en carencia y la reserva solo bajaría el tope útil al 80 %). Sus tests van como criterio bloqueante en su issue.
- **Temporadas** (TA-28): `RankedProfit`, `season_stats`, leaderboard, `farm-report` y cierre de temporada. Criterio bloqueante en su issue: agregar a la retención de `ip_hash` el término `temporada + 7 días` **antes** de abrir la primera temporada.
- **Peleas en vivo y commit-reveal** (TA-13): scheduler, canales de pelea y el test de la semilla con sesión de admin (test 12).
- **Despliegue**: VPS, Coolify, dominio, `redirect_uri` de producción y logs de Traefik (TA-6, TA-8). Publicar el aviso de privacidad es requisito para abrir al público, no para mergear esta feature.
- Magic link, email, recuperación de cuenta, perfil público, avatar y personalización (Fase 3).
- Endpoints HTTP de admin y panel de moderación.

## Decisiones
- **Monedas iniciales (owner, 2026-10-28).** El asiento inicial es el `grant` del día UTC y reusa su índice único. No hay asiento de bienvenida, y ADR-0005 no cambia. `B` va por configuración validada al arrancar. Su valor (100, propuesto por game-designer) no está decidido: lo aprueba el owner en la spec. Cada asiento guarda su monto. El bono de los días siguientes es de TA-27.
- **Reparto de tests y reserva de D6 (owner, 2026-10-28).** Se acepta la desviación de ADR-0008 con el reparto de Restricciones (TA-27, TA-13, TA-28). La reserva de D6 se aplaza a TA-27. El architect escribe la nota en ADR-0008.
- **Ban (owner, 2026-10-28).** Entra en esta feature: `server admin ban|unban` por CLI, con su fila en `admin_audit` y la revocación de sesiones de D10.

## Preguntas abiertas
Todas se resuelven **antes de aprobar la spec**.

- **Login en local. Decide: architect y security-reviewer, en la spec.** El proveedor `dev` es obligatorio (lo exige el e2e sin red y queda acotado en Restricciones). Falta decidir si además se documentan apps OAuth de desarrollo con `redirect_uri` a `localhost` para probar Google y Discord reales en local. El architect recomienda las dos.
- **Cookies `__Host-` en `http://localhost`. Decide: architect y security-reviewer, en la spec.** Exigen `Secure`, y WebKit, que corre en el e2e, puede no aceptarlas en `http://localhost`. Salidas: https local, o un nombre alternativo solo detrás del gate de Restricciones. El architect recomienda un solo punto de config que derive los nombres, con `__Host-` fijo fuera de loopback y un test que lo afirme.
- **`locale` y `theme` en `users`. Decide: architect, en la spec.** ¿Esta feature sincroniza las preferencias de la UI con la cuenta, o solo crea las columnas? El architect recomienda solo las columnas.

## Revisión
### architect — 2026-10-28 — REQUEST_CHANGES
- Verifiqué: frontmatter válido; el intent no redecide D1–D10 de ADR-0008 y respeta ADR-0005 (monedas solo por asiento), ADR-0007 (solo códigos) y ADR-0012 (canarios, A8, A9). Contra el código: `http/read_only.rs` es la capa de 405 que se quita; `ws/` tiene `Origin`, tope global y límites por socket pero ningún límite por IP; `config.rs` ya tiene el patrón `decide_echo` (solo `http` en loopback), reutilizable para un proveedor falso y para el nombre de cookie en local; solo existe la migración baseline, así que no hay ledger. Viable en el VPS de 1 vCPU y sin costo: el limiter y el registro de sockets en memoria son los que asume ADR-0008. Google y Discord aceptan `redirect_uri` en `http://localhost`. El aplazamiento de la reserva de D6 es coherente: sin apuestas, toda cuenta sigue en carencia.
- Cambios pedidos:
  1. **Topología del e2e y de CI.** Hoy `just e2e` levanta solo `node build` de web, sin server ni Postgres, y el job `e2e` de `ci.yml` corre en la imagen de Playwright, sin Rust ni Postgres, y solo cuando cambia web. Un login en el e2e necesita el server, Postgres (Docker en local, un service container en CI) y que cambios en `apps/server` también disparen `e2e`. Agrégalo a "Sistemas" (`justfile`, `apps/web/playwright.config.ts`, `.github/workflows/ci.yml`, `CLAUDE.md`) y a "Restricciones" (sin servicios de pago; el job tiene que caber en el runner gratuito con caché del build de Rust).
  2. **El ledger nace aquí.** Di de forma explícita que esta feature crea el esquema base del ledger (`ledger_entries` de doble entrada, la cuenta del sistema que emite, el tipo `grant` con el índice único parcial `(user_id, grant_day)` de D8), y que la feature de apuestas lo extiende sin reescribirlo. Sin esto, la spec de apuestas puede rehacerlo.
  3. **Test 28, último punto** ("dos IPv6 del mismo `/64` dan el mismo `ip_hash`") entra en este alcance: aquí ya se escribe `ip_hash`. Muévelo a la lista de tests que bloquean.
  4. **Reparto de tests frente a ADR-0008.** El ADR dice que los tests mínimos "bloquean el merge de la feature de auth", y el intent saca 7, 8, 12, 23–28 y la reserva de 18. Es razonable, pero contradice el texto literal del ADR: tiene que quedar como desviación explícita con OK del owner (y una nota en ADR-0008 sobre qué feature es dueña de cada test, que puedo escribir yo y no aprobar). Lo mismo para la reserva de D6: aplazarla o implementarla ya con todas las cuentas en carencia, lo que en la práctica baja el tope útil al 80 %.
  5. **El criterio móvil se tiene que poder verificar.** "El flujo OAuth sobrevive al ida y vuelta en Safari y Chrome móviles" no tiene criterio: en CI solo hay `Desktop Safari`. Indica cómo se comprueba (por ejemplo, WebKit con emulación móvil y el proveedor falso en el e2e, más una prueba manual con las apps reales antes de abrir al público).
  6. **Preguntas abiertas: cada una con quién decide y para cuándo** (antes de aprobar la spec). Recomendación del architect, sin decidir por el autor: login local (c), los dos, con el proveedor falso detrás de la misma regla que `decide_echo` y fuera de la imagen de release si se puede; un solo punto de config deriva los nombres de cookie, con `__Host-` fijo fuera de loopback y un test que lo afirme; ban con `server admin ban|unban` por CLI en esta feature (si no, el criterio de D10 solo se prueba con SQL); `locale`/`theme` solo como columnas; retención con el piso de 30 días y un job diario.
- Solo el owner decide: (a) si las monedas iniciales son el `grant` del día o un asiento de bienvenida aparte. Un asiento aparte es una fuente de monedas que ADR-0005 no lista (bono, rescate, ganancias) y obliga a enmendarlo. (b) El `B` provisional, con game-designer. (c) Aceptar el reparto de tests y la opción para la reserva de D6 (punto 4).

### security-reviewer — 2026-10-28 — REQUEST_CHANGES
**🤖 security-reviewer**

- Verifiqué: el intent contra ADR-0008 (D1 a D10, números fijados y los 28 tests mínimos), ADR-0005 (monedas solo por el ledger), ADR-0012 (A4.13, A8, A9), ADR-0002 y el código actual de `apps/server` (`config.rs::decide_echo`, `http/read_only.rs`, `ws/`, `telemetry.rs`, la migración baseline y el `PUBLIC_ORIGIN: https://${PUBLIC_HOST}` de `infra/docker-compose.yml`). La dirección es correcta: no redecide D1 a D10, las monedas iniciales entran como asiento, los límites anónimos por `ip_key` entran ya y la restricción de logs sigue a ADR-0012. Todos los hallazgos de abajo son **Important** y bloquean.

- Cambios pedidos:

  1. **Proveedor falso de desarrollo: el intent debe fijar que no se puede encender en producción, no dejarlo abierto como opción.** *Escenario:* el proveedor falso deja elegir el `provider_user_id`. Si queda encendido en producción por un error de variables en Coolify, cualquiera entra como cualquier cuenta, incluida la del admin. El gate actual de `ENABLE_WS_ECHO` falla cerrado y en silencio (`Refused`), y para el echo eso basta. Para auth no basta. *Cambio:* agregar a Restricciones que, si existe un proveedor falso, (a) se compila solo con una feature de cargo que el `Dockerfile` de release no activa, y un chequeo de CI (`check-compose` o un test de la imagen) lo comprueba; (b) en runtime exige además `PUBLIC_ORIGIN` `http` en loopback, y con el flag puesto y otro origen el server **no arranca**; (c) usa su propio valor de `provider` (p. ej. `dev`), nunca `google` ni `discord`, así que ni encendido puede suplantar una identidad real; (d) pasa por el mismo `start`, `oauth_flows`, `state`, PKCE y `callback` que los proveedores reales, para que los tests 4, 13 y 19 ejerciten el camino de producción; (e) `web` no lleva su botón en el bundle: lo muestra solo si el server lo lista.
  2. **La cookie de loopback, con el mismo gate.** *Escenario:* si el server aceptara en producción una cookie sin prefijo (p. ej. `session`) como alternativa a `__Host-session`, un subdominio hermano podría plantarla con `Domain=` del dominio padre y fijar la sesión (cookie tossing). Ese es el ataque que D3 cierra. *Cambio:* cualquier nombre o flag alternativo lo decide el mismo gate de 1(a)+(b) al arrancar. El server lee **un solo** nombre de cookie, y en producción solo `__Host-session` y `__Host-oauth`. Un test lo comprueba con el binario de release.
  3. **El reparto de tests deja fuera uno que es de este alcance y no queda atado a nadie.** (a) El test 28 trae el caso "dos IPv6 del mismo `/64` producen el mismo `ip_hash`", y `ip_hash` nace en esta feature: va en Criterios. (b) ADR-0008 dice que los 28 tests "bloquean el merge de la feature de auth". Moverlos a otras features es una enmienda del ADR y la aprueba el owner (ver abajo). Además, cada test movido (7, 8, 12, 23 a 27, lo demás de 28 y la reserva de 18) debe quedar como criterio bloqueante en el issue de Linear de su feature dueña, con el ID escrito aquí. *Escenario:* la feature de apuestas sale sin la reserva de D6 ni el test 7, porque "estaba anotado" en un intent ya cerrado. La reserva diferida no abre un hueco hoy, porque todavía no hay cuentas fuera de carencia que proteger. El hueco se abre si apuestas sale sin ella.
  4. **Monedas iniciales: atomicidad e idempotencia.** *Escenario:* el primer login inserta `users` y el asiento, la inserción de `oauth_accounts` choca con otro login concurrente, y si la transacción no se revierte queda un segundo usuario con monedas. *Cambio:* fijar que `users`, `oauth_accounts`, la apertura de la cuenta en el ledger, el asiento inicial y su `auth_events` van en **una** transacción. El índice `UNIQUE (user_id, grant_day) WHERE kind = 'grant'` y `grant_day` calculado en la BD se crean en esta feature. Vincular, desvincular y la rotación de sesión nunca crean asientos. Si el owner elige un asiento de bienvenida aparte, ese asiento lleva su propio `UNIQUE (user_id) WHERE kind = '…'`. El criterio "ningún código escribe un saldo" se vuelve verificable así: no hay columna de saldo, y `ledger_entries` es solo de inserción (sin `UPDATE`/`DELETE` en el código, con un chequeo en CI o un trigger).
  5. **Los canarios del criterio no cubren la lista de la restricción ni ADR-0012 A4.13.** *Escenario:* un `tracing::info!(user_id = …)` en el callback o un `?err` con el `provider_user_id` pasa el criterio tal como está escrito. *Cambio:* agregar a los canarios de la corrida de login `X-Internal-Secret`, `code_verifier`, el access token del proveedor (el proveedor falso devuelve un canario), `token_hash`, `ip_key`, `ip_hash`, `user_id`, `provider_user_id` y `display_name`. Sumar un criterio de A9: un renombre que choca con el `UNIQUE` no escribe el alias en el log de Postgres.
  6. **Secretos nuevos.** `IP_HASH_KEY` y `X-Internal-Secret` (y los secretos OAuth): fijar que son obligatorios fuera de loopback, de 32 bytes como mínimo, que el server y `web` no arrancan sin ellos o con el valor del `.env.example`, y que el `Debug` de `Config` los redacta (como ya hace con `DATABASE_URL`). *Escenario:* producción arranca con el `IP_HASH_KEY` de ejemplo, que es público, y cualquiera revierte `ip_hash` probando todo IPv4.
  7. **Retención de `ip_hash`.** Aplicar hoy solo el piso de 30 días es correcto según `max(30, temporada + 7)`. Pero la feature de temporadas debe agregar el término **antes** de abrir la primera temporada; si no, el job borra los datos de los primeros días antes de `farm-report`. Va como criterio bloqueante en su issue (igual que en el punto 3).

- Solo el owner decide:
  - La enmienda de ADR-0008 que reasigna los tests mínimos a sus features dueñas (punto 3b).
  - El asiento inicial: `grant` del día o uno de bienvenida aparte. Un asiento aparte es una fuente nueva de monedas que enmienda ADR-0005, y le da más saldo a cada alt de una granja. Mi recomendación de seguridad es el `grant` del día. El valor provisional de `B` lo decide con game-designer.
  - El ban por CLI. Recomiendo que entre en esta feature (`server admin ban|unban`, con `admin_audit` y la revocación de D10). Sin una forma de aplicarlo, "un usuario baneado no inicia sesión" no se puede verificar, y no hay respuesta posible ante un abuso en cuanto haya login público.

### architect — 2026-10-28 — APPROVE condicionado a las respuestas del owner a (a), (b) y (c) (ronda 2)
- Verifiqué mis seis puntos de la ronda 1. (1) La topología del e2e y de CI está en Sistemas, Restricciones y Criterios, y un cambio en `apps/server` dispara `e2e`. (2) El ledger nace aquí: doble entrada, cuenta emisora, `grant` con su índice, sin columna de saldo y sin `UPDATE`/`DELETE` sobre los asientos. (3) El caso `/64` del test 28 está en el alcance. (4) El reparto de tests figura como desviación de ADR-0008, pendiente del owner, con la nota a cargo del architect. (5) El criterio móvil es verificable: WebKit y Chromium con emulación en CI, más la prueba manual con el deploy. (6) Cada pregunta abierta tiene quién decide, todas antes de aprobar la spec. El proveedor `dev` (feature de cargo más gate de loopback que impide arrancar con otro origen, `provider` propio, mismo flujo) es viable y no debilita producción. El primer login en una sola transacción encaja con D1 y ADR-0005.
- Sigue abierto, sin bloquear el intent: los IDs de Linear de apuestas y temporadas (`TA-<pendiente>`) se escriben antes de aprobar la spec, como dice el criterio. La nota en ADR-0008 la escribo yo cuando el owner acepte (c). La spec elige entre chequeo de CI y trigger para los asientos, y decide las cookies en loopback, las apps OAuth de desarrollo y `locale`/`theme`.
- Queda en espera del owner: (a) el tipo del asiento inicial, (b) el `B` provisional y (c) el reparto de tests con la reserva de D6, además del ban por CLI. Hasta que responda, no agrego mi aprobación a `approvals` y el `status` sigue en `changes-requested`.

### security-reviewer — 2026-10-28 (segunda pasada) — APPROVE condicionado a las respuestas del owner a (a), (b) y (c)
**🤖 security-reviewer**

- Verifiqué la versión revisada contra mis 7 puntos:
  1. Proveedor `dev`: queda resuelto en Restricciones y Criterios con feature de cargo fuera del `Dockerfile` de release y chequeo de CI, server que no arranca fuera de loopback `http`, `provider = dev`, mismo flujo `start`/`state`/PKCE/`callback` y botón listado por el server.
  2. Cookies: un solo nombre por cookie, y el binario de release acepta solo `__Host-`.
  3. El caso `/64` → `ip_hash` del test 28 está en Criterios, y los tests movidos quedan como criterio bloqueante con su ID de Linear, escrito antes de aprobar la spec.
  4. El primer login es una sola transacción, con un test de fallo a mitad, el ledger es solo de inserción y no hay columna de saldo.
  5. Los canarios cubren A4 y A4.13, más el criterio de A9.
  6. Los secretos son obligatorios, de 32 bytes como mínimo, no aceptan el valor del `.env.example` y se redactan en `Debug`.
  7. La retención con el término de temporadas antes de la primera temporada va como criterio bloqueante.
- No queda ningún hallazgo de seguridad abierto fuera de las tres preguntas del owner: (a) el asiento inicial y el `B` provisional; (b) el reparto de tests, con la reserva de D6, como desviación de ADR-0008; (c) el ban por CLI. Mi aprobación queda en suspenso hasta que el owner las responda. Si en (a) elige un asiento de bienvenida, o en (c) pospone el ban, vuelvo a revisar el intent.
- `approvals` y `status` no cambian hasta entonces.

### architect — 2026-10-28 — APPROVE (ronda 3)
- Verifiqué que la sección "Decisiones" recoge las tres respuestas del owner. (a) El asiento inicial es el `grant` del día, sin asiento de bienvenida, así que ADR-0005 no cambia, y `B` va por configuración validada, con 100 propuesto y la aprobación del owner pendiente en la spec. (b) Están el reparto de tests y la reserva de D6 aplazada a TA-27. (c) Está el ban por CLI, con su criterio verificable. Las preguntas que quedan son técnicas y las decide la spec.
- Coherencia con la "Nota (2026-10-28): reparto de tests por feature" de ADR-0008: Restricciones y Criterios asignan los mismos tests a TA-26, TA-27, TA-13 y TA-28, incluidos los casos parciales de 18, 27 y 28 y el término de retención `temporada + 7 días`.

### security-reviewer — 2026-10-28 (tercera pasada) — APPROVE
**🤖 security-reviewer**

- Verifiqué la sección Decisiones y su reflejo en el cuerpo:
  - (a) El asiento inicial es el `grant` del día y reusa su índice único. No hay fuente nueva de monedas, ADR-0005 no cambia, `B` va por configuración validada al arrancar y cada asiento guarda su monto.
  - (b) El reparto de tests a TA-27, TA-13 y TA-28 coincide con la nota de ADR-0008, y el caso `/64` → `ip_hash` se queda en 008.
  - (c) `server admin ban|unban` tiene criterio verificable: 401, cierre de sockets, `auth.banned`, una fila en `admin_audit` y ningún endpoint escribe `banned_at`.
- Lo que la feature 008 necesita en materia de seguridad se queda en 008. Las preguntas abiertas que siguen (apps OAuth de desarrollo, nombre de la cookie en loopback, `locale`/`theme`) se deciden en la spec, dentro de las Restricciones ya fijadas.
- Nota: la aceptación del owner llegó transmitida por el coordinador. Igual que en la nota de ADR-0008, conviene que el owner la confirme en este documento.
