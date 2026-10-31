---
id: "010"
title: "Scheduler de peleas en vivo, fase B: canales, hub, clientes lentos y dev-fights"
type: plan
status: approved
risk: high
author: backend-engineer
reviewers: [architect]
approvals:
  - by: architect
    date: 2026-10-31
linear: TA-13
depends_on: ["001", "008", "010"]
created: 2026-10-31
updated: 2026-10-31
---

# Plan: Scheduler de peleas, fase B (desde spec.md 2026-10-31)

> [intent](./intent.md) · [spec](./spec.md) · [plan-a](./plan-a.md) · [plan-c](./plan-c.md) · ADRs: [0002](../../decisions/0002-commit-reveal-semilla.md), [0006](../../decisions/0006-despliegue-y-ci.md) D10, [0008](../../decisions/0008-autenticacion.md) D6, [0012](../../decisions/0012-logs-y-cabeceras-de-seguridad.md) A4 y A8

Escrito en solo lectura, sin compilar. Parte de `main` en `e69fc99`, con el PR #69 (fase A) ya fusionado: `apps/server`, `crates`, `scripts`, `justfile` y `.github` son idénticos a los de `543a6d7`, contra el que se leyó. Se implementa en la rama `ta-13-fight-events`.

## Alcance
**Entra:** RF37 a RF39, RF41 a RF45, el hub como sink de RF40, el uso de `events_of_settled` (RF23), y las partes de B de RF26, RF27 y RF28. También los RNF de memoria y carga (`fights_fanout.rs`) y los criterios "Sincronizada" y "Llegar tarde" en su nivel de integración.

| RF parcial | Ya está en A | En B |
|---|---|---|
| RF23 | Tipos sellados y `events_of_settled` | Su uso para `last` tras un reinicio (RF38) |
| RF26 | Forma de `FightSummary` y `FightsCurrent` | Forma de las tres variantes nuevas de `ServerMessage` |
| RF27 | `ROUTES` × estados × identidades, y `self` | `schedule` y `fight:{id}`, y ningún evento antes de su hora por socket |
| RF28 | Corrida en `trace` sin sockets | La misma corrida con un socket conectado, con control positivo |
| RF40 | Tarea, tick y prueba con un sink | El hub conectado, y la misma hora medida en el socket |

**No entra:** renderer, vista y cliente WS (C y D); el trabajo de cierre de la spec (`CLAUDE.md`, `.env.example`, README, glosario, enmienda de RF-B13 en la spec de 001), que es de D; el script de `FIGHT_SCRIPT_PATH` del e2e, que es de D; escritura de `voided` y apuestas (TA-27); la prueba de carga en el VPS (ADR-0008, con TA-8).

**Preocupaciones 7 y 8, alcance exacto.** La spec no pide en B una prueba de carga real: la 7 la deja para TA-8. B implementa RF42, comprueba que siguen los topes (50 por IP, global, `ws.capacity`) y mide 2000 sockets suscritos en local. La 8 sí se decide aquí: la medición dice si conviene partir `fight_sync` y bajar el tope (ver "Medición").

**Se puede probar al terminar:** un cliente WS de prueba ve el vivo y entra tarde. Test 12 completo. Nada delata antes de `Live`.

## Comprobado en el código
Leí el código y las dependencias fijadas en `Cargo.lock`. Nada de esto está medido todavía.

1. **`read_buffer_size` no está en RF43, y es lo que más memoria usa.** tungstenite 0.29 usa 128 KiB por defecto. `FrameCodec::new` reserva esa capacidad y `read_in` hace `in_buffer.resize(…, 0)` en cada lectura, así que escribe ceros en todo el buffer y esas páginas pasan a ser residentes. Con 2000 sockets son unos 250 MiB, contra un `mem_limit` de 256 MiB. Hoy `handshake.rs` no lo fija. Ver DP-B1.
2. **`out_buffer` no se encoge.** `write_out_buffer` usa `Vec::drain`, que conserva la capacidad, y no hay API para reducirla. Un socket conserva hasta cerrarse la capacidad de su mensaje más grande, aunque no esté atascado. La preocupación 8 habla solo de sockets atascados durante 5 s. Como RF39 manda `fight_sync` en cada transición, el de `Settled` llega con todos los eventos a cada suscriptor. Ver DP-B2.
3. **Tamaños.** En los 7 dorados, `fight_sync` tendría como mucho 153 eventos y unos 14 KB con el resumen, y el evento más largo mide 111 bytes. En la cota de 418 eventos son unos 47 KB con el resumen, y en el tope de guarda del motor (`max_events = 512`), unos 57 KB. Todo cabe en los 72 KiB de RF43: `buffer_frame` falla solo si `frame + out_buffer` los supera, y cada `send` vacía el buffer o el socket se cierra.
4. **`only_me_balance_overrides_its_ts_type` prohíbe `ts(type = …)`.** `Channel` usa `ts(as = "String")`, que `ts-rs` 12 admite en enums. En TypeScript queda `type Channel = string`. `apps/web` no usa `Channel`, solo `Extract<ServerMessage, …>` en `echo.ts`.
5. **`publish` solo corre al final de la pasada.** `step` encadena `Close` y `Recompute`, así que hoy el estado `betting_closed` se publicaría después de calcular el log. Por socket, eso haría depender del cálculo la hora de un mensaje. Y en `go_live` o al reanudar, el primer tick sale antes de que el hub conozca la fila. Ver "Publicar antes de calcular y de emitir".
6. **La telemetría ya corta lo ajeno.** Lo que no es de `server::*` escribe solo `event: external`, con tope en `warn`. No hay `tracing-log`, así que el `trace!("writing frame {frame}")` de tungstenite no llega al log. RF28 con socket lo comprueba en vez de suponerlo.
7. **`release_due` es `pub` por una razón.** `fights_settle.rs:319` y el doctest positivo de `sealed.rs` lo llaman desde fuera del crate. Ver "`release_due(u32::MAX)`".
8. **Bug de la fase A, confirmado leyendo el código: un final puede no llegar nunca por socket.** `store::last_settled` ordena por `opens_at DESC` (`store.rs:169`), no por cuándo se liquidó. Durante una detención, N sigue sin liquidar y N+1 se cancela por `missed_window`, que RF15 permite con detención, así que `last` es N+1. Cuando N se liquida, `last` sigue siendo N+1, porque su `opens_at` es mayor, así que `GET /api/fights/current` ya hoy no da la última liquidada que pide RF35. Con la regla de borrado que tenía este plan, además, N saldría del hub sin difundir su resumen `settled`. Se reproduce primero con un test. Ver "Qué peleas viven aquí" y DP-B8.
9. **Dos excepciones reales para las barreras.** `profile/name.rs:175` ya usa `#[path = "name_tests.rs"]` bajo `cfg(test)`, y esa línea exacta queda como única excepción. `include!` hoy solo aparece en un comentario de `version_rule.rs`. El `include!` real está en `build.rs`, fuera de `src`.
10. **Las pruebas suponen una sola feature de desarrollo.** `startup.rs:182` y `:351`, `ws.rs:178` y `support/bin.rs:85` usan `cfg(not(feature = "dev-auth"))` o `cfg(feature = "dev-auth")` para decir "esta build exige loopback". Con `dev-fights` sola, eso sería falso. Pasan a `any(…)` donde significan eso.

## Desviaciones y vacíos de la spec
Ninguna toca un ADR. Architect las recoge en `spec.md` como "Enmienda 3 (fase B, 2026-10-31)", que entra en la rama de esta fase antes de tocar código. Architect y security-reviewer aceptaron todas (ver "Revisión"). Este plan no redacta esa enmienda.

