---
id: "0012"
title: Contrato de logs y cabeceras de seguridad (CSP y HSTS)
type: adr
status: approved
risk: high
author: architect
reviewers: [security-reviewer, devops-engineer, owner]
approvals: [{by: security-reviewer, date: 2026-10-26}, {by: devops-engineer, date: 2026-10-26}, {by: owner, date: 2026-10-26}]
supersedes: null
linear: TA-7
depends_on: ["0002", "0003", "0006", "0007", "0008", "0009"]
created: 2026-10-26
updated: 2026-10-30
---

# ADR-0012: Contrato de logs y cabeceras de seguridad (CSP y HSTS)

## Contexto
- La feature 001 fijó en su spec (`docs/features/001-fundaciones/spec.md`: RF-A10, RF-A19, RF-A23, RF-B12, RF-B15, RF-C4, "Diseño → Logs y plazo" y "Áreas de preocupación" 3, 4 y 13) dos decisiones que obligan a todas las features que vienen: qué entra en un log y qué CSP y cabeceras sirve la app. Una spec de feature no es el lugar para un contrato transversal, así que este ADR lo eleva. La spec de 001 sigue describiendo cómo se implementó y este ADR fija lo que no se puede romper.
- El aviso de privacidad (`docs/legal/aviso-de-privacidad.md`, fila "Registros técnicos del servidor", sección 8 y "Pendiente" 8) promete logs **sin IP, cookies, cabeceras, navegador ni identificadores de usuario**, conservados **menos de 3 semanas**. Esa promesa se apoya en código que ya existe y en la configuración del compose.
- ADR-0002 (la semilla no aparece en logs antes de `Settled`), ADR-0008 D3 (redactar headers y quitar el query de los callbacks), D5 (headers internos) y D7/D8 (`ip_key`, `ip_hash`), y ADR-0006 D1 (`json-file` de 3 × 10 MB) y D11 (Traefik sin access logs) ya ponen piezas del contrato. El sistema real es más estricto que ADR-0008 D3: no registra la URI ni los parámetros de la ruta.
- Las features 002 (renderer con PixiJS) y 003 (motor con `engine-wasm` para verificar en el navegador) van a necesitar ampliar la CSP. La spec de 001 exige que este ADR esté aprobado antes de que 002 la toque.
- Restricciones: VPS de 1 vCPU, presupuesto cero, ningún servicio externo sin OK del owner (ADR-0006) y ningún CDN (ADR-0009). El motor y el protocolo no llevan texto (ADR-0007), y los logs tampoco: llevan códigos.
- **Alcance de la parte A: los logs de `web`, `server` y Postgres** (el recurso standalone de Coolify en producción, y los servicios `postgres` de `infra/compose.local.yml` e `infra/compose.dev.yml`). Postgres no usa nuestros formateadores, así que su parte del contrato se cumple por configuración (A9). Traefik queda cubierto por ADR-0006 D11 (sin access logs). Ningún otro contenedor del proyecto escribe logs.

### Línea base (lo que ya existe)
| Pieza | Archivo |
|---|---|
| Formateador JSON del server, allowlist `ALLOWED_FIELDS`, crates externos reducidos a `ts`/`level`/`target`/`event:"external"` y topados en `warn` (`CAPPED_TARGETS`), panic hook sin mensaje, valores escritos con `serde_json` | `apps/server/src/telemetry.rs` |
| Una línea `http.request` por petición: `method` (conjunto cerrado), `route` (`MatchedPath` o `unmatched`), `status`, `latency_ms`, `request_id` (UUID v4) | `apps/server/src/http/request_log.rs` |
| Headers sensibles (`Cookie`, `Authorization`, `X-Internal-Secret`, `X-Client-Ip`, `X-Forwarded-For`, `Set-Cookie`), `nosniff` y HSTS en toda respuesta | `apps/server/src/http/mod.rs` |
| Errores de BD como código (`error_kind`), nunca su `Display` | `apps/server/src/db.rs` |
| Único escritor de logs de `web`, allowlist, `error_message` y `stack` saneados | `apps/web/src/lib/server/log.ts` |
| Saneador por lista negra, `MAX_INPUT` de 8 KB, patrones lineales (anti-ReDoS) | `apps/web/src/lib/server/sanitize.ts` |
| Solo se registra `kind: 'unknown'`; `error_id` nuevo por error registrado | `apps/web/src/lib/server/hooks/handle-error.ts` |
| `unhandledRejection` y `uncaughtException` (este último sale con 1) | `apps/web/src/lib/server/hooks/process-handlers.ts` |
| `nosniff`, `Referrer-Policy` y HSTS en `handle` | `apps/web/src/lib/server/hooks/security-headers.ts` |
| CSP de Kit en `mode: 'auto'` | `apps/web/vite.config.ts` |
| `json-file` 3 × 10 MB, `RUST_LOG: info`, `accesslogs=false` en los cuatro routers, middleware `torneo-web-headers` | `infra/docker-compose.yml`, verificado por `scripts/check-compose.mjs` |
| Postgres con los parámetros de log por defecto (hueco que cierra A9); `command` con el tuning de ADR-0006 D4 solo en `compose.local.yml` | `infra/compose.local.yml`, `infra/compose.dev.yml`, `POSTGRES_COMMAND` en `scripts/check-compose.mjs` |
| Sin `trustedOrigins`, `checkOrigin` ni `paths.origin`; ninguna ruta prerenderizada | `scripts/check-kit-config.mjs`, `scripts/check-prerender.mjs` |
| `no-console` fuera de `log.ts` (ESLint); `print_stdout` y `print_stderr` denegados (clippy) | `apps/web/eslint.config.js`, `Cargo.toml` |
| Canarios | `apps/server/tests/logs.rs`, `apps/web/e2e/logs.spec.ts`, `apps/web/e2e/support/logs.ts`, `apps/web/e2e/csp.spec.ts` |

## Decisión

### Parte A: logs

#### A1. Formato
- Un objeto JSON por línea, escrito con un serializador JSON (`serde_json` en el server, `JSON.stringify` en `web`), nunca concatenando texto. Ningún valor puede partir la línea ni inyectar campos.
- `ts`: RFC 3339 en UTC con sufijo `Z` y fracción de segundo (3 a 9 dígitos).
- `level`: `error`, `warn`, `info`, `debug` o `trace`, **en minúsculas** en las dos apps. Hoy el server escribe `INFO`; se corrige (Consecuencias, trabajo 3).
- `event`: código estable que cumple `^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$` (p. ej. `http.request`, `db.unavailable`, `unhandled_error`). Para eventos nuevos se prefiere `<área>.<qué>`. Renombrar un evento rompe el contrato y se trata como agregar uno.
- En el server, `target` solo aparece en las líneas de crates externos.

#### A2. Niveles
| Nivel | Uso | Ejemplos actuales |
|---|---|---|
| `error` | Falla propia que necesita atención: el proceso no puede seguir o una petición falló por nuestro código | `panic`, `db.migrate_failed`, `config.invalid`, `cli.invalid`, `server.bind_failed`, `unhandled_error`, `unhandled_rejection`, `uncaught_exception` |
| `warn` | Degradación que se recupera sola; techo de los crates externos | `db.unavailable`, `server.shutdown_timeout`, cualquier línea `external` |
| `info` | Ciclo de vida y, en el server, una línea por petición HTTP | `server.listening`, `server.stopping`, `http.request`, `web_started` |
| `debug` | Diagnóstico en desarrollo | `ws.closed` |
| `trace` | Solo en desarrollo local | — |

- **Producción corre con `RUST_LOG=info` exacto** (lo exige `check-compose.mjs`). `debug` y `trace` no se activan en producción. Para diagnosticar se reproduce en local o en `prod-local`.
- `web` solo tiene `info`, `warn` y `error` (`LogLevel` en `log.ts`).
- **Sin salidas de diagnóstico de Node en producción.** `web` no define `NODE_DEBUG` (p. ej. `NODE_DEBUG=http` escribe headers, cookie incluida, a stderr sin pasar por `log.ts`) ni `DEBUG`, y `NODE_OPTIONS` no lleva `--inspect*`, `--trace-*`, `--report-*` ni otros flags de diagnóstico. Hoy lo hacen cumplir la allowlist de variables por servicio y el `NODE_OPTIONS` exacto de `check-compose.mjs`. **No se agregan variables de entorno desde la UI de Coolify** fuera de las que el compose interpola: el entorno de producción es el del compose, revisado en un PR.
- La allowlist se aplica igual en todos los niveles. `debug` no es una vía para registrar lo que `info` no puede.
- `/api/health` se registra en `info` como cualquier petición (RF-B16). El plazo de A7 depende de eso.

#### A3. Allowlist cerrada (control primario) y saneador (defensa en profundidad)
- Cada app tiene **una lista cerrada de campos**. Un campo que no está en la lista no se escribe, aunque el código lo pase. Hoy:
  - **server** (`ALLOWED_FIELDS`): `event`, `method`, `route`, `status`, `latency_ms`, `request_id`, `var` (nombre de una variable de entorno), `port`, `error_kind`, `close_code`, `file` y `line` (ubicación de un pánico). Más `ts` y `level`, que pone el formateador. `message` nunca está en la lista.
  - **web** (`ALLOWED_FIELDS` de `log.ts`): `event`, `error_id`, `route_id` (`event.route.id`, la plantilla), `status`, `error_name`, `error_code` (A5; reemplaza a `error_message`) y `stack`. Más `ts` y `level`.
- **Los crates externos del server** solo escriben `ts`, `level`, `target` y `"event":"external"`, y nunca bajan de `warn` aunque `RUST_LOG` los nombre (`CAPPED_TARGETS`, combinado con `and` para que una directiva más específica no gane). Agregar un crate ruidoso a `CAPPED_TARGETS` no requiere este ADR.
- **Las bibliotecas de `web`** no escriben a la consola. Hay tres excepciones conocidas, todas de texto fijo: las dos de Kit y adapter-node (RF-A19), que vigila la lista blanca del e2e (`apps/web/e2e/support/logs.ts`), y los avisos de `process.emitWarning` de Node (p. ej. `DeprecationWarning`), que salen a stderr con el texto que fija Node o la biblioteca que los emite. Un aviso de `emitWarning` cuyo texto lleve datos de la petición es un defecto de esa biblioteca y se trata como fuga. Cualquier otra salida es un defecto.
- **Solo hay un escritor por app**: `log.ts` en `web` (ESLint `no-console`) y `tracing` con el formateador de `telemetry.rs` en el server (clippy `print_stdout`/`print_stderr`).
- **Spans y capas del server** (Consecuencias, trabajo 12):
  - **Los campos de span nunca se escriben.** El formateador escribe solo los campos del evento; los de sus spans (de `#[instrument]`, `info_span!` o `Span::record`) no salen en ninguna línea.
  - **`telemetry::layer` es la única capa del subscriber que produce salida.** Las capas sin salida (el filtro de `RUST_LOG` y `CAPPED_TARGETS`) se permiten. Agregar otra capa con salida (un segundo `fmt::layer()`, OpenTelemetry, `console-subscriber`/tokio-console, `tracing-appender`, `tracing-journald` o similares) requiere un ADR que reemplace este, porque esa capa vería los campos de span y los textos de los crates externos.
  - **`#[tracing::instrument]` siempre con `skip_all`.** Si un span necesita un campo, se declara con `fields(...)` y solo con valores que cumplen A3 y A4, aunque hoy no se escriban. Así, un cambio futuro de capa no expone el `Debug` de un argumento (p. ej. un `Fight` con su semilla).
- **El saneador** de `web` (`sanitize.ts`: IPs, query y fragmento de URLs, credenciales en URLs y el valor que sigue a un header sensible, con `MAX_INPUT` y patrones lineales) se aplica a todo campo que pueda llevar texto de un error (hoy, `stack`). Es defensa en profundidad: la garantía la dan la allowlist y A5, no el saneador. TA-11 cerró los huecos que salieron de la revisión de #2 (IPv6 tras una clave con forma hex, valores en la línea siguiente, en un objeto o en un par `{ name, value }`), sumó `forwarded`, `x-real-ip` y `x-client-*` a `HEADER_NAMES` y extendió los límites de `no-restricted-imports` a los `import()` dinámicos (`torneo/no-restricted-dynamic-imports`).
- **Tipos por campo** (Consecuencias, trabajos 1, 4 y 5): los campos de cadena (`event`, `method`, `route`, `error_kind`, `error_code`, `var`, `file`) solo aceptan cadenas; los numéricos (`status`, `latency_ms`, `port`, `close_code`, `line`) solo números enteros. En el server, solo `request_id` acepta un valor con `Display`, y ningún campo acepta `Debug`. Así, `error_kind = ?err` deja de escribir el `Debug` de un error.
- En el server, el visitor implementa de forma explícita **todos** los métodos de `tracing::field::Visit`. Los que no están en la regla anterior (`record_bool`, `record_f64`, `record_i128`, `record_u128`, `record_bytes`, `record_error`, y `record_debug` en cualquier campo que no sea `request_id`) **descartan el campo**. Ningún método cae en la implementación por defecto, que reenvía a `record_debug` y escribiría el `Debug` (o el `Display` de un error) en un campo permitido.

