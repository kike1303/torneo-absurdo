---
id: "008"
title: "Autenticación y sesiones, fase A: IP, límites y Origin"
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

# Plan: Autenticación y sesiones, fase A (desde spec.md 2026-10-28)

> [intent](./intent.md) · [spec](./spec.md) · ADRs [0006](../../decisions/0006-despliegue-y-ci.md) (D1, D2), [0007](../../decisions/0007-i18n-desde-cero.md), [0008](../../decisions/0008-autenticacion.md) (D4, D5, D6, D7, "Nota (2026-10-28)"), [0012](../../decisions/0012-logs-y-cabeceras-de-seguridad.md) (A4, A6, A8)

Alcance: RF-A1 a RF-A9 de la spec, y de la estrategia de pruebas T1, T2, T3, T15 y T21, la parte server de T14 y la parte de IP de T22. No hay BD de dominio, migraciones, sesiones, cookies, rutas de auth ni `handleFetch`: eso es de las fases B, C y D. No toca `crates/engine`, `crates/engine-wasm`, `crates/sim`, `crates/content`, `packages/renderer`, `.github/workflows/` ni el e2e.

**Otros revisores del PR.** La fase A toca, además del server:
- `apps/web` en un punto mínimo (decisión 14): revisa **frontend-engineer**.
- `infra/docker-compose.yml` y `scripts/check-compose.mjs` (decisión 13): revisa **devops-engineer**.
- Por `risk: high` y por tocar D4, D5 y D7: revisa **security-reviewer**.

No cambia ningún workflow. El job `policy` corre siempre, y los cambios en `infra/` y `scripts/` disparan los jobs que ya existen.

## Para el architect: lo que tienes que decidir
1. **`INTERNAL_API_SECRET` opcional en la fase A.** RF-A6 necesita el secreto, pero RF-B18 (fase B) es quien lo vuelve obligatorio y fija los valores de los `.env.example`. Propongo esto para A: si la variable falta, ninguna petición es interna. Si viene, se valida igual que en B (base64url, ≥ 32 bytes decodificados) y, si no cumple, `config.invalid` con `var = INTERNAL_API_SECRET`. Así el compose de producción no cambia dos veces y `web`, que todavía no firma nada, no se ve afectado. B la vuelve obligatoria.
2. **`TRUSTED_PROXIES` en el compose de producción ya en la fase A.** Sin ella, después de A todo el tráfico de producción llega con el peer `10.250.0.2`. Todos los usuarios compartirían una sola `IpKey`, con 50 sockets anónimos y 30 upgrades por minuto para todo el sitio. Propongo agregar `TRUSTED_PROXIES: 10.250.0.2/32` (ADR-0006 D2) al servicio `server` y a la allowlist `ENV.server` de `check-compose.mjs`, con valor exacto. Así `main` sigue siendo desplegable aunque no se despliegue.
3. **El IT de "la petición 11 de `start`" (RF-A4).** `POST /api/auth/{provider}/start` no existe hasta la fase B. Además, con la cuota de D7 (10/min, ráfaga 5) y GCRA, la 6.ª petición en ráfaga ya da 429, y en 60 s pasan hasta 15 (5 de ráfaga más 1 cada 6 s). Propongo:
   - **En A:** el bucket `AUTH_IP` y la capa HTTP de rate limit, probados sobre un router de test. El test comprueba la semántica GCRA exacta: la 6.ª petición en ráfaga da 429 con `Retry-After: 6`, y tras avanzar 6 s pasa una más. T22 (IP) se prueba con el upgrade del `/ws`, que sí existe.
   - **En B:** se monta la capa en `start`, `link` y `callback`, con su IT sobre la ruta real.
   - Si prefieres "11 en un minuto" al pie de la letra, la cuota tendría que ser `per_minute(10)` con ráfaga 10. Eso contradice la ráfaga 5 de D7.
4. **La propiedad "`client_ip` nunca devuelve una IP en `TRUSTED_PROXIES`".** No se cumple cuando el peer es de confianza y `X-Forwarded-For` falta o solo trae proxies: RF-A1 manda devolver el peer. Propongo esta formulación: "si el resultado está en `TRUSTED_PROXIES`, es el peer, y ninguna entrada válida de `X-Forwarded-For` quedaba fuera de `TRUSTED_PROXIES`". Más una segunda propiedad: con un peer que no es de confianza, el resultado es el peer.
5. **Textos en/es de `auth.csrf_origin` y `http.rate_limited`.** En A ninguna pantalla los muestra, así que propongo dejarlos para la fase D, junto a las pantallas. Solo `ws.ip_capacity` necesita llave ya, porque `closeReasonKey` hace un `switch` exhaustivo sobre `WsErrorCode` y `svelte-check` falla sin el caso (decisión 14).
6. **Precedencia del 403.** El middleware de `Origin` corre antes del enrutado. Un `POST` sin `Origin` válido da 403 `auth.csrf_origin` aunque la ruta no exista. Con `Origin` válido se conservan el 404 y el 405 (RF-A5). Lo dejo explícito porque cambia lo que hoy prueba `write_methods_are_405_everywhere`.
7. **`TRUSTED_PROXIES` rechaza también `0.0.0.0`, `::`, multicast y broadcast.** La spec solo pide rechazar los prefijos que no son `/32` ni `/128`. Ninguna de esas direcciones puede ser el peer de un proxy real, y aceptarlas solo serviría para esconder un error de configuración.

Si las siete te parecen bien, quedan como decisiones 17 a 23 sin más cambios.