- **DP-B1. Sumar `read_buffer_size` a RF43. Security: no.** Propongo `.read_buffer_size(8 KiB)`. El server solo lee mensajes de hasta `MAX_MESSAGE_BYTES` (4 KiB), así que cabe sin cambiar nada del protocolo. Sin esto no se cumple el RNF de memoria, con cualquier diseño del hub. La primera corrida de `fights_fanout.rs` compara 128 y 8 KiB, y el número va en el PR.
- **DP-B2. Corregir la preocupación 8. Security: no.** El costo no es solo un pico de 5 s de los atascados: todo socket retiene el tamaño de su mayor mensaje. El caso real son unos 14 KB. El peor caso, unos 47 KB, deja un piso de `2000 × (8 + 47 KiB)` más las tareas, unos 110 MiB. Sigue por debajo del límite, pero es permanente. No cambia el diseño salvo que lo diga la medición.
- **DP-B3. `schedule_update` cada vez que cambia `halted`. Security: sí.** RF39 nombra transiciones y detención, y RF35 dice que `halted` es nulo sin el bloqueo. Sin este aviso, un cliente conectado seguiría viendo `halted: false` sin que el server lo sepa, y el paso de nulo a `false` al terminar la evaluación inicial no llegaría a nadie. `halted_state` cambia en cuatro métodos de `FightsShared` (`publish`, `lock_taken`, `lock_lost` e `initial_evaluation_done`), y `runner::run` llama a `lock_lost` también tras un pánico, así que la comparación y el aviso corren en los cuatro. No expone nada nuevo: el mismo valor ya sale en `GET /api/fights/current`.
- **DP-B4. `server_time` de un mensaje compartido. Security: no.** RF43 pide compartir el mensaje y RF44 pide la hora del server. Un `fight_sync` o `schedule_update` en caché lleva la hora de cuando se serializó. Propongo reserializar si tiene más de 250 ms al servir una suscripción. Solo puede atrasar la muestra, y el RNF de sincronización se queda con el máximo. Con eso, una avalancha de reconexiones serializa como mucho 4 veces por segundo y por mensaje.
- **DP-B5. Canal de una pelea que sale del hub. Security: no.** La spec no lo define. El socket queda suscrito sin recibir nada, y la suscripción sigue contando para el tope de 8 hasta que el cliente la cierre. No se le avisa, para no agregar otro mensaje. Resuscribirse a ese canal ya abierto responde `subscribed`, como pide RF38.
- **DP-B6. Forma de `dev-fights` que RF45 no fija. Security: sí.** Un script ilegible o inválido da `config.invalid` con `FIGHT_SCRIPT_PATH`, y el server no arranca. La fila del script se elige por el rango de la pelea en la tabla (ver "`dev-fights`"). Fuera de loopback, `config.invalid` con `var = DEV_FIGHTS`, como `DEV_AUTH`. La puerta corre antes de leer el archivo.
- **DP-B7. Límite de los `fight_sync` que pide un `subscribe`. Security: decidido por security-reviewer.** RF38 frena el reenvío en un canal abierto, pero `unsubscribe` + `subscribe` pide otro mensaje completo. Límite por socket con `RateWindow::new(3, 60 s)`. Al pasarse, el socket se cierra con `ws.rate_limited`. Los `fight_sync` que envía el server por su cuenta (transición o socket atrasado) no cuentan, y `schedule_update` tampoco. Baja la cota del conjunto de unos 47 a unos 4,7 MB/s de media.
- **DP-B8. El hub conserva una pelea liquidada hasta haber difundido su final. Security: aprobada.** RF38 dice "no liquidadas y la última liquidada". Una pelea liquidada que nunca llega a ser `last`, porque otra se liquidó justo después, tiene que seguir en el hub hasta avisar a sus suscriptores, unos segundos más de lo que dice RF38. Solo expone datos ya públicos. Además, `last_settled` pasa a ordenar por `settled_at`, que es lo que RF35 ya dice y no cambia el contrato.

### Notas de seguridad pedidas
- **RF28 con socket:** en el binario de un test (`fights_logs.rs`) y en el real (`fights_binary.rs`), siempre con control positivo: el socket tiene que haber recibido `end`, o la ausencia de canarios no prueba nada.
- **R4, `elapsed` negativo.** `Stopwatch` ya da `None` antes de la hora y `release::run` no libera. En B importa porque un socket ya puede mirar: la ruta tardía (`live_started_at = now + 10 s`) publica `live` antes de empezar, y un reloj atrasado al reanudar hace lo mismo. Por eso: `fight_sync` con `state = live` y `events: []`, sin `intro` ni `fight_events` hasta `live_started_at`, probado por socket. El hub nunca calcula con horas: solo guarda lo que le da el sink.
- **`release_due(u32::MAX)`: B lo cierra en lo que una barrera puede cerrar, es decir, para errores.** Ver su apartado. Para código escrito a propósito sigue el "Límite conocido" de RF24.
- **`dev-fights` fuera de producción:** barreras en "Scripts". El nombre de la feature y `FIGHT_SCRIPT_PATH` no pueden estar en el `Dockerfile`, la cadena se busca en el binario de `rust`, `images` y `release.yml`, y `--expect-dev` la exige en el del e2e. Su puerta se prueba con la feature sola, porque con las dos la de `dev-auth` la ocultaría.
- **`check-compose.mjs:927`: B lo arregla,** porque RF45 obliga a tocar ese archivo. Va en un commit propio, para que devops lo revise aparte.

## Archivos que cambian
| Archivo | Cambio |
|---|---|
| `crates/protocol/src/ws.rs` | `FightId(u32)`; `Channel` con `Schedule` y `Fight(FightId)`; `Display` y `parse` (RF37); serde por cadena; `ts(as = "String")`; `as_str` desaparece. Tres variantes de `ServerMessage` (RF39). Tests de forma exacta |
| `crates/protocol/src/lib.rs` | Exporta `FightId` |
| `packages/protocol-ts/src/**` | Solo con `just gen` |
| `apps/server/src/fights/hub.rs` | Nuevo: `FightHub`, `Cursor`, caché de mensajes, `orphans` e `impl EventSink`. Sin `tracing` |
| `apps/server/src/fights/board.rs` | `FightsShared` posee el hub. `publish` recibe un `BoardUpdate`. `publish`, `lock_taken`, `lock_lost` e `initial_evaluation_done` comparan y avisan si cambió `halted_state` o el tablero, sin contar `server_time` (DP-B3) |
| `apps/server/src/fights/release.rs` | `HubAndSink`, sink compuesto: el hub y el `sink` de `FightSettings`. Sin cambios en `run` |
| `apps/server/src/fights/runner.rs` | Publica tras cada transición y antes de emitir. Lee el final de las peleas huérfanas del hub en `reload` (DP-B8). Decodifica los eventos de `last` una vez por pelea. Con el feature, el script al anunciar y abrir |
| `apps/server/src/fights/sealed.rs` | Con el feature, `open_window_with_seed`, con su parte común con `open_window` |
| `apps/server/src/fights/store.rs` | `last_settled` ordena por `settled_at DESC, id DESC`. Con el feature, `fight_rank` |
| `apps/server/src/fights/script.rs` | Nuevo, todo bajo `cfg(feature = "dev-fights")`: `FightScript`, única mención de `FIGHT_SCRIPT_PATH` |
| `apps/server/src/fights/mod.rs` | `pub mod hub`; `script` bajo `cfg`; `FightSettings.script` bajo `cfg` |
| `apps/server/src/config.rs` | Con el feature: la puerta de loopback y la carga del script |
| `apps/server/src/app.rs` | Con el feature: pasa `config.fight_script` a `settings.fights` en `serve` |
| `apps/server/src/ws/channels.rs` | `subscribe` recibe qué peleas existen; `is_empty`. El test T18 pasa a ser el noveno real |
| `apps/server/src/ws/session.rs` | Canales públicos, `Cursor` por canal, brazo del `watch`, RF42, el límite de DP-B7 y el arreglo de `self` (RF38) |
| `apps/server/src/ws/rate.rs` | Constantes del límite de DP-B7, con el `RateWindow` que ya existe |
| `apps/server/src/ws/handshake.rs` | Los tres tamaños de buffer; pasa el hub a la sesión |
| `apps/server/src/ws/mod.rs` | `WsConfig` suma `read_buffer`, `write_buffer`, `max_write_buffer` y `subscribe_sync_limit` |
| `apps/server/Cargo.toml` | `dev-fights = []`. El comentario no puede nombrarla: `check-compose` cuenta la palabra también en comentarios |
| `apps/server/tests/fights_ws.rs` | Nuevo: RF38 a RF40, RF42 a RF44, R4, reconexión, `voided` |
| `apps/server/tests/fights_ws_parity.rs` | Nuevo: RF41 |
| `apps/server/tests/fights_script.rs` | Nuevo, bajo `cfg`: RF45 |
| `apps/server/tests/fights_script_logs.rs` | Nuevo, bajo `cfg`, con exactamente un test (`test_layout.rs`): RF28 con la semilla del script como canario |
| `apps/server/tests/fights_fanout.rs` | Nuevo, `#[ignore]`: medición |
| `apps/server/tests/fights_no_leaks.rs` | RF27 por socket, en cada estado y con cada identidad |
| `apps/server/tests/fights_logs.rs`, `fights_binary.rs` | RF28 con socket |
| `apps/server/tests/fights_source_guard.rs` | Barreras nuevas, con sus casos negativos |
| `apps/server/tests/ws.rs`, `shutdown.rs`, `ws_session.rs` | Campos nuevos de `WsConfig`; casos de RF42 |
| `apps/server/tests/startup.rs`, `support/{ws,fights,bin}.rs` | Puertas de `dev-fights`, cada `cfg` que significa "exige loopback" (hallazgo 10) y ayudas de socket. `support/ws.rs` devuelve también la respuesta del upgrade |
| `apps/server/tests/fights_lifecycle.rs` | Reproduce el fallo de `last` (hallazgo 8) |
| `scripts/check-compose.mjs`, su test y `scripts/__fixtures__/check-compose/valid/apps/server/Cargo.toml` | Reglas para las dos features y la línea 927 |
| `scripts/check-release-binary.mjs` y su test | `FIGHT_SCRIPT_PATH` |
| `justfile` | `check-rust`, `_test-rust` y `e2e` con las dos features; clippy y puerta con `dev-fights` sola. `just dev` no cambia |
| `.github/workflows/ci.yml` | `e2e-server`: `--features dev-auth,dev-fights`. El artefacto conserva su nombre |

`crates/engine`, `crates/content`, `crates/sim`, el ledger, `apps/web`, `infra/*`, `telemetry.rs` (`ALLOWED_FIELDS`), `Cargo.lock` y `apps/server/Dockerfile` no cambian. No hay dependencias nuevas.