#### A4. Lo que nunca se registra
En ningún nivel, en ninguna app (Postgres incluido, A9) y en ningún campo, ni siquiera saneado:
1. La IP del cliente en cualquier forma: `getClientAddress()`, `X-Forwarded-For`, `X-Real-Ip`, `Forwarded`, `X-Client-Ip`, la dirección del peer, `ip_key` ni `ip_hash` (ADR-0008 D7 y D8).
2. Cookies y tokens de sesión (`__Host-session`, `__Host-oauth`), su hash (`token_hash`) ni ningún id de sesión.
3. `Authorization`, `X-Internal-Secret`, cualquier `X-Client-*` (`X-Client-Ip`, `X-Client-Origin`, `X-Client-Fetch-Site`; ADR-0008 D5) y, en general, ningún header de la petición ni de la respuesta (`Origin`, `Referer` y `Host` incluidos).
4. El user agent ni su familia (`user_agent_family`).
5. La URL, la URI, el query string ni el fragmento. Las rutas se registran como plantilla (`MatchedPath`, `event.route.id`), así que ni los parámetros de la ruta (p. ej. un `fight_id`) llegan al log.
6. Cuerpos de petición y de respuesta, y los mensajes de WebSocket.
7. El `code` y el `state` de OAuth, el `code_verifier`, y los access tokens, refresh tokens e `id_token` de los proveedores (ADR-0008 D1 y D2).
8. **Semillas, en ningún momento, tampoco después de `Settled`.** Antes de `Settled` lo prohíbe ADR-0002. Después, la semilla ya es pública por el API, así que registrarla no aporta nada. Además, quien escribe una línea no siempre sabe con certeza si la pelea ya se liquidó, y una regla sin excepciones no deja lugar a ese error.
9. El contenido de un `FightLog` o de sus eventos, en ningún momento (cubre los eventos futuros de ADR-0002 punto 3). Una pelea se identifica en los logs, si algún día hace falta, por un campo agregado según A6.
10. `DATABASE_URL`, `IP_HASH_KEY`, los secretos de OAuth y el **valor** de cualquier variable de entorno. Solo se registra el **nombre** (`var`).
11. El mensaje de un pánico (el hook registra solo `event`, `file` y `line`) y el `Display`/`Debug` de cualquier error (A5).
12. El texto libre de crates y bibliotecas externos (A3).
13. **Valores de persona, sesión o dispositivo.** Ningún campo lleva un valor que **es o deriva de** una clave o un atributo de una fila de persona, sesión o dispositivo: alias (`display_name`), email (no existe; ADR-0008 D1), `user_id`, `bet_id`, ids de asientos del ledger, ids de OAuth (`provider_user_id`), y cualquier id, hash, token o alias con el mismo efecto (también un hash o un truncado de uno de ellos). El criterio mira **el valor del campo**, no lo que se pueda inferir cruzando la línea con otros datos. Esto rige mientras un ADR no lo cambie (A6).
   - **Riesgo residual aceptado: correlación por tiempo.** `ts`, `method`, `route` y `status` (p. ej. un `POST /api/bets` con 201) cruzados con columnas `created_at` de la BD pueden apuntar a una fila y, con ella, a una persona. Se acepta: quien puede hacer ese cruce ya tiene la BD, que contiene esos datos de forma directa, y los logs no salen del VPS (A7). No se recorta `ts` a milisegundos, porque una coincidencia en el mismo milisegundo sigue siendo casi única y el recorte no cambia el riesgo.

#### A5. Errores
Decisión sobre la sugerencia de security-reviewer: **se adopta y se endurece**. Un error se registra por su código y nunca por su mensaje.
- **web**:
  - **Error propio con código**: una instancia de `CodedError` (clase nueva en `apps/web/src/lib/server/errors.ts`) cuyo `code` pertenece a la lista cerrada `SERVER_ERROR_CODES` del mismo archivo (tipo `ServerErrorCode`, con el formato de `event` de A1). Se reconoce con `instanceof CodedError` **y** con una comprobación en tiempo de ejecución de que `code` está en la lista. Su código se registra en `error_code`. No se confunde con el `ErrorCode` de `handle-error.ts`, que es lo que ve el cliente (`not_found`, `internal`) y no cambia.
  - **`error_message` sale de la allowlist.** El mensaje no se registra nunca, tampoco el de un `CodedError`: el código ya dice lo mismo sin texto libre.
  - **Cualquier otro error**: `error_name` y `stack`. `error_name` se escribe solo si cumple `^[A-Za-z_$][A-Za-z0-9_$]{0,63}$`; si no, vale `other`. Para valores que no son objetos se registra `typeof`, como hoy.
  - **`stack` sin encabezado**: V8 empieza el stack con `Name: message`, así que registrar el stack equivale a registrar el mensaje. Se quita el prefijo igual a `` `${name}: ${message}` `` y, si no coincide (el mensaje cambió después de crearse el error), todo lo que hay antes de la primera línea de marco (`^\s+at `). Lo que queda pasa por el saneador. No se registran `cause` ni los `errors` de un `AggregateError`, y no se cambia `Error.stackTraceLimit`.
  - **Sin marcos no hay `stack`**: si `stack` no es una cadena o no tiene ninguna línea de marco (`^\s+at `), el campo `stack` no se escribe. Cubre un objeto lanzado con un `stack` arbitrario y una dependencia que fije `Error.stackTraceLimit = 0`, casos en que todo el `stack` sería encabezado o texto libre.
  - Riesgo residual aceptado: un mensaje de varias líneas que imite marcos sobrevive al recorte; lo cubre el saneador.
- **server**: el equivalente es `error_kind`, un `&'static str` que sale de un `match` sobre una lista cerrada (como `db::error_kind`). Nunca `%err` ni `?err` en una macro de `tracing`. El tipo por campo de A3 lo hace cumplir en el formateador, además de la revisión.
- Lo que ve el cliente no cambia: `handleError` devuelve `not_found` o `internal` y el `error_id` (RF-A19), y el server devuelve `ApiError` con su código.

#### A6. Agregar un campo o un evento
- **Evento nuevo** con campos ya permitidos: basta el PR de la feature. El código cumple A1 y el nivel sigue A2.
- **Campo nuevo cuyo valor no es de persona, sesión ni dispositivo** según el criterio de A4.13 (un código de una lista cerrada, un número, un id público como `fight_id`). Que el campo, cruzado por tiempo con la BD, pueda apuntar a una persona no lo saca de esta categoría: es el riesgo residual de A4.13. En el mismo PR:
  1. El campo entra en la allowlist de su app, con su tipo (A3), y en el espejo del e2e (`apps/web/e2e/support/logs.ts`) si es de `web`.
  2. Un canario que intente filtrar un valor sensible por ese campo (A8).
  3. Si el campo cambia lo que dice la fila "Registros técnicos" del aviso de privacidad, el aviso se actualiza en ese PR.
  4. Revisa security-reviewer.
- **Campo cuyo valor es o deriva de una clave o un atributo de persona, sesión o dispositivo** (A4.13): además de lo anterior, requiere **un ADR que reemplace o enmiende este** con OK del owner (gate de `risk: high`). Ese ADR justifica la finalidad y actualiza en el mismo PR las secciones 3, 4 y 8 del aviso. El plazo de A7 se vuelve a medir, porque una línea más larga cambia los días.
- **Lo de A4.1 a A4.12 no tiene procedimiento**: cambiarlo requiere reemplazar este ADR y, para la IP, también ADR-0008.

#### A7. Plazo y retención
- El alcance es el de la parte A: `web`, `server` y Postgres.
- `web` y `server` usan `json-file` con `max-size: 10m` y `max-file: 3`, fijado en el compose y verificado por `check-compose.mjs` (ADR-0006 D1, RF-C4). Que Coolify respeta ese bloque al reescribir el compose lo confirma el `docker inspect` de `LogConfig` de los prerrequisitos de RF-C18; hasta entonces, el plazo es una afirmación sobre el compose, no sobre el VPS.
- Los logs **pueden borrarse antes** del máximo: Docker los borra al recrear el contenedor, lo que pasa cuando Coolify despliega o hace rollback a otro tag. Un "Restart" desde Coolify o un reinicio por `restart: unless-stopped` los conserva. **El máximo no depende de los despliegues**: el plazo se apoya solo en la rotación y en el volumen mínimo.
- **server**: el healthcheck (cada 15 s) garantiza un mínimo de unos 1.67 MB por día (línea de 192 B, 290 B con el envoltorio de `json-file`, medida el 2026-10-24), así que 30 MB duran **como máximo 18.8 días**. El aviso dice **"menos de 3 semanas"**.
- **web**: escribe solo arranque y errores, así que sus logs pueden durar más. El aviso lo declara (sección 8), y es aceptable porque no contienen datos del cliente (A4).
- **Postgres**: escribe poco (arranque, checkpoints, autovacuum y errores), así que sus logs pueden durar más de 3 semanas, como los de `web`. Es aceptable solo porque A9 impide que contengan datos de la petición o de las filas. Su rotación no la fija nuestro compose de producción (es un recurso standalone de Coolify): la comprueba el `docker inspect` de `LogConfig` de los prerrequisitos de RF-C18, y si no es `json-file` 10m × 3 se fija en `/etc/docker/daemon.json` del VPS (`log-driver` y `log-opts`, que valen para los contenedores creados después) y se recrea el recurso.
- **Traefik**: sin access logs, apagados en el proxy y por router (ADR-0006 D11, que también fija cómo activarlos para diagnosticar).
- Los logs no salen del VPS: no hay driver remoto, agregador ni servicio externo (eso necesitaría un ADR y el OK del owner). Un extracto copiado para diagnosticar se borra al cerrar el diagnóstico, como máximo a los 7 días, igual que en ADR-0006 D11.
- **Cambiar la promesa exige actualizar el aviso en el mismo PR**, con la medición nueva. Cuenta como cambio todo lo que alargue la vida de una línea: cambiar el driver, `max-size` o `max-file`; espaciar el healthcheck; bajar `http.request` o `/api/health` de `info`; acortar la línea de petición; o conservar logs entre despliegues (volumen, driver remoto). Alargar la línea acorta el plazo, así que no rompe la promesa, aunque igual pasa por A6.

#### A8. Pruebas (canarios)
- **server**: `apps/server/tests/logs.rs` corre el binario con `RUST_LOG=info`, con `debug` y con `trace` para terceros. Manda canarios en `Cookie`, `Authorization`, `X-Internal-Secret`, `X-Client-Ip`, `X-Forwarded-For`, el query, `Origin` y un payload de WS, más la contraseña de la BD, y comprueba que ningún canario sale y que solo aparecen campos permitidos. Los unitarios de `telemetry.rs` prueban la allowlist, los externos, los topes, la inyección y `ts`.
- **web**: el e2e `apps/web/e2e/logs.spec.ts` con `/dev/error-probe` y la lista blanca de `e2e/support/logs.ts` (campos y `CANARIES`), más los unitarios de `sanitize.ts`, `log.ts` y `handle-error.ts`.
- **Regla**: toda fuente nueva de dato sensible agrega su canario en el mismo PR que la introduce. Por ejemplo, auth agrega la cookie de sesión, `__Host-oauth`, `code` y `state` en el callback, `X-Client-Origin` y `X-Client-Fetch-Site`; y la feature de peleas, una semilla canario antes y después de `Settled`. Todo campo nuevo (A6) agrega un canario que intente pasar por él.

#### A9. Postgres
Postgres no pasa por ninguna allowlist, así que su parte del contrato es de configuración. Con los valores por defecto (`log_error_verbosity = default`, `log_min_error_statement = error`), cada error escribe su `DETAIL` y la sentencia: una violación de unicidad escribe `Key (display_name)=(…)`, y una de `NOT NULL` o `CHECK`, `Failing row contains (…)` con la fila completa (alias, `ip_hash`, `token_hash`). Eso rompería A4.1, A4.2 y A4.13.
- **Parámetros fijos** en el Postgres de producción (la "Custom PostgreSQL Configuration" de Coolify, junto al tuning de ADR-0006 D4), en el `command` de `infra/compose.local.yml` y en el de `infra/compose.dev.yml`. Los verifiqué contra la documentación de PostgreSQL 17 (la versión de las imágenes):
  | Parámetro | Valor | Efecto |
  |---|---|---|
  | `log_error_verbosity` | `terse` | Quita `DETAIL`, `HINT`, `QUERY` y `CONTEXT` de cada mensaje; queda el mensaje primario |
  | `log_min_error_statement` | `panic` | No escribe la línea `STATEMENT` de una sentencia que falla (solo lo haría ante un `PANIC`, y entonces el proceso cae) |
  | `log_statement` | `none` | No registra sentencias (es el valor por defecto; se fija para que no cambie) |
  | `log_min_duration_statement` | `-1` | Apaga el registro de sentencias lentas, que escribiría la sentencia y, con `log_parameter_max_length`, sus parámetros |
  | `log_connections` | `off` | Sin una línea por conexión, que lleva el host del cliente (en PostgreSQL 18 pasa a ser una lista; al subir de mayor se revisa) |
  | `log_disconnections` | `off` | Sin una línea por desconexión, que también lleva el host |
- `log_line_prefix` se deja en su valor por defecto (`%m [%p] `) y nunca lleva `%h` ni `%r` (host del cliente). `log_hostname` queda en `off`. No se cargan `auto_explain` ni `pg_stat_statements` con salida a log.
- **El mensaje primario sí se escribe**, y `terse` no lo quita. Algunos llevan el valor que falló (`invalid input syntax for type uuid: "…"`, `invalid input value for enum …: "…"`). Por eso, **en el server todo valor que viene del cliente se valida o se parsea a su tipo en Rust antes de llegar a SQL** (un UUID se parsea a `Uuid`, un enum a su tipo, un número a su entero con rango), y se envía como parámetro con tipo, nunca como texto que Postgres convierte. Además, las funciones, triggers y `CHECK` propios nunca interpolan valores de una fila en un `RAISE` (p. ej. los del ledger, ADR-0005): su mensaje es un código fijo.
- La comparación entre los tres sitios la hace `check-compose.mjs` en los dos compose locales; en producción, la confirma `SHOW log_error_verbosity` (y los demás parámetros) en los prerrequisitos de RF-C18 (Consecuencias, trabajo 13).

### Parte B: CSP y cabeceras

#### B1. CSP base (RF-A10, tal cual está en `apps/web/vite.config.ts`)
```
default-src 'self'; script-src 'self' 'nonce-…'; style-src 'self' 'unsafe-inline';
img-src 'self' data: blob:; font-src 'self'; connect-src 'self'; object-src 'none';
base-uri 'self'; form-action 'self'; frame-ancestors 'none'
```
- La emite **solo Kit** (opción `csp` del plugin `sveltekit()`, `mode: 'auto'`), como cabecera en cada respuesta SSR. Como ninguna ruta se prerenderiza (`check-prerender.mjs`), `auto` siempre usa nonce por petición; el modo hash no se usa. Traefik no pone CSP (no conoce el nonce), y ni `handle` ni un `<meta>` agregan una segunda política.
- `style-src 'unsafe-inline'` lo impone Bits UI (posiciona con estilos en línea). Es un riesgo residual aceptado y nunca se extiende a `script-src`.
- Los estáticos que sirve `sirv` no llevan CSP. Es aceptable: la CSP protege documentos, y los únicos documentos son SSR.

