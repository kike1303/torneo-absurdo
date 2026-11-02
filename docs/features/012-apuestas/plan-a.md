---
id: "012"
title: "Apuestas, fase A: ledger y tablas, reembolso, anulación y secret_lost"
type: plan
status: approved
risk: high
author: backend-engineer
reviewers: [architect]
approvals:
  - {by: architect, date: 2026-11-02}
linear: TA-27
depends_on: ["008", "010"]
created: 2026-11-02
updated: 2026-11-02
---

# Plan: Apuestas, fase A (desde spec.md 2026-11-02)

> [intent](./intent.md) · [spec](./spec.md) · ADRs: [0002](../../decisions/0002-commit-reveal-semilla.md), [0005](../../decisions/0005-economia-sin-dinero-real.md), [0008](../../decisions/0008-autenticacion.md), [0012](../../decisions/0012-logs-y-cabeceras-de-seguridad.md) · 010: RF7, RF9, RF16, RF24, RF25, RF27, RF31, RF59

**Alcance.** RF1 a RF4 y RF6, la columna de RF5, RF7 a RF9 solo con reembolso, la inmutabilidad de `rake_bp` de RF10, RF13 y RF14 en su parte de reembolso, y RF16 a RF24. Al terminar no hay API de apuestas: los tests siembran filas de `bets` y las reembolsan. La parte de `apps/web` (paso 7) la revisa frontend-engineer.

**Queda para B:** `parimutuel`, `RankedProfit`, escribir `rake_bp` al anunciar, el asiento de `rake` y las pruebas de RF5, RF10 (función), RF11, RF15 y RF25 en adelante.

**Gate.** El PR se puede abrir, revisar y aprobar, pero **no se fusiona hasta que el owner acepte en el chat la enmienda de ADR-0002**, con el riesgo que ella describe a la vista. Este gate manda sobre la autonomía de merge. Hasta entonces, la enmienda sigue `proposed`, sin editarla. Con el OK, ese mismo PR la marca como aprobada (trabajo de cierre de la spec). `risk: high` pide además el OK del owner a la spec.

**Si el owner la rechaza,** se quitan los pasos 5, 6 y 7 (preocupación 5), con `--backup-checked` y la parte de RF24 del README sobre el backup y reabrir la aceptación. El orden de abajo deja `secret_lost` en commits y en una migración propios, y la negativa de un secreto ausente queda como `void.secret_missing`. `--previous-binary` y el resto de RF24 siguen.

## Desviaciones de la spec encontradas al leer el código
Ninguna cambia un requisito. Cada una se aplica como se indica, salvo que architect pida otra cosa.

| # | Spec | Código real | Qué hace este plan |
|---|---|---|---|
| D1 | RF16: "RF16 de 010 sigue recorriendo `cli::parse` y `admin --help`". RF23: ruta `/api/fights/void-stats` | `fights_source_guard.rs`, `neither_a_route_nor_a_command_…`, falla con esto: ninguna ruta puede contener `void`, `["fight", "void", …]` debe dar `Invalid` (con el comando nuevo, ese caso solo seguiría pasando porque `x` no es un UUID), y `cli.rs` y `admin.rs` no pueden contener `fight` | **La spec exige cambiar este test, y no lo dice.** Ver "Barreras de fuente que cambian". Cada regla queda más estrecha, nunca se borra |
| D2 | RF18: un `sealed::void_fight` escribe el final | `nothing_writes_a_voided_final` prohíbe `void_reason = $` y `void_reason = '` en todo `src` y cuenta exactamente 2 `void_reason` en `store.rs`. `void_stats.rs` necesita leer esa columna | Igual que D1 |
| D3 | RF18: `sealed::void_fight` "inserta en `admin_audit`" | Todo el SQL de `admin_audit` vive en `admin.rs`, y `sealed.rs` es el módulo que guarda la semilla | `sealed::void_fight` decide, escribe el final y llama al gancho. `admin.rs` inserta la auditoría **en la misma transacción**, y una prueba de fuente lo deja como único llamador. La atomicidad de RF18 no cambia |
| D4 | RF17 filas 9 y 10: hash y slugs primero, sobre los bytes | `FightSnapshot::from_row` decodifica **antes** de comparar el hash, así que un spec corrupto daría `Decode` (fila 10) en vez de `snapshot_invalid` (fila 9) | Función nueva `snapshot::stored_bytes_error(row)`, sin tipos del motor. `from_row` no cambia, porque el runner depende de su orden |
| D5 | RF17 fila 13 nombra `EventLimit`, `encode_failed` y pánico | `ComputeError::Engine(code)` lleva cualquier `SpecError`. Con `validate` ya pasado, otro código no es alcanzable, y la spec no le da fila | `Engine("event_limit")`, `Encode` y pánico van a las filas 13 y 14. Cualquier otro `Engine(_)` da `void.binary_problem`: la negativa por defecto de RF59 |
| D6 | "Datos": `CHECK` y `UNIQUE` sin nombre | Las migraciones nombran cada restricción que puede fallar (ADR-0012 A9) | Todas llevan nombre fijo (`bets_side_ck`, `bets_client_key_uq`…) |
| D7 | "Componentes": `VoidReason::parse` en `state.rs` | Está en `crates/protocol/src/fights.rs`, y `state.rs` solo la llama | Se edita en `protocol`, y se suma `as_str`. `state.rs` no cambia |
| D8 | "Componentes": migraciones `…_void_secret_lost` y `…_bets` | Si la de `secret_lost` también lleva `rake_bp`, rechazar la enmienda obliga a reescribirla | Primero `_bets`, con `rake_bp`, y después `_void_secret_lost`. `fights_guard` se reemplaza dos veces |
| D9 | RF7: "integración sobre `app::serve`" | Ningún test llama a `app::serve` en proceso, `FightSettings::default()` da `NoBets`, y los tests del ciclo usan `runner::run` | `serve` fija `hook: BetsHook` sin condición y `default()` sigue en `NoBets`. La prueba en proceso va en el paso 4 |
| D10 | RF22: "muestra el contador". No dice de dónde lo lee la web | `/api/fights/current` sale de memoria a propósito: el SSR de la home no toca la base | El SSR lee `void-stats` solo si `board.last` es `secret_lost`. Si el final llega por socket, el cliente lo lee una vez, con 0 a 3 s de espera. Si falla, no hay línea de contador y el resto del texto sigue |
| D11 | RF2: una cuenta de usuario pertenece a `entry.user_id` | `created_xact_from_the_client_is_overwritten` y `an_entry_built_inside_a_savepoint_commits` (`tests/ledger.rs`) debitan la cuenta de `b` en un `grant` de `a`: darían `TA008` | Se adaptan en el commit del paso 1, sin quitarles lo que prueban. Ver abajo |

