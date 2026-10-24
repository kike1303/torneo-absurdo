---
id: "001"
title: Fundaciones del monorepo — fase B
type: plan
status: approved
risk: high
author: backend-engineer
reviewers: [architect, security-reviewer]
approvals:
  - by: architect
    date: 2026-10-23
  - by: security-reviewer
    date: 2026-10-23
  - by: owner
    date: 2026-10-23
linear: TA-1
depends_on: []
created: 2026-10-23
updated: 2026-10-23
---

# Plan: Fundaciones del monorepo — fase B (desde spec.md 2026-10-23)

> [intent](./intent.md) · [spec](./spec.md) · [plan-a](./plan-a.md) · ADRs [0001](../../decisions/0001-motor-propio-determinista.md), [0002](../../decisions/0002-commit-reveal-semilla.md), [0003](../../decisions/0003-stack-tecnologico.md), [0006](../../decisions/0006-despliegue-y-ci.md) (D2, D3, D8, D10), [0008](../../decisions/0008-autenticacion.md) (D3, D4 "sin CORS", D6 y D7 solo en lo que la spec aplica a B) · [Aviso de privacidad](../../legal/aviso-de-privacidad.md)

**Alcance:** RF-B1 a RF-B20 y los RNF que tocan al server (seguridad, logs sin datos del cliente, i18n por códigos, memoria). Rama `ta-1-fundaciones-fase-b`, que sale de `main` cuando TA-10 (nits de la fase A) ya esté fusionada. Un solo PR. La fase C (imágenes, compose de producción, CI) tiene su propio plan.

**Por qué security-reviewer:** este plan fija los logs del server y su plazo (aviso, "Pendiente" punto 8), el `/ws` sin auth que llegará a las imágenes de producción, la capa de solo lectura que reemplazará el middleware de ADR-0008 D4 y la configuración por variables de entorno (`DATABASE_URL` con contraseña).

**Bloqueo de inicio (revisión del architect, 2026-10-23):** las respuestas del architect a las preguntas de este plan cambian la spec; están en "Enmienda 2026-10-23 (b): fase B" de `spec.md` (`ws.rate_limited` en RF-B5, RF-B7 y RF-B13; `ApiError { error: ErrorCode }` en RF-B4 y RF-B5, con `rename` por variante; `BIND_ADDR` (antes `HOST`) en la tabla de variables y en RF-B11; orden `Origin` → upgrade → semáforo en RF-B13). Este plan está alineado con ese texto. security-reviewer ya aprobó la enmienda; falta que quede aprobada en el frontmatter de la spec. **El paso 2 no empieza hasta que esa enmienda esté aprobada**; los pasos 0, 1 y 4 no dependen de ella.

**Qué no toca:** `crates/engine`, `crates/engine-wasm`, `packages/renderer`, `content/`, `.github/`, `infra/docker/`. ADR-0001 y ADR-0002 no aplican a esta fase más allá de una regla: `crates/protocol` no tiene ningún tipo con semilla ni eventos de pelea, y nada de lo que se agrega usa aleatoriedad fuera de `uuid` v4 para el `request_id` (que no es el motor).

## Decisiones de implementación

Son decisiones que la spec deja a la implementación, o precisiones que el código obliga a tomar. Las que cambian un detalle literal de la spec ya tienen la respuesta del architect (decisiones 3, 7 y 10) y entran en su enmienda menor.

1. **`lib.rs` + `main.rs` delgado.** `apps/server` es un paquete con biblioteca (`server`) y binario (`server`). `main.rs` solo parsea el subcomando, llama a `server::run_cli()` y convierte el resultado en `ExitCode` con `exit_code(&err)`, **sin formatear el error** y sin devolver `Result` desde `main` (un `main` que devuelve `Err` imprime su `Debug` en stderr). No hay `anyhow`: la biblioteca y el binario usan errores con `thiserror` (nota 5 del architect). Así los tests de integración arman el router y el estado en el proceso, y los que necesitan el proceso real (señales, código de salida, stdout) usan `env!("CARGO_BIN_EXE_server")`.
2. **Subcomandos sin `clap`.** `server` o `server serve` arranca; `server healthcheck` hace la comprobación de RF-B11; cualquier otro argumento sale con código 2 y una línea `{"event":"cli.invalid"}`. Son dos subcomandos y ninguna opción: una dependencia no aporta nada.
3. **Configuración (RF-B8)** en `config.rs`, leída de una vez con `std::env::var_os` en una función pura `Config::from_lookup(impl Fn(&str) -> Option<OsString>)`, para probarla con una tabla sin tocar el entorno del proceso. Si falta una obligatoria o es inválida, el error lleva **solo el nombre** de la variable (`ConfigError::Invalid { var: &'static str }`); `main` registra `{"event":"config.invalid","var":"PUBLIC_ORIGIN"}` y sale con 1. El valor nunca se formatea: el tipo de error no lo contiene.
   - `PUBLIC_ORIGIN`: misma regla que el validador de `web` (decisión 19 de `plan-a.md`). Se parsea con `url::Url` y es válido solo si el esquema es `http` o `https` **y** `url.origin().ascii_serialization() == valor`. Eso rechaza ruta, query, fragmento, credenciales, barra final, mayúsculas en el host y puerto por defecto (`:443`, `:80`). El guard de `Origin` del WS compara cadenas exactas, así que el valor tiene que estar en la forma en que el navegador serializa `Origin`.
   - `PORT`: `u16`, por defecto `8080`. Se acepta `0` (puerto efímero) solo porque lo usan los tests del binario; el puerto real se registra en `server.listening`.
   - `WS_MAX_CONNECTIONS`: entero en `1..=65535`, por defecto `2000`.
   - `RUST_LOG`: por defecto `info`; si no se puede parsear como filtro, es `config.invalid` (mejor fallar que quedar con un nivel inesperado).
   - `ENABLE_WS_ECHO`: ver decisión 9.
   - **`BIND_ADDR`** (aceptada por el architect en la enmienda b de la spec; se llamaba `HOST` y se renombró en la ronda 2 porque zsh define `HOST` con el nombre de la máquina y `web` ya usa `HOST` en adapter-node, que no se toca). Opcional, se parsea como `std::net::IpAddr` (`config.invalid` si no lo es), por defecto `0.0.0.0` (lo que necesita el contenedor). `apps/server/.env.example` la fija en `127.0.0.1`, para que `cargo run` no publique el puerto 8080 en la red local del desarrollador (el criterio del RNF "en local, todo puerto publicado se ata a `127.0.0.1`"). Dos condiciones del architect: (a) `server healthcheck` siempre llama a `127.0.0.1`, así que **supone `BIND_ADDR` = `0.0.0.0` o `127.0.0.1`**; con otra IP (o `::1`) el healthcheck falla, y así lo dice el comentario de la variable en `config.rs` y en `.env.example`; (b) no entra al compose de producción: `check-compose.mjs` la prohíbe en `server` (lo implementa la fase C, y lo anoto para su plan).
4. **Formato de log propio** (`telemetry.rs`). Ni `tower_http::trace::TraceLayer` (su span por defecto lleva la URI completa) ni el formateador JSON de `tracing-subscriber` (escribe `timestamp`, anida `fields`, agrega `target` y `message`, y no permite una lista blanca). Se implementa un `FormatEvent` que escribe una línea JSON por evento con:
   - `ts` (RFC 3339 en UTC, con el `SystemTime` de `tracing-subscriber`) y `level` siempre;
   - solo los campos de una **lista blanca cerrada**: `event`, `method`, `route`, `status`, `latency_ms`, `request_id`, `var`, `port`, `error_kind`, `close_code`, `file` y `line`. Eventos de arranque: `server.listening` (con `port`) y `server.bind_failed` (con `error_kind`, nunca la dirección). Cualquier otro campo, incluido `message`, se descarta sin escribirse. `file` y `line` solo los escribe el *panic hook*, con valores tomados **únicamente** de `std::panic::Location` (`location.file()` y `location.line()`), nunca del payload del pánico;
   - un evento es "nuestro" solo si `target == "server" || target.starts_with("server::")` (un crate llamado `server_x` no cuenta). Para el resto, solo `ts`, `level`, `target` y `"event":"external"`. Así un `debug` o `trace` de sqlx, hyper o tungstenite nunca vuelca texto con datos;
   - **no recorre spans**: no escribe campos del span actual ni de sus padres, solo los del evento. Ninguna capa del server abre spans con campos;
   - todo valor se escribe con `serde_json` (`serde_json::to_writer` sobre un `Map`), así que comillas, saltos de línea y caracteres de control salen escapados y ningún valor puede partir la línea ni inyectar campos;
   - los valores de tipo código son `&'static str` de un conjunto cerrado: `error_kind` (`"io"`, `"tls"`, `"protocol"`, `"timeout"`, `"pool"`, `"other"`, mapeados por `match` desde `sqlx::Error`) y `method` (decisión 5). Nunca un `String` que venga de la petición o de un error.
   Además, el filtro siempre agrega topes `sqlx=warn`, `hyper=warn`, `hyper_util=warn`, `tungstenite=warn`, `tokio_tungstenite=warn` y `h2=warn` **después** de las directivas de `RUST_LOG`, para que ganen aunque `RUST_LOG` nombre esos crates (un unit con `RUST_LOG=sqlx=trace,hyper=trace` comprueba que un evento `debug` de `sqlx::query` no pasa el filtro; nota 7 del architect). Y un *panic hook* que registra solo `{"event":"panic","file":…,"line":…}`, sin el mensaje del pánico. Los canarios del paso 11 prueban las capas juntas.
5. **Línea de petición (RF-B15).** Un `middleware::from_fn` aplicado con `Router::layer` (cubre las rutas, el *fallback* y los 405) registra al terminar, en `info`: `event: "http.request"`, `method` (de un conjunto cerrado: `GET`, `HEAD`, `OPTIONS`, `POST`, `PUT`, `PATCH`, `DELETE` u `other` para cualquier método de extensión; nunca el texto del método que mandó el cliente), `route` (la plantilla de `MatchedPath`, o `unmatched`), `status`, `latency_ms` (entero) y `request_id` (`Uuid::new_v4()`). No lee headers, URI, query ni cuerpo. `/api/health` se registra como cualquier otra (RF-B16).
6. **Orden de capas del router** (de fuera hacia dentro, con `ServiceBuilder` dentro de `Router::layer`):
   1. `SetSensitiveRequestHeadersLayer` (`Cookie`, `Authorization`, `X-Internal-Secret`, `X-Client-Ip`, `X-Forwarded-For`);
   2. línea de petición (decisión 5), así también se registran los 405;
   3. `SetSensitiveResponseHeadersLayer` (`Set-Cookie`);
   4. `SetResponseHeaderLayer::overriding` con `X-Content-Type-Options: nosniff` y `Strict-Transport-Security: max-age=31536000` (RF-B12), así también los llevan los 404, 405, 403 y 503;
   5. **solo lectura**: todo método distinto de `GET`, `HEAD` y `OPTIONS` recibe 405 con `Allow: GET, HEAD, OPTIONS` y `ApiError` `http.method_not_allowed`, sin llegar al router. El comentario de la capa dice que la feature de auth la reemplaza por el middleware de ADR-0008 D4;
   6. el router: `/api/health`, `/ws`, `fallback` (404 `http.not_found`) y `method_not_allowed_fallback` (405 `http.method_not_allowed`, para que un `OPTIONS /api/health`, que el `MethodRouter` de axum responde con 405 y cuerpo vacío, también lleve el código; nota 7 de devops). No se monta `CorsLayer`.