#### B2. Cabeceras
**web**: cada una sale **una sola vez**, con el mismo valor en `handle` (`security-headers.ts`, para `prod-local` y los e2e) y en el middleware Traefik `torneo-web-headers` (para `sirv` y la 308 de Kit). `check-compose.mjs` compara los dos. Si se duplican, se corrige el middleware y no se quita de `handle` (RF-C4).
| Cabecera | Valor | Estado |
|---|---|---|
| `X-Content-Type-Options` | `nosniff` | existe |
| `Referrer-Policy` | `strict-origin-when-cross-origin` | existe |
| `Strict-Transport-Security` | `max-age=31536000` | existe |
| `Cross-Origin-Opener-Policy` | `same-origin` | **nueva** |
| `Permissions-Policy` | `camera=(), microphone=(), geolocation=(), payment=(), usb=(), display-capture=()` | **nueva** |
| `Content-Security-Policy` | B1 | existe; solo Kit, nunca Traefik |

- COOP `same-origin` no rompe el login: el flujo OAuth es de navegación completa con 303, sin popups (ADR-0008 D2). `fullscreen` queda con su valor por defecto (`self`) para la vista de pelea.

**server** (`apps/server/src/http/mod.rs`): en toda respuesta, errores incluidos. Traefik no agrega cabeceras al router `torneo-api`.
| Cabecera | Valor | Estado |
|---|---|---|
| `X-Content-Type-Options` | `nosniff` | existe |
| `Strict-Transport-Security` | `max-age=31536000` | existe |
| `Cross-Origin-Resource-Policy` | `same-origin` | **nueva** |
| `Content-Security-Policy` | `default-src 'none'; frame-ancestors 'none'` | **nueva** (respuestas JSON: nada que cargar ni enmarcar) |
- Ninguna `Access-Control-Allow-*` (ADR-0008 D4) y `Cache-Control: private, no-store` donde lo pide ADR-0008 D5.

**HSTS**: un año, **sin `includeSubDomains` ni `preload`**, en las dos apps. `includeSubDomains` forzaría HTTPS durante un año en otros subdominios del owner que este repo no controla. `preload` exige `includeSubDomains`, y salir de la lista de precarga tarda meses. Los navegadores ignoran HSTS sobre `http`, así que local y e2e no se afectan.

#### B3. Prohibido siempre
- En `script-src`: `'unsafe-eval'`, `'unsafe-inline'`, `'unsafe-hashes'`, `'strict-dynamic'`, `data:` y `blob:`.
- En cualquier directiva: `*`, comodines de host (`*.ejemplo.com`), esquemas sueltos (`https:`, `http:`, `wss:`, `ws:`) y cualquier origen externo (CDN, analítica, fuentes; ADR-0009).
- `data:` en `default-src`, `script-src`, `connect-src` y `worker-src`. `blob:` fuera de `img-src`.
- Quitar o relajar `object-src 'none'`, `base-uri 'self'`, `form-action 'self'` o `frame-ancestors 'none'`, cambiar `mode` a algo que no sea `auto` o quitar la opción `csp`.
  - *Ver la "Enmienda 2026-10-30: `form-action` para el login con Google y Discord" (pendiente de las firmas de security-reviewer y devops-engineer): `form-action` admite, además de `'self'`, las dos fuentes de B4.6, y nada más.*
- `'wasm-unsafe-eval'` en cualquier directiva que no sea `script-src`.
- En cualquier directiva, una fuente `'nonce-…'` o `'sha256-…'`/`'sha384-…'`/`'sha512-…'` escrita en la configuración. El único nonce es el que Kit genera por petición; un nonce fijo (p. ej. `'nonce-dev'`) lo puede repetir un XSS en `<script nonce="dev">`, y un hash fijo autoriza un script en línea fuera del bundle.
- `'unsafe-allow-redirects'` e `'inline-speculation-rules'` en cualquier directiva.
- En `img-src`, cualquier esquema distinto de `data:` y `blob:` (`filesystem:`, `mediastream:`, etc.).
- **La regla que lo resume**: la configuración `csp` de `vite.config.ts` es exactamente la de B1 más, como mucho, las ampliaciones de B4 (`'wasm-unsafe-eval'` en `script-src` y `worker-src 'self'`; el origen `wss://` de B4.4 solo si llega a hacer falta, con su chequeo). Cualquier otra directiva o fuente está prohibida aunque esta lista no la nombre, y el chequeo del trabajo 6 es una **allowlist exacta**, no una lista negra.

#### B4. Ampliaciones permitidas y sus condiciones
1. **`'wasm-unsafe-eval'` en `script-src`**, para `engine-wasm` (verificación en el navegador; ADR-0002 punto 4, intent de 003).
   - Entra **en el PR de la feature que carga `engine-wasm` en el navegador**, no antes (002 no la necesita).
   - El `.wasm` se sirve como archivo del mismo origen (bajo `/_app/`) y se carga con `fetch`, cubierto por `connect-src 'self'`. Nunca inlineado como `data:`.
   - Es global, porque la CSP de Kit es una sola para toda la app. Se acepta porque solo permite compilar WebAssembly, no evaluar JavaScript.
2. **PixiJS v8 (002): sin cambios en la CSP.**
   - El renderer importa `pixi.js/unsafe-eval` en el punto de entrada de `packages/renderer`, antes de crear cualquier renderer, para que ningún consumidor lo olvide. Pese al nombre, ese módulo hace que Pixi funcione **sin** `'unsafe-eval'`. Lo verifiqué en `pixi.js@8.22.0`: `lib/unsafe-eval/init.mjs` reemplaza por polyfills los generadores con `new Function` (sincronización de shaders, uniforms, UBO y partículas) y anula `_unsafeEvalCheck`. Sin el import, el chequeo de `AbstractRenderer` sondea `new Function`, lo que dispara una violación, y lanza.
   - **Sin workers de Pixi.** Con `preferWorkers: true` (el valor por defecto), el cargador de texturas crea workers desde URLs `blob:` (`_virtual/loadImageBitmap.worker.mjs`, `checkImageBitmap.worker.mjs`). El renderer fija `Assets.setPreferences({ preferWorkers: false })` y decodifica en el hilo principal: con unos pocos spritesheets el costo es despreciable. Los cargadores KTX2 y Basis de Pixi también usan workers `blob:` y no se usan.
   - `img-src data: blob:`, que ya existe, cubre los SVG y `HTMLText` de Pixi (`data:image/svg+xml`). No se agrega nada más.
3. **Workers propios**: solo como archivo empaquetado del mismo origen (`new Worker(new URL('./x.worker.ts', import.meta.url), { type: 'module' })`), agregando `worker-src 'self'` explícitamente en ese PR. **Nunca `worker-src blob:`** ni `?worker&inline` de Vite.
4. **`connect-src 'self'`** cubre `fetch` y el WebSocket del mismo origen (en CSP nivel 3, `'self'` incluye `wss:` del mismo host).
   - Hoy **no está comprobado sobre `https`/`wss`**. El e2e de `/dev/status` es opcional: queda fuera de la CI (`testIgnore` en `playwright.config.ts`), corre solo con `playwright.dev.config.ts` contra `just dev` y usa `http` y `ws://`. Si `'self'` no cubriera `wss:`, la conexión fallaría cerrada (no hay fuga), pero la vista en vivo no funcionaría.
   - **La primera feature que use el WebSocket en producción** agrega, en su PR, una comprobación en WebKit de una página `https` que abre `wss://` del mismo host sin violación de CSP: en la CI si puede servir `https` (p. ej. detrás de un proxy con certificado local), o si no como prerrequisito de RF-C18 contra producción (Consecuencias, trabajo 14).
   - Si un navegador soportado falla, se agrega el origen exacto `wss://<host>`, derivado de `PUBLIC_ORIGIN` en `handle`, porque la CSP de Kit es de build y la imagen sirve a cualquier host (RF-C7). Nunca el esquema suelto `wss:`. Esa es la única fuente de B4 que no está en `vite.config.ts`, y su PR extiende el chequeo del trabajo 6 para que la vea.
5. Cualquier otra ampliación requiere un ADR que reemplace este.
   - *Ver la "Enmienda 2026-10-30": agrega B4.6 y permite ampliar por enmienda de este ADR con las firmas de sus revisores.*

#### B5. Reportes de CSP
- **No hay `report-uri` ni `report-to`**, tampoco `Content-Security-Policy-Report-Only` en producción. Las razones:
  - Un reporte trae `document-uri`, `referrer`, `blocked-uri` y una muestra del código, es decir, URLs con query (incluidos `code` y `state` de OAuth): registrarlos rompe A4.5 y A4.7.
  - Haría falta un endpoint público de escritura sin sesión, con ruido de extensiones y volumen que compite por la única vCPU, mientras el server es de solo lectura (RF-B12).
  - Un colector externo es un servicio externo (ADR-0006).
- La detección es el e2e: `apps/web/e2e/csp.spec.ts` escucha `securitypolicyviolation` en Chromium y WebKit.

#### B6. Cambiar la CSP o una cabecera
- Implementa frontend-engineer (o backend-engineer para el server) y revisan **security-reviewer y architect**. El PR es `risk: high` si relaja algo y `medium` si solo endurece.
- Solo se permiten las ampliaciones de B4. Cualquier otra necesita un ADR nuevo.
  - *Ver la "Enmienda 2026-10-30": o una enmienda según B4.5 enmendado.*
- En el mismo PR:
  1. `csp.spec.ts` cubre cada ruta nueva o afectada (p. ej. la página del renderer o la de verificación) y no da ninguna violación en Chromium ni en WebKit.
  2. El chequeo de directivas (Consecuencias, trabajo 6) pasa.
  3. En `web`, `security-headers.ts` y las etiquetas de `torneo-web-headers` cambian juntos (`check-compose.mjs` falla si divergen), y el e2e de cabeceras comprueba los valores.
  4. Una cabecera de B2 se verifica en estos sitios, y todos cambian en el mismo PR: `apps/web/e2e/security.spec.ts`; `headerLabels` y la lista `expected` de `parseSecurityHeaders` en `scripts/check-compose.mjs`; el paso "Health, version and headers (RF-C7)" del job `images` de `.github/workflows/ci.yml`; el test de integración de RF-B12 para las del server; el paso de cabeceras del job `verify` de `release.yml` (trabajo 11), y los `curl -I` de los prerrequisitos de RF-C18 (y del issue "Primer despliegue a producción" mientras siga abierto).
  5. La descripción del PR justifica cada fuente nueva.

## Alternativas descartadas
- **Registrar `error_message` saneado** (lo de 001): el saneador es una lista negra y siempre le va a faltar un formato (TA-11 ya tiene huecos). Para los errores ajenos, el mensaje es texto libre de bibliotecas que pueden incluir URLs, headers o datos de la petición.
- **Registrar `error_message` solo de errores propios** (la sugerencia literal): el código del error propio ya es la información, y su mensaje, si alguien interpola un valor por descuido, vuelve a ser texto libre. Registrar `error_code` da lo mismo sin ese riesgo.
- **Registrar el stack completo**: su primera línea es `Name: message`, así que reintroduce el mensaje.
- **Lista negra en vez de allowlist como control primario**: falla abierta ante cualquier campo nuevo.
- **`user_id` en la línea de petición** para correlacionar: convierte los logs en un historial de actividad por persona que dura semanas y obliga a declararlo. Si hace falta, va por A6 con ADR y OK del owner.
- **Registrar la semilla después de `Settled`**: no aporta nada (ya es pública) y abre un caso límite donde un error de estado la escribe antes.
- **`debug` en producción para diagnosticar**: con la allowlist no aporta campos y multiplica el volumen, lo que acorta y vuelve impredecible el plazo del aviso.
- **Agregador de logs (Loki, Better Stack, etc.)**: es un servicio externo o más memoria en un VPS de 4 GB, y saca los logs del host, lo que cambia el plazo y los destinatarios del aviso.
- **`worker-src 'self' blob:` para los workers de Pixi**: permite ejecutar como worker cualquier código que la página arme en un `Blob`, fuera del bundle revisado. `preferWorkers: false` cuesta casi nada.
- **`'unsafe-eval'` para Pixi**: abre `eval` y `new Function` a cualquier XSS. Pixi trae el módulo para evitarlo.
- **`'wasm-unsafe-eval'` solo en la ruta de verificación** (CSP por ruta desde `handle`): la CSP de Kit es global, y reescribirla por ruta duplica la política en dos lugares a cambio de muy poco.
- **`wss:` como esquema en `connect-src`**: permitiría abrir sockets a cualquier host, lo que es un canal de exfiltración.
- **Reportes de CSP a un colector propio o externo**: ver B5.
- **`includeSubDomains` y `preload` en HSTS**: ver B2.
- **`Cross-Origin-Embedder-Policy`**: solo hace falta para `SharedArrayBuffer`, que no se usa, y puede romper recursos propios sin `CORP`. Se revisa si el motor en WASM usa hilos.
- **`X-Frame-Options`**: `frame-ancestors 'none'` ya lo cubre en todos los navegadores soportados.
- **Trusted Types (`require-trusted-types-for 'script'`)**: no está probado con Svelte 5, Kit 3 ni Bits UI. Se revisa más adelante.
- **CSP en Traefik**: no conoce el nonce, así que obligaría a `'unsafe-inline'` o a hashes fijos.
- **Apagar los logs de Postgres (`log_min_messages = fatal` o `panic`)**: perdería los avisos de arranque, checkpoints y autovacuum, que sirven para diagnosticar memoria y disco en el VPS. Con A9 esas líneas no llevan datos de la petición.
- **Revisar a mano la duplicación de cabeceras en cada despliegue**: depende de que alguien se acuerde; el job `verify` ya existe y lo hace en segundos (trabajo 11).
- **Capas de diagnóstico del subscriber (tokio-console, OpenTelemetry) en producción**: ven los campos de span y el texto de los crates externos, y tokio-console abre un puerto gRPC. Requieren un ADR nuevo (A3).