## Diseño de la implementación

### Protocolo (RF37, RF39, RF44)
- `Channel` se serializa con `collect_str` y se deserializa con `parse`, así que nunca hay dos formas. `parse` acepta `self`, `schedule` y `fight:` más un decimal canónico de 1 a `i32::MAX`. Los rechazos son los que lista RF37, más `fight:`, `fight:-1` y `fight:٣` (dígitos no ASCII). La deserialización con una cadena inválida falla.
- `fight_sync { fight, server_time, events }`, `fight_events { fight_id, from_index, events, server_time }` (`fight_id: u32`, igual que `FightSummary.id`, y `from_index: u32`) y `schedule_update { board }`. `board` ya trae `server_time`, así que RF44 se cumple sin duplicarlo.
- Sin tipos de 64 bits. `FightId` no se exporta a TypeScript.

### Hub (RF38, RF43)
Un hub por proceso, dentro de `FightsShared`, así que `FightsShared::new(clock)` no cambia de firma.

```rust
pub struct FightHub { clock: FightClock, state: RwLock<HubState>, changes: watch::Sender<u64> }
struct FightEntry {
    version: u64,                  // sube cuando cambia el resumen
    summary: FightSummary,
    settled_seen_at_ms: Option<i64>, // cuándo llegó su resumen `settled`
    released: Vec<FightEvent>,     // solo lo liberado
    last_batch: Option<(usize, Utf8Bytes)>, // from_index y fight_events ya serializado
    sync: Mutex<Option<Cached>>,   // fight_sync compartido: (version, released, at_ms, texto)
}
pub struct Cursor { version: u64, released: usize }
```

- **Por socket no hay nada en el hub.** Cada socket tiene un `watch::Receiver<u64>` y sus `Cursor` en su propia tarea, como mucho 8. `Utf8Bytes` envuelve `Bytes`, así que clonarlo no copia el mensaje.
- **`next_for(id, &mut Cursor) -> Option<Utf8Bytes>`:**
  1. Sin entrada: nada (DP-B5).
  2. `version` distinta: `fight_sync`.
  3. `released` igual: nada.
  4. `last_batch` empieza en `cursor.released`: ese lote.
  5. Si no: `fight_sync`. Así salta un cliente lento, sin cola.

  El cursor avanza bajo el mismo cerrojo de lectura con el que se eligió el mensaje. Ningún guard cruza un `await`, y clippy lo vigila (`await_holding_lock`).
- **`EventSink::publish(id, from_index, events)` es idempotente.** Una tarea reconstruida (R7) o un pánico del bucle vuelven a empezar en `from_index = 0`, y el hub solo agrega desde su longitud. Si no hay nada nuevo, no hay aviso, así que nadie recibe un lote repetido. Con un salto (`from_index > len`) o una pelea desconocida, descarta: es inalcanzable por "Publicar antes…", y un test lo cubre.
- **Eventos en un estado que no es `live`:** el hub los descarta. Es una segunda barrera contra un error de orden, con test unitario.
- **Una pelea nunca queda sin aviso:** cada cambio hace `send_modify` del contador. `watch` junta avisos, y por eso el socket siempre consulta con `next_for` en vez de confiar en cuántos llegaron.
- **Qué peleas viven aquí (RF38, DP-B8):** las que trae `BoardUpdate`: las filas no liquidadas, `last` y los finales de las huérfanas. Reglas de borrado, sin mirar el orden de los `id`:
  - Una entrada sin liquidar nunca se borra por faltar en las filas: es una **huérfana**.
  - `FightHub::orphans(filas, last)` da esas entradas. `Runner::reload` las lee con `store::by_id` antes de poner `dirty = false`, y envía su resumen `settled` en `BoardUpdate`. Un error de la base aborta la pasada y se reintenta. Como el hub es la única fuente, sirve igual tras liquidar, tras una cancelación, con la CLI (RF59) y tras un pánico del bucle, sin una lista aparte en el runner.
  - **Solo un resumen `settled` cuenta como final.** Si `store::by_id` da otro estado, por ejemplo tras restaurar un backup, o `None`, la entrada se borra sin difundir. Si la fila no decodifica (`sqlx::Error` que no es de la base ni de conexión), también, con `fights.orphan_invalid` en `warn`, sin abortar la pasada.
  - **Retención, una sola regla:** una entrada `settled` que no es `last` se borra en un `publish` posterior, cuando ya pasaron 60 s desde que llegó su resumen (`SETTLED_RETENTION_MS`). Un socket envía canal por canal, hasta 8 con `close_grace` cada uno, unos 40 s. Un `last` anterior también espera. Una entrada pesa como mucho unos 57 KB y estas filas son raras.
- **`last` tras un reinicio:** el runner guarda `(id, eventos)` y llama a `sealed::events_of_settled` solo cuando cambia el `id` de `last`. Si la entrada del hub tiene menos eventos que el log, los reemplaza: ya son públicos. Con `BadLog`, el resumen sale sin eventos y se registra `fights.last_log_invalid` en `warn`, con `error_kind = bad_log`, desde `runner.rs`. Una cancelada o una anulada no tienen log.
- **Caché:** `fight_sync` y `schedule_update` se serializan al primer uso por versión y se reusan mientras tengan menos de 250 ms (DP-B4). `fight_events` se serializa una vez al publicar, en la tarea de liberación, y nunca espera a la base.

### Publicar antes de calcular y de emitir (RF8, RF39, RF41)
`Runner::publish` pasa a correr también justo después de `open`, `close` y `go_live`, y dentro de `start_emission_if_ready`, **después de comprobar `state == Live`** y justo antes del `spawn`. `recompute` llama a esa función al terminar `compute`, también en `betting_closed`. Si publicara antes de comprobar el estado, cualquier cambio pendiente del tablero saldría a una hora que depende de cuánto tardó el cálculo.
- `schedule_update` de `betting_closed` sale antes de calcular el log. Sin esto, la hora de ese mensaje dependería de la duración de `simulate`. Se prueba reteniendo `compute_gate`: el mensaje llega con la puerta cerrada.
- El hub conoce la fila en `live` antes del primer tick, también al reanudar sin transición.
- `publish` es seguro a mitad de pasada: `halted_state` sigue nulo hasta `initial_done`. La deduplicación del tablero hace que pasadas de más no generen mensajes.
- La misma comparación corre en `lock_taken`, `lock_lost` e `initial_evaluation_done` (DP-B3). Así, el paso de nulo a `false` y el de `false` a nulo se difunden aunque no haya ninguna pasada de por medio.

El runner siempre emite a `HubAndSink`. Así, olvidar conectar el sink en `serve` es imposible, y `FightSettings.sink` queda como observador de los tests, sin cambiar los de A.

### Sesión (RF38, RF39, RF42, RF43)
En `serve`, un brazo nuevo después de `self_rx`:

```rust
changed = hub_rx.changed(), if schedule.is_some() || !fights.is_empty() => { … }
```

Por cada canal abierto, consulta el hub y envía con `send(…, close_grace)`. Si falla, `End::Drop`, como hoy. `changed()` es seguro de cancelar, y un cambio ocurrido durante un `send` lento vuelve a despertar el brazo.
- **Suscribir:** responde primero `subscribed` y después el mensaje inicial, solo si el canal no estaba abierto. El orden se comprueba en un test.
- **`fight:{id}`:** se consulta el hub en memoria, nunca la base. Comprobaciones, en orden: ya abierto (`subscribed`), pelea desconocida (`ws.forbidden_channel`), tope (`ws.too_many_subscriptions`).
- **Límite de `fight_sync` (DP-B7):** después de esas tres comprobaciones y antes de responder `subscribed`, `sync_requests.allow(now)`. Si lo pasa, `policy(WsErrorCode::RateLimited)`, sin abrir el canal. Un canal ya abierto no gasta, porque no envía nada. El valor vive en `WsConfig.subscribe_sync_limit`, para que el escenario F compare con y sin límite.
- **`self`:** solo `Channel::SelfChannel` abre o cierra `self_subscription`. Hoy lo hace cualquier canal (RF38).
- **RF42:** el brazo de `idle_deadline` lleva `if subscriptions.is_empty()`. Como `unsubscribe` ya pone `last_data = now`, el socket que queda vacío tiene sus 120 s completos. El ping y `ws.pong_timeout` no cambian.
- **`handshake.rs`:** los tres tamaños desde `WsConfig`, por defecto 8 KiB de lectura, 8 KiB de escritura y 72 KiB de máximo. Como tungstenite entra en pánico si el máximo no supera al de escritura (`assert_valid`), un test de `WsConfig::default()` lo fija.

