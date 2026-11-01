---
id: "010"
title: "Scheduler de peleas en vivo, fase A: ciclo, integridad, recuperación y API de lectura"
type: plan
status: done
risk: high
author: backend-engineer
reviewers: [architect]
approvals:
  - by: architect
    date: 2026-10-31
linear: TA-13
depends_on: ["010"]
created: 2026-10-31
updated: 2026-11-01
---

# Plan: Scheduler de peleas, fase A (desde spec.md 2026-10-31)

> [intent](./intent.md) · [spec](./spec.md) · ADRs: [0002](../../decisions/0002-commit-reveal-semilla.md), [0006](../../decisions/0006-despliegue-y-ci.md) D10, [0008](../../decisions/0008-autenticacion.md) D8, [0012](../../decisions/0012-logs-y-cabeceras-de-seguridad.md) A4, A8 y A9

## Alcance
**Entra:** RF1 a RF25, RF29 a RF36, RF40 (la tarea y su prueba, con un sink que descarta; DP-7), la base de RF59 (enum, columna y reglas, sin ningún código que la escriba), RNF de imagen, y la parte de A de RF26, RF27 y RF28.

| RF parcial | En A | Queda para B |
|---|---|---|
| RF23 | Los tres tipos sellados y `events_of_settled`, con su test | Su uso desde el hub |
| RF26 | Test de forma: `FightSummary` y `FightsCurrent` no tienen semilla ni log; ningún tipo de `protocol` contiene `FightLog` salvo el propio log | `ServerMessage` con sus variantes nuevas |
| RF27 | Todas las entradas de `ROUTES`, en cada estado y con las tres identidades, y el canal `self` de `/ws` | Canales públicos y eventos por socket |
| RF28 | Corrida completa en `trace`, sin sockets | La misma corrida con un socket conectado |

**No entra:** `Channel`, `ServerMessage`, hub (el sink real de RF40), `fight_sync`, catch-up, RF42, RF43, `dev-fights` (RF45), renderer, vista, escritura de `voided`, apuestas, y el trabajo de cierre de la spec (`CLAUDE.md`, `.env.example`, README, arquitectura, glosario), que es de la fase D.

**Se puede probar al terminar:** con `just dev` hay peleas en la base, de principio a fin y sin WS, y cada una se verifica con `GET /api/fights/{id}`.