## Consecuencias
- **Más fácil**: una feature sabe qué puede registrar sin releer la spec de 001, y el aviso de privacidad tiene una sola fuente técnica. La CSP de 002 y 003 ya está decidida, y 002 no la toca.
- **Más difícil**:
  - Diagnosticar en producción sin URL, sin mensaje y sin usuario. Se compensa con `error_id`, `request_id`, `route`, `error_code` y `error_kind`, y reproduciendo en `prod-local`.
  - Todo error propio de `web` que valga la pena distinguir necesita un código en `SERVER_ERROR_CODES`.
  - Diagnosticar un error de Postgres sin `DETAIL` ni sentencia. Se compensa con el `error_kind` del server y reproduciendo en local, donde se puede subir la verbosidad a mano sin commitearlo.
- **Vigilar**:
  - Que una versión nueva de Pixi agregue un sitio con `new Function` que `pixi.js/unsafe-eval` no cubra (lo detecta `csp.spec.ts` en la ruta del renderer).
  - La duplicación de cabeceras entre `handle` y Traefik solo se observa en producción (RF-C18, "Áreas de preocupación" 13 de 001). Deja de ser una vigilancia manual con el trabajo 11: el job `verify` de cada release falla si una cabecera falta, sale dos veces o cambia de valor.
  - Que una subida de mayor de Postgres cambie los parámetros de A9 (en PostgreSQL 18, `log_connections` pasa a ser una lista).
  - Que el plazo depende del healthcheck (A7).
- **Relación con otros ADRs**: este ADR prevalece sobre la viñeta "Logs" de ADR-0008 D3, que es más laxa, y no la contradice. ADR-0006 D11 sigue vigente para Traefik.

### Trabajo de código derivado (no se implementa en este ADR)
Ya existe **TA-11**: huecos del saneador y lint de `import()` dinámicos. Este ADR le suma: agregar `x-real-ip`, `forwarded` y el patrón `x-client-[a-z-]+` a `HEADER_NAMES` de `sanitize.ts`.

Trabajo nuevo. Cada punto dice su destino en Linear; los que comparten issue se implementan juntos.
1. **Errores con código en `web`** (issue nuevo "ADR-0012: logs de web", con 2):
   - Crear `apps/web/src/lib/server/errors.ts` con `CodedError` y `SERVER_ERROR_CODES` (`ServerErrorCode`).
   - En `log.ts`: quitar `error_message`, agregar `error_code`, validar `error_name`, recortar el encabezado del `stack` y no escribir `stack` si no tiene ninguna línea de marco (A5). Cambiar `readError` en `handle-error.ts` y `errorFields` en `log.ts` (que usa `process-handlers.ts`).
   - Pruebas: actualizar los tests y `ALLOWED_FIELDS` de `apps/web/e2e/support/logs.ts`. Agregar a `/dev/error-probe` un canario en el mensaje, para comprobar que el mensaje ya no llega en el `stack`, y un canario en `name` con un carácter fuera de `^[A-Za-z_$][A-Za-z0-9_$]{0,63}$` (p. ej. `-`), comprobando que no aparece y que `error_name` vale `other`. Unitarios para un `stack` sin marcos (objeto con `stack` arbitrario y `Error.stackTraceLimit = 0` restaurado al terminar), que no escriben `stack`.
2. **`event` tipado en `web`** (con 1): una unión cerrada `LogEvent` en `log.ts` (`web_started`, `unhandled_error`, `unhandled_rejection`, `uncaught_exception`).
3. **`level` en minúsculas en el server** (issue nuevo "ADR-0012: logs del server", con 4, 5 y 12): el formateador de `apps/server/src/telemetry.rs` y sus tests, y cualquier aserción sobre `INFO`/`WARN` en `apps/server/tests/logs.rs`.
4. **Tipo por campo en el server** (con 3): el visitor de `telemetry.rs` acepta los campos de cadena (`file` incluido) solo por `record_str`, los numéricos solo por `record_u64`/`record_i64`, y `record_debug` (`%`/`?`) solo para `request_id`. Implementa de forma explícita `record_bool`, `record_f64`, `record_i128`, `record_u128`, `record_bytes` y `record_error`, que descartan el campo (A3). Tests que comprueban que `error_kind = ?err`, `route = %x`, un `&dyn Error` en `error_kind` (`record_error`) y bytes en `route` no se escriben.
5. **Headers sensibles del server** (con 3): agregar `X-Real-Ip` y `Forwarded` a `SetSensitiveRequestHeadersLayer` en `apps/server/src/http/mod.rs` (la feature de auth agrega `X-Client-Origin` y `X-Client-Fetch-Site` cuando los introduce).
6. **Chequeo de directivas CSP como allowlist exacta** (issue nuevo "ADR-0012: CSP y cabeceras", con 7, 8 y 11):
   - Extender `scripts/check-kit-config.mjs`, que ya lee `apps/web/vite.config.ts` y ya está en `check-web`, para que compare la opción `csp` con una tabla exacta: `mode` igual a `auto` y, por directiva, el conjunto de fuentes de B1. Las únicas diferencias aceptadas son las de B4: `'wasm-unsafe-eval'` agregado a `script-src` y la directiva `worker-src` con exactamente `['self']`. Cualquier otra directiva, fuente u opción falla (incluidos `'nonce-…'`, hashes, `'unsafe-allow-redirects'`, `'inline-speculation-rules'` y esquemas nuevos de B3), como falla que falte una directiva de B1.
   - Tests en `scripts/__tests__/` con un caso por prohibición de B3 y uno por ampliación permitida.
   - Si el plan prefiere un `scripts/check-csp.mjs` nuevo, ese PR lo agrega a la receta `check-web` del `justfile` y a la lista de `just check` de `CLAUDE.md`.
7. **Cabeceras nuevas en `web`** (con 6):
   - `Cross-Origin-Opener-Policy` y `Permissions-Policy` en `SECURITY_HEADERS` (`security-headers.ts`) y en `torneo-web-headers` (`infra/docker-compose.yml`, vía `customResponseHeaders` y `permissionsPolicy`).
   - En `scripts/check-compose.mjs`, el mapeo en `headerLabels` y la lista `expected` de `parseSecurityHeaders` (hoy exige exactamente tres cabeceras), con sus tests; y las aserciones en `apps/web/e2e/security.spec.ts`.
   - En el paso "Health, version and headers (RF-C7)" del job `images` de `.github/workflows/ci.yml`, COOP y `Permissions-Policy` en el bucle que exige que cada cabecera salga una sola vez en `/es/`.
8. **Cabeceras nuevas en el server** (con 6): `Cross-Origin-Resource-Policy: same-origin` y `Content-Security-Policy: default-src 'none'; frame-ancestors 'none'` en `apps/server/src/http/mod.rs`, con el test de integración de RF-B12 (en una 200 y en una 404). En el mismo paso del job `images`, que CORP y la CSP del server salgan una sola vez en `/api/health` y en una 404 de `/api/…`.
9. **Condiciones para el plan de 002** (no es un issue aparte): import de `pixi.js/unsafe-eval` en la entrada de `packages/renderer`, `Assets.setPreferences({ preferWorkers: false })`, ninguna textura KTX2/Basis, y la ruta del renderer en `csp.spec.ts`.
10. **Condiciones para la feature que cargue `engine-wasm`** (no es un issue aparte): `'wasm-unsafe-eval'` en `script-src`, el `.wasm` como archivo del mismo origen y la ruta de verificación en `csp.spec.ts`.
11. **Cabeceras en el job `verify` de `release.yml`** (con 6): un paso después de que `/api/health` reporte el tag hace `curl -sI --proto '=https'` a `https://$PUBLIC_HOST/es/`, `/_app/version.json`, `/healthz/` y `/api/health`, y falla si una cabecera de B2 que corresponde a esa respuesta falta, sale más de una vez o tiene otro valor (y si HSTS lleva `includeSubDomains` o `preload`). Los valores esperados se escriben en el workflow, que no puede importar `security-headers.ts`; `scripts/check-workflows.mjs` y su tabla de permisos tienen que seguir pasando (el job ya tiene `permissions: {}`).
12. **Spans y capas en el server** (con 3): un test en `telemetry.rs` con un canario en un campo de span (de `info_span!` y de `#[instrument(fields(...))]`) que comprueba que no aparece en la línea del evento. Una regla de revisión, o un test que recorra `apps/server/src/` y falle con un `#[instrument` sin `skip_all`, y que `telemetry::init` registra solo `telemetry::layer` como capa con salida (A3).
13. **Logs de Postgres** (issue nuevo "ADR-0012: logs de Postgres", de devops-engineer):
    - Los parámetros de A9 en el `command` de `infra/compose.local.yml` (junto al tuning de ADR-0006 D4) y un `command` nuevo en `infra/compose.dev.yml` con los mismos parámetros de log.
    - En `scripts/check-compose.mjs`: `POSTGRES_COMMAND` con los parámetros nuevos, y la comparación de los parámetros de log de `compose.dev.yml` contra los mismos valores, con sus tests.
    - Los mismos parámetros en la "Custom PostgreSQL Configuration" de Coolify, documentados en la sección "Despliegue" de `docs/architecture.md` junto al tuning de D4.
    - En los prerrequisitos de RF-C18 y en el checklist del issue "Primer despliegue a producción": el `docker inspect` de `LogConfig` del contenedor de Postgres (`json-file` 10m × 3; si no, `daemon.json` y recrear, A7), y `SHOW` de los seis parámetros de A9 con sus valores.
    - La regla de A9 sobre valores del cliente antes de SQL entra en la revisión de backend-engineer y security-reviewer de cada PR con consultas; no es código hoy.
14. **Condición para la primera feature que use el WebSocket en producción** (no es un issue aparte): la comprobación de `wss://` en WebKit de B4.4.

Prerrequisitos de RF-C18 que este ADR suma (van al issue "Primer despliegue a producción", de devops-engineer): los `curl -I` de `/es/`, `/_app/version.json` y `/healthz/` comprueban también COOP y `Permissions-Policy`; uno más a `/api/health` comprueba CORP, la CSP del server, `nosniff` y HSTS, cada una una sola vez; y lo de Postgres del trabajo 13.

Docs que se actualizan al aprobarse (no son código):
- Aviso de privacidad: "Pendiente" 8 nombra `error_message`; cambiarlo a `error_code` y remitir a este ADR. En la sección 8, declarar los logs de Postgres como los de `web`: pueden durar más de 3 semanas y no contienen datos del cliente (A7, A9).
- `docs/architecture.md`: enlazar este ADR en seguridad y logs.
- Spec de 001: en "Áreas de preocupación" 3 y 4, anotar que quedaron resueltas por ADR-0012.

## Enmienda 2026-10-30: `form-action` para el login con Google y Discord
**Firmas:** escrita por architect, como autor de este ADR, tras su revisión de `docs/features/008-autenticacion/plan-d.md` (pregunta abierta 1). Necesita las firmas de security-reviewer y devops-engineer, los revisores de este ADR (ver "Revisión"). El texto incorpora, desde el 2026-10-30, las recomendaciones no bloqueantes 1 a 3 de security-reviewer y los cambios 1 y 2 de devops-engineer, marcados abajo con su origen. El gate del owner de `docs/process.md` ("aceptar o reemplazar un ADR") lo cubre la delegación del owner del 2026-10-29, registrada textual en la sección "Revisión → owner" de la spec de 008: mientras no haya nada desplegado, las aprobaciones del owner para las enmiendas de ADRs `risk: high` de 008 quedan cubiertas por las firmas de los agentes revisores. Esa delegación no cubre un tag `v*` ni un deploy. La enmienda entra en vigor cuando firmen los dos revisores, y se aplica en el PR de la fase D de 008.

Cambia B3, B4 (punto nuevo 6), B4.5 y B6. El resto de la Parte B no cambia, y la Parte A tampoco.

- **Por qué.** `POST /api/auth/{google,discord}/start` y `POST /api/auth/link/{google,discord}` son envíos de formulario a nuestro origen que responden 303 a la URL de autorización del proveedor (constantes de `apps/server/src/auth/providers.rs`). Chromium aplica `form-action` a cada redirección que sigue al envío de un formulario, así que con B1 el login real queda bloqueado. En WebKit no está verificado. El e2e con `dev` no lo veía, porque todo es del mismo origen. Las alternativas eran peores:
  - una página del server con `<meta http-equiv="refresh">` al proveedor mete un documento HTML en el server, que solo responde códigos (ADR-0007), y cambia los contratos de las fases B y C;
  - navegar al proveedor con JavaScript rompe el login sin JavaScript (spec de 008, UI).