**Los dos tests de D11.** Son los únicos con movimientos de otro usuario que llegan a la regla. Sigue la misma pregunta, con los dos movimientos válidos en un `grant`:
- `created_xact_from_the_client_is_overwritten`: los movimientos pasan a `(account_a, 3)` y `(ISSUANCE_ACCOUNT, -3)`. Sigue probando que el `created_xact` del cliente se reemplaza y que la entrada confirma.
- `an_entry_built_inside_a_savepoint_commits`: `(account_a, +7)` dentro del savepoint e `(ISSUANCE_ACCOUNT, -7)` después. Sigue probando que la entrada sigue abierta tras el savepoint, y el saldo final de `a` sigue en 7.
- En los dos desaparece el usuario `b`. `a_committed_entry_takes_no_new_postings` no cambia: también usa cuentas ajenas, pero `TA003` corre antes por el orden de los triggers, y eso es lo que fija.

**Resuelto por architect en la revisión:** RF2 suma "`bet_escrow` solo se debita en `bet_payout`, `refund` y `rake`", con una línea más en el trigger `ledger_postings_rules`. Sin ella, un asiento `grant` podría sacar monedas de la custodia con SQL directo. security-reviewer la revisa en el PR.

## Archivos que cambian

| Archivo | Cambio | Paso |
|---|---|---|
| `apps/server/migrations/20261102000000_bets.sql` | Nuevo: RF1, RF3, RF4, RF6, `rake_bp` y `fights_guard`, trigger de RF2, `admin_audit_*_ck`, `auth_events` | 1 |
| `apps/server/migrations/20261102000100_void_secret_lost.sql` | Nuevo: `fights_void_reason_ck`, `fights_reveal_ck`, función del plazo, `fights_guard`, índice parcial (RF19, RF20, RF23) | 5 |
| `apps/server/src/ledger/mod.rs` | `ESCROW_ACCOUNT`, `RAKE_SINK_ACCOUNT`, `bet_stake`, `bailout`, `settle_batch` | 2 |
| `apps/server/src/db.rs` | `TA007` y `TA008` en `error_kind` | 1 |
| `apps/server/src/bets/{mod,settle}.rs`, `lib.rs` | Nuevo: `BetsHook` y los totales de un reembolso | 3 |
| `apps/server/src/app.rs` | `serve` construye `BetsHook` | 4 |
| `apps/server/src/fights/hook.rs` | Solo el comentario de `NoBets` | 4 |
| `apps/server/src/fights/snapshot.rs` | `stored_bytes_error`, `version_available` | 4 |
| `apps/server/src/fights/store.rs` | `lock_for_void` (`FOR UPDATE`, da la fila y su `ClosedWindow`), `write_voided`, `secret_lost_due` | 4, 5 |
| `apps/server/src/fights/sealed.rs` | `VoidCompute`, `RealCompute`, `VoidRefusal`, `void_fight` | 4, 5 |
| `apps/server/src/fights/mod.rs` | `void_stats`, `SECRET_LOST_MARGIN_MS` | 5, 6 |
| `apps/server/src/fights/void_stats.rs` | Nuevo: RF23 | 6 |
| `apps/server/src/{cli.rs,admin.rs}` | `Command::FightVoid`, `VoidCommand`, `run_fight_void`, `HELP` | 4, 5 |
| `apps/server/src/http/routes.rs` | `Handler::FightsVoidStats` y su entrada, **posible choque con TA-44** | 6 |
| `crates/protocol/src/{fights.rs,lib.rs}` | `VoidReason::SecretLost`, `as_str`, `VoidStats`. `just gen` | 5, 6 |
| `packages/protocol-ts/` | Generado, nunca a mano | 5, 6 |
| `apps/server/tests/support/{bets.rs,mod.rs}` | Nuevo: `seed_bet` (siembra con `ledger::bet_stake`), foto de lo escrito | 2 |
| `apps/server/tests/support/fights.rs` | `time_limit_ms` y `max_duration_ms` en `RawFight`, fixtures con horas pasadas | 4, 5 |
| `apps/server/tests/support/public_verifier.rs` | RF21, ver "Choques" | 5 |
| `apps/server/tests/fights_source_guard.rs`, `admin_cli.rs`, `admin_audit.rs`, `fights_schema.rs`, `fights_public_verify.rs`, `ledger.rs`, `routes.rs` | Casos nuevos. En `fights_source_guard.rs`, los cambios de abajo | 1 a 6 |
| `apps/server/tests/{bets_schema,bets_settle,fight_void,fight_void_logs,fight_void_compute_logs,void_stats}.rs` | Nuevos | 1 a 6 |
| `apps/web/src/lib/features/fight/live/view-state.ts` y su test | `ResultKind` suma `voided_unverifiable` | 7 |
| `apps/web/src/lib/features/fight/live/void-stats.ts` y su test | Nuevo: lectura del cliente, una vez por pelea | 7 |
| `apps/web/src/lib/components/fight/{FightResultCard,FightHome}.svelte` | Texto propio y prop opcional `voidStats`, **posible choque con TA-44** | 7 |
| `apps/web/src/lib/server/api/reads.ts`, `routes/+page.server.ts` | `getVoidStats`, solo con `last` en `secret_lost` | 7 |
| `apps/web/messages/{en,es}.json`, `routes/dev/components/_sections/FightSection.svelte`, `e2e/home.spec.ts` | Dos claves, catálogo y e2e | 7 |
| `apps/server/README.md`, `docs/architecture.md` | Ver paso 8 | 8 |
| `docs/decisions/0002-commit-reveal-semilla.md` | Marcar la enmienda, **solo tras el OK del owner** | 9 |

Sin cambios: `fights/api.rs`, `fights/runner.rs`, `fights/hub.rs`, `crates/engine`, `crates/content`, `crates/sim`, `justfile` y `Cargo.lock`.

## Orden de trabajo
Cada paso deja `just check` en verde y es un commit `feat(scope): … (TA-27)`.