7. **Códigos de error y `ApiError`** (confirmado por el architect; entra en la enmienda de RF-B4 y RF-B5). RF-B13 pide `ApiError { error: "ws.origin_forbidden" }` y `ApiError { error: "ws.capacity" }` en las respuestas HTTP del handshake, pero RF-B5 pone esos códigos en `WsErrorCode` y define `ApiError { error: ApiErrorCode }`. Para no duplicar variantes, `ApiError { error: ErrorCode }` con `#[serde(untagged)] enum ErrorCode { Http(ApiErrorCode), Ws(WsErrorCode) }`. El JSON es el mismo que pide la spec y el TS generado queda `type ErrorCode = ApiErrorCode | WsErrorCode`. Cada variante lleva `#[serde(rename = "http.not_found")]` y similares (`rename_all` no produce puntos), y `as_str()` devuelve la misma cadena para usarla como razón de cierre del WS; un unit comprueba que `as_str()` y serde coinciden en todas las variantes. Como `untagged` deserializa probando las variantes en orden, **un unit comprueba que los conjuntos de cadenas de `ApiErrorCode` y `WsErrorCode` no se solapan** (y que cada cadena hace ida y vuelta a la variante correcta de `ErrorCode`). Los códigos de auth futuros (`auth.*`) van en una variante nueva de `ErrorCode`, no dentro de las existentes.
8. **`/api/health` con caché y una consulta en vuelo (RF-B10).** `HealthState` guarda `tokio::sync::Mutex<Option<(Instant, DbStatus)>>`. La petición toma el mutex; si el resultado tiene menos de `cache_ttl`, lo devuelve; si no, corre `SELECT 1` con `tokio::time::timeout(db_timeout, …)` **con el mutex tomado**, guarda y devuelve. Las peticiones concurrentes esperan el mutex y encuentran el resultado fresco, así que hay una sola consulta en vuelo y el pool de 8 nunca se llena desde aquí. La consulta va detrás de un trait `DbProbe` (implementación real `PgProbe`); el test envuelve la real con un contador (nota 6 de devops: un proxy TCP cuenta conexiones, no consultas). `cache_ttl` (1 s) y `db_timeout` (1 s) son campos de `HealthConfig`, no variables de entorno; los tests los acortan. Responde `Cache-Control: no-store`. `version` sale de `option_env!("APP_VERSION")` con `dev` por defecto.
9. **Eco solo en `localhost` (sugerencia de security-reviewer, ronda 2).** El eco se enciende si `ENABLE_WS_ECHO == "1"` **y** `PUBLIC_ORIGIN` ya parseado es `http` con host `localhost`, `127.0.0.1` o `[::1]`. Es más estricto que "no empieza con `https://`" (cubre un host público con `http`). Con la variable en `1` y otro origen, registra `warn` `ws.echo_refused` y el eco queda apagado; si lo enciende, `warn` `ws.echo_enabled`. Los dos `warn` se verifican: en el unit (la función devuelve el evento que hay que registrar) y en `tests/logs.rs` (la línea aparece con `level` `WARN` en stdout del binario). Los dev, tests y e2e usan `http://localhost:<puerto>`.
10. **WebSocket** (`ws/`):
    - Orden del handshake (respuesta 1 del architect, RF-B13 enmendado): `Origin` exacto (si falta o no coincide, 403 `ws.origin_forbidden`, sin upgrade) → upgrade válido (extracción de `WebSocketUpgrade`) → tope global. El tope es un `Arc<Semaphore>` de `WS_MAX_CONNECTIONS` permisos; el handler toma el permiso con **`try_acquire_owned`**, **antes** de devolver el 101, y si no hay responde 503 `ws.capacity` de inmediato. El permiso se **mueve al closure de `on_upgrade`**, vive en la tarea del socket y se libera al cerrar. **Nunca `acquire().await`**: una espera sin límite de cola acumularía peticiones en memoria, un DoS (nit 1 de security-reviewer a la enmienda b).
    - `WebSocketUpgrade` se extrae como `Result<_, WebSocketUpgradeRejection>`: un `GET /ws` con `Origin` válido y sin cabeceras de upgrade responde con **el 4xx que dé axum** (un `HEAD /ws` da 405: en axum 0.8 `get` también atiende `HEAD`, y el 405 lo produce el rechazo `MethodNotGet` de `WebSocketUpgrade`, no el router), **cuerpo vacío y sin `Content-Type`** (enmienda 2026-10-23 b), sin código nuevo, para no emitir el texto en inglés del rechazo (el API solo emite códigos). Lleva `nosniff` y HSTS (capa 4 de la decisión 6) y su línea `http.request` con `route: "/ws"`.
    - En el handler, antes de llamar a `on_upgrade`, se toman `tracker.token()` (un `TaskTrackerToken`, que mantiene vivo el seguimiento mientras exista) y `token.clone()` del `CancellationToken`, y los dos se **mueven al closure de `on_upgrade` junto con el permiso del semáforo**. Así un socket mejorado justo durante el apagado nunca queda fuera del `TaskTracker`, y si el upgrade falla los tres se sueltan solos al descartarse el closure (ronda 2 del architect, hallazgo 2; `track_future` no sirve porque el future no existe hasta el callback). El bucle del socket usa `tokio::select! { biased; _ = token.cancelled() => …, … }`, con la cancelación como primera rama.
    - Límites en el protocolo: `max_message_size(4096)` y `max_frame_size(4096)`. Si `recv` devuelve el error de capacidad, se intenta `Close(1009)` con un plazo de `close_grace`; si tungstenite ya invalidó la conexión, se suelta.
    - Ritmo: ventana deslizante de 10 mensajes por 10 s (`VecDeque<Instant>`). Cuentan los `Text`, los `Binary` y los **`Ping` del cliente** (sugerencia de security-reviewer: así un cliente no puede hacer que el server conteste pings sin límite); los `Pong` no cuentan. El undécimo cierra con `Close(1008, "ws.rate_limited")`: `WsErrorCode::RateLimited` es nuevo (respuesta 2 del architect, entra en la enmienda de RF-B5 y RF-B13), con su test de forma JSON, su llave `ws_close_reason_rate_limited` en `es` y `en` y su caso en `closeReasonKey`.
    - `Binary` → `1003`. `Text` que no es un `ClientMessage` → `1008` `ws.invalid_message`. `Echo` con el eco apagado → `1008` `ws.echo_disabled`.
    - Vida del socket: `ping_interval` 30 s; si no llega **ningún** frame del cliente en `pong_timeout` 60 s, `1008` `ws.pong_timeout`; inactividad de `idle_timeout` 120 s sin un mensaje de datos aceptado, `1008` `ws.idle_timeout`; todo envío y el cierre van con `tokio::time::timeout(close_grace = 5 s)` y, si vence, se suelta la conexión TCP. Los cuatro plazos son campos de `WsConfig` (no variables de entorno).
    - Nada del socket se registra en `info`. En `debug`, solo `{"event":"ws.closed","close_code":1008}`. Nunca `payload` ni `Origin`.
11. **Apagado ordenado (RF-B14).** hyper no sigue las conexiones ya mejoradas a WS, así que `with_graceful_shutdown` no basta. Un `CancellationToken` y un `TaskTracker` (de `tokio-util`) acompañan a cada socket, tomados en el handler (`tracker.token()` y `token.clone()`) y movidos al closure de `on_upgrade` (decisión 10). `TaskTracker::wait()` espera a que se suelten todos los `TaskTrackerToken`, después de `tracker.close()`: al recibir `SIGTERM` o `SIGINT`, se cancela el token (cada socket manda `Close(1012)` con `close_grace` y termina), `axum::serve` deja de aceptar, y `main` llama a `tracker.close()` y espera al servidor y a `tracker.wait()` con un tope de 9 s. Sale con 0.
12. **`server healthcheck` sin cliente nuevo (RF-B11).** Usa `hyper::client::conn::http1` sobre un `TcpStream` de tokio a `127.0.0.1:${PORT}`, con un `timeout` de 2 s para todo el intercambio. hyper ya está en el grafo por axum (solo se activan sus features `client` y `http1`); no entran `reqwest`, rustls ni OpenSSL en este camino. La petición lleva `Host: 127.0.0.1:<PORT>` puesto a mano (el `http1` de hyper no lo agrega). Solo se mira el código de estado: el cuerpo **no se lee**, se suelta la respuesta. No lee `DATABASE_URL` ni el resto de la configuración (solo `PORT`), no escribe nada en stdout y tampoco en stderr: ningún error se formatea con `Display` ni `Debug`, solo cambia el código de salida (Docker guarda la salida del healthcheck en `docker inspect`). Con `BIND_ADDR` distinto de `0.0.0.0` o `127.0.0.1` falla siempre (decisión 3).
13. **Arranque (RF-B9).** `db::connect_with_retry(options, interval = 2 s, deadline = 60 s)` intenta `PgPoolOptions::new().max_connections(8).acquire_timeout(1 s).connect_with(…)`; cada fallo registra `{"event":"db.unavailable","error_kind":"io"}` (el tipo de `sqlx::Error`, nunca su `Display`). Al vencer el plazo devuelve `StartupError::DbUnavailable`, y `exit_code(&StartupError)` lo convierte en 1. Después corre `sqlx::migrate!("./migrations")` y **solo entonces** hace `TcpListener::bind`. Si el `bind` falla, registra `{"event":"server.bind_failed","error_kind":…}` (`addr_in_use`, `addr_not_available`, `permission_denied` u `other`, desde `io::ErrorKind`; sin la dirección ni el puerto) y sale con 1 (ronda 2 del architect, hallazgo 4). `DATABASE_URL` se parsea a `PgConnectOptions` en la configuración (`config.invalid` si falla) y nunca se registra.
14. **Sin `query!` ni `.sqlx/`.** La única consulta es `SELECT 1` con `sqlx::query`, sin macros verificadas contra la BD, así que la fase B no necesita `.sqlx/` ni `sqlx-cli`. `sqlx::migrate!` embebe los archivos de `migrations/` sin conectarse. El `SQLX_OFFLINE=true` de RF-C1 no estorba. La primera feature con `query!` agrega `.sqlx/` y `cargo sqlx prepare --check` a `just check`.
15. **`APP_VERSION` validada al compilar.** `build.rs` lee `APP_VERSION` (`cargo:rerun-if-env-changed`), y si está definida y no cumple `^(v\d+\.\d+\.\d+|sha-[0-9a-f]{7}|dev)$`, el build falla. La función de validación vive en `src/version_rule.rs`, sin dependencias, y `build.rs` la incluye con `include!`; el unit la prueba con la misma tabla. `build.rs` también declara `cargo:rerun-if-changed=migrations`.
16. **Dependencias exactas y antigüedad mínima.** `[workspace.dependencies]` con requisitos `=x.y.z` para las dependencias **directas**. Eso solo fija las directas: no es un equivalente de `save-exact` + `minimumReleaseAge` de pnpm, porque cargo no tiene una antigüedad mínima y `cargo generate-lockfile` resuelve las ~200 transitivas a su versión compatible más reciente. Las transitivas las fijan `Cargo.lock` (commiteado, `--locked` en toda receta) y la comprobación de antigüedad de abajo. Criterio: toda versión del lock, directa o transitiva, con **al menos 3 días** publicada (el mismo `cooldown` de Dependabot de RF-C16). Por eso tokio va en 1.53.3 y no en 1.54.x (1.54.0 tiene 4 días pero 1.54.1, de hace 2, la corrige; espero a la línea estable).
    - **`scripts/check-crate-age.mjs`** (security, Important 1) lee `Cargo.lock`, toma cada `[[package]]`: los que no tienen `source` deben ser miembros del workspace (`server`, `protocol`); los que tienen `source` deben ser exactamente `registry+https://github.com/rust-lang/crates.io-index` o `sparse+https://index.crates.io/`, y **cualquier otro** (`git+…`, otro registro, `path`) hace fallar el script (security, ronda 2, Minor 2). De los de crates.io, consulta `created_at` de esa versión en `https://crates.io/api/v1/crates/<crate>/<versión>` (con `User-Agent` propio, una petición por segundo según la política de crates.io, `AbortSignal.timeout(20_000)` por petición) y lista las que tienen menos de 3 días; sale con 1 si hay alguna o si una consulta falla. La lógica pura (parseo del lock y comparación de fechas) se exporta y tiene unit con fixtures. Receta `just crate-age`, fuera de `just check` porque usa la red.
    - **Corre después de `cargo generate-lockfile` y antes del primer `cargo build`** (paso 1): `generate-lockfile`, `cargo tree` y `cargo metadata` no ejecutan `build.rs` ni *proc-macros*; el primer `build` sí, en la Mac del owner. Cada versión marcada se baja con `cargo update -p <crate>@<versión> --precise <versión anterior con ≥ 3 días>`, y se vuelve a correr hasta que sale con 0. Cada bajada se anota en "Desviaciones". Se repite en cada paso que cambie `Cargo.lock`.
17. **Lints.** `[workspace.lints]`: `rust.unsafe_code = "forbid"`, `clippy.unwrap_used = "deny"`, `clippy.expect_used = "deny"`, `clippy.dbg_macro = "deny"`, `clippy.print_stdout = "deny"` y `clippy.print_stderr = "deny"` (todo log pasa por `tracing`, como `log.ts` en `web`). `clippy.toml` con `allow-unwrap-in-tests = true` y `allow-expect-in-tests = true`; los crates de `tests/` llevan además `#![allow(clippy::unwrap_used, clippy::expect_used)]` para sus funciones de soporte.
18. **Base de datos de cada test.** `tests/support/db.rs` crea una base `torneo_test_<uuid>` por test con la conexión de `DATABASE_URL` (el usuario de desarrollo es superusuario en la imagen de Postgres) y la borra al final con `DROP DATABASE … WITH (FORCE)`. Dos guardas (security, Minor 6): **rechaza** (pánico al inicio del test) un `DATABASE_URL` cuyo host no sea de loopback (`127.0.0.1`, `::1` o `localhost`), para que un `DATABASE_URL` real en el shell nunca reciba un `CREATE`/`DROP DATABASE`; y solo borra la base cuyo nombre generó ese mismo objeto (lo guarda en el `struct`), validado contra `^torneo_test_[0-9a-f]{32}$` antes del `DROP`. Nunca lista ni borra otras bases. Si un test aborta, la base queda; `just db-reset` la limpia.
    - **Entorno de los tests del binario** (nota 6 del architect): `tests/support/bin.rs` lanza el binario con `Command::env_clear()` y solo `PATH` más las variables explícitas de cada test, para que un `DATABASE_URL`, `RUST_LOG` o `ENABLE_WS_ECHO` del shell no cambie el resultado. Prefiero esto a `#[sqlx::test]` porque los tests del binario necesitan la misma base fuera del proceso.