## Decisiones de implementación
1. **Módulo `net/` (RF-A1, RF-A2).** Va en `apps/server/src/net/` y es puro: sin I/O ni estado global.
   - `ip_key.rs`: `IpKey`.
     - Enum privado `V4([u8; 4])` | `V6([u8; 8])`, con los primeros 8 bytes de la IPv6 (el `/64`).
     - Deriva `Clone`, `Copy`, `PartialEq`, `Eq` y `Hash`.
     - `Debug` escrito a mano, que imprime `IpKey(..)`. No implementa `Display`, `Serialize` ni ningún acceso a los bytes fuera del crate.
     - `IpKey::from(IpAddr)` normaliza antes `::ffff:a.b.c.d` a IPv4 (`Ipv6Addr::to_ipv4_mapped`, que no trata `::a.b.c.d` como mapeada).
   - `trusted.rs`: `TrustedProxies`, un `Vec<IpAddr>` normalizado (las mapeadas pasan a IPv4).
     - `TrustedProxies::parse(&str, loopback_origin: bool) -> Result<Self, ()>`.
     - Formato: lista separada por comas, con espacios alrededor de cada entrada. Cada entrada es `ip`, `ip/32` (IPv4) o `ip/128` (IPv6). La cadena vacía da la lista vacía. Una entrada vacía (`a,,b`) es inválida.
     - Rechaza cualquier otro prefijo, un prefijo con ceros a la izquierda (`/032`), las direcciones de la decisión 7 de arriba y el loopback (`127.0.0.0/8`, `::1` y sus mapeadas) si `loopback_origin` es `false` (RF-A3).
     - `contains(IpAddr)` compara contra la dirección normalizada. No hay aritmética de CIDR: solo direcciones exactas, así que no hace falta `ipnet`.
   - `client_ip.rs`: `client_ip(peer: IpAddr, headers: &HeaderMap, internal: bool, trusted: &TrustedProxies) -> IpAddr`, con la tabla de RF-A1.
     - **Interna:** exactamente un `X-Client-Ip` que se parsea como `IpAddr` sin recortar. Si no, el peer.
     - **Peer de confianza:** las líneas de `X-Forwarded-For`, en orden (`get_all`). Cada línea tiene que ser `to_str()` válida, si no cuenta como mal formada. Se parten por `,` y se recorta cada entrada. Se recorren de derecha a izquierda: una entrada de confianza se salta, la primera que no lo es se devuelve, y una mal formada (vacía, con puerto, entre corchetes, con zona `%`, `unknown`) termina la búsqueda y devuelve el peer. Si se acaban las entradas, también el peer.
     - **Cualquier otro peer:** el peer, sin leer el header.
     - El resultado siempre sale normalizado (las mapeadas pasan a IPv4).
   - `internal.rs`: `InternalSecret`, un newtype sobre `Box<[u8]>` con el texto base64url tal como llega.
     - `Debug` redactado (`InternalSecret(..)`).
     - `is_internal(&HeaderMap, Option<&InternalSecret>) -> bool`: exactamente un `X-Internal-Secret`, comparado byte a byte con `subtle::ConstantTimeEq` (RF-A6). `ct_eq` sobre slices de distinto largo devuelve falso. Ese largo no es secreto, porque el `.env.example` lo muestra.
     - Sin secreto configurado, o con un header ausente, repetido o incorrecto, la petición es externa, sin error propio.
   - `mod.rs`: `ClientContext { ip: IpAddr, key: IpKey, internal: bool }`.
     - `Debug` redactado (solo `internal`).
     - La capa `net::layer` lo calcula una vez por petición y lo inserta como extensión. Toma el peer de `ConnectInfo<SocketAddr>` (decisión 3), y `TrustedProxies` y el secreto de `NetState`, que va en `AppState`.
     - Sin `ConnectInfo` (solo pasaría en un test que no usa `serve_listener`), salta un `debug_assert!` y se usa `0.0.0.0`. Todo cae en un solo bucket: falla hacia más restrictivo.
2. **Config (RF-A3, RF-A6).** En `config.rs`, en este orden, después de `ENABLE_WS_ECHO`:
   - `TRUSTED_PROXIES`: por defecto, vacía. Usa `TrustedProxies::parse` con `is_loopback_http_origin(&origin)`.
   - `INTERNAL_API_SECRET`: opcional en A (pregunta 1). Si viene, base64url con o sin relleno (`base64` con `DecodePaddingMode::Indifferent` sobre `URL_SAFE`) y ≥ 32 bytes decodificados. Si no, `config.invalid`.

   El predicado de loopback de `decide_echo` se extrae a `pub fn is_loopback_http_origin(&Url) -> bool`, y `decide_echo` lo usa. Es el mismo predicado que piden RF-B9 y RF-B18 (nota de security-reviewer para el plan-b). El `Debug` de `Config` redacta el secreto (`<redacted>` o `<unset>`) y muestra `trusted_proxies` solo como un número de entradas: también es una IP (A4.1), aunque sea la del proxy.
3. **El peer llega al router.** `serve_listener` hoy descarta la dirección (`Ok((stream, _))`). Pasa a conservarla y envuelve el router en `tower::util::MapRequest`, que inserta `axum::extract::ConnectInfo(peer)` en las extensiones de cada petición de esa conexión. No cambia nada más del bucle de aceptación. Así funcionan igual `serve()`, el binario y los tests en proceso (`tests/support/app.rs` ya usa `serve_listener`).
4. **Middleware de `Origin` (RF-A5).** `http/origin.rs` reemplaza `http/read_only.rs`, que se borra.
   - **Métodos:** `GET`, `HEAD` y `OPTIONS` pasan sin chequeo. Cualquier otro (`POST`, `PUT`, `PATCH`, `DELETE`, `PURGE`…) se revisa.
   - **Petición del navegador** (`internal = false`): exactamente un `Origin`, con bytes iguales a `PUBLIC_ORIGIN`. `null` no lo es. Sin ningún `Origin`, exactamente un `Sec-Fetch-Site` igual a `same-origin`. Dos `Origin` dan 403, aunque uno sea válido.
   - **Petición interna:** exactamente un `X-Client-Origin` igual a `PUBLIC_ORIGIN`, y `X-Client-Fetch-Site` ausente o exactamente uno igual a `same-origin`. El `Origin` propio de la petición interna no se mira.
   - Si falla, 403 `{"error":"auth.csrf_origin"}`, sin llegar al handler.
   - Va dentro de `net::layer` (necesita `ClientContext`) y fuera de las rutas, aplicado con `Router::layer`. Así cubre las rutas, el `fallback` y el `method_not_allowed_fallback`.
   - Orden de capas resultante, de afuera hacia adentro: headers sensibles de la petición, `request_log`, headers sensibles de la respuesta, `nosniff` y HSTS, `net::layer`, `origin::layer` y las rutas.
   - `errors::ALLOW_READ_ONLY` pasa a llamarse `ALLOW_SAFE`, con el mismo valor: lo sigue usando el 405 de `/api/health`.
   - Sin `CorsLayer` (RF-A8). Nada cambia ahí, y T21 lo vigila.