### 1. Migración `_bets` y barreras de datos
1. Reemplaza `ledger_accounts_kind_ck` y crea las cuentas con ids `…0002` (`bet_escrow`) y `…0003` (`rake_sink`).
2. `ledger_entries`: reemplaza `kind_ck` y `user_ck`, y suma `fight_id` y `bet_id`, con dos `CHECK` con nombre (`ledger_entries_bet_refs_ck`, `ledger_entries_rake_fight_ck`).
3. Crea `bets`, `bet_settlements` y `pool_settlements`, con sus índices, `ledger_immutable` y su trigger de `TRUNCATE`. La FK de `ledger_entries.bet_id` va después de `bets`.
4. Trigger `bets_open_guard` (`TA007`).
5. Trigger hermano `ledger_postings_rules` (`TA008`), con las cinco reglas de RF2: `rake_sink` nunca se debita; `issuance` solo se debita en `grant` y `bailout`; solo `bet_stake` acredita `bet_escrow`; `bet_escrow` solo se debita en `bet_payout`, `refund` y `rake`; y una cuenta de usuario pertenece a `entry.user_id`, con `IS DISTINCT FROM` para un `user_id` nulo. **El nombre importa:** los `BEFORE` corren por orden alfabético, así que `ledger_postings_open_entry` corre antes y un asiento cerrado sigue dando `TA003`. Este commit adapta los dos tests de D11.
6. Índices únicos de RF3.
7. `fights.rake_bp` y `CREATE OR REPLACE fights_guard`, idéntico salvo la columna en las dos listas de RF4.
8. `admin_audit_action_ck` suma `fight_void`, y `admin_audit_actor_ck` suma `cli_attributed`. `auth_events_kind_ck` suma `bailout` y `bet`, con `bet_id` nulo y FK a `bets`.

Se prueba solo con SQL directo, en `bets_schema.rs`.

### 2. `ledger/`
`bet_stake(tx, bet_id, fight_id, user_id, amount)`, `bailout(tx, user_id, amount) -> Option<Uuid>` y `settle_batch(tx, fight_id, &[SettleEntry])`, con `UNNEST` en lotes de 1000. `SettleEntry` tiene clase `Refund` o `Payout`, con el mismo SQL. El asiento `rake` va en B. Con esto, `support/bets.rs` siembra por la misma ruta que usará la API.

### 3. `bets/settle.rs` (**paralelizable** con la parte de `snapshot.rs` del paso 4)
`BetsHook`, sin configuración:
1. Lee `id, user_id, side, stake` de `bets`, ordenado por `id`.
2. Sin filas, sale con `Ok` y sin escribir.
3. Decide con la tabla de RF9. Un `decided` con ganador y los dos lados da `Err(SettlementError { kind: "payout_unavailable" })` **antes de escribir** (RF7).
4. `INSERT INTO pool_settlements … ON CONFLICT DO NOTHING RETURNING`. Sin fila, sale con `Ok`.
5. `ledger::settle_batch` y `bet_settlements` por lotes, con `refunded`, `payout = stake`, `net = 0` y `ranked_profit = 0`, y `rake_bp = rake = residue = 0`.

`refund_totals(conn, fight_id)` da `(bets, coins)` para la auditoría, así que `SettlementHook` no cambia. **Atención:** la búsqueda de RF24 rechaza `.outcome` y `::outcome` fuera de `sealed.rs`, también dentro de un literal SQL. En Rust va `pool_result`, y en SQL, `outcome` sin calificar.

### 4. `server fight void`, sin `secret_lost`
- **`snapshot.rs`:** `version_available(row)`, y `stored_bytes_error(row) -> Option<SnapshotError>`, que da `Hash` o `Slug`. El slug se lee con `serde_json::Value`. Si no es JSON, o no trae `slug` como texto, no devuelve error y sigue a la fila 10: solo un slug presente y distinto prueba un snapshot inválido.
- **`sealed.rs`:**
  ```rust
  pub trait VoidCompute: Send + Sync {
      fn compute(&self, seed: SealedSeed, snapshot: &FightSnapshot, window: &ClosedWindow)
          -> Result<(), (SealedSeed, ComputeError)>;
  }
  pub async fn void_fight(
      conn: &mut PgConnection,
      request: &VoidRequest, // id, previous_binary; backup_checked en el paso 5
      compute: Arc<dyn VoidCompute>,
      hook: &dyn SettlementHook,
  ) -> Result<VoidReason, VoidError>; // Refused(VoidRefusal), Settlement(_), Db(_)
  ```
  `RealCompute` llama a `SealedLog::compute` y descarta el log. Va en `sealed.rs`, porque `SealedLog` solo se puede nombrar allí, en `release.rs` y en `runner.rs`. `compute` corre con `spawn_blocking`, como en el runner, porque en el perfil de release un pánico sigue siendo capturable (`panic` no es `abort`, y el motor tiene `overflow-checks`). Tras un pánico la semilla se pierde con la tarea, y se vuelve a leer con `SealedSeed::load` para revelarla.
- **Evaluación en el orden de RF17:** `lock_for_void` (filas 2 y 3), versión (4), `SealedSeed::load` (5; en este paso, `Missing` da `void.secret_missing`), `stored_bytes_error` (9), `from_row` (10) y `compute` (11 a 14).
- **Al aplicar:** `store::write_voided` (`settled_at = now()`), `DELETE` del secreto, y el gancho con `SettlingFight`.
- **`admin.rs`:**
  - `VoidCommand::parse`: `fight void <id> --by <uuid> [--previous-binary <tag>]`. `--backup-checked` se suma en el paso 5. Las banderas van en cualquier orden y sin repetir, y cualquier otra da `Invalid`: la razón nunca se elige. El tag es `vX.Y.Z` o `sha-` más 7 caracteres hexadecimales. El id es un decimal canónico, con su propio parser, porque `fights/api.rs` no se toca. `Debug` oculta el id y el usuario (A8).
  - `run_fight_void(options, command, compute, hook)`: una conexión, `BEGIN`, `lock_timeout = 5 s`, fila 1 sobre `users`, `sealed::void_fight`, `refund_totals`, `INSERT INTO admin_audit`, `COMMIT`, línea en stdout y evento. Las negativas hacen `ROLLBACK` y registran `fight.void_refused` con `error_kind` igual al código.
  - **Un fallo que no es negativa** (`VoidError::Db`, `VoidError::Settlement`, conexión, `lock_timeout` o `COMMIT`): `ROLLBACK` si la conexión sigue viva, exit 1, sin línea en stdout y sin fila de RF17. Registra un evento propio, `fight.void_failed`, con `error_kind` de `db::error_kind` o de `SettlementError.kind`. Nunca se confunde con `void.*`: quien lo lee sabe que no se decidió nada y puede reintentar.