19. **Proxy TCP de prueba.** `tests/support/pg_proxy.rs`: escucha en `127.0.0.1:0` y reenvía a Postgres con `tokio::io::copy_bidirectional`. `cut()` cierra todas las conexiones vivas y rechaza las nuevas (cierra el socket aceptado al instante); `restore()` vuelve a reenviar. Lo usan RF-B9, RF-B10 y RF-B11.
20. **Tipos TS (RF-B6).** `ts-rs` 12 con `TS_RS_EXPORT_DIR` absoluto y `TS_RS_IMPORT_EXTENSION=js` (en ts-rs 12 reemplaza a la feature `import-esm`), para que los imports generados lleven `.js` como el resto de `apps/web` (Kit 3). `TS_RS_LARGE_INT` se quita del entorno (`env -u`) para que un `u64` siga saliendo como `bigint` y lo detecte el chequeo. **`just gen` vacía `packages/protocol-ts/src/generated/*.ts` antes de exportar** (blocker 1 del architect): `ts-rs` no borra el archivo de un tipo eliminado, así que sin esto el barril seguiría exportando un huérfano. `crates/protocol/bindings/` (el directorio por defecto de `ts-rs` si alguien corre `cargo test -p protocol --features ts` sin `just gen`) entra en `.gitignore`. `packages/protocol-ts/src/generated/` **sale de `.prettierignore`**: Prettier ignora los archivos ignorados aunque se le pasen por ruta, así que, si no, `just gen` no podría formatearlos como pide RF-B6.
21. **Frescura sin tocar el índice de git.** `_check-gen` copia `packages/protocol-ts/src` a un `mktemp -d`, corre `just gen` en su sitio y compara los dos árboles con `scripts/compare-trees.mjs` (archivos solo en uno de los lados, incluido un `.ts` de un tipo borrado en Rust, o con contenido distinto; la lógica se exporta y tiene unit con los cuatro casos: iguales, archivo sobrante, archivo faltante, contenido distinto). Distingue dos fallos (ronda 2 del architect, hallazgo 5): si `just gen` falla (no compila, `bigint`, Prettier), `_check-gen` restaura la copia del `mktemp -d` sobre `packages/protocol-ts/src` y sale con "just gen falló: el árbol no se tocó" y el código de `gen`; si `gen` termina bien y los árboles difieren, sale con "protocol-ts estaba desactualizado: se regeneró; revisa y commitea". Un unit de `compare-trees.mjs` cubre los mensajes y un caso de B19 rompe `gen` a propósito. Compara contra el árbol de trabajo y no contra el índice, así que en local no obliga a hacer `git add` antes de `just check`, y en la CI (checkout limpio) equivale al `git diff --exit-code` de RF-B6. Descarto generar a un directorio temporal: fuera del repo Prettier no encuentra `.prettierrc.json` ni `.editorconfig`, y dentro de `target/` lo ignora por `.gitignore`.
22. **La web solo importa tipos.** `health.ts` y `echo.ts` usan `import type` de `@torneo/protocol`, así que el paquete no tiene código en tiempo de ejecución y Vite no necesita transformarlo. Las dos llamadas son del cliente (en 001 el SSR no llama al API). `echo.ts` abre `new WebSocket('/ws')` con una URL **relativa** (el estándar WHATWG la admite y la soportan las dos últimas versiones de Safari, Chrome y Firefox), así que no arma ninguna URL absoluta y cumple la decisión 20 de `plan-a.md`. Las dos funciones reciben `fetch` y el constructor de `WebSocket` por parámetro con valor por defecto, para probarlas en Vitest. En `/dev/status`, el `payload` recibido se muestra **solo con interpolación de texto** de Svelte (`{payload}`), nunca con `{@html}` (security, Minor 9); la verificación del paso 13 incluye `grep -rn '{@html' apps/web/src/routes/dev/status` vacío.
23. **Proxy de Vite** (RF-B17): `server.proxy` con claves de expresión regular, `'^/api(?:/|$)'` a `http://127.0.0.1:8080` y `'^/ws$'` a `ws://127.0.0.1:8080` con `ws: true`, sin `rewrite`, sin `changeOrigin` (el `Origin` del navegador llega intacto al server) y sin `xfwd`. Las expresiones evitan que `/apix` o `/wsx` vayan al server, igual que las exclusiones por segmento de `localeRedirect`.
24. **`just dev` compatible con bash 3.2** (el de macOS). Receta *shebang* que sube Postgres, carga `apps/server/.env` solo para el proceso del server (`set -a; . apps/server/.env; set +a` dentro de un subshell), lanza el server y la web en segundo plano, instala `trap 'kill 0' INT TERM` y hace `wait`. Sin `wait -n` (bash ≥ 4.3). `DATABASE_URL` nunca se exporta de forma global en el `justfile` (nota 5 de devops: rompería `prod-local` en C).

## Versiones fijadas

Verificadas el **2026-10-23** contra `static.rust-lang.org` (toolchain), la API de crates.io (`/api/v1/crates/<crate>`, con `timeout`) y la de Docker Hub (Postgres). El paso 1 vuelve a comprobarlas y anota cualquier cambio en "Desviaciones".

| Pieza | Versión | Publicada | Features / notas |
|---|---|---|---|
| Rust (`rust-toolchain.toml`) | `1.99.1` | 2026-10-13 | `components = ["rustfmt", "clippy"]`, `targets = ["x86_64-unknown-linux-musl"]`, `profile = "minimal"`. Edición 2024, `rust-version = "1.99"`. El builder `rust:1.99.1` de RF-C1 ya existe en Docker Hub |
| `axum` | `0.8.9` | 2026-04-14 | `default-features = false`, `["http1", "json", "matched-path", "tokio", "ws"]`. Sin `tracing` (registra rechazos con detalle). Trae `tokio-tungstenite` 0.29 |
| `tokio` | `1.53.3` | 2026-10-13 | `["rt-multi-thread", "macros", "net", "signal", "time", "sync", "io-util"]`. Decisión 16 |
| `tokio-util` | `0.7.20` | 2026-10-09 | `["rt"]` (`TaskTracker`); pide tokio `^1.51` |
| `tower` | `0.5.3` | 2026-01-12 | `["util"]` |
| `tower-http` | `0.7.1` | 2026-08-31 | `["sensitive-headers", "set-header"]` |
| `hyper` | `1.12.0` | 2026-10-06 | `["client", "http1"]` (healthcheck, decisión 12) |
| `hyper-util` | `0.1.21` | 2026-09-24 | `["tokio"]` (`TokioIo`) |
| `http-body-util` | `0.1.5` | 2026-08-12 | cuerpo vacío del healthcheck |
| `sqlx` | `0.9.0` | 2026-05-21 | `default-features = false`, `["runtime-tokio", "tls-rustls-ring", "postgres", "migrate", "macros"]`. En 0.9, `tls-rustls-ring` es alias de `tls-rustls-ring-webpki`. `macros` hace falta para `migrate!`. MSRV 1.94 |
| `url` | `2.5.8` | 2026-01-05 | ya está en el grafo por `sqlx-core` |
| `serde` | `1.0.229` | 2026-07-18 | `["derive"]` |
| `serde_json` | `1.0.154` | 2026-10-11 | server; en `protocol`, solo `dev-dependencies` (RF-B3) |
| `ts-rs` | `12.0.1` | 2026-01-31 | **opcional** en `protocol`, feature `ts`; features por defecto (`serde-compat`) |
| `tracing` | `0.1.44` | 2025-12-18 | |
| `tracing-subscriber` | `0.3.23` | 2026-03-13 | `default-features = false`, `["std", "fmt", "env-filter", "registry"]` (sin `json`, decisión 4) |
| `uuid` | `1.28.0` | 2026-10-10 | `["v4"]` (`request_id` y bases de test) |
| `mimalloc` | `0.1.52` | 2026-05-22 | `#[global_allocator]` (ADR-0006 D8) |
| `thiserror` | `2.0.21` | 2026-09-23 | |
| `tokio-tungstenite` (dev) | `0.29.0` | 2026-03-17 | cliente WS de los tests; la misma línea que usa axum, para no duplicar `tungstenite` |
| `futures-util` (dev) | `0.3.34` | 2026-08-11 | `SinkExt`/`StreamExt` en los tests |
| `cargo-deny` (herramienta) | `0.20.2` | 2026-07-09 | `cargo install --locked cargo-deny@0.20.2`; opcional en local (RF-B19) |
| Postgres (dev) | `postgres:17.10-alpine@sha256:742f40ea20b9ff2ff31db5458d127452988a2164df9e17441e191f3b72252193` | 2026-07-08 | Digest del índice multi-arquitectura, así sirve en Mac ARM y en el runner amd64. El de `17-alpine` (17.11, `sha256:eee6c6e1…`) se publicó el 2026-10-22 y no cumple los 3 días (nota 8 del architect). Si el paso 4 se hace el 2026-10-25 o después, se cambia a `postgres:17-alpine@sha256:eee6c6e1b0894fa4fe0075eb054af9fd1cf39a35321c2d4110626cfe506411fb` tras confirmarlo, y se anota. Dependabot (ecosistema `docker-compose`, RF-C16, con `cooldown` de 3 días) lo mueve después. El paso 4 lo confirma con `docker buildx imagetools inspect` |

Del lado de `apps/web` no entra ningún paquete de npm: solo `"@torneo/protocol": "workspace:*"` en `devDependencies` (`minimumReleaseAge` no aplica a paquetes del workspace).

**No verificado (se comprueba en el paso indicado):** (a) que `cargo deny check bans` pase con sqlx 0.9 y `tls-rustls-ring` sin traer `aws-lc-sys` (paso 1); (b) qué hace tungstenite 0.29 tras el error de capacidad: si se puede mandar `Close(1009)` o la conexión ya quedó inválida (paso 9); (c) que el `PgConnectOptions` de sqlx 0.9 no aparezca en ningún log de sqlx en `debug` (lo atrapa el canario del paso 11 y lo neutraliza la decisión 4); (d) que la CSP `connect-src 'self'` deje pasar `ws://localhost:5173/ws` en WebKit con `vite dev` (paso 13); (e) que `axum::serve` 0.8.9 aplique un `header_read_timeout` de hyper (30 s por defecto solo cuando hay un *timer*). El paso 7 lo prueba en `tests/http.rs`: una conexión TCP que manda cabeceras parciales y nunca las termina debe quedar cerrada por el server. Si `axum::serve` no lo aplica o no permite acortarlo para el test, el server sirve con `hyper_util::server::conn::auto::Builder` y un `header_read_timeout` explícito de 10 s (configurable en el test), y se anota en "Desviaciones" (security, Minor 5).

## Archivos que cambian

| Archivo | Cambio |
|---|---|
| `Cargo.toml` | nuevo: `[workspace]` con `members = ["apps/server", "crates/protocol"]` (explícitos), `resolver = "3"`, `[workspace.package]` (edición 2024, `rust-version`, `license = "AGPL-3.0-only"`), `[workspace.dependencies]` con `=x.y.z`, `[workspace.lints]` (decisión 17) y `[profile.release]` `lto = "thin"`, `codegen-units = 1`, `strip = true` (RF-B1) |
| `Cargo.lock` | nuevo (generado; se revisa con `cargo tree`) |
| `rust-toolchain.toml` | nuevo: `1.99.1`, `rustfmt`, `clippy`, target musl (RF-B1) |
| `deny.toml` | nuevo: `[graph] targets = ["x86_64-unknown-linux-musl"]`; `[bans] deny = [{ crate = "openssl-sys" }, { crate = "openssl" }, { crate = "aws-lc-sys" }]`, `multiple-versions = "warn"` (RF-B2) |
| `clippy.toml` | nuevo: decisión 17 |
| `.prettierignore` | modificado: sale `packages/protocol-ts/src/generated/` (decisión 20) |
| `crates/protocol/Cargo.toml` | nuevo: `serde`; `ts-rs` opcional con `[features] ts = ["dep:ts-rs"]`; `serde_json` solo en `dev-dependencies` |
| `crates/protocol/src/lib.rs` | nuevo: `#![forbid(unsafe_code)]`, módulos y reexportes; doc del contrato (sin I/O, sin texto para humanos, sin enteros de 64 bits) |
| `crates/protocol/src/{health,ws,error}.rs` | nuevos: `HealthResponse`, `ClientMessage`, `ServerMessage`, `ApiErrorCode`, `WsErrorCode`, `ErrorCode`, `ApiError` (RF-B4, RF-B5, decisión 7), con `#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]` y tests de forma JSON exacta |
| `crates/engine/.gitkeep`, `crates/engine-wasm/.gitkeep` | sin cambios |
| `crates/protocol/.gitkeep`, `apps/server/.gitkeep`, `packages/protocol-ts/.gitkeep` | se borran (los directorios ya tienen contenido) |
| `packages/protocol-ts/package.json` | nuevo: `@torneo/protocol`, `private`, `type: module`, `license`, `exports: { ".": { "types": "./src/index.ts", "default": "./src/index.ts" } }`, sin dependencias |
| `packages/protocol-ts/src/index.ts` | nuevo (generado): cabecera y `export type * from './generated/<T>.js'` por archivo, en orden alfabético |
| `packages/protocol-ts/src/generated/*.ts` | nuevos (generados y commiteados, RF-B6) |
| `packages/protocol-ts/README.md` | nuevo: una línea "generado por `just gen` desde `crates/protocol`; no editar" |
| `scripts/gen-protocol-ts.mjs` | nuevo: antepone la cabecera de "archivo generado, no editar" a cada `.ts` de `generated/`, falla si alguno contiene `bigint` (RF-B4) y escribe el barril. Exporta `finalizeProtocolTs(dir)` para el test |
| `scripts/__tests__/gen-protocol-ts.test.mjs`, `scripts/__fixtures__/gen-protocol-ts/**` | nuevos: un fixture con `bigint` que falla, uno válido que produce el barril y las cabeceras esperadas, idempotencia (correrlo dos veces no duplica la cabecera) y que el barril solo lista los archivos presentes |
| `scripts/compare-trees.mjs`, `scripts/__tests__/compare-trees.test.mjs` | nuevos: comparación de árboles de `_check-gen` (decisión 21), con unit de iguales, archivo sobrante (tipo borrado sin regenerar), archivo faltante y contenido distinto |
| `scripts/check-crate-age.mjs`, `scripts/__tests__/check-crate-age.test.mjs`, `scripts/__fixtures__/check-crate-age/**` | nuevos: decisión 16; unit del parseo de `Cargo.lock` (acepta miembros del workspace sin `source`; **falla** con un fixture que trae un `source = "git+https://…"` y con otro de un registro ajeno) y de la comparación de fechas, con la red simulada |
| `.gitignore` | modificado: `crates/protocol/bindings/` (decisión 20) |
| `apps/server/Cargo.toml` | nuevo: `[lib] name = "server"`, `[[bin]] name = "server"`, dependencias de la tabla, `build = "build.rs"`, `[lints] workspace = true` |
| `apps/server/build.rs` | nuevo: decisión 15 |
| `apps/server/src/main.rs` | nuevo: `#[global_allocator] static GLOBAL: mimalloc::MiMalloc`; subcomandos (decisión 2); `ExitCode` |
| `apps/server/src/lib.rs` | nuevo: `run_cli`, `serve`, `AppState`, reexportes para los tests |
| `apps/server/src/config.rs` | nuevo: decisión 3 |
| `apps/server/src/version_rule.rs`, `apps/server/src/version.rs` | nuevos: decisión 15 |
| `apps/server/src/telemetry.rs` | nuevo: formateador con lista blanca, filtro con topes y *panic hook* (decisión 4) |
| `apps/server/src/db.rs` | nuevo: `connect_with_retry`, `migrate`, `DbProbe`/`PgProbe` (decisiones 8 y 13) |
| `apps/server/src/http/{mod,health,errors,read_only,request_log}.rs` | nuevos: router y capas (decisiones 5, 6 y 8) |
| `apps/server/src/ws/{mod,handshake,session,rate}.rs` | nuevos: decisión 10 |
| `apps/server/src/shutdown.rs` | nuevo: señales, `CancellationToken` y `TaskTracker` (decisión 11) |
| `apps/server/src/healthcheck.rs` | nuevo: decisión 12 |
| `apps/server/migrations/20261023000000_baseline.sql` | nuevo: comentario "baseline sin tablas de dominio (TA-1)" y `SELECT 1;` (RF-B9) |
| `apps/server/tests/support/{mod,db,pg_proxy,bin,http,ws}.rs` | nuevos: base por test (decisión 18), proxy (decisión 19), lanzador del binario con captura de stdout/stderr, cliente HTTP mínimo con hyper y cliente WS |
| `apps/server/tests/{startup,health,http,healthcheck,ws,shutdown,logs}.rs` | nuevos: integración (ver "Pruebas") |
| `apps/server/.env.example` | nuevo: `DATABASE_URL` local, `PUBLIC_ORIGIN=http://localhost:5173`, `ENABLE_WS_ECHO=1`, `BIND_ADDR=127.0.0.1`, `RUST_LOG=info`, con un comentario "solo desarrollo; nunca valores de producción" (RF-B18) |
| `infra/compose.dev.yml` | nuevo: `name: torneo-dev`, solo `postgres` por digest, `127.0.0.1:${DEV_PG_PORT:-5432}:5432`, usuario `torneo`, contraseña `torneo_dev`, BD `torneo`, volumen nombrado `torneo-dev-pg`, healthcheck `pg_isready -U torneo -d torneo` (2 s / 3 s / 30 reintentos) (RF-B17) |
| `justfile` | modificado: ver "`just check` en la fase B" |
| `apps/web/package.json`, `pnpm-lock.yaml` | modificados: `"@torneo/protocol": "workspace:*"` en `devDependencies` |
| `apps/web/vite.config.ts` | modificado: `server.proxy` (decisión 23) |
| `apps/web/.env.example` | sin cambios (ya tiene `PUBLIC_ORIGIN` y `ENABLE_DEV_CATALOG`, RF-B18) |
| `apps/web/src/lib/features/system/health.ts` (+ `health.test.ts`) | nuevos: `fetchHealth(fetchFn = fetch)` → `{ kind: 'ok', version } \| { kind: 'db_unavailable' } \| { kind: 'network_error' }`, con un *type guard* de `HealthResponse` (una respuesta con forma inesperada cuenta como `network_error`) y `healthMessageKey()` (RF-B7) |
| `apps/web/src/lib/features/system/echo.ts` (+ `echo.test.ts`) | nuevos: `openEcho(WebSocketCtor = WebSocket)` con estados `connecting \| connected \| sent \| received \| closed`, `send(payload)` con un `ClientMessage` tipado, parseo de `ServerMessage` y `closeReasonKey(reason)` con un `switch` exhaustivo sobre `WsErrorCode` (`never` en el `default`), más una llave para razones vacías o desconocidas |
| `apps/web/src/routes/dev/status/+page.svelte` | nuevo: estados de "Diseño → UI" (`skeleton` mientras carga, `badge` con la versión, `badge` destructivo, error de red, y el eco con un `input`, un `button` y el estado). Sin `+page.server.ts`: todo corre en el cliente, en `onMount` |
| `apps/web/messages/{en,es}.json` | modificados: llaves `dev_status_*` (título, estados del server, estados del eco, etiquetas) y `ws_close_reason_*` (una por `WsErrorCode`, más `unknown`) |
| `apps/web/e2e/security.spec.ts` | modificado: quita el comentario "no existe en la fase A" de `/es/dev/status`; ahora sí cuenta como cobertura del guard de RF-A16 |
| `apps/web/e2e/dev-status.spec.ts`, `apps/web/playwright.dev.config.ts` | nuevos: e2e opcional contra `just dev` (decisión de "Áreas de preocupación" 4 de la spec), fuera de `just e2e` y de la CI |
| `CLAUDE.md` | modificado: "Comandos" (RF-B20) |
| `docs/architecture.md` | modificado: "Tipos compartidos" con `ts-rs` y las convenciones de RF-B4 (RF-B20) |
| `docs/legal/aviso-de-privacidad.md` | modificado: `{{PLAZO_LOGS}}`, fila "Registros técnicos del servidor", aclaración de los logs de `web` y punto 8 de "Pendiente" (RF-B20) |

