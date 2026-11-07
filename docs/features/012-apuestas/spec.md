---
id: "012"
title: Apuestas parimutuel, bono diario y rescate
type: spec
status: approved
risk: high
author: architect
reviewers: [product-owner, security-reviewer, game-designer]
approvals:
  - {by: product-owner, date: 2026-11-02}
  - {by: security-reviewer, date: 2026-11-02}
  - {by: game-designer, date: 2026-11-02}
  - {by: owner, date: 2026-11-06}
linear: TA-27
depends_on: ["008", "010"]
created: 2026-11-02
updated: 2026-11-06
---

# Spec: Apuestas parimutuel, bono diario y rescate

> Intent: [intent.md](./intent.md) · ADRs: [0002](../../decisions/0002-commit-reveal-semilla.md), [0005](../../decisions/0005-economia-sin-dinero-real.md), [0007](../../decisions/0007-i18n-desde-cero.md), [0008](../../decisions/0008-autenticacion.md) D5 a D9 y su nota del 2026-10-28, [0009](../../decisions/0009-ui-design-system.md), [0012](../../decisions/0012-logs-y-cabeceras-de-seguridad.md) · Specs: [010](../010-scheduler-de-peleas/spec.md) RF7, RF9, RF16, RF18, RF25, RF27 a RF33, RF41, RF54, RF59 y preocupaciones 1, 3 y 13

## Resumen
El usuario con sesión apuesta `Coins` a un lado mientras la ventana está abierta, ve moverse el pozo y, al liquidarse la pelea, ve su pago, su pérdida o su reembolso. Reclama el bono de cada día UTC y, si se queda sin monedas, el rescate. Todo movimiento es un asiento del ledger, y la liquidación corre en la transacción del reveal, por el gancho de 010. Cada apuesta guarda `ranked_stake` al crearse y `RankedProfit` al liquidarse. Antes de la primera apuesta existe la salida `server fight void`, incluida la pelea con el secreto perdido, que pide enmendar ADR-0002.

**Lo que esta spec decide y conviene revisar primero:** pozos públicos, pago por relectura, rake con tope en el lado perdedor (0 al inicio y 5 % como objetivo), residuo al sumidero, reembolso de todo pozo con un lado vacío, un solo lado por usuario y pelea, y carencia contada por peleas (preocupaciones 1 a 4). El tope del rake y la carencia por peleas piden una nota en ADR-0008, escrita como `proposed` y pendiente del owner.

## Fases
Cada fase es un PR con su plan. Ninguna empieza hasta aprobar esta spec. `voided` va primero: ningún merge deja aceptar una apuesta sin esa salida.

| Fase | Plan | Dueño | Contenido | Gate propio | Qué se puede probar al terminar |
|---|---|---|---|---|---|
| A | `plan-a.md` | backend-engineer (la parte de `apps/web` la revisa frontend-engineer) | Migración de ledger y tablas, módulo `bets` con reembolso y gancho, `server fight void` completo, `secret_lost`, RF7, contador público, estado "no verificable" en la vista | **Enmienda de ADR-0002 aceptada antes del merge** | Sin API de apuestas: filas sembradas por los tests se reembolsan al anular. Cada negativa de RF59 |
| B | `plan-b.md` | backend-engineer | API, reparto, `RankedProfit`, bono, rescate, carencia, topes, pozo por WS, reserva de capacidad, configuración | **OK del owner a `B`, `m` y `r`, a las notas `proposed` de ADR-0008 y a las preocupaciones 1 y 3 de 010** | Con dos clientes HTTP en `just dev`: apuestan, se liquida, cobran. Tests 7, 8, 18, 23, 24, 25 y 26 |
| C | `plan-c.md` | frontend-engineer | Panel en la home, lecturas SSR, form actions, i18n, e2e, trabajo de cierre | OK del owner al criterio en local | Todos los criterios del intent, `just check` y `just e2e` |

`qa-engineer` verifica cada fase y `security-reviewer` revisa las tres. `risk: high`: el owner da su OK explícito a esta spec, además de las aprobaciones.

## Requisitos funcionales

### Ledger y datos (A)
- **RF1. Cuentas y tipos de asiento.** Una migración aditiva reemplaza `ledger_accounts_kind_ck` (suma `bet_escrow` y `rake_sink`), crea esas dos cuentas con id fijo (`ESCROW_ACCOUNT`, `RAKE_SINK_ACCOUNT`), reemplaza `ledger_entries_kind_ck` (`grant`, `bailout`, `bet_stake`, `bet_payout`, `refund`, `rake`) y `ledger_entries_user_ck` (`(kind = 'rake') = (user_id IS NULL)`: un `rake` es de la pelea y de nadie, porque con usuario podría pagarle desde la custodia), y suma `fight_id` y `bet_id` nulos a `ledger_entries`, con `CHECK (kind NOT IN ('bet_stake', 'bet_payout', 'refund') OR (bet_id IS NOT NULL AND fight_id IS NOT NULL))` y `CHECK (kind <> 'rake' OR fight_id IS NOT NULL)`: con esas columnas nulas, los índices de RF3 no verían el asiento. Un asiento de apuesta mueve las monedas de su apuesta: solo `bet_stake`, `bet_payout` y `refund` llevan `bet_id` (`ledger_entries_bet_kind_ck`), y una FK compuesta (`ledger_entries_bet_owner_fk`, de `(bet_id, user_id, fight_id)` a `bets_owner_uq`) lo ata a su dueño y a su pelea. El importe no se ata: ver preocupación 21. → integración con SQL directo: cada tipo se acepta, uno desconocido falla, un asiento de apuesta sin `bet_id` o sin `fight_id` falla, un `rake` sin `fight_id` o con usuario falla, otro tipo con `bet_id` falla, un asiento con el usuario o la pelea de otra apuesta falla, y un test ata los ids a la migración como ya hace `ISSUANCE_ACCOUNT`.
- **RF2. Solo `ledger/` inserta asientos, y la base limita qué mueve cada uno.** `ledger/` suma `bet_stake`, `bailout` y un `settle_batch` que inserta por lotes de 1000 filas con `UNNEST`. Un trigger sobre `ledger_postings` (se amplía `ledger_posting_open_entry` o va uno hermano, con el código `TA008`) exige, al insertar cada movimiento:
  - `rake_sink` nunca se debita;
  - `issuance` solo se debita en `grant` y `bailout`;
  - solo `bet_stake` acredita `bet_escrow`;
  - `bet_escrow` solo se debita en `bet_payout`, `refund` y `rake`;
  - una cuenta de usuario pertenece a `entry.user_id`;
  - **desde la fase B, con su propia migración,** un `bet_stake` solo mueve `bets.stake` de su apuesta: usuario `−stake` y `bet_escrow +stake`, sin otra cuenta (preocupación 21). No se aplica a `refund`, `bet_payout` ni `rake`, así que no suma lecturas a la transacción del reveal.

  Su costo entra en la medición de RF15. → `only_the_ledger_module_inserts_ledger_rows` y `no_source_updates_or_deletes_the_ledger` siguen pasando; integración con SQL directo, un caso rechazado por regla y los flujos reales aceptados. La regla de la cuenta de usuario exige cambiar dos tests de `tests/ledger.rs`, `created_xact_from_the_client_is_overwritten` y `an_entry_built_inside_a_savepoint_commits`, que mueven monedas entre dos usuarios dentro de un `grant`: pasan a `issuance` y al usuario del asiento, y siguen probando lo mismo.
- **RF3. Un asiento por hecho, exigido por la base.** Índices únicos parciales: `(user_id, grant_day) WHERE kind = 'bailout'`, `(bet_id) WHERE kind = 'bet_stake'`, `(bet_id) WHERE kind IN ('bet_payout', 'refund')` y `(fight_id) WHERE kind = 'rake'`. → integración: un segundo asiento de cada clase falla.
- **RF4. `bets`, solo inserción.** Tabla de abajo. `ledger_immutable` rechaza `UPDATE`, `DELETE` y `TRUNCATE` (`TA001`). Un trigger rechaza con `TA007` un `INSERT` si la pelea no está en `betting_open`, segunda barrera de RF29 de 010. → integración con SQL directo.
- **RF5. `ranked_stake` es una foto.** Se escribe al crear la apuesta con `NOT carencia`, leída bajo el bloqueo del usuario, y no cambia nunca (RF4). → integración: una apuesta de una cuenta en carencia guarda `false`, y una de una fuera de carencia, `true`. Una cuenta que sale de la carencia entre su apuesta y la liquidación conserva `false` (test 25(c)).
- **RF6. Resultado inmutable por apuesta.** `bet_settlements` guarda por apuesta `outcome`, `payout`, `net` y `ranked_profit`, solo inserción, con `bet_id` como llave primaria. `CHECK`: `ranked_profit <= net`, `refunded` implica `net = 0` y `ranked_profit = 0`, y `lost` implica `payout = 0` y `ranked_profit = net`. **`season_stats` es de TA-28**: esta feature no crea esa tabla ni temporadas. → integración con SQL directo; TA-28 suma desde esta cifra.

### Liquidación (A: reembolso. B: reparto)
- **RF7. Gancho real.** `BetsHook` implementa `SettlementHook` en `apps/server/src/bets/settle.rs` y reemplaza a `NoBets` en `app::serve`. `NoBets` queda solo para las pruebas del ciclo sin apuestas. **En la fase A, un `decided` con los dos lados con stakes devuelve `Err`:** todavía no hay reparto, y ese error lleva a la detención, nunca a una pelea `settled` sin asientos. Ese caso no tiene camino sin API, y solo se daría volviendo de B a A. `apps/server/README.md` prohíbe volver a un binario con `NoBets` si hay apuestas sin liquidar. → integración sobre `app::serve`: una apuesta real termina pagada; prueba de fuente de que `serve` construye `BetsHook`; en A, integración del `Err`: sin `settled` ni asientos.
- **RF8. Idempotente.** Lo primero que escribe el gancho, si la pelea tiene apuestas, es `INSERT INTO pool_settlements … ON CONFLICT (fight_id) DO NOTHING RETURNING`. Sin fila devuelta, ya se liquidó y sale con `Ok` sin escribir nada. Una pelea sin apuestas no escribe nada. → integración: llamar dos veces al gancho, en la misma transacción y tras un `COMMIT`, deja los mismos asientos y las mismas filas (test 25, "reliquidar no cambia nada"). Con un `kill` antes del `COMMIT`, no queda nada y el reintento lo deja una sola vez (RF31 de 010).
- **RF9. Qué final hace qué.** El gancho decide solo con `final_result` y las filas de `bets`:

  | Final o pozo | Resultado | `pool_settlements.outcome` |
  |---|---|---|
  | `decided` con ganador y los dos lados con stakes | Reparto (RF10) | `paid` |
  | `decided` con ganador y un lado vacío | Reembolso | `refund_one_sided` |
  | `decided` con empate | Reembolso | `refund_draw` |
  | `voided`, cualquier razón | Reembolso | `refund_voided` |
  | `cancelled` | Reembolso de lo que haya: nunca hay nada (RF33 de 010), y si lo hubiera, no se pierde | `refund_cancelled` |

  Un reembolso es un asiento `refund` por apuesta (`bet_escrow −stake`, usuario `+stake`), sin rake, con `outcome = 'refunded'` y `ranked_profit = 0`. No cuenta como apuesta liquidada. → integración, una prueba por fila.