5. **Manifiesto de rutas (T1).** `http/routes.rs` define `ROUTES: &[RouteSpec { method: Method, path: &'static str, effect: Effect }]`, con `Effect::{Read, Mutate}`, y `SAFE_METHOD_EXCEPTIONS = [(GET, "/api/auth/{provider}/callback")]`, declarada ya aunque la ruta entre en la fase B.
   - `http::router` registra sus rutas a partir del manifiesto: una función `route_for(spec)` devuelve el handler de cada entrada. En A hay dos: `GET /api/health` y `GET /ws`, las dos `Read`.
   - `check_manifest(&[RouteSpec]) -> Result<(), RouteSpec>` falla si una entrada `Mutate` usa `GET`, `HEAD` u `OPTIONS` y no está en las excepciones. Un UT le pasa un manifiesto sintético con un `GET` que muta y espera el error.
   - Un UT recorre `apps/server/src/**/*.rs` (desde `CARGO_MANIFEST_DIR`) y falla si `.route(` aparece fuera de `http/routes.rs`. Así ninguna ruta se registra por fuera del manifiesto. La fase B extiende el manifiesto, no la regla.
   - El IT de T1 recorre el router real: cada entrada responde por su método (ni 404 ni 405), y cada método inseguro sobre esa ruta, con `Origin` válido, da 405.
6. **Rate limiter (RF-A4).** Va en `apps/server/src/rate/`. Usa `governor` directamente, sin `tower-governor`, por tres motivos:
   - el extractor de una sola llave no cubre los dos buckets de la fase B;
   - `tower-governor` trae `governor` con `quanta`, `dashmap` y `jitter` por defecto, y `tonic` como feature por defecto;
   - no tiene tope de llaves.

   Las piezas:
   - `clock.rs`: `ServerClock`, que implementa `governor::clock::Clock` con `Instant = governor::nanos::Nanos`. Tiene dos variantes:
     - `Monotonic { start: std::time::Instant }`: `now()` es el tiempo transcurrido desde `start`.
     - `Manual(Arc<AtomicU64>)`: `now()` es el contador, y `advance(Duration)` lo sube.

     Los tests usan `Manual` para un reloj controlado sin `tokio::time::pause()`. Pausar el tiempo de tokio adelantaría también los timers de hyper (`header_read_timeout`) y los del WS en los IT con sockets reales.
   - `store.rs`: `CappedStore`, un newtype sobre `Arc<governor::state::keyed::HashMapStateStore<IpKey>>`. Implementa `StateStore` y `ShrinkableKeyedStateStore` delegando en el store de `governor`. Agrega `contains(&IpKey)` (por el `lock()` inherente del `Mutex` del alias, sin depender de `parking_lot`) y `capacity()`.
   - `mod.rs`: `KeyedLimiter`, con un `RateLimiter<IpKey, CappedStore, ServerClock>`, `max_keys`, un clon del store y un `std::sync::Mutex<()>` de admisión. Su `check(&IpKey) -> Decision` devuelve `Decision::{Allow, Limited { retry_after_secs: u64 }, Full}`:
     - Si la llave existe, `check_key` directo.
     - Si no existe, toma el mutex de admisión, vuelve a mirar, y con `len() >= max_keys` devuelve `Full` sin insertar. Si no, `check_key`.
     - `sweep()` toma el mismo mutex y llama a `retain_recent()` y `shrink_to_fit()`.

     Como las llaves nuevas solo entran con el mutex tomado y la limpieza también lo toma, el tope es exacto y **nunca se desaloja una llave viva**. El mutex se toma con `lock().unwrap_or_else(PoisonError::into_inner)` (sin `unwrap`). `retry_after_secs` es `ceil(wait_time_from(now))`, con un mínimo de 1. `Full` responde `Retry-After: 60`, que es el intervalo de la limpieza.
   - `quotas.rs`: `AUTH_IP = Quota::per_minute(10).allow_burst(5)` (montado en la fase B), `WS_ANON_UPGRADE_IP = Quota::per_minute(30).allow_burst(10)` y `MAX_KEYS: usize = 100_000`. Este último es una constante, no una variable de entorno (RF-A4). `Settings` (los parámetros que no son env y que los tests acortan) gana `rate: RateSettings { max_keys, clock }`, con `MAX_KEYS` y `Monotonic` por defecto.
   - `http.rs`: `rate::http::layer(limiter)`, un middleware de axum que lee `ClientContext` y responde 429 `{"error":"http.rate_limited"}` con `Retry-After` en `Limited` y en `Full`. En A no se monta en ninguna ruta (pregunta 3).
   - `RateLimits` (en `AppState`) agrupa los limiters. `sweeper(limits, CancellationToken)` corre un `tokio::time::interval(60 s)` con `MissedTickBehavior::Delay`. Lo lanza `serve()` y se cancela con la señal de apagado. No registra nada (RF-A9).
7. **Límites del `/ws` anónimo (RF-A7).** En `ws/handshake.rs`, el orden pasa a ser este:
   1. `Origin` (como hoy, 403 `ws.origin_forbidden`, D6).
   2. Upgrade válido (como hoy).
   3. Rate de upgrade por `IpKey`: `WS_ANON_UPGRADE_IP`. Con `Limited` o `Full`, 429 `{"error":"ws.rate_limited"}` con `Retry-After`.
   4. Tope de sockets anónimos por `IpKey` (50, `WS_ANON_PER_IP`). El 51.º recibe 503 `{"error":"ws.ip_capacity"}`.
   5. Tope global (como hoy, 503 `ws.capacity`).

   El tope por IP es `ws/ip_slots.rs`: `IpSlots`, un `Mutex<HashMap<IpKey, u32>>` con `try_acquire(IpKey) -> Option<IpSlot>`. `IpSlot` es un guard RAII que en su `Drop` resta uno y quita la llave al llegar a 0. Su tamaño queda acotado por `WS_MAX_CONNECTIONS`, porque cada llave tiene al menos un socket vivo. El `IpSlot` se toma antes del upgrade y se mueve al closure de `on_upgrade` con el permiso global y el token del tracker, igual que hoy. Si el upgrade falla, los tres se sueltan. El límite 50 es una constante. `WsState::new` recibe el `KeyedLimiter` del upgrade y el límite por IP desde `Settings`, así los IT lo acortan sin variable de entorno. El handler lee `Extension<ClientContext>`.
8. **Protocolo.** En `crates/protocol/src/error.rs`:
   - `ApiErrorCode::RateLimited` (`http.rate_limited`).
   - `WsErrorCode::IpCapacity` (`ws.ip_capacity`).
   - Un enum nuevo, `AuthErrorCode`, con prefijo `auth.` y por ahora solo `CsrfOrigin` (`auth.csrf_origin`). La fase B agrega el resto.
   - `ErrorCode::Auth(AuthErrorCode)` y `From<AuthErrorCode>`.

   Los tests del archivo se actualizan: `ALL`, los conteos, los prefijos sin solaparse, el round-trip, y el caso que hoy espera que `"auth.csrf_origin"` falle al deserializar pasa a esperar `ErrorCode::Auth`. Después, `just gen`: aparece `AuthErrorCode.ts` y cambian `ApiErrorCode.ts`, `WsErrorCode.ts`, `ErrorCode.ts` y el barrel. Se commitea el resultado.