## Orden de trabajo

Cada paso termina con un comando que lo verifica. Un commit por paso o por grupo, con `feat(server|protocol|web|infra): … (TA-1)`. Los pasos 2–3 y 4 son **paralelizables**; el 13 (web) es **paralelizable** con los pasos 5 a 11 una vez terminado el 3.

0. **Rama y toolchain.** `git switch -c ta-1-fundaciones-fase-b main` (con TA-10 ya en `main`). `rust-toolchain.toml` primero, y `rustup toolchain install` (lee el archivo). → `rustup show active-toolchain` en la raíz dice `1.99.1-aarch64-apple-darwin (overridden by '…/rust-toolchain.toml')`; `rustup target list --installed` incluye `x86_64-unknown-linux-musl`.
1. **Workspace (RF-B1, RF-B2).** Vuelvo a comprobar las versiones de la tabla con la API de crates.io. `Cargo.toml`, `deny.toml`, `clippy.toml`, los dos `Cargo.toml` de miembros, un `lib.rs` vacío en `protocol` y un `main.rs` mínimo con `mimalloc` en el server.
   - **1a. Lock sin compilar:** `cargo generate-lockfile`. Todavía nada de `cargo build`, `check`, `test` ni `clippy`, que ejecutan `build.rs` y *proc-macros*. **El editor queda cerrado, o rust-analyzer desactivado** (y cualquier `cargo watch`), hasta que 1b salga con 0: rust-analyzer corre `cargo check` al abrir el workspace (security, ronda 2, Minor 1).
   - **1b. Antigüedad de todo el lock** (decisión 16, security Important 1): `scripts/check-crate-age.mjs` con su unit, y `just crate-age`. Cada versión con menos de 3 días se baja con `cargo update -p <crate>@<versión> --precise <anterior>`, y se repite. → `just crate-age` sale con 0; su salida (paquetes revisados, ninguno marcado) va al PR, y las bajadas quedan en "Desviaciones".
   - **1c. Primer build.** → `cargo build --workspace --locked`; `cargo tree -p server -e normal --locked | grep -E 'openssl|aws-lc'` vacío; `cargo deny check bans` en verde (si no está instalado, `cargo install --locked cargo-deny@0.20.2` en este paso: es la única vez que lo pido, porque es la prueba de RF-B2 hasta que llegue la CI); `cargo tree -p server -e normal | grep ts-rs` vacío.
2. **Tipos de `protocol` (RF-B3, RF-B4, RF-B5).** Los cuatro módulos, con tests que comparan la cadena JSON exacta de cada caso: `{"status":"ok","version":"dev"}`, `{"status":"db_unavailable"}`, `{"type":"echo","payload":"hola"}` en los dos sentidos, `{"error":"http.not_found"}`, `{"error":"http.method_not_allowed"}`, `{"error":"ws.origin_forbidden"}`, `{"error":"ws.capacity"}`, cada `WsErrorCode` y la ida y vuelta (`from_str` de la forma exacta). También: `{"type":"echo"}` sin `payload` y `{"type":"subscribe"}` no deserializan. → `cargo test -p protocol --locked` y `cargo test -p protocol --features ts --locked` en verde; `cargo clippy -p protocol --features ts --all-targets -- -D warnings` limpio.
3. **`@torneo/protocol` y `just gen` (RF-B6).** `package.json` del paquete, `scripts/gen-protocol-ts.mjs` con su test y fixtures, la receta `gen`, el cambio a `.prettierignore` y `pnpm install` para enlazar el paquete. → `just gen` dos veces seguidas deja `git status --porcelain packages/protocol-ts` igual entre corridas; `grep -L 'Generated by' packages/protocol-ts/src/generated/*.ts` vacío; `grep -r bigint packages/protocol-ts` vacío; los imports generados terminan en `.js`; agregar a mano un campo `u64` temporal a `HealthResponse` hace fallar `just gen` con un mensaje que nombra el archivo (y se revierte); `pnpm exec vitest run --config scripts/vitest.config.mjs` en verde.
4. **Postgres de desarrollo (RF-B17, parte).** `infra/compose.dev.yml` y las recetas `db-up`, `db-down`, `db-reset`. Confirmo el digest con `docker buildx imagetools inspect postgres:17-alpine` (arquitecturas `linux/amd64` y `linux/arm64`). → `just db-up` dos veces (la segunda no recrea nada); `docker compose -f infra/compose.dev.yml ps` muestra `healthy`; `docker port torneo-dev-postgres-1 5432` da `127.0.0.1:5432`; `DEV_PG_PORT=55432 just db-up` publica en `127.0.0.1:55432`; `docker buildx imagetools inspect` confirma que el digest elegido tiene ≥ 3 días.
5. **Configuración, versión y telemetría (RF-B8, RF-B15 parte).** `config.rs`, `version_rule.rs`, `build.rs`, `telemetry.rs`. → unit de `Config::from_lookup`: falta `DATABASE_URL` o `PUBLIC_ORIGIN` → `Invalid { var }` con ese nombre; `PUBLIC_ORIGIN` válido (`http://localhost:5173`, `https://torneo.example`, `http://127.0.0.1:8080`) e inválido (con ruta, barra final, query, fragmento, credenciales, `ftp://`, vacío, mayúsculas en el host, `https://…:443`, `http://…:80`), y en cada caso el `Display` del error **no contiene** la entrada; `PORT` y `WS_MAX_CONNECTIONS` fuera de rango; `RUST_LOG` imparseable; tabla del eco (decisión 9): `1` + `http://localhost:5173` → encendido, `1` + `https://torneo.example` → apagado y `ws.echo_refused`, `1` + `http://torneo.example` → apagado, `true` o ausente → apagado. Unit de `version_rule` (`v1.2.3`, `sha-abcdef0`, `dev` válidos; `v1.2`, `sha-ABCDEF0`, `sha-abcdef`, `latest`, vacío inválidos). Unit del formateador con un `MakeWriter` en memoria: un evento con un campo fuera de la lista blanca y un `message` solo escribe `ts`, `level` y los campos permitidos; un evento de `target = "sqlx::query"` solo escribe `ts`, `level`, `target` y `event: "external"`. `APP_VERSION=bad cargo build -p server` falla.
6. **BD, migración y `/api/health` (RF-B9, RF-B10, RF-B16).** `db.rs`, `http/health.rs`, la migración baseline y el arranque en `lib.rs`. → `tests/startup.rs` y `tests/health.rs` en verde (ver "Pruebas", filas B9 y B10).
7. **Router y capas (RF-B12, RF-B15).** `http/{mod,errors,read_only,request_log}.rs`. → `tests/http.rs` en verde (fila B12).
8. **`server healthcheck` (RF-B11).** → `tests/healthcheck.rs` en verde (fila B11).
9. **WebSocket (RF-B13).** `ws/*`. Primero un test que observa qué hace tungstenite 0.29 con un mensaje de 4097 bytes (No verificado b) y lo anoto en "Desviaciones". → `tests/ws.rs` en verde (fila B13).
10. **Apagado (RF-B14).** `shutdown.rs` y su conexión con `serve`. → `tests/shutdown.rs` en verde (fila B14).
11. **Canarios de logs (RF-B15, RF-B16).** `tests/logs.rs` contra el binario real, una vez con `RUST_LOG=info` y otra con `RUST_LOG=debug`. → en verde (fila B15).
12. **`justfile` (RF-B17, RF-B19).** Recetas de la sección siguiente. → `just check` en verde desde cero con Docker encendido; con Docker apagado, `just check` falla en `db-up` con el mensaje "Docker no responde: `just check` necesita Docker desde la fase B"; `just dev` levanta Postgres, el server y la web, y Ctrl-C deja `pgrep -f 'target/debug/server'` y `pgrep -f 'vite'` vacíos (Postgres sigue arriba).
13. **Web (RF-B7, RF-B17 parte, RF-B18).** Depende del paso 3 para escribir el código y de los pasos 9 y 12 para verificarlo (el WS y `just dev`; nota 10 del architect). Dependencia del workspace, proxy de Vite, `health.ts`, `echo.ts`, mensajes, `/dev/status` y el cambio en `security.spec.ts`. → `just check` en verde (svelte-check, ESLint, Vitest de `health.ts` y `echo.ts`, `check-i18n`, `check-kit-config`); `just e2e` en verde (incluido `/es/dev/status` → 404 sin la variable); con `just dev`, `curl -s localhost:5173/api/health` da el JSON del server, `curl -sI localhost:5173/apix` no llega al server (lo responde `web`) y `/es/dev/status` muestra `ok` con la versión y un eco de ida y vuelta, en `es` y `en`; `grep -rn '{@html' apps/web/src/routes/dev/status` vacío. `pnpm --filter web exec playwright test -c playwright.dev.config.ts` en Chromium y WebKit con `just dev` arriba: sin `securitypolicyviolation`, estado `ok` y eco recibido.
14. **Plazo de logs y docs (RF-B20).** Con `cargo run --release -p server` (para que la latencia y el ancho de `latency_ms` sean los de producción), 100 `curl` a `/api/health`, y con un *one-liner* de Node que envuelve cada línea como lo hace `json-file` (`JSON.stringify({ log: line + "\n", stream: "stdout", time: <RFC 3339 con nanosegundos> })`, que ya incluye el escape de las comillas, nota 9 de devops) calculo el tamaño medio por línea, los bytes por día con 5,760 healthchecks y los días que caben en 30 MB. Escribo `{{PLAZO_LOGS}}` con ese resultado (regla en el riesgo "Plazo de logs"), declaro en la fila "Registros técnicos del servidor" del aviso todos los campos de la línea, incluidos la duración de la petición (`latency_ms`) y un identificador aleatorio por petición sin relación con el usuario (`request_id`) (security, Minor 7), y actualizo `CLAUDE.md`, `docs/architecture.md` y el aviso. → revisión del PR; el cálculo y sus entradas van en la descripción.
15. **Cierre.** En un worktree temporal limpio (`git worktree add`, `just install`): `just check`, `just e2e` y `cargo deny check bans`. → salidas pegadas en el PR, con la evidencia de B1 (`just dev`), el cálculo del paso 14 y las capturas de `/dev/status`.