- **RF10. Reparto en enteros.** Una función pura, sin SQL ni reloj (`bets/parimutuel.rs`), con aritmética `i128` y división hacia abajo. Con `W` la suma del lado ganador, `L` la del perdedor, `T = W + L` y `r_bp` el rake de la pelea en puntos básicos:
  - `rake = min(⌊T · r_bp / 10000⌋, L)`: un % de **todo el pozo**, con tope en el lado perdedor (nota `proposed` en ADR-0008).
  - `N = T − rake`, y `payout_i = ⌊stake_i · N / W⌋` por apuesta. Como `N ≥ W`, **`payout_i ≥ stake_i`: acertar nunca hace perder monedas.**
  - `residuo = N − Σ payout_i`.
  - El asiento `rake` mueve `rake + residuo` de `bet_escrow` a `rake_sink`, y `pool_settlements` guarda los dos por separado. Con `rake + residuo = 0` no hay asiento `rake`: el ledger rechaza un importe 0 (`ledger_postings_amount_ck`).
  - **El rake es por pelea e inmutable.** `fights.rake_bp` se escribe al crear la fila, con el valor de la configuración de ese momento, y entra en la lista de RF4 de 010, así que tampoco cambia al abrir. Lo usan `FightPool.rake_bp`, `payout_bp`, `BetQuote` y el gancho, que por eso no necesita configuración.
  - `pool_settlements.rake_bp` guarda el de la pelea en `paid`, y 0 en un reembolso, que no cobra rake y no lo lee: así la CLI funciona sin configuración.

  Ejemplos con 500 pb: 7 y 5 en A contra 9 en B, gana A: `T = 21`, `rake = 1`, `N = 20`, pagos 11 y 8, `residuo = 1`, y al sumidero van 2. 500 contra 10, gana el de 500: `rake = min(25, 10) = 10`, pago 500 y neto 0. → propiedad sobre pozos aleatorios: `Σ payout + rake + residuo = T`; `rake ≤ L`; `payout_i ≥ stake_i`; `0 ≤ residuo < número de ganadoras`; el resultado no depende del orden de las apuestas; repartir un stake en varias apuestas nunca paga más que una sola (RF12). Unitaria: los ejemplos y `r_bp = 0`. Integración: cambiar la configuración entre el anuncio y la liquidación paga con el valor de la pelea, y la siguiente pelea anunciada usa el nuevo. Integración con SQL directo, en la fase A y con todas las migraciones aplicadas: `UPDATE fights SET rake_bp` da `TA006` en cada estado no `settled`.
- **RF11. `RankedProfit` por apuesta.** Función pura, según D8, de `(payout, stake, L, L_c)`, con `d = payout − stake` y `L_c` la parte de `L` con `ranked_stake = true`: perdedora, `−stake`; ganadora con `d ≤ 0`, `d`; ganadora con `d > 0` y `L = 0`, `d`; ganadora con `d > 0`, `⌊d · L_c / L⌋`. Se calcula en la transacción del gancho y se guarda en `bet_settlements` (RF6). El pago no depende de esta cifra. → test 25 completo, ver "Mapeo". Ni `d < 0` ni `L = 0` tienen camino por la liquidación (RF9 y RF10), así que 25(d) y 25(e) se prueban sobre la función, con `payout` y `stake` de entrada.
- **RF12. El residuo no premia a nadie.** Es una decisión con prueba: dividir un stake nunca conviene, porque `⌊a⌋ + ⌊b⌋ ≤ ⌊a + b⌋`, y ninguna regla de desempate entra en el dinero. → la propiedad de RF10.
- **RF13. Conservación.** En todo momento, la suma de todos los movimientos es 0 y `saldo(bet_escrow) = Σ stakes de peleas sin liquidar`. Tras liquidar una pelea, lo que entró por ella salió entero. **También se comprueba fuera de los tests (fase B):**
  - **En producción:** `apps/server/README.md` lleva una consulta de solo lectura, en una sola sentencia para que una liquidación a medias no dé un falso positivo. Da el saldo de `bet_escrow`, los stakes sin liquidar, la suma de todos los movimientos, las apuestas sin un `bet_stake` igual a su `stake`, y los `refund` distintos de su `stake`. Sana, los dos primeros coinciden y el resto da 0.
  - **En el gancho:** antes de escribir, en memoria y sin lecturas extra, `Σ payout + rake + residuo = T`. Si no, devuelve `Err` y no escribe nada: reintento y detención, nunca un reparto sin cuadrar.

  → propiedad de integración, tras una secuencia aleatoria de apuestas y finales. Un test lee la consulta del README y la ejecuta: limpia en una base sana, y la señala tras un `refund` de más con SQL directo y con una apuesta sin `bet_stake`. Unitaria del cuadre del gancho con un reparto que no suma.
- **RF14. RF31 de 010: ningún estado alcanzable por la API hace fallar el gancho.** El gancho lee solo `bets`, `fights.rake_bp`, `ledger_accounts` y su propio `pool_settlements`, y nunca `users`, sesiones ni `banned_at`. Nada que haga el usuario tras apostar cambia su resultado. Un gancho que no confirma a tiempo también es un fallo alcanzable, y por eso existe RF15. → integración, cada una con su asiento correcto y la pelea en `settled`: cuenta baneada tras apostar, frente a la misma corrida sin ban (mismos asientos); cuenta que desvincula un proveedor tras apostar; apuestas en los topes (`5 B` y `1 B`); pozo de un solo lado; rake de 0, y rake en su tope; `BETS_MAX_PER_FIGHT` apuestas (RF15). Con un `attempt_timeout` de prueba menor que lo que tarda el gancho: reintenta, llega la detención, no queda `settled` parcial, y al subir el límite confirma una sola vez. Prueba de fuente: `bets/settle.rs` no contiene `banned_at` ni `users`.
- **RF15. Dimensionado.** El gancho corre dentro de la transacción del reveal, con el `attempt_timeout` de 30 s del runner, y tras 60 s de fallos hay detención (RF18 de 010). Si no cabe en esos 30 s, reintenta siempre y detiene todo, mientras `void` responde `fight_healthy`. Por eso el número de apuestas tiene tope:
  - `BETS_MAX_PER_USER_PER_FIGHT = 10`, comprobado bajo el bloqueo del usuario, y `BETS_MAX_PER_FIGHT = 20 000`, por conteo (un exceso de pocas filas por concurrencia no importa). Son constantes, no entorno.
  - El gancho carga las apuestas una vez (unos 64 B por apuesta, 1,3 MB en el tope), calcula en memoria y escribe en lotes de RF2.
  - **Medición, de dos pozos con 20 000 apuestas:** 2000 cuentas con 10 apuestas de `m` al mismo lado, que da 20 000 asientos `refund`; y un pozo `paid` con los dos lados, con sus `bet_payout` y su asiento de `rake`. Incluye el `COMMIT`, donde corren los triggers diferidos, y el trigger de RF2. Corre con el pool real (8 conexiones, 1 s de `acquire_timeout`), con apuestas concurrentes, con la relectura de RF36 y con `quote`. Comprueba si el backend de un intento cancelado sigue vivo.
  - **Criterio:** ≤ 10 s en local, en el PR de B, con un test `#[ignore]`. Si no se cumple, se baja `BETS_MAX_PER_FIGHT` en ese PR. **Prerrequisito bloqueante nuevo de TA-8:** ≤ 10 s en el VPS, o bajar `BETS_MAX_PER_FIGHT`.
  - **Salida del atasco,** en `apps/server/README.md`: quitar la contención y reiniciar; si no basta, un binario con mayor `attempt_timeout`. Las apuestas ya no suben, porque en una detención la ventana no las acepta.
  - Sin bloqueos de usuario en el gancho: ver RF28.

  → integración en CI con 2000 apuestas y `#[ignore]` con 20 000. La apuesta 20 001 da `bet.pool_full`.
### Salida `voided` (A)
- **RF16. Comando.** `server fight void <fight_id> --by <admin_user_id> [--previous-binary <tag>] [--backup-checked]`, en `cli.rs` y `admin.rs`. Corre una transacción con `lock_timeout` de 5 s que bloquea la fila de la pelea con `FOR UPDATE`. El runner nunca lo llama (prueba de RF16 de 010), y no hay endpoint. La razón se deduce, nunca se elige. La salida es una línea de texto fijo con el código y sin semilla ni datos del log. Lee solo `DATABASE_URL` y `RUST_LOG`: un reembolso guarda `rake_bp = 0` (RF10). Exit 0 al aplicar, 1 ante una negativa y 2 con una línea de comandos inválida. Una negativa registra `fight.void_refused` con `error_kind` igual a su código, sin campos nuevos en `ALLOWED_FIELDS`. → unitarias de `cli::parse`; RF16 de 010 sigue recorriendo `cli::parse` y `admin --help`, que documenta el comando. **Esta spec obliga a estrechar las barreras de `fights_source_guard.rs`, sin borrar ninguna:** el comando y `GET /api/fights/void-stats` (RF23) son excepciones exactas, una sola función escribe `void_reason`, y fuera de la entrada de `cli.rs` nada llama a la anulación. El detalle está en el plan de la fase A.
- **RF17. Negativas, en este orden.** Cada una no escribe nada. El orden extiende RF59 de 010 y pone `secret_lost` donde pide la preocupación 13:

  | # | Condición | Resultado |
  |---|---|---|
  | 1 | `--by` no existe o no es `admin` | `void.not_admin` |
  | 2 | La pelea no existe | `void.not_found` |
  | 3 | Estado distinto de `betting_closed` o `live` | `void.bad_state` |
  | 4 | Falta la versión de reglas | `void.version_missing`. La salida es volver a la versión anterior |
  | 5 | La fila del secreto existe y no coincide con el commit | `void.secret_mismatch`. **Nunca es `secret_lost`** |
  | 6 | La fila no existe y `now() < plazo` (RF19) | `void.emission_possible` |
  | 7 | La fila no existe, ya pasó el plazo y falta `--backup-checked` | `void.backup_not_checked` |
  | 8 | La fila no existe, ya pasó el plazo y está `--backup-checked` | Procede con `secret_lost` |
  | 9 | Hash de RF2 o `slug_*` distintos (sobre los bytes, sin el motor) | Procede con `snapshot_invalid` |
  | 10 | Hash y slugs bien, falla decodificar o reserializar, o difieren `max_hp_*`, `time_limit_ms` o `max_duration_ms`, o `validate` falla | `void.binary_problem`. La salida es volver a la versión anterior |
  | 11 | `compute` funciona | `void.fight_healthy` |
  | 12 | `compute` da `NoEnd` (`log_without_end`) | `void.no_end`. Se niega por defecto |
  | 13 | `compute` falla por `EventLimit`, `encode_failed` o un pánico, y falta `--previous-binary` | `void.previous_binary_not_tried` |
  | 14 | Igual que la 13, con `--previous-binary` | Procede con `engine_error` |

  Cualquier otro `ComputeError::Engine(_)`, sin fila en esta tabla, da `void.binary_problem`: con `validate` ya superado no debería ocurrir, y la negativa por defecto de RF59 de 010 no deja que un código desconocido publique ni anule nada.

  Un `decided` que no puede liquidar (RF7 en fase A, o RF15) da `void.fight_healthy`, porque el log se reconstruye: `void` no es su salida. → integración, un caso por fila, comprobando que no se escribió nada: ni en `fights`, ni en `fight_secrets`, ni en el ledger, ni en `admin_audit`.