9. **Logs (RF-A9).** `SENSITIVE_REQUEST_HEADERS` pasa de 7 a 10 nombres: `x-client-origin`, `x-client-fetch-site` y `sec-fetch-site`. El UT `sensitive_request_headers_are_marked` suma los tres. No hay ningún campo ni evento nuevo en `telemetry.rs`: los rechazos de `Origin`, de rate limit y de tope se ven solo en la línea `http.request` por su `status`. Ningún tipo nuevo tiene un `Debug` que muestre una IP o el secreto (decisiones 1 y 2).
10. **IT con IPs distintas.** Los tests corren en `127.0.0.1`. Para simular otros clientes, el `AppState` del test se arma con `TrustedProxies` que contiene `127.0.0.1` (lo permite el origen de loopback de los tests) y se manda `X-Forwarded-For`. Para los casos de peer no confiable, se arma con la lista vacía. Lo encapsula `tests/support/app.rs` con `state_with(net: NetConfig, settings: Settings)`.
11. **Bench de memoria (RNF de memoria, RF-A4).** Son dos mediciones:
    - **Automática** (UT en `rate/mod.rs`): llena un `KeyedLimiter` con 100 000 `IpKey` IPv6 distintas y comprueba que `capacity() × (size_of::<(IpKey, InMemoryState)>() + 1) + 16` es menor que 20 MB. Es la cota del layout de la tabla de `hashbrown` en `std`. Comprueba también que la llave 100 001 da `Full` y que una llave existente sigue con su bucket.
    - **Manual:** `apps/server/examples/rate_memory.rs` (con `#![allow(clippy::print_stdout)]`) llena el limiter del upgrade hasta el tope e imprime `len()`. Se corre con `/usr/bin/time -l` (macOS) o `-v` (Linux), con 0 y con 100 000 llaves, en release. La diferencia de RSS máximo va en el PR y en "Desviaciones" si supera la estimación. Cota esperada: 131 072 cubetas × 25 B ≈ 3,3 MB por limiter. En A hay dos.
12. **Propiedades (proptest).** `proptest` no está en el workspace, así que se agrega como dev-dependency del server, con `default-features = false, features = ["std"]` (sin `fork`, `timeout` ni `tempfile`). Hay tres propiedades:
    - las dos de `client_ip` de la pregunta 4;
    - `IpKey` de dos IPv6 que comparten los primeros 64 bits es igual, y de dos que difieren en ellos es distinta.

    No toca el motor: el determinismo de `crates/engine` no cambia.
13. **Compose y `check-compose` (pregunta 2).**
    - `infra/docker-compose.yml`: `TRUSTED_PROXIES: 10.250.0.2/32` en `server.environment`.
    - `scripts/check-compose.mjs`: `ENV.server` exige ese valor exacto.
    - El fixture `scripts/__fixtures__/check-compose/valid/infra/docker-compose.yml` se actualiza, y `scripts/__tests__/check-compose.test.mjs` suma un caso en que falta o tiene otro valor (`10.250.0.0/28`, la subred, debe fallar).
    - `INTERNAL_API_SECRET` no entra en el compose hasta la fase B.
    - En `just prod-local` no hay proxy en `10.250.0.2`, así que el peer real se usa tal cual. No cambia nada.
14. **Web, mínimo (pregunta 5).** El `switch` exhaustivo de `apps/web/src/lib/features/system/echo.ts` deja de compilar con `ws.ip_capacity`. Cambia esto:
    - el caso `'ws.ip_capacity' → 'ws_close_reason_ip_capacity'` y la llave en `CloseReasonKey`;
    - los textos en `apps/web/messages/en.json` y `es.json` ("Too many connections from your network. Try again later." / "Hay demasiadas conexiones desde tu red. Vuelve a intentarlo más tarde.");
    - la fila nueva en `echo.test.ts`.

    `ErrorCode` y `ApiErrorCode` no tienen `switch` exhaustivo en la web, así que no hace falta nada más.
15. **`.env.example` del server.** Agrega `TRUSTED_PROXIES=` (vacía, con un comentario: en `just dev` el proxy de Vite no agrega `X-Forwarded-For` y el peer es `127.0.0.1`) y `# INTERNAL_API_SECRET=` comentada, con la nota de que la fase B la vuelve obligatoria.
16. **Dependencias nuevas.** Se fijan con `=` en `[workspace.dependencies]`, como las demás:
    - `governor = { version = "=0.10.4", default-features = false, features = ["std"] }`, publicada el 2025-12-16. Sin `quanta`, `dashmap` ni `jitter`. Trae `parking_lot` (ya está en el lock), `hashbrown 0.16` (ya está), `smallvec` y `futures-sink` (ya están), y las nuevas `nonzero_ext`, `portable-atomic`, `spinning_top`, `web-time` y `futures-timer`.
    - `subtle = "=2.6.1"` y `base64 = "=0.22.1"`: ya están en el lock por `rustls`, así que no agregan versiones.
    - `proptest = { version = "=1.12.0", default-features = false, features = ["std"] }`, publicada el 2026-10-10. Solo como dev-dependency. Trae `rand 0.10` (ya está), `chacha20 0.10`, `rand_xorshift`, `unarray`, `bitflags` y `num-traits` (estos dos ya están) y `regex-syntax` (ya está).
    - Ninguna trae OpenSSL ni `aws-lc`.
    - Orden obligatorio: editar los `Cargo.toml`, `cargo update -w` (o `cargo generate-lockfile --offline` si no hace falta red), `just crate-age` **antes de compilar**, bajar con `cargo update -p <crate>@<v> --precise <anterior>` lo que marque, y `just deny`.