- **`cli.rs`:** `Command::FightVoid`, con `BetsHook` y `RealCompute`. Exit 0, 1 y 2. `CliError` suma `FightVoid` (negativa o fallo: exit 1). `AdminConfig` se reutiliza, así que solo lee `DATABASE_URL` y `RUST_LOG`.
- **`app.rs`:** `serve` fija `hook: Arc::new(BetsHook)`, también en la rama `dev-fights`.

### 5. `secret_lost` (commit propio; lo primero que sale si se rechaza la enmienda)
**Es lo que pide un commit aislado en la revisión.**
1. `VoidReason::SecretLost` y `as_str`, y `just gen`. Sin esto `summary()` da `None`, y una `secret_lost` rompería el tablero y `GET /api/fights/{id}`.
2. Migración:
   ```sql
   CREATE FUNCTION fight_secret_lost_deadline(started timestamptz, live_at timestamptz, max_ms int)
   RETURNS timestamptz LANGUAGE sql STABLE AS $$
     SELECT COALESCE(started, live_at) + (max_ms + 5000 + 120000) * interval '1 millisecond'
   $$;
   ```
   `fights_guard`, sobre el cuerpo del paso 1 (con `rake_bp`). En `live>settled` y `betting_closed>settled` **solo se condicionan las dos líneas del reveal**, sin una rama aparte:
   ```sql
   IF NEW.settled_at IS NULL
      OR NEW.cancel_reason IS NOT NULL
      OR NEW.live_started_at IS DISTINCT FROM OLD.live_started_at
      OR CASE WHEN NEW.void_reason IS NOT DISTINCT FROM 'secret_lost' THEN
             NEW.revealed_seed IS NOT NULL
             OR EXISTS (SELECT 1 FROM fight_secrets WHERE fight_id = NEW.id)
             OR now() < fight_secret_lost_deadline(OLD.live_started_at, NEW.live_at, NEW.max_duration_ms)
         ELSE
             NEW.revealed_seed IS NULL
             OR sha256(NEW.revealed_seed) IS DISTINCT FROM NEW.seed_commit
         END
   THEN
       RAISE EXCEPTION USING ERRCODE = 'TA006', MESSAGE = 'fight guard';
   END IF;
   ```
   `settled_at`, `cancel_reason`, `live_started_at` y el bloque siguiente (`end_reason`, `winner` y `log` nulos en una `voided`) rigen igual en `secret_lost`, y esas líneas no se tocan. `fights_reveal_ck` suma `OR (void_reason = 'secret_lost' AND revealed_seed IS NULL)`.
3. `--backup-checked` en `VoidCommand`, `VoidRequest` y el `payload` de `admin_audit`.
4. `store::secret_lost_due` usa esa función con el `now()` de la transacción. **El comando y el guard leen la misma hora, y no pueden discrepar.** Esperar el bloqueo atrasa `now()`, y por eso se niega de más, nunca de menos.
5. Filas 6 a 8 en `void_fight`. Tras un `secret_lost`, stdout imprime la línea fija de RF24.
6. `public_verifier.rs`: RF21.

### 6. Contador (**paralelizable** con el paso 7 tras `just gen`)
`VoidStats`, y `void_stats.rs` con `count(*)` y `FILTER`, sobre el índice parcial `fights_voided_idx ... WHERE void_reason IS NOT NULL`, y con el `lock_timeout` y el plazo de `by_id_bounded`. Un conteo fuera de `u32` da 503 `fight.unavailable`. La entrada `Effect::Read` va junto a `FightsCurrent`, antes de `{id}`, con las capas de `FightDetail`: `FIGHT_READ_IP`, y `no-store` por fuera. `fights_no_leaks` y `routes` la recogen solos.

### 7. Web (revisión de frontend-engineer)
- `resultOfFinal`: `voided` con `reason === 'secret_lost'` da `kind: 'voided_unverifiable'`. `finalVisible` no cambia, porque una `voided` se muestra al instante.
- `FightResultCard`: nuevo caso del `switch` y prop opcional `voidStats`. Claves propuestas, con texto a revisar:
  - `fight_result_voided_unverifiable`: "Anulada: se devolvió lo apostado y no se puede verificar" / "Voided: stakes were returned and it cannot be verified".
  - `fight_result_void_count`: "Peleas anuladas: {voided}. Sin poder verificar: {secret_lost}" / "Voided fights: {voided}. Unverifiable: {secret_lost}".
- La tarjeta de la home no muestra semilla ni comprobación. La lectura de D10 valida la forma de la respuesta.
- `check-i18n` y `check-tokens`. El catálogo de `/dev` suma el estado.

### 8. Documentación de la fase
- `apps/server/README.md`: uso de `server fight void`, tabla de códigos, procedimiento de RF24 (issue en Linear y nueva decisión del owner), la prohibición de RF7 (no volver a un binario con `NoBets` con apuestas sin liquidar) y qué significa `--previous-binary` y `--backup-checked`.
- `docs/architecture.md`: tablas y cuentas nuevas, la CLI de anulación y el contador.
- `server::admin::HELP` documenta el comando desde el paso 4.
- `CLAUDE.md`, el glosario, `REVIEW.md` y las notas en la spec de 010 quedan en el cierre de la fase C, como dice la spec.

### 9. Cierre
`just check`, `just e2e`, capturas y PR. Con el OK del owner, marcar la enmienda de ADR-0002. Fusionar solo entonces.

## Barreras de fuente que cambian (D1, D2)
"Arregla el código, no el test" sigue valiendo: estos cambios los pide la spec, y por eso van explícitos y aparte en la revisión.