- **RF18. Al aplicar.** En una sola transacción, por una sola entrada, `admin::run_fight_void`. Primero `sealed::void_fight`, cuyo único llamador es esa función: escribe `void_reason` y `settled_at`, escribe `revealed_seed` y borra el secreto salvo en `secret_lost`, y llama al gancho (RF9). Después, en la misma transacción, `admin.rs`, donde vive todo el SQL de auditoría, inserta en `admin_audit` con `action = 'fight_void'`, `actor = 'cli_attributed'`, `admin_id = --by`, `target` igual al id y `payload` con `reason`, `previous_binary` si aplica, `backup_checked`, `bets_refunded` y `coins_refunded`. Una migración reemplaza `admin_audit_action_ck` y `admin_audit_actor_ck`, y `admin_audit_cli_ck` queda igual. → integración: cada razón con apuestas sembradas de varios usuarios: reembolso exacto, una fila de auditoría y la semilla publicada salvo en `secret_lost`. Con `RUST_LOG=trace`, ni los logs ni stdout contienen la semilla con los canarios de RF27 de 010. Un gancho que falla revierte todo.
- **RF19. Plazo de `secret_lost`.** `plazo = COALESCE(live_started_at, live_at) + max_duration_ms + SETTLE_DELAY_MS + SECRET_LOST_MARGIN_MS`, con el margen en 120 s: 60 s de relectura del runner y 60 s de la detención por liquidación fallida. Son 215 s con los valores por defecto. **En `betting_closed`, sin `live_started_at`, rige `live_at`:** una pelea así con el plazo vencido no pudo emitir nada, porque pasar a `live` cambiaría la fila que el comando tiene bloqueada. La hora es la de la base, `now()`, nunca la del proceso. → integración con horas en la fila: `live` y `betting_closed`, un milisegundo antes y después.
- **RF20. Barreras en la base para `secret_lost`.** Una migración suma el valor a `fights_void_reason_ck`, relaja `fights_reveal_ck` (`void_reason = 'secret_lost' AND revealed_seed IS NULL` es la única forma sin semilla) y reemplaza `fights_guard`: con `secret_lost` exige `revealed_seed` nulo, `NOT EXISTS` en `fight_secrets` y el plazo de RF19. Con cualquier otra razón, todo sigue como en RF9 de 010. → integración con SQL directo: `secret_lost` con la fila del secreto presente falla; antes del plazo falla; `secret_lost` con `revealed_seed` falla; `engine_error` sin semilla sigue fallando; y un `UPDATE` directo no reabre nada en una fila `settled`. Un test ata las constantes de la migración a las de Rust.
- **RF21. Verificación pública (RF7 de 010).** Un helper de test con cuatro resultados: `verified`, `cancelled_unverifiable` (lo que ya hay para una cancelada), **`unverifiable_secret_lost`** e `invalid`. Una `voided(secret_lost)` da `unverifiable_secret_lost` solo si `seed` y `log` son nulos y el commit existe. **Una semilla nula nunca pasa como una comprobación omitida:** una `voided` de otra razón, o una `decided`, sin semilla da `invalid`. Una `secret_lost` con semilla también da `invalid`. RF2 se sigue comprobando. → `fights_public_verify.rs`, sin importar `server`, un caso por combinación.
- **RF22. Vista.** `VoidReason::SecretLost` llega a la vista como un resultado propio, `voided_unverifiable`, con su texto, distinto del de `voided` (en y es): dice que la pelea se anuló, que se devolvió lo apostado y que **no se puede verificar**. Muestra el contador de RF23, que el SSR de la home lee solo si `last` es `secret_lost`, y el cliente una vez si el final llega por socket; si la lectura falla, no hay línea de contador y el resto del texto sigue. No enseña ni el bloque de comprobación ni la semilla. → Vitest de `view-state` y e2e con un mensaje simulado.
- **RF23. Contador público.** `GET /api/fights/void-stats`, sin sesión, da `VoidStats { voided, secret_lost }`, donde `voided` es el total e incluye a `secret_lost`, con el límite `FIGHT_READ_IP` y `no-store`, en un archivo propio (`fights/void_stats.rs`), con un índice parcial. → integración: sube con cada anulación.
- **RF24. Pasos humanos, con rastro.**
  - **Binario anterior** (preocupación 3 de 010): `--previous-binary <tag>` acepta solo `vX.Y.Z` o `sha-<7>` y queda en `admin_audit`. Es una declaración, no una prueba. Lo que la hace refutable es que la semilla sale publicada y cualquiera puede resimular.
  - **Backup:** `--backup-checked` obliga a declarar que se intentó restaurar, y queda en `admin_audit`.
  - **Reabrir la aceptación:** tras un `secret_lost`, el comando imprime una línea fija con ese aviso. `apps/server/README.md` lleva el procedimiento: abrir un issue en Linear con el id y pedirle al owner que vuelva a decidir sobre la preocupación 1 de 010. El comando no puede comprobar que se hizo.

  → integración de la negativa sin bandera y de la línea impresa; revisión del README en el PR.

### Apostar (B)
- **RF25. `POST /api/bets`.** Cuerpo `BetRequest { fight_id, side, amount, client_key }`, con `deny_unknown_fields`. Con `Effect::Mutate`, el middleware de `Origin` (D4), la capa `MAX_FORM_BYTES` y los límites de D7 (usuario 30/min con ráfaga de 10, IP 120/min). Un cuerpo mal formado o un `client_key` que no es UUID da 422 `bet.invalid_request`, sin texto de serde. `fight_id` pasa a `i32` con `try_from`, y fuera de rango da `bet.window_closed`. Una transacción, con `SET LOCAL lock_timeout = '1s'` antes de todo (lo que RF29 de 010 dejó a TA-27), evalúa en este orden:

  | # | Comprobación | Código |
  |---|---|---|
  | 1 | Sesión | 401 `auth.unauthenticated` (el existente) |
  | 2 | Bloquea al usuario con `FOR UPDATE`, con `banned_at IS NULL` | 401, igual que un ban ya aplicado (ver `banned_writes.rs`) |
  | 3 | Misma `client_key` de este usuario | Con el mismo cuerpo, 200 con la apuesta original y sin escribir. Con otro, 409 `bet.key_reused` |
  | 4 | Lee el reloj **aquí, después de la fila 2,** y llama a `lock_open_for_bet` | 409 `bet.window_closed`, sin decir por qué (RF29 de 010). También sirve para una pelea que no existe |
  | 5 | `amount ≥ m` | 422 `bet.below_minimum` |
  | 6 | No tiene apuestas al otro lado en esta pelea | 409 `bet.side_locked` |
  | 7 | Tiene menos de `BETS_MAX_PER_USER_PER_FIGHT` | 409 `bet.too_many_bets` |
  | 8 | La pelea tiene menos de `BETS_MAX_PER_FIGHT` | 409 `bet.pool_full` |
  | 9 | `Σ stakes propios en la pelea + amount ≤ tope` (RF27) | 409 `bet.fight_cap_exceeded` |
  | 10 | `saldo ≥ amount` | 409 `bet.insufficient_balance` |

  Si todo pasa: inserta la apuesta, el asiento `bet_stake` (usuario `−amount`, `bet_escrow +amount`) y un `auth_events` de tipo `bet`, con `bet_id` e `ip_hash`. Responde 201 `BetPlaced { wallet, mine }`. Un error de la base da 503 `bet.unavailable`, **salvo en la fila 4:** `lock_open_for_bet` convierte todo fallo en `WindowClosed` (RF29 de 010), y da 409 `bet.window_closed`. Falla cerrado, no escribe nada, y registra `fights.db_unavailable`. → integración, un caso por fila; la fila 4 con la pelea retenida más allá del `lock_timeout`, que da 409 sin escribir; apuesta contra cierre: la que vio la ventana abierta confirma antes del cierre; apuesta y `bet_stake` en una sola transacción, con el mismo importe, y un `bet_stake` con otro importe da `TA008`; cuerpo sobre el límite, mal formado, `client_key` inválido y `fight_id` fuera de rango; test 23; una apuesta rechazada no deja ni asiento ni fila; con el usuario retenido por otra transacción mientras cruza `closes_at`, da `bet.window_closed` al soltarse.
- **RF26. Un solo lado por usuario y pelea: obligatorio.** Decisión de esta spec (preocupación 3). Con `r = 0`, cubrirse cuesta 0 en cualquier pozo: sin esta regla, una sola cuenta arma un pozo de dos lados consigo misma, sin riesgo, que no se reembolsa y cuenta para salir de la carencia. La UI lo avisa antes de apostar (RF42). → la fila 6 de RF25.
- **RF27. Carencia y topes.** Una cuenta está en carencia si `users.created_at > now() − 7 días` **o** tiene menos de 10 **peleas** con una apuesta ganada o perdida (nota `proposed` en ADR-0008). Se deriva de `bet_settlements`, sin columna, y nunca se vuelve a entrar, porque los dos datos solo crecen. Tope: `1 B` en carencia y `m_cap · B` fuera de ella, con `m_cap = 5`. Un reembolso no cuenta. `MainEvent` todavía no existe, así que hay un solo múltiplo, y el intent no lo sube. → test 24, ver "Mapeo". Unitaria de los dos ejes por separado. Prueba: diez apuestas en una sola pelea cuentan como una (preocupación 4).
- **RF28. Orden de bloqueos.** Usuario y después pelea, siempre con `FOR UPDATE` (D8; ver preocupación 8). Una apuesta solo toma `FOR SHARE` sobre una pelea `betting_open`, y una liquidación solo escribe peleas fuera de ese estado, así que no se forma un ciclo. Rescate y bono no bloquean peleas. El gancho no toma bloqueos de usuario, pero las FK de sus asientos toman `FOR KEY SHARE` en `users`: esperan a un `FOR UPDATE` en curso, que dura milisegundos, y lo retienen hasta su `COMMIT`. Una espera de más de 2 s da `lock_timeout` y reintento. → integración: una transacción retiene al usuario 3 s durante la liquidación, la liquidación reintenta y confirma sin detención; y apuestas y liquidación en paralelo sin `40P01`.
### Bono y rescate (B)
- **RF29. `POST /api/bonus/claim`.** Con `MAX_FORM_BYTES`, `SET LOCAL lock_timeout = '1s'` y `SELECT … banned_at IS NULL FOR UPDATE` del usuario. `ledger::grant` con `B`, por el índice único, y su `auth_events` `grant`. Sin insertar, 409 `bonus.already_claimed`. Un fallo de la base da 503 `wallet.unavailable`. El primer login ya da el bono de ese día (`auth/login.rs`), así que ese día el reclamo responde 409. Límites de D7: usuario 5/min, IP 60/min. Responde 200 con `Wallet`. → test 7, sin el limitador, porque 20 peticiones lo agotarían.
- **RF30. `POST /api/bailout/claim`.** Con `MAX_FORM_BYTES`, `SET LOCAL lock_timeout = '1s'` y `SELECT … banned_at IS NULL FOR UPDATE` del usuario, y en este orden: sin el `grant` de hoy, 409 `bailout.grant_available`; con `saldo + stakes abiertos ≥ m`, 409 `bailout.not_eligible`; y un `INSERT … ON CONFLICT DO NOTHING` por el índice de RF3, sin insertar, 409 `bailout.already_claimed`. Mueve `R` de `issuance` al usuario, con su `auth_events` `bailout`. Un fallo de la base da 503 `wallet.unavailable`. Stakes abiertos son los de peleas con `state <> 'settled'`, así que una pelea detenida sigue contando (RF16 de 010). **`saldo + stakes abiertos` se calcula en una sola sentencia:** en dos, el saldo podría leerse antes del `COMMIT` de una liquidación y los stakes después, y el usuario se vería sin monedas sin estarlo. Responde 200 con `Wallet`. → tests 7 (sin el limitador), 8 y 26; rescate contra liquidación, a ambos lados del `COMMIT`: nunca se concede con un pago en camino.
- **RF31. Banear tras apostar no cambia nada,** y bono y rescate piden sesión válida: una cuenta baneada no reclama. → RF14, y `banned_writes.rs` cubre las dos rutas.
### Lecturas (B)
- **RF32. `GET /api/wallet`.** Con sesión, `private, no-store` y `Vary: Cookie`. Da `Wallet`: saldo libre, stakes abiertos, si el bono está disponible, `bailout` (`available`, `grant_first`, `not_eligible`, `already_claimed`), `B`, `R`, `m`, el siguiente reinicio (`day_resets_at`), su tope por pelea (`fight_cap`) y `probation` (`null`, o `{ days_left, fights_left }`). **Calcula lo mismo que los `POST`**, con las mismas funciones. → integración por estado; una fila por valor de `bailout`.
- **RF33. `GET /api/bets/mine?fight_id=`.** Con sesión, y la misma política de caché. Solo del usuario de la sesión, sin parámetro de usuario. Da `MyFightBets`: `side`, `staked`, `max_stake` (lo que el server aceptaría ahora: `min(tope − staked, saldo)`, y 0 si la ventana no está abierta, si ese valor es menor que `m`, o si la siguiente apuesta daría `bet.too_many_bets` o `bet.pool_full`), `bets` y, si la pelea está liquidada, `result { outcome, payout, net, refund_reason }`. Sin apuestas, listas vacías y 200. No expone `ranked_profit` (TA-28 decide si se muestra). El historial queda fuera. → integración; el usuario B no ve nada de A; un `fight_id` no canónico da 404 `fight.not_found`; `max_stake` da 0 en cada uno de esos casos.
- **RF34. `GET /api/fights/{id}/pool`.** Sin sesión, `FIGHT_READ_IP`, `no-store`. Da `FightPool`: por lado `stake`, `bets` y `payout_bp` (lo que pagaría cada 10 000 monedas con el pozo de ahora, por la misma función de RF10; `null` si algún lado está vacío; satura en `u32::MAX`, sin `as` ni `unwrap`), `rake_bp` (el de la pelea, RF10), `server_time` y, en una liquidada, `settlement { outcome, rake, residue }`. **Pozos públicos durante la ventana:** es lo que pide el intent ("ve cómo se mueve el pozo"), y no filtra el resultado, porque no depende de la semilla y el log no existe hasta el cierre (RF17 de 010). Sale de memoria para la ventana abierta, y de la base para las demás. Sin identidades de ningún apostador. → integración; RF27 y RF41 de 010 con apuestas sembradas: dos peleas con los mismos stakes y semillas distintas dan los mismos pozos.
- **RF35. Pozo en vivo por `schedule`.** Una tarea cada 2 s lee los pozos de las peleas `betting_open`, y si cambiaron, publica en el hub un mensaje `pool_update { pool }`, por el canal `schedule`, al que todo cliente ya está suscrito. Se serializa una vez, con el último valor por socket, sin cola, como RF43 de 010. Una pelea sin apuestas no emite nada, así que RF41 de 010 sigue igual. Pesa menos de 8 KiB. **Por qué no por sondeo:** con 2000 clientes cada 10 s son 200 peticiones por segundo por Traefik y un CGNAT agota `FIGHT_READ_IP`. **Por qué no por `fight:{id}`:** el cliente no sigue ese canal hasta `betting_closed` (`followed-fights.ts`). Tras el cierre, la tarea envía un último valor si cambió, para que nadie se quede con un pozo incompleto. → integración: dos sockets reciben lo mismo; un socket lento salta a lo último; la carga en la base es de una consulta cada 2 s, con o sin espectadores.
- **RF36. Cómo le llega el pago a la UI: relectura.** No hay `SelfEvent` nuevo. Cuando la vista muestra el resultado de una pelea en la que el usuario apostó, y tras un retraso aleatorio de 0 a 3 s, la web vuelve a leer `wallet` y `bets/mine`. Se elige sobre el canal `self` porque:
  - Sirve igual para la CLI, que es otro proceso y no alcanza el `SelfHub`, para una reconexión y para quien entra tarde.
  - No adelanta el resultado: el pago en un mensaje podría llegar antes de lo que RF54 de 010 deja mostrar.
  - El gancho está dentro de la transacción, así que publicar exigiría otro paso tras el `COMMIT`, con una consulta por apostador.
  - `SelfHub::publish` descarta si la cola está llena: serviría como aviso, nunca como dato.

  `ws.rs` corrige su comentario ("The first balance event arrives with TA-27"). → e2e, también para `voided`, con su retraso de hasta 60 s (RF59 de 010).