## `just check` en la fase B

```
set shell := ["bash", "-euo", "pipefail", "-c"]

pg_port := env("DEV_PG_PORT", "5432")
# Only the test recipes use this default; it is never exported globally (devops note 5).
test_db_url := env("DATABASE_URL", "postgres://torneo:torneo_dev@127.0.0.1:" + pg_port + "/torneo")

check: check-web check-rust

check-web: _web-prepare            # the phase A `_check-web`, renamed so CI (phase C) can call it
    ...same lines as phase A...

check-rust:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --locked -- -D warnings
    cargo clippy -p protocol --features ts --all-targets --locked -- -D warnings
    {{just_executable()}} db-up
    DATABASE_URL='{{test_db_url}}' cargo test --workspace --locked
    {{just_executable()}} _check-gen
    {{just_executable()}} _check-ts-rs-graph

test: (phase A lines) + {{just_executable()}} db-up + DATABASE_URL='{{test_db_url}}' cargo test --workspace --locked
crate-age:                          # decision 16; uses the network, so it is not part of `check`
gen: ...                            # decision 20
_check-gen: ...                     # decision 21
_check-ts-rs-graph:                 # RF-B3: fails if ts-rs is in the server's normal graph
deny:                               # `cargo deny check bans`; in local, skips with a notice if not installed
db-up:                              # docker compose -f infra/compose.dev.yml up -d --wait (checks `docker info` first)
db-down:                            # docker compose -f infra/compose.dev.yml down (keeps the volume)
db-reset:                           # down -v + up: drops the volume and leftover test databases
dev: _web-prepare                   # decision 24
```

- **Orden** (RF-B19): fmt → clippy → `db-up` → tests → frescura de `gen` → `ts-rs` fuera del grafo. `db-up` va después de clippy y no como dependencia de la receta, para que un error de formato salga sin esperar a Docker. `just` corta en la primera línea que falla.
- `check-web` queda **antes** que `check-rust`: es lo más rápido y no necesita Docker. Renombro `_check-web` a `check-web` y agrego `check-rust` públicos porque los jobs `web` y `rust` de la fase C (RF-C9, RF-C10) los llaman por separado. `just check` sigue siendo el mismo camino en local y en CI.
- **`_check-gen` necesita Node y pnpm** (Prettier). Lo anoto para el plan de C: el job `rust` instala Node y corre `pnpm install --frozen-lockfile` (nota 4 de devops).
- **No entra:** `just deny` (en local es opcional; en CI corre siempre, RF-B19) ni Playwright.
- **Docker** es requisito de `just check` desde esta fase (spec, "Áreas de preocupación" 7). `db-up` primero corre `docker info` y, si falla, sale con un mensaje que lo explica. El `pre-push` de la fase C hereda la dependencia; `CLAUDE.md` lo documenta.
- **Tiempo:** la primera corrida compila el workspace (unos minutos en una Mac ARM); con caché, `check-rust` debería quedar por debajo de 1 min más lo que tarden los tests de integración (los plazos del WS y del health están acortados; el más largo es el de apagado, de unos segundos).

## Riesgos

- **`just check` y el `pre-push` necesitan Docker.** → `db-up` con mensaje claro; es lo que acepta la spec ("Áreas de preocupación" 7). El puerto 5432 puede estar ocupado por un Postgres local: `DEV_PG_PORT` lo cambia para el compose y para `test_db_url`.
- **Plazo de logs (lo más delicado para el aviso).** Con la lista blanca de la decisión 4, la línea de `/api/health` es más corta que los ~200 B que supone la spec. Mi estimación previa: unos 150 B de línea, unos 175 B escapada y unos 75 B de envoltorio, unos 250 B en total, 1.44 MB/día, **unos 21 días**. Eso queda justo en el límite de "menos de 3 semanas". → El paso 14 mide la línea real, sin rellenarla para que cuadre. **Regla (security, Minor 7):** si el máximo medido es de **19 días o más**, el aviso dice "menos de 4 semanas"; solo por debajo de 19 días dice "menos de 3 semanas". El margen cubre la variación de la latencia y del `ts`. Lo marco en el PR para security-reviewer, y la fase C lo confirma con `prod-local` como ya prevé la spec. El valor del aviso tiene que ser cierto, no el de la spec.
- **Fugas a los logs desde crates de terceros** (sqlx, hyper, tungstenite, un pánico). → Decisión 4: lista blanca de campos, eventos externos sin mensaje, topes del filtro y *panic hook*; los canarios del paso 11 corren con `info` y con `debug` global, y miran stdout **y** stderr del proceso real.
- **Tests dependientes del tiempo** (ping/pong, inactividad, caché, apagado). → Todos los plazos son configuración que el test acorta (decenas o cientos de ms), con márgenes de al menos 3× entre lo que se espera y lo que se mide; nada usa `sleep` para sincronizarse si puede esperar un evento (la línea `server.listening`, un frame de cierre). Si un test resulta intermitente, se agranda el margen y se anota; nunca se marca `#[ignore]`.
- **Conexiones mejoradas a WS fuera del apagado de hyper.** → Decisión 11 y el test del binario con `SIGTERM`.
- **Comportamiento de tungstenite al pasar los 4 KB** (No verificado b). → El paso 9 lo observa primero. El test acepta `Close(1009)` o el cierre de la conexión sin eco antes de 1 s, como permite RF-B13, y la desviación anota cuál de los dos ocurre.
- **sqlx 0.9 es una versión mayor reciente** y su feature `tls-rustls-ring` es ahora un alias. → `cargo deny check bans` en el paso 1 y la comprobación de `cargo tree` sin `openssl`/`aws-lc`. Si 0.9 trae algo prohibido, paro y el plan vuelve a revisión (el respaldo sería 0.8.6, que necesita OK del architect).
- **Unificación de features que meta `ts-rs` en el server.** → `_check-ts-rs-graph` en `just check` (RF-B3); `protocol` solo activa `ts` con `--features ts` explícito.
- **Salida de `ts-rs` que cambie entre versiones** (formato, cabecera, extensión de los imports). → Versión exacta; la frescura de `gen` lo detecta y un bump de Dependabot regenera los archivos en el mismo PR.
- **Toolchain.** El owner tiene 1.93.0 activa por `rustup`; `rust-toolchain.toml` hace que `cargo` use 1.99.1 en el repo, y la primera vez descarga la toolchain y el `std` de musl. → Paso 0 explícito. Si `cargo` del `PATH` no es el *proxy* de rustup, el paso 0 lo detecta (`rustup show active-toolchain`).
- **bash 3.2 de macOS** en las recetas *shebang*. → Decisión 24: sin `wait -n`, arrays asociativos ni `mapfile`.
- **Bases de test que quedan** si un test aborta. → `just db-reset`; los nombres llevan el prefijo `torneo_test_`.
- **`/ws` sin límites por IP en las imágenes de producción.** No es de esta fase resolverlo; la spec bloquea el primer despliegue hasta los límites anónimos de ADR-0008 D6 y D7 (RF-C18). → En B, el ping/pong y la inactividad acotan un socket mudo a 120 s, y el tope global es configurable.
- **Slowloris sobre HTTP** (cabeceras lentas). No está en la spec. → El `header_read_timeout` está en "No verificado (e)" y el paso 7 lo prueba con cabeceras parciales. El tope de conexiones HTTP que no son WS (hoy solo `/ws` tiene tope) queda para la feature de auth, junto con el rate limit por IP; lo anoto para su spec (security, Minor 5).
- **La regla de 3 días en Rust.** cargo no tiene antigüedad mínima nativa y `=x.y.z` solo cubre las directas. → Decisión 16: `just crate-age` sobre **todo** `Cargo.lock` antes del primer build y en cada cambio del lock. Entre corridas manuales no hay nada automático; desde la fase C, Dependabot aplica el `cooldown` de 3 días a los PRs de `cargo` (nota 8 del architect).
- **Tamaño del PR.** 20 RF, la parte de web incluida. → Commits por paso y orden de lectura igual al de este plan.

## Alternativas descartadas

- **`tower_http::trace::TraceLayer` para la línea de petición:** su span por defecto lleva `uri` y `version`, y con `debug` global se escribiría; personalizarlo hasta la lista de RF-B15 es más código que el middleware propio (decisión 5).
- **El formateador JSON de `tracing-subscriber`:** nombres de campo fijos (`timestamp`), `fields` anidados y sin lista blanca (decisión 4).
- **`reqwest` o un cliente HTTP escrito a mano para `server healthcheck`:** `reqwest` agrega un grafo grande; escribir HTTP a mano es frágil. hyper ya está en el grafo (decisión 12).
- **`clap` y `dotenvy`:** dos subcomandos no justifican `clap`, y leer un `.env` dentro del binario de producción sorprende; lo carga `just dev` (decisiones 2 y 24).
- **`set dotenv-load` en el `justfile`:** exportaría `DATABASE_URL` a todas las recetas (nota 5 de devops).
- **`#[sqlx::test]`:** útil en el proceso, pero los tests del binario necesitan la misma base fuera de él (decisión 18).
- **`query!` con `.sqlx/` desde ya:** no hay consultas que verificar (decisión 14).
- **Un código `ws.upgrade_required` para el `GET /ws` sin upgrade:** el architect prefirió el 4xx de axum con cuerpo vacío (enmienda 2026-10-23 b).
- **`anyhow` en `main.rs`:** formatear un `anyhow::Error` es la forma más fácil de volcar un `DATABASE_URL` en stderr; `thiserror` y `exit_code` bastan (decisión 1).
- **`acquire().await` en el semáforo de `/ws`:** cola sin límite en memoria (decisión 10).
- **`ApiError { error: ApiErrorCode }` con los códigos `ws.*` duplicados en `ApiErrorCode`:** dos fuentes del mismo código (decisión 7).
- **Generar a un directorio temporal para la frescura:** Prettier no encuentra su configuración fuera del repo y dentro de `target/` lo ignora (decisión 21).
- **`git diff --exit-code` en `just check` local:** obligaría a hacer `git add` antes de cada `just check` (decisión 21). En CI es equivalente.
- **tokio 1.54.x:** tiene menos de 3 días con su corrección (decisión 16).
- **Probar `/dev/status` dentro de `just e2e`:** `node build` no tiene proxy a `/api` ni a `/ws` y la CI no tiene server; queda como e2e opcional contra `just dev` y evidencia en el PR.

## Respuestas del architect (incorporadas)

Las tres preguntas de la primera versión del plan quedaron resueltas y están en la enmienda 2026-10-23 (b) de la spec:
1. **`GET /ws` sin upgrade:** el 4xx de axum, cuerpo vacío y sin `Content-Type`; orden `Origin` → upgrade → semáforo (decisión 10, fila B13).
2. **Cierre por ritmo:** `Close(1008, "ws.rate_limited")`, con `WsErrorCode::RateLimited`, su llave `ws_close_reason_rate_limited` y su caso en `closeReasonKey` (decisión 10, filas B5, B7 y B13).
3. **`BIND_ADDR`** (renombrada desde `HOST`, aceptado en la ronda 2) como `IpAddr` con sus dos condiciones (decisión 3, filas B8 y B11) y **`ErrorCode` sin etiquetar** con el unit de no solapamiento (decisión 7, fila B5).

## Pruebas (cómo se demuestra que funciona)

- [ ] `just check` en verde (salida pegada en el PR), `just e2e` en verde y `cargo deny check bans` en verde.

