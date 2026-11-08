---
id: "012"
title: "Apuestas, fase B: API, reparto, bono, rescate, carencia, pozo y capacidad"
type: plan
status: in-review
risk: high
author: backend-engineer
reviewers: [architect]
approvals: []
linear: TA-27
depends_on: ["008", "010"]
created: 2026-11-02
updated: 2026-11-07
---

# Plan: Apuestas, fase B (desde spec.md 2026-11-02)

> [intent](./intent.md) · [spec](./spec.md) · [plan-a](./plan-a.md) · ADRs: [0005](../../decisions/0005-economia-sin-dinero-real.md), [0008](../../decisions/0008-autenticacion.md) D6 a D8 y sus notas `proposed`, [0012](../../decisions/0012-logs-y-cabeceras-de-seguridad.md) · 010: RF27 a RF33, RF41, RF43

**Alcance.** RF5 (pruebas), RF10 a RF12, RF13 con reparto, RF14, RF15, RF25 a RF40 y RF47, con los tests 7, 8, 18, 23, 24, 25 y 26. La web solo suma los códigos nuevos y que el socket ignore `pool_update`. Panel, lecturas SSR, form actions, mensajes de panel y e2e son de C.

**Punto de partida.** `main` en `192b8ab`, con A fusionada (PR #86), en la rama `ta-27-apuestas-b`. La enmienda de ADR-0002 y la spec ya tienen el OK del owner (2026-11-06).

**Gate.** El PR se puede abrir, revisar y aprobar, pero **no se fusiona sin el OK explícito del owner a:**
- los valores `B`, `m` y `r` (RF40);
- las dos notas `proposed` de ADR-0008 (tope del rake y carencia por peleas);
- las preocupaciones 1 y 3 de la spec de 010, que se vuelven reales con la primera apuesta (preocupación 6), y lo que queda abierto de la preocupación 21 de esta spec.

**OK del owner, en chat, el 2026-11-07:** los valores de RF40 tal como están, las dos notas de ADR-0008 (ya marcadas como aprobadas en el ADR), y las preocupaciones 1 y 3 de 010 y lo abierto de la 21 de esta spec, como riesgo aceptado. Con eso, el gate queda cumplido. Lo que falta antes del merge es lo de siempre: `just check`, `just e2e`, reviewer y security-reviewer.

Con ese OK, el mismo PR marca aprobadas las notas de ADR-0008. Este gate manda sobre la autonomía de merge. **Si el owner rechaza una nota,** se vuelve a la letra de D8 y la spec regresa a revisión (preocupación 4): el paso 3 y el 5 cambian de fórmula, y no se fusiona con la nota rechazada.

## Importes de los asientos (decidido por architect, preocupación 21)
Cierra las dos condiciones de security-reviewer para B (ronda 2 del PR #86). Ver también RF2, RF13 y RF25.
- **`bet_stake` se ata en la base.** Migración `20261103000000_bet_stake_amount.sql`: `CREATE OR REPLACE FUNCTION ledger_posting_rules` suma que un `bet_stake` solo admite usuario `−stake` y `bet_escrow +stake`, con el `stake` de su `bet_id`, y si no, `TA008`. La lectura de `bets` va solo en la rama de `bet_stake`, así que el reveal no paga nada. El resto del cuerpo no cambia, y el PR muestra su `diff`. Esta función está vigilada, así que `GUARD_CHANGES_ALLOWED` suma esta línea.
- **`refund = stake` no entra.** Costaría una lectura por movimiento en el reveal, no frena a quien tiene la credencial, y `bet_payout` dejaría el mismo hueco.
- **Consulta de producción.** `apps/server/README.md` lleva, entre marcadores, una consulta de solo lectura en una sola sentencia, con lo que pide RF13. Un test la lee del README y la ejecuta.
- **Cuadre del gancho.** `parimutuel::check_split(&split, total)` en memoria, antes de la primera escritura, que es `pool_settlements` (paso 4). Si falla, `Err` con `kind: "split_mismatch"`.

## Desviaciones de la spec encontradas al leer el código
Ninguna cambia un requisito. Cada una se aplica como se indica, salvo que architect pida otra cosa.

| # | Spec | Código real | Qué hace este plan |
|---|---|---|---|
| D1 | RF25 fila 4 y "Un error de la base da 503 `bet.unavailable`" | `lock_open_for_bet` convierte todo error de base en `WindowClosed`, a propósito (010 RF29), y la transacción ya está perdida | Un fallo en esa fila da 409 `bet.window_closed`. No se cambia ese contrato de 010. Todos los demás fallos dan 503 |
| D2 | RF25 fila 10, RF30: saldo en la transacción | `ledger::balance` recibe un `PgPool`: leería fuera de la transacción y sin ver lo propio | Función nueva `bets::facts`, una sola sentencia sobre la conexión de la transacción (ver "Hechos de una cuenta") |
| D3 | RF36: "`ws.rs` corrige su comentario" | Ese comentario está en `crates/protocol/src/ws.rs`, encima de `SelfEvent`, y sale en el TS generado | Se corrige allí, y `just gen` lo recoge |
| D4 | Preocupación 13: `CoinAmount` y `Me.balance` | `ts(type = "number")` es atributo de campo, y `only_me_balance_overrides_its_ts_type` cuenta 1 en todo `src` | `CoinAmount(#[ts(type = "number")] i64)`, con `serde(try_from = "i64")` y `new` acotado a `MAX_SAFE_INTEGER`. `Me.balance` pasa a `CoinAmount`: el JSON no cambia y el contador sigue en 1. El test se renombra a `only_coin_amount_overrides…`. `wire_balance` se reemplaza por `CoinAmount::new` |
| D5 | RF40: "`BET_*`, en `Config`" | Quien escribe la fila es `Roster::announce` y `FightScript::announce`, con `FightsConfig` | `rake_bp` se lee en `Config` y entra en `FightsConfig`, para que el anuncio lo escriba sin otra dependencia. `NewFight` suma `rake_bp`. `columns!` y `FightRow` no cambian: solo el gancho lo lee, con su propia consulta |
| D6 | RF29 y RF30, D7: "usuario 5/min, IP 60/min"; RF37: "`wallet` y `bets/mine`" | D7 y RF37 dan un límite por fila, sin decir si es un bucket por ruta | **Un bucket por fila**, como `FIGHT_READ_IP`: `bonus` y `bailout` comparten el suyo, y `wallet` y `bets/mine` también. **Aviso para C:** un SSR con `wallet` y tres `bets/mine` gasta 4 de los 60/min por usuario |
| D7 | RF47, RF33, RF25: un `{id}` no canónico | `parse_id` es privado en `fights/api.rs`, que no se toca (preocupación 11), y `admin.rs` ya tiene otra copia | `fights::canonical_id` en `fights/mod.rs`, que usan `bets/` y `admin.rs`. `api.rs` conserva la suya hasta que TA-44 esté fusionada |
| D8 | "Tabla de la API": 503 con su código de dominio | `session::current` falla con `auth.try_again` en el resto del server | En estas rutas, una sesión que no se puede leer da el 503 del dominio (`bet.unavailable`, `wallet.unavailable`), para que RF42 tenga una sola salida |
| D9 | RF35: la tarea del pozo | `serve_listener` lo usan los tests en proceso, y una tarea allí consultaría una base que muchos no tienen | La tarea se lanza en `app::serve`, junto al runner, y los tests la lanzan a mano, como `f::start`. `GET pool` lee de la base si no hay caché, y `quote` responde `bet.window_closed` |
| D10 | RF2: los flujos de `ledger/` | `settle_batch` ya existe. Falta el asiento `rake`, con `user_id` nulo por `ledger_entries_user_ck` | `ledger::rake(tx, fight_id, amount)` |

RF1 ya recoge lo que A endureció: este plan parte del código, con `rake` sin usuario y sin `bet_id`.

## Archivos que cambian

| Archivo | Cambio | Paso |
|---|---|---|
| `crates/protocol/src/{coins.rs,bets.rs,lib.rs}` | Nuevos: `CoinAmount` y los tipos de "Tipos" de la spec, con `PoolOutcome` | 1 |
| `crates/protocol/src/{error.rs,me.rs,ws.rs}` | `BetErrorCode`, `BonusErrorCode`, `BailoutErrorCode`, `WalletErrorCode`; `Me.balance`; `ServerMessage::PoolUpdate` y el comentario de `SelfEvent` | 1 |
| `packages/protocol-ts/` | Generado, nunca a mano | 1 |
| `apps/web/src/lib/features/auth/{codes.ts,error-text.ts}`, `messages/{en,es}.json` | Los códigos nuevos y su texto: sin ellos, `svelte-check` falla | 1 |
| `apps/web/src/lib/features/fight/live/` (`fight-socket` y `live-store`, con tests) | Ignorar `pool_update`, con prueba | 1 |
| `apps/server/src/config.rs`, `apps/server/.env.example`, `infra/docker-compose.yml`, fixtures de `check-compose` si hacen falta | RF40 y `DAILY_GRANT = 500` | 2 |
| `apps/server/src/fights/{mod.rs,store.rs,roster.rs,script.rs}` | `rake_bp` en `FightsConfig` y en `NewFight`; `canonical_id` | 2 |
| `apps/server/src/bets/parimutuel.rs` | Nuevo: RF10, `payout_bp`, RF11, `check_split`. Puro | 3 |
| `apps/server/migrations/20261103000000_bet_stake_amount.sql`, `GUARD_CHANGES_ALLOWED` de `apps/server/src/admin.rs` | RF2: `bet_stake` atado a `bets.stake` | 7 |
| `apps/server/src/ledger/mod.rs` | `rake` | 4 |
| `apps/server/src/bets/settle.rs` | El reparto reemplaza a `PAYOUT_UNAVAILABLE` | 4 |
| `apps/server/src/bets/probation.rs` | Nuevo: RF27, puro, y sus constantes | 5 |
| `apps/server/src/bets/facts.rs` | Nuevo: la sentencia única y `BailoutState` | 5 |
| `apps/server/src/bets/{mod.rs,claims.rs,wallet.rs,api.rs,audit.rs}` | Nuevos: RF25, RF29 a RF33, y el `auth_events` propio de `bets/` | 6, 7 |
| `apps/server/src/rate/{quotas.rs,mod.rs}`, `apps/server/src/http/routes.rs`, `apps/server/src/app.rs` | Cuotas, rutas (**posible choque con TA-44**), estado y tarea | 6 a 9 |
| `apps/server/src/profile/me.rs` | `Me.balance` con `CoinAmount::new`, sin `wire_balance` (D4) | 1 |
| `apps/server/src/bets/pool.rs` | Nuevo: caché y tarea (RF34, RF35, RF47) | 8 |
| `apps/server/src/fights/hub.rs`, `apps/server/src/ws/session.rs` | Últimos `pool_update`, por `schedule` | 8 |
| `apps/server/src/ws/{handshake.rs,mod.rs}` | RF39 | 9 |
| `apps/server/tests/support/{bets.rs,fights.rs}` | `rake_bp`, sesiones de apostadores, sembrado masivo, tarea del pozo | 2 a 8 |
| `apps/server/tests/{parimutuel_vectors,bets_api,bets_claims,bets_wallet,bets_pool,bets_logs,bets_load,ws_capacity}.rs` | Nuevos | 3 a 11 |
| `apps/server/tests/{bets_settle,fight_void,fights_no_leaks,fights_fanout,banned_writes,routes,startup,logs}.rs`, `apps/server/src/{config.rs,admin.rs}` | Ver "Tests existentes que cambian" | 2 a 9 |
| `apps/server/README.md`, `docs/architecture.md` | Paso 12 | 12 |
| `docs/decisions/0008-autenticacion.md` | Marcar las notas, **solo tras el OK del owner** | 13 |

Sin cambios: las migraciones ya existentes, `fights/api.rs`, `fights/runner.rs`, `fights/sealed.rs`, `crates/engine`, `crates/content`, `crates/sim`, `justfile` y `Cargo.lock`.

**Una sola migración,** la de `bet_stake`, en el paso 7. A ya creó todo lo demás: tablas, índices, `auth_events.bet_id` y `fights.rake_bp`.

## Orden de trabajo
Cada paso deja `just check` en verde y es un commit `feat(scope): … (TA-27)`.

### 1. Protocolo y web mínima
Tipos de la spec, con `Wallet`, `BetPlaced`, `MyFightBets`, `BetQuote`, `FightPool` y `ServerMessage::PoolUpdate`. `CoinAmount` según D4. Los enums de error con `ALL` y `as_str`, cada uno dueño de su prefijo. `just gen`.

En la web: `ERROR_CODES`, `ERROR_TEXT` y sus mensajes en en y es, por los que RF46 revisa textos en C. `live-store` ya ignora tipos desconocidos, por su cadena de `if`; se agrega el test de que `pool_update` no cambia la vista, y de que no dispara lecturas.

**Atención:** si TA-44 ya está en `main`, cualquier cambio de `protocol` pide `just gen-wasm`.

### 2. Configuración y `rake_bp` (RF40, RF10)
`BET_MIN` (25), `BET_RAKE_BP` (0) y `BET_CAP_MULTIPLE` (5), con `R = ⌊B / 2⌋`. Reglas: `1 ≤ m ≤ R`, `0 ≤ r ≤ 2000`, `1 ≤ m_cap ≤ 20`. Un `m > R` da `config.invalid` con `BET_MIN`, porque el valor por defecto es lo que un `DAILY_GRANT` chico hace fallar. `BetsConfig` en `Config` y en `AppState`. La CLI no la lee.

`rake_bp` se escribe al anunciar, en `Roster::announce` y `FightScript::announce`. `DAILY_GRANT: '500'` en el compose y en `.env.example`, sin agregar `BET_*` (preocupación 12).

### 3. Funciones puras (**paralelizable** con el paso 5)
`bets/parimutuel.rs`, sin SQL, sin reloj y en `i128`:
- `split(stakes_ganadoras, L, r_bp) -> Split { rake, residue, payouts }`, en el orden de entrada, con la fórmula exacta de RF10.
- `payout_bp(propio, otro, r_bp) -> Option<u32>`: `⌊10000 · N / W⌋`, con `None` si un lado está vacío, y saturando en `u32::MAX` con `try_from`, sin `as`.
- `ranked_profit(payout, stake, L, L_c)`, con las cuatro ramas de RF11.
- `quote(amount, propio, otro, r_bp) -> (payout, one_sided)`: suma `amount` al pozo y llama a la misma función (RF47).

Ninguna conversión con `as`. Una cifra fuera de `i64` da error, y el gancho la convierte en `row_invalid`.

### 4. Reparto en el gancho (RF9 a RF14)
`settle.rs` lee también `ranked_stake` y `SELECT rake_bp FROM fights WHERE id = $1`. `PoolResult::Split`, primero todo en memoria y después las escrituras:
1. `split` y `check_split` en memoria. `L_c` sale de las perdedoras con `ranked_stake`. Si `check_split` falla, `Err` sin escribir nada.
2. `pool_settlements` con `paid`, `rake` y `residue`: la primera escritura (RF8).
3. `settle_batch` con `SettleKind::Payout`, solo para las ganadoras: `payout ≥ stake > 0` y RF10 lo garantiza.
4. `ledger::rake` si `rake + residue > 0`.
5. `bet_settlements`: `won` y `lost`, por lotes.

Desaparece `PAYOUT_UNAVAILABLE`. RF14 sigue: la fuente de `settle.rs` no lee `users` ni `banned_at`, y la búsqueda de RF24 sigue prohibiendo `.outcome`.

### 5. Hechos de una cuenta (RF27, RF30, RF32)
`bets/probation.rs`, puro: `Probation::of(age, fights) -> Option<Probation>`, con 7 días y 10 peleas, y sin volver a entrar. `facts.rs`, **una sola sentencia** sobre la conexión que se le pase, que da en una foto:
- `saldo`, `stakes abiertos` (de peleas con `state <> 'settled'`);
- si hay `grant` y si hay `bailout` de hoy;
- la edad de la cuenta y `count(DISTINCT fight_id)` de `bet_settlements` con `outcome <> 'refunded'`, por el índice parcial;
- el siguiente reinicio, con el `now()` de la base.

`BailoutState::of(facts, m)` es puro, con el orden de RF30. `Wallet`, `bonus`, `bailout` y `POST /api/bets` usan esto mismo: esa es la garantía de RF32.

### 6. Bono, rescate y `wallet` (RF29 a RF32)
`bets/claims.rs` y `bets/wallet.rs`. Cada `POST`: cuotas (IP antes de la sesión y usuario después, como `profile::update`), `SET LOCAL lock_timeout = '1s'`, `SELECT … banned_at IS NULL FOR UPDATE`, y lo que dice cada RF. Sin fila, 401 con la cookie vencida, igual que `banned_writes`. `claim_bonus(tx, user)` y `claim_bailout(tx, user)` son funciones aparte: los tests 7 y 8 las llaman sin el limitador.

**`auth_events` propio.** `login::event` no escribe `bet_id`, y sumarle un parámetro tocaría a todos sus llamadores de auth por algo que no es de auth. `bets/audit.rs` tiene su propio `INSERT INTO auth_events`, como ya hacen `reauth.rs`, `link.rs`, `unlink.rs` y `admin.rs`: `grant` y `bailout` con `ledger_entry_id`, y `bet` con `bet_id`. `login.rs` no cambia. Si una prueba de fuente lista quién escribe `auth_events`, suma `bets/audit.rs`.

### 7. Apostar y `bets/mine` (RF25 a RF28, RF33)
`bets/api.rs`, en el orden exacto de la tabla de RF25, con el reloj leído **después** de la fila 2. Puntos que no se pueden perder:
- El cuerpo con `Result<Json<BetRequest>, JsonRejection>`: todo rechazo da 422 `bet.invalid_request`, sin texto de serde, también si supera `MAX_FORM_BYTES`.
- `client_key` en forma canónica. Otra forma da `bet.invalid_request`.
- Fila 3, sobre `UNIQUE (user_id, client_key)`: como el usuario está bloqueado, no hay carrera.
- `ranked_stake = NOT carencia`, con los hechos leídos bajo ese bloqueo.
- `bets.id` con `Uuid::new_v4`. Bet, `ledger::bet_stake` y `auth_events` en la misma transacción.
- `max_stake` por una sola función, que usa `MyFightBets` y las filas 7 a 10.
- La migración de `bet_stake` entra en este paso, antes de la ruta. Los fixtures de `bets_schema.rs` y `ledger.rs` con un `bet_stake` de otro importe pasan a su `stake`, sin quitarles lo que prueban.

### 8. Pozo (RF34, RF35, RF47)
`bets/pool.rs`: cada `POOL_INTERVAL` (2 s, en `Settings`, nunca en el entorno), **una** consulta: las peleas en `betting_open`, más las que estaban abiertas en la vuelta anterior, con sus sumas y conteos por lado, `rake_bp` y `closes_at`.
- La caché (`RwLock<BTreeMap<i32, PoolEntry>>`) guarda también las peleas sin apuestas: `quote` las necesita.
- Si un pozo cambió y tiene apuestas, va al hub. Una pelea que deja de estar abierta manda su último valor si cambió, y sale de la caché.
- Un fallo de la consulta registra `bets.pool_read_failed` con `error_kind` y conserva lo último. `quote` nunca da una ventana abierta pasado `closes_at`.

`hub.rs` guarda por pelea `(version, texto)`, serializado una vez. `next_pool(&mut seen)` da el de menor versión mayor que `seen`, así que un socket lento salta al último valor, sin cola. `session.rs` lo consulta en el mismo turno que `next_schedule`, y solo con `schedule` suscrito. `hub.rs` no gana `tracing` ni `sealed`.

`GET pool`: de la caché si la ventana está abierta, y si no, de la base, con `settlement`. `GET quote`: solo sesión y caché.

### 9. Reserva de capacidad (RF39)
En el paso 6 del handshake, tras tomar el permiso: si quedan menos de `reserve = WS_MAX_CONNECTIONS / 5`, y es anónimo, o la cuenta está en carencia, se suelta y da 503 `ws.capacity`. La carencia solo se consulta en ese caso, con 1 s de plazo, y si falla cuenta como carencia. `WsState` guarda `reserve`. Un socket abierto no se reevalúa.

### 10. Pruebas de carrera y de fugas
Tests 23, RF28, rescate contra liquidación a ambos lados del `COMMIT`, y `fights_no_leaks` con apuestas sembradas. Van al final, porque necesitan todas las rutas.

### 11. Medición de RF15
Ver su sección. **Es un criterio bloqueante del PR:** si no se cumple, se baja `BETS_MAX_PER_FIGHT` en este PR.

### 12. Documentación
- `apps/server/README.md`: la consulta de custodia de RF13 y qué hacer si no da limpia, salida del atasco de RF15, consulta de la métrica de RF40, activar el rake, el camino de `just dev`, y quitar lo de "Mientras no exista el reparto".
- `docs/architecture.md`: reparto, `pool_update`, reserva, y la fila nueva de prerrequisitos de TA-8 (≤ 10 s en el VPS).
- `CLAUDE.md`, glosario y cierres de Linear quedan en C, como dice la spec.

### 13. Cierre
`just check`, medición y camino de `just dev` pegados en el PR, PR. Con el OK del owner, marcar las notas de ADR-0008. Fusionar solo entonces.

## Medición de RF15
`apps/server/tests/bets_load.rs`. Un test de integración con 2000 apuestas corre en `just check`, y otro con 20 000 va con `#[ignore]`.

**Comando:** `cargo test --release -p server --test bets_load -- --ignored --nocapture`, y también en debug para comparar con A. Se reportan los dos, con la carga de la máquina, porque hoy hay otras sesiones.

**Montaje real:**
- El pool de `db::pool_options()`: 8 conexiones y 1 s de `acquire_timeout`.
- `attempt_timeout` por defecto y un runner real, con la transacción del reveal, para que incluya `write_decided`, el gancho y **el `COMMIT`**, donde corren los triggers diferidos de cuadre.
- `ledger_postings_rules` y las llaves de A activos. Nada apagado.

**Dos pozos, cada uno con 20 000 apuestas sembradas por SQL y sus stakes por `UNNEST`,** con cada `bet_stake` por el mismo importe que su `bets.stake`: la migración de `bet_stake` rechaza otro, y la medición corre con esa regla activa, porque suma su lectura de `bets` a cada stake sembrado:
1. **Reembolso:** 2000 cuentas con 10 apuestas de `m` al mismo lado, que da 20 000 asientos `refund`.
2. **Reparto:** los dos lados, con `r = 500`, sus `bet_payout`, y su `rake`.

**Contención durante cada liquidación, por HTTP contra la app:**
- apuestas concurrentes en otra pelea abierta;
- la relectura de RF36 (`wallet` y `bets/mine`) de esos 2000 usuarios, con su retraso de 0 a 3 s, a través de un número acotado de tareas;
- `quote`.

**Qué se reporta:**
- tiempo del intento, del primer `UPDATE` a `COMMIT`;
- memoria transitoria, comparada con los 1,3 MB estimados;
- `p50` y `p99` de una apuesta concurrente, y cuántas dan 503, porque RF28 acepta algunas (preocupación 8);
- si el pool se agotó, y si el backend de **un intento cancelado sigue vivo:** con un `attempt_timeout` corto, se lee `pg_stat_activity` tras el corte.

**Criterio:** ≤ 10 s en local. Si no, se baja `BETS_MAX_PER_FIGHT` y se repite. Referencia de A: una anulación de 20 000 reembolsos tardó 3,8 s en debug, sin contención y sin `bet_payout`.

**En TA-8,** el mismo test en el VPS es prerrequisito bloqueante. No se puede cerrar aquí, y queda anotado en `docs/architecture.md`.

## Camino de `just dev` con dos clientes HTTP
Es lo que pide "Qué se puede probar al terminar". Va en `apps/server/README.md` y su salida real se pega en el PR. Los comandos exactos se confirman al implementarlo: hoy están escritos leyendo `auth/start.rs` y los e2e, sin ejecutar.

**Preparación.** `apps/server/.env` desde el ejemplo, con una rejilla corta, que cumple RF11 (`60 + 10 + 95 + 10 ≤ 180`):
```
FIGHT_CADENCE_SECS=180
FIGHT_BETTING_WINDOW_SECS=60
FIGHT_PRE_LIVE_SECS=10
```
`just dev` ya compila con `dev-auth`, que da el proveedor `dev` y la cookie `ta_dev_session`.

**Dos cuentas,** cada una con su archivo de cookies, por el proxy de Vite:
```sh
BASE=http://localhost:5173
for who in alice bob; do
  curl -sS -L -o /dev/null -c $who.jar -b $who.jar -H "Origin: $BASE" \
    -d "dev_identity=$who&next=/es/" "$BASE/api/auth/dev/start"
done
```
**Pasos, con `curl -c/-b` y `Origin`:**
1. `GET /api/wallet` en las dos: saldo 500, bono ya reclamado por el login, y `bailout` en `not_eligible`.
2. `POST /api/bonus/claim`: 409 `bonus.already_claimed`.
3. `GET /api/fights/current`: el `id` de la pelea en `betting_open`.
4. `POST /api/bets` de alice al lado `a` y de bob al `b`, cada una con un `uuidgen` en minúsculas.
5. Repetir el `POST` de alice con la misma llave: 200 y sin moverse el saldo. Luego uno de alice al `b`: 409 `bet.side_locked`.
6. `GET /api/fights/{id}/quote?side=a&amount=100` y `GET /api/fights/{id}/pool`.
7. Esperar `settled` (hasta 3 min) y leer `GET /api/bets/mine?fight_id={id}` y `/api/wallet` en las dos: pago y pérdida, o reembolso si fue empate.
8. El rescate: quien perdió apuesta su saldo en la siguiente, lo pierde, y reclama `POST /api/bailout/claim`.

El paso 8 depende de quién gane, así que puede pedir más de una pelea. **Prueba reproducible:** el mismo recorrido corre sin navegador en `tests/bets_api.rs`, sobre vectores conocidos y con reloj manual.

## Tests existentes que cambian
No se debilita ninguno. Cada uno cambia porque la spec cambia lo que afirma.

| Test | Por qué | Queda |
|---|---|---|
| `bets_settle.rs`: `a_decided_fight_with_both_sides_has_no_split_yet`, `the_runner_halts_on_a_decided_fight_with_both_sides` | RF7 solo pide `Err` en A | Se reemplazan por el pago real, con su asiento y su conteo |
| `bets_settle.rs`: la propiedad de RF13 | Evitaba `KO` con dos lados | Lo incluye |
| `fight_void.rs::a_healthy_fight_is_refused` | Sigue igual: sigue `fight_healthy` | Sin cambios; solo cambia su comentario |
| `crates/protocol/src/me.rs::only_me_balance…` | D4 | Renombrado, sigue contando 1 |
| `apps/server/src/profile/me.rs`, `the_balance_guard…` | D4 | Se mueve a `CoinAmount::new`, con los mismos casos |
| `config.rs::daily_grant_is_an_integer_from_1_to_10000` y `the_env_example…` | `DAILY_GRANT = 1` no arranca con `m = 25` (preocupación 12) | Con `BET_MIN = 1` y `DAILY_GRANT = 2`, el borde de abajo, y un caso nuevo de que `1` falla por `BET_MIN`. `.env.example` en 500 |
| `fights_no_leaks.rs::request_of` | `match` exhaustivo | Una fila por ruta nueva, y `quote` y `bets/mine` con su consulta |
| `fights_fanout.rs` | Con la reserva, 2000 anónimos no entran (preocupación 14) | Sube `WS_MAX_CONNECTIONS` para medir lo mismo |
| `ws.rs`, con un tope pequeño (`max = 1`) | `1 / 5 = 0`: sin reserva | Sin cambios, y se comprueba |
| `banned_writes.rs` | RF31 | Suma las dos rutas de reclamo y `POST /api/bets` |
| `logs.rs` u otros con `ALLOWED_FIELDS` | Ningún campo nuevo | Sin cambios |

## Riesgos
- **Paso más riesgoso: el gancho con pagos, dentro de la transacción del reveal.** Un error de redondeo crea o destruye monedas, y un fallo detiene las peleas. → Función pura con propiedades, `check_split` antes de escribir, conservación por integración, y `CHECK` de RF6 como última barrera.
- **La migración de `bet_stake` reemplaza `ledger_posting_rules`,** una función vigilada que corre en cada asiento. Un error en su cuerpo rompe todos los movimientos, no solo las apuestas. → El PR muestra el `diff` del cuerpo, que solo suma la rama de `bet_stake`; los tests de A de cada tipo de asiento siguen pasando; y `GUARD_CHANGES_ALLOWED` suma solo esa línea.
- **El gancho no cabe en 30 s.** Reintenta para siempre y detiene todo, mientras `void` responde `fight_healthy`. → Topes, medición y README. La cifra del VPS no existe hasta TA-8 (preocupación 9).
- **El conteo de la fila 8 y la consulta del pozo crecen con las apuestas.** Un `count` por apuesta recorre hasta 20 000 entradas de índice, en una transacción con el usuario bloqueado, frente a los ≤ 50 ms del RNF. → Se mide en el paso 11. Si no cabe, sale de la caché del pozo, porque la spec acepta un exceso de pocas filas.
- **`FOR UPDATE` contra las FK del gancho.** Una apuesta puede agotar su 1 s y dar 503 durante una liquidación (preocupación 8). → Se mide y se reporta. Es aceptado por la spec, pero si fuera frecuente hay que decirlo.
- **Fuga por el pozo.** Un pozo calculado tras el cierre no depende de la semilla, pero un error podría mezclar datos. → RF27 y RF41 con apuestas: dos peleas con los mismos stakes y semillas distintas dan los mismos pozos.
- **Datos ajenos en lecturas privadas.** → Test 17 extendido a `bets/mine`, sin parámetro de usuario.
- **Logs con importes o ids.** → `bets_logs.rs`, en proceso y a `trace`, con canarios; un solo test por `test_layout.rs`.
- **Con `r = 0` nada ejercita el rake en producción.** → Los tests crean peleas con `rake_bp` distinto de 0, y la medición usa 500.
- **Cambiar `DAILY_GRANT` a 500.** Sube la economía respecto de hoy. → Es parte del gate del owner, y ninguna cuenta real existe (sin despliegue).
- **Reserva de capacidad.** Con pocas cuentas maduras, el tope útil es el 80 % (preocupación 14). → Se acepta, y `fanout` se ajusta.
- **Choque con TA-44.** `routes.rs`, `protocol`, `fights_no_leaks.rs`, `messages` y `store.rs`, donde TA-44 suma `settled_page` y este plan cambia `insert`. → Quien fusione después hace rebase y corre `just gen` y, si aplica, `just gen-wasm`.
- **Volver atrás.** Una imagen de A no paga: da `payout_unavailable` y detiene. Una anterior a A no entiende nada de esto. → El README lo prohíbe; va en el checklist de rollback.

## Alternativas descartadas
- **Calcular el reparto en SQL:** rápido, pero sin función pura no hay propiedades, y `quote` necesitaría otra copia.
- **Una consulta por apostador tras el `COMMIT`:** es lo que RF36 descarta.
- **`i64` con `checked_mul`:** `stake · N` cabe en `i128` siempre, sin ramas de error que probar.
- **Un `CHECK` para la carencia:** no se puede expresar, y se deriva, sin columna (RF27).
- **`pool_update` por `fight:{id}` o sondeo:** ver RF35.
- **Una migración para contadores por pelea:** ahorraría el `count`, a costa de una fila que actualizar por apuesta, en tablas solo de inserción.
- **`Me.balance` sin cambiar:** dejaría dos excepciones de 64 bits.

## Pruebas (cómo se demuestra que funciona)
- [ ] `just check` en verde, con la salida pegada en el PR.
- [ ] `Cargo.lock` sin cambios. `just gen` sin diferencias. Si cambia el lock, `just crate-age` antes de compilar.

**Funciones puras (`bets/parimutuel.rs`, `bets/probation.rs`)**
- [ ] RF10, propiedad sobre pozos aleatorios: `Σ payout + rake + residuo = T`; `rake ≤ L`; `payout_i ≥ stake_i`; `0 ≤ residuo < ganadoras`; el orden no cambia nada; dividir un stake nunca paga más (RF12).
- [ ] RF10, unitarias: 7 y 5 contra 9 con 500 pb; 500 contra 10; `r = 0`.
- [ ] RF11, ramas y 25(b), (d), (e) y (g) sobre la función, con `d < 0` y `L = 0` de entrada.
- [ ] `payout_bp`: lados vacíos y saturación en `u32::MAX`, sin pánico.
- [ ] RF27, los dos ejes por separado.

**Importes (`bets_schema.rs`, `bets_settle.rs`, `bets_api.rs`)**
- [ ] RF2: un `bet_stake` de otro importe (7 sobre una apuesta de 1), con otra cuenta en vez de `bet_escrow` o con el signo al revés da `TA008`. El `diff` del cuerpo de `ledger_posting_rules` solo suma esa rama.
- [ ] RF25: la apuesta y su `bet_stake` confirman juntos y con el mismo importe; si falla el asiento, no queda la apuesta.
- [ ] RF13: la consulta del README, leída del propio archivo, da limpio en una base sana y tras un reparto. Señala un `refund` de más por SQL directo y una apuesta sin `bet_stake`.
- [ ] RF13: `check_split` rechaza un reparto que no suma, y el gancho no escribe nada con ese error.

**Liquidación (`bets_settle.rs`)**
- [ ] RF7, sobre `app::serve`: una apuesta real termina pagada.
- [ ] RF9, fila `paid`. 500 contra 10 con 500 pb da rake 10, pago 500 y neto 0.
- [ ] RF10: cambiar la configuración entre el anuncio y la liquidación paga con el valor de la pelea, y la siguiente usa el nuevo.
- [ ] RF8, "reliquidar no cambia nada", con pagos: dos llamadas en la misma transacción y tras `COMMIT`, y un `kill` antes del `COMMIT`.
- [ ] RF13: la propiedad con finales `decided` de dos lados.
- [ ] RF14: cuenta baneada y sin baneo, mismos asientos; desvincular un proveedor tras apostar; topes `5 B` y `1 B`; un solo lado; rake 0 y rake en su tope; `BETS_MAX_PER_FIGHT` apuestas.
- [ ] RF14, tiempo: con `attempt_timeout` menor que el gancho reintenta, llega la detención, no queda `settled` parcial, y al subirlo confirma una vez.
- [ ] Test 25 completo: (a) lado perdedor solo con `false`, suma 0; (b) `L_c = L / 2` con `d` impar; (c) sale de carencia entre apuesta y liquidación, sigue `false`; (d) 500 contra 10 por la liquidación; (f) empate real, y cancelada por el gancho; (g) propiedad y `CHECK`.

**Apostar (`bets_api.rs`)**
- [ ] RF25, un caso por cada una de las 10 filas; sobre el límite, mal formado, `client_key` inválido, y `fight_id` fuera de rango; una rechazada no deja asiento ni fila.
- [ ] Test 23: dos `POST` concurrentes bajo el tope que juntos lo pasan: se acepta exactamente uno.
- [ ] Test 24: carencia no pasa de `1 B`, ni con varias apuestas; 7 días, 9 peleas y varios reembolsos siguen en carencia; diez apuestas en una pelea cuentan como una.
- [ ] RF5: una cuenta en carencia guarda `false`, y una fuera, `true`.
- [ ] RF25, reloj: usuario retenido mientras cruza `closes_at`, da `bet.window_closed` al soltarse.
- [ ] D1, fila 4: con la pelea retenida por otra transacción más allá del `lock_timeout`, da 409 `bet.window_closed`, no escribe apuesta, asiento ni `auth_events`, y registra `fights.db_unavailable`.
- [ ] Apuesta contra cierre (`bets_open_guard` lee sin bloqueo, así que todo recae en `lock_open_for_bet`): con una apuesta en curso y el cierre en paralelo, la que vio la ventana abierta confirma antes de que la pelea pase a `betting_closed`, y ninguna apuesta entra después del cierre.
- [ ] RF28: una transacción retiene al usuario 3 s durante la liquidación, y confirma sin detención; apuestas y liquidación en paralelo sin `40P01`.
- [ ] RF15: la apuesta 20 001 da `bet.pool_full`; la 11 da `bet.too_many_bets`.
- [ ] RF38: los tests de solapamiento y exhaustividad.

**Bono y rescate (`bets_claims.rs`, `banned_writes.rs`)**
- [ ] Test 7: 20 reclamos concurrentes dan exactamente un `grant`, e igual con `bailout`, sin el limitador.
- [ ] Test 8: una apuesta abierta que deja el saldo en 0 da `bailout.not_eligible`.
- [ ] Test 26: sin el bono de hoy, `bailout.grant_available`.
- [ ] RF30, rescate contra liquidación, a ambos lados del `COMMIT`: nunca se concede con un pago en camino.
- [ ] RF29: el primer login ya da el bono, así que el reclamo da 409.
- [ ] RF31: una cuenta baneada no reclama, y un fallo de la base da 503.

**Lecturas (`bets_wallet.rs`, `bets_pool.rs`)**
- [ ] RF32, una fila por valor de `bailout`.
- [ ] RF33: el usuario B no ve nada de A; `fight_id` no canónico da 404; `max_stake` da 0 en cada caso; y sin `ranked_profit` en la respuesta.
- [ ] RF34: forma exacta, y en una liquidada, su `settlement`. Sin identidades.
- [ ] RF35: dos sockets reciben lo mismo; uno lento salta a lo último; una consulta cada 2 s con o sin espectadores; sin apuestas, nada; un último valor tras el cierre; menos de 8 KiB.
- [ ] RF47: vectores de RF10; **una apuesta real con el mismo pozo cobra lo estimado**; un solo lado; ventana cerrada; `{id}` no canónico e inexistente; `amount = u32::MAX` sin pánico.
- [ ] RF37: cuotas y 503 con `no-store`.
- [ ] D6, un bucket por fila: agotar la cuota con `wallet` deja a `bets/mine` en 429, y agotarla con `bonus` deja a `bailout` en 429, para el mismo usuario y para la misma IP.
- [ ] RF27 y RF41 de 010 con apuestas sembradas, y RF28 de 010.

**Capacidad y configuración**
- [ ] Test 18 (`ws_capacity.rs`): con `WS_MAX_CONNECTIONS = 5`, cuatro sockets; el quinto anónimo y el de carencia dan 503; el de una cuenta madura pasa; al liberar uno, un anónimo sigue sin entrar si no baja del 80 %.
- [ ] RF39: si falla la consulta de carencia, cuenta como carencia.
- [ ] RF40: unitarias y arranque, con cada límite.
- [ ] Fuente: RF14, RF24 de 010, y que `settle.rs` no lee `banned_at` ni `users`.
- [ ] `bets_logs.rs`: ningún log con importes ni ids.

**Manual**
- [ ] Medición de RF15 pegada en el PR.
- [ ] Camino de `just dev` pegado en el PR.
- [ ] security-reviewer y qa-engineer revisan la fase.
- [ ] OK del owner a `B`, `m`, `r`, a las notas de ADR-0008 y a las preocupaciones 1 y 3 de 010, antes del merge.

## Estimación de CI
Sin cambios en workflows. El job `rust` suma varias decenas de tests de integración y el de 2000 apuestas, unos segundos cada uno, sin esperas largas. El de 20 000 va con `#[ignore]`, fuera de CI. El tiempo real se reporta en el PR. `check-compose` sigue pasando con `DAILY_GRANT: '500'`.

## Desviaciones durante la implementación
- **Medición de RF15, más simple que "Montaje real".** `tests/bets_load.rs` cronometra el gancho y su `COMMIT` sobre dos pozos de 20 000 apuestas sembradas por SQL (cada `bet_stake` por su `stake`, con los triggers activos), sin runner, sin apuestas concurrentes, sin la relectura de RF36 y sin `quote`. Resultado en release, con otras sesiones corriendo (carga media 11,5): reembolso 3,0 s y reparto 1,8 s. El criterio de ≤ 10 s se cumple con margen, y `BETS_MAX_PER_FIGHT` queda en 20 000. La medición con contención queda para el prerrequisito 12 de TA-8, en el VPS.
- **`ledger::rake` y `check_split`** como dice el plan. `SPLIT_MISMATCH` reemplaza a `PAYOUT_UNAVAILABLE`, que desaparece.
- **La consulta de custodia** compara con `open_stakes` de peleas que no están `settled`. El gancho solo, llamado fuera del runner, no cambia el estado de la pelea, así que un test que lo llama a mano ve `open_stakes` mayor que `escrow`; con el runner los dos cambian en la misma transacción.
- **Helpers de tests.** `user_with_session` y `user_with_dev_account` decían crear la cuenta del ledger y no la creaban; ahora la crean, como un login real. Los tests que arrancan la app corren en `multi_thread`, como pide `TestDb`.
- **El pozo en el hub** guarda el último valor de hasta 8 peleas (`MAX_POOLS`), y un socket recibe los de ese momento al suscribirse a `schedule`.
- **Para C:** el hub guarda el último pozo de hasta 8 peleas, también de peleas ya liquidadas, así que un socket nuevo recibe `pool_update` viejos. El panel de C debe ignorar los de una pelea que no está en su ventana.
- **`check-compose` exige `DAILY_GRANT` de 50 a 10 000:** el compose no pasa `BET_MIN`, y con el de por defecto el server no arranca por debajo de 50.
- **La consulta de custodia** vive en `apps/server/sql/custody.sql`; el README la enlaza y el test la ejecuta tal cual.
- **La barrera de `.outcome`** de `fights_source_guard.rs` no distingue SQL: las consultas de `bets/` no califican esa columna.

## Revisión
### architect — 2026-11-02 — REQUEST_CHANGES
- Verifiqué contra el código, sin compilar: el alcance de B en "Fases" y en los mapeos; las 10 desviaciones. D1: `lock_open_for_bet` falla cerrado, no escribe y registra `fights.db_unavailable`, así que el 409 es correcto, y RF25 ya lo dice. D6: el bucket por fila coincide con D7 y con `AUTH_IP`, y un 429 no rompe el SSR (RF44), así que también vale. D2, D3, D4, D5, D7, D8, D9 y D10 coinciden con `ledger::balance`, `ws.rs:102`, `Me.balance`, `NewFight`, `session::current`, `serve` y `ledger_entries_user_ck`. También que `DefaultBodyLimit` deja que `JsonRejection` dé 422 sobre el límite.
- Cambios que hice, porque me los encargaron: "Importes de los asientos", en lugar de la decisión abierta, con su migración, sus archivos, su paso y sus pruebas; el gate; y las notas de la spec (ver su "Revisión"). security-reviewer debe revisar esa decisión en el PR.
- Cambios pedidos al autor:
  1. **El paso 4 tiene el orden al revés.** Pone `pool_settlements` primero y `split` después, pero esa fila guarda `rake` y `residue`. RF8 pide que sea lo primero que se **escribe**: calcula `split` y `check_split` en memoria, y después inserta.
  2. **`login::event` no sirve tal cual para una apuesta.** RF25 pide `auth_events` con `bet_id`, y hoy esa función no escribe esa columna y `pub(crate)` no lo arregla. Di si suma un parámetro, con sus llamadores, o si `bets/` tiene el suyo.
  3. **Faltan pruebas que D1 y D6 hacen necesarias.** D1: la pelea retenida más allá del `lock_timeout` da 409, sin escribir. D6: gastar `wallet` agota `bets/mine`, y lo mismo con `bonus` y `bailout`. security-reviewer pide además una por el `POST`, apuesta contra cierre, porque `bets_open_guard` lee sin bloqueo y todo recae en `lock_open_for_bet`: la que vio la ventana abierta confirma antes del cierre, y ninguna entra después.
  4. **Texto viejo.** Quita "architect puede estar tocando RF1", tras D10: RF1 ya recoge lo de A. Actualiza "Punto de partida", porque la rama ya está en `649815e`. "Paso más riesgoso" y "Medición" suman la migración de `bet_stake`, y el sembrado masivo debe usar el mismo importe que `bets.stake`.
- Tras estos cambios, el plan vuelve a `in-review` para una lectura corta de esos puntos.

### backend-engineer — 2026-11-07 — Cambios pedidos por architect
1. Paso 4: primero `split` y `check_split` en memoria, y la primera escritura es `pool_settlements`, con `rake` y `residue`.
2. `bets/audit.rs` escribe su propio `auth_events` (con `ledger_entry_id` o `bet_id`), como `reauth`, `link`, `unlink` y `admin`. `login.rs` no cambia.
3. Pruebas nuevas: la fila 4 con la pelea retenida, apuesta contra cierre, y los buckets compartidos de D6.
4. Texto: punto de partida en `main` con A fusionada, sin la nota de RF1, el riesgo de reemplazar `ledger_posting_rules`, y el sembrado de la medición con `bet_stake = bets.stake`.

Vuelve a `in-review` para la lectura corta de architect.