## Archivos que cambian
| Archivo | Cambio |
|---|---|
| `Cargo.toml` | modificado: `governor`, `subtle`, `base64` y `proptest` en `[workspace.dependencies]` (decisión 16) |
| `Cargo.lock` | modificado: el resultado de lo anterior (pasa `just crate-age` y `just deny`) |
| `apps/server/Cargo.toml` | modificado: `governor`, `subtle` y `base64` en `[dependencies]`, `proptest` en `[dev-dependencies]` |
| `apps/server/src/lib.rs` | modificado: `pub mod net; pub mod rate;` |
| `apps/server/src/net/mod.rs` | nuevo: `ClientContext`, `NetState`, `net::layer` (decisión 1) |
| `apps/server/src/net/ip_key.rs` | nuevo: `IpKey` (RF-A2) |
| `apps/server/src/net/trusted.rs` | nuevo: `TrustedProxies::parse`, `contains` (RF-A3) |
| `apps/server/src/net/client_ip.rs` | nuevo: `client_ip` con la tabla de UT y las propiedades (RF-A1) |
| `apps/server/src/net/internal.rs` | nuevo: `InternalSecret`, `is_internal` (RF-A6) |
| `apps/server/src/config.rs` | modificado: `TRUSTED_PROXIES`, `INTERNAL_API_SECRET`, `is_loopback_http_origin`, `Debug` redactado, UT (decisión 2) |
| `apps/server/src/app.rs` | modificado: `ConnectInfo` por conexión en `serve_listener` (decisión 3); `AppState` con `net` y `rate`; `Settings.rate`; `serve()` lanza el `sweeper` |
| `apps/server/src/http/mod.rs` | modificado: capas `net::layer` y `origin::layer` en lugar de `read_only`; rutas desde el manifiesto; 10 headers sensibles (RF-A5, RF-A9) |
| `apps/server/src/http/origin.rs` | nuevo: middleware de D4 con sus UT (RF-A5) |
| `apps/server/src/http/read_only.rs` | **se borra** (RF-A5) |
| `apps/server/src/http/routes.rs` | nuevo: `ROUTES`, `SAFE_METHOD_EXCEPTIONS`, `check_manifest`, UT de fuente (T1) |
| `apps/server/src/http/errors.rs` | modificado: `ALLOW_READ_ONLY` pasa a `ALLOW_SAFE`; helper `too_many_requests(code, secs)` con `Retry-After` |
| `apps/server/src/rate/mod.rs` | nuevo: `KeyedLimiter`, `Decision`, `RateLimits`, `sweeper`, UT del tope y de memoria (RF-A4) |
| `apps/server/src/rate/clock.rs` | nuevo: `ServerClock` (decisión 6) |
| `apps/server/src/rate/store.rs` | nuevo: `CappedStore` (decisión 6) |
| `apps/server/src/rate/quotas.rs` | nuevo: `AUTH_IP`, `WS_ANON_UPGRADE_IP`, `MAX_KEYS` |
| `apps/server/src/rate/http.rs` | nuevo: capa 429 `http.rate_limited`, con UT sobre un router de test (pregunta 3) |
| `apps/server/src/ws/mod.rs` | modificado: `WsState` con el limiter del upgrade, `IpSlots` y el límite por IP |
| `apps/server/src/ws/handshake.rs` | modificado: el orden de RF-A7 |
| `apps/server/src/ws/ip_slots.rs` | nuevo: `IpSlots`, `IpSlot` RAII (RF-A7) |
| `apps/server/examples/rate_memory.rs` | nuevo: bench manual de RSS (decisión 11) |
| `apps/server/tests/support/app.rs` | modificado: `state_with(net, settings)`, `ws_state` con los parámetros nuevos |
| `apps/server/tests/support/http.rs` | modificado: helper para varias líneas del mismo header (dos `Origin`, varias `X-Forwarded-For`) |
| `apps/server/tests/http.rs` | modificado: `write_methods_are_405_everywhere` pasa a "403 sin `Origin`, 405 o 404 con `Origin`"; T2, T3, T15 y T21 |
| `apps/server/tests/client_ip.rs` | nuevo: T14 (server), T22 (IP) y T3 (parte de IP) por el upgrade del `/ws` |
| `apps/server/tests/ws.rs` | modificado: socket 51, upgrade 11 en ráfaga, tope lleno, orden de los rechazos, liberación RAII (RF-A7, RF-A4) |
| `apps/server/tests/routes.rs` | nuevo: T1 contra el router real |
| `apps/server/tests/logs.rs` | modificado: canarios de IP en `X-Forwarded-For` (con `TRUSTED_PROXIES=127.0.0.1/32`), `X-Client-Origin`, `X-Client-Fetch-Site` y `Sec-Fetch-Site`, con un `INTERNAL_API_SECRET` válido y uno incorrecto (RF-A9) |
| `apps/server/tests/startup.rs` | modificado: arranque con `TRUSTED_PROXIES` inválido o loopback con origen `https`: código de salida y solo `var` en el log |
| `apps/server/.env.example` | modificado: decisión 15 |
| `crates/protocol/src/error.rs` | modificado: decisión 8 |
| `crates/protocol/src/lib.rs` | modificado: reexporta `AuthErrorCode` |
| `packages/protocol-ts/src/**` | regenerado con `just gen` (no se edita a mano) |
| `apps/web/src/lib/features/system/echo.ts` y `echo.test.ts` | modificado: `ws.ip_capacity` (decisión 14, revisa frontend-engineer) |
| `apps/web/messages/en.json` y `es.json` | modificado: `ws_close_reason_ip_capacity` |
| `infra/docker-compose.yml` | modificado: `TRUSTED_PROXIES: 10.250.0.2/32` (decisión 13, revisa devops-engineer) |
| `scripts/check-compose.mjs`, su fixture `valid` y `scripts/__tests__/check-compose.test.mjs` | modificado: allowlist con valor exacto y casos de fallo |

## Orden de trabajo
1. **Dependencias** (decisión 16): `Cargo.toml` del workspace y del server, el lock, `just crate-age` antes de compilar y `just deny`. Commit `build(server): add governor, subtle, base64 and proptest (TA-26)`.
2. **Protocolo y web** (decisiones 8 y 14): primero los tests de `error.rs` en rojo, luego los códigos, `just gen`, `echo.ts`, los mensajes y `echo.test.ts`. Commit `feat(protocol): add auth.csrf_origin, http.rate_limited and ws.ip_capacity (TA-26)`. **Paralelizable** con el paso 3 (archivos distintos).
3. **`net/` y config** (decisiones 1 y 2): primero los UT de tabla de `client_ip`, `IpKey` (`/64` y `Debug`), `TrustedProxies` (prefijos, loopback, decisión 7) y `is_internal`, más las propiedades. Después la implementación y los UT de `config.rs` (incluido `is_loopback_http_origin`, que `decide_echo` sigue pasando igual). Commit `feat(server): client IP, ip_key and trusted proxies (TA-26)`.
4. **Peer y capa de contexto** (decisión 3): `ConnectInfo` en `serve_listener`, `net::layer` y `AppState`/`Settings` con `net`. Los IT existentes tienen que seguir en verde sin cambiar sus aserciones.
5. **`Origin` y manifiesto** (decisiones 4 y 5): primero se reescribe `tests/http.rs` (T2, T15 y T3, la parte de `Origin`), en rojo contra la capa de solo lectura. Después `origin.rs`, `routes.rs`, se borra `read_only.rs` y se agregan los headers sensibles. Commit `feat(server): replace the read-only layer with the Origin middleware (TA-26)`. Es el paso más riesgoso (ver Riesgos).
6. **Rate limiter** (decisión 6): primero los UT de `KeyedLimiter` con `ServerClock::Manual` (ráfaga, recarga, `Retry-After`, tope exacto, sin desalojo, `sweep`) y el de memoria. Después la implementación, la capa HTTP con su UT y el `sweeper`. **Paralelizable** con el paso 5 hasta que los dos tocan `http/mod.rs` y `app.rs`: se integra después.
7. **`/ws`** (decisión 7): primero los IT en rojo (upgrade 11, socket 51, tope lleno, orden de rechazos, RAII y `tests/client_ip.rs`), después `ip_slots.rs` y `handshake.rs`. Commit `feat(server): per-IP limits on the anonymous /ws (TA-26)`.
8. **Logs** (decisión 9): el canario nuevo en `tests/logs.rs`, en sus tres niveles de `RUST_LOG`.
9. **Compose** (decisión 13) y `.env.example` (decisión 15). Commit `feat(infra): set TRUSTED_PROXIES for the server (TA-26)`. **Paralelizable** con los pasos 3 a 8.
10. **Bench manual** (decisión 11), con el resultado en el PR.
11. `just fmt`, `just check` completo, `just deny`, y la salida en el PR.