| Test | Hoy | Queda |
|---|---|---|
| Palabras en rutas | Ninguna ruta con `cancel`, `void`, `seed` o `secret` | Igual, con una sola excepción exacta: `GET /api/fights/void-stats`, con `Effect::Read` |
| Comandos | `fight void` da `Invalid` | `fight void` solo con su gramática exacta. Siguen en `Invalid`: `fight`, `fight cancel 1`, `fight seed 1`, `--help`, `fight void 1 --by x`, `fight void 1` sin `--by`, y `fight void 1 --by <uuid> --reason x`: la razón nunca se elige |
| `fight` en `cli.rs` y `admin.rs` | Prohibida | Fuera de `#[cfg(test)]`, ningún `fight cancel` ni `fight seed`, y `HELP` no nombra semilla ni secreto |
| `void_reason = EXCLUDED` en `src` | Prohibido | Sin cambios: sigue prohibido en todo `src` |
| `void_reason = $` en `src` | Prohibido | Aparece exactamente una vez, en `store.rs`, dentro de `write_voided` |
| `void_reason = '` en `src` | Prohibido | Solo el literal exacto `void_reason = 'secret_lost'`, una vez, en `void_stats.rs`, una lectura: el archivo no tiene `UPDATE`. Cualquier otro literal, en cualquier archivo, sigue fallando |
| 2 `void_reason` en `store.rs` | Cuenta exacta | Se reemplaza por "una sola escritura": ese número también cambiaría con `settled_page` de TA-44 |
| El runner no nombra `voided` ni `VoidReason` | Sin cambios | Sin cambios |
| **Nuevos** | | `store::write_voided` solo desde `sealed.rs`. `sealed::void_fight` solo desde `admin.rs`. `admin::run_fight_void` solo desde `cli.rs`, y `RealCompute` solo se nombra en `fights/sealed.rs` y `cli.rs`: sin esto, un handler podría llamarlo sin una ruta con `void`, y RF16 dice que no hay endpoint. `serve` nombra `BetsHook`. En `src`, `NoBets` solo en `hook.rs` y en el `Default` de `fights/mod.rs`. `bets/settle.rs` no contiene `banned_at` ni `users` |

Sigue sin cambios: `fight_secrets` y `revealed_seed` no salen de su módulo, `sealed.rs` no tiene `tracing`, y `no_migration_switches_a_guard_off` no necesita línea nueva, porque ninguna migración nombra esos guards.

## Choques con la sesión de repeticiones (`ta-44-repeticiones`)
Comprobé su spec en esa rama, que todavía solo tiene documentos.

- **`http/routes.rs`:** una entrada y un `Handler` por lado, sin conflicto de rutas (`/api/fights`, `/api/fights/void-stats`, `/api/fights/{id}`).
- **`FightDetail`:** su spec no lo cambia, y solo suma `FightsPage`. Quien fusione después revisa que una `secret_lost` siga dando `seed: null`. Lo fija un test de `fights_public_verify.rs`.
- **`public_verifier::verify`:** su RF14 lo usa como `Ok` o `Err`. Por eso queda `Result<Verified, String>`, con `Verified` en `Verified`, `CancelledUnverifiable` y `UnverifiableSecretLost`, y `Err` como `invalid`. Los usos actuales de `.unwrap()` pasan a comparar el resultado exacto, para que una `secret_lost` no pase por una `decided`.
- **`crates/protocol`:** si TA-44 fusiona primero, todo cambio de `protocol` falla `_check-wasm-artifact`, y este PR debe correr **`just gen-wasm`** además de `just gen`.
- **`verify.ts`:** su `switch` deja de compilar con `SecretLost`, a propósito. Si TA-44 fusiona primero, este PR le da su estado, sin bloque de comprobación ni semilla en `/fights/{id}` (RF22).
- **`FightResultCard`:** los dos suman una prop opcional, y también comparten `messages`, `reads.ts`, `store.rs`, `fights_no_leaks.rs`, `README` y `architecture.md`. Sus cambios son pequeños, y quien fusione después hace rebase.

## Riesgos
- **Paso más riesgoso: reemplazar `fights_guard` dos veces.** Un error abre una transición ilegal. → `fights_schema.rs` corre sin cambiar ningún caso existente, y el PR muestra el `diff` de cada cuerpo contra el anterior.
- **`secret_lost` es irrefutable, y el abuso está descrito en el ADR.** Las barreras frenan errores, no a quien tiene la credencial. → Gate del owner, las cinco condiciones de la enmienda como tests, contador y auditoría.
- **Anular una pelea sana por un mal orden de negativas.** → Una prueba por fila, y combinaciones que fijan el orden: versión y secreto ausentes, secreto distinto con plazo vencido, y hash malo con spec no decodificable.
- **Fuga de la semilla por una ruta nueva.** → `sealed.rs` sigue sin `tracing`, `SealedSeed` sigue opaca, y las pruebas de canarios cubren stdout y stderr. `_check-pg-log` sigue corriendo.
- **`VoidCompute` es `pub` para inyectar fallos.** Un llamador podría pasar una falsa y anular una pelea sana. → Solo `cli.rs` construye `RealCompute` y llama a `run_fight_void`, lo fijan dos pruebas de fuente, y la CLI ya exige la credencial (preocupación 1 de 010).
- **Pelea con dos lados en fase A.** Daría `Err` y detención. → Sin API no hay camino, y solo se daría volviendo de B a A: el README lo prohíbe y `void` responde `fight_healthy`.
- **Restaurar un backup durante un `secret_lost`.** Un `INSERT` en `fight_secrets` que ya leyó la fila sin liquidar puede confirmar tras el `COMMIT`, y dejar un secreto en una pelea `secret_lost`. No cambia ningún final ni expone datos. → El procedimiento del README pide no correr ambos a la vez. No lo cierro con más barreras.
- **Tests con hora real.** `T0` (`1.8e12`) está en el futuro, y el guard usa `now()` de la base. → Las fixtures de `secret_lost` usan horas pasadas. Los límites de 1 ms van en una sola transacción, donde `now()` no avanza, sin esperas ni reintentos.
- **Trigger por movimiento (RF2).** Suma una lectura por fila. → Su costo se mide en B (RF15). En A solo hay lotes pequeños.
- **Dependencias.** No se agrega ninguna: `serde_json`, `proptest` y los arreglos de `sqlx` ya están. → `git diff --quiet Cargo.lock` antes de compilar. Si cambiara, `just crate-age` primero.

## Alternativas descartadas
- **Un enum de cuatro variantes para `verify`:** obliga a tocar cada `.is_err()`, y choca con RF14 de TA-44 sin dar más seguridad.
- **Cambiar `BetsHook` en `FightSettings::default()`:** cambiaría todos los tests del ciclo y dejaría `NoBets` sin uso, contra RF7.
- **`SettlementHook` que devuelve totales:** rompe los ganchos de prueba existentes, y `refund_totals` lo resuelve con una lectura en la misma transacción.
- **Comprobar el plazo en Rust:** dos relojes, y el comando podría discrepar del guard.
- **El módulo entero en `admin.rs`:** las barreras de RF24 no lo admiten, porque `revealed_seed` y `SealedLog` no pueden salir de `fights/`.
- **Una sola migración:** rechazar la enmienda obligaría a editarla.
- **Reescribir `void_reason = '…'` para esquivar la búsqueda en `void_stats.rs`:** cumpliría el test sin cumplir su intención. La excepción explícita se ve en la revisión.
- **Leer el contador en cada SSR:** agrega una consulta a una ruta que se diseñó sin base (D10).