| RF | Prueba | Nivel |
|---|---|---|
| B1 | `cargo metadata --format-version 1 --no-deps` lista solo `server` y `protocol`; `rustup show active-toolchain` = 1.99.1 con el target musl; revisión del perfil `release`. En CI desde la fase C | PR |
| B2 | `cargo deny check bans` en verde; `cargo tree -p server -e normal --locked` sin `openssl`, `openssl-sys` ni `aws-lc-sys`; `grep -n global_allocator apps/server/src/main.rs` | PR · CI (C) |
| B3 | `_check-ts-rs-graph` en `just check`: `cargo tree -p server -e normal` sin `ts-rs`; `cargo tree -p protocol -e normal --prefix none` (sin features) sin `ts-rs` ni `tokio` (los derive de `serde` traen `syn` y `proc-macro2`, así que no se exige "solo `serde`"; nota 9 del architect) | `just check` |
| B4 | Tests de forma JSON (paso 2); `gen-protocol-ts.mjs` falla con el fixture que contiene `bigint` y con un `u64` agregado a mano (paso 3); revisión de "Tipos compartidos" en `docs/architecture.md` | unit · PR |
| B5 | `crates/protocol`: un test por variante con la cadena JSON exacta (incluido `ws.rate_limited`), ida y vuelta, `as_str()` igual a serde en todos los códigos, `ApiError { error: ErrorCode }` con un código de cada enum, y el unit de **no solapamiento** entre las cadenas de `ApiErrorCode` y `WsErrorCode` (cada cadena deserializa a la variante correcta de `ErrorCode`) | unit |
| B6 | `_check-gen` en `just check` (decisión 21); cabecera en cada archivo generado; barril con `export type *`; idempotencia de `just gen` | `just check` · unit |
| B7 | `svelte-check` con los tipos de `@torneo/protocol`; Vitest de `health.ts` con los dos fixtures de `HealthResponse` (200 `ok` → `{ kind: 'ok', version }`; 503 `db_unavailable` → `{ kind: 'db_unavailable' }`), una respuesta con forma inesperada y un `fetch` que rechaza (→ `network_error`); Vitest de `echo.ts` con un `WebSocket` falso: ida y vuelta, `closeReasonKey` para cada `WsErrorCode` (incluido `ws.rate_limited` → `ws_close_reason_rate_limited`) y para una razón vacía o desconocida; el `switch` exhaustivo no compila si falta un código (lo prueba `svelte-check` al agregar el código en Rust y regenerar); `check-i18n` sobre las llaves nuevas | unit · `just check` |
| B8 | Unit de `Config::from_lookup` (paso 5). Integración con el binario: sin `PUBLIC_ORIGIN` sale con 1 y stdout trae exactamente una línea con `"event":"config.invalid","var":"PUBLIC_ORIGIN"`; con `PUBLIC_ORIGIN=https://x.example/ruta-canario`, igual y sin el canario en stdout ni stderr; con `BIND_ADDR=no-es-ip`, `config.invalid` con `var: "BIND_ADDR"`; con `BIND_ADDR=192.0.2.1` (IP válida que no es de la máquina), el `bind` falla: sale con 1 y stdout trae `{"event":"server.bind_failed","error_kind":"addr_not_available"}` sin la dirección | unit · integración |
| B9 | `tests/startup.rs`: (1) `connect_with_retry` contra un puerto cerrado, con `interval` 50 ms y `deadline` 300 ms, devuelve `DbUnavailable` entre 300 y 1,000 ms, y `exit_code` lo convierte en 1; (2) binario con el proxy cortado: a los 3 s no hay línea `server.listening` y el puerto no acepta conexiones; al restaurar el proxy aparece la línea, y `_sqlx_migrations` de la base del test contiene la versión `20261023000000` con `success = true`; (3) en el proceso, el pool tiene `max_connections = 8` | integración |
| B10 | `tests/health.rs` con el proxy y `cache_ttl` = 100 ms: 200 `{"status":"ok","version":"dev"}` con `Cache-Control: no-store` → `cut()` y espera de 300 ms → 503 `{"status":"db_unavailable"}` → `restore()` y espera de 300 ms → 200, sin reiniciar el server; 50 peticiones concurrentes con la caché vencida producen **una** llamada al `DbProbe` (contador, decisión 8); con un `DbProbe` que tarda 3 s, la respuesta es 503 en ≤ 1.2 s (`db_timeout`) | integración |
| B11 | `tests/healthcheck.rs` con el binario real: `server healthcheck` sale con 0 con el server arriba y la BD arriba; con 1 con el proxy cortado (tras el TTL de la caché del server, que en este test lanza el binario con su valor por defecto de 1 s, así que espera 1.5 s); con 1 sin server (`PORT` de un puerto cerrado) en < 2.5 s; con 1 contra un servidor de prueba que no responde nunca, en ≤ 2.5 s (timeout de 2 s). La petición lleva `Host: 127.0.0.1:<PORT>` (el servidor de prueba lo verifica) y el healthcheck no lee el cuerpo (un servidor de prueba que responde 200 con un cuerpo infinito no lo hace esperar más allá de los 2 s ni crecer en memoria). En ningún caso escribe en stdout **ni en stderr**. El arranque con `BIND_ADDR=192.0.2.1` sale con 1 y `server.bind_failed` (fila B8); el caso de un `BIND_ADDR` válido de la máquina que no sea loopback ni `0.0.0.0` (healthcheck → 1) queda documentado, no probado | integración |
| B12 | `tests/http.rs` en el proceso: `GET /nada?x=1` → 404 `{"error":"http.not_found"}`; `POST /api/health` y `DELETE /cualquier` → 405 `{"error":"http.method_not_allowed"}` con `Allow: GET, HEAD, OPTIONS`; `OPTIONS /api/health` → 405 con el mismo cuerpo (`method_not_allowed_fallback`); `OPTIONS /api/health` y `GET /api/health` con `Origin: https://evil.example` y `Access-Control-Request-Method: POST` → ninguna cabecera `Access-Control-Allow-*`; `nosniff` y HSTS `max-age=31536000` (sin `includeSubDomains`) en una 200, una 404, una 405 y la 403 del WS; `HEAD /api/health` → 200; una conexión TCP que manda `GET / HTTP/1.1\r\nHost: x\r\n` y nunca termina las cabeceras queda cerrada por el server tras el `header_read_timeout` (No verificado e) | integración |
| B13 | `tests/ws.rs` con `WsConfig` acortada: `Origin` ausente y ajeno (también `http://localhost:5173/` con barra y `HTTP://localhost:5173`) → 403 `{"error":"ws.origin_forbidden"}` sin upgrade; **orden** (enmienda b): `Origin` ajeno sin cabeceras de upgrade → 403 (Origin va primero); `GET /ws` y `HEAD /ws` con `Origin` válido sin upgrade → **cualquier 4xx** (el que dé axum; `HEAD` da 405), cuerpo vacío, sin `Content-Type`, con `nosniff` y HSTS, y sin consumir permisos del semáforo (con el tope lleno siguen dando 4xx y no 503); eco encendido → `{"type":"echo","payload":"hola"}` vuelve igual; eco apagado (sin la variable, y con `1` + `PUBLIC_ORIGIN` `https://`) → `Close(1008, "ws.echo_disabled")`; 4097 bytes → `Close(1009)` o cierre sin eco (riesgo); 11 mensajes en 10 s → `Close(1008, "ws.rate_limited")`; 11 pings del cliente en 10 s → `Close(1008, "ws.rate_limited")` (decisión 10); frame binario → `1003`; texto no JSON y `{"type":"subscribe"}` → `Close(1008, "ws.invalid_message")`; `WS_MAX_CONNECTIONS=2` con los dos sockets abiertos (tope lleno): el tercer upgrade → 503 `{"error":"ws.capacity"}` **de inmediato** (en < 200 ms, sin esperar a que se libere un permiso: prueba de `try_acquire_owned`), y al cerrar uno el siguiente entra; cliente que no lee el socket → al leer después encuentra `Close(1008, "ws.pong_timeout")` y la conexión TCP cerrada; cliente que contesta pings sin mandar mensajes → `Close(1008, "ws.idle_timeout")`; con `close_grace` acortado, un cliente que no completa el cierre ve cortada la conexión TCP. Unit de la ventana de ritmo y de la decisión del eco | integración · unit |
| B14 | `tests/shutdown.rs` con el binario real: un cliente WS conectado; `kill -TERM <pid>` → el cliente recibe `Close(1012)` y el proceso sale con 0 en < 10 s; después del `SIGTERM`, una conexión nueva al puerto es rechazada. Igual con `SIGINT` | integración |
| B15 | `tests/logs.rs` con el binario real (`env_clear`), tres veces: `RUST_LOG=info`, `RUST_LOG=debug` y `RUST_LOG=debug,sqlx=trace,tungstenite=trace` (security, Minor 4), eco encendido: peticiones con `Cookie`, `Authorization`, `X-Internal-Secret`, `X-Client-Ip` y `X-Forwarded-For` de valores canario únicos a `/api/health`, a `/no-existe?q=<canario>` y al handshake de `/ws` con `Origin: https://<canario>.example`, un eco con `payload` canario, una petición con un método de extensión (`PURGE /api/health`, que da 405 y se registra con `method: "other"`), y un `GET /ws` con `Origin` válido sin upgrade (su línea lleva `route: "/ws"`). También se comprueba la línea `warn` `ws.echo_enabled` al arrancar. Al terminar (proceso detenido), **stdout y stderr completos**: ninguna línea contiene un canario, la query ni el `payload`; cada línea es JSON con solo campos de la lista blanca; hay exactamente una línea `http.request` por petición HTTP, con `method` del conjunto cerrado, `route` `/api/health`, `unmatched` o `/ws` y un `request_id` UUID distinto en cada una; ninguna línea de un `target` externo lleva más que `ts`, `level`, `target` y `event`. Arranque con `DATABASE_URL=postgres://torneo:<canario>@127.0.0.1:1/torneo` detenido a los 5 s: hay líneas `db.unavailable` y ninguna contiene el canario ni `127.0.0.1:1` | integración |
| B16 | En el mismo test: `GET /api/health` produce una línea `info` con `route":"/api/health"` | integración |
| B17 | Evidencia en el PR (criterio B1 del intent): `just dev` desde un clon limpio con `apps/{server,web}/.env` copiados de los `.env.example`, migraciones aplicadas (`psql … -c 'select version from _sqlx_migrations'`), `/es/dev/status` con el server `ok` y el eco; Ctrl-C detiene los dos procesos y Postgres sigue arriba; `just db-down` lo baja. `docker port` muestra `127.0.0.1` | manual · PR |
| B18 | Revisión de los dos `.env.example`: valores de desarrollo, sin secretos reales | PR |
| B19 | `just check` en verde (salida pegada); una prueba a propósito por paso nuevo, pegada en el PR: un error de `rustfmt` falla en `cargo fmt`, un tipo de `protocol` cambiado sin `just gen` falla en `_check-gen` con el mensaje de árbol desactualizado, un `u64` en un tipo exportado falla en `_check-gen` con el mensaje de `gen` fallido y deja el árbol intacto, y `ts-rs` puesto como dependencia normal del server falla en `_check-ts-rs-graph` (los tres se revierten) | PR |
| B20 | Revisión de `CLAUDE.md`, `docs/architecture.md` y del aviso (security-reviewer), con el cálculo del paso 14 | PR |

- [ ] **Evidencia en el PR:** salidas de `just check`, `just e2e` y `cargo deny check bans`; las tres pruebas de B19; `just dev` y las capturas de `/dev/status` (a 375 px y 1280 px, `es` y `en`, los dos temas); el cálculo del plazo de logs; el extracto de stdout de los canarios.

## Estimación de CI

Sin cambios en esta fase: no hay workflows hasta la fase C. Para C, `check-rust` en un runner con caché caliente debería tardar unos 2–3 min (clippy dos veces, `db-up` con la imagen de Postgres ya descargada por la caché de Docker o en unos 10 s si no, y los tests de integración con plazos acortados), más Node y `pnpm install` para `_check-gen`. La compilación musl de RF-C9 va aparte.

## Desviaciones durante la implementación

1. **Paso 1, versiones (2026-10-23).** Volví a comprobar la tabla contra crates.io: todas las directas existen, no están retiradas y tienen 3 días o más. tokio 1.54.1 (2026-10-21) y uuid 1.29.0 (2026-10-23) siguen sin cumplir los 3 días, así que se quedan 1.53.3 y 1.28.0.
2. **Paso 1b, transitivas bajadas con `cargo update -p <crate>@<versión> --precise <anterior>`.** `cargo generate-lockfile` resolvió 222 paquetes de crates.io; la primera corrida de `just crate-age` marcó 15 con menos de 3 días. Bajadas (todas a la versión anterior más reciente con ≥ 3 días):

   | Crate | Del lock | Bajada a | Publicada |
   |---|---|---|---|
   | `cc` | 1.10.0 | 1.9.0 | 2026-10-19 |
   | `ctutils` | 0.4.5 | 0.4.3 (0.4.4 también es reciente) | 2026-10-05 |
   | `icu_collections`, `icu_locale_core`, `icu_normalizer`, `icu_normalizer_data`, `icu_properties`, `icu_properties_data` | 2.4.0 | 2.3.0 | 2026-08-13 |
   | `icu_provider` | 2.4.0 | 2.3.1 | 2026-08-20 |
   | `smallvec` | 1.16.4 | 1.16.3 | 2026-10-10 |
   | `tinystr` | 0.8.5 | 0.8.4 | 2026-08-13 |
   | `webpki-roots` | 1.0.11 | 1.0.10 | 2026-10-19 |
   | `yoke` | 0.8.4 | 0.8.3 | 2026-06-03 |
   | `zerovec` | 0.11.9 | 0.11.8 | 2026-08-19 |
   | `zerovec-derive` | 0.11.7 | 0.11.6 | 2026-08-20 |

   La familia ICU 2.4 se pide entre sí con `~2.4.0`, así que se bajó de arriba hacia abajo: `icu_normalizer` primero (cargo bajó con él `icu_collections`, `icu_normalizer_data`, `icu_properties` e `icu_properties_data`), y después `icu_provider`, `icu_locale_core`, `tinystr`, `zerovec`, `zerovec-derive` y `yoke`. Al bajar `webpki-roots`, cargo también bajó `rustls-pki-types` 1.15.1 → 1.14.0 y `zeroize` 1.9.1 → 1.8.2 (más viejas; las dos pasan la regla). La segunda corrida: `222 crates.io package(s) checked, 0 younger than 3 days, 0 problem(s)`, salida 0.