- **B4.6 (nuevo). `form-action` del login con proveedores.** `form-action` queda **exactamente** con estas tres fuentes:
  ```
  form-action 'self' https://accounts.google.com/o/oauth2/v2/auth https://discord.com/oauth2/authorize
  ```
  - **Solo en `form-action`.** Ninguna otra directiva gana un origen externo.
  - **Con la ruta del endpoint de autorización, no solo el origen.** Admite los mismos dos orígenes, `https://accounts.google.com` y `https://discord.com`, pero es más estricto. Según CSP nivel 3 ("Does url match expression in origin with redirect count?"), la ruta de una fuente solo se compara cuando la petición no viene de una redirección. Por eso el 303 de `start` y de `link` pasa, porque llega por redirección y se compara solo el origen. En cambio, un formulario inyectado que postee directo a otra ruta de esos orígenes queda bloqueado, y eso incluye un webhook de Discord (`https://discord.com/api/webhooks/…`), que es un canal de exfiltración que controla quien lo crea. Con el origen solo, ese canal quedaba abierto. La ruta se escribe igual que en `providers.rs`, sin barra final y sin query.
  - **Nunca un comodín, un esquema suelto, otro host ni otro puerto.** Si la prueba manual de TA-6 muestra que un proveedor necesita otro host en la cadena de redirecciones que sigue al formulario, eso es otra enmienda con las mismas firmas. Las redirecciones que sigan dentro del mismo origen del proveedor ya pasan.
  - **Atada a `providers.rs`.** Un UT del server fija `GOOGLE_AUTHORIZE` y `DISCORD_AUTHORIZE` como literales, con un comentario que remite a esta enmienda. El PR que cambie una de esas URLs cambia también la CSP y esta enmienda. Si alguien cambia una sin la otra, el login falla cerrado (la CSP bloquea) y el e2e de abajo lo detecta.
  - **Ninguna redirección propia hacia esos orígenes** (recomendación 1 de security-reviewer). Como la ruta de una fuente no se compara tras una redirección, una ruta nuestra que redirija a otra URL de `accounts.google.com` o `discord.com` reabriría el canal del webhook: `'self'` deja pasar el envío y B4.6 la redirección por origen, y un 307 o 308 conservaría el cuerpo. Por eso **ninguna ruta de `server` ni de `web` redirige a esos dos orígenes salvo a las constantes de `providers.rs`, y nunca con 307 ni 308** (la de `start` y `link` es 303). Lo fijan un UT del server (esos orígenes solo aparecen en `providers.rs`, y `auth/` no usa `Redirect::temporary` ni `Redirect::permanent`), un IT que lee el estado de la redirección de `start` y `link`, y un Vitest de web (ningún archivo de `src/**` fuera de `vite.config.ts` nombra esos orígenes). `next` ya no puede apuntar fuera (regex de D2 de ADR-0008).
  - **Global.** La CSP de Kit es una sola para toda la app, así que vale en todas las páginas, no solo en `/login` y `/account`. Se acepta: un formulario con destino al endpoint de autorización solo le pide al proveedor que muestre su pantalla de login. Un `<a href>` inyectado, que CSP nunca restringió, logra lo mismo.
  - **Qué recibe el proveedor** (recomendación 2 de security-reviewer). Con el 303 de `start` y `link`, solo los parámetros que arma el server. Con una inyección de HTML que deje un `<form>` sin cerrar (form hijacking), los campos legítimos de nuestra página viajarían al endpoint de autorización. Ninguno de los dos proveedores los reenvía a un tercero sin el clic del usuario en su pantalla de consentimiento, y ese reenvío va al `redirect_uri` registrado del cliente, no al nuestro. Para que esos campos no sirvan de nada, **ningún formulario nuestro lleva un campo `state`, `client_id` ni `redirect_uri`, ni un secreto** (los arma solo el server, en el `Location` del 303). Lo fija un Vitest de web sobre `src/**`.
- **B3 (enmendado).** El punto "Quitar o relajar … `form-action 'self'`" sigue prohibiendo quitar `'self'` y agregar cualquier fuente a `form-action` que no sean las dos de B4.6. El punto "cualquier origen externo" tiene esta única excepción. "La regla que lo resume" pasa a ser: la configuración `csp` de `vite.config.ts` es exactamente B1 más, como mucho, las ampliaciones de B4 (`'wasm-unsafe-eval'` en `script-src`, `worker-src 'self'`, las dos fuentes de B4.6 en `form-action` y, solo si llega a hacer falta, el origen `wss://` de B4.4 con su chequeo).
- **B4.5 (enmendado).** Cualquier otra ampliación requiere un ADR que reemplace este o una enmienda de este ADR, firmada por sus revisores (security-reviewer y devops-engineer) y con el gate del owner de `docs/process.md`.
- **B6 (enmendado).** "Cualquier otra necesita un ADR nuevo" pasa a "un ADR nuevo o una enmienda según B4.5". El PR que aplique B4.6 es `risk: high` (relaja), lo revisan security-reviewer y architect, y cambia en el mismo PR, como pide B6.4:
  1. `apps/web/vite.config.ts` (`'form-action'` con las tres fuentes);
  2. la allowlist exacta de `scripts/check-kit-config.mjs` (`CSP_EXTENSIONS`) y sus tests, con un caso que acepta las tres fuentes y casos que rechazan el origen sin ruta, otra ruta, otro host, `https:`, un comodín y una fuente de B4.6 en otra directiva;
  3. la cadena `b1` del paso "Health, version and headers (RF-C7)" del job `images` de `.github/workflows/ci.yml`;
  4. la cadena `b1` del paso de cabeceras del job `verify` de `.github/workflows/release.yml`;
  5. `apps/web/e2e/security.spec.ts` (la tabla de directivas esperadas) y `csp.spec.ts` (`/login` y `/account`, sin violación en Chromium ni en WebKit);
  6. *(cambio 2 de devops-engineer)* nada en los prerrequisitos de RF-C18 (`docs/architecture.md`, "Prerrequisitos del primer despliegue a producción", punto 2) ni en TA-8: solo comparan la CSP del server en `/api/health`, que no cambia. Si un PR posterior agrega una comparación de la CSP de `web`, la cadena va idéntica a `b1` y entra en el chequeo del punto 7;
  7. *(cambio 1 de devops-engineer)* `scripts/check-workflows.mjs` comprueba que el valor de `b1=` del paso "Health, version and headers (RF-C7)" del job `images` de `ci.yml` y el del paso "Security headers (ADR-0012 B2)" del job `verify` de `release.yml` existen, aparecen una sola vez cada uno y son idénticos byte a byte, con un caso en `scripts/__tests__/check-workflows.test.mjs` que falla si difieren y otro que falla si falta uno. Corre en `policy`, que corre siempre, y como `images` ya compara la cadena de `ci.yml` con la cabecera de la imagen, la de `release.yml` queda comprobada antes del merge y no recién en `verify`, con producción ya actualizada. Es una regla general: cubre también cualquier ampliación posterior de B4 (`wss://` de B4.4 incluida). Los comentarios junto a las dos cadenas y el mensaje de error de `images` dicen "B1 más las ampliaciones de B4 en uso (B4.6)", no "B1" a secas.
- **Prueba de que funciona y de que no sobra.** El e2e de la fase D de 008 postea el formulario de Google y el de Discord con un proxy de egreso local que registra los hosts y corta la conexión (nada sale de loopback), en Chromium y en WebKit, con loopback (`localhost`, `127.0.0.1`, `::1`) en `bypass` en los dos motores y un teardown que falla si el registro del proxy tiene un host no declarado (así la app nunca pasa por él). Con la enmienda, el proxy recibe `CONNECT accounts.google.com:443` y `CONNECT discord.com:443` (recomendación 3 de security-reviewer) y no hay `securitypolicyviolation`. El e2e también comprueba el caso negativo: un formulario que postea directo a `https://discord.com/api/webhooks/x` da una violación de `form-action` y el proxy no recibe nada. Si WebKit no aplica `form-action` a las redirecciones, la enmienda no le cambia nada, y el e2e lo deja escrito en el PR.

## Revisión
### devops-engineer — 2026-10-26 — REQUEST_CHANGES
- Verifiqué:
  - La línea base de infra coincide con el repo. `infra/docker-compose.yml` tiene `json-file` 10m × 3 en los dos servicios y `RUST_LOG: info` como literal. Como no es `${RUST_LOG}`, una variable de Coolify no lo puede cambiar. También tiene `observability.accesslogs=false` en los cuatro routers y `torneo-web-headers` solo en `torneo-web`, con sus cinco etiquetas.
  - `check-compose.mjs` exige `RUST_LOG` exacto (`ENV.server`), el bloque `logging` exacto y el conjunto exacto de etiquetas: una etiqueta `contentSecurityPolicy` en Traefik fallaría como `unexpected`. Deriva las etiquetas de `SECURITY_HEADERS`. `torneo-api` no lleva middleware de cabeceras, así que B2 ("Traefik no agrega cabeceras a `torneo-api`") es correcto.
  - El cálculo de A7 da 31,457,280 B / (290 B × 5,760) = 18.8 días. Coincide con el aviso (sección 8, "Pendiente" 8) y con ADR-0006 D1/D11.
  - Las cabeceras nuevas se pueden poner en Traefik v3 (Coolify v4): `permissionsPolicy` es una opción propia del middleware `headers`, y COOP va por `customResponseHeaders`. Según el código de Traefik, las dos (y las de `secure`) usan `Header.Set` sobre la respuesta, así que reemplazan el valor de `handle` en vez de duplicarlo. El riesgo real es que los valores diverjan, y eso ya lo cubre `check-compose.mjs`.
  - La CSP y la CORP del server no chocan con el 101 de `/ws` ni con el fetch interno de `web` a `torneo-server` (CORP no aplica fuera del navegador). Las páginas de error del propio Traefik (502, o host sin router) salen sin nuestras cabeceras. Es aceptable y no depende de este repo.
  - Un `check-csp` nuevo en `scripts/` ya entra en el filtro `web` de `ci.yml` (`scripts/**`): no hace falta cambiar workflows y cuesta casi 0 minutos.
- Cambios pedidos:
  1. **(Bloqueante) Postgres queda fuera del contrato sin decirlo.** A4 dice "en ninguna app" y A7 habla de "los dos contenedores", pero el Postgres standalone de Coolify también escribe logs en el VPS. Su configuración por defecto (`log_error_verbosity = default`, `log_min_error_statement = error`) registra el `DETAIL` y el `STATEMENT` de cada error. Una violación de unicidad escribe `Key (display_name)=(…)` y una de `CHECK`, `Failing row contains (…)` con la fila completa (alias, `ip_hash`, `token_hash`). Eso rompe A4.1, A4.2 y A4.13 por una vía que ninguna allowlist cubre. Además, su retención no la fija nuestro compose ni está medida: depende del `daemon.json` del VPS y, con el poco volumen que escribe, puede durar más de 3 semanas. Pido:
     - (a) En A7 y en el Contexto, declarar el alcance de forma explícita: `web`, `server` y Postgres.
     - (b) Un trabajo derivado de infra. En la "Custom PostgreSQL Configuration" de Coolify (donde ya va el tuning de ADR-0006 D4), poner `log_error_verbosity = terse` (quita `DETAIL`, `HINT`, `QUERY` y `CONTEXT`), `log_min_error_statement = panic`, `log_statement = none`, `log_min_duration_statement = -1` y `log_connections`/`log_disconnections = off`. Replicarlo en el `command` de `infra/compose.local.yml` y en `infra/compose.dev.yml`, con la comparación en `check-compose.mjs`.
     - (c) Agregar a los prerrequisitos de RF-C18 el `docker inspect` de `LogConfig` del contenedor de Postgres (rotación 10m × 3) y un `SHOW log_error_verbosity`.
     - (d) Una regla para el server: todo valor del cliente se valida o se parsea antes de llegar a SQL (p. ej. un UUID). Así el mensaje primario de Postgres (`invalid input syntax for type uuid: "…"`), que `terse` no quita, nunca lleva texto del cliente.
  2. **(No bloqueante) Los trabajos 7 y 8 no nombran la verificación existente en CI ni el checklist del primer despliegue.** Agregar:
     - en el paso "Health, version and headers (RF-C7)" del job `images` de `.github/workflows/ci.yml`, que COOP y `Permissions-Policy` salgan una sola vez en `/es/` y que `Cross-Origin-Resource-Policy` y la CSP del server salgan una sola vez en `/api/health` y en una 404 de `/api/…`;
     - en los prerrequisitos de RF-C18 (y en el issue de Linear "Primer despliegue a producción"), las cabeceras nuevas en los `curl -I` de `/es/`, `/_app/version.json`, `/healthz/` y `/api/health`;
     - en `check-compose.mjs`, además de `headerLabels`, la lista `expected` de `parseSecurityHeaders`, que hoy exige exactamente tres cabeceras.
  3. **(No bloqueante) Automatizar el "Vigilar" de duplicación.** No hace falta observarlo a mano en producción: el job `verify` de `release.yml`, después de que `/api/health` reporte el tag, puede hacer `curl -sI` a esas cuatro URLs y fallar si una cabecera de B2 falta, sale dos veces o tiene otro valor. Son segundos dentro de un job que ya existe. Proponerlo como trabajo derivado 11 o dejar dicho por qué no se hace.
  4. **(No bloqueante) A7, "se borran en cada despliegue y en cada rollback".** Solo pasa cuando Coolify recrea el contenedor (cambia el tag). Un "Restart" desde Coolify o un reinicio por `restart: unless-stopped` los conserva. Redactarlo como "pueden borrarse antes; el máximo no depende de eso", para que nadie apoye el plazo en los despliegues. Y citar que lo que confirma que Coolify respeta el bloque `logging` es el `docker inspect` de `LogConfig` de RF-C18.
  5. **(No bloqueante) Trabajo 6.** Si se crea `scripts/check-csp.mjs` en vez de extender `check-kit-config.mjs`, agregarlo a `check-web` en el `justfile` y a la lista de `just check` en `CLAUDE.md`, en el mismo PR.