## Pruebas (cómo se demuestra que funciona)
- [ ] `just check` en verde, con la salida pegada en el PR. `just e2e` en verde.
- [ ] `Cargo.lock` sin cambios. `just gen` sin diferencias.

**Datos y ledger (`bets_schema.rs`, `ledger.rs`)**
- [ ] RF1: cada tipo se acepta; uno desconocido, un asiento de apuesta sin `bet_id` o sin `fight_id` y un `rake` sin `fight_id` fallan; un test ata los ids a la migración.
- [ ] RF2: un rechazo con `TA008` por regla (debitar `rake_sink`; debitar `issuance` en `refund`; acreditar `bet_escrow` en `refund`; debitar `bet_escrow` en `grant`; cuenta de otro usuario) y los flujos reales aceptados. Las pruebas de fuente del ledger siguen pasando. Un asiento cerrado sigue dando `TA003`.
- [ ] D11: los dos tests adaptados siguen probando `created_xact` y el savepoint, y `a_committed_entry_takes_no_new_postings` sigue dando `TA003` sin cambios.
- [ ] RF3: un segundo asiento de cada clase falla. RF4: `TA001` en `UPDATE`, `DELETE` y `TRUNCATE`, y `TA007` en cada estado que no es `betting_open`.
- [ ] RF6: cada `CHECK`, y que la tabla es solo inserción.
- [ ] RF10: `UPDATE fights SET rake_bp` da `TA006` en `scheduled`, `betting_open`, `betting_closed` y `live`, con **todas** las migraciones aplicadas.
- [ ] `admin_audit`: `cli_attributed` sin `admin_id` falla por `admin_audit_cli_ck`, y `('cli', admin)` sigue fallando.

**Liquidación (`bets_settle.rs`)**
- [ ] RF9, una prueba por fila de reembolso, y 25(f): empate real con el vector `DRAW`, un lado vacío con `KO`, y `cancelled` llamando al gancho: asientos exactos, `ranked_profit = 0` y sin contar como apuesta liquidada.
- [ ] RF7: `decided` con dos lados da `Err`, sin `settled` ni asientos; con `settle_halt_ms` corto llega la detención; y `void` responde `void.fight_healthy`.
- [ ] RF7: `app::serve` en proceso, reloj manual y sin gancho de prueba. Un pozo de un lado termina reembolsado, algo que `NoBets` no haría. Más la prueba de fuente.
- [ ] RF8: dos llamadas, en la misma transacción y tras `COMMIT`, dejan lo mismo. Una pelea sin apuestas no escribe. Un gancho que muere antes del `COMMIT` no deja nada, y el reintento lo deja una vez.
- [ ] RF13: propiedad con `proptest`, tras secuencias de apuestas y finales de reembolso: la suma de movimientos es 0 y `saldo(bet_escrow) = Σ stakes sin liquidar`. B le suma el reparto.
- [ ] RF14, parte de reembolso: una cuenta baneada tras apostar recibe los mismos asientos, y la prueba de fuente.

**Anulación (`fight_void.rs`, `admin_cli.rs`, unitarias de `cli::parse`)**
- [ ] RF16: formas válidas e inválidas, exit 0, 1 y 2, `Debug` sin datos, y `--help` que documenta el comando sin conectar.
- [ ] RF17, una prueba por fila, con `support::bets` comparando antes y después `fights`, `fight_secrets`, ledger, `bets`, liquidaciones y `admin_audit`:
  - 1: `--by` inexistente y sin rol.
  - 2 y 3: `scheduled`, `betting_open` y `settled`.
  - 4 a 8: versión, secreto distinto, plazo y `--backup-checked`.
  - 9: hash y slug.
  - 10: no decodifica, no reserializa, `max_hp_*`, `time_limit_ms`, `max_duration_ms` y `validate`.
  - 11: sana. 12 a 14: con `VoidCompute` falsa para `NoEnd`, `event_limit`, `Encode` y pánico, con y sin `--previous-binary`. D5: otro `Engine(_)`.
- [ ] Orden: versión y secreto ausentes da la versión. Un secreto distinto con plazo vencido nunca es `secret_lost`.
- [ ] RF18, por cada razón (`snapshot_invalid`, `engine_error`, `secret_lost`), con apuestas de varios usuarios: reembolso exacto, una fila de auditoría con su `payload`, y semilla publicada salvo en `secret_lost`. Un gancho que falla revierte todo.
- [ ] Fallo que no es negativa, por el binario: con otra transacción reteniendo la fila más de 5 s (`lock_timeout`), y con una base que no responde, sale con exit 1, `fight.void_failed` y su `error_kind`, sin `fight.void_refused`, sin línea en stdout y sin escribir nada. En proceso: un gancho que falla da el mismo evento con `SettlementError.kind`.
- [ ] RF59 de 010: tras la anulación, el runner sale de la parada segura, levanta la detención y difunde el final, también con `secret_lost`.
- [ ] RF24: la negativa sin bandera, la bandera en `admin_audit` y la línea impresa.
- [ ] Logs: `fight_void_logs.rs` (binario, con `RUST_LOG=trace` y `debug,sqlx=trace`) y `fight_void_compute_logs.rs` (en proceso, un solo test por la regla de `test_layout.rs`, para `engine_error`). Ni logs ni stdout tienen la semilla en hexadecimal, base64, base64url ni arreglo de bytes, ni el `canonical_json`. Solo `event` y `error_kind`.

**`secret_lost`**
- [ ] RF19: en una transacción, `live` y `betting_closed`, a 1 ms antes y justo en el plazo, por el guard y por `sealed::void_fight` con esa misma conexión.
- [ ] RF20: con la fila del secreto falla; antes del plazo falla; con `revealed_seed` falla; `engine_error` sin semilla sigue fallando; una `settled` no se reabre; `secret_lost` que cambia `live_started_at` falla; `secret_lost` con `log` falla; y un test ata `5000` y `120000` a las constantes de Rust.
- [ ] RF21: un caso por combinación. Una `voided` de otra razón o una `decided`, sin semilla, dan `Err`. Una `secret_lost` con semilla o con log, o sin commit, da `Err`. RF2 se sigue comprobando. Por HTTP real da `UnverifiableSecretLost` y `seed: null`.
- [ ] RF23 (`void_stats.rs`): sube con cada anulación, `voided` incluye a `secret_lost`, `no-store` también en el 429 y el 503.