3. **Paso 1c.** `cargo deny check bans` da `bans ok`, sin `openssl`, `openssl-sys` ni `aws-lc-sys`. Quedan avisos `duplicate` (permitidos por `multiple-versions = "warn"`): `block-buffer`, `cpufeatures`, `crypto-common`, `digest`, `getrandom` (3), `hashbrown`, `rand`, `rand_core`, `sha2` y `syn`.
4. **Paso 3: `_check-gen` y `compare-trees.mjs` entran en el paso 3** (el plan los ponía con el resto del `justfile` en el paso 12). No necesitan Docker, y con Docker apagado al empezar la fase B adelantarlos permitió verificar la frescura de `gen` desde ya. `check-rust` (paso 12) solo los llama. Los mensajes de `_check-gen` van en inglés, como el resto de la salida de los scripts del repo: `just gen failed: the tree was not touched` y `protocol-ts was out of date: it was regenerated; review and commit`. La dependencia `"@torneo/protocol": "workspace:*"` de `apps/web` también entra en el paso 3, que es cuando `pnpm install` enlaza el paquete.
5. **Paso 3: prueba del `u64`.** Agregar un campo `u64` a `HealthResponse::Ok` rompe antes la compilación de los tests de forma JSON, así que no llega a `ts-rs`. La prueba se hizo con un `struct Probe { uptime: u64 }` temporal exportado. `just gen` falla con `generated/Probe.ts: contains \`bigint\``, y `_check-gen` sale con el mensaje de `gen` fallido y deja el árbol igual (mismo `shasum`). Se revirtió.
6. **Paso 5: topes del filtro como un filtro aparte.** La decisión 4 ponía los topes como directivas de `EnvFilter` agregadas después de `RUST_LOG`. Eso funciona para `RUST_LOG=sqlx=trace` (con la misma especificidad gana la última directiva), pero no para una directiva más específica: con `RUST_LOG=sqlx::query=trace`, `sqlx::query` le gana a `sqlx=warn`. Por eso los topes son un `Targets` (`with_default(TRACE)` más `warn` para `sqlx`, `hyper`, `hyper_util`, `tungstenite`, `tokio_tungstenite` y `h2`), combinado con el `EnvFilter` de `RUST_LOG` con `FilterExt::and`. Un evento solo pasa si lo dejan pasar los dos. El unit cubre `sqlx=trace,hyper=trace`, `trace` y `debug,sqlx::query=trace,tungstenite=trace`.
7. **Paso 5: esquema de `DATABASE_URL`.** `PgConnectOptions::from_str` de sqlx 0.9 acepta un URL con cualquier esquema (`mysql://…` lo parsea sin error). Un test lo reprodujo y `config.rs` exige ahora `postgres` o `postgresql` antes de parsear (`config.invalid` con `var: "DATABASE_URL"` si no).
8. **Paso 7: `header_read_timeout` (No verificado e) y servidor propio.** `axum::serve` 0.8.9 crea el `Builder` de hyper sin *timer* (`Builder::new(TokioExecutor::new())`), así que el `header_read_timeout` por defecto de hyper (30 s, que solo actúa con *timer*) no se aplica, y `axum::serve` no permite configurarlo. Se tomó el respaldo del plan con una variante: en vez del `auto::Builder` de `hyper_util` (que trae HTTP/2), `app::serve_listener` acepta en su propio bucle y sirve cada conexión con `hyper::server::conn::http1::Builder`, con `TokioTimer`, `header_read_timeout` (10 s por defecto, `HttpConfig` en `Settings`; el test lo acorta a 300 ms) y `with_upgrades()` para el WebSocket. El apagado deja de aceptar, pide `graceful_shutdown` a cada conexión y las espera con un `TaskTracker`. Eso activa `server` en hyper y `server`/`service` en hyper-util, que axum ya activaba: `Cargo.lock` no cambia. `tests/http.rs` comprueba que una conexión con cabeceras a medias se cierra en menos de 2 s.
9. **Paso 8: calentamiento del binario en los tests de tiempo.** En macOS, la primera ejecución de un binario recién enlazado tarda unos 0.5 s más porque el sistema lo escanea. Eso hacía que `server healthcheck` contra un servidor que nunca responde midiera 2.6 s en el test, aunque el binario solo sale en 2.01 s (medido fuera del test). `tests/support/bin.rs` tiene `warm_up()`, que ejecuta el binario una vez (con un argumento inválido) antes de medir. Es un ajuste del arnés, no del código. Los casos de B8 (`config.invalid`, `server.bind_failed` con `192.0.2.1` y `cli.invalid` con salida 2) quedaron en `tests/startup.rs`, porque la lista de archivos de test del plan no tiene uno de configuración.
10. **Paso 9: lo que hace tungstenite 0.29 con 4097 bytes (No verificado b).** Devuelve `Error::Capacity(MessageTooLong)` al leer la cabecera del frame, sin invalidar el estado de la conexión, y el server alcanza a mandar `Close(1009)`: el test observa `Close(1009, "")` en todas las corridas. El test sigue aceptando también el corte sin cierre, como permite RF-B13. Para reconocer ese error detrás de `axum::Error` (`into_inner()` y `downcast_ref`), `tungstenite` =0.29.0 (sin features por defecto) es ahora dependencia directa del server. Ya estaba en el grafo por axum, la misma versión: el lock solo agrega la arista `server → tungstenite`, y `just crate-age` sigue en 0.
11. **Paso 9: test del plazo de pong con un cliente crudo.** Con el cliente de tokio-tungstenite, el test fallaba 2 de cada 15 veces. Al leer tarde, el cliente intenta contestar los pings atrasados con pongs. El server ya cerró, así que esa escritura provoca un RST y el kernel descarta los datos sin leer, incluido el `Close(1008)`. El test usa ahora `silent_client_frames` (`tests/support/ws.rs`): un `TcpStream` que hace el *handshake* a mano, nunca vuelve a escribir y parsea los frames del server hasta el EOF. Comprueba que hay solo pings, después `Close(1008, "ws.pong_timeout")`, y que el server cierra la conexión TCP sin respuesta del cliente (el caso de `close_grace`). 20 corridas seguidas en verde.
12. **Paso 4, imagen de Postgres.** El paso 4 se hizo el 2026-10-24, así que se usa `postgres:17.10-alpine@sha256:742f40ea…` como dice la tabla. `docker buildx imagetools inspect` confirmó el digest, que es un índice con `linux/amd64` y `linux/arm64/v8`, creado el 2026-07-07. El Docker de la máquina es Colima (2 CPU, 2 GB); los tests de integración entraron sin problemas en esos recursos. El mensaje de `db-up` con Docker apagado está en inglés, como el resto de la salida de las recetas: ``Docker is not responding: `just check` needs Docker since phase B``.
13. **Paso 12, `just dev`.** `trap 'trap - INT TERM; kill 0' INT TERM` en vez de `trap 'kill 0' INT TERM`, para que la señal no se vuelva a atrapar. Con un SIGINT al grupo (lo que hace Ctrl-C), `just dev` sale con 143, y `pgrep -f target/debug/server` y `pgrep -f vite` quedan vacíos con Postgres arriba. `just fmt` también corre `cargo fmt --all`.
14. **Paso 13, e2e opcional.** `e2e/dev-status.spec.ts` vive en `e2e/` como dice la tabla, pero el `playwright.config.ts` principal lo excluye con `testIgnore` para que no entre en `just e2e`. `playwright.dev.config.ts` lo corre solo a él contra `just dev`. Con eso queda verificado el "No verificado (d)": sin `securitypolicyviolation` en Chromium ni en WebKit con `ws://localhost:5173/ws`.
15. **Paso 14, plazo de logs.** Medido el 2026-10-24 con `target/release/server` y 100 `curl` a `/api/health`. Cada línea mide 192 B, siempre igual: el `ts` tiene microsegundos fijos, el `request_id` es un UUID y `latency_ms` tiene una cifra. Con el envoltorio de `json-file` (`time` con nanosegundos) son 290 B. Con 5,760 healthchecks al día son 1,670,400 B/día, y 3 × 10 MiB (31,457,280 B) alcanzan para 18.8 días (18.0 si se cuentan 30 MB decimales; 18.7 con una latencia de 3 cifras). Como el máximo queda por debajo de 19 días, el aviso dice "menos de 3 semanas". La línea real (192 B) sale más larga que la estimación del plan (~150 B) y cerca de los ~200 B de la spec.
16. **Paso 15: bug del apagado encontrado en el cierre.** En el worktree limpio, `sigterm_closes_sockets_with_1012_and_exits_with_zero` falló una vez: el cliente vio la conexión cortada sin `Close(1012)`. Con la máquina muy cargada (*load average* de 700 a 1000, por otra sesión de macOS), un test en el proceso con 100 intentos (señal justo después del *handshake*) lo reprodujo en casi todas las corridas. La causa: en `app::run_until_shutdown`, al llegar la señal quedan listas a la vez `stop.cancelled()` y el fin de `serve_listener`, que también mira la señal. `tokio::select!` sin `biased` elegía a veces la segunda rama y retornaba sin cancelar los sockets; en el binario, `main` terminaba y el runtime cortaba el WebSocket sin frame. Ahora el `select!` es `biased` y la secuencia de cierre (cancelar los sockets, `close()` y `wait()` del `TaskTracker` con el tope de 9 s) corre siempre, incluso si el server terminara sin señal. Antes de dar con la causa probé si hyper perdía el *upgrade* durante `graceful_shutdown`; la instrumentación lo descartó y ese cambio no quedó. La parte posterior al `bind` de `serve()` pasó a `run_until_shutdown`, que es pública para ese test. Con la corrección: 15 corridas seguidas de 100 intentos sin fallos.

## Revisión

### architect — 2026-10-23 — REQUEST_CHANGES

**Verifiqué:** que el frontmatter es válido; que el plan cubre los RF-B1 a B20, los RNF, la estrategia de pruebas y las enmiendas 2026-10-22 y 2026-10-23; que respeta ADR-0001, 0002, 0003, 0006 (D3, D8, D10) y 0008 (D3, D4, D6, D7); y que encaja con el código de la fase A ya fusionado (`justfile`, `.prettierignore`, `vite.config.ts`, `check-kit-config.mjs`, validadores). La validación de `PUBLIC_ORIGIN` coincide con la de web. El proxy de Vite no dispara `check-kit-config`. `protocol` queda puro. Las versiones son coherentes; las verifica el paso 1. Están bien el formateador propio, el apagado con 1012 (tope de 9 s), la división `check-web` y `check-rust` y que `.prettierignore` deje de ignorar lo generado.

**Respuestas a las preguntas del plan:**
1. **`GET /ws` sin upgrade:** se queda el estado de axum con cuerpo vacío, sin código nuevo. El orden de los chequeos es `Origin` → upgrade → semáforo. Agregar la prueba en B13: `Origin` válido sin `Upgrade` debe dar 4xx, cuerpo vacío, `nosniff`, HSTS y una línea `http.request` con `route: "/ws"`.
2. **Cierre por tasa:** se agrega `ws.rate_limited` a `WsErrorCode`. Va como `Close(1008, "ws.rate_limited")` y lleva su llave `ws_close_reason_rate_limited`. Esto cambia la spec: lo escribe el architect como enmienda menor.
3. **`HOST`:** se acepta, parseado como `IpAddr`, con dos condiciones. Hay que documentar que `server healthcheck` supone `0.0.0.0` o `127.0.0.1`. Y `check-compose.mjs` lo prohíbe en `server` (fase C). Cambia la tabla de variables de la spec.
4. **`ErrorCode` sin etiquetar:** se confirma. Agregar un unit que verifique que los conjuntos de códigos no se solapan. Los códigos de auth futuros van en una variante nueva. Cambian dos cosas de la spec: RF-B5 queda como `ApiError { error: ErrorCode }`, y RF-B4 usa `rename` por variante.

**Bloqueantes:**
1. **`just gen` deja archivos huérfanos.** `ts-rs` no borra el `.ts` de un tipo borrado, así que el barril lo sigue exportando y el `diff` no lo detecta. Cambio: `just gen` vacía `generated/*.ts` antes de exportar, y se agregan tests que fallan si un tipo se borra sin regenerar.
2. **La lista blanca contradice al *panic hook*.** El hook escribe `file` y `line`, que no están en la lista. Cambio: agregar los dos a la lista, con valores solo de `Location`. Escribir además que el formateador no recorre spans y que escapa los valores con `serde_json`.
3. **Faltan las respuestas 1 a 4 en el plan.** El paso 2 no empieza hasta que la enmienda de la spec esté aprobada.

**No bloqueantes:**
4. **`TaskTracker`:** tomar el token antes de `on_upgrade` y usar `select!` con `biased` sobre `token.cancelled()`.
5. **`anyhow` en `main`:** quitarlo. Si se queda, que nunca se formatee y que `main` no devuelva `Result`.
6. **Tests del binario:** usar `Command::env_clear()` y dejar solo `PATH` más las variables explícitas.
7. **Topes de `RUST_LOG`:** un unit con `RUST_LOG=sqlx=trace,hyper=trace` que compruebe que siguen mandando los topes.
8. **Digest de Postgres:** el elegido tiene 1 día, así que se usa el anterior o se justifica. Anotar en Riesgos que la regla de 3 días cubre solo las directas.
9. **Comandos:**
   - B3: comprobar "sin `ts-rs` ni `tokio`", porque los derive de `serde` traen `syn`.
   - Paso 14: agregar `--release`.
   - La receta `test` llama a `db-up`.
10. **Paso 13:** sus verificaciones dependen de los pasos 9 y 12.
11. **`crates/protocol/bindings/`:** agregarlo a `.gitignore` o documentarlo.
12. **Tabla de la decisión 9:** verificar el `warn` `ws.echo_enabled`.

### security-reviewer — 2026-10-23 — REQUEST_CHANGES

**🤖 security-reviewer**

**Verifiqué:**
- Leí el plan completo y lo contrasté con:
  - RF-B1 a RF-B20, los RNF de "Seguridad", "Diseño → Logs y plazo" y "Diseño → WebSocket";
  - mis rondas anteriores;
  - ADR-0008 D3, D4, D6 y D7, y ADR-0006 D3 y D11;
  - el aviso de privacidad.
- **Lo que aprobé en rondas anteriores está en el plan:** ping/pong, idle, `close_grace`, 4 KB en el protocolo, eco solo en loopback, 405, caché de health y redacción de `X-Client-Ip`/`X-Forwarded-For`.
- **Otras piezas que reviso y están bien:**
  - `ConfigError` sin el valor;
  - el formateador con lista blanca, más los topes del filtro y un *panic hook* sin mensaje;
  - `server healthcheck` con hyper;
  - `deny.toml`;
  - el digest de Postgres;
  - el proxy de pruebas fuera de `src/`;
  - el WebSocket relativo, que se resuelve a `wss:`;
  - el plazo de logs medido sin relleno.

**Important**
1. **La regla de 3 días solo cubre las dependencias directas.** `cargo generate-lockfile` resuelve las ~200 transitivas a su versión más reciente, y su `build.rs` o *proc-macro* se ejecuta en la Mac del owner en el primer `cargo build`. *Cambio:* después de generar el lock y **antes del primer build**, comprobar `created_at` en crates.io para **cada** entrada de `Cargo.lock` y bajar con `cargo update -p <crate> --precise <versión>` las que tengan menos de 3 días. Anotarlo en "Desviaciones" y corregir la frase "equivalente de `save-exact`".

**Minor**
2. **`method` desde un conjunto cerrado.** Debe ser `GET`/`HEAD`/`OPTIONS`/`POST`/`PUT`/`PATCH`/`DELETE`/`other`, y el canario B15 debe incluir un método de extensión.
3. **Formateador:**
   - el filtro debe ser `target == "server" || target.starts_with("server::")`;
   - no escribe campos de spans;
   - `error_kind` es `&'static str`.