## Riesgos
- **Quitar la capa de solo lectura abre mutaciones sin defensa.** → En A no existe ninguna ruta de mutación. El middleware de `Origin` cubre todo el router, `fallback` incluido. El UT de fuente impide registrar rutas fuera del manifiesto, y `check_manifest` impide un método seguro que muta. Los tests de `Origin` se escriben en rojo contra la capa vieja antes de quitarla.
- **Desplegar A sin `TRUSTED_PROXIES`** comprime todo el tráfico en la `IpKey` del proxy (50 sockets para todo el sitio). → Decisión 13, con `check-compose` exigiendo el valor exacto. El arranque no cambia si falta, porque la lista vacía es válida, así que el chequeo vive en el compose.
- **Un `X-Forwarded-For` falsificado elige la llave.** → Solo se lee desde un peer de confianza y de derecha a izquierda. Lo cubren el UT de tabla, las propiedades de la pregunta 4 y T14 (spoof a la izquierda de la entrada del proxy).
- **Los IT con IP o tiempo son frágiles.** → `ServerClock::Manual` y `X-Forwarded-For` desde `127.0.0.1` de confianza: ni `sleep` ni `pause()`. Para abrir los 50 sockets del IT de RF-A7, el test avanza el reloj 2 s entre upgrades (1 token cada 2 s), sin tocar la cuota.
- **Llenar el tope del rate limiter deja fuera a los usuarios nuevos** (Área 7 de la spec). → Es el comportamiento elegido por la spec. `Retry-After: 60` coincide con la limpieza, y el IT con un tope reducido prueba que una llave existente conserva su bucket.
- **Contención del lock de admisión** en 1 vCPU. → Desde la desviación 8 es un `RwLock`: el chequeo de una llave existente lo toma compartido, y solo la admisión de una llave nueva y la limpieza (una vez por minuto) lo toman exclusivo. Una ráfaga de `/64` nuevas serializa las admisiones, acotada por el tope de llaves.
- **Un tipo nuevo filtra la IP por `Debug`.** → `IpKey`, `ClientContext`, `TrustedProxies` (en `Config`) e `InternalSecret` tienen `Debug` a mano con UT. Ningún `tracing::` nuevo, y el canario de `tests/logs.rs` corre con `trace` de terceros.
- **`governor` cambia su API interna** (`HashMapStateStore` como `Mutex<HashMap<…>>`). → Versión fijada con `=`. `CappedStore` usa solo items públicos (`StateStore`, `ShrinkableKeyedStateStore`, el alias y `InMemoryState`), y un bump de Dependabot que lo rompa falla en `cargo build`, no en silencio.
- **La web deja de compilar** por el `switch` exhaustivo. → Decisión 14, en el mismo PR que `just gen`.
- **Peer IPv4 mapeada** si algún día se escucha en `::` dual-stack. → Se normaliza el peer, las entradas de `X-Forwarded-For`, `X-Client-Ip` y `TRUSTED_PROXIES`, con UT para cada uno.

## Alternativas descartadas
- **`tower-governor`.** Una sola llave por capa, sin tope de llaves, y con `governor` arrastrando `quanta`, `dashmap`, `jitter` y `tonic` por defecto (decisión 6).
- **Un GCRA propio.** Son unas 60 líneas, pero la spec nombra `governor`, que ya está probado. El tope se resuelve con un store propio sobre el de `governor`.
- **Tope blando** (mirar `len()` sin el mutex de admisión). Se pasa del tope por tantas llaves como peticiones concurrentes, y una limpieza concurrente puede reinsertar una llave. El mutex lo deja exacto con un costo despreciable.
- **`tokio::time::pause()` como reloj de los IT.** Adelanta también los timers de hyper y del WS, y los tests con sockets reales se vuelven no deterministas.
- **`ipnet` para `TRUSTED_PROXIES`.** Solo se aceptan `/32` y `/128`, y una lista de direcciones exactas basta.
- **Tomar el peer con `into_make_service_with_connect_info`.** El server usa `serve_listener` con hyper directo (timeouts y apagado propios), así que la extensión se inserta por conexión con `MapRequest`.
- **Hacer obligatorio `INTERNAL_API_SECRET` ya en A.** Obligaría a tocar el compose y los `.env.example` de las dos apps antes de que `web` lo use, y lo volvería a tocar B (pregunta 1).

## Pruebas (cómo se demuestra que funciona)
- [ ] `just check` en verde, con la salida pegada en el PR.
- [ ] `just crate-age` y `just deny` en verde con el lock nuevo.
- [ ] **RF-A1:** UT de tabla de `client_ip`:
  - interna con `X-Client-Ip` válida, inválida o repetida;
  - peer de confianza con `X-Forwarded-For` de una línea, de varias y con entradas de confianza a la derecha;
  - entrada mal formada en el medio (devuelve el peer);
  - sin header;
  - peer no confiable con `X-Forwarded-For` (se ignora);
  - mapeadas en el peer, en `X-Forwarded-For` y en `X-Client-Ip`.

  Más las dos propiedades de la pregunta 4 con proptest y T14 (server) en `tests/client_ip.rs`: desde un peer no confiable, 10 upgrades con `X-Forwarded-For` distintos y el 11.º da 429. Desde uno de confianza, `spoof_i, 203.0.113.7` también da 429 en el 11.º.