- **RF37. Límites de lectura.** `wallet` y `bets/mine`: usuario 60/min e IP 600/min. `quote`: usuario 120/min e IP 600/min. Son constantes en `rate/quotas.rs`. **Cada fila es un solo bucket,** como `AUTH_IP` y `FIGHT_READ_IP`: `wallet` y `bets/mine` comparten el suyo, y lo mismo `bonus` y `bailout` con los límites de D7. Un fallo de la base da 503 con `no-store`. → integración.
- **RF47. `GET /api/fights/{id}/quote?side=&amount=`: estimación antes de confirmar.** Con sesión, `private, no-store`. Da `BetQuote { fight_id, side, amount, payout, net, one_sided, rake_bp }`: lo que cobraría esa apuesta si gana su lado, **con el pozo actual más esa misma apuesta**, por la función de RF10. **`payout` y `net` son solo de esa suma**, nunca del total al sumar a una apuesta anterior, que entra solo como parte del pozo. Con el otro lado vacío, `one_sided = true` y `payout = amount`. Sale de la caché de pozos de RF35, sin tocar la base más allá de la sesión. No comprueba topes ni saldo: eso es de `POST`. Una ventana cerrada, y un `{id}` no canónico o inexistente, dan 409 `bet.window_closed`. Un importe inválido da 422 `bet.below_minimum` o `bet.invalid_request`. **La web pide una al cambiar importe o lado, con 300 ms de espera, y nunca por un `pool_update`.**

  Lo que muestra la web:
  - **Texto:** toda estimación dice que, si pierde su lado, pierde lo apostado, y que el `payout` es de esa suma, también al sumar a una apuesta. Con `one_sided`: "si al cierre sigue sin rival, se devuelve; si alguien apuesta al otro lado, puedes ganar o perder lo apostado".
  - **Desactualizada:** un `pool_update` la marca así, con una forma de recalcular y sin petición automática.
  - **Fuera de orden:** no se muestra una respuesta de otro importe o lado.
  - **Sin estimación:** con error, 429 o sin JS, "estimación no disponible", sin bloquear la apuesta.
  - **Mientras escribe:** un 422 `bet.below_minimum` muestra el mínimo como ayuda, no la fila "Rechazada".

  **Por qué en el server:** es lo que `payout_bp` no puede dar, porque 10 contra 10 muestra x1,90, y con una apuesta de 500 se cobra menos; y una copia en JS sería una segunda fuente de verdad del dinero. → integración: vectores de RF10; una apuesta real con el mismo pozo cobra lo estimado; un solo lado; ventana cerrada; `{id}` no canónico e inexistente; `amount = u32::MAX` sin pánico. Vitest de cada caso de arriba, y ninguna petición por `pool_update`. e2e del texto, en la fila de estimación del mapeo.
- **RF38. Códigos, nunca texto.** Enums nuevos en `protocol`, cada uno dueño de su prefijo: `BetErrorCode` (`bet.*`, con `bet.invalid_request`), `BonusErrorCode` (`bonus.*`), `BailoutErrorCode` (`bailout.*`) y `WalletErrorCode` (`wallet.unavailable`). Entran en `ErrorCode` con `ALL` y `as_str`. Los códigos `void.*` son de la CLI y no van en `protocol`. → los tests de solapamiento y exhaustividad que ya existen.

### Capacidad del WebSocket (B)
- **RF39. Reserva de D6.** `reserve = WS_MAX_CONNECTIONS / 5`, en enteros. En el paso 6 del handshake, un socket toma su permiso. Si después quedan menos de `reserve` libres, y es anónimo o de una cuenta en carencia, lo suelta y da 503 `ws.capacity`. Tomar y luego comprobar nunca rebasa el 80 %, y dos que compiten en el borde como mucho fallan los dos. La carencia solo se consulta en ese caso, con 1 s de plazo, así que el camino normal no suma consultas, y si la consulta falla cuenta como carencia. Un socket abierto no se reevalúa. → test 18 (parte de TA-27): con `WS_MAX_CONNECTIONS = 5`, cuatro sockets; el quinto anónimo y el de una cuenta en carencia reciben 503, el de una cuenta fuera de carencia pasa, y al liberar uno, un anónimo sigue sin entrar si no baja del 80 %.

### Configuración (B)
- **RF40. Valores.** `B` sigue siendo `DAILY_GRANT`. Son nuevas `BET_MIN` (`m`, por defecto 25), `BET_RAKE_BP` (`r`, por defecto **0**) y `BET_CAP_MULTIPLE` (por defecto 5), en `Config`. `R = ⌊B / 2⌋`, derivado. Al arrancar se exige `1 ≤ m ≤ R`, para que un rescate siempre permita apostar (con `m = 25`, `B ≥ 50`), `0 ≤ r_bp ≤ 2000` y `1 ≤ m_cap ≤ 20`. Si no, `config.invalid` con el nombre de la variable. El compose de producción no pasa `BET_*`: rigen los valores por defecto (preocupación 12). La CLI de anulación no las lee. → unitarias de `config.rs`; test de arranque.

  **Valores: decisión de quien coordina, a partir de la propuesta de game-designer, pendientes del OK del owner** (gate de B). Reemplazan a la recomendación del intent.

  | Parámetro | Valor | Consecuencia |
  |---|---|---|
  | `B` | 500 | |
  | `R` | 250 (`0.5 B`, fijado por D8) | Hasta 10 apuestas mínimas tras un rescate |
  | `m` | 25 | 30 apuestas mínimas al día para quien va en quiebra |
  | `r` | 0 al inicio, objetivo 5 % (500 pb) | Ver abajo |
  | Tope | `5 B` = 2500, `1 B` = 500 en carencia | |

  **Activar el rake.** No hay cambio automático. El owner lo decide, con un PR que agrega `BET_RAKE_BP` al compose y a `check-compose`, y rige para las peleas anunciadas después (RF10).
  - **Métrica:** `refund_one_sided / (paid + refund_one_sided)` en `pool_settlements`, en 7 días móviles, por debajo del 10 %. La consulta va en `apps/server/README.md`.
  - **Revisión forzosa a los 90 días** de la primera apuesta en producción, aunque la métrica no se cumpla: según game-designer, con `r = 0` la masa de monedas por usuario activo llega a 55 `B` en D90 y a 110 `B` en D180.
  - **Sin cifra de usuarios prometida:** manda la métrica, y la revisión de los 90 días si no llega.
### UI en `apps/web` (C)
- **RF41. Panel en la home.** `BetPanel` en `lib/components/fight/`, con la lógica en `lib/features/bets/`, datos por props y sin importar el renderer, en la misma página `/`. Una columna a 375 px, con objetivos táctiles de al menos 44 px; en escritorio, en la zona del panel que dejó prevista 010. → e2e a 375 px y Vitest de la decisión de estados.
- **RF42. Estados del panel.** Función pura de `(board, wallet, mine, pool, quote, view)`. La pelea objetivo es `current` si está en `betting_open`, si no `next` si lo está, y si no, ninguna. Tres zonas independientes, que pueden convivir:
  - **Bono:** visible siempre que `wallet.bonus_available`, también con saldo y sin pelea objetivo.
  - **Resultado:** el de `last`, si el usuario apostó, **convive con el formulario de la siguiente hasta que esa esté en vivo**, para que un pago no se vea solo unos segundos.
  - **Pelea:** una fila de la tabla de abajo.

  | Estado | Qué muestra |
  |---|---|
  | Sin sesión | El pozo, y un enlace a entrar que vuelve a `/`. Nunca un formulario |
  | Cargando o error de lectura | El pozo si se tiene, y un aviso. Nunca un saldo inventado |
  | Ventana abierta | Lados con su pozo y `payout_bp` como multiplicador; importe con accesos a mínimo y máximo, que vienen del server (`m`, `max_stake`); **para ese importe y lado, el pago estimado de RF47, marcado como estimación que cambia hasta el cierre, con lo que pierde si pierde su lado y sus casos de RF47**; saldo; **aviso antes de apostar de que no se cancela ni se cambia de lado** (RF26); explicación (RF48); progreso de carencia si aplica (RF46) |
  | Un lado vacío | Lo anterior, con el aviso: "si al cierre sigue sin rival, se devuelve; si alguien apuesta al otro lado, puedes ganar o perder lo apostado" |
  | Enviando | Botón ocupado, sin doble envío, misma `client_key` en un reintento |
  | Rechazada | Motivo y salida de la tabla de abajo, **con el importe conservado** y `wallet` y `mine` releídos |
  | Apostado | Su lado y lo apostado; puede subir la apuesta en el mismo lado |
  | Sin monedas (`balance < min_bet`, **siempre antes que "Sin poder sumar"**) | Según `wallet.bailout`, con el bono si está disponible y el reinicio en hora local: `available`, el rescate; `grant_first`, el bono; `not_eligible`, tienes apuestas abiertas, espera a que se liquiden; `already_claimed`, vuelve tras el reinicio. Con su apuesta si la tiene |
  | Sin poder sumar (`balance ≥ min_bet`, `max_stake = 0` y la ventana abierta) | "No puedes sumar más a esta pelea", sin formulario, con su apuesta si la tiene: `bet.pool_full` puede darse sin apuestas propias |
  | `betting_closed` o `live` | Su apuesta en la `current`, también tras recargar a mitad de pelea, sin formulario salvo que otra ventana esté abierta |
  | Detenida | Su apuesta en la `current`, con el aviso de que sigue guardada, sin promesa de hora |

  Resultado: `won` con pago y neto (con neto 0, "ganaste y recuperas lo apostado"), `lost`, `refunded` con su motivo (empate, sin rival, anulada) y `voided_unverifiable`.

  | Código de rechazo | Motivo | Salida |
  |---|---|---|
  | `bet.window_closed` | La ventana cerró | Pasa al estado de la pelea, y la próxima si la hay |
  | `bet.below_minimum` | El mínimo es `m` | Accesos de mínimo |
  | `bet.side_locked` | Ya apostaste a `mine.side`, y solo puedes sumar a ese lado | Preselecciona ese lado |
  | `bet.fight_cap_exceeded` | Con `max_stake > 0`: tu tope es `wallet.fight_cap`, y aún puedes apostar `mine.max_stake`. Con `max_stake = 0`: "No puedes sumar más a esta pelea", nunca "aún puedes apostar 0" | Accesos de máximo, o el estado "Sin poder sumar" |
  | `bet.insufficient_balance` | No te alcanza | Accesos de máximo, bono o rescate |
  | `bet.too_many_bets`, `bet.pool_full` | No se admiten más apuestas | "No puedes sumar más a esta pelea" |
  | `bet.key_reused`, `bet.invalid_request` | Algo falló al enviar | Recarga el panel, con una llave nueva |
  | `bet.unavailable`, `http.rate_limited` | Inténtalo en un momento | Reintentar, con la misma llave |

  → Vitest, una fila por estado y por código; `balance < min_bet` con cada valor de `wallet.bailout`, y `balance ≥ min_bet` en el tope; e2e de los flujos de RF44 y RF48.