## Desviaciones de la spec
Architect aceptó estos puntos y están en la ["Enmienda (fase A, 2026-10-31)"](./spec.md#enmienda-fase-a-2026-10-31) de la spec, con el mismo número. Security-reviewer ya revisó DP-3, DP-4, DP-5, DP-6, DP-7, DP-10 y M6: su resultado, y las recomendaciones R1 a R8, están aplicados aquí y en esa enmienda.

### Problemas que impedían cumplir la spec original
- **DP-1. El mínimo de 60 s cancela toda pelea con una ventana corta.** La tabla de RF15 cancela una `scheduled` si queda menos de 60 s de ventana, sin mirar `FIGHT_BETTING_WINDOW_SECS`. Con la rejilla mínima que RF11 admite y que usa la preocupación 9 (`ventana = 5`), al llegar `opens_at` quedan 5 s y sale `cancelled(missed_window)`, siempre. El e2e, la prueba "Sola" y cualquier valor menor de 60 no podrían abrir ninguna pelea. **Resolución:** el mínimo es `min(60 s, ventana / 2)`, calculado en ms: con `ventana = 1 s` es 500 ms, no 0. RF11 exige además `ventana > 0`, y `config.rs` rechaza `0` con `config.invalid`. Con los valores por defecto sigue en 60 s.
- **DP-2. A la tabla de RF15 le falta la fila de espera antes de `live_at`.** La fila "`betting_closed`, `now ≤ live_at + 1 s`, sin otra en `live`" no tiene cota inferior: leída al pie de la letra, la pelea pasa a `live` justo tras el cierre y se salta `pre_live`. **Resolución:** agregar `betting_closed | now < live_at | Espera`, después de la fila que recalcula el log y de la de detención.
- **DP-3. `FightSummary` no se puede construir con una versión ausente.** `time_limit_ms` y `max_duration_ms` son obligatorios y solo salen de `Ruleset::for_version(v)`. Con la versión ausente, un caso que RF15 y RF16 prevén, esa función da `None`, y RF34 y RF35 tienen que seguir sirviendo esa fila. **Resolución:** dos columnas derivadas, `time_limit_ms` y `max_duration_ms` (`int NOT NULL`), escritas en RF1 e inmutables por RF4, como `max_hp_*`. RF5 y RF7 las comparan con el ruleset. Una diferencia se trata como `max_hp_*`, y entran en el paso 4 de RF59.
- **DP-4. RF10 y RF40 se contradicen con la base caída.** La conexión del bloqueo es de la misma base. Si cae, RF10 dice "deja de actuar" y "no difunde", y RF40 dice "con la base caída, siguen saliendo". **Resolución:** sin bloqueo no hay transiciones, liquidaciones, filas nuevas ni emisiones nuevas, y las emisiones ya en curso siguen hasta su `end`, sin liquidar. Sus eventos son deterministas y salen a su hora, así que no exponen nada nuevo. Al recuperar el bloqueo, el runner relee y reconcilia. A lo prueba con un sink de prueba (`fights_release.rs` y `fights_lock.rs`).
- **DP-5. RF24 falla con el código que ya existe.** `outcome` es hoy una variable local en `ws/session.rs`, `admin.rs` y `db.rs`, así que el test por tokens fallaría sin que haya ninguna fuga. **Resolución:** para `outcome` solo se prohíben las formas calificadas, `.outcome` y `::outcome`, porque la defensa real es que no se obtiene un `FightLog` sin un token prohibido. La búsqueda es por tokens, no por subcadena, así que un espacio o un salto de línea en medio no la evitan. El resto de tokens de RF24 no cambia. Los casos negativos suman `log.outcome()`, `log\n    .outcome()`, `FightLog::outcome` y `<_>::outcome(&log)`, y el caso positivo es `let outcome = 1;`. Todos los tokens se normalizan quitando `r#` antes de buscar (R1), con los casos negativos `log.r#outcome()` y `r#simulate(…)`. El quitador de comentarios no se desincroniza con `"http://"`, `'"'`, `r#"…"#` ni comentarios anidados, con un caso negativo por cada uno.
- **DP-6. Las reglas de RF8 y RF9 se pueden saltar con un `INSERT`.** RF4 y RF8 solo hablan de transiciones, y un `INSERT` directo de una fila `settled` no pasaría por ninguna. **Resolución:** el guard corre en `BEFORE INSERT OR UPDATE`, un `INSERT` exige `state = 'scheduled'` y todas las columnas de ciclo nulas, y un `UPDATE` sin transición se rechaza. También, `fight_secrets` rechaza un `INSERT` si su pelea ya está `settled`. La restauración de la preocupación 13 sigue siendo posible, porque esa pelea no está liquidada. Además, "ya `settled` en esa transacción" no se puede comprobar de forma fiable desde un trigger, porque con un savepoint el `xmin` de la fila no coincide con el de la transacción. El trigger de `DELETE` comprueba `state = 'settled'` según lo que ve la transacción, como dice RF25 enmendado. Otra transacción no ve ese estado hasta el `COMMIT`. Si un liquidador olvidara borrar la fila, el secreto seguiría ahí tras el `COMMIT`, y un `DELETE` posterior solo quitaría un valor ya público en `revealed_seed`. Por eso un test de RF18 comprueba que, tras liquidar, no queda la fila.
- **DP-7. La fase A necesita liberar eventos.** La tabla de fases pone "liberación por tiempo" en B, pero RF18 (fase A) solo permite liquidar tras liberar `end`, y `SealedLog` no da ningún tiempo futuro. Sin liberación no hay `Settled`. **Resolución:** A implementa y prueba la tarea de RF40 (tick fijo de 50 ms, reloj de RF40) con un `EventSink` que por defecto descarta los eventos. B solo conecta el hub como sink.
- **DP-8. La firma de `lock_open_for_bet` no alcanza.** `lock_open_for_bet(tx, id, now)` tiene que conocer la detención, el bloqueo y la evaluación inicial, que viven en memoria, y la firma no recibe ese estado. **Resolución:** `FightsShared::lock_open_for_bet(&self, tx, id, now)`. La semántica no cambia.

### Vacíos de la spec original
- **DP-9. La fase A toca `apps/web`.** `ERROR_CODES` (`features/auth/codes.ts`) y `ERROR_TEXT` (`features/auth/error-text.ts`) son tipos mapeados sobre `ErrorCode`, así que un código nuevo sin su entrada rompe `svelte-check` y `just check`. RF36 obliga a tocar esos dos archivos y `apps/web/messages/{en,es}.json`, aunque la tabla de fases no lo diga. Es el mínimo: dos entradas y dos mensajes, sin UI.
- **DP-10. RF34 no define el fallo de la base.** **Resolución:** 503 `fight.unavailable` con `no-store`, solo si falla la base, en `FightErrorCode` como pide RF36 y no `auth.try_again`, porque la ruta no tiene sesión. `spec_a` y `spec_b` pasan a `Option<String>`. Si los bytes de `spec_a`, `spec_b` o `log` no son UTF-8, ese campo va `null` y el resto sale igual, incluida `seed`: un 503 ocultaría la semilla de una `voided` por `snapshot_invalid` (ADR-0002 punto 4, RF7, RF59). Nunca se usa `from_utf8_lossy`, que rompería "byte a byte".
- **DP-11. `just prod-local` no admite una cadencia corta.** `check-compose` tiene una lista cerrada de variables para `server`, y `compose.local.yml` solo puede agregar `ports`. **Resolución:** no abrir esa lista. La prueba manual corre con los valores por defecto (hasta unos 20 min por pelea) y el resultado se pega en el PR.

### Menores (sin cambiar el comportamiento)
- **M1.** `final` es palabra reservada en Rust: `SettlingFight` usa `final_result`, igual que `FightSummary`. `futures-util` es solo de desarrollo: el gancho devuelve un alias propio `Pin<Box<dyn Future + Send + 'a>>`, como `db::PingFuture`.
- **M2.** Un `CHECK` no puede lanzar `TA006`. El guard corre antes y da `TA006` en todas las reglas; los `CHECK` quedan como segunda barrera (`23514`), que el test comprueba con el trigger deshabilitado.
- **M3.** `ci.yml` no tiene un filtro `e2e-server`: el job usa el filtro `e2e`, que es el que se ajusta.
- **M4.** `server --help` no existe (da `Invalid`). La parte de RF16 se prueba con `cli::parse` y con el texto de `admin --help`.
- **M5.** "`cancel` solo se llama desde `next_action`" se prueba así: `Action::Cancel` solo se construye en `state.rs`, y una cancelación solo la escribe un intento del runner, con `sealed::cancel_fight`, el único que llama a `store::cancel`.

### Cambia el comportamiento
- **M6.** RF5 suma `spec.validate(ruleset)` a la construcción del snapshot: un spec que `simulate` rechazaría da `fight_unrunnable` en `scheduled`, antes del commit, y parada segura después. `SnapshotError` lleva una variante propia, `SpecInvalid`, para el log. En RF59 se trata como el paso 4: se niega, porque `validate` es pura y pasó antes del commit, y un fallo posterior con los bytes intactos viene del binario. `engine_error` queda para `EventLimit`, `encode_failed` o un pánico de `compute`.

## Archivos que cambian
| Archivo | Cambio |
|---|---|
| `crates/protocol/src/fights.rs` | Nuevo: tipos de "Tipos" de la spec (`FightDetail` con `spec_a` y `spec_b` en `Option<String>`, DP-10), con `ts(export)`, tests de forma exacta y `TimestampMs` (`#[serde(transparent)]`, `from_ms`, `as_ms`) |
| `crates/protocol/src/error.rs`, `lib.rs` | `FightErrorCode` (`fight.not_found`, `fight.unavailable`), `ErrorCode::Fight`, `From`, `ALL`, `PREFIX` y tests exhaustivos |
| `packages/protocol-ts/src/**` | Solo con `just gen` |
| `apps/web/src/lib/features/auth/{codes,error-text}.ts`, `apps/web/messages/{en,es}.json` | DP-9 |
| `Cargo.toml`, `Cargo.lock`, `apps/server/Cargo.toml` | `engine` y `content` normales; `engine-wasm` en `[workspace.dependencies]` y como dev-dependency. Ninguna versión nueva |
| `apps/server/migrations/20261031000000_fights.sql` | Nuevo: tablas, restricciones y triggers |
| `apps/server/src/fights/mod.rs` | Nuevo: constantes (`RULESET_VERSION`, 3500, 5000, 10 s), `FightSettings`, `FightsShared` y hex (el arranque está en `runner::run`) |
| `apps/server/src/fights/clock.rs` | Nuevo: `FightClock` (`Real`, `Manual`) con pared, monotónico y espera despertable |
| `apps/server/src/fights/snapshot.rs` | Nuevo: `snapshot_sha256`, `FightSnapshot::from_row` y `max_hp` |
| `apps/server/src/fights/sealed.rs` | Nuevo: `SealedSeed`, `ClosedWindow`, `SealedLog`, `FinalLog`, `events_of_settled`; único SQL de `fight_secrets`; doctests. Sin `tracing` |
| `apps/server/src/fights/state.rs` | Nuevo: `FightRow`, `Action`, `Context` y `next_action` (pura), `target_slot` |
| `apps/server/src/fights/matchup.rs` | Nuevo: `pick_matchup` (pura) y `roster_sha256` |
| `apps/server/src/fights/roster.rs` | Nuevo: único llamador de `load_roster`, con `spawn_blocking` |
| `apps/server/src/fights/store.rs` | Nuevo: único SQL de `fights` |
| `apps/server/src/fights/hook.rs` | Nuevo: `SettlementHook`, `SettlingFight`, `SettlementError`, `NoBets` |
| `apps/server/src/fights/bets_contract.rs` | Nuevo: `lock_open_for_bet`, `OpenWindow`, `WindowClosed`, `unsettled_in` |
| `apps/server/src/fights/release.rs` | Nuevo: tarea de liberación y `EventSink` (DP-7) |
| `apps/server/src/fights/runner.rs` | Nuevo: bloqueo, bucle, recuperación, detención y liquidación |
| `apps/server/src/fights/board.rs` | Nuevo: tablero en memoria de RF35. B lo reutiliza |
| `apps/server/src/fights/api.rs` | Nuevo: los dos handlers |
| `apps/server/src/lib.rs`, `app.rs` | `pub mod fights`; `AppState.fights`; `Settings.fights`; arranque solo desde `serve` |
| `apps/server/src/config.rs` | `FIGHT_CADENCE_SECS`, `FIGHT_BETTING_WINDOW_SECS`, `FIGHT_PRE_LIVE_SECS`, `CONTENT_DIR` y la validación de RF11 |
| `apps/server/src/db.rs` | `TA006` → `fight_guard`; `55P03` → `lock_timeout` |
| `apps/server/src/http/routes.rs`, `rate/{mod,quotas,http}.rs` | Dos `Handler` y dos entradas en `ROUTES`; `FIGHT_READ_IP`; el limitador en `RateLimits` y en `sweep` |
| `apps/server/tests/routes.rs`, `support/{bin,mod}.rs`, `support/fights.rs` | Muestra `{id}`; `CONTENT_DIR` absoluto en `DEFAULT_ENV`; fixtures de peleas (nuevo) |
| `apps/server/tests/support/pg_proxy.rs` | Modo nuevo que retiene los bytes sin cerrar la conexión: hoy `cut` solo cierra, y RF10 tiene que probar una conexión colgada |
| `apps/web/playwright.config.ts` | `CONTENT_DIR` absoluto en el `env` del server: `serve-with-log.mjs` lo arranca con `cwd` en `apps/web`, y el valor por defecto daría `fights.roster_invalid` en cada e2e |
| `apps/server/tests/golden/fight-snapshot.json` | Nuevo: vector de RF2 |
| `apps/server/tests/fights_*.rs`, `ruleset_freeze.rs` | Nuevos: ver "Pruebas" |
| `apps/server/Dockerfile` | Etapa final: `COPY content/characters /content/characters` y `ENV CONTENT_DIR=/content/characters` |
| `scripts/check-compose.mjs`, `scripts/__tests__/check-compose.test.mjs`, `scripts/__fixtures__/check-compose/valid/**` | Regla nueva de esas dos líneas, y final sin `RUN` |
| `.github/workflows/ci.yml` | Filtro `images`: `content/characters/**`. Filtro `e2e`: `crates/content/**`, `crates/engine/**` y el comentario que dice que el server solo depende de `protocol` |

`crates/engine`, `crates/content`, `crates/sim`, el ledger, `ws/*`, `telemetry.rs` (`ALLOWED_FIELDS`), `infra/*`, `.dockerignore` y `check-release-binary.mjs` no cambian. `apps/web` solo cambia en lo que indican DP-9 y `playwright.config.ts`. `snapshot.rs`, `release.rs`, `board.rs` y `clock.rs` separan responsabilidades y no cambian ningún contrato.

## Diseño de la implementación

### Migración
- Tablas de "Datos" de la spec, más DP-3. Horas en `timestamptz`. Restricciones con nombre fijo (`fights_*_ck`, `fights_opens_at_key`): `octet_length = 32` en `snapshot_sha256`, `roster_sha256`, `seed_commit` y `revealed_seed`; listas cerradas de `state`, `winner`, `end_reason`, `cancel_reason` y `void_reason`; `closes_at > opens_at`; `live_at >= closes_at`; `ruleset_version > 0`; `max_hp_*` de 1 a 65 535; `time_limit_ms > 0` y `max_duration_ms >= time_limit_ms` (DP-3); `CHECK` de RF9 y RF25.
- `fights_guard()`, `BEFORE INSERT OR UPDATE`, con `ERRCODE = 'TA006'` y `MESSAGE = 'fight guard'`, sin valores. En un `UPDATE`, en este orden:
  1. `OLD.state = 'settled'` → rechaza.
  2. Una columna inmutable de RF4 (más DP-3) cambia → rechaza.
  3. Una columna de una sola escritura ya tiene valor y cambia → rechaza.
  4. `NEW.state = OLD.state` → rechaza: todo `UPDATE` legítimo es una transición, y así `seed_commit` no se puede escribir fuera de su arista.
  5. La arista no está en el grafo de RF8 → rechaza.
  6. Por arista: solo se escriben sus columnas, y esas columnas no pueden quedar nulas: `seed_commit` y `opened_at` hacia `betting_open`; `closed_at` hacia `betting_closed`; `live_started_at` hacia `live`; `settled_at` y el final hacia `settled`.
  7. Hacia `settled`: exactamente uno de `end_reason`, `cancel_reason` o `void_reason`; `winner` solo con `end_reason`; y las reglas de RF9, incluida `sha256(revealed_seed) IS NOT DISTINCT FROM seed_commit`.
- `fights_no_delete` (fila) y `fights_no_truncate` (sentencia). `fight_secrets_guard` para `INSERT`, `UPDATE` y `DELETE` (DP-6), y `fight_secrets_no_truncate`. Un `INSERT` exige además `seed_commit IS NULL OR sha256(NEW.seed) = seed_commit`, y la arista hacia `betting_open` exige que exista el secreto y coincida con el commit (R2). Por eso `sealed::open_window` inserta el secreto antes del `UPDATE`.
- Índice parcial `fights_unsettled_idx ON fights (opens_at) WHERE state <> 'settled'`.

### SQL de horas
Sin librería de fechas ni funciones nuevas: lectura `(extract(epoch FROM col) * 1000)::bigint`, y escritura `timestamptz 'epoch' + $1::bigint * interval '1 millisecond'`. `to_timestamp(double)` no se usa, porque puede perder exactitud. Toda hora escrita por el runner viene de `FightClock`, nunca de `now()`. Consultas con `sqlx::query` y `bind`, sin el envoltorio de SQL formateado que prohíbe `db::sql_guard`.

### Tipos sellados y límites
- `FightSnapshot::from_row(&FightRow) -> Result<FightSnapshot, SnapshotError>`: versión, decodificación (`deny_unknown_fields` ya está en `CharacterSpec`), reserialización idéntica, hash de RF2, `slug_*`, `max_hp_*`, cotas (DP-3) y `validate` (M6, con su variante `SpecInvalid`). `SnapshotError::code()` es de lista cerrada.
- `SealedSeed::load(conn, id, commit)` compara `sha256` y da `Missing` o `Mismatch`, sin bytes. Campo privado y sin traits.
- `SealedLog::compute(seed, &snapshot, &ClosedWindow) -> Result<SealedLog, (SealedSeed, ComputeError)>`. `ClosedWindow` tiene campo privado, y solo lo crean `store::close` tras su `COMMIT` y `store::read_closed_window(pool, id)`, que lee la fila de la base y solo lo da si está en `betting_closed` o `live`.
- El nombre `canonical_json` está prohibido fuera de `sealed.rs`, así que `FinalLog` no expone nada con ese nombre ni sus bytes fuera del módulo. La escritura va en `sealed::settle_decided(conn, &FinalLog, at)`, que pasa `&[u8]` a `store::write_decided` y luego borra el secreto. Así, `store` sigue siendo el único SQL de `fights`, y `sealed` el único de `fight_secrets`.
- `sealed::open_window(tx, id, at)`: `store::lock_scheduled` (`FOR UPDATE`; sin fila, nada), semilla de `OsRng`, `INSERT` del secreto y `store::mark_open`. El orden evita que dos runners choquen en la clave primaria. Si `OsRng` falla, reintenta con `error_kind = os_rng`.

### Decisión pura
`next_action(&FightRow, now_ms, &Context) -> Action`, con `Action = Wait(until_ms) | Idle | Open | Cancel(reason) | Close | Recompute | GoLive { started_at_ms }`. `Context = { other_live, halted, log_ready }`. Sigue la tabla de RF15 con DP-1 y DP-2. La validez del snapshot en `scheduled` se comprueba dentro, desde los bytes de la fila, y sigue siendo pura. La recuperación y la parada segura no son un `Action`: las decide el runner según el resultado de `Recompute`.

`target_slot(now, cadencia) = (floor(now / cadencia) + 1) · cadencia` unifica "anuncio" y "recién arrancado" (RF12).

### Runner
Una tarea supervisora y un bucle, con el reloj inyectado:

1. **Roster**, una vez, antes de pedir el bloqueo (RF22). Un fallo solo impide crear filas.
2. **Bloqueo:** una sola conexión retenida todo el tiempo que dure el bloqueo, con `PgConnection::connect_with` como en `admin.rs` y `application_name = torneo-fight-runner`; nunca un pool, porque un `acquire` tras una caída abriría otra conexión y el `SELECT 1` pasaría sin tener el bloqueo. `pg_try_advisory_lock` con una clave fija, reintento cada 5 s. Mientras se tiene, una tarea propia hace `ping` cada 5 s sobre esa misma conexión, con un tiempo máximo de 3 s: cualquier fallo o vencimiento cuenta como bloqueo perdido, se registra `fights.lock_lost`, se descarta la conexión y se vuelve a pedir. La conexión lleva `idle_session_timeout` (30 s), para que Postgres cierre la sesión de un cliente colgado aunque el TCP siga vivo, y `tcp_keepalives_idle`, `tcp_keepalives_interval`, `tcp_keepalives_count` y `tcp_user_timeout` para una partición de red (R5).
3. **Pasada:** cada plazo vencido, y al menos cada 60 s, relee las no liquidadas.
   - Reconcilia: una fila que ya está `settled` aborta su emisión.
   - Evalúa de la más antigua a la más nueva, **primero las transiciones y después la creación de la fila siguiente**.
   - Reconstruye las causas de detención y el tablero. Al terminar la primera pasada, marca `initial_done`.
4. **Espera:** `min(plazos, settle_at, reintentos, 60 s)`. Se despierta también con el fin de una tarea y con el reloj manual, y nunca con un dato que no sea público.

**Cierre y cálculo:** `store::close` hace `COMMIT` y da `ClosedWindow`. Después corre la puerta de prueba (`compute_gate`), y `compute` en `spawn_blocking`: un pánico del motor se trata como un fallo de reconstrucción (parada segura), y la semilla se vuelve a leer en el siguiente intento.

**Liberación:** una tarea por pelea, sin acceso al pool, con tick de 50 ms. Al salir `end`, la tarea del `JoinSet` del bucle devuelve el `FinalLog`, y el runner agenda `settle_at`. Perder el bloqueo no la detiene (DP-4), y al recuperarlo la emisión en curso se conserva: nunca hay dos por pelea (R6). `elapsed` usa resta saturada: con `live_started_at` en el futuro no se libera nada, ni `intro` (R4). Si una tarea muere sin entregar el `FinalLog`, la pelea vuelve a `Recompute` (R7). `release.rs` y `EventSink` no registran ningún evento: ni `tracing` ni contenido de los lotes, y `fights_logs.rs` busca sus fragmentos como canarios.

**Liquidación (RF18):** el `FinalLog` vive en un `Arc`, para que un pánico no lo pierda. Cada intento es un `tokio::spawn` con tiempo máximo de 30 s, que abre la transacción, aplica `lock_timeout`, llama a `settle_decided`, al gancho y confirma. Resultados:

| Resultado | Efecto |
|---|---|
| `Confirmed` | Limpia la emisión y sus causas |
| `AlreadySettled` (0 filas) | Relee, cierra la emisión, sin reintento ni error |
| `Err` o `JoinError` | `fights.settle_failed` en `error`, backoff de 1 a 30 s; a los 60 s de fallos seguidos con el bloqueo tomado, causa de detención |

**Detención:** conjunto de causas (`safe_stop(id)`, `version_missing(id)`, `settle_failing(id)`). `version_missing` se amplía a cualquier fallo de `from_row` en `betting_open`, `betting_closed` o `live`, porque sale de datos públicos y no delata el resultado (R8). `halted` es "el conjunto no está vacío", y solo se publica con el bloqueo y tras `initial_done`. `fights.safe_stop` se registra en `error` cada 60 s, con `error_kind`.

**Supervisión:** las tareas de liberación y los intentos de liquidación viven en un `JoinSet` del bucle. Si el bucle entra en pánico, `fights.runner_panic` en `error`, se abortan las tareas de esa vuelta y se reinicia con backoff, partiendo de la base, para que una pelea nunca tenga dos emisiones. El apagado solo cancela: ninguna transición se ejecuta por apagarse, y una transacción abierta se revierte.

**Arranque:** `app::serve` crea un `CancellationToken`, hace `spawn` y envuelve `shutdown` para cancelarlo. `serve_listener` y `run_until_shutdown` no cambian. Los tests lo arrancan con `fights::runner::run` y `Settings.fights`.

### Ajustes de prueba
`Settings.fights`, nunca variables de entorno, como `LinkGateConfig`: `clock`, `hook`, `compute_gate`, `sink` y tiempos de reintento. La fuente de semillas de RF45 no entra en A.

### Semillas conocidas sin `dev-fights`
RF45 es de B. En A, un test obtiene la semilla de dos formas: la lee de `fight_secrets` por SQL propio, o sigue la ruta que RF4 y RF8 ya admiten, que describe la preocupación 1 (`INSERT` en `scheduled` con specs dorados, `INSERT` del secreto y `UPDATE` a `betting_open`). Con esa ruta, `support/fights.rs` fuerza KO, decisión y empate con los vectores de `crates/engine/tests/golden`, y corrompe un snapshot sin deshabilitar triggers. Con R2, un secreto ausente o distinto solo se puede simular con `ALTER TABLE fight_secrets DISABLE TRIGGER` en el test, que es exactamente el caso de la preocupación 1. Ningún código de prueba entra en el binario.

### API
- Rutas `GET /api/fights/current` y `GET /api/fights/{id}`, `Effect::Read`.
- `{id}`: `Result<Path<String>, PathRejection>`. Todo rechazo, y todo valor que no sea un decimal canónico de 1 a `i32::MAX`, da 404 `fight.not_found`. Así, `%FF` no devuelve el texto de axum.
- `FightDetail` se construye en un solo sitio, `api::detail_of(FightRow)`, que solo llena `seed` y `log` con `state = settled`.
- `no-store` en todas las respuestas de los dos handlers.
- `current` no toca la base; `server_time` se calcula en cada petición con `FightClock`.

### Logs
Eventos nuevos, solo con `event` y `error_kind`, sin `fight_id`: en `debug`, `fights.opened`, `fights.closed`, `fights.live`, `fights.settled`; en `warn`, `fights.lock_timeout`, `fights.db_unavailable`, `fights.row_failed`, `fights.lock_lost`; en `error`, `fights.safe_stop`, `fights.settle_failed`, `fights.roster_invalid`, `fights.runner_panic`. `error_kind` sale de `db::error_kind`, `SnapshotError::code`, `ContentError::code` o `SpecError::code`.

## Orden de trabajo
Un commit por paso con `feat(server): … (TA-13)` o similar. Cada paso termina con sus tests en verde, ejecutando solo `cargo test -p server --test <archivo>`.

1. **Protocolo.** `fights.rs`, `FightErrorCode`, `just gen`, DP-9. `svelte-check` en verde.
2. **Migración y `db::error_kind`**, con `tests/fights_schema.rs`. **Es el paso más riesgoso:** de él dependen todas las garantías, y una vez publicada, una migración no se corrige, solo se suma otra.
3. **Dependencias, `snapshot.rs`, `sealed.rs` y `clock.rs`**, con el dorado, los doctests, `ruleset_freeze.rs` y `fights_source_guard.rs`. Tras cambiar el lock: `just crate-age`.
4. **Puras:** `matchup.rs`, `state.rs` y la configuración (RF11). **Paralelizable con 3**, una vez hecho el 2.
5. **`store.rs`, `roster.rs`, `hook.rs`, `bets_contract.rs`.** RF1, RF3, RF8, RF9, RF19, RF22, RF29, RF31 a RF33, con llamadas directas y sin runner.
6. **`release.rs`, `runner.rs`, `board.rs` y arranque.** RF10, RF12 a RF18, con reloj manual.
7. **API y límite.** RF34 a RF36, y la verificación pública de RF7.
8. **Test 12 y canarios.** RF27 y RF28, parte de A.
9. **Imagen y CI.** `Dockerfile`, `check-compose` con sus fixtures, y `ci.yml`. **Paralelizable con 3 a 8**: no toca los mismos archivos.
10. **Cierre.** `just dev`, `just prod-local`, `just check` y `just deny`, con la salida en el PR.

## Riesgos
- **Fuga de la semilla o del log por un camino nuevo.** → Tipos sin traits, doctests, barrera por tokens y test 12 generado desde `ROUTES`, con un `match` exhaustivo sobre `Handler`: una ruta nueva sin caso no compila.
- **Un secreto o el log en un mensaje de error.** → Todo error se reduce a un código. Con `A9`, Postgres no escribe el `Failing row contains` de un `CHECK`. `tests/pg_log.rs` suma un caso con un canario en `spec_a` que viola el guard.
- **Pelea con el par perdido en una ronda.** Si una `scheduled` que cierra su ronda se cancela con la fila siguiente ya creada, su par no vuelve a salir y se rompe "exactamente 100". → Se crea la fila siguiente solo tras evaluar las transiciones, y no hay filas nuevas en detención. La ronda se calcula con las no canceladas. Un test cubre ese orden de evaluación.
- **`FOR SHARE` encadenados retrasan el cierre.** → El filtro por hora de RF29 y `lock_timeout` con reintento, con un test de plazo acotado.
- **Reloj manual y tick real se desincronizan en un test.** → Los tests avanzan el reloj y esperan una condición en la base con un límite. No hay `sleep` fijos ni `tokio::time::pause`.
- **Los tests que ya ejecutan el binario real ahora tienen runner.** Con la `cwd` de `cargo test`, el `CONTENT_DIR` relativo daría `roster_invalid`, y hay una conexión más. → `DEFAULT_ENV` y `playwright.config.ts` llevan `CONTENT_DIR` absoluto. Se corren enteros `logs`, `startup`, `shutdown`, `healthcheck` y `pg_log` en los pasos 6 y 10.
- **RF27 agota las cuotas o revoca la sesión con la que prueba.** → Una IP de `X-Forwarded-For` por petición (`TRUST_LOOPBACK`), y una sesión nueva antes de cada ruta que la pueda revocar.
- **`dev-auth` cambia la lista de rutas.** → El `match` de RF27 lleva los brazos bajo `cfg`, y `just test` corre las dos variantes.
- **Disco justo (unos 18 GiB).** → Solo `cargo test -p server --test …` hasta el paso 10, una sola pasada completa de `just check`, y `just prod-local` al final, con `docker image rm` de la imagen local al terminar. Sin builds de release fuera de eso.
- **Rama larga, un PR grande.** → Commits por paso, para revisar uno a uno. `qa-engineer` verifica contra la tabla de RF de abajo, y `devops-engineer` revisa el paso 9.

## Alternativas descartadas
- **Ejecutar la liberación dentro del bucle del runner.** Una espera de la base detendría los eventos, contra RF40. Por eso va en su propia tarea, sin pool.
- **Escribir el reveal en `sealed.rs` entero.** Rompería "único SQL de `fights`". Con `store::write_decided` que recibe `&[u8]`, los bytes solo salen dentro de esa llamada.
- **Adelantar `dev-fights` a A.** Agrega superficie al binario y a los scripts antes de hacer falta, porque los tests obtienen la semilla sin ella.
- **`Option` en `time_limit_ms` y `max_duration_ms` (DP-3).** Trasladaría el caso de la versión ausente a cada cliente, y un valor por defecto mentiría.
- **Renombrar los `outcome` locales (DP-5).** Cumple RF24 al pie de la letra, pero toca archivos ajenos y cualquier variable futura con ese nombre rompería un test sin que haya fuga.
- **`SERIALIZABLE` en las transiciones.** `UPDATE … WHERE state = $previo` y `FOR SHARE` ya dan la garantía de RF29, y sumaría reintentos.

## Pruebas (cómo se demuestra que funciona)
Todas las integraciones usan `TestDb` y reloj manual, y sus plazos son de milisegundos o segundos reales. Un binario con la captura de logs lleva exactamente un test (`tests/test_layout.rs`).

| RF | Nivel y archivo | Qué comprueba |
|---|---|---|
| RF1, RF3 | `fights_lifecycle.rs` | Roster inválido, sin filas; bytes de `canonical_spec_json`; `sha256(secreto) == commit`; un fallo a mitad no deja ni commit ni estado |
| RF2 | `fights_snapshot.rs` + `golden/fight-snapshot.json` | Vector fijo y concatenación exacta |
| RF4, RF8, RF9, RF25, RF59 (base) | `fights_schema.rs` | SQL directo, un caso por columna, arista y regla (DP-3, DP-6), con `TA006` (`23514` en las cotas de DP-3), incluidos: un secreto que no coincide con el commit y `betting_open` sin secreto (R2), cada arista con su columna nula, `winner` sin `end_reason`, y `time_limit_ms` y `max_duration_ms` fuera de rango; `DELETE`, `TRUNCATE`; `CHECK` con el trigger deshabilitado; reglas de `voided` |
| RF5 | `fights_recovery.rs`, `fights_source_guard.rs` | Cambiar o borrar `content/` y reiniciar da el mismo log; `load_roster` solo en `roster.rs`; cada discrepancia da parada segura; un spec que decodifica y no pasa `validate` da `fight_unrunnable` en `scheduled` y parada segura en `betting_closed` (M6) |
| RF6 | `ruleset_freeze.rs` | Tabla vacía y mecanismo con versión y hash de prueba |
| RF7 | `fights_public_verify.rs` | El verificador es un módulo sin ningún `server::` (un test de fuente lo exige): commit, RF2, `simulate`, `engine_wasm::run`, `final`, resumen. En una `decided`, `spec_*` o `log` nulos son fallo de verificación, no un paso omitido (R3). También `voided` y cancelada, y tras cambiar `content/` |
| RF8 | `fights_lifecycle.rs` (`proptest`) | Secuencias repetidas o desordenadas: mismo estado final y ningún error visible |
| RF9, RF31 | `fights_settle.rs` | Dos runners, un final, una llamada efectiva; con `decided{a}`, `decided{null, draw}`, `cancelled` y `voided` (este con una transacción de prueba que lo escribe por SQL y llama al gancho, porque nada en `src` lo escribe): si falla o se mata antes del `COMMIT`, no quedan ni fila ni reveal |
| RF10 | `fights_lock.rs` | Dos runners: avanza uno; al soltar, retoma el otro; con `pg_proxy`, tanto con la conexión cerrada como colgada (modo que retiene los bytes), deja de actuar antes del tiempo máximo: nada se liquida y una emisión en curso sigue (DP-4); al recuperarlo, una sola emisión (R6); con el modo colgado manteniendo vivo el backend, otro runner retoma en un plazo acotado (R5) |
| RF11 | `config.rs` (unitario), `startup.rs` | Validación, incluida `ventana = 0`, y `config.invalid` con el nombre de la variable |
| RF12, RF13, RF14 | `state.rs` (tabla), `fights_lifecycle.rs` | Toda la tabla de RF15 con DP-1 (con `ventana = 1 s`, el mínimo es 500 ms) y DP-2; `scheduled` con un spec que no pasa `validate` (M6); anuncio; sin duplicados; una en `live`; ruta tardía |
| RF15, RF16 | `fights_recovery.rs`, `fights_binary.rs` | Runner abortado en cada fila; semilla, snapshot y `canonical_json` idénticos, y un final; causas de parada; detención y relectura de 60 s; un secreto restaurado retoma; un fallo de `from_row` en `betting_open` activa la detención (R8); tras un pánico del bucle, una sola emisión por pelea. `fights_binary.rs`: SIGKILL real con `live_started_at` en el pasado |
| RF16 | `fights_source_guard.rs` | Ver "Menores"; `ROUTES` y `cli::parse` sin anular ni leer la semilla; gancho que falla varias ranuras: ninguna ventana nueva, y al corregirlo, un final por pelea |
| RF17 | Doctests, `fights_bets_contract.rs` | `compile_fail`; `ClosedWindow` no se fabrica; puerta antes del cálculo |
| RF18 | `fights_settle.rs` | Gancho que falla, que entra en pánico, kill antes del `COMMIT`, y fila ya liquidada por otro proceso; sin detención antes de 60 s; tras liquidar, no queda la fila en `fight_secrets` (DP-6) |
| RF19 | `fights_lifecycle.rs` | Sin commit, reveal ni log, y con gancho |
| RF20, RF21 | `matchup.rs` (`proptest`) | 1200 peleas no canceladas, 100 por par, con canceladas intercaladas; recorrido exhaustivo con 4 personajes: distancia ≥ 5 (lo confirmé en una simulación aparte de RF20: 16 741 estados y un 49,6 % de transiciones con personaje compartido); ronda nueva con otro `roster_sha256`. Grafo: `check-engine` |
| RF22 | `fights_roster.rs` | Error o menos de 2 personajes: el server arranca, sin filas nuevas, y las creadas siguen |
| RF23 | Doctests en `sealed.rs` | `compile_fail,E0277`, `E0382`, `E0599`, con un doctest positivo al lado, para que un error ajeno no los dé por buenos |
| RF24 | `fights_source_guard.rs` | Tokens tras quitar comentarios, con cadenas incluidas, para ver el SQL; casos negativos por forma y DP-5, incluidos `log\n    .outcome()`, `<_>::outcome(&log)`, `log.r#outcome()` y `r#simulate(…)`, y los del quitador de comentarios (R1); `base64::engine` no cuenta; ningún tipo de `protocol` contiene `FightLog` |
| RF26 | `crates/protocol/src/fights.rs` | Forma exacta de los tipos |
| RF40 | `fights_release.rs` | Con un sink de prueba y reloj manual: en `t − 1` no llegó, y en `t` llega en el siguiente tick, incluidos `ko` y `end`. Con la base caída, siguen saliendo y nada se liquida. Con `live_started_at` en el futuro no sale nada, ni `intro` (R4). Una tarea que muere sin entregar el `FinalLog` vuelve a `Recompute` (R7) |
| RF27 | `fights_no_leaks.rs` | Cada estado × tres identidades × `ROUTES`, más `self`; cuerpos, errores y cabeceras; semilla en hexadecimal, base64, base64url y bytes; `canonical_json`; eventos no liberados |
| RF28 | `fights_logs.rs` (un test), `fights_binary.rs` | `RUST_LOG=trace`: ni semilla, ni log, ni fragmentos de eventos, también con el sink que descarta (DP-7). `ALLOWED_FIELDS` no cambia |
| RF29, RF30, RF33 | `fights_bets_contract.rs` | T1, cierre en espera, puerta, T2 `WindowClosed`; por hora, sin esperar; detención, sin bloqueo o antes de la evaluación inicial; lectores encadenados, plazo acotado; `scheduled` y `settled` dan `WindowClosed` |
| RF32 | `fights_bets_contract.rs` | `unsettled_in`, rango semiabierto, `voided` cuenta como liquidada |
| RF34, RF35, RF36 | `fights_api.rs`, `routes.rs` | `id` no canónico, incluido `%FF`; `seed` y `log` solo en `settled`; `no-store`; 429; 503 solo si falla la base (DP-10); una `voided` con un spec que no es UTF-8 da 200, con ese campo `null` y con `seed`, y el commit se comprueba; un spec que no es UTF-8 en `live` da `seed` nula (R3); `current` con la base caída; `halted` nulo sin bloqueo; `Effect::Read` |
| Imagen | `check-compose` y manual | Fixtures de la regla nueva; `just prod-local` (DP-11) |

También:
- [ ] `just gen` sin diferencias y `svelte-check` en verde (DP-9).
- [ ] `just dev`: al menos una pelea completa en la base, verificada a mano con `GET /api/fights/{id}`, con el resultado en el PR.
- [ ] `just prod-local`: una pelea completa con los personajes de la imagen.
- [ ] `just deny` y `just crate-age`, tras cambiar el lock.
- [ ] `just check` en verde, con la salida pegada en el PR.
- [ ] Sin captura de pantalla: la fase A no tiene UI.

## Estimación de CI
Sin jobs nuevos. Más ejecuciones de `rust`, `e2e` e `images`, porque sus filtros crecen. Los tests nuevos usan reloj manual y suman unos 1 a 2 min a `rust`. `fights_binary.rs` usa un `live_started_at` en el pasado para no esperar una pelea entera.

## Desviaciones durante la implementación
- **R5, `idle_session_timeout` en la sesión del bloqueo.** Las opciones `tcp_*` que pidió security-reviewer no bastan para el caso que pide probar: con el `pg_proxy` colgado, el kernel del proxy responde los keepalives y el backend sigue vivo, con el bloqueo. Por eso la sesión del bloqueo también lleva `idle_session_timeout` (30 s por defecto), muy por encima del latido de 5 s: sin latido, el propio Postgres cierra la sesión y suelta el bloqueo. Las opciones `tcp_*` siguen para una partición real. El latido va en su propia tarea, para que una pasada lenta no lo retrase.
- **Tiempo máximo de una pasada (`pass_timeout_ms`, 30 s), y una pasada se interrumpe si se pierde el bloqueo.** Con una base colgada, una consulta del pool no vuelve nunca, y `halted` y `accepts_bets` seguirían diciendo que todo va bien. Cortar una pasada es seguro: una transacción abierta se revierte y las filas se releen.
- **`next_action` no recibe `&Timing`.** La ventana mínima sale de las horas de la propia fila, así que un cambio de configuración nunca toca una pelea ya anunciada. Con la configuración por defecto da lo mismo que DP-1.
- **Una cancelación también es un intento en su tarea.** RF19 llama al gancho y RF18 pide que un pánico del gancho no mate al runner. `Settling` cubre los dos finales, en lugar de una ruta aparte. **Consecuencia para RF20:** el par de la cancelada cuenta como usado hasta su `COMMIT`, así que `pass` no anuncia ninguna fila mientras haya una cancelación pendiente, y el fin de esa tarea despierta el bucle. Si el gancho falla, la fila siguiente se retrasa con él, y a los 60 s hay detención, que tampoco crea filas. Lo prueba `a_cancelled_fight_gives_back_its_pair_before_the_next_announcement`, con el gancho reteniendo la transacción, sin depender de una carrera.
- **Una fila que falla no para a las demás (decisión, pedida por QA).** Un error de la base en el paso de una fila ya no aborta la pasada: las demás se evalúan y esa se reintenta con el backoff de la base, con `fights.row_failed` o `fights.lock_timeout` en `warn`. Un error de conexión sí la aborta, porque afecta a todas. Una fila **sin commit** nunca detiene nada: no tiene semilla ni apuestas, y termina cancelada al perder su ventana. Una fila **con commit** que falla durante `settle_halt_ms` (60 s) activa la detención, igual que una liquidación, en cualquiera de sus aristas. Al arrancar, mientras falla, la evaluación inicial no se da por terminada, así que no se aceptan apuestas ni `halted` dice `false` sin saberlo. A mitad de corrida, `halted` sigue en `false` y `accepts_bets` en `true` para las otras ventanas hasta esos 60 s, porque una fila que falla una vez no debe cerrar las apuestas de las demás (decisión de QA). Una fila que falla sigue reintentando durante la detención que ella misma causa, o nunca se levantaría, y una espera por detención no cuenta como que dejó de fallar.
- **Pasadas que abortan sin perder el bloqueo.** Con un error que no es de una fila (por ejemplo `LOCK TABLE fights`, que el latido no nota) y sin que ninguna pasada termine en 60 s, también hay detención, medida desde el inicio de la primera pasada fallida. `halted` sale de memoria, sin tocar la base. El runner solo lo nota en su siguiente pasada, que puede tardar hasta la relectura de 60 s, así que con un bloqueo de tabla el tope real es de unos 2 minutos: medido con el binario, 115,6 s.
- **RF20: una fila vencida y aún `scheduled` retiene el anuncio.** O se está cancelando, o no puede abrir y aún puede perder su ventana, y su par sigue contando hasta entonces. El anuncio se retrasa como mucho hasta que pierde la ventana, antes de la ranura siguiente, porque `ventana < cadencia` (RF11). Mientras tanto no hay fila para la ranura siguiente (matiz de RF12).
- **`sealed::cancel_fight`** borra en la misma transacción un secreto plantado en una fila cancelada, y es el único que llama a `store::cancel`. `GET /api/fights/{id}` usa `lock_timeout` de 1 s y un límite total de 3 s: un bloqueo da 503 y la conexión queda limpia.
- **`snapshot_errors` se reevalúa en cada relectura.** Una fila reparada a mano levanta la detención en un máximo de 60 s, sin reiniciar.
- **El 429 de `/api/fights/{id}` lleva `no-store`** con una capa por fuera del limitador.
- **Pendiente para TA-27:** `ComputeError::NoEnd` (`log_without_end`) no está en la lista de `engine_error` de RF59 paso 6. No se cambia la spec aquí. TA-27 decide cómo lo trata; hoy es inalcanzable, porque el motor siempre escribe `end`.
- **Causas de detención.** `version_missing(id)` se implementa como "`from_row` falla en `betting_open`, `betting_closed` o `live`" (R8), con `fights.safe_stop` en `error`, igual que una parada segura.
- **Reparto de commits.** `store.rs` entra con `sealed.rs`, porque se necesitan entre sí, y `fights_source_guard.rs` y `fights_bets_contract.rs` entran con el runner, porque leen o arrancan su código.
- **Doctests `compile_fail`.** En Rust estable, rustdoc no comprueba el código de error `E0xxx`, solo que no compile. Por eso hay un doctest positivo con las mismas formas, que falla si un `use` queda mal.
- **`lazy_pool` de los tests** usa `ACQUIRE_TIMEOUT`, como el pool real. `/api/fights/{id}` es la primera ruta sin sesión que toca la base, y los tests sin base esperarían 30 s.
- **`tests/startup.rs`** suma la migración nueva a su lista exacta.

## Revisión
### architect — 2026-10-31 — REQUEST_CHANGES
- Verifiqué, sin compilar: el plan contra la spec y ADR-0002, 0006 D10, 0008 D8 y 0012 A4, A8 y A9. En el código: los `outcome` locales (`db.rs`, `admin.rs`, `ws/session.rs`) y que ningún otro token de RF24 aparece hoy en `apps/server/src`; `simulate` ya llama a `validate`; `engine_wasm::run` es Rust normal, utilizable en un test nativo; `time_limit_ms` y `end_ms` existen en `Ruleset`; `ERROR_CODES` y `ERROR_TEXT` son mapeados; `server --help` da `Invalid`; `ci.yml` no tiene un filtro `e2e-server`; `check-compose` cierra las variables de `server` y `compose.local.yml` solo agrega `ports`; `.dockerignore` deja pasar `content/characters`; el `Dockerfile` ya copia `crates`; `TA006` es el siguiente código libre.
- DP-1 a DP-9 y DP-11, y todos los menores: aceptados, con los ajustes de abajo. DP-10: cambiado. Quedan en la "Enmienda (fase A, 2026-10-31)" de la spec. Ninguno toca un ADR.
- Cambios pedidos:
  1. **DP-10.** Un 503 por bytes que no son UTF-8 oculta la semilla justo en una `voided` por `snapshot_invalid` (ADR-0002 punto 4, RF7, RF59). `spec_a` y `spec_b` pasan a `Option<String>` y `log` va `null` si no es UTF-8; `seed` sale igual. 503 solo si falla la base. Sumar a `fights_api.rs` una `voided` con un spec que no es UTF-8: 200, con `seed`, y el commit se comprueba.
  2. **Bloqueo (RF10).** "Pool propio de una conexión" no basta tal como está escrito. En un pool, un `acquire` tras una caída abre otra conexión, y el `SELECT 1` pasaría sin tener el bloqueo. Hay que retener una sola conexión todo el tiempo que dure el bloqueo: un `PgConnection::connect_with` directo, como en `admin.rs`. El `SELECT 1` necesita un tiempo máximo, y cualquier fallo o vencimiento cuenta como bloqueo perdido. `fights_lock.rs` lo prueba con `pg_proxy`, también con la conexión colgada y no solo cerrada. Hoy `cut` solo cierra, así que `support/pg_proxy.rs` necesita un modo que retenga los bytes, y va en "Archivos que cambian".
  3. **DP-7 y DP-4.** Sumar RF40 a "Entra" y una fila a la tabla de pruebas: con un sink de prueba y reloj manual, en `t − 1` no llegó y en `t` llega en el siguiente tick, incluidos `ko` y `end`, y con la base caída siguen saliendo. Quitar "la fase B la prueba": A ya puede probarlo con el sink. `release.rs` y `EventSink` no registran ningún evento, y `fights_logs.rs` ya busca sus fragmentos como canarios.
  4. **M6 (`validate`).** Sacarlo de "Menores", porque cambia el comportamiento. `SnapshotError` lleva una variante propia para él, para que TA-27 no lo trate como el paso 4 de RF59: sigue a los pasos 5 y 6. Sumar el caso a `fights_recovery.rs` y a `state.rs`.
  5. **Migración.** Cada arista exige no nulas las columnas que escribe (`seed_commit` y `opened_at`; `closed_at`; `live_started_at`; `settled_at`), y `winner` solo puede tener valor con `end_reason`. Hoy el guard solo dice "solo se escriben sus columnas", y un `live` sin `live_started_at` pasaría. `CHECK` de `time_limit_ms > 0` y `max_duration_ms >= time_limit_ms` (DP-3). Un caso por regla en `fights_schema.rs`.
  6. **DP-5.** La búsqueda es por tokens, no por subcadena: los casos negativos suman `log\n    .outcome()` y `<_>::outcome(&log)`.
  7. **DP-1 y RF11.** `config.rs` rechaza `ventana = 0` con `config.invalid`, y el mínimo se calcula en ms: con `ventana = 1` es 500 ms, no 0. Casos en `state.rs` y `config.rs`.
  8. **e2e.** `serve-with-log.mjs` arranca el server con `cwd` en `apps/web`, así que el `CONTENT_DIR` por defecto da `fights.roster_invalid` en cada corrida. Sumar `CONTENT_DIR` absoluto al `env` del server en `apps/web/playwright.config.ts`, y ese archivo a "Archivos que cambian".
  9. **Reinicio tras un pánico.** Hoy el plan dice que el bucle "se reinicia partiendo de la base", pero las tareas de liberación son aparte. Al reiniciar se abortan las de la vuelta anterior (por ejemplo, con un `JoinSet` del bucle), para que una pelea no tenga dos emisiones. Sumar el caso a `fights_recovery.rs`.
  10. **Texto.** Cambiar "La spec no se edita…" por una referencia a la enmienda, corregir el razonamiento de DP-6 según RF25 enmendado (no es cierto que el secreto "solo pueda existir" dentro de esa transacción si un liquidador olvida borrarlo; un test de RF18 comprueba que no queda la fila).
- Al corregir, se vuelve a pedir revisión de architect, solo sobre estos diez puntos. La enmienda de la spec queda pendiente de security-reviewer. Por `risk: high`, sigue haciendo falta el OK del owner.

### backend-engineer — 2026-10-31 — correcciones de la ronda 1
- Apliqué los diez puntos, sin objeciones. Comprobé en el código los que dependían de él: `admin.rs` usa `PgConnection::connect_with`, `PgProxy::cut` solo cierra, y `serve-with-log.mjs` arranca el server con `cwd` en `apps/web`.
- De paso, el `JoinSet` del punto 9 cubre también los intentos de liquidación.
- Pido de nuevo revisión de architect, solo sobre estos diez puntos.

### architect — 2026-10-31 — APPROVE (segunda ronda)
- Verifiqué, sin compilar, solo los diez puntos de la ronda 1 y los agregados. Los diez están aplicados y coinciden con la enmienda de la spec.
- Agregados aceptados. `JoinSet` en los intentos de liquidación: abortar uno es seguro, porque sin `COMMIT` se revierte, y si ya confirmó, el siguiente intento da `AlreadySettled`. `SELECT 1` con 3 s: un vencimiento en falso cierra hacia el lado seguro (`WindowClosed`, `halted` nulo). Sin fila en `fight_secrets` tras liquidar: es lo que pide RF18.
- Notas para implementar, que no cambian el plan:
  - El `JoinSet` tiene que sobrevivir a la pérdida del bloqueo, que no reinicia el bucle (DP-4). Solo el pánico del bucle aborta sus tareas.
  - Sin bloqueo no empieza ningún intento de liquidación. Uno ya en curso puede terminar, porque el `UPDATE … WHERE state = 'live'` da un solo final.
  - `PgConnection::connect_with` lleva su propio tiempo máximo, como en `admin.rs`.
  - Las cotas de `time_limit_ms` y `max_duration_ms` son solo `CHECK`: `fights_schema.rs` espera `23514` en esos casos, no `TA006`, salvo que el guard las repita.
- La enmienda de la spec (DP-3, DP-4, DP-5, DP-6, DP-7, DP-10 y M6) sigue pendiente de security-reviewer. Si cambia algún punto, el plan se ajusta antes de tocar ese código.
- Esta aprobación es solo la de architect, no registra ningún OK del owner.

### backend-engineer — 2026-10-31 — ajustes antes de implementar
- Corrijo M6 según la spec enmendada: `SpecInvalid` es variante propia para el log y en RF59 se trata como el paso 4. Corrijo el motivo de DP-5: no era cierto que fueran "las únicas" formas; la defensa real es que no se obtiene un `FightLog` sin un token prohibido.
- Aplico las recomendaciones R1 a R8 de security-reviewer, comunicadas por quien coordina, en la migración, el runner y las pruebas. R2 cambia los fixtures: un secreto ausente o distinto ahora solo se simula deshabilitando el trigger en el test.