### `release_due(u32::MAX)`
Decisión: **se cierra para errores con la barrera de fuente, sin cambiar la firma.** Reglas nuevas en `fights_source_guard.rs`, por tokens y sobre `apps/server/src`:
- `release_due` solo en `sealed.rs` y `release.rs`.
- `SealedLog` y `FinalLog` solo en `sealed.rs`, `release.rs` y `runner.rs` (hoy ya es así).
- `stopwatch` solo en `clock.rs` y `release.rs`: `clock.stopwatch(i64::MIN)` es la otra forma de pedir el log entero.
- En `release.rs`, una sola llamada, con el argumento `elapsed`, y ningún token `MAX`.
- `events_of_settled` solo en `sealed.rs` y `runner.rs`.
- `hub.rs` no nombra `sealed`, y `ws/` no nombra `sealed`, `runner` ni `store`.
- `into_final` solo en `sealed.rs` y `release.rs`.
- `fight_secrets` sin distinguir mayúsculas: en SQL, `FIGHT_SECRETS` es la misma tabla.
- `include!` en ningún archivo de `src`, y `#[path` tampoco, salvo la línea exacta `#[path = "name_tests.rs"]` de `profile/name.rs` (hallazgo 9). Se busca sin comentarios y sin espacios, así que `include !` y `# [path` cuentan.
- `open_window_with_seed` solo en `sealed.rs` y `runner.rs`.
- `script.rs` sin `tracing`, igual que `hub.rs`, `sealed.rs` y `release.rs`.
- `open_window_with_seed` y `fight_rank` van inmediatamente después de `#[cfg(feature = "dev-fights")]`.

La función que busca lleva un caso negativo por forma, incluidos `log.release_due(u32::MAX)` en otro módulo, `r#release_due`, `log.into_final(0)`, `"DELETE FROM FIGHT_SECRETS"`, `include!("x.rs")`, `#[path = "x.rs"]` y `# [ path = "name_tests.rs" ]` en otro archivo. El límite de RF24 sigue escrito: alguien que edite `release.rs` a propósito lo salta, y eso lo ve la revisión. `fights_settle.rs:319` no cambia, porque la barrera no recorre `tests/`.

### `dev-fights` (RF45)
- **Forma del archivo:** `[{ "seed": "<64 hex>", "spec_a": {…}, "spec_b": {…} }, …]`, con al menos una fila.
- **Carga:** `FightScript::from_json` decodifica cada `CharacterSpec`, lo pasa por `validate` y guarda `canonical_spec_json`, `max_hp` y la semilla como `[u8; 32]`. Ni `FightScript` ni su tipo intermedio de deserialización tienen `Debug` o `Serialize`.
- **Errores:** el de `serde_json` se descarta en la misma línea (`map_err(|_| ScriptError::Invalid)`), porque su texto puede citar la semilla. `ScriptError` no lleva datos.
- **Dónde se lee:** en `Config::from_env`, bajo `cfg`. Va en `config.fight_script`. Como su tipo no tiene `Debug`, el `Debug` manual de `Config` no puede incluirlo, y un test lo comprueba. `serve` lo copia a `settings.fights.script`, y esa misma entrada es la "fuente de semillas" de las integraciones. En release ni se compila.
- **Ciclo sin estado:** `fila = rango % filas`, con `rango = count(*) WHERE id < $id`. Las filas nunca se borran (RF4), así que el rango es estable, sobrevive a un reinicio y no le afectan los huecos de la secuencia. Un contador en memoria se reiniciaría y separaría semilla y specs.
- **Anunciar:** con script, el runner no usa el roster ni `pick_matchup`. Inserta los specs de la fila que corresponde al rango siguiente (`matchup_round = 1`, `roster_sha256` de los slugs del script). Funciona sin roster.
- **Abrir:** si los bytes de la fila coinciden con los specs de la pelea, `sealed::open_window_with_seed`. Si no, por ejemplo una fila anunciada antes de configurar el script, `open_window` con `OsRng`. `fight_secrets` sigue solo en `sealed.rs`.
- **Puerta:** con el feature, siempre, con o sin variable: `PUBLIC_ORIGIN` y `BIND_ADDR` de loopback, como `dev-auth`. **Corre antes de leer el archivo:** una ruta inexistente fuera de loopback da `DEV_FIGHTS`, no `FIGHT_SCRIPT_PATH`. Con las dos features, `DEV_AUTH` saldría primero y ocultaría esta puerta, así que se prueba con `dev-fights` sola.

### Scripts
Revisa devops-engineer.
- **`check-compose.mjs`, `Dockerfile`:** la regla actual se repite por palabra, y los mensajes de `dev-auth` no cambian, así que sus tests quedan como están. Nuevas: `no-dev-fights` y `no-fight-script`.
- **`check-compose.mjs`, `Cargo.toml`:** `dev-fights = []` exactamente una vez, la palabra fuera de su definición falla, y `default` no llega a ninguna de las dos, directa o transitivamente. La regla de fixtures en sincronía obliga a actualizar el `Cargo.toml` de `valid/`.
- **`check-compose.mjs:927`:** hoy `has` solo busca que exista la línea, así que una segunda `ENV CONTENT_DIR=/tmp` pasa y gana. Regla nueva: en la etapa final, fuera de las dos líneas exactas, ninguna instrucción nombra `CONTENT_DIR` ni `/content`, y cada línea exacta aparece una vez. Casos: línea extra de `ENV`, `ENV A=1 CONTENT_DIR=/x` y un segundo `COPY`.
- **`check-release-binary.mjs`:** `FIGHT_SCRIPT_PATH` se suma a lo prohibido en release y a lo exigido con `--expect-dev`, con su propio mensaje. El test "the dev-auth binary passes" cambia su binario de prueba, porque cambia el contrato.
- **Consecuencia de `--expect-dev`:** el binario del e2e necesita las dos features, en `just e2e` y en `e2e-server`. Sin variable, `dev-fights` no hace nada, así que los e2e de hoy siguen igual.
- **`justfile`:** `_test-rust` corre `--features dev-auth,dev-fights`, donde viven las integraciones con script. `check-rust` suma clippy con las dos y con `dev-fights` sola, y conserva el de `dev-auth`, que es lo que compila `just dev`. `_test-rust` suma `cargo test -p server --features dev-fights --test startup dev_fights`, solo para esa puerta, porque el resto de la suite ya corre con las dos.

### Medición (`fights_fanout.rs`)
`#[ignore]`, no corre en `just check` ni en CI. Comando, en la cabecera del archivo: `ulimit -n 8192 && cargo test --release -p server --test fights_fanout -- --ignored --nocapture fanout_server`. Falla con un mensaje claro si el límite de descriptores no alcanza.

- **Proceso:** el test tiene el server en proceso, con el mismo `#[global_allocator]` que `main.rs`, porque si no el RSS no serviría. Lanza a sí mismo como hijo (`current_exe`, test `fanout_clients`) para que los 2000 clientes no cuenten en el RSS medido. Lee el RSS con `ps -o rss= -p <pid>` cada 100 ms y guarda el pico.
- **Sockets:** una IP de `X-Forwarded-For` por cliente (`TRUST_LOOPBACK`), como `fights_no_leaks.rs`. Los topes de producción no se tocan.
- **Eventos:** el test alimenta el hub con el sink, sin runner. Así prueba también el caso de 418 eventos, que ningún dorado alcanza.

| Escenario | Qué se reporta |
|---|---|
| A. 2000 sockets suscritos a `schedule` y `fight:{id}`, en silencio (preocupación 7) | RSS base y por socket, con `read_buffer` de 128 y de 8 KiB (DP-B1). El socket 2001 da `ws.capacity` |
| B. Se reproduce el dorado `draw` a su ritmo | p50, p99 y máximo de `reloj − server_time` de cada `fight_events`, y que todos reciben los mismos bytes |
| C. Los 2000 reciben el `fight_sync` final, de unos 14 KB | RSS antes y después: lo retenido (DP-B2) |
| D. Igual, con 418 eventos de 111 bytes (47 KB) | Pico y retenido en el peor caso |
| E. 2000 sockets con `SO_RCVBUF` mínimo que no leen, con avisos seguidos | Pico de RSS, ese pico más `sockets × último mensaje sin confirmar`, tiempo hasta cerrarlos (≈ `close_grace`), y que el RSS y el hub vuelven a su nivel |
| F. 1000 sockets alternan `unsubscribe` y `subscribe` al límite, con `fight_sync` de 47 KB, y 1000 solo miran. Variantes: F1 con DP-B7; F2 solo con los 10 mensajes en 10 s de hoy; F3 los 3 mensajes a la vez en todos los sockets; F4 reconectando | Bytes/s de salida, medios y de pico, p99 de `fight_events` de los que solo miran, y cuántos de ellos se cierran por `close_grace`. **Criterio: p99 ≤ 1000 ms.** En loopback no hay límite de enlace, así que no prueba la salida del VPS: F se repite en TA-8. Si no cumple allí, vuelve a security-reviewer antes del lanzamiento público, con estas opciones: tope global con el `fight_sync` diferido, sin esperar dentro del brazo del socket, o límite en el borde |

**Criterio para la preocupación 8, sin bloquear el PR.** El umbral es de 200 MiB, cerca del 80 % de 256 MiB:
- **En A a D** se aplica al RSS.
- **En E** se aplica a `pico de RSS + sockets × último mensaje sin confirmar`. En Linux con cgroup v2, `mem_limit` también cuenta la memoria de los sockets, que `ps` no ve. Lo ya escrito sigue en el kernel, además de la capacidad que retiene `out_buffer`: en el peor caso, unos 47 KB × 2000 ≈ 90 MiB más. El PR reporta los dos valores.