- **RF43. La UI no calcula dinero.** Todo importe, tope, estimación y neto viene del server. Hay dos excepciones de presentación, en un solo archivo: la barra de proporción del pozo, con los `stake` tal cual, y mostrar `payout_bp` como multiplicador (dividir entre 10 000 para formatear). La web también formatea con `Intl` (`formatCoins`). → prueba de fuente en `lib/features/bets/` y `BetPanel`: sin operadores aritméticos sobre campos de monedas, salvo ese archivo. Comparar `balance` con `min_bet` para elegir un estado no calcula dinero.
- **RF44. Mutaciones por form action.** `?/bet`, `?/claimBonus` y `?/claimBailout` en `routes/+page.server.ts`, con `lib/server/api/mutations.ts` (ADR-0008 D5, test `no-mutation-in-load`), y `use:enhance`. Sin JS, el formulario también funciona: **`load` genera `client_key` y la deja en un campo oculto, con o sin JS,** así que reenviar el mismo formulario no apuesta dos veces (RF25 fila 3), y solo una lectura nueva da otra llave. `load` suma, solo con sesión, `wallet` y `bets/mine` de la pelea objetivo, de la `current` si es otra y de `last`. Un fallo de esas lecturas no rompe la página (RF53 de 010). El header actualiza el saldo con la misma invalidación. → e2e, también con JS apagado y reenviando el formulario; recarga a mitad de pelea: su apuesta sigue a la vista.
- **RF45. Sin adelantar el resultado.** El panel muestra el resultado y dispara la relectura de RF36 solo cuando la vista muestra el de esa pelea (RF54 de 010, E4-6). → e2e: con un cliente atrasado, ni el panel ni el saldo cambian antes.
- **RF46. Texto y acceso.** Todo en en y es (`check-i18n`), los códigos de RF38 en `ERROR_CODES` y `ERROR_TEXT`, y `ranking_probation_progress` con `days_left` y `fights_left` (nota `proposed` de ADR-0008). El texto de carencia dice "peleas ganadas o perdidas", explica que un reembolso no cuenta y da el tope en monedas. **Nunca usa la palabra "carencia" sin explicarla.** `bailout.not_eligible` dice que hay apuestas abiertas y que hay que esperar a que se liquiden. Tokens de ADR-0009 (`check-tokens`). El resultado va en una región `aria-live`, y los errores se asocian al campo. Sin lenguaje que invite a comprar: son monedas ficticias (ADR-0005). → `check-i18n`, `a11y` y revisión de game-designer de los textos.
- **RF48. Explicación corta.** Visible a 375 px sin salir del panel, en en y es: quienes aciertan se reparten el pozo; la comisión, con `rake_bp` del pozo (y su texto propio con 0); "si al cierre sigue sin rival, se devuelve; si alguien apuesta al otro lado, puedes ganar o perder lo apostado"; no se cancela ni se cambia de lado; son monedas ficticias. → e2e a 375 px en en y es, que comprueba ese texto.

## Fuera de alcance
- **Historial de apuestas** (TA-45). `bets/mine` responde por pelea.
- Temporadas, `season_stats`, leaderboard, `farm-report` y `season close` (TA-28).
- Despliegue.
- Tope propio para el main event.
## Requisitos no funcionales
- **Rendimiento:** con la pelea normal, una apuesta hace 8 o 9 consultas por índice y confirma en ≤ 50 ms en local. El saldo es un `SUM` por `ledger_postings_account_id_idx` (008). Los límites de RF15 y RF35 acotan el resto.
- **Memoria (`mem_limit` de 256 MB):** una caché de pozos con las peleas abiertas, un mensaje serializado una vez y 1,3 MB transitorios al liquidar en el tope. Sin estado nuevo por socket más allá de un número de versión.
- **Seguridad:** nada de datos de otro usuario en las lecturas privadas (test 17 extendido a `bets/mine`). Todas las rutas nuevas entran solas en RF27 de 010 y en el test 1 de ADR-0008. Ningún log nuevo con importes, ids de usuario o de apuesta (ADR-0012 A8): solo `event` y `error_kind`.
- **Determinismo:** el motor no cambia. El reparto es una función pura sin reloj ni azar. Ni `sim` ni `engine` conocen las apuestas.
- **Privacidad:** el aviso ya declara bono, rescate y apuesta entre los eventos con `ip_hash`. Sin cambios en el texto. La retención de 30 días de `auth_events` sigue: `bets` no depende de esas filas.
- **Sin red:** `just check` y `just e2e` no salen de loopback.

## Diseño

### Componentes afectados
| Componente | Cambio | Fase |
|---|---|---|
| `apps/server/migrations/` | Dos migraciones, en este orden: `…_bets` y `…_void_secret_lost`, para que rechazar la enmienda solo quite la segunda (ver "Datos"). Solo aditivas (ADR-0006 D10) | A |
| `apps/server/src/ledger/mod.rs` | RF2 | A, B |
| `apps/server/src/bets/` (nuevo) | `parimutuel` (puro), `settle` (gancho), `api`, `pool` (tarea y caché), `probation` | A, B |
| `apps/server/src/fights/sealed.rs`, `store.rs` | `void_fight`, escritura de `rake_bp`. RF24 de 010 sigue: el secreto solo se toca en `sealed.rs` | A, B |
| `apps/server/src/fights/void_stats.rs` (nuevo) | RF23 | A |
| `apps/server/src/fights/hub.rs`, `ws/session.rs` | Último `pool_update`, por `schedule` | B |
| `apps/server/src/{cli.rs,admin.rs}` | RF16 | A |
| `apps/server/src/ws/handshake.rs`, `ws/mod.rs` | RF39 | B |
| `apps/server/src/{app.rs,config.rs,http/routes.rs,rate/quotas.rs}` | Cableado, RF40, rutas, cuotas | A, B |
| `apps/server/src/fights/api.rs` | **Sin cambios** (preocupación 11) | |
| `crates/protocol` | `VoidReason::SecretLost`, su `parse` y `VoidStats` (A); el resto de tipos (B). `just gen` | A, B |
| `apps/web` | `view-state`, resultado no verificable (A); solo los códigos nuevos y que el socket ignore `pool_update` (B); panel, lecturas, acciones, mensajes, e2e (C) | A, B, C |
| `crates/engine`, `crates/content`, `crates/sim` | Sin cambios | |

### Datos
```sql
-- A: ledger (RF1, RF3)
ALTER TABLE ledger_entries ADD COLUMN fight_id int NULL REFERENCES fights,
                           ADD COLUMN bet_id uuid NULL;  -- FK compuesta tras crear bets; CHECK de RF1
-- A: el rake de la pelea (RF10). Inmutable: entra en la lista de RF4 de 010. B lo escribe al crear
-- la fila; hasta entonces y en filas anteriores, 0.
ALTER TABLE fights ADD COLUMN rake_bp smallint NOT NULL DEFAULT 0
    CONSTRAINT fights_rake_bp_ck CHECK (rake_bp BETWEEN 0 AND 2000);

CREATE TABLE bets (
    id uuid PRIMARY KEY,
    fight_id int NOT NULL REFERENCES fights,
    user_id uuid NOT NULL REFERENCES users,
    side text NOT NULL CHECK (side IN ('a', 'b')),
    stake bigint NOT NULL CHECK (stake > 0),
    ranked_stake boolean NOT NULL,       -- la foto de D8 (RF5)
    client_key uuid NOT NULL,            -- RF25 fila 3
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, client_key),
    UNIQUE (id, user_id, fight_id)       -- destino de la FK compuesta de RF1
);                                       -- índices: (fight_id, user_id), (user_id, fight_id)

CREATE TABLE bet_settlements (
    bet_id uuid PRIMARY KEY REFERENCES bets,
    fight_id int NOT NULL REFERENCES fights,
    user_id uuid NOT NULL REFERENCES users,
    outcome text NOT NULL CHECK (outcome IN ('won', 'lost', 'refunded')),
    payout bigint NOT NULL CHECK (payout >= 0),
    net bigint NOT NULL,
    ranked_profit bigint NOT NULL,       -- RF11. TA-28 suma desde aquí
    CHECK (ranked_profit <= net),
    CHECK (outcome <> 'refunded' OR (net = 0 AND ranked_profit = 0)),
    CHECK (outcome <> 'lost' OR (payout = 0 AND ranked_profit = net))
);                                       -- índice: (user_id, fight_id) WHERE outcome <> 'refunded'

CREATE TABLE pool_settlements (
    fight_id int PRIMARY KEY REFERENCES fights,  -- idempotencia (RF8)
    outcome text NOT NULL CHECK (outcome IN ('paid', 'refund_one_sided', 'refund_draw',
                                             'refund_voided', 'refund_cancelled')),
    pool_a bigint NOT NULL, pool_b bigint NOT NULL,
    rake_bp int NOT NULL, rake bigint NOT NULL, residue bigint NOT NULL,
    settled_at timestamptz NOT NULL DEFAULT now(),  -- para la métrica de RF40
    CHECK (outcome = 'paid' OR (rake_bp = 0 AND rake = 0 AND residue = 0))
);
-- `auth_events`: `auth_events_kind_ck` suma 'bailout' y 'bet', y una columna `bet_id` nula.
```
Las tres tablas son solo inserción (`ledger_immutable`). En la migración, cada `CHECK` y `UNIQUE` que puede fallar lleva nombre fijo (ADR-0012 A9). Códigos nuevos: `TA007` y `TA008` en `db::error_kind`. **`settled` implica liquidada** (RF31 de 010), así que "abierta" se lee del estado de la pelea, sin columna ni fila de pendiente.

### Tipos (`crates/protocol`)
```rust
pub struct CoinAmount(i64);   // se exporta como `number`, acotado a MAX_SAFE_INTEGER (preocupación 13)
pub enum VoidReason { SnapshotInvalid, EngineError, SecretLost }
pub struct VoidStats { voided: u32, secret_lost: u32 }
pub struct BetRequest { fight_id: u32, side: Side, amount: u32, client_key: String }
pub struct BetPlaced { wallet: Wallet, mine: MyFightBets }
pub enum BailoutState { Available, GrantFirst, NotEligible, AlreadyClaimed }
pub struct Probation { days_left: u16, fights_left: u16 }
pub struct Wallet { balance: CoinAmount, open_stakes: CoinAmount, bonus_available: bool,
                    bailout: BailoutState, bonus_amount: CoinAmount, bailout_amount: CoinAmount,
                    min_bet: CoinAmount, fight_cap: CoinAmount, day_resets_at: TimestampMs,
                    probation: Option<Probation> }
pub enum BetOutcome { Won, Lost, Refunded }
pub enum RefundReason { OneSided, Draw, Voided, Cancelled }
pub struct BetView { id: String, side: Side, stake: CoinAmount, placed_at: TimestampMs }
pub struct MyFightResult { outcome: BetOutcome, payout: CoinAmount, net: CoinAmount,
                           refund_reason: Option<RefundReason> }
pub struct MyFightBets { fight_id: u32, side: Option<Side>, staked: CoinAmount, max_stake: CoinAmount,
                         bets: Vec<BetView>, result: Option<MyFightResult> }
pub struct BetQuote { fight_id: u32, side: Side, amount: CoinAmount, payout: CoinAmount, net: CoinAmount,
                      one_sided: bool, rake_bp: u16 }
pub struct PoolSide { stake: CoinAmount, bets: u32, payout_bp: Option<u32> }
pub struct PoolSettlement { outcome: PoolOutcome, rake: CoinAmount, residue: CoinAmount }
pub struct FightPool { fight_id: u32, a: PoolSide, b: PoolSide, rake_bp: u16,
                       server_time: TimestampMs, settlement: Option<PoolSettlement> }
// ServerMessage suma: PoolUpdate { pool: FightPool }
```
Sin texto para humanos. Todas las apuestas de un usuario en una pelea son del mismo lado (RF26), así que `result` es uno solo por pelea.