- [ ] **RF-A2:** UT de que `2001:db8:1:2::a` y `2001:db8:1:2:ffff::1` dan la misma `IpKey` y `2001:db8:1:3::a` otra, de que una IPv4 y su mapeada dan la misma, y de que `format!("{:?}", key) == "IpKey(..)"` y no contiene ningún octeto. Propiedad del `/64` con proptest.
- [ ] **RF-A3:** UT de config:
  - vacía y ausente;
  - una y varias entradas, con y sin `/32` y `/128`, y con espacios;
  - `/24`, `/64`, `/0`, `/032`, `/33`, entrada vacía, texto, `0.0.0.0` y `::`;
  - `127.0.0.1/32` y `::1` aceptados con `http://localhost:5173` y rechazados con `https://torneo.example` y `http://torneo.example`.

  En cada error, `var = TRUSTED_PROXIES` y el valor no aparece en el `Display` ni en el `Debug`. IT de arranque en `tests/startup.rs`.
- [ ] **RF-A4:** UT de `KeyedLimiter` con reloj manual:
  - `AUTH_IP`: 5 pasan, la 6.ª da `Limited { 6 }`, y tras `advance(6 s)` pasa una más;
  - `WS_ANON_UPGRADE_IP`: 10 y la 11.ª;
  - tope 3: la 4.ª llave da `Full`, una existente sigue `Limited` (no se reinicia) y otra existente con saldo sigue en `Allow`;
  - `sweep()` tras la recarga libera llaves y entonces una nueva pasa;
  - memoria con 100 000 llaves < 20 MB (decisión 11).

  UT de la capa HTTP: 429 `{"error":"http.rate_limited"}` con `Retry-After`. IT del tope lleno en el `/ws` (`ws.rate_limited`). Bench manual con su número en el PR.
- [ ] **RF-A5 (T2, T15, T1):** en `tests/http.rs`, para `POST`, `PUT`, `PATCH`, `DELETE` y `PURGE` sobre `/api/health` y sobre una ruta inexistente:
  - sin `Origin` ni `Sec-Fetch-Site`, con `Origin` ajeno, con `Origin: null`, con dos `Origin` y con `Sec-Fetch-Site` igual a `same-site`, `cross-site` o `none` sin `Origin`: 403 `{"error":"auth.csrf_origin"}`, con `nosniff` y HSTS;
  - con `Origin` válido, o con `Sec-Fetch-Site: same-origin` sin `Origin`: 405 con `Allow` en `/api/health` y 404 en la ruta inexistente.

  `GET`, `HEAD` y `OPTIONS` siguen sin chequeo. T15 con un secreto válido:
  - sin `X-Client-Origin`, con uno ajeno y con dos: 403;
  - con `X-Client-Origin` válido y `X-Client-Fetch-Site` igual a `cross-site`, `same-site` o `none`: 403;
  - ausente o `same-origin`: pasa (405 o 404).

  T1: `tests/routes.rs` y los UT de `routes.rs`, incluido el manifiesto sintético con un `GET` que muta, que falla.
- [ ] **RF-A6 (T3):** con un `X-Internal-Secret` incorrecto, repetido o sin secreto configurado, y `X-Client-Origin` válido sin `Origin`: 403. Con un secreto incorrecto y un `X-Client-Ip` distinto en cada petición, el 11.º upgrade da 429: se usó el peer. UT de `is_internal` (correcto, incorrecto del mismo largo, otro largo, repetido, no ASCII).
- [ ] **RF-A7:** en `tests/ws.rs`:
  - con el reloj manual, 50 sockets abiertos desde la misma `/64` y el 51.º da 503 `{"error":"ws.ip_capacity"}`;
  - otra `/64` sigue entrando;
  - al cerrar uno, el siguiente entra (RAII), y tras cerrar todos, `IpSlots` queda vacío;
  - el upgrade 11 en ráfaga da 429 `{"error":"ws.rate_limited"}` con `Retry-After`;
  - orden: con `Origin` ajeno y la cuota agotada, 403; con la cuota agotada y el tope por IP lleno, 429; con el tope por IP lleno y el global lleno, `ws.ip_capacity`; un upgrade inválido no consume cuota.
- [ ] **RF-A8 (T21):** preflight `OPTIONS` con `Origin: https://evil.example` y `Access-Control-Request-Method: POST` a `/api/health`, a una ruta inexistente y a `/ws`, más un `POST` rechazado: ninguna respuesta lleva un header que empiece por `access-control-`.
- [ ] **RF-A9:** `tests/logs.rs` en `info`, `debug` y `debug,sqlx=trace,tungstenite=trace`, con `TRUSTED_PROXIES=127.0.0.1/32` y un `INTERNAL_API_SECRET` de prueba. Canarios:
  - una IPv6 canaria en `X-Forwarded-For` (con un upgrade aceptado y otro con 429);
  - otra en `X-Client-Ip` con el secreto válido;
  - `X-Client-Origin` y `X-Client-Fetch-Site` canarios con el secreto válido (403);
  - `Sec-Fetch-Site` canario;
  - el secreto mismo.

  Ninguno sale, y solo aparecen campos de `ALLOWED_FIELDS`. UT de `sensitive_request_headers_are_marked` con los 10 nombres.
- [ ] **Compose:** `scripts/__tests__/check-compose.test.mjs` pasa con el fixture actualizado y falla sin `TRUSTED_PROXIES` o con `10.250.0.0/28`. `node scripts/check-compose.mjs` pasa sobre `infra/`.
- [ ] **Web:** `echo.test.ts` con la fila de `ws.ip_capacity`, más `svelte-check` y `check-i18n` en verde (dentro de `just check`).

## Estimación de CI
Sin cambios en los workflows. El job `rust` suma unos 30 a 60 s: la compilación de `governor` y `proptest`, y los IT del `/ws` con 50 sockets, que corren en ms con el reloj manual. `policy` y `web` suman segundos.