**Web**
- [ ] Vitest de `view-state`: `secret_lost` da `voided_unverifiable`, y `engine_error` sigue en `voided`. Vitest de `void-stats`: una lectura por pelea, y sin contador si falla.
- [ ] e2e con mensaje simulado: el texto propio y el contador, sin semilla.
- [ ] Capturas a 375 px y en escritorio, en en y es, en el PR.

**Criterios manuales**
- [ ] security-reviewer y qa-engineer revisan la fase, y frontend-engineer la parte web.
- [ ] `apps/server/README.md` revisado.
- [ ] OK del owner a la enmienda de ADR-0002, antes del merge.

## Estimación de CI
Sin cambios en workflows. El job `rust` suma unas decenas de tests de integración con base propia, sin esperas largas. El tiempo real se reporta en el PR. `e2e` suma un caso de `home.spec.ts`. `check-workflows` no cambia.

## Desviaciones durante la implementación
- **El `CASE` del guard va entre paréntesis (paso 5).** El SQL aprobado no compila: PL/pgSQL termina la condición de un `IF` en el primer `THEN` fuera de paréntesis, y el `THEN` del `CASE` cerraba el `IF` (`syntax error at end of input`). La lógica es la misma.
- **`fights_reveal_ck` usa `IS NOT DISTINCT FROM` (paso 5).** El plan decía `void_reason = 'secret_lost' AND revealed_seed IS NULL`. Con `void_reason` nulo y sin semilla, esa expresión da `NULL`, y un `CHECK` deja pasar un `NULL`: una `decided` sin semilla habría pasado la segunda barrera. `the_checks_are_the_second_barrier` lo fija con el guard apagado.
- **El enum de `public_verifier` se llama `Verification`,** no `Verified`, porque `Verified::Verified` no dice nada. La forma `Result<_, String>` es la del plan.
- **`VoidReason::as_str` entra en el paso 4,** no en el 5: `write_voided` lo necesita. No cambia los tipos generados.
- **`apps/web/tests/routes/home.test.ts` cambia (paso 7),** y el plan no lo lista: compara la forma exacta de `load`, que ahora suma `voidStats`. Los tres casos existentes suman ese campo en `null`, y hay dos casos nuevos: solo una `secret_lost` lee el contador, y si falla no se lleva el tablero.
- **`FightHome` y `FightResultCard` importan `VoidStats` de `void-stats.ts`,** no de `@torneo/protocol`: `tests/fight-boundary.test.ts` no deja que un archivo importe a la vez del protocolo y del renderer.
- **`apps/server/tests/startup.rs` suma las dos migraciones (pasos 1 y 5),** y el plan no lo lista: fija la lista exacta de versiones aplicadas, y cada feature con migración la amplía. Cada versión entra en el commit de su migración.
- **Revisión del PR #86: la base ata cada asiento a su apuesta.** Va más allá de la letra de RF1, y architect debe llevarlo a la spec: `ledger_entries_user_ck` pasa a `(kind = 'rake') = (user_id IS NULL)`; `ledger_entries_bet_kind_ck` deja `bet_id` solo en `bet_stake`, `bet_payout` y `refund`, para que la llave siempre aplique; y `ledger_entries_bet_owner_fk` es una llave compuesta `(bet_id, user_id, fight_id)` hacia `bets_owner_uq`. Se editó `20261102000000_bets.sql`, sin migración nueva: nada de esto se fusionó, y ni `torneo` ni `torneo_e2e` de desarrollo la tienen aplicada. Quien la haya aplicado en otra base necesita `just db-reset`.
  - **Costo medido** con 20 000 apuestas de 2000 cuentas, 3 corridas alternadas por `psql`, en local y con carga de 11: reembolso en lote con `COMMIT`, 3,10 s antes y 3,12 s después, dentro del ruido. Insertar las apuestas, 659 ms antes y 680 ms después, unos 3 %. El trigger de RF2 ya iba en las dos. **Sin `bet_settlements`:** es la comparación de los asientos, no la medición de RF15, que sigue en B.
  - **Queda abierto:** el importe no está atado. Con SQL directo, el dueño de una apuesta de 1 aún puede sacar 1000 de `bet_escrow` con un `refund`. No se pidió cerrarlo, y `bet_payout` no puede tener ese límite. Para decidir antes de B.
- **Revisión del PR #86: plazo de `fight void`.** `VOID_DEADLINE`, de 120 s, cubre todo el comando, y da `fight.void_failed` con `timeout`. `run_fight_void_within` existe para que los tests lo acorten. Una anulación con 20 000 apuestas tardó 3,8 s (compilación de debug, con carga). Con un plazo que cae durante el `COMMIT` el resultado no se conoce: el reintento da `void.bad_state`.
- **Revisión del PR #86: forma canónica.** `--by` solo acepta el UUID en minúsculas y con guiones, y `--previous-binary` rechaza números con ceros a la izquierda. `is_valid_app_version` no cambia: la usa `build.rs` para `APP_VERSION`.
- **Revisión del PR #86: guards.** `no_migration_switches_a_guard_off` vigila también `ledger_postings_rules`, `bets_open_guard`, los seis triggers de inmutabilidad y las funciones `ledger_posting_rules` y `ledger_immutable`, cuyos nombres no contienen el del trigger. No vigila `fights_guard`, porque no se pidió y esta fase lo reemplaza a propósito. La búsqueda de `void_reason` normaliza espacios y mayúsculas, y rechaza cualquier valor asignado, no solo tres formas.
- **Delta de security-reviewer en el PR #86.**
  - `bet_settlements_owner_fk`: un resultado es del dueño y de la pelea de su apuesta, y reemplaza a las tres FK simples. Se editó la misma migración: sigue sin fusionarse ni aplicarse en `torneo` ni en `torneo_e2e`.
  - `void.bad_state` sobre una `secret_lost` repite el aviso de RF24 (`VoidRefusal::AlreadySecretLost`, `VoidFailure::RefusedAfterSecretLost`), porque un plazo en el `COMMIT` lo pierde. El README manda mirar `void_reason` y `admin_audit`.
  - `no_migration_switches_a_guard_off` suma los triggers de inmutabilidad, cuadre y asiento cerrado del ledger, y `DROP CONSTRAINT` y `DROP INDEX` de las barreras de las apuestas. `GUARD_CHANGES_ALLOWED` declara los dos `DROP CONSTRAINT` que 012 ya hace para ampliarlas.
  - **Límite conocido:** no vigila `fights_guard` ni las restricciones de `fights`, que esta fase reemplaza a propósito, ni un `DROP TABLE` o `DROP COLUMN`. Es una ayuda contra errores, no una barrera contra quien edita una migración.