### API
| Método y ruta | Sesión | Efecto | Respuesta | Errores |
|---|---|---|---|---|
| `POST /api/bets` | Sí | Mutate | 201 `BetPlaced`, 200 si se repite la llave | RF25 |
| `POST /api/bonus/claim` | Sí | Mutate | 200 `Wallet` | `bonus.already_claimed` |
| `POST /api/bailout/claim` | Sí | Mutate | 200 `Wallet` | `bailout.grant_available`, `bailout.not_eligible`, `bailout.already_claimed` |
| `GET /api/wallet` | Sí | Read | 200 `Wallet` | 401, 503 `wallet.unavailable` |
| `GET /api/bets/mine?fight_id=` | Sí | Read | 200 `MyFightBets` | 401, 404 `fight.not_found`, 503 `bet.unavailable` |
| `GET /api/fights/{id}/quote?side=&amount=` | Sí | Read | 200 `BetQuote` | 401, `bet.window_closed`, `bet.below_minimum`, `bet.invalid_request` |
| `GET /api/fights/{id}/pool` | No | Read | 200 `FightPool` | 404 `fight.not_found`, 503 `fight.unavailable` |
| `GET /api/fights/void-stats` | No | Read | 200 `VoidStats` | 503 `fight.unavailable` |

Por WS, solo `pool_update` en `schedule`. Ninguna acción viaja por WS (D6).

### Flujo
```
 navegador         web (form action)       server                         Postgres
    │ POST ?/bet        │ POST /api/bets      │ BEGIN, lock_timeout 1s         │
    ├──────────────────▶├────────────────────▶│ usuario FOR UPDATE             │
    │                   │                     │ pelea FOR SHARE (RF29 de 010)  │
    │                   │                     │ topes, saldo                   │
    │◀── wallet, mine ──┤◀────── 201 ─────────┤ bet + bet_stake + auth_events  │ COMMIT
    │                                         │
    │ pool_update (schedule, cada 2 s si cambió)
    │◀────────────────────────────────────────┤
    │                                     settle_at: BEGIN
    │                                         │ final, reveal, DELETE secreto
    │                                         │ BetsHook: pool_settlements (idempotencia)
    │                                         │   reparto puro → bet_payout, rake, bet_settlements
    │ schedule_update (final)                 │ COMMIT
    │◀────────────────────────────────────────┤
    │ la vista muestra el resultado → 0–3 s → relee wallet y bets/mine
```

### UI
Una columna a 375 px: escenario o anuncio, y debajo el panel. En escritorio, el panel va en la segunda zona. Cargando, vacío y error en RF42. Los accesos de mínimo y máximo evitan teclear en el teléfono. El texto de "ganaste y recuperas lo apostado" es propio, porque neto 0 se puede alcanzar (preocupación 2). La explicación de RF48 se despliega en el panel, sin navegar. El panel no empuja la vista en vivo mientras la pelea corre.

## Áreas de preocupación
1. **Pozo de un solo lado: se reembolsa, sin rake. Decidido, y roza a D8.** Nadie puede cobrar si nadie apostó al ganador, la casa no puede pagar ni debe quedarse esas monedas, y si solo apostaron al ganador, cobrar el rake sería una pérdida por acertar. **Encaje con D8:** su párrafo "Por qué `min`" advierte contra "cobrar el rake solo del lado perdedor **y** obligar a reembolsar". Aquí la base sigue siendo todo el pozo, pero con el tope de RF10 el rake sale en la práctica del lado perdedor, así que nos acercamos a lo que ese párrafo evitaba. Por eso va una nota `proposed` en ADR-0008, en vez de darlo por compatible. La regla `min` se conserva y se prueba en la función.

   **Costo medido por game-designer, y ningún parámetro lo cambia:** con N = 20 usuarios, el 77 % de los pozos con apuestas tiene un lado vacío y se reembolsa el 62 % de las apuestas. En horas valle, el 94 % de los pozos. Con la actividad a la mitad, entre el 77 y el 79 % de los casuales sigue en carencia en D30. Para ver un pago en local hacen falta dos cuentas. product-owner y el owner deben aceptarlo.