### security-reviewer — 2026-10-26 — REQUEST_CHANGES
- Verifiqué:
  - **Línea base del server.** Coincide con el código. En `telemetry.rs`: `ALLOWED_FIELDS` (los 12 campos de A3), `CAPPED_TARGETS` combinado con `and`, externos reducidos a `ts`/`level`/`target`/`event:"external"`, panic hook con solo `file`/`line`, `serde_json` y `level` en mayúsculas (lo prueban los tests). En `request_log.rs`: `method_code` cerrado, `MatchedPath` o `unmatched`, `request_id` UUID v4, y no lee headers, URI ni cuerpo. En `http/mod.rs`, los 5 + 1 headers sensibles, más `nosniff` y HSTS. `db::error_kind` y `bind_error_kind` devuelven `&'static str`, y ningún `tracing::` del crate usa `%err`/`?err`. `apps/server/tests/logs.rs` corre con `info`, `debug` y `debug,sqlx=trace,tungstenite=trace`, con los canarios que dice A8 y la contraseña de la BD. clippy deniega `print_stdout`/`print_stderr` en `Cargo.toml`.
  - **Línea base de `web`.** `log.ts` tiene hoy `error_message` en la allowlist y lo sanea junto con `stack`. `HEADER_NAMES` de `sanitize.ts` no incluye `x-real-ip`, `forwarded` ni `x-client-*` (lo suma TA-11). `handle-error.ts` solo registra `kind: 'unknown'` y su `readError` duplica `errorFields`. `process-handlers.ts` coincide. `security-headers.ts` tiene las tres cabeceras. `no-console` aplica fuera de `log.ts`, y el espejo `e2e/support/logs.ts` coincide con la allowlist. La CSP de B1 es literal la de `vite.config.ts`, y `check-compose.mjs` fija `RUST_LOG: info` exacto y una allowlist de variables de entorno por servicio.
  - **Coherencia.** No contradice ADR-0002 (A4.8 y A4.9 son más estrictos que su punto 2), ADR-0008 D3, D5, D7 y D8 (A4.1 a A4.3 y A4.13), ADR-0006 D1 y D11, ni ADR-0009 (sin orígenes externos). Coincide con la fila "Registros técnicos" del aviso, con su sección 8 (también la frase sobre `web`) y con RF-A19 y RF-B15. El cálculo de A7 coincide con "Pendiente" 8.
  - **Decisiones de seguridad que comparto.** Que el mensaje de un error no se registre nunca y que se recorte el encabezado del stack. Que la semilla no se registre tampoco después de `Settled`. Sin reportes de CSP. HSTS sin `includeSubDomains`. Que el server lleve `default-src 'none'`. `'wasm-unsafe-eval'` acotado a `script-src` y al PR que carga el `.wasm`. `preferWorkers: false` en vez de `worker-src blob:`. Nunca `wss:` suelto. No pude comprobar las afirmaciones sobre `pixi.js@8.22.0` porque el paquete no está instalado en el repo. Las acepto como condición del plan de 002, porque `csp.spec.ts` en la ruta del renderer las verifica.
- Cambios pedidos:
  1. **(Bloqueante) B3 no prohíbe un nonce ni un hash fijos en la configuración, y el chequeo del trabajo 6 es una lista negra.** *Escenario:* para depurar un script en línea, alguien agrega `'nonce-dev'` (o un `'sha256-…'`) a `script-src` en `vite.config.ts`. Ninguna regla de B3 lo prohíbe y el chequeo pasa. Después, un XSS reflejado inyecta `<script nonce="dev">…</script>`, que se ejecuta y lee el saldo o dispara apuestas con la sesión de la víctima. El nonce por petición de Kit deja de proteger. Pido:
     - (a) En B3, prohibir en cualquier directiva las fuentes `'nonce-…'` y `'sha256-'`/`'sha384-'`/`'sha512-'` escritas en la configuración (el nonce lo pone Kit), y también `'unsafe-allow-redirects'`, `'inline-speculation-rules'` y cualquier esquema que no sea `data:`/`blob:` en `img-src` (`filesystem:`, `mediastream:`, etc.).
     - (b) Que el trabajo 6 sea una **allowlist exacta**: las directivas y fuentes de B1, más solo las ampliaciones de B4 (`'wasm-unsafe-eval'` en `script-src` y `worker-src 'self'`). Cualquier otra directiva o fuente falla. Es la misma lógica de A3 ("lista negra ... falla abierta ante cualquier campo nuevo", en Alternativas) y es la única forma de hacer verificable B4.5 ("cualquier otra ampliación requiere un ADR").
  2. **(Bloqueante) A4.13 no se puede cumplir tal como está escrito.** Prohíbe "cualquier otro valor que, solo o cruzado con la BD, apunte a una persona". Pero `ts` (con hasta 9 decimales), `route`, `method` y `status`, que el contrato permite, cruzados con `bets.created_at` o `auth_events.created_at` sí apuntan a una persona. *Escenario:* `{"ts":"…T20:14:03.512347Z","method":"POST","route":"/api/bets","status":201}` se cruza con la fila de `bets` creada en ese milisegundo y da el `user_id`. Según A4.13, la línea base ya viola el contrato. Y como A6 decide con ese criterio si un campo nuevo necesita ADR y OK del owner, la puerta queda sin un criterio aplicable. Pido reformular el criterio en función del **valor del campo**: un valor que es o deriva de una clave de una fila de persona, sesión o dispositivo (ids, hashes, alias, tokens). Y declarar como riesgo residual aceptado la correlación por tiempo entre `ts` y la BD (quien tiene la BD ya tiene esos datos), con la opción de limitar `ts` a milisegundos en el server si se quiere acotar.
  3. **(Bloqueante) El contrato no cubre los spans ni otras capas del subscriber, y por ahí puede salir la semilla.** Hoy el formateador nunca escribe campos de span (doc de `telemetry.rs`), pero el ADR no lo dice, y "un solo escritor" (A3) no prohíbe agregar otra `Layer`. *Escenario:* la feature de peleas pone `#[tracing::instrument]` en `settle(fight: &Fight)`, que registra el `Debug` de `fight` con su `seed` en el span. Es inofensivo mientras solo exista `JsonLine`. Para diagnosticar la CPU del VPS, alguien agrega `console-subscriber` (tokio-console), una capa de OpenTelemetry o un segundo `fmt::layer()`, y la semilla de una pelea abierta sale por esa capa (tokio-console la sirve por gRPC en `:6669`) antes de `Settled`. Pido agregar a A3: (a) "los campos de span nunca se escriben"; (b) "`telemetry::layer` es la única capa del subscriber que produce salida; agregar otra (fmt, OTel, `console-subscriber`, `tracing-appender`) requiere reemplazar este ADR"; (c) `#[instrument]` siempre con `skip_all`, y un test en `telemetry.rs` que compruebe que un campo de span con un canario no aparece en la línea.
  4. **(No bloqueante, pero es una afirmación inexacta) B4.4: "Lo comprueba el e2e de `/dev/status` en WebKit".** Ese e2e está fuera de la CI (`testIgnore` en `playwright.config.ts` y solo con `playwright.dev.config.ts` contra `just dev`) y corre sobre `http`/`ws://`. No prueba que `'self'` cubra `wss:` en una página `https`, que es lo que importa en producción. Falla cerrado, así que no es una fuga, pero el texto debe decir que es un e2e opcional sobre `ws://`, y que la primera feature que use el WebSocket en producción agrega a la CI (o a los prerrequisitos de RF-C18) una comprobación sobre `https`/`wss` en WebKit.
  5. **(No bloqueante) A3, tipos por campo.** Agregar `file` a la lista de campos de cadena, y decir que todo método del visitor que no esté listado (`record_bool`, `record_f64`, `record_i128`/`u128`, `record_bytes`, `record_error`) descarta el campo. Hoy `record_error` y `record_bytes` caen en `record_debug` por defecto y escribirían el `Debug` en cualquier campo permitido. El trabajo 4 lo cierra solo si se dice explícitamente.
  6. **(No bloqueante) A5, `stack`.** (a) Decir que, si no hay ninguna línea de marco (`^\s+at `), el `stack` no se registra. Eso cubre un objeto lanzado con un `stack` arbitrario y `Error.stackTraceLimit = 0` en una dependencia. (b) En el trabajo 1, que el canario de `name` lleve un carácter fuera de `^[A-Za-z_$][A-Za-z0-9_$]{0,63}$` (por ejemplo `-`) y que el test compruebe que `error_name` vale `other`. Con un canario que sea un identificador válido, el test registraría el canario a propósito.
  7. **(No bloqueante) Otras salidas de diagnóstico de Node.** A2 prohíbe `debug` en producción para el server, pero no dice nada de `web`. `NODE_DEBUG=http` escribe headers (incluida la cookie) a stderr sin pasar por `log.ts`, y lo mismo hacen `--inspect` y `--trace-*` en `NODE_OPTIONS`. Hoy lo impiden la allowlist de entorno y el `NODE_OPTIONS` exacto de `check-compose.mjs`. Pido citarlos en A2 como parte del contrato: producción no define `NODE_DEBUG`, `DEBUG` ni flags de diagnóstico, y no se agregan variables desde la UI de Coolify fuera del compose. También pido declarar los `process.emitWarning` de Node (texto fijo, a stderr) como una excepción conocida, igual que las dos de Kit y adapter-node.

### architect — 2026-10-26 — RESPUESTA
- devops 1: (a) alcance `web`, `server` y Postgres en Contexto, A4 y A7; (b) sección nueva A9 con los seis parámetros (verificados contra la doc de PostgreSQL 17, la versión de las imágenes; nota de que `log_connections` cambia en 18), `log_line_prefix` sin `%h`/`%r`, y trabajo 13 (compose local y dev, `POSTGRES_COMMAND` y comparación en `check-compose.mjs`, Coolify); (c) `docker inspect` de `LogConfig` de Postgres y `SHOW` de los parámetros en trabajo 13 y en "Prerrequisitos de RF-C18 que este ADR suma", con el plan B de `daemon.json` en A7; (d) regla de validar o parsear todo valor del cliente antes de SQL, y sin interpolar valores en `RAISE` propios, en A9. Línea base con la fila de Postgres, y Alternativas con "apagar los logs de Postgres".
- devops 2: B6.4 lista todos los sitios que verifican una cabecera; trabajos 7 y 8 suman `expected` de `parseSecurityHeaders` y el paso del job `images`; los `curl -I` nuevos (incluido `/api/health`) van en "Prerrequisitos de RF-C18 que este ADR suma".
- devops 3: adoptado como trabajo 11 (paso de cabeceras en el job `verify` de `release.yml`); "Vigilar" y Alternativas lo reflejan.
- devops 4: A7 reescrito: los logs pueden borrarse antes al recrear el contenedor, "Restart" y `unless-stopped` los conservan, el máximo no depende de los despliegues, y el `docker inspect` de `LogConfig` de RF-C18 es lo que confirma el bloque `logging`.
- devops 5: trabajo 6 elige extender `check-kit-config.mjs` (ya está en `check-web`); si el plan crea `check-csp.mjs`, lo agrega al `justfile` y a `CLAUDE.md` en el mismo PR.
- security 1: B3 prohíbe `'nonce-…'` y hashes escritos en la configuración, `'unsafe-allow-redirects'`, `'inline-speculation-rules'` y esquemas de `img-src` distintos de `data:`/`blob:`, y fija la regla de allowlist exacta (B1 más B4); trabajo 6 reescrito como allowlist exacta con un test por prohibición y por ampliación.
- security 2: A4.13 redefinido por el valor del campo (es o deriva de una clave o atributo de persona, sesión o dispositivo), con la correlación por tiempo como riesgo residual aceptado y la razón para no recortar `ts`; A6 usa ese criterio en sus dos categorías.
- security 3: A3 suma "Spans y capas del server" (campos de span nunca se escriben, `telemetry::layer` única capa con salida y otra requiere ADR nuevo, `#[instrument]` siempre con `skip_all`); trabajo 12 con el test del canario en campo de span y el chequeo de `skip_all`; Alternativas con las capas de diagnóstico.
- security 4: B4.4 dice que el e2e de `/dev/status` es opcional, fuera de la CI y sobre `ws://`, y que la primera feature con WebSocket en producción agrega la prueba `https`/`wss` en WebKit (trabajo 14).
- security 5: A3 agrega `file` a los campos de cadena, lista los numéricos y exige implementar todo método del visitor, descartando el campo en los no listados; trabajo 4 con tests de `record_error` y `record_bytes`.
- security 6: A5 "Sin marcos no hay `stack`"; trabajo 1 con el canario de `name` fuera de la regex comprobando `error_name: other`, y unitarios sin marcos.
- security 7: A2 prohíbe en producción `NODE_DEBUG`, `DEBUG` y flags de diagnóstico en `NODE_OPTIONS`, cita la allowlist de entorno de `check-compose.mjs` y que no se agregan variables desde la UI de Coolify; A3 declara `process.emitWarning` como tercera excepción conocida.

### security-reviewer — 2026-10-26 (2ª ronda) — APPROVE
- Verifiqué en el cuerpo, no solo en la respuesta, que mis siete puntos quedaron resueltos:
  1. B3 prohíbe `'nonce-…'` y hashes escritos en la configuración, `'unsafe-allow-redirects'`, `'inline-speculation-rules'` y los esquemas de `img-src` que no sean `data:`/`blob:`. "La regla que lo resume" fija la allowlist exacta (B1 más B4), y el trabajo 6 la implementa con un test por prohibición y otro por ampliación. El escenario de `<script nonce="dev">` queda cerrado y B4.5 se puede verificar.
  2. A4.13 se mide por el valor del campo (es o deriva de una clave o un atributo de persona, sesión o dispositivo), la correlación por tiempo queda como riesgo residual aceptado con su razón, y A6 aplica ese criterio a sus dos categorías. La línea base ya no viola el contrato, y la decisión de si un campo necesita ADR tiene un criterio que se puede aplicar.
  3. A3, "Spans y capas del server": los campos de span nunca se escriben, `telemetry::layer` es la única capa con salida, cualquier otra requiere un ADR nuevo, y `#[instrument]` va siempre con `skip_all`. Lo cubren el trabajo 12 y las Alternativas. Contra el código: `JsonLine::format_event` ignora `_ctx` y solo recorre `event.record`, y `telemetry::init` registra solo `registry().with(layer)`. Hoy no hay ningún `#[instrument]` en `apps/server/src` ni en `crates/`.
  4. B4.4 dice que el e2e de `/dev/status` es opcional, que corre fuera de la CI (`testIgnore: ['dev-status.spec.ts']`) y sobre `ws://`, y el trabajo 14 deja la prueba sobre `https`/`wss` en WebKit como condición.
  5. A3, tipos por campo: lista `file` entre los de cadena y los numéricos, y exige implementar todos los métodos de `Visit` y descartar el campo en los no listados. El trabajo 4 prueba `record_error` y `record_bytes`. Lo que hace hoy el código coincide con lo que el ADR dice que se corrige: `record_bool` escribe el valor y `record_error`/`record_bytes` caen en `record_debug`.
  6. A5, "Sin marcos no hay `stack`", y el trabajo 1 con el canario de `name` fuera de la regex, que comprueba `error_name: other`, más los unitarios sin marcos.
  7. A2 prohíbe `NODE_DEBUG`, `DEBUG` y los flags de diagnóstico en `NODE_OPTIONS`, y no permite variables agregadas desde la UI de Coolify. A3 declara `emitWarning` como excepción y lo trata como fuga si lleva datos. Contra el código: `ENV.web` en `check-compose.mjs` fija `NODE_OPTIONS: '--max-old-space-size=256'` y la lista cerrada de variables.