- **Pruebas que cambiaron de archivo respecto de "Archivos que cambian" y "Pruebas":**
  - Los casos de `admin_audit` (`cli_attributed`, `admin_audit_cli_ck`) están en `bets_schema.rs`, no en `admin_audit.rs`, que no cambia.
  - Las formas de `fight void` están en las unitarias de `cli.rs`, y la ayuda, en `fight_void_logs.rs`, no en `admin_cli.rs`, que no cambia.
  - Los exit codes y las salidas también están en `fight_void_logs.rs`.
  - `routes.rs` no cambia, porque recorre `ROUTES` solo. `fights_api.rs` suma la ruta a `the_routes_only_read`.
  - `fights_no_leaks.rs` y `startup.rs` cambian; están anotados arriba.
  - `SettleKind::Payout` tiene su test en `ledger.rs` (`a_payout_goes_by_the_same_batches`): la variante ya existe en esta fase, aunque el reparto sea de B.
- **RF19 y RF20 van en `tests/secret_lost.rs`,** un archivo nuevo, y no en `fight_void.rs` y `fights_schema.rs`, porque comparten el fixture de una sola transacción.

## Revisión
### architect — 2026-11-02 — REQUEST_CHANGES
- Verifiqué contra el código, sin compilar: el alcance de A en "Fases" y en los dos mapeos; D1 a D10; el orden de los pasos; `SETTLE_DELAY_MS`; el código `event_limit`; `admin_audit_cli_ck`; el orden alfabético de los triggers; que `no_migration_switches_a_guard_off` no vigila `fights_guard`; que ningún test llama a `app::serve`; que el runner ya cierra lo que otro liquidó; y que `secret_lost` queda en commit y migración propios. D5 da `void.binary_problem` a todo `Engine(_)` sin fila, y `fights_guard` conserva `rake_bp` en su segundo reemplazo porque el test de RF10 corre con todas las migraciones.
- Cambios que hice, porque me los encargaron: la quinta regla de RF2, en este plan y en su prueba, y las notas de la spec (ver su "Revisión"). No cambian los puntos de abajo, que son del autor.
- Cambios pedidos:
  1. **Una prueba existente se rompe y el plan no lo dice.** `created_xact_from_the_client_is_overwritten` y `an_entry_built_inside_a_savepoint_commits` (`tests/ledger.rs`) debitan la cuenta de `b` en un `grant` de `a`: con RF2 dan `TA008` en el paso 1. Agrégalas a "Barreras de fuente que cambian" o a una desviación: pasan a `ISSUANCE_ACCOUNT` y `account_a`, y siguen probando `created_xact` y el savepoint. RF2 ya lo pide.
  2. **Las excepciones de `fights_source_guard.rs` no son exactas del todo.** En su tabla:
     - `void_reason = EXCLUDED` sigue prohibido en todo `src`.
     - `void_reason = $` aparece una vez, en `store.rs`, dentro de `write_voided`.
     - La lectura exenta de `void_stats.rs` es solo el literal `void_reason = 'secret_lost'`.
     - **Nuevo:** `admin::run_fight_void` solo desde `cli.rs`, y `RealCompute` solo desde ahí (el riesgo ya lo promete). Hoy nada impide que un handler lo llame sin una ruta con `void`, y RF16 dice que no hay endpoint.
     - La lista de `Invalid` suma `fight void 1` sin `--by`, y `fight void 1 --by <uuid> --reason x`: la razón nunca se elige.
  3. **El cuerpo de `fights_guard` en el paso 5 es ambiguo, y es el paso más riesgoso.** "Con `secret_lost`, … Con otra razón, el texto actual" se puede leer como dos ramas. Di que solo se condicionan las dos líneas del reveal: `settled_at`, `cancel_reason`, `live_started_at` y la comprobación de `end_reason`, `winner` y `log` siguen rigiendo también en `secret_lost`. Suma a RF20 un caso que cambie `live_started_at` y otro con `log`.
  4. **Falta el camino de un fallo que no es negativa.** Con un error de la base o `lock_timeout` en `void`: `ROLLBACK`, exit 1, y un evento propio con `error_kind` (`fight.void_failed`), sin fila de RF17. Agrégalo al paso 4 y a sus pruebas.
  5. **Texto viejo.** El punto "Numeración" de "Choques" ya no vale: TA-44 es `011` y esta feature, `012`; quítalo. En "Archivos", `support/bets.rs` va en el paso 2, porque siembra con `ledger::bet_stake`. "Si el owner la rechaza" suma `--backup-checked` y la parte de RF24 del README.
- Tras estos cambios, el plan vuelve a `in-review` para una lectura corta de esos puntos.

### backend-engineer — 2026-11-02 — Cambios aplicados
- Apliqué los cinco puntos: D11 con los dos tests de `ledger.rs`, excepciones exactas y `run_fight_void` solo desde `cli.rs`, cuerpo del guard con solo las líneas del reveal condicionadas y sus dos casos de RF20, `fight.void_failed`, y el texto viejo. También listé las cinco reglas de RF2 en el paso 1. Decidí mover `--backup-checked` al paso 5, para que rechazar la enmienda no deje una bandera sin uso. No compilé: el SQL del guard y los tests siguen sin ejecutar. No apruebo este documento: lo escribí yo.

### architect — 2026-11-02 — APPROVE
- Verifiqué, leyendo y sin ejecutar, solo los cinco puntos pedidos y el movimiento de `--backup-checked`: D11 y sus dos tests contra `tests/ledger.rs`, incluido que `a_committed_entry_takes_no_new_postings` sigue en `TA003`; las excepciones exactas y las dos pruebas de fuente nuevas; el SQL del guard, donde solo cambian las líneas del reveal; `fight.void_failed` y su prueba; el texto viejo; y que rechazar la enmienda quita `--backup-checked` con el paso 5, sin dejar una bandera sin uso.
- Esta aprobación no levanta el gate: el PR no se fusiona sin el OK del owner a la enmienda de ADR-0002. El SQL del guard y los tests siguen sin ejecutar, y el PR debe mostrar el `diff` de cada cuerpo de `fights_guard`.