Si queda por debajo, no se parte `fight_sync` ni se baja el tope. Si lo pasa, el PR no cambia nada por su cuenta: vuelve a architect con los números y estas opciones: partir `fight_sync`, no repetir los eventos en la transición a `settled`, bajar el tope o bajar `WS_MAX_CONNECTIONS`.

**Cómo leer los números.** Son de una Mac, cuyas páginas de 16 KiB en ARM pueden inflar el RSS frente a las de 4 KiB del VPS, y `ps` no mide allí los buffers del kernel. La confirmación sigue siendo la prueba de TA-8.

## Orden de trabajo
Un commit por paso, `feat(server): … (TA-13)` o similar, con sus tests en verde usando solo `cargo test -p server --test <archivo>`.

1. **Protocolo.** `Channel`, `FightId`, mensajes, tests y `just gen`. `svelte-check` en verde.
2. **`last` (hallazgo 8), en un commit `fix(server): … (TA-13)`.** Primero el test que reproduce el fallo, en `fights_lifecycle.rs`, sin ningún código nuevo: `last` tiene que ser N. Después, `ORDER BY settled_at DESC, id DESC`.
3. **Hub y tablero.** `hub.rs`, `BoardUpdate`, deduplicación, huérfanas y tests unitarios, con un caso por cada uno de los cuatro métodos de DP-B3.
4. **Runner.** Publicar antes de calcular y emitir, `HubAndSink`, `last`. Los tests de A siguen sin cambios: `fights_lifecycle`, `fights_recovery`, `fights_release`, `fights_settle`, `fights_lock`.
5. **Sesión, canales, buffers y DP-B7.** `fights_ws.rs`, con la prueba del final que no llegaba como primer caso. **Es el paso más riesgoso:** se toca el bucle de todos los sockets, y una espera mal puesta deja de responder a pings, apagado o revocación. Se corren enteros `ws`, `ws_session` y `shutdown`.
6. **Test 12 y RF41.** `fights_no_leaks.rs` y `fights_ws_parity.rs`.
7. **RF28 con socket.** `fights_logs.rs` y `fights_binary.rs`.
8. **Barreras.** **Paralelizable con 6 y 7.**
9. **`dev-fights`.** `script.rs`, config y runner, con `fights_script.rs`, `fights_script_logs.rs` y `startup.rs`. **Paralelizable con 5 a 8,** una vez hecho el 4.
10. **Scripts, `justfile` y CI, y por separado `check-compose:927`.** **Paralelizable con 5 a 9,** salvo que `e2e` necesita el feature del paso 9.
11. **Medición.** `fights_fanout.rs` y los números en el PR, con la decisión de la preocupación 8.
12. **Cierre.** Un cliente de prueba con Node contra `just dev`, que entra tarde a una pelea real, con la salida en el PR. `just e2e`, porque cambia su binario. `just check` y `just deny`.

## Riesgos
- **Un evento sale antes de su hora por un camino nuevo.** → El hub solo recibe del sink y de `events_of_settled`, descarta fuera de `live`, y no tiene tiempos. Test 12 compara cada mensaje con el reloj manual, y las barreras limitan quién llama a `release_due`.
- **El bucle del socket se bloquea.** → Ningún cerrojo cruza un `await`, todo `send` tiene plazo, y las consultas al hub son cortas. Los tests de revocación, apagado y ping se corren enteros en el paso 5, y `fights_ws.rs` suma uno con un socket atascado mientras otro sigue recibiendo.
- **Los 128 KiB de lectura, o los buffers que no se encogen, superan el límite.** → DP-B1, DP-B2 y escenarios A, C y D. Mi cálculo sale de leer el código, sin medir: si DP-B1 no se acepta, el RNF de memoria no se cumple con 2000 sockets.
- **Amplificación con suscribir y desuscribir.** Un mensaje de 40 bytes pide hasta 47 KB. No cuesta CPU por socket, gracias a la caché, pero sí perjudica a todos: la salida del VPS y Traefik son compartidos, así que ese tráfico puede retrasar los eventos de quienes solo miran.
  - Solo con el límite de hoy (10 mensajes en 10 s) serían unos 24 KB/s por socket y 47 MB/s con los 2000.
  - Con DP-B7 son unos 2,4 KB/s por socket, unos 590 KB/s por IP (50 sockets anónimos y 200 con sesión, `ip_slots.rs:13`, aunque esos 200 exigen 40 cuentas por el tope de 5 por usuario) y unos 4,7 MB/s con los 2000.
  - **Límites de esa cota:** es una media. Los 3 mensajes pueden salir juntos, así que un ataque coordinado aún manda unos 280 MB en una ráfaga por minuto, unas 3 veces lo que RF39 ya envía al liquidar (94 MB con 2000 sockets y 47 KB).
  - **Reconexión:** reconectar abre otra ventana, frenado solo por el límite de upgrades: 30 × 3 × 47 KB por minuto, unos 70 KB/s por IP, de 4,7 a unos 7,5 MB/s con 40 IP. El límite es por `/64`, así que quien rota `/64` multiplica esa cifra. **No hay un tope global.**
  - **Daño posible:** el vivo se retrasa más que el margen de 1000 ms y la escena salta. No cambia ni delata un resultado. **Dueño del riesgo residual: TA-8.** → El escenario F mide media, pico, p99 y cierres de los que miran, con y sin DP-B7.
- **El final de una pelea detenida no llega, o llega tarde.** → DP-B8: la entrada no se borra hasta difundir su resumen `settled`, con pruebas del caso de la detención y del caso de dos finales en la misma relectura. El final tarda como mucho hasta la siguiente relectura, que es inmediata si liquidó este runner y de 60 s si fue la CLI, como dice RF59.
- **`ORDER BY settled_at` sin índice.** Recorre la tabla en cada relectura (unas 52 000 filas al año, cada 60 s como mínimo), sin leer `log` salvo la fila elegida, porque es TOAST. No lo mido en B y no agrego una migración: si un día pesa, un índice parcial es solo una adición.
- **`settled_at` sale del reloj del runner o de la CLI.** Un salto de reloj puede desordenar `last`, y el desempate es por `id`. Solo cambia qué pelea se muestra como `last`, nunca un final, y las huérfanas se difunden igual.
- **Un cliente legítimo llega al límite de DP-B7.** Tres `fight_sync` pedidos por socket y minuto deberían sobrar, y una reconexión tiene ventana nueva, pero el cliente aún no existe. → Lo confirma el plan D.
- **Una suscripción silenciosa vive para siempre (preocupación 7).** → Siguen ping/pong, el tope por IP y el global. Un cliente que no responde al ping se cierra igual. Se prueba junto con RF42.
- **Mensajes de más durante una detención o con pasadas repetidas.** → Deduplicación sin `server_time`, y RF41 compara las secuencias enteras.
- **Un mensaje en caché que no corresponde a su versión.** → La caché se guarda con `version` y `released`, y un test cambia cada uno y exige un mensaje nuevo. Otro reconstruye el resultado parseando con `ServerMessage`.
- **`dev-fights` llega a una imagen.** → Las barreras de RF45 más el control en `check-release-binary`, que falla si la cadena aparece. La carga está bajo `cfg`, las barreras de fuente exigen ese `cfg` en sus dos funciones, y sin el feature el binario ni reconoce la variable, cosa que prueba un test.
- **La puerta de `dev-fights` queda tapada por la de `dev-auth`.** → Clippy y un test con la feature sola.
- **La semilla de un script acaba en un mensaje de error o en un log.** → Sin `Debug`, sin conservar el error de `serde_json`, `script.rs` sin `tracing`, y RF28 la usa como canario. Es una semilla de desarrollo, pero así el camino queda igual de cerrado que el real.
- **El binario del e2e cambia en B sin que el e2e use el script.** → Sin variable, el código de `dev-fights` no corre. `just e2e` en el paso 12 lo confirma.
- **Un test de RF27 pasa sin probar nada.** → Lo recibido tiene que ser exactamente el prefijo del log de cada hora, incluidos `ko`, `end` y el `fight_sync` final. RF28 exige `end` en el socket.
- **Tiempos en los tests.** → Reloj manual y espera por condición con límite, como en A. El tick de 50 ms es real, así que "no llegó" se comprueba con `SEVERAL_TICKS`, como en `fights_release.rs`.
- **Disco justo.** → `--release` solo una vez, en el paso 11, y una sola pasada completa de `just check`. La pasada con `dev-fights` sola compila el server y solo el test `startup`.