2. **Tope del rake: nota `proposed` en ADR-0008, pendiente del owner.** Decisión de quien coordina: `rake = min(⌊r·T⌋, L)`, y acertar nunca hace perder monedas. Sin el tope, 500 contra 10 con 5 % pagaba 485 (neto −15), y un apostador podía poner `m` en el lado vacío al último segundo para que los demás ganaran perdiendo. Con el tope, ese caso da rake 10, pago 500 y neto 0. **Se paga con:** el test 25(d) deja de tener camino real, así que se prueba en la función, como 25(e). En simulación, sin usuarios reales, el sumidero como parte de las monedas que entran casi no cambia con `r = 5 %`: 38,4 % con tope y 38,2 % sin él, con N = 200. Con `r = 0` no aplica hasta activar el rake. Falta decidir si la nota basta o si el owner quiere reescribir D8.
3. **Un solo lado por usuario y pelea: no está en D8.** Lo agrega esta spec (RF26), y es obligatorio: con `r = 0`, cubrirse cuesta 0 en cualquier pozo, y una cuenta sola maduraría contra sí misma sin riesgo. **No evita dos cuentas**, que es la granja que D8 ya acepta y `farm-report` revisa.
4. **Carencia contada por peleas: nota `proposed` en ADR-0008, pendiente del owner.** D8 dice "10 apuestas liquidadas". Con varias apuestas por pelea (test 24, "ni con varias apuestas"), diez de `m` en una sola pelea cumplirían ese requisito en una sola ventana, y solo quedarían los 7 días. Se cuenta `DISTINCT fight_id`, y `Probation` da `fights_left`. El test 24 pasa con 9 peleas. Es más estricto, y agrava el costo de la preocupación 1: las peleas reembolsadas tampoco cuentan. **Si el owner rechaza una de las dos notas, se vuelve a la letra de D8 y la spec vuelve a revisión.**
5. **Enmienda de ADR-0002: bloquea el merge de A.** Está escrita en el ADR como `proposed`, sin aplicar. Si el owner la rechaza, salen RF19 a RF23 y la fila 8 de RF17 (la negativa pasa a ser `void.secret_missing`), y sigue valiendo todo lo demás de A: el resto de la feature no depende de ella. Riesgo que se le muestra: una anulación sin semilla no se puede refutar nunca y saca una pérdida del ranking.
6. **Las preocupaciones 1, 3 y 13 de 010 se vuelven reales con la primera apuesta.** Quien tiene `DATABASE_URL` puede leer `fight_secrets` durante la ventana y apostar, también con una cuenta propia, sin que nada lo detecte. Se aceptó por delegación el 2026-10-31, y security-reviewer pidió que el owner lo decidiera antes de la primera apuesta. **El gate de B exige ese OK explícito del owner antes de la primera apuesta.** Sigue en firme la cláusula de revisar si otra persona obtiene la credencial.
7. **Declaraciones, no pruebas.** `--previous-binary` y `--backup-checked` quedan en `admin_audit`, pero nada comprueba que sean ciertas, y "reabrir la aceptación" es un paso humano (RF24). Con un solo operador es lo que se puede exigir.
8. **El `FOR UPDATE` de usuario y el gancho se esperan entre sí. Se fija `FOR UPDATE`.** Las FK de `ledger_entries.user_id` toman `FOR KEY SHARE`, que conflictúa con él, en los dos sentidos. Un gancho que espera tiene `lock_timeout` de 2 s y reintenta, y solo 60 s de fallos llevan a detención. Mientras el gancho no confirma, retiene ese `FOR KEY SHARE`, así que la siguiente apuesta o reclamo de esos usuarios puede agotar su 1 s y dar 503: hasta 10 s en el tope de RF15, y con los valores por defecto ninguna ventana se abre en ese rato. **`FOR NO KEY UPDATE` quitaría la espera y por eso no se usa:** dejaría pasar un rescate con un pago en camino (RF30).
9. **Tiempo del gancho frente a detener el vivo.** Un gancho lento detiene todo (preocupación 6 de 010), y `void` no es salida, porque la pelea está sana. RF15 lo acota con topes, medición y README, pero la cifra en el VPS de 1 vCPU no existe hasta TA-8, donde su criterio es bloqueante. Un tope global da `bet.pool_full`, un estado visible que espero no ver con estos usuarios.
10. **Pozos públicos y apuestas de último segundo.** Quien espera al cierre ve el pozo casi final con ventaja. Es normal en un parimutuel y no toca el resultado. El ataque de poner `m` en el lado vacío ya no hace perder a los demás (preocupación 2), pero **sigue siendo una opción barata:** por `m`, convierte un reembolso en un pozo real y, si acierta, se lleva casi todo el otro lado. Quienes estaban en ese lado arriesgan lo que creían que se devolvería, y la UI lo avisa como estimación, no como promesa. Un total de un solo apostador deja deducir cuánto apostó, sin saber quién.
11. **Otra sesión trabaja en repeticiones (`ta-44-repeticiones`) y puede tocar `fights/api.rs`.** Esta spec no toca ese archivo: `detail_of` ya da `seed: null` a una `secret_lost`, y las rutas nuevas viven en `bets/api.rs` y `fights/void_stats.rs`. Pueden chocar en `http/routes.rs` (una entrada por ruta, fácil de resolver) y, si repeticiones cambia `FightDetail`, en RF21. Quien abra su PR después hace rebase. Revisar que `FightDetail` siga dando `seed: null` en una `secret_lost` después de ese merge.
12. **Los valores son configuración, pero el compose está cerrado.** `check-compose` fija las variables de `server` y el compose no pasa `BET_*`, así que en producción rigen los valores por defecto del binario, y activar el rake pide un PR (RF40). `DAILY_GRANT` sí va en el compose. **Para el plan B:** pasa a 500 en `infra/docker-compose.yml:37` y en `apps/server/.env.example:24`, y cambian el test de `apps/server/src/config.rs:880` y las pruebas de arranque con `DAILY_GRANT < 50`, que dejan de arrancar por `m ≤ R` con `m = 25`.
13. **Otra excepción de 64 bits.** `lib.rs` dice que `Me.balance` es la única y un test lo vigila. `CoinAmount` amplía esa excepción con un solo tipo, con la misma cota. La alternativa es enviar monedas como cadena, como `TimestampMs`, que complica un dato que siempre se muestra. El plan B cambia ese test, sin quitarlo, y `Me.balance` pasa a usar el mismo tipo si no rompe el contrato.
14. **Reserva en el arranque.** Con la reserva activa y sin cuentas maduras, el tope útil es el 80 % para todos, algo que la nota del 2026-10-28 de ADR-0008 ya anticipa. `fights_fanout.rs`, con 2000 sockets anónimos, necesita subir `WS_MAX_CONNECTIONS` para medir lo mismo, y los tests de capacidad con un tope chico cambian según `tope / 5`.
15. **e2e con ventana corta.** `_e2e-fights-run` usa una ventana de 5 s, poca para que dos cuentas apuesten sin fallos intermitentes. El plan C propone subirla, con cuidado de no pasar del presupuesto de la preocupación 9 de 010, o un proyecto aparte.
16. **Ancho de banda de `pool_update`.** Con 2000 sockets son 200 KB cada 2 s como máximo, solo durante la ventana y si el pozo cambió. Entra en el presupuesto, pero se suma a la medición de TA-8.
17. **Con `r = 0`, baja el costo de una granja.** Sin rake, la inyección de alts maduras no se paga con sumidero, y la fórmula de riesgo residual de D8 sube a `N · (B + R)`. Con `B = 500`, el valor absoluto también sube, sin cambiar su proporción frente a un jugador honesto. Hay que avisarlo en TA-28, cuyo `farm-report` lo revisa.
18. **Borrado de cuenta, a futuro.** El aviso de privacidad promete conservar movimientos y apuestas seudonimizados. `bets` y `bet_settlements` tienen FK a `users`, y son solo inserción, igual que el ledger, así que esa feature tendrá que anonimizar la fila de `users`, no borrarla.
19. **Estimación y caché.** `BetQuote` sale de una caché de hasta 2 s, y el pozo cambia hasta el cierre. Por eso va marcada como estimación.
20. **Riesgo para TA-28: un leaderboard casi vacío.** Con el 77 % de los pozos con un lado vacío y hasta el 79 % de los casuales en carencia en D30 (preocupación 1), casi nadie saldría en el leaderboard con poca población. Consta aquí para que TA-28 lo decida.
21. **El importe de un asiento de apuesta: `bet_stake` se ata en la base, `refund` y `bet_payout` no.** security-reviewer aceptó esto para A con dos condiciones para B (ronda 2 del PR #86), y B las cumple.
    - **`bet_stake` se ata (RF2, RF25).** Su importe es exactamente `bets.stake`, no depende del pozo y no va en la transacción del reveal. Y es el que importa: `BetsHook` paga `bets.stake` sin mirar el asiento, así que una ruta con un bug que guarde 1000 y asiente 1 vaciaría la custodia **por la liquidación legítima, sin SQL directo.** La regla no atrapa una apuesta sin `bet_stake`: eso lo cubren la transacción única y la prueba de RF25, y lo detecta la consulta de RF13.
    - **`refund = stake` no entra, tampoco en B.** Con SQL directo, el dueño de una apuesta de 1 aún puede sacar 1000 de `bet_escrow`. La regla no frenaría a quien puede hacerlo, porque `DATABASE_URL` también apaga triggers (preocupaciones 5 y 6). Costaría una lectura de `bets` por movimiento en la transacción del reveal, justo lo que RF15 acota. Y no cerraría la custodia, porque `bet_payout` no admite una regla igual: depende de todo el pozo. Como barrera contra un error tampoco hace falta: el importe sale de `bets.stake`, por una sola ruta, y RF9 y RF13 lo prueban.
    - **Detección (RF13).** La consulta del README y el cuadre en memoria del gancho. El abuso se detecta solo si alguien corre la consulta: nada la programa, y quien tiene la credencial también podría ocultarlo. Programarla es trabajo de TA-8, si el owner lo pide.

    Riesgo sobre monedas ficticias y el ranking (ADR-0005), que entra en el OK del owner de la preocupación 6, en el gate de B. **Se revisa** si la app pasa a un rol de base sin permiso para apagar triggers, o si aparece otra ruta que escriba estos asientos.

## Respuestas a preguntas abiertas del intent
1. **Valores de `B`, `m` y `r`** → RF40: 500, 25, y 0 al inicio con 5 % de objetivo. `R = 250`, tope de 2500 y 500. Decidido por quien coordina a partir de la propuesta de game-designer, **pendiente del OK del owner.**
2. **`season_stats`** → TA-28, ya decidido. Aquí, RF6 y RF11.
3. **Pozo de un solo lado** → reembolso sin rake (RF9, preocupación 1). 25(e) se prueba sobre la función.

Lo que architect arrastró del intent:
- **Probar el binario anterior y reabrir la aceptación** → RF24.
- **Plazo de `secret_lost` en `betting_closed`** → RF19: rige `live_at`.
- **Residuos** → RF10 y RF12: al sumidero. Nunca a la casa como ganancia, porque se destruyen, y siempre con asiento.
- **`lock_timeout` de RF29** → RF25: 1 s.
- **RF7, RF9, RF25 y el glosario** → RF20, RF21 y "Trabajo de cierre".

## Estrategia de pruebas

### Mapeo de los criterios del intent
| Criterio bloqueante | RF | Prueba | Nivel | Fase |
|---|---|---|---|---|
| Del owner: apostar, ver pago o reembolso, bono y rescate en local | RF25, RF29, RF30, RF36, RF42, RF44 | Dos cuentas en lados opuestos, pelea forzada con `dev-fights`, y prueba a mano en `just dev` en el PR | e2e, manual | C |
| Test 7 | RF29, RF30 | 20 `POST` concurrentes dan exactamente un `grant`, e igual con `bailout` | integración | B |
| Test 8 | RF30 | Apuesta abierta que deja el saldo libre en 0: `bailout.not_eligible` | integración | B |
| Test 18, parte de TA-27 | RF39 | Reserva con el 80 % ocupado, por clase de socket | integración | B |
| Test 23 | RF25, RF27 | Dos `POST` concurrentes bajo el tope que juntos lo pasan: se acepta exactamente uno | integración | B |
| Test 24 | RF27 | Carencia no pasa de `1 B`, ni con varias apuestas. Con 7 días, 9 peleas ganadas o perdidas y varios reembolsos, sigue en carencia (nota `proposed` de ADR-0008) | integración | B |
| Test 26 | RF30 | Sin el bono del día: `bailout.grant_available` | integración | B |
| `ranked_stake` es una foto | RF5 | Cuenta en carencia y fuera de ella | integración | B |
| Test 25(a) | RF11 | Lado perdedor solo con `false`: el ganador suma 0 | integración | B |
| Test 25(b) | RF11 | `L_c = L / 2`: suma `⌊d / 2⌋`, con `d` impar | integración, unitaria | B |
| Test 25(c) | RF5, RF11 | Sale de la carencia entre apuesta y liquidación: sigue `false` | integración | B |
| Test 25(d) | RF11 | Sobre la función aislada, con `payout < stake` de entrada, porque RF10 no deja ese caso: baja lo mismo que `net`. Por la liquidación, 500 contra 10 con 500 pb da rake 10, pago 500 y neto 0 | unitaria, integración | B |
| Test 25(e) | RF11 | `L = 0` sin reembolso, sobre la función aislada | unitaria | B |
| Test 25(f) | RF9 | Empate real, y cancelada llamando al gancho con ese final, porque no hay camino real (RF33 de 010): reembolso, 0, no cuenta | integración | A, B |
| Test 25(g) | RF6, RF11 | Propiedad, y `CHECK` en la base | propiedad, integración | B |
| Test 25, "reliquidar no cambia nada" | RF8 | Dos llamadas al gancho | integración | B |
| Reembolso real de `draw`, anulada y un lado vacío; sin `RankedProfit` ni apuesta liquidada | RF9, RF27 | Asientos `refund` y conteo de carencia sin cambios | integración | A, B |
| RF31: sin fallos alcanzables; cuenta baneada, topes, un solo lado | RF14, RF15 | Ver RF14 | integración | B |
| RF59 completo antes de la primera apuesta | RF16 a RF18 | Cada negativa sin escribir, cada razón, reembolso, canarios en logs | integración | A |
| `log_without_end` se niega | RF17 fila 12 | Un `compute` de prueba que devuelve `NoEnd` | integración | A |
| Probar antes el binario anterior | RF17 fila 13, RF24 | Negativa sin bandera, y la bandera en `admin_audit` | integración, README | A |
| Enmienda de ADR-0002 aceptada antes del merge | Preocupación 5 | Se comprueba en el ADR y en el chat del owner | manual, gate | A |
| `secret_lost` solo sin la fila del secreto | RF17 filas 5 a 8, RF20 | Comando y guard con SQL directo | integración | A |
| Se niega mientras pueda haber emisión | RF19, RF20 | Antes y después del plazo, en `live` y `betting_closed` | integración | A |
| Contador público y reabrir la aceptación | RF23, RF24 | Sube con cada anulación; línea impresa; README | integración, revisión | A |
| "No verificable" propio en RF7 y en la vista | RF21, RF22 | Cuatro resultados; semilla nula de otra razón da `invalid`; vista | integración, Vitest, e2e | A |
| Solo tras las negativas | RF17 filas 4 y 5 | Versión ausente y secreto ausente: gana la versión. Secreto distinto: nunca `secret_lost` | integración | A |

### Mapeo de lo que sumó la revisión
| Tema | RF | Prueba | Nivel | Fase |
|---|---|---|---|---|
| Rake por pelea, inmutable | RF10 | `UPDATE fights SET rake_bp` con SQL directo da `TA006` en cada estado no `settled`, con todas las migraciones aplicadas | integración | A |
| Rake de la apertura al pagar | RF10 | Cambiar la configuración antes de liquidar | integración | B |
| Barreras del ledger | RF1, RF2 | Un caso rechazado por regla | integración | A |
| `bet_stake` atado a `bets.stake` | RF2, RF25, preocupación 21 | Otro importe u otra cuenta dan `TA008`; apuesta y asiento en una transacción | integración | B |
| Custodia comprobable en producción | RF13, preocupación 21 | La consulta del README, limpia y tras romperla; cuadre del gancho | integración, unitaria | B |
| Hook lento | RF14, RF15 | `attempt_timeout` de prueba; medición con pool real | integración, `#[ignore]` | B |
| `decided` en fase A | RF7 | `Err`, sin `settled` | integración | A |
| Reclamos y rescate contra liquidación | RF29, RF30 | `banned_writes.rs`, 503, a ambos lados del `COMMIT` | integración | B |
| Reloj, cuerpo y rangos | RF25 | Usuario retenido cruzando `closes_at`; `bet.invalid_request` | integración | B |
| Sin monedas frente a sin poder sumar | RF33, RF42 | `max_stake = 0`, por causa; `balance < min_bet` con cada `wallet.bailout`; `balance ≥ min_bet` en el tope | integración, Vitest | B, C |
| Estimación antes de confirmar | RF42, RF47 | Lo estimado es lo cobrado con el mismo pozo; Vitest de desactualizada, fuera de orden, no disponible y mínimo; e2e a 375 px en es y en, que comprueba el texto de perder y de sin rival | integración, e2e | B, C |
| Rechazo | RF42, RF46 | Cada código en Vitest; e2e de `bet.fight_cap_exceeded` y `bet.side_locked` a 375 px en es y en, con el importe conservado | Vitest, e2e | C |
| Carencia | RF27, RF46 | Texto con peleas, reembolso y tope; e2e a 375 px en es y en | e2e | C |
| Bono con saldo y sin pelea objetivo | RF42 | e2e a 375 px en es y en | e2e | C |
| Recarga a mitad de pelea | RF42, RF44 | Su apuesta sigue a la vista en `live` y en detenida | e2e | C |
| Resultado junto a la siguiente | RF42 | `last` junto al formulario, hasta que la siguiente esté en vivo | Vitest, e2e | C |
| Explicación | RF48 | Visible sin salir del panel | e2e | C |
| Sin JS, sin doble apuesta | RF44 | Reenviar el formulario | e2e | C |

### Otras
- **Propiedad:** RF10, RF11, RF12 y RF13.
- **Barreras de fuente:** RF14, RF24 de 010 sin cambios, RF43, y que el server no lea `banned_at` al liquidar.
- **Integración de fugas:** RF27, RF28 y RF41 de 010, con apuestas sembradas.
- **e2e:** en y es, 375 px, Chromium y WebKit, con y sin sesión: panel, apuesta, estimación, rechazo, carencia, resultado de victoria, empate y un lado vacío, rescate y bono, también con saldo. Sin red.
- **Carga:** RF15 con `#[ignore]`, reportada en el PR de B.

## Alternativas consideradas
- **Pago por `self` en vez de relectura.** Sería algo más rápido, pero tiene dos caminos, uno de ellos fuera del proceso, y puede adelantar el resultado (RF36).
- **Pozos ocultos hasta el cierre.** Evitan apostar por información de último segundo, pero van contra el intent y quitan el interés de ver el pozo.
- **Sondeo o `fight:{id}` para el pozo.** Ver RF35.
- **Rake sin tope.** Es la letra de D8 y mantiene un camino real para 25(d), pero se puede ganar y perder monedas, y un apostador de último segundo lo provoca (preocupación 2).
- **Rake solo del lado perdedor.** Se aparta de "% del pozo", mientras que el tope solo actúa cuando `L` es chico.
- **`r` de la configuración al liquidar.** Más simple, pero un cambio pagaría distinto de lo que se publicó al apostar.
- **`rake_bp` en una tabla aparte.** No toca `fights_guard`, pero exige otra fila en la transacción de la apertura. `fights` ya tiene la regla de inmutabilidad.
- **Activar el rake solo, al cumplirse la métrica.** Cambiaría la economía sin una decisión.
- **Estimación en JS con vectores compartidos.** Sin petición y al instante, pero duplica la lógica del dinero y pide una excepción a RF43. Se elige `quote` (RF47).
- **Redondeo hacia arriba del rake.** Puede pasar de `r`. Hacia abajo, nunca.
- **Residuo a los ganadores por mayor resto.** No se destruye ni una moneda de más, pero necesita desempate y premia ciertas divisiones del stake. Hacia el sumidero es más simple y demuestra RF12. Se pierde menos de 1 moneda por apuesta ganadora.
- **Cuenta de custodia por pelea.** Da un invariante por pelea, a costa de una fila por pelea en una tabla inmutable. `fight_id` en los asientos da lo mismo con una consulta.
- **Una columna `status` en `bets`.** Rompe "solo inserción" y abre el riesgo de reescribir un resultado.
- **Un pago por usuario en vez de por apuesta.** Menos filas, pero el test 25 y `RankedProfit` son por apuesta, y `BETS_MAX_PER_USER_PER_FIGHT` ya acota las filas.
- **Elegir la razón de la anulación.** RF59 de 010 lo prohíbe: quien corre el comando no debe poder escoger la que no se puede refutar.
- **Dejar `secret_lost` para otra feature.** Dejaría a la primera pelea con el secreto perdido sin salida, con apuestas ya en juego.

## Trabajo de cierre (fase C)
No se edita antes de aprobar y solo cuando la pieza exista:
- `docs/architecture.md`: tablas (`bets`, `bet_settlements`, `pool_settlements`), cuentas `bet_escrow` y `rake_sink`, `pool_update`, CLI de anulación y la nueva fila de prerrequisitos de TA-8 (RF15). La fase A registra su parte.
- `docs/glossary.md`: `Anulada` (salvo `secret_lost`, no revela semilla si no la hay), `Anulada no verificable`, `Pozo reembolsado`. `Comisión` (`Rake`): "% del pozo, con tope en el lado perdedor", y el residuo también va al sumidero. `Carencia`: "peleas" en vez de "apuestas", tras el OK del owner.
- Spec de 010: notas en RF7, RF9, RF25 y RF59 que apunten aquí. Se cierra la preocupación 13.
- ADR-0002: marcar la enmienda como aprobada, solo tras el gate (fase A). ADR-0008: marcar las notas de rake y carencia como aprobadas, solo tras el OK del owner (fase B).
- `CLAUDE.md`: `BET_*`, `server fight void`. `apps/server/.env.example`. `apps/server/README.md`: procedimiento de RF24. `REVIEW.md`: una liquidación nueva nunca lee `users`. `apps/server/README.md` lleva también la consulta de RF40, la salida del atasco de RF15 y la prohibición de RF7.
- `docs/architecture.md`: prerrequisito numérico de RF15 en TA-8 (≤ 10 s en el VPS).
- Linear: comprobar en TA-28 que recoge `bet_settlements.ranked_profit` como origen y el aviso de la preocupación 17, cerrar los criterios delegados de TA-13, y dejar con fecha la revisión de los 90 días de RF40.

## Revisión
Las tres revisiones son de 2026-11-02, sobre la versión anterior de esta spec y la enmienda de ADR-0002. Las registra architect tal como llegaron de quien coordina: los revisores no editaron este documento. Sin aprobaciones todavía.

### game-designer — 2026-11-02 — REQUEST_CHANGES
- G1. Valores y sus derivados en RF40, preocupación 12, respuestas y archivos del plan B. Quitar "un pozo paga rake desde 20 monedas".
- G2. Rake por pelea, inmutable desde la apertura, en lo que usan `FightPool.rake_bp`, `payout_bp` y el gancho.
- G3. `BET_RAKE_BP` por defecto 0, sin compose ni cambio automático; activación por decisión del owner; métrica de un lado vacío bajo el 10 % en 7 días; revisión forzosa a los 90 días (`r = 0` da 55 `B` en D90 y 110 `B` en D180), sin prometer una cifra de usuarios.
- G4. Con el tope del rake: `payout_i ≥ stake_i`; 25(d) sobre la función; sin la rama de pago 0; texto de neto 0; glosario.
- G5. `max_stake` da 0 si no alcanza `m`, o con `bet.too_many_bets` o `bet.pool_full`, y su estado en RF42.
- G6. `fights_left` en vez de `bets_left`, con nota en ADR-0008.
- G7. RF26 es obligatorio: con `r = 0`, cubrirse cuesta 0; avisar antes de apostar.
- G8. Mostrar el costo de la preocupación 1: con N = 20, 77 % de los pozos con un lado vacío y 62 % de las apuestas reembolsadas; horas valle, 94 %; con actividad a la mitad, 77-79 % de los casuales en carencia en D30.
- No bloqueantes: `MainEvent` no existe; `payout_bp` satura; avisar a TA-28.

### product-owner — 2026-11-02 — REQUEST_CHANGES
- P1. Antes de confirmar, pago estimado para ese importe y lado, con su apuesta incluida y marcado como estimación, sin una petición por `pool_update`. RF43 debe permitir el multiplicador.
- P2. Explicación corta a 375 px sin salir del panel, en en y es.
- P3. Fila "Rechazada" con motivo y salida de cada código, importe conservado y relectura. Texto de `bailout.not_eligible`.
- P4. Texto de carencia con "peleas ganadas o perdidas", reembolsos y tope; glosario.
- P5. Bono visible siempre que esté disponible, con e2e.
- P6. En `betting_closed`, `live` y detenida se lee también la `current`; probar recarga a mitad de pelea.
- P7. El resultado de `last` convive con el formulario de la siguiente hasta que esté en vivo.
- P8. e2e a 375 px en es y en: rechazo, carencia, estimación, bono con saldo; mapear los RF nuevos. "Fuera de alcance" con el historial.

### security-reviewer — 2026-11-02 — REQUEST_CHANGES
- S1. `bet_id` y `fight_id` nulos evaden RF3: `CHECK` y barrera en la base para cada cuenta, en lugar de la prueba de fuente.
- S2. Un hook lento es un fallo alcanzable: medición con `COMMIT`, pool real, concurrencia y backend cancelado; prerrequisito numérico de TA-8; prueba del tiempo límite; README con la salida.
- S3. En fase A, un `decided` con dos lados da `Err`; README prohíbe volver a `NoBets` con apuestas sin liquidar.
- S4. Reclamos con `lock_timeout`, `banned_at IS NULL FOR UPDATE` y 503; `banned_writes.rs` en las dos rutas.
- S5. Fijar `FOR UPDATE`; saldo y stakes abiertos en una sola sentencia; rescate contra liquidación a ambos lados del `COMMIT`.
- S6. Leer el reloj después de la fila 2 de RF25, con su prueba.
- S7. `MAX_FORM_BYTES` en los tres `POST`; `bet.invalid_request` sin texto de serde; `fight_id` fuera de rango da `bet.window_closed`.
- S8. El gate de B suma el OK explícito del owner a las preocupaciones 1 y 3 de 010.
- S9. Enmienda de ADR-0002: describir la vía completa de abuso; contador y `admin_audit` detectan, no previenen.
- No bloqueantes: saturación de `payout_bp`; `client_key` sin JS; test 7 sin limitador; `rake_bp = 0` de la CLI; último envío tras el cierre; alcance de `VoidStats.voided`; nota de borrado de cuenta.

### architect — 2026-11-02 — Cambios aplicados
- Apliqué lo anterior y las decisiones de quien coordina: rake con tope, rake inicial 0 con 5 % de objetivo, carencia por peleas y los valores. Son decisiones de quien coordina y siguen pendientes del owner. Elegí `fights.rake_bp`, escrito al crear la fila (G2), y un `quote` del server (P1). Leí el código sin compilar, y no verifiqué las cifras de G3 y G8, que vienen de game-designer. No apruebo este documento: lo escribí yo.

### Revisores — 2026-11-02 — REQUEST_CHANGES (segunda ronda)
Quien coordina transmitió estos puntos juntos. Solo atribuyó el 1, a security-reviewer y game-designer, así que los demás no van firmados por un revisor. Los revisores no editaron este documento.
1. RF10 y su mapeo, fase A: `UPDATE fights SET rake_bp` con SQL directo da `TA006` en cada estado no `settled`.
2. RF42: con saldo bajo `m`, siempre "Sin monedas", antes que "Sin poder sumar"; nunca "aún puedes apostar 0".
3. RF42, RF47 y RF48: el texto de sin rival dice que si alguien apuesta al otro lado se puede ganar o perder, y toda estimación indica que se pierde lo apostado si pierde su lado.
4. RF47: desactualizada, fuera de orden, no disponible, mínimo como ayuda, y de qué suma es `payout`.
5. TA-45 para el historial.
6. Enmienda de ADR-0002: controlar el server o su contenedor ya da la credencial.
7. Preocupación 6: el gate de B exige el OK explícito del owner antes de la primera apuesta.
- No bloqueantes: RF47 con `{id}` inválido y `u32::MAX`; RF15 con `quote` y un pozo `paid`; sumidero medido en la preocupación 2; qué pasa si el owner rechaza una nota; riesgo para TA-28.

### architect — 2026-11-02 — Cambios de segunda ronda aplicados
- Apliqué los puntos anteriores. Elegí que `payout` de `BetQuote` sea solo de esa suma. No verifiqué las cifras del sumidero ni qué porcentaje miden. No apruebo este documento: lo escribí yo.

### product-owner — 2026-11-02 — APPROVE
- Verifiqué: los cuatro cambios de la segunda ronda (aviso "sin rival", precedencia de "Sin monedas", estimación y TA-45). Solo cubre el criterio de producto. Aviso para TA-28: con poca población, el leaderboard quedaría casi vacío en D30.

### game-designer — 2026-11-02 — APPROVE
- Verifiqué: RF42, RF10 y la cifra del sumidero contra los resultados guardados de la simulación. Solo cubre la economía. Los valores siguen pendientes del OK del owner.

### security-reviewer — 2026-11-02 — APPROVE
- Verifiqué: RF10 y su mapeo, la enmienda de ADR-0002, la preocupación 6, RF47 y RF15. Es una revisión del texto, sin ejecutar nada, y no acepta riesgos ni pone en vigor la enmienda o las notas.

### architect — 2026-11-02 — Cambios tras revisar `plan-a.md`
Las aprobaciones de arriba son anteriores a estos cambios. Ninguno relaja un requisito.
- **Regla nueva, a la vista de security-reviewer en el PR de A:** RF2 suma "`bet_escrow` solo se debita en `bet_payout`, `refund` y `rake`". El plan lo había dejado abierto: sin ella, un asiento `grant` con SQL directo podía sacar monedas de la custodia.
- **Alineación con el código:** RF2 nombra los dos tests de `ledger.rs` que su última regla rompe. RF16 dice que hay que estrechar `fights_source_guard.rs`. RF17 da `void.binary_problem` a un `Engine(_)` sin fila. RF18 deja la auditoría en `admin.rs`, en la misma transacción. RF22 dice de dónde sale el contador. "Componentes" corrige el orden de las migraciones y el sitio de `VoidReason::parse`. "Datos" pide nombres fijos.

### architect — 2026-11-02 — Cambios tras el PR #86
Posteriores a todas las aprobaciones. Ninguno relaja un requisito.
- **RF1 y "Datos" recogen lo que la migración ya exige, más estricto que la letra anterior:** `ledger_entries_user_ck` da `(kind = 'rake') = (user_id IS NULL)`, y se suman `ledger_entries_bet_kind_ck`, `ledger_entries_bet_owner_fk` y `bets_owner_uq`, con sus casos de prueba.
- **Decisión nueva, preocupación 21:** un `refund` no ata `importe = stake` en la base. Es de architect, no aceptación de riesgo: security-reviewer la revisa en el PR, y el owner la ve en el gate de B.

### architect — 2026-11-02 — Cambios tras revisar `plan-b.md`
Posteriores a todas las aprobaciones. Ninguno relaja un requisito, salvo la excepción de la fila 4 de RF25, que falla cerrado.
- **Condiciones de security-reviewer a la preocupación 21 (ronda 2 del PR #86), resueltas:** RF2 ata `bet_stake` a `bets.stake`, con una migración de B. RF13 suma una consulta de producción en el README y el cuadre en memoria del gancho. `refund = stake` no entra en B. security-reviewer revisa este texto en el PR de B: esta nota no lo da por aceptado.
- **Alineación con el código:** RF25 da 409 `bet.window_closed` a un fallo en la fila 4, con sus pruebas. RF37 fija un bucket por fila.