- Secciones nuevas:
  - **A9.** Con `log_error_verbosity=terse` desaparecen el `DETAIL` de unicidad y el `Failing row contains`, y también el `DETAIL` de un deadlock, que trae las sentencias de los dos procesos. Con `log_min_error_statement=panic` desaparece la línea `STATEMENT`, y con eso `log_parameter_max_length_on_error` deja de importar. La regla de parsear antes de SQL y de usar `RAISE` con código fijo cubre el mensaje primario. Las imágenes son `postgres:17.10-alpine` en los dos compose (y `postgres:17-alpine` en Coolify según ADR-0006), así que la referencia a la doc de PostgreSQL 17 es correcta. Hoy `compose.dev.yml` no tiene `command` y `POSTGRES_COMMAND` solo trae el tuning, como dice la línea base.
  - **Spans y capas.** No introducen problemas. Un `#[instrument(skip_all, err)]` o `ret` emite un evento con los campos `error`/`return`, que no están en `ALLOWED_FIELDS`, así que se descartan.
  - **B3 ampliado.** No contradice B4: `'wasm-unsafe-eval'` y `worker-src 'self'` son las únicas diferencias que acepta la allowlist.
- Afirmaciones sobre el repo que comprobé: `apps/web/e2e/{csp,security,logs}.spec.ts` y `e2e/support/logs.ts` existen. El job `verify` de `release.yml` tiene `permissions: {}`, como dice el trabajo 11. El paso "Health, version and headers (RF-C7)" existe en `ci.yml`. `check-kit-config.mjs` ya lee `apps/web/vite.config.ts`. ADR-0006 deja Postgres sin puerto público.
- Notas no bloqueantes, para el plan o una enmienda menor:
  - A9: fijar también `log_transaction_sample_rate = 0` y `log_statement_sample_rate`/`log_lock_waits` en sus valores por defecto, y sumarlos al `SHOW` de RF-C18. *Escenario:* alguien sube `log_transaction_sample_rate` en Coolify para diagnosticar la latencia y, aunque `log_statement = none`, Postgres escribe todas las sentencias de las transacciones muestreadas. Hoy el riesgo es bajo, porque `sqlx` usa parámetros y el texto de la sentencia solo lleva `$1`.
  - A9: los mensajes `FATAL` de conexión (`no pg_hba.conf entry for host "…"`) llevan la IP en el mensaje primario, y `terse` no la quita. Es aceptable porque solo `server` se conecta por `torneo-db` y Postgres no tiene puerto público (ADR-0006). Conviene escribirlo en A9 como condición: si Postgres se expone algún día, esto se revisa.
  - B1 dice que `handle` no agrega "una segunda política", y B4.4 permite agregar el origen `wss://` desde `handle`. Aclarar en B4.4 que `handle` reescribe la cabecera que emite Kit y no agrega otra.
  - Trabajo 12: preferir el test que recorre `apps/server/src/` y busca `#[instrument` sin `skip_all` antes que una regla de revisión, porque el test no depende de que alguien se acuerde.

### devops-engineer — 2026-10-26 (2ª ronda) — APPROVE
- Verifiqué en el cuerpo, no solo en la respuesta, mis cinco puntos:
  1. Contexto ("Alcance de la parte A"), A4 ("Postgres incluido, A9") y A7 declaran `web`, `server` y Postgres. A9 trae los seis parámetros, `log_line_prefix` sin `%h`/`%r`, la regla de parsear antes de SQL y los `RAISE` con código fijo. El trabajo 13 cubre los dos compose, `POSTGRES_COMMAND`, la comparación de `compose.dev.yml`, Coolify y `docs/architecture.md`. El `docker inspect` de `LogConfig` y el `SHOW` están en el trabajo 13 y en "Prerrequisitos de RF-C18 que este ADR suma". La línea base tiene su fila de Postgres.
  2. Los trabajos 7 y 8 nombran la lista `expected` de `parseSecurityHeaders`, el paso "Health, version and headers (RF-C7)" del job `images` (COOP y `Permissions-Policy` en `/es/`; CORP y la CSP del server en `/api/health` y en una 404) y los `curl -I` de RF-C18, `/api/health` incluido. B6.4 reúne todos los sitios.
  3. El trabajo 11 es el paso de cabeceras en `verify`. "Vigilar" y Alternativas lo reflejan.
  4. A7 dice que los logs "pueden borrarse antes" al recrear el contenedor, que "Restart" y `unless-stopped` los conservan y que el máximo no depende de los despliegues. Cita el `docker inspect` de `LogConfig` de RF-C18 como la confirmación de que Coolify respeta el bloque.
  5. El trabajo 6 extiende `check-kit-config.mjs`, que ya está en `check-web`. Si el plan crea `check-csp.mjs`, el PR lo agrega al `justfile` y a `CLAUDE.md`.
- **A9 comprobado con la imagen fijada** (`postgres:17.10-alpine@sha256:742f40ea…`, PostgreSQL 17.10, en Colima):
  - `postgres -C` acepta los seis valores y devuelve `terse`, `panic`, `none`, `-1`, `off` y `off`. Los valores por defecto son `log_line_prefix = '%m [%p] '` y `log_hostname = off`, como dice A9. Un valor inválido hace que el proceso no arranque (`FATAL: parameter "log_connections" requires a Boolean value`). Así, un error de tipeo en el `command` lo detecta el healthcheck al instante y no deja a Postgres con los valores por defecto.
  - Con los valores por defecto, una violación de unicidad escribe `DETAIL: Key (alias)=(CANARY_ALIAS) already exists.` más `STATEMENT: insert …`, y una de `CHECK`, `DETAIL: Failing row contains (3, CANARY_ROW, 0).` más su `STATEMENT`. Con los seis parámetros de A9 quedan solo las líneas `ERROR` primarias (`duplicate key value violates unique constraint "t_alias_key"`, `violates check constraint`, `violates not-null constraint`), sin canario. `invalid input syntax for type uuid: "CANARY_UUID"` sí sale, que es justo el caso que A9 cubre con la regla de parsear antes de SQL.
  - El entrypoint de la imagen pasa los `-c` del `command` sin cambios: `pg_settings.source` vale `command line`. Por eso añadir un `command` a `compose.dev.yml` (y extender el de `compose.local.yml`) funciona igual que el tuning actual. Además, la línea de comandos tiene prioridad sobre `postgresql.auto.conf`, así que un `ALTER SYSTEM` en local no los pisa.
- **Mecanismo de Coolify**: viable. La "Custom PostgreSQL Configuration" es el mismo campo donde va el tuning de ADR-0006 D4. Coolify la monta como `postgresql.conf` y arranca con `config_file=` apuntando a ella. Como ese archivo reemplaza la configuración de la imagen en vez de sumarse a ella, el `SHOW` de RF-C18 es el control correcto: no da por hecho que Coolify la aplicó.
- **ADR-0006**: sin contradicciones. D1 (3 × 10 MB en `web` y `server`) sigue igual y A7 solo agrega Postgres. A9 usa el mismo lugar que D4. D11 sigue vigente para Traefik. El trabajo 11 agrega unos segundos a un job que ya existe, dentro de sus 10 min (como mucho 5 min de sondeo más cuatro `curl`), y no cambia los disparadores, los permisos (`permissions: {}`) ni la aprobación de `production`. También corre en un rollback por `workflow_dispatch`, lo que es deseable.
- Notas no bloqueantes, para el issue de Postgres (trabajo 13) o el plan del issue de CSP y cabeceras (sin repetir las de security-reviewer):
  - **Plan B de `daemon.json` (A7)**: es correcto que valga solo para los contenedores creados después y que haya que recrear el recurso, pero faltan tres detalles operativos que el trabajo 13 debe documentar en `docs/architecture.md`. (a) Los valores de `log-opts` van como cadenas (`{"log-driver": "json-file", "log-opts": {"max-size": "10m", "max-file": "3"}}`), porque con `"max-file": 3` `dockerd` no arranca. (b) Se fusiona con el `daemon.json` que ya exista (el instalador de Coolify suele escribir uno, a menudo con estos mismos valores), sin reemplazarlo. (c) `log-opts` no se recarga con `SIGHUP`, así que hay que reiniciar `dockerd`, lo que reinicia todos los contenedores del VPS compartido, Coolify incluido. Eso pide una ventana y el OK del owner, y antes conviene validar el archivo (`dockerd --validate --config-file /etc/docker/daemon.json`).
  - **Trabajo 11**: `/healthz/` responde con la 308 de Kit, así que `curl -sI` sin `-L` comprueba justo la respuesta que solo pasa por Traefik (bien). Conviene que el paso también exija el código de estado esperado de cada URL (200, 200, 308, 200), para que una 404 o una 502 de Traefik, que salen sin nuestras cabeceras, den un error claro en vez de "falta la cabecera".
  - **`SHOW` de RF-C18**: usar `SELECT name, setting, source FROM pg_settings WHERE name IN (…)`, que en un solo comando muestra el valor y de dónde sale (`configuration file`).

### owner — 2026-10-26 — APPROVE
- Aprobado por el owner en la sesión de trabajo: "Como owner apruebo el ADR-0012."

### architect — 2026-10-30 — ENMIENDA (`form-action`, pregunta abierta 1 de plan-d de 008)
- Agregué la "Enmienda 2026-10-30: `form-action` para el login con Google y Discord". Enmienda B3, B4.5 y B6 y agrega B4.6. No reescribí las secciones originales ni las revisiones: puse una línea en cursiva en cada punto afectado que remite a la enmienda.
- Me aparto de lo que anuncié en la revisión de plan-d, que eran los dos orígenes a secas, y pongo las fuentes con la ruta del endpoint de autorización. Los orígenes admitidos son los mismos, y la ruta no frena el 303, porque la ruta de una fuente no se compara tras una redirección. Sí cierra el post directo a un webhook de Discord.
- `status` sigue en `approved`, y `approvals` no cambia hasta que firmen los revisores. Pido revisión a security-reviewer (superficie de la CSP y el caso del webhook) y a devops-engineer (las cadenas `b1` de `images` y `verify`). El gate del owner lo cubre su delegación del 2026-10-29 (spec de 008, "Revisión → owner").

### security-reviewer — 2026-10-30 — APPROVE (Enmienda 2026-10-30: `form-action` para el login con Google y Discord)
**🤖 security-reviewer**

- Verifiqué:
  - El `git diff` de la enmienda y las cuatro líneas en cursiva de B3, B4.5 y B6, contra B1, B3, B4, B5 y B6 originales y contra la sección "Preguntas abiertas → 1" de `plan-d.md`.
  - `apps/server/src/auth/providers.rs`: `GOOGLE_AUTHORIZE = "https://accounts.google.com/o/oauth2/v2/auth"` y `DISCORD_AUTHORIZE = "https://discord.com/oauth2/authorize"`. Las fuentes de B4.6 coinciden carácter por carácter. Las dos van sin barra final, así que CSP compara la ruta **exacta**, y la query no entra en la comparación. `TOKEN` y `USERINFO` son peticiones del server, no navegaciones del navegador, y bien quedan fuera.
  - **La afirmación de CSP3.** Es correcta. En "Does url match expression in origin with redirect count?", el `path-part` de una `host-source` solo se compara si `redirect count` es 0. Es la regla de CSP2 ("Paths and Redirects"), que existe para que una política no filtre la ruta de destino de una redirección cross-origin. El efecto es el que describe la enmienda: el 303 de `start` y `link` se compara solo por origen y pasa, y un `action` directo a `https://discord.com/api/webhooks/…` se compara con ruta y se bloquea. Lo verifiqué contra lo que sé del texto de la spec y de las implementaciones, no contra una copia en línea (en esta sesión no tengo red). Por eso la evidencia que manda es el e2e.
  - **Por navegador.**
    - Chromium aplica `form-action` a cada redirección de la navegación que sigue al envío, con la ruta ignorada tras la primera. Es justo lo que rompe el login con B1.
    - WebKit compara las fuentes con un flag de "recibió redirección" que salta la ruta (`ContentSecurityPolicySource::matches`), y su chequeo de `form-action` recibe ese flag. Espero el mismo comportamiento que en Chromium. Si no lo aplicara a redirecciones, el positivo pasaría igual.
    - Firefox no está en la matriz del e2e. Históricamente no aplica `form-action` a las redirecciones (MDN lo documenta como inconsistencia entre navegadores). Ahí la enmienda no cambia el positivo, y el negativo sigue bloqueado, porque un `action` directo siempre se compara con ruta (redirect count 0) en los tres motores.
  - **El caso negativo** (plan-d, paso 7). Es la prueba que importa: un webhook de Discord es un canal de exfiltración que controla quien lo crea, y un 307/308 conservaría el cuerpo. Que no sea `fixme` y que pase también con B1 lo convierte en una guarda de regresión: si alguien relaja B4.6 al origen sin ruta, se pone rojo.
  - **El UT que fija los literales**, con el comentario que remite a la enmienda, más el positivo de plan-d (paso 5) con los dos proveedores. Entre los dos cubren la deriva entre `providers.rs` y la CSP, y la deriva falla cerrada (el login se bloquea, nada se abre).
  - **"Global".** Un formulario inyectado hacia el endpoint de autorización no agrega superficie frente a un `<a href>` inyectado, que CSP nunca restringió: los dos llevan al usuario a la pantalla del proveedor con parámetros del atacante. La CSP de nonce sigue impidiendo el script, así que hace falta un clic del usuario.
  - **B3, B4.5 y B6.** "Una enmienda con las firmas de sus revisores y el gate del owner" no abre una vía más laxa que un ADR nuevo: las firmas son las mismas. La lista de B6.4 incluye la allowlist exacta de `check-kit-config` con los casos negativos correctos (origen sin ruta, otra ruta, otro host, `https:`, comodín y una fuente de B4.6 en otra directiva), y las dos cadenas `b1` de CI y del release.
  - El frontmatter (`updated: 2026-10-30`). Sigo el precedente de ADR-0008 (enmienda del 2026-10-22): la firma de una enmienda queda en esta sección y `approvals` no cambia.