## Alternativas descartadas
- **`broadcast` o cola por socket.** La spec ya los descarta.
- **`Elapsed` como tipo que solo crea `Stopwatch`, en lugar de `u32`.** Cambia RF23 y `fights_settle.rs`, y no cierra nada: `stopwatch(i64::MIN)` lo fabricaría igual. La barrera de tokens frena el mismo error con menos cambios.
- **Bajar `release_due` a `pub(super)`.** Rompe el doctest y las integraciones de A sin aportar más que la barrera.
- **Sink del hub conectado solo en `serve`.** Los tests con `spawn_runner` no probarían el camino real, y un olvido dejaría el vivo vacío sin ningún error.
- **Componer el JSON de `fight_sync` a mano, para reducir su costo.** Abre la puerta a que se separe de `ServerMessage`. La caché ya evita serializar por socket.
- **Hub con `Mutex` global sin caché, serializando en cada suscripción.** 2000 reconexiones serializarían 2000 veces, contra el RNF.
- **Plantilla de TypeScript para `Channel`.** Exige `ts(type = …)`, que un test reserva a `Me.balance`, y `ClientMessage.channel` ya es una cadena libre.
- **Medir con el binario real.** No admite inyectar 418 eventos, y agregar esa entrada sería código de prueba en el binario.
- **Script por `opens_at % filas`.** Depende de la hora de arranque, así que el e2e no sabría qué fila le toca.
- **Solo ordenar `last` por `settled_at` (hallazgo 8).** No alcanza: si N se liquida y N+1 se cancela antes de la relectura, N nunca es `last` y sus suscriptores no reciben el final.
- **Borrar por orden de `id` (la primera versión de este plan).** Es justo lo que produce el fallo.
- **Lista de peleas pendientes en el runner, en lugar de `orphans`.** Se pierde con un pánico del bucle, mientras el hub sobrevive. Con el hub como única fuente no hay dos estados que puedan divergir.
- **Contar en DP-B7 también los `fight_sync` del server.** Cerraría a un cliente legítimo por las transiciones de la pelea, que no pidió.

## Pruebas (cómo se demuestra que funciona)
Reloj manual y `TestDb`, salvo lo indicado.

| RF | Nivel y archivo | Qué comprueba |
|---|---|---|
| RF37 | `crates/protocol/src/ws.rs` | `parse` y `Display` ida y vuelta; cada rechazo de RF37 y los de este plan; serde con la forma de cadena; `just gen` sin diferencias |
| RF38 | `fights_ws.rs`, `channels.rs` | Sin sesión; pelea desconocida da `ws.forbidden_channel`; el noveno canal real da `ws.too_many_subscriptions`; suscribir no consulta la base (pool cortado con `pg_proxy`); resuscribirse no reenvía; desuscribir `fight:1` no cierra `self`, y suscribir `schedule` no abre `self`; `last` tras reiniciar el runner sale de `events_of_settled`; `BadLog` no rompe nada. **DP-B7:** el cuarto `fight_sync` pedido en la ventana cierra con `ws.rate_limited`, sin abrir el canal; resuscribirse a un canal abierto y los `fight_sync` del server no gastan; `schedule` tampoco |
| RF39 | `fights_ws.rs`, `hub.rs` | Forma exacta de cada variante; orden `subscribed` y luego inicial; `schedule_update` en cada transición y detención, sin duplicados; **DP-B3, un caso por método en `hub.rs`:** `publish`, `lock_taken`, `lock_lost` (también tras un pánico, en `fights_ws.rs`) e `initial_evaluation_done`, incluido el paso de nulo a `false`; `fight_sync` en cada transición; `from_index` correcto y sin huecos |
| Final en detención (hallazgo 8, DP-B8) | `fights_lifecycle.rs`, `fights_ws.rs`, `hub.rs` | **Antes de corregir, tiene que fallar:** gancho que falla solo para N, N en detención, N+1 cancelada por `missed_window`, y al arreglar el gancho `shared.current().last` es N. Por socket, `fight:N` recibe el `fight_sync` `settled` con todos sus eventos. N y N+1 terminadas antes de la misma relectura: N se difunde aunque nunca sea `last`, y se borra pasados 60 s. Una huérfana que no está en la base, que está en otro estado o cuya fila no decodifica se borra sin difundir, y la pasada sigue (`fights.orphan_invalid`). Huérfana tras un pánico del bucle. Una huérfana con eventos parciales se completa desde el log |
| RF40 | `fights_ws.rs` | En `t − 1` no llegó y en `t` llega al siguiente tick, incluidos `ko` y `end`, medido en el socket; con la base caída siguen saliendo; con `sink` de test, el hub también recibe |
| R4 | `fights_ws.rs` | Ruta tardía: `live` con `events: []` y nada más hasta `live_started_at`, y `intro` al llegar. Reanudar con el reloj atrasado: igual |
| R7 | `fights_ws.rs`, `hub.rs` | Una tarea que muere se reconstruye sin lotes repetidos en el socket |
| RF41 | `fights_ws_parity.rs` | Dos bases, mismos specs y dos semillas con resultado o duración distintos (el test lo exige), mismo guion de reloj: mismas secuencias de mensajes y mismos cuerpos de las dos `GET` hasta `live_started_at`, salvo `id`, `seed_commit` y horas. Con `compute_gate` retenida, `schedule_update` de `betting_closed` ya llegó. Al soltarla, con la pelea aún en `betting_closed`, no sale ningún mensaje: `start_emission_if_ready` no publica antes de comprobar `state == Live` |
| RF42 | `ws.rs` | Suscrito no cierra al pasar `idle_timeout` y sigue cerrando por `ws.pong_timeout`; al desuscribirse, cierra a su plazo; sin suscripciones, como hoy; una suscripción rechazada no cuenta |
| RF43 | `fights_ws.rs`, `hub.rs` | `fight_sync` de 418 eventos de 111 bytes, sin `WriteBufferFull`; socket con `SO_RCVBUF` mínimo que no lee: el estado del hub no crece, los demás siguen a tiempo, se cierra en `close_grace` y al reconectar se pone al día; salta de lote a `fight_sync` con un cursor atrasado; dos sockets reciben los mismos bytes; `Default` de `WsConfig` válido |
| RF44 | `fights_ws.rs` | `server_time` en los tres, como cadena y con la hora del reloj; caché de menos de 250 ms (DP-B4) |
| RF45 | `fights_script.rs`, `startup.rs`, unitarios | Script con `DRAW` y `KO`: `log == canonical` de cada vector, en ciclo, también tras reiniciar. JSON, hexadecimal o spec inválidos, y archivo inexistente, dan `config.invalid` con la variable. Con `dev-fights` sola (`cfg(all(…, not(dev-auth)))`), fuera de loopback da `var = DEV_FIGHTS`, también con una ruta inexistente, lo que prueba que la puerta corre antes de leer; y un `BIND_ADDR` público da `BIND_ADDR`. Sin el feature, el binario ignora la variable, con ruta inexistente. Un script inválido que contiene la semilla como canario no la escribe en la salida. `format!("{config:?}")` no contiene `fight_script` ni la semilla |
| RF26 | `crates/protocol/src/ws.rs`, `fights_source_guard.rs` | Ninguna variante tiene semilla ni log. Por socket, tampoco tras `Settled` |
| RF27 | `fights_no_leaks.rs` | En cada estado y con anónimo, sesión y admin: `schedule`, `fight:{id}` y `self`. Antes de `live`, todos los canarios actuales y `events` vacío. En `live`, los eventos se parsean, porque ya no sirve buscar `"t":` en el texto. Tras cada paso del reloj, y al quedar el socket en silencio, lo recibido es **exactamente** el prefijo del log que corresponde a esa hora: mismos eventos, en orden, sin huecos y sin faltar ninguno, con cada `from_index` en su posición. Semilla, `canonical_json` y `final` siguen como canarios. Tras `Settled`, por socket nunca la semilla ni el log. Un observador conectado toda la pelea y uno que entra tarde. Los canarios cubren también **las cabeceras de la respuesta del upgrade y el motivo de cierre**, forzando un cierre en cada estado |
| RF28 | `fights_logs.rs` (el mismo y único test), `fights_binary.rs` | `RUST_LOG=trace` con socket conectado y **control positivo**: el socket recibió `end`. Sin semilla, log, fragmentos ni nombres de mensaje en ninguna línea. `ALLOWED_FIELDS` sin cambios. El binario real, además, prueba que `serve` conecta el hub. `fights_script_logs.rs`: lo mismo con un script, con su semilla como canario |
| RF24 (B) | `fights_source_guard.rs` | Reglas de "`release_due(u32::MAX)`", caso negativo por forma; existe el test `dev_fights` de `startup.rs`, con su `cfg`, y `justfile` lo nombra, porque un filtro sin tests pasa en verde; `hub.rs` y `script.rs` sin `tracing`; `FIGHT_SCRIPT_PATH` solo en `script.rs`; `cfg` obligatorio en `open_window_with_seed` y `fight_rank` |
| RF45, barreras | `scripts/__tests__/*` | Un caso por regla nueva, incluidos los de `check-compose:927` y `default = ["dev-fights"]` |
| Fanout | `fights_fanout.rs` | Manual, con los números en el PR |

También:
- [ ] `just gen` sin diferencias y `svelte-check` en verde.
- [ ] Con `just dev`, un cliente de prueba con Node (`WebSocket` global) ve `schedule_update`, entra tarde a una pelea real y recibe solo lo liberado. Salida en el PR.
- [ ] `just e2e` en verde con el binario de las dos features.
- [ ] `just deny`. El lock no cambia.
- [ ] `just check` en verde, con la salida pegada en el PR.
- [ ] Sin captura de pantalla: la fase B no tiene UI.