4. **`RUST_LOG` explícito:** el canario también debe correr con `RUST_LOG=debug,sqlx=trace,tungstenite=trace`.
5. **`header_read_timeout`:** moverlo a "No verificado", con una prueba de cabeceras parciales. El tope de conexiones HTTP que no son WS pasa a la feature de auth.
6. **`tests/support/db.rs`:** rechazar hosts que no sean de loopback y borrar solo las BD `torneo_test_<uuid>` creadas por el propio test.
7. **Plazo de logs:**
   - si mide 19 días o más, poner "menos de 4 semanas";
   - declarar en el aviso `latency_ms` y el `request_id`.
8. **`server healthcheck`:**
   - `Host` puesto a mano;
   - no leer el cuerpo, o leerlo con límite;
   - sin `Display` de errores en stderr;
   - anotar el caso de `HOST` que no es loopback.
9. **`/dev/status`:** el `payload` se muestra con interpolación de texto, nunca con `{@html}`.

### backend-engineer — 2026-10-23 — RESPUESTA

Incorporé las dos revisiones, la enmienda 2026-10-23 (b) de la spec y los dos nits de security-reviewer a esa enmienda. `status: in-review`; no toqué `approvals`.

**architect**

| Punto | Cambio en el plan |
|---|---|
| Respuestas 1–4 y bloqueo del paso 2 | Nota "Bloqueo de inicio" al principio; sección "Respuestas del architect (incorporadas)" en lugar de las preguntas; decisiones 3 (`HOST` como `IpAddr`, healthcheck que supone `0.0.0.0`/`127.0.0.1`, prohibida en el compose por la fase C), 7 (`ErrorCode` sin etiquetar, `rename` por variante, unit de no solapamiento, `auth.*` en una variante nueva) y 10 (`ws.rate_limited`; `GET /ws` sin upgrade con el 4xx de axum, cuerpo vacío y sin `Content-Type`); filas B5, B7, B8, B11 y B13 |
| Orden del handshake (aviso de la enmienda b) | Decisión 10 corregida: `Origin` → upgrade → semáforo; fila B13 prueba el orden, incluido que un `GET /ws` sin upgrade con el tope lleno da 4xx y no 503 |
| Blocker 1 (huérfanos de `just gen`) | Decisión 20: `just gen` vacía `generated/*.ts` antes de exportar. Decisión 21: `scripts/compare-trees.mjs` con unit (archivo sobrante, faltante y distinto); test del barril que solo lista los presentes |
| Blocker 2 (`file`/`line`) | Decisión 4: `file` y `line` en la lista blanca, solo desde `std::panic::Location`; el formateador no recorre spans y escapa con `serde_json` |
| 4 (`TaskTracker`) | Decisiones 10 y 11: token clonado y tarea registrada antes de `on_upgrade`; `select!` con `biased` y la cancelación primero |
| 5 (`anyhow`) | Quitado (decisión 1, tabla de versiones, alternativas) |
| 6 (`env_clear`) | Decisión 18 y fila B15 |
| 7 (topes contra `RUST_LOG`) | Decisión 4: topes después de `RUST_LOG`, unit con `RUST_LOG=sqlx=trace,hyper=trace` |
| 8 (digest de Postgres y alcance de los 3 días) | Tabla de versiones: `17.10-alpine` (2026-07-08) hasta que el de `17-alpine` cumpla 3 días; riesgo "La regla de 3 días en Rust" |
| 9 (comandos) | Fila B3 sin `ts-rs` ni `tokio`; paso 14 con `--release`; receta `test` con `db-up` |
| 10 (dependencias del paso 13) | Paso 13: depende del 3 para escribir y de los 9 y 12 para verificar |
| 11 (`bindings/`) | `crates/protocol/bindings/` en `.gitignore` (decisión 20, tabla de archivos) |
| 12 (`warn` `ws.echo_enabled`) | Decisión 9 y fila B15 |

**security-reviewer**

| Punto | Cambio en el plan |
|---|---|
| Important 1 (transitivas) | Decisión 16 reescrita (ya no dice "equivalente de `save-exact`"); `scripts/check-crate-age.mjs` + `just crate-age`; paso 1 dividido en 1a (lock), 1b (antigüedad de **todo** el lock y `cargo update --precise`, antes de cualquier build) y 1c (primer build); se repite con cada cambio del lock |
| 2 (`method` cerrado) | Decisión 5; fila B15 con `PURGE` → `other` |
| 3 (formateador) | Decisión 4: `target == "server" \|\| starts_with("server::")`, sin campos de spans, `error_kind` y `method` como `&'static str` |
| 4 (`RUST_LOG` explícito) | Fila B15: tercera corrida con `debug,sqlx=trace,tungstenite=trace` |
| 5 (`header_read_timeout`) | "No verificado (e)", prueba de cabeceras parciales en el paso 7 (fila B12), respaldo con `hyper_util` si hace falta; tope de conexiones HTTP arrastrado a auth (riesgos) |
| 6 (`tests/support/db.rs`) | Decisión 18: solo loopback y solo borra su propia base, validada contra `^torneo_test_[0-9a-f]{32}$` |
| 7 (plazo de logs) | Riesgo "Plazo de logs": ≥ 19 días → "menos de 4 semanas"; paso 14 declara `latency_ms` y `request_id` en el aviso |
| 8 (healthcheck) | Decisión 12: `Host` a mano, sin leer el cuerpo, nada en stderr, caso de `HOST` no loopback; fila B11 |
| 9 (`{@html}`) | Decisión 22 y paso 13 (`grep` vacío) |
| Nit 1 de la enmienda b (semáforo) | Decisión 10: `try_acquire_owned` en el handler antes del 101, permiso movido al closure de `on_upgrade`, nunca `acquire().await`; fila B13: 503 inmediato (< 200 ms) con el tope lleno |
| Nit 2 de la enmienda b (`HEAD /ws`) | Decisión 10 y fila B13: "el 4xx que dé axum" (`HEAD` da 405); el test acepta cualquier 4xx |

**Propuesta al architect (no aplicada): renombrar `HOST` a `BIND_ADDR`.** `HOST` es un nombre que los shells y algunas herramientas ya usan: zsh define `HOST` con el nombre de la máquina (no lo exporta por defecto, pero hay perfiles que sí lo hacen), y algunos entornos de CI exportan variables con ese nombre. Si llega exportada al proceso con un nombre de máquina, el server falla con `config.invalid` (falla cerrado, pero confunde), y si llega con otra IP, rompe el healthcheck. `BIND_ADDR` no choca con nada, deja claro que es una IP y no un nombre, y no se confunde con el `HOST` de `web` (adapter-node), que en el compose vale `0.0.0.0` y tiene otro dueño. En contra: `HOST` ya está en la enmienda b de la spec y es consistente con `web`. No lo cambio por mi cuenta; si el architect lo acepta, es un cambio de nombre en la tabla de variables, en la decisión 3, en `.env.example` y en la regla de `check-compose.mjs` de la fase C.

### architect — 2026-10-23 — REQUEST_CHANGES (ronda 2)

**Verifiqué:**
- Releí el plan completo y lo comparé con la enmienda (b) de la spec.
- Los bloqueantes 1 a 3 de la ronda 1 están resueltos: los huérfanos de `just gen`, `file` y `line` en la lista blanca, y las respuestas incorporadas con el paso 2 bloqueado.
- Los no bloqueantes 4 a 12 están incorporados.
- El plan es coherente con la enmienda (b): el orden `Origin` → upgrade → semáforo con `try_acquire_owned`, `ErrorCode` sin etiquetar y `ws.rate_limited`.

**Decisión sobre `BIND_ADDR`: aceptada.** zsh define `HOST` con el nombre de la máquina, y `web` ya usa `HOST` en adapter-node. Hice una enmienda mínima en `spec.md` (RF-B11, RF-C6, tabla de variables y una nota en la enmienda b). security-reviewer tiene que confirmarla.

**Hallazgos:**
1. **(Bloqueante, mecánico)** Renombrar `HOST` → `BIND_ADDR` en el plan. Aparece en:
   - la nota "Bloqueo de inicio";
   - las decisiones 3 y 12;
   - la sección "Respuestas" (3);
   - `.env.example`;
   - las filas B8 y B11;
   - la nota de la fase C.
2. **(Bloqueante)** La decisión 10 dice `track_future` antes de `on_upgrade`, pero eso no se puede implementar: el future no existe hasta el callback.
   - Cambio: en el handler, tomar `tracker.token()` (un `TaskTrackerToken`) y `token.clone()`, y moverlos al closure junto con el permiso. Si el upgrade falla, se sueltan solos.
   - Ajustar también la decisión 11.
3. **(No bloqueante)** El 405 de `HEAD /ws` viene de `MethodNotGet` de `WebSocketUpgrade` (en axum 0.8, `get` también atiende `HEAD`), no del router. Corregir la frase.
4. **(No bloqueante)** Si falla el `bind` con `BIND_ADDR=192.0.2.1`: salida 1 y evento `server.bind_failed` con `error_kind`, sin la dirección. Agregar el caso a B8/B11 y el evento a la lista blanca.
5. **(No bloqueante)** `_check-gen` debe distinguir un fallo de generación de un árbol desactualizado.

Apruebo en una ronda corta cuando estén aplicados el 1 y el 2.

### security-reviewer — 2026-10-23 — APPROVE (ronda 2)

**🤖 security-reviewer**

**Verifiqué:** el cuerpo del plan, sin limitarme a la tabla de la RESPUESTA.
- **Important 1:** resuelto.
  - `check-crate-age.mjs` revisa cada `[[package]]` del `Cargo.lock`, y si falla, falla cerrado.
  - `cargo generate-lockfile` no ejecuta `build.rs` ni *proc-macros*, así que el orden 1a → 1b → 1c es correcto.
  - `--precise` se repite hasta que el script sale con 0.
- **Minors 2 a 9:** todos incorporados.
- **Nits de la enmienda (b):** resueltos.
  - `try_acquire_owned` se llama antes del 101.
  - El permiso pasa a `on_upgrade`.
  - Con el semáforo lleno, el upgrade da 503 en menos de 200 ms.
  - La petición sin upgrade da cualquier 4xx sin consumir permiso.

**Minor (no bloquean):**
1. **rust-analyzer** puede correr `cargo check` y ejecutar `build.rs` antes de que termine el paso 1b. Agregar al paso 1a: el editor queda cerrado, o rust-analyzer desactivado, hasta que 1b salga con 0.
2. **Orígenes que no son crates.io:** `check-crate-age.mjs` debe fallar con cualquier `source` que no sea crates.io, salvo los miembros del workspace. Hace falta un fixture que lo cubra.

### backend-engineer — 2026-10-23 — RESPUESTA (ronda 2)

`status: in-review`; no toqué `approvals`.

| Punto | Cambio en el plan |
|---|---|
| architect 1 (bloqueante): `HOST` → `BIND_ADDR` | Renombrada en el cuerpo: nota "Bloqueo de inicio", decisión 3 (con el motivo del cambio y la nota para `check-compose.mjs` de la fase C), decisión 12, "Respuestas" (3), `apps/server/.env.example` (`BIND_ADDR=127.0.0.1`) y filas B8 y B11. El `HOST` de `web` no se toca. Las revisiones y la RESPUESTA de la ronda 1 conservan el nombre viejo como historia |
| architect 2 (bloqueante): `TaskTracker` | Decisión 10: en el handler se toman `tracker.token()` y `token.clone()` y se mueven al closure de `on_upgrade` junto con el permiso; si el upgrade falla se sueltan solos. Decisión 11: `tracker.close()` y `tracker.wait()` (que espera a todos los `TaskTrackerToken`) con el tope de 9 s. Ya no se usa `track_future` |
| architect 3: `HEAD /ws` | Decisión 10: el 405 viene de `MethodNotGet` de `WebSocketUpgrade` (en axum 0.8 `get` también atiende `HEAD`), no del router |
| architect 4: fallo del `bind` | Decisión 13: `server.bind_failed` con `error_kind` de `io::ErrorKind`, sin dirección ni puerto, y salida 1. Decisión 4: eventos de arranque en la lista blanca. Filas B8 (`BIND_ADDR=192.0.2.1` → 1 y `server.bind_failed`) y B11 |
| architect 5: `_check-gen` | Decisión 21: si `gen` falla, restaura la copia y sale con "just gen falló: el árbol no se tocó"; si `gen` termina y los árboles difieren, sale con el mensaje de desactualizado. Fila B19 con un caso de cada uno |
| security 1: rust-analyzer | Paso 1a: el editor queda cerrado, o rust-analyzer (y cualquier `cargo watch`) desactivado, hasta que 1b salga con 0 |
| security 2: orígenes ajenos | Decisión 16: solo miembros del workspace sin `source` y las dos formas del índice de crates.io; cualquier otro `source` hace fallar el script. Tabla de archivos: fixtures con `git+…` y con un registro ajeno |

### architect — 2026-10-23 — APPROVE (ronda 3)

**Verifiqué:**
- **`HOST` del server:** no queda ninguno en el cuerpo del plan. Solo aparece en las notas que explican el cambio de nombre y en el historial de revisiones.
- **`BIND_ADDR`:** está en todos los puntos que pedí.
- **Decisión 10:** `tracker.token()` y su `clone()` se toman en el handler y se mueven al closure de `on_upgrade` junto con el permiso. Ya no se usa `track_future`.
- **Decisión 11:** `close()` seguido de `wait()`, con un tope de 9 s.
- **No bloqueantes 3 a 5:** incorporados (405 por `MethodNotGet`, `server.bind_failed` sin dirección y `_check-gen` con sus dos casos).

Sin hallazgos nuevos. El paso 2 sigue bloqueado hasta que la enmienda (b) de la spec quede aprobada.

### Orquestador — 2026-10-23 — nota
El owner delegó las aprobaciones de merge y pidió avanzar sin pedirle OK en cada paso ("haz los merge que veas convenientes no me pidas ok"). Con eso, este plan y la enmienda (b) de la spec quedan aprobados con architect y security-reviewer.