- Hallazgos de seguridad confirmados: ninguno.
- No bloquean (recomendados en el PR de la fase D o en esta enmienda):
  1. **Una redirección propia reabre el canal.** Como la ruta se ignora tras una redirección, cualquier ruta de nuestro origen que redirija a una URL de `discord.com` o `accounts.google.com` distinta de las constantes deja pasar un formulario inyectado que postee a esa ruta. Si responde 307 o 308, el cuerpo llega al webhook.
     - *Escenario:* una feature futura agrega `/api/…/go?to=` con 307, y un formulario inyectado postea a `/api/…/go?to=https://discord.com/api/webhooks/<id>/<token>`. `'self'` deja pasar el envío, `https://discord.com/oauth2/authorize` deja pasar la redirección por origen, y el webhook recibe los campos del formulario.
     - Hoy no pasa: `start` y `link` usan las constantes, y `next` lo valida el regex de D2 como ruta relativa.
     - *Cambio:* una línea en B4.6, "ninguna ruta de `server` ni de `web` redirige a esos orígenes salvo a las constantes de `providers.rs`, y nunca con 307 o 308".
  2. **"Ninguno de los dos recibe datos nuestros" es casi cierto.** Con una inyección de HTML y un `<form>` sin cerrar (form hijacking), los campos legítimos de la página viajan al proveedor. Google y Discord solo reenvían `state` al `redirect_uri` del cliente del atacante, y lo hacen solo tras el clic del usuario en la pantalla de consentimiento, porque si la redirección siguiera la misma cadena la bloquearía `form-action`.
     - Hoy ningún formulario de `apps/web/src` tiene un campo `state` ni nada secreto.
     - *Cambio:* precisar la frase, y como regla de UI "ningún formulario nuestro lleva un campo `state`, `client_id` ni `redirect_uri`".
  3. **La sección "Prueba" del ADR nombra solo Google.** Plan-d (paso 5) prueba los dos, y "el e2e lo detecta" depende de eso. *Cambio:* nombrar `CONNECT discord.com:443` también en la enmienda.
  4. **La deriva se ve antes en `just check` que en el e2e.** `check-kit-config.mjs` puede leer `GOOGLE_AUTHORIZE` y `DISCORD_AUTHORIZE` de `providers.rs` y compararlos con `CSP_EXTENSIONS`. Es barato y no depende de que corra el job `e2e`.
  5. **Para la prueba manual de TA-6.** Una cuenta de Google Workspace con SSO SAML salta, en la misma cadena, al IdP de su organización. Con B4.6, Chromium la bloquea (falla cerrada). Hay que anotarlo en el issue como caso conocido, para no "arreglarlo" con un comodín. La regla de B4.6 ya lo cubre: otro host es otra enmienda.

### devops-engineer — 2026-10-30 — REQUEST_CHANGES (Enmienda 2026-10-30: `form-action` para el login con Google y Discord)
**🤖 devops-engineer**

- Verifiqué:
  - La decisión de B4.6 me parece bien, y no la discuto: las tres fuentes, solo en `form-action`, con la ruta tal como está en `providers.rs`. Lo que pido es sobre B6.4, sobre si la lista está completa y si se puede comprobar.
  - **Todas las copias de la CSP de `web` en el repo** (busqué `form-action` y `frame-ancestors` fuera de `docs/`): `apps/web/vite.config.ts:35`, `CSP_BASE` y `CSP_EXTENSIONS` de `scripts/check-kit-config.mjs`, sus tests (`scripts/__tests__/check-kit-config.test.mjs`) y el fixture `scripts/__fixtures__/check-kit-config/valid/vite.config.ts`, `b1` en `.github/workflows/ci.yml:467` (job `images`) y en `.github/workflows/release.yml:262` (job `verify`), y `CSP_B1` en `apps/web/e2e/security.spec.ts:22`. `apps/server` solo tiene su propia CSP (`default-src 'none'; frame-ancestors 'none'`), que no cambia. La lista de B6.4 enmendado cubre todas las copias. El fixture válido puede seguir con B1 a secas, porque las ampliaciones de B4 son opcionales.
  - **Qué comprueba cada copia, y cuándo:**
    - `check-kit-config` (en `just check` y en `web`) compara `vite.config.ts` con `CSP_BASE` y `CSP_EXTENSIONS`. Es un **techo**, no una igualdad, porque `extraSources` es opcional: un `vite.config.ts` con solo la fuente de Google también pasa. Lo de "exactamente las tres fuentes" lo garantizan los dos chequeos que siguen.
    - El `b1` de `ci.yml` se compara con la cabecera que emite la imagen. `apps/web/vite.config.ts` está en el filtro `images`, así que el job corre en el PR que cambia la CSP, y si el `b1` no coincide se pone rojo antes del merge.
    - `CSP_B1` de `security.spec.ts` se compara con lo que Kit emite, en el job `e2e` del mismo PR.
    - **El `b1` de `release.yml` no lo compara nada antes del deploy.** El job `verify` corre después de que `deploy` se aprobó y se ejecutó. Ni `check-workflows` ni `check-kit-config` leen esa cadena. Si el PR de la fase D se olvida de `release.yml` (o si un PR posterior edita solo una de las dos cadenas `b1`), el problema aparece como un `verify` rojo con producción ya actualizada. Falla cerrado, así que no abre nada, pero obliga a otro tag y otro deploy para corregir un error que se podía detectar en el PR.
  - **El punto 6** ("los `curl -I` de RF-C18 y del issue 'Primer despliegue', si comparan la CSP completa"). Lo resolví: esos prerrequisitos están en `docs/architecture.md` ("Prerrequisitos del primer despliegue a producción", punto 2), y el issue es TA-8 en Linear, no un issue de GitHub. Ninguno compara la CSP de `web`: solo la del server en `/api/health`, que no cambia. Hoy el punto 6 no cambia nada, pero como está escrito, con un "si", no hay forma de comprobarlo.
  - **Los minutos de CI.** Sin cambios. No hay job nuevo: `images` y `e2e` ya corren en ese PR por el filtro de `vite.config.ts`, y el chequeo que pido abajo va en `policy`, que corre siempre y dura segundos.
  - Frontmatter: `updated: 2026-10-30`. Sigo el precedente de ADR-0008 y de security-reviewer: `approvals` no cambia por una enmienda.
- Cambios pedidos (en la enmienda, B6 enmendado):
  1. **Agregar a la lista de B6.4 un punto 7, para que la cadena de `release.yml` quede comprobada en el PR.** `scripts/check-workflows.mjs` comprueba que el valor de `b1=` del paso "Health, version and headers (RF-C7)" de `images` en `ci.yml` y el del paso "Security headers (ADR-0012 B2)" de `verify` en `release.yml` existen, aparecen una sola vez cada uno y son **idénticos byte a byte**. Se prueba en `scripts/__tests__/check-workflows.test.mjs`, con un caso que falla si difieren y otro que falla si falta una de las dos. Como `images` ya compara la cadena de `ci.yml` con la imagen, por transitividad la de `release.yml` queda comprobada antes del merge, y en `policy`, que corre siempre. **[devops]** lo implemento en el commit del paso 14 de plan-d, junto con las cadenas `b1`. Es una regla general, no solo de B4.6: cualquier ampliación de B4 posterior (`wss://` de B4.4 incluida) queda cubierta.
  2. **Cambiar el punto 6 por un hecho comprobable:** "Los prerrequisitos de RF-C18 (`docs/architecture.md`, punto 2) y TA-8 no comparan la CSP de `web`, así que no cambian. Si un PR posterior agrega esa comparación, la cadena va idéntica a `b1` y entra en el chequeo del punto 7".
- No bloquean (para el PR de la fase D):
  1. Los comentarios que acompañan a las dos cadenas dicen "ADR-0012 B1 exactly" y "ADR-0012 B1 as Kit emits it". Hay que cambiarlos por "B1 plus the B4 extensions in use (B4.6)" y que el mensaje de error de `images` no diga "is not ADR-0012 B1" cuando lo esperado ya no es B1 a secas. El nombre de la variable `b1` puede quedar, porque el chequeo del punto 7 lo busca por ese nombre.
  2. Revisé `csp_norm` (en los dos workflows) y `effectiveCsp` (en `security.spec.ts`) con fuentes que llevan ruta: parten por espacios y `;`, y la regex del nonce solo toca `'nonce-…'`, así que las URLs pasan intactas. No hace falta tocar la normalización.
  3. Para el e2e con proxy: no tengo verificado que el `proxy` de Playwright salte loopback igual en WebKit que en Chromium. Conviene fijar `bypass` con `127.0.0.1` y `localhost` en los dos motores, y que el test compruebe que en el registro del proxy solo aparecen hosts del proveedor, para que la app nunca pase por él y el negativo ("el proxy no recibe nada") siga siendo cierto.

### security-reviewer — 2026-10-30 — APPROVE (confirmación del texto enmendado de la Enmienda 2026-10-30)
**🤖 security-reviewer**

- Verifiqué el texto de la enmienda en 120ab22 contra mi revisión anterior y contra 97053ae (`git diff 97053ae 120ab22`):
  - **Recomendación 1.** B4.6 prohíbe toda redirección propia de `server` o `web` a `accounts.google.com` o `discord.com` salvo a las constantes de `providers.rs`, y nunca con 307 ni 308. Lo fijan tres pruebas: un UT (orígenes solo en `providers.rs`, sin `Redirect::temporary` ni `Redirect::permanent` en `auth/`), un IT (estado 303 de `start` y `link`) y un Vitest sobre `src/**`. Cierra el escenario del `/go?to=` con 307 hacia un webhook.
  - **Recomendación 2.** La frase quedó precisada: con form hijacking los campos sí viajan, pero el proveedor no los reenvía sin consentimiento y solo al `redirect_uri` registrado. Además ningún formulario lleva `state`, `client_id`, `redirect_uri` ni un secreto, y lo fija un Vitest.
  - **Recomendación 3.** La prueba nombra `CONNECT accounts.google.com:443` y `CONNECT discord.com:443`, pone loopback en `bypass` en los dos motores y tiene un teardown que falla ante un host no declarado. Con eso, el negativo del webhook ("el proxy no recibe nada") sigue siendo cierto.
  - Los puntos 6 y 7 de devops-engineer en B6.4 no tocan la superficie de la CSP. El punto 7 la endurece: deja comprobada la cadena de `release.yml` antes del merge.
  - Las fuentes de B4.6 siguen idénticas a `GOOGLE_AUTHORIZE` y `DISCORD_AUTHORIZE` de `apps/server/src/auth/providers.rs`.
- Hallazgos de seguridad confirmados: ninguno. Mi APPROVE anterior se mantiene sobre este texto.
- No bloquean (para el PR de la fase D):
  1. El UT de "esos orígenes solo aparecen en `providers.rs`" tiene que excluir su propio módulo de test, o vivir en `providers.rs`, para no contarse a sí mismo.
  2. Cuando firmen los dos revisores, la línea en cursiva de B3 que dice "pendiente de las firmas" queda vieja. Conviene actualizarla en el PR que aplique la enmienda.

### devops-engineer — 2026-10-30 (2ª ronda) — APPROVE (Enmienda 2026-10-30: `form-action` para el login con Google y Discord)
**🤖 devops-engineer**

- Verifiqué:
  - **Cambio 1 (B6.4, punto 7).** Está completo y se puede comprobar. Los nombres de los pasos coinciden con los workflows: "Health, version and headers (RF-C7)" en `ci.yml:413` (job `images`, `b1=` en la línea 467) y "Security headers (ADR-0012 B2)" en `release.yml:245` (job `verify`, `b1=` en la línea 262). Hoy las dos cadenas son idénticas byte a byte, así que el chequeo nuevo pasa en `main` antes de que la fase D toque nada. Pide que cada una exista y aparezca una sola vez, la igualdad byte a byte, los dos casos de `scripts/__tests__/check-workflows.test.mjs` (el archivo existe) y que corra en `policy`. También deja escrito el argumento de transitividad con `images` y que la regla vale para cualquier ampliación de B4, `wss://` de B4.4 incluida.
  - **Cambio 2 (punto 6).** Ahora es un hecho, sin el "si". Lo comprobé en `docs/architecture.md:242`: el `curl -I` de RF-C18 solo mira `/api/health` y la CSP del server (`default-src 'none'; frame-ancestors 'none'`), que la enmienda no toca. Si alguien agrega después una comparación de la CSP de `web`, queda dentro del punto 7.
  - **Mis no bloqueantes.** El 1 (los comentarios y el mensaje de error de `images` dicen "B1 más las ampliaciones de B4 en uso (B4.6)") quedó en el punto 7. El 3 (`bypass` con loopback en los dos motores y un teardown que falla si el proxy registra un host no declarado) quedó en "Prueba de que funciona". El 2 no pedía cambios.
  - **Las recomendaciones 1 a 3 de security-reviewer**, desde lo que me toca a mí. Las tres guardas nuevas son un UT, un IT y un Vitest de web. Corren en `just check` y en los jobs `rust` y `web` que ya existen, sin job nuevo. Que la prueba nombre `CONNECT discord.com:443` no cambia el costo del e2e, porque el proxy es local y corta la conexión.
  - **Los minutos de CI.** Sin cambios respecto de la 1ª ronda: no hay job ni workflow nuevo, y lo que se agrega a `policy` dura segundos.
  - El frontmatter: `updated: 2026-10-30`. `approvals` no cambia, según el precedente de ADR-0008 que también sigue security-reviewer.
- No bloquea: la línea en cursiva de B3 todavía dice "pendiente de las firmas de security-reviewer y devops-engineer". Con esta firma y la de security-reviewer ya no está pendiente. Que architect le quite "pendiente" cuando toque el ADR (en el PR de la fase D, por ejemplo). No afecta a la decisión.
- **[devops]** Me encargo del punto 7 y de las dos cadenas `b1` en el commit del paso 14 de plan-d, como ya anuncié.