## Estimación de CI
Sin jobs nuevos. `rust` suma unos 2 a 3 min por tests, dos clippy más y la compilación de `startup` con `dev-fights` sola. `e2e-server` compila con una feature más, sin diferencia apreciable. `fights_fanout.rs` no corre en CI.

## Para el plan D
- Tras un reinicio, `fight:{id}` da `ws.forbidden_channel` hasta la primera evaluación. El cliente se suscribe a `schedule`, y al `id` que le dé el tablero.
- `fight_sync` puede traer `events: []` también en `live`, antes de `live_started_at` (R4), además del caso que ya anota plan-c.
- Un canal cuya pelea salió del hub queda en silencio (DP-B5), y el cliente lo cierra: cuenta para el tope de 8.
- `Channel` es `string` en TypeScript.
- **DP-B7:** D confirma que un cliente legítimo no llega a 3 `fight_sync` pedidos por socket y minuto, también al cambiar de pelea, al volver a primer plano y con varias pestañas. Si llega, vuelve a security-reviewer. El cierre con `ws.rate_limited` ya no es solo de clientes abusivos, así que D lo trata como cualquier reconexión con backoff.
- D escribe el script de `FIGHT_SCRIPT_PATH` del e2e y documenta el feature en el trabajo de cierre.

## Desviaciones durante la implementación
- **Reparto de commits.** Hub y runner (pasos 3 y 4) van en un commit, porque `publish` cambia de firma y uno no compila sin el otro. Las barreras (paso 8) entraron después de `dev-fights` (paso 9), porque varias nombran lo que este agrega. En el primer commit, `self` sigue siendo el único canal que se puede abrir, hasta el paso 5, para que los tests de cada commit sigan en verde.
- **`Subscriptions::subscribe` se parte en `check` y `open`.** DP-B7 tiene que cerrar el socket sin abrir el canal, y `check` no cambia nada.
- **`FightsShared` pierde `derive(Debug)`,** porque el hub no lo tiene. Su `Debug` es fijo, sin contenido.
- **`FightHub::held`.** Un método público que da los eventos y los bytes en caché de una pelea. Lo usan las pruebas de memoria y para esperar a la liberación. No da más de lo que ya es público.
- **`fights.orphan_invalid` solo se prueba como unidad (`orphan_outcome`).** Hoy no se puede llegar a él en integración: una fila `settled` decodifica siempre, y un `state` fuera de la lista también aborta `store::unsettled`, código de A que esta fase no cambia. Sin esto, la detención permanente que DP-B8 evita para una huérfana sigue siendo posible por esa otra consulta, y solo con la credencial.
- **Una fila `settled` sin final que el protocolo pueda decir (`summary()` nulo)** se trata como una que no decodifica: se borra con `fights.orphan_invalid`.
- **Prueba manual sin `just dev`.** El worktree no tiene `.env`, y `just dev` no admite una rejilla corta sin tocarlo. Corrí el binario con los valores de `.env.example`, una base propia, borrada después, y `FIGHT_CADENCE_SECS=113`, con un cliente de Node. Resultado: la pelea #1 (110 eventos, KO en 68 642 ms) coincide con el log publicado, y quien entró a los 20 024 ms recibió exactamente los 27 eventos de esa hora. Vi las cuatro transiciones por socket, y lag de 2 ms en los lotes de `ko` y `end`.
- **`fights_fanout.rs`, cómo se mide.** Un proceso por escenario, porque el asignador no devuelve lo que usó, un solo worker, por el vCPU del server, y los clientes en otra copia del proceso de test. F2 quita DP-B7 con una ventana de 1 ms.

### Medición (2026-10-31)
Mac ARM con páginas de 16 KiB, `--release` y `mimalloc`, carga media de la máquina de 4 a 9 por otras sesiones. Loopback no tiene límite de enlace, y `ps` no ve los buffers del kernel.

| Escenario | Resultado |
|---|---|
| A, `read_buffer` de 128 KiB | RSS 281 MiB, 138 KiB por socket. **Pasa de los 256 MiB:** confirma DP-B1 |
| A, 8 KiB | RSS 46 MiB, 17,5 KiB por socket. El socket 2001 da 503 `ws.capacity` |
| B, `draw` a su ritmo | 153 lotes, p50 18 ms, p99 39 ms, máximo 46 ms. Los 2000 reciben los mismos bytes, sin cierres |
| C, `fight_sync` final de unos 14 KB | RSS retenido **121 MiB**, entregado en unos 1 s |
| D, 418 eventos, unos 47 KB | RSS retenido **226 MiB. Pasa de 200 MiB** |
| E, 2000 sockets que no leen | Cerrados entre 9,1 y 9,6 s. Pico de RSS 224 MiB, más 89 MiB sin confirmar: **313 MiB. Pasa de 200 MiB y de los 256 MiB** |
| F1, DP-B7, repartido (1000 + 1000) | 2,9 MB/s de media, 4,4 de pico, p99 de quienes miran 26 ms, 0 cierres |
| F2, sin DP-B7 | 25,4 MB/s de media, 32,7 de pico, p99 31 ms, 0 cierres |
| F3, los 3 a la vez, dos ráfagas | 4,8 MB/s de media, **174,5 MB/s de pico**, p99 25 ms, 0 cierres |
| F4, reconectando (900 + 1000 + 40 IP) | 4,5 MB/s de media, 10,1 de pico, p99 25 ms, 0 cierres |

- **Preocupación 8: el criterio se incumple en D y E, y por lo tanto vuelve a architect, sin cambios por mi cuenta.** Opciones de "Medición": partir `fight_sync`, no repetir los eventos en la transición a `settled`, bajar el tope o bajar `WS_MAX_CONNECTIONS`.
- **El piso de DP-B2 era optimista.** Estimé 110 MiB para el peor caso, y es el doble: D retiene unos 92 KiB por socket sobre lo de A, para mensajes de 47 KB. C retiene unos 38 KiB para mensajes de 14 KB. No investigué cuánto de esa diferencia es de las páginas de 16 KiB y de `mimalloc`, y cuánto de `out_buffer`, así que una medición en Linux puede dar menos.
- **Escenario F: cumple p99 ≤ 1000 ms en las cuatro variantes,** con la salvedad de que en loopback no hay enlace que saturar. Se repite en TA-8.

## Revisión
### architect — 2026-10-31 — REQUEST_CHANGES
- Verifiqué, sin compilar: el plan contra RF37 a RF45, los RNF de memoria y carga y la preocupación 8. En `tungstenite` 0.29: `read_in` escribe ceros hasta la capacidad de `in_buffer`, por defecto 128 KiB, y `assert_valid` exige máximo > escritura. En `axum` 0.8.9 existen los tres métodos de buffer y `Utf8Bytes`. En el repo: `session.rs` (el arreglo de `self` y `last_data`), `channels.rs`, `handshake.rs`, que hoy no fija ningún buffer, `publish` al final de la pasada, `fights_settle.rs:319`, `check-compose.mjs:927`, el `DEV_AUTH` de `config.rs` y las pasadas de test de `justfile`.
- **DP-B1, DP-B2, DP-B4 y DP-B5: aceptados.** Sin cambios.
- **DP-B3 y DP-B6: aceptados por architect, pendientes de security-reviewer.** DP-B3 con el punto 3 de abajo.
- **Umbral de la preocupación 8:** vale 200 MiB, con el punto 2 de abajo: `mem_limit` también cuenta los buffers de socket del kernel, y una Mac no los mide.
- **Enmienda:** va en `spec.md`, como "Enmienda 3 (fase B, 2026-10-31)", antes de `## Revisión`, con su marca en cada RF: RF38, RF39, RF43, RF44, RF45, el RNF de memoria y la preocupación 8. La escribe architect cuando security-reviewer resuelva DP-B3 y DP-B6, y entra en la rama de esta fase antes de tocar código. Ningún ADR cambia. La enmienda de RF-B13 de 001 sigue en D.
- Cambios pedidos:
  1. **Amplificación (riesgo 4).** "Solo llena el ancho de banda de quien lo pide" no es cierto: los 47 MB/s salen del único enlace del VPS y pasan por Traefik, así que afectan a todos los espectadores. El escenario D tampoco da ese número, porque manda una sola vez. Corregir la frase y sumar un escenario F a `fights_fanout.rs`: N sockets alternando `unsubscribe` y `subscribe` al límite del `RateWindow`, con bytes/s y el p99 de los sockets que solo miran. La regla, si hace falta, depende de security-reviewer.
  2. **Criterio de la preocupación 8.** En Linux con cgroup v2, `mem_limit` también cuenta la memoria de los sockets, y `ps` no la ve. En el escenario E, lo que el server ya escribió queda en el kernel, además de la capacidad que `out_buffer` retiene: en el peor caso, unos 47 KB × 2000 ≈ 90 MiB más. El umbral se aplica a `pico de RSS + sockets × último mensaje sin confirmar` en E, y al RSS en A a D, y el PR reporta los dos valores. Decir también que las páginas de 16 KiB de una Mac ARM pueden inflar el RSS, y que la confirmación sigue siendo TA-8.
  3. **Aviso de `halted` (DP-B3).** `halted_state` cambia en `lock_taken`, `lock_lost` e `initial_evaluation_done`, no solo en `publish`, y `runner::run` llama a `lock_lost` también tras un pánico. La fila de `board.rs` y "Publicar antes…" tienen que decir que la comparación y el aviso corren en esos cuatro métodos, con un caso por método en `hub.rs`. Si no, el paso de nulo a `false` al terminar la evaluación no llega a nadie.
  4. **Barreras.** Faltan los otros huecos del nit de security-reviewer en el PR #69, que además señaló que esta fase toca esa zona, y que hoy `fights_source_guard.rs` no cubre: `fight_secrets` sin distinguir mayúsculas, `include!` y `#[path` en `src`, e `into_final` solo en `sealed.rs` y `release.rs`. Sumarlos a "`release_due(u32::MAX)`", con un caso negativo por forma.
  5. **Texto.** `fight_id` de `fight_events` es `u32` en `ServerMessage`, igual que `FightSummary.id`, ya que `FightId` no se exporta a TypeScript. `depends_on` suma `"010"`. Cambiar "La spec no se edita desde este plan" por una referencia a la Enmienda 3.