## Desviaciones durante la implementación
1. **`TrustedProxies::parse` devuelve `Result<Self, InvalidTrustedProxies>`**, un tipo unitario con `thiserror`, en lugar de `Result<Self, ()>`: `clippy::result_unit_err` falla con `-D warnings` en una función pública. No lleva el valor.
2. **El limiter del upgrade no entra por `WsState::new`.** El handler del `/ws` lo lee de `AppState.rate` con un segundo extractor `State<Arc<RateLimits>>` (`FromRef`). El tope por IP es `WS_ANON_PER_IP` por defecto y los IT lo acortan con `WsState::with_ip_limit(n)`, no por `Settings`. Así `WsState::new` conserva su firma en todos los IT y los limiters viven en un solo lugar. `AppState::new` también conserva la suya y gana `with_net(NetState)`; el soporte de los IT expone `state_with(net, &settings)` y `state_with_ws(net, &settings, ws)`.
3. **El `sweeper` lo lanza `run_until_shutdown`** y no `serve()`: ahí vive el token que cancela la señal de apagado. Corre igual en el binario y en los IT que usan `run_until_shutdown`.
4. **`tests/support/http.rs` no cambia:** `send` ya agrega cada header con `append`, así que dos `Origin` o varias `X-Forwarded-For` salen como líneas distintas. Se agregó `ws::connect_with` para abrir sockets con headers extra.
5. **Cota del UT de memoria.** `HashMap::capacity()` es 7/8 de las cubetas, no las cubetas, así que el UT calcula `buckets = (capacity × 8 / 7).next_power_of_two()` y acota `buckets × (size_of::<(IpKey, InMemoryState)>() + 1) + 16`. Con 100 000 llaves, `capacity() = 114 688` (131 072 cubetas).
6. **Bench manual (decisión 11), release en macOS (`/usr/bin/time -l`):** RSS máximo de 1,56 MB con 0 llaves y de 8,34 MB con 100 000. La diferencia, unos 6,8 MB, supera la estimación de 3,3 MB: en el último crecimiento la tabla vieja (65 536 cubetas) y la nueva (131 072) conviven, y se suma el overhead del allocator. Sigue por debajo de 20 MB por limiter.
7. **Orden de los tests del paso 5.** Los tests de `Origin` se escribieron después del middleware y no se corrieron en rojo contra `read_only.rs`. Sí se vio fallar el test viejo (`write_methods_are_405_everywhere`: 403 en lugar de 405) antes de reescribirlo.
8. **El mutex de admisión pasa a `RwLock<()>`** (hallazgo de QA). Con el `Mutex` del plan, el chequeo de una llave existente no tomaba el lock. Si una limpieza borraba la llave K entre la búsqueda y la medición, y una llave nueva L ocupaba el lugar libre, la medición de K la volvía a insertar y la tabla quedaba en `max_keys + 1`. Ahora el chequeo de una llave existente toma el lock compartido, así que la limpieza (exclusiva) no puede correr en medio. La admisión de una llave nueva y la limpieza lo toman exclusivo. Lo prueba un UT determinista con un hook de test entre la búsqueda y la medición (`a_swept_key_in_flight_never_takes_the_table_over_the_cap`), que falló antes del arreglo.

## Revisión
### architect — 2026-10-28 — APPROVE
- Verifiqué el plan contra los RF-A1 a RF-A9 de la spec, contra ADR-0008 (D4, D5, D6, D7, la "Nota (2026-10-28)" y la "Enmienda 2026-10-28"), contra ADR-0006 D2 y ADR-0012 (A4, A8), y contra el código actual (`http/mod.rs`, `read_only.rs`, `ws/handshake.rs`, `config.rs`, `check-i18n.mjs`).
  - Cada RF tiene su decisión, sus archivos y su prueba.
  - El orden de trabajo escribe los tests en rojo antes de quitar la capa de solo lectura.
  - El orden del handshake coincide con RF-A7, y un upgrade inválido no consume cuota.
  - El tope de llaves es exacto y no desaloja ninguna llave, como pide RF-A4.
  - Ningún tipo nuevo filtra la IP ni el secreto por `Debug`.
  - No toca el motor (ADR-0001).
  - Alguien que no vio la conversación lo puede implementar: nombra los archivos, los pasos, los commits, los riesgos y las dependencias con `crate-age` antes de compilar.
- Respuestas a las preguntas del plan. Ninguna cambia el plan, y quedan como decisiones 17 a 23:
  1. **`INTERNAL_API_SECRET` opcional en A y obligatorio en B: sí.** Sin secreto, ninguna petición es interna, que es fallar hacia lo seguro. Con secreto, se valida igual que pedirá RF-B18. El plan-b lo vuelve obligatorio y le pone valor de desarrollo a los dos `.env.example`.
  2. **`TRUSTED_PROXIES: 10.250.0.2/32` en el compose de producción ya en A: sí.** Es el valor de ADR-0006 D2. Sin él, la fase A juntaría todo el sitio en la `IpKey` del proxy. `check-compose` con valor exacto (la subred falla) es lo correcto.
  3. **La prueba de "la petición 11 de `start`": tienes razón.** La frase de la spec es un error mío: con la ráfaga 5 de D7, la 6.ª en ráfaga da 429. La cuota es la de D7 (`per_minute(10)`, ráfaga 5), y la prueba válida es la tuya: la 6.ª da 429 con `Retry-After: 6`, y tras 6 s pasa una más, sobre un router de test en A y sobre `start` en B. El plan-b repite el IT sobre la ruta real.
  4. **La propiedad de `client_ip`: acepto tu formulación** ("si el resultado está en `TRUSTED_PROXIES`, es el peer y no quedaba ninguna entrada válida fuera de la lista"), más la segunda propiedad (con un peer que no es de confianza, el resultado es el peer). La de la spec estaba mal escrita: RF-A1 manda devolver el peer en ese caso.
  5. **Los textos de `auth.csrf_origin` y `http.rate_limited` en la fase D: sí.** `check-i18n` no relaciona los códigos con las llaves, y en A ninguna pantalla los muestra. Condición para el plan-d: un `switch` exhaustivo (o un test) que exija una llave en en y es para cada código de `AuthErrorCode`, `ApiErrorCode` y `WsErrorCode`. Así el criterio del intent ("todo código nuevo tiene su texto") queda verificado por máquina.
  6. **El 403 antes del 404/405 en métodos inseguros: sí.** D4 aplica a "toda petición no segura", sin importar la ruta, y no dar 404 a un origen ajeno no filtra qué rutas existen. Con `Origin` válido se conservan el 404 y el 405.
  7. **Rechazar `0.0.0.0`, `::`, multicast y broadcast en `TRUSTED_PROXIES`: sí.** Ninguna puede ser el peer de un proxy, así que aceptarlas solo esconde un error de configuración. Que el UT cubra una de cada tipo (`224.0.0.1`, `ff02::1`, `255.255.255.255`).
- Sin cambios pedidos. Las erratas de la spec (puntos 3 y 4) no hace falta enmendarlas: el plan aprobado es la fuente para la fase A, y el plan-b repite el punto 3 sobre la ruta real.