- Al corregir, se vuelve a pedir revisión de architect, solo sobre estos cinco puntos y lo que cambie security-reviewer.

### backend-engineer — 2026-10-31 — correcciones de la ronda 1
- Apliqué los cinco puntos de architect y los pedidos de security-reviewer que me comunicó quien coordina, sin compilar. Security-reviewer no dejó aquí su revisión.
- **Bug del final:** confirmado leyendo `store.rs:169`, `runner.rs` (`reload`, `on_task`) y la tabla de RF15: una `scheduled` se cancela también con detención, así que N+1 puede liquidarse antes que N. Además de lo pedido, encontré que ordenar por `settled_at` no basta si las dos terminan antes de una misma relectura: por eso `orphans` y la retención de 10 s (DP-B8, nueva, que architect debe aceptar para la Enmienda 3).
- **Barreras:** `#[path` tiene una excepción real, `profile/name.rs:175`. La dejo como línea exacta permitida, en lugar de tocar ese archivo.
- **`cfg`:** los `cfg` de `dev-auth` que significan "exige loopback" se amplían, porque con `dev-fights` sola serían falsos.
- **DP-B7:** la cota de 4,7 MB/s es una media. El riesgo deja escrito que puede salir en ráfagas y que reconectar abre otra ventana.
- Pido de nuevo revisión de architect sobre sus cinco puntos, lo de security-reviewer y DP-B8.

### architect — 2026-10-31 — APPROVE (segunda ronda)
- Verifiqué, sin compilar: los cinco puntos de la ronda 1, aplicados. DP-B7 y lo que llegó de security-reviewer: `start_emission_if_ready` publica tras comprobar `state == Live`, la puerta de `dev-fights` corre antes de leer el archivo, y RF27 cubre el prefijo exacto, las cabeceras del upgrade y el motivo de cierre. Hallazgo 8 confirmado en `store.rs:169` y en RF15: una `scheduled` se cancela también con detención.
- **DP-B7:** aceptada. La cota es una media, y el riesgo ya dice que puede salir en ráfagas.
- **DP-B8:** aceptada. El hub como única fuente es lo correcto, porque sobrevive a un pánico del bucle, y también cubre a otro runner y a la CLI.
- **`ORDER BY settled_at` sin índice:** aceptado, con un disparador. Es un cambio frente a A, donde la consulta usaba el índice de `opens_at`. Con filas de unos 2 KB, según mi cálculo, es un recorrido de unos 100 MiB por relectura al cabo de un año. El costo es bajo hasta entonces, pero la frase "si un día pesa" no obliga a mirarlo: se revisa en TA-8 y como tarde al llegar a 50 000 filas.
- **Que el `fix` de `last` vaya como paso 2:** de acuerdo, con su test primero.
- **Enmienda 3** escrita en `spec.md`, con DP-B1 a DP-B8.
- Notas para implementar, que no cambian el plan:
  - `orphans`: si `store::by_id` da `None`, la entrada se borra. Si no, tras un `TRUNCATE` manual o con una base reemplazada, esa huérfana se consultaría en cada relectura para siempre.
  - La retención es una sola regla: no es `last` y pasaron 10 s desde que llegó. Un `last` anterior liquidado hace 1 s también espera, aunque la frase "se va en cuanto cambia `last`" diga otra cosa.
  - La línea "Desviaciones y vacíos" dice que DP-B8 está pendiente de architect: actualizarla al commitear.
- Security-reviewer no dejó su revisión en este plan. Su resultado sobre DP-B3, DP-B6 y DP-B7 lo comunica quien coordina, y esta aprobación es solo la de architect.

### security-reviewer — 2026-10-31 — APPROVE, con tres ajustes
- Verifiqué, sin compilar y sin medir: este plan contra `session.rs`, `rate.rs`, `channels.rs`, `ws/mod.rs`, `quotas.rs`, `ip_slots.rs`, `runner.rs`, `store.rs` y `fights_source_guard.rs`. Sin hallazgos Important: el plan no abre ningún camino a la semilla ni a eventos futuros, y un fallo de estos puntos no toca el resultado ni las monedas. Aplicar los ajustes no necesita otra ronda mía.
- **Mis pedidos de la ronda anterior:** aplicados. DP-B3 en los cuatro métodos; puerta de `dev-fights` antes de leer y probada con la feature sola; barreras nuevas, también `FIGHT_SECRETS`; prefijo exacto, cabeceras del upgrade y motivo de cierre en RF27; canario de la semilla del script en RF28; y `publish` tras comprobar `state == Live`.
- **DP-B3 y DP-B6:** aprobados.
- **DP-B7:** aprobado como cota por socket. No es una defensa contra un ataque coordinado, y ninguna regla por socket puede serlo:
  - La ráfaga de 280 MB es solo unas 3 veces lo que RF39 ya envía sin ataque al liquidar (94 MB con 2000 sockets y 47 KB; 28 MB con 14 KB).
  - Reconectar suma unos 70 KB/s por IP (30 upgrades × 3 × 47 KB por minuto): de 4,7 a unos 7,5 MB/s con 40 IP. El límite es por `/64`, así que quien rota `/64` multiplica esa cifra: **no hay un tope global**.
  - Daño posible: el vivo se retrasa más que el margen de 1000 ms de latencia y la escena salta. Los eventos salen del hub a su hora y las apuestas ya cerraron, así que no cambia ni delata un resultado.
- **Ajuste 1, escenario F (línea 230):** sumar una variante con los 3 mensajes a la vez en todos los sockets y otra que reconecta, y reportar también cuántos sockets que solo miran se cierran por `close_grace`. Criterio: p99 de `fight_events` ≤ 1000 ms. En loopback no hay límite de enlace, así que este número no prueba la salida del VPS: F se repite en TA-8. Si no cumple allí, vuelve a security-reviewer antes del lanzamiento público, con estas opciones: tope global con el `fight_sync` diferido, sin esperar dentro del brazo del socket, o límite en el borde.
- **Ajuste 2, riesgo (línea 263):** escribir las cifras de reconexión, que el límite es por `/64`, que no hay tope global y que el dueño de ese riesgo residual es TA-8.
- **DP-B8:** aprobado para seguridad. Solo difunde datos ya públicos: la huérfana se lee tras el `COMMIT`, y completar desde el log solo ocurre en una fila `settled`. Las reglas de borrado y `settled_at` no cambian ningún final.
- **Ajuste 3, DP-B8 (líneas 151 y 152):**
  - Subir `SETTLED_RETENTION_MS` a 60 s. El motivo escrito no es cierto: el socket envía canal por canal, hasta 8 con `close_grace` cada uno, unos 40 s sin contar los otros brazos. Con 10 s, un socket lento pero vivo puede perder el final. Una entrada pesa como mucho unos 57 KB y estas filas son raras, así que 60 s no cuesta nada, y no depende de un plazo justo si el reloj de pared salta.
  - El hub solo acepta como final un resumen `settled`. Si `store::by_id` devuelve otro estado, por ejemplo tras restaurar un backup, la entrada se borra sin difundir, igual que con `None`.
  - Un error al decodificar esa fila (`Protocol`, no de la base) borra la entrada con `fights.orphan_invalid` en `warn`, sin abortar la pasada. Una fila corrupta no debe detener el vivo: solo es alcanzable con la credencial, pero hoy se convertiría en una detención permanente por una pelea ya terminada.
- **Nit:** `cargo test … --test startup dev_fights` pasa en verde si el filtro no encuentra ningún test. Que `fights_source_guard.rs` exija que ese test exista, con su `cfg`, y que `justfile` lo nombre.
- **Límites:** esta aprobación no acepta el riesgo residual en nombre del owner. Sigue pendiente lo que pedí para TA-27 en el PR #69 sobre la preocupación 13.

### backend-engineer — 2026-10-31 — ajustes antes de implementar
- Incorporo los tres ajustes de security-reviewer y las notas de architect: variantes y criterio del escenario F, cifras de reconexión y dueño del riesgo, retención de 60 s y las tres reglas de borrado de una huérfana, y el nit del filtro `dev_fights`. También corrijo los 60 s en la Enmienda 3 de la spec.
