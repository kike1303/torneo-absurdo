---
id: "010"
title: Scheduler de peleas en vivo
type: spec
status: approved
risk: high
author: architect
reviewers: [product-owner, security-reviewer, game-designer]
approvals:
  - by: game-designer
    date: 2026-10-31
  - by: product-owner
    date: 2026-10-31
  - by: security-reviewer
    date: 2026-10-31
linear: TA-13
depends_on: ["002", "003", "005", "008"]
created: 2026-10-30
updated: 2026-10-31
---

# Spec: Scheduler de peleas en vivo

> Intent: [intent.md](./intent.md) · ADRs: [0002](../../decisions/0002-commit-reveal-semilla.md), [0005](../../decisions/0005-economia-sin-dinero-real.md), [0006](../../decisions/0006-despliegue-y-ci.md) D10, [0007](../../decisions/0007-i18n-desde-cero.md), [0008](../../decisions/0008-autenticacion.md) D6, D8 y D9, [0012](../../decisions/0012-logs-y-cabeceras-de-seguridad.md) A4 · Specs: [002](../002-spike-renderer/spec.md) (preocupación 2), [003](../003-motor-de-pelea/spec.md) (RF10, RF22, preocupaciones 2, 3 y 9), [005](../005-timeline-v2-fines-sin-ko/spec.md), [008](../008-autenticacion/spec.md)

## Resumen
El server programa una pelea cada 10 minutos en un horario fijo y la lleva por una máquina de estados en Postgres: `Scheduled` → `BettingOpen` → `BettingClosed` → `Live` → `Settled`. La integridad usa la **opción (a) de RF22**: el snapshot de personajes y reglas se publica al anunciar la pelea, **antes de que exista la semilla**, y el commit de ADR-0002 no cambia. El `FightLog` vive sellado en memoria, se recalcula de semilla y snapshot, y se guarda solo en la transacción de `Settled`. Los eventos salen a su hora exacta, sin adelanto: el margen lo pone el cliente, que reproduce 3,5 s por detrás. **Una vez publicado el commit, el runner nunca termina una pelea sin resultado:** si no puede seguir, se detiene hasta que se corrija. Solo se cancela una pelea `scheduled`, sin semilla ni apuestas. Un fallo irrecuperable tiene una salida manual y acotada, el final `voided` (RF59, lo implementa TA-27). TA-27 recibe un cierre serializable y un gancho de liquidación dentro de la transacción del reveal. **Esta spec no enmienda ningún ADR** (ver "Alternativas"). La pelea cuyo secreto se perdió ya tiene una salida decidida, que implementa TA-27 junto con su enmienda de ADR-0002 (preocupación 13).

## Fases
Cada fase es un PR con su plan. Ninguna empieza hasta aprobar esta spec.

| Fase | Plan | Dueño | Contenido | Qué se puede probar al terminar |
|---|---|---|---|---|
| A | `plan-a.md` | backend-engineer | Protocolo de datos, migración, tipos sellados, runner, recuperación, cruces, gancho de liquidación, API de lectura, imagen, liberación por tiempo con un sink que descarta *(Enmienda fase A: DP-7)* | Con `just dev`, peleas solas en la base, sin WS. Por HTTP se verifica cada pelea. Reinicio en cada estado, final único, cierre serializable, canarios de logs, `just prod-local` |
| B | `plan-b.md` | backend-engineer | Canales públicos, hub conectado como sink de la liberación *(Enmienda fase A: DP-7)*, catch-up, clientes lentos, regla de inactividad, feature `dev-fights` y sus barreras | Un cliente WS de prueba ve el vivo y entra tarde. Test 12 completo. No hay fugas antes de `Live` |
| C | `plan-c.md` | frontend-engineer | `Timeline` parcial, `loadLive` y `extend`, adaptador de prefijos. Independiente de B | Vitest con los dorados: reproducir por prefijos da la misma escena. `/dev/arena?live=1` simula el goteo sin server |
| D | `plan-d.md` | frontend-engineer | Vista en `/`, cliente WS, sincronización, i18n, e2e y trabajo de cierre | Todos los criterios del intent, `just check` y `just e2e` |

`qa-engineer` verifica cada fase, `devops-engineer` revisa los cambios del `Dockerfile`, de `check-compose` y de `check-release-binary` (A y B).

## Requisitos funcionales

### Fijación e integridad (A)
- **RF1. Snapshot antes que semilla.** Al crear la fila de la pelea (`Scheduled`), en una sola transacción, se guardan `ruleset_version`, `spec_a = canonical_spec_json(a)`, `spec_b = canonical_spec_json(b)` y `snapshot_sha256`. El orden de lados forma parte de lo fijado. Sin los cuatro no existe la fila (`NOT NULL`), y sin fila no hay apuestas. → integración: un roster inválido no crea filas; los bytes guardados son los de `canonical_spec_json`.
- **RF2. Forma del snapshot.** `snapshot_sha256 = SHA-256({"ruleset_version":N,"a":<spec_a>,"b":<spec_b>})`, con esos bytes exactos, sin espacios (N en decimal y los specs tal cual). Es reproducible por concatenación en cualquier lenguaje. No entra en el commit. → dorado `apps/server/tests/golden/fight-snapshot.json` con un vector fijo.
- **RF3. Commit sin cambios.** Al abrir las apuestas, en una sola transacción: semilla de 32 bytes con `OsRng`, fila en `fight_secrets`, `seed_commit = SHA-256(seed)` (ADR-0002 punto 1) y `state = betting_open`. La difusión va después del `COMMIT`. → integración: `sha256(secreto) == commit`; un fallo a mitad no deja commit ni estado nuevo.
- **RF4. Inmutable en la base.** Un trigger rechaza con `TA006` cambiar `id`, `opens_at`, `closes_at`, `live_at`, `ruleset_version`, `spec_a`, `spec_b`, `snapshot_sha256`, `slug_a`, `slug_b`, `max_hp_a`, `max_hp_b`, `time_limit_ms`, `max_duration_ms` *(Enmienda fase A: DP-3)*, `matchup_round` y `roster_sha256`, y reescribir un valor ya escrito de `seed_commit`, `opened_at`, `closed_at`, `live_started_at` o `settled_at`, además de todo `DELETE` y `TRUNCATE` en `fights`. El trigger corre también en `INSERT`, que exige `state = 'scheduled'` y nulas todas las columnas de ciclo (`seed_commit`, `opened_at`, `closed_at`, `live_started_at`, `settled_at`, `winner`, `end_reason`, `cancel_reason`, `void_reason`, `revealed_seed` y `log`), y rechaza un `UPDATE` que no cambie `state`: todo `UPDATE` legítimo es una transición *(Enmienda fase A: DP-6)*. `scheduled → betting_open` exige además que exista el secreto y coincida con el commit *(Enmienda 2: R2)*. Los `RAISE` usan un código fijo sin datos de la fila, y `db::error_kind` traduce `TA006` a `fight_guard`. → integración con SQL directo, columna por columna.
- **RF5. Solo el snapshot alimenta al motor.** `SealedLog::compute` recibe un `FightSnapshot` construido solo desde la fila: decodifica los dos specs, comprueba que reserializados dan los mismos bytes, que `snapshot_sha256` coincide, que `slug_*` y `max_hp_*` coinciden con los specs, que `time_limit_ms` y `max_duration_ms` coinciden con el ruleset *(Enmienda fase A: DP-3)* y que los specs pasan `validate` *(Enmienda fase A: menor M6)*, y usa `Ruleset::for_version(ruleset_version)`. El runner nunca lee `content/` para simular. `load_roster` solo se llama en `fights/roster.rs`. → integración: borrar o cambiar `content/` tras anunciar y reiniciar da el mismo log; prueba de fuente; cada discrepancia lleva a parada segura (RF16).
- **RF6. Reglas atadas a la versión.** `apps/server/tests/ruleset_freeze.rs` lleva una tabla `(version, sha256)`. El hash cubre el `Debug` de `Ruleset::for_version(n)`, y los bytes de `canonical_spec_json` y de `canonical_json` de un conjunto fijo de peleas (specs de los dorados y semillas fijas). Hoy la tabla está vacía, porque RF10 de 003 aún deja cambiar la versión 1. La primera entrada es un prerrequisito nuevo del primer despliegue. Cambiar o quitar una entrada es bloqueante en revisión (`REVIEW.md`, trabajo de cierre). → el test comprueba cada entrada; un test del mecanismo usa una versión y un hash de prueba.
- **RF7. Verificación pública.** Con solo `GET /api/fights/{id}`, un test que no importa nada de `server` comprueba para una pelea `Settled` con final `decided`: `SHA-256(seed) == seed_commit`; RF2; `canonical_json(simulate(seed, a, b, for_version(v))) == log`, byte a byte; `engine_wasm::run` (dependencia de desarrollo) da esos mismos bytes; `final` coincide con `log.outcome()`; `slug`, `max_hp` y `ruleset_version` del resumen coinciden con los specs y con el log; y `time_limit_ms` y `max_duration_ms` coinciden con `for_version(v)` *(Enmienda fase A: DP-3)*. Se repite tras cambiar `content/`. Una `voided` comprueba siempre el commit, y RF2 salvo con `snapshot_invalid`. Una cancelada no tiene semilla ni log, y comprueba RF2 salvo con `fight_unrunnable`, que puede deberse a un snapshot corrupto.

### Máquina de estados y programación (A)
- **RF8. Estados públicos:** `scheduled`, `betting_open`, `betting_closed`, `live` y `settled`. Toda transición es `UPDATE … WHERE id = $1 AND state = $previo RETURNING …`. Con 0 filas, otro ya la hizo y se relee, sin efectos. La difusión va siempre después del `COMMIT`. El trigger de RF4 solo admite `scheduled → betting_open`, `scheduled → settled` (cancelada), `betting_open → betting_closed`, `betting_closed → live`, `live → settled` (`decided`), y `betting_closed → settled` y `live → settled` (`voided`, RF59). Las transiciones del runner usan `SET LOCAL lock_timeout`, con reintento y `fights.lock_timeout` en `warn` (RF29). → propiedad: toda secuencia de transiciones repetidas o desordenadas termina en el mismo estado, sin error visible; integración con SQL directo: cada arista fuera del grafo falla.
- **RF9. Final único, exigido por la base.** `settled` lleva un solo `FightFinal`: `decided { winner: a|b|null, reason: ko|decision|draw }`, `cancelled { reason }` o `voided { reason }`. El trigger exige, con `TA006`, y los `CHECK` repiten como segunda barrera (`23514`) lo que pueden expresar *(Enmienda fase A: menor M2)*: una fila `settled` no admite ningún `UPDATE`; `decided` solo viene de `live` y exige `log` y `(winner IS NULL) = (end_reason = 'draw')`; `live → settled` con `cancel_reason` se rechaza; `cancel_reason` solo viene de `scheduled` y exige `seed_commit` nulo; `void_reason` solo viene de `betting_closed` o `live` y exige `log` nulo; si `seed_commit` no es nulo, `settled` exige `revealed_seed` no nulo y `sha256(revealed_seed) IS NOT DISTINCT FROM seed_commit`, sin excepciones, también con `voided`. Un `CHECK` exige `seed_commit` desde `betting_open`: `state = 'scheduled' OR seed_commit IS NOT NULL OR cancel_reason IS NOT NULL`. → integración con SQL directo, un caso por regla: reescribir el final falla; dos runners compiten por cerrar la misma pelea y queda un solo final y una sola llamada efectiva al gancho (RF31).
- **RF10. Una sola instancia.** El runner toma `pg_try_advisory_lock` en una conexión propia (fuera del pool, como el listener de sesiones) y solo actúa con el bloqueo. Sin él, reintenta cada 5 s y no crea filas, no hace transiciones, no liquida y no empieza emisiones. Una emisión empezada con el bloqueo sigue hasta su `end` (RF40), sin liquidar. Si pierde la conexión, deja de actuar en esos términos y vuelve a pedirlo; al recuperarlo, relee y reconcilia, y si la fila ya está `settled` cierra la emisión *(Enmienda fase A: DP-4)*. La sesión del bloqueo lleva `idle_session_timeout` (30 s) y opciones `tcp_*`, con un latido en su propia tarea cada 5 s, para que Postgres suelte el bloqueo de un cliente colgado. Una pasada tiene un máximo de 30 s y se corta si se pierde el bloqueo *(Enmienda 2: R5)*. → integración con dos runners sobre la misma base: solo uno avanza; al soltar, el otro retoma; con la conexión perdida, una emisión en curso sigue y nada se liquida.
- **RF11. Horario fijo.** Las peleas caen en una rejilla: `opens_at = epoch + n · cadencia`. `closes_at = opens_at + ventana`, `live_at = closes_at + pre_live` y son inmutables. Ningún horario depende de la duración de una pelea. Se valida al arrancar `ventana > 0` *(Enmienda fase A: DP-1)* y `ventana + pre_live + reserva + 10 s ≤ cadencia`, con `reserva = max_duration_ms + settle_delay_ms` (95 s). Si no, `config.invalid` con el nombre de la variable. → unitario y test de arranque.

  | Parámetro | Por defecto | Dónde |
  |---|---|---|
  | Cadencia | 600 s (`FIGHT_CADENCE_SECS`) | entorno |
  | Ventana de apuestas | 450 s (`FIGHT_BETTING_WINDOW_SECS`) | entorno |
  | De cierre a inicio | 30 s (`FIGHT_PRE_LIVE_SECS`) | entorno |
  | Anticipo del anuncio | 1 cadencia | constante |
  | Retraso de reproducción | 3500 ms | constante, se publica |
  | De `end` a `Settled` | 5000 ms | constante |
  | Ventana mínima para abrir tarde | `min(60 s, ventana / 2)`, calculada en ms: 60 s con los valores por defecto *(Enmienda fase A: DP-1)* | constante |
  | Espera si arranca tarde | 10 s | constante |

  Con los valores por defecto, por ranura: abre en 0:00, cierra en 7:30, empieza en 8:00, termina como tarde en 9:30 y se liquida como tarde en 9:35. Son 144 peleas al día. La espera sin animación es `cadencia − duración`: con las duraciones de `just sim` (p10, p50 y p90 de 50,6, 59,4 y 70,6 s) y las cotas de 45 a 90 s, son 8:30 como mínimo, unos 9:00 lo típico y 9:15 o algo más en las más cortas, con la pelea anterior, la siguiente anunciada y su cuenta atrás en pantalla (RF54). Con cadencia de 12 o 15 min, solo crece la ventana. Son valores para que los juzgue game-designer.
- **RF12. Anuncio.** Siempre hay una fila para la ranura siguiente en cuanto `now ≥ opens_at − anticipo`, salvo durante una detención (RF16) o mientras una fila vencida siga en `scheduled`: su par cuenta como usado hasta que abra o se cancele (RF20), y el anuncio se retrasa como mucho hasta que pierda su ventana, antes de la ranura siguiente porque `ventana < cadencia` *(Enmienda 2)*. Con el server recién arrancado se crea la próxima ranura futura. Las ranuras que pasaron sin fila no se recuperan. `UNIQUE (opens_at)` evita duplicados. → integración.
- **RF13. Una pelea en `Live` a la vez.** Abrir y cerrar ventanas no espera a otra pelea que siga su curso normal. Pasar a `Live` sí: si otra sigue en `live`, espera y arranca por la ruta tardía de RF15. Durante una detención no se abren ventanas nuevas (RF16). → integración.
- **RF14. Decisión pura.** `next_action(fila, now, contexto) -> Action` es una función pura que cubre la tabla de RF15 y se prueba en tabla. `contexto` tiene `otra_en_live`, `detenido` y si el log ya está en memoria. **No conoce `end.t` ni ningún dato del log.** El runner es una sola tarea con parada por `CancellationToken`, como `jobs::run`, que despierta en el siguiente plazo. Con un error de base, reintenta con backoff y registra `fights.db_unavailable` en `warn`. Se arranca desde `app::serve`, no desde `serve_listener`: las pruebas en proceso no programan peleas salvo que lo pidan. El reloj se inyecta al estilo de `rate::ServerClock`, sin `tokio::time::pause`.
- **RF15. Reanudación y recuperación.** Los plazos son absolutos (ADR-0006 D10). Al arrancar se aplica esta tabla. El apagado no cambia ningún estado.

  | Estado | Situación | Acción |
  |---|---|---|
  | `scheduled` | `now < opens_at` | Espera |
  | `scheduled` | queda menos de la ventana mínima (RF11, *Enmienda fase A: DP-1*), con o sin detención | `cancelled(missed_window)`. No hay semilla ni apuestas |
  | `scheduled` | llegó `opens_at`, con detención y al menos la ventana mínima | Espera |
  | `scheduled` | llegó `opens_at`, sin detención, y versión ausente o snapshot inválido (RF5) | `cancelled(fight_unrunnable)`. Sin semilla ni apuestas |
  | `scheduled` | llegó `opens_at`, sin detención, y `closes_at − now` es al menos la ventana mínima | Abre, tarde si hace falta, con el mismo `closes_at` |
  | `betting_open` | `now < closes_at` | Espera, también con detención |
  | `betting_open` | venció `closes_at` | Cierra, también con detención. **Nunca se cancela** |
  | `betting_closed` | sin log en memoria | Recalcula (RF17). Si falla, **parada segura** |
  | `betting_closed` | con detención | Espera |
  | `betting_closed` | `now < live_at` *(Enmienda fase A: DP-2)* | Espera |
  | `betting_closed` | `now ≤ live_at + 1 s`, sin otra en `live` | Pasa a `live` con `live_started_at = live_at` |
  | `betting_closed` | más tarde | Pasa a `live` con `live_started_at = now + 10 s`, en cuanto no haya otra en `live` |
  | `live` | se puede recalcular | Recalcula y entrega la pelea a la liberación (RF40), que da los vencidos como ya ocurridos y sigue a su hora |
  | `live` | no se puede recalcular | **Parada segura** |

  Al arrancar, las filas se evalúan de la más antigua a la más nueva. `detenido` vive en memoria y se reconstruye así: toda fila en `betting_open`, `betting_closed` o `live` cuya versión falte o que no pase RF5 *(Enmienda 2: R8)*, o en `betting_closed` o `live` cuyo log no se pueda reconstruir, lo activa, sin esperar a que llegue su plazo. Una liquidación fallida lo vuelve a activar tras sus reintentos (RF18). El runner relee las filas no liquidadas cada 60 s, con o sin detención, y así ve un secreto restaurado o una pelea que la CLI terminó (RF59). Hasta terminar la evaluación inicial no se aceptan apuestas (RF29) y `halted` es nulo (RF35). Esa evaluación no termina mientras falle una fila con commit, salvo que esa falla ya haya activado la detención *(Enmienda 2)*.

  `live_started_at` se escribe una vez y no cambia: la pelea no se vuelve a emitir tras un reinicio. → integración: matar el runner en cada fila de la tabla; al reanudar, semilla, snapshot y `canonical_json` son idénticos a los de una corrida sin caída, y hay un solo final.
- **RF16. Cancelar y detener.**
  - **Cancelar** solo es posible en `scheduled`, antes de que exista la semilla. Hay dos motivos, un enum cerrado en `protocol`: `missed_window` y `fight_unrunnable` (RF15).
  - **Con el commit publicado, el runner nunca termina una pelea sin resultado**, tampoco en `betting_open`. Si no, quien lee la semilla podría anular un resultado que no le conviene con un despliegue sin esa versión o tocando el snapshot, y tras un reinicio el proceso anterior pudo haber calculado el log.
  - **Parada segura.** En `betting_closed` o `live`, cualquier fallo al reconstruir el log detiene la pelea: versión ausente, snapshot que no pasa RF5, secreto ausente o que no coincide con el commit (RF23), o `simulate` con `Err`. Se registra `fights.safe_stop` en `error` cada 60 s, con `error_kind`. Si falla por versión ausente, la salida es volver a la versión anterior. Si el secreto sigue disponible y el fallo es irrecuperable, la salida es `voided` (RF59). **Con el secreto perdido, la salida es restaurarlo desde un backup. Si es imposible, la salida decidida la implementa TA-27, y hasta entonces no hay otra** (preocupación 13).
  - **Detención.** Hay detención durante una parada segura, con una fila que no pasa RF5 desde `betting_open` (RF15), cuando una liquidación o una cancelación lleva 60 s fallando (RF18, RF19), cuando un paso de una fila con commit lleva 60 s fallando por un error de la base, y cuando las pasadas llevan 60 s abortando sin perder el bloqueo *(Enmienda 2)*. Una fila sin commit que falla nunca detiene nada: termina cancelada al perder su ventana. Hasta los 60 s, a mitad de corrida, `halted` sigue en `false` y las otras ventanas siguen aceptando apuestas: un fallo pasajero de una fila no debe cerrar las demás. Con pasadas abortadas, `halted` llega en unos 2 minutos, porque el runner solo lo nota en su siguiente pasada; durante ese tiempo, un bloqueo de tabla impide también confirmar apuestas. `next_action` no abre ventanas nuevas y no se crean filas nuevas. Una `scheduled` que pierda su ventana termina en `cancelled(missed_window)`. Una `betting_open` no acepta más apuestas (RF29), cierra en `closes_at` y no pasa a `live`. El resto del server sigue, y `FightsCurrent.halted` lo publica sin causa (RF35). Al resolverse, las peleas retoman sin maratón: como mucho, la detenida y la que estaba abierta.
  - **Una pelea detenida bloquea `farm-report` y el cierre de su temporada** (RF32, ADR-0008 D8), y sus stakes siguen contando como apuestas abiertas para el rescate. Por eso `voided` tiene que existir antes de la primera apuesta.
  - **No hay endpoint para cancelar, anular ni leer la semilla, ni comando de CLI para leerla.** El único comando es el de RF59. El runner nunca lo llama.

  → prueba de fuente de que `cancel` solo se llama desde `next_action` (`Action::Cancel` solo se construye en `state.rs`, y el único brazo de ese `Action` en `runner.rs` agenda un intento, que llama a `sealed::cancel_fight`, el único que llama a `store::cancel`, *Enmienda 2*) y de que el runner no referencia la anulación; recorrido de `ROUTES`, de `cli::parse` y del texto de `admin --help`, porque `server --help` no existe *(Enmienda fase A: menores M4 y M5)*; integración de cada motivo y de cada causa de parada; integración con un gancho que falla durante varias ranuras: ninguna ventana nueva y, al corregirlo, un final por pelea.
- **RF17. Cálculo del log.** Solo después de confirmar el cierre. `SealedLog::compute` exige un `ClosedWindow`, y este solo se obtiene de un `COMMIT` del cierre o de leer una fila en `betting_closed` o `live`. No se persiste nada del log antes de `Settled`. → prueba de tipos (`compile_fail` en doctest), prueba de fuente, integración con una puerta antes del cálculo (RF29).
- **RF18. Liquidar.** Tras liberar `end`, `SealedLog::into_final` da un `FinalLog`, cuyo `settle_at = live_started_at + end.t + settle_delay` ya es deducible de lo público. En `settle_at`, una transacción escribe el final, `revealed_seed` y `log = canonical_json`, borra la fila de `fight_secrets`, llama al gancho (RF31) y confirma. Solo después se difunde. Una caída antes del `COMMIT` deja `live`, y se reintenta al reanudar. Si la transacción falla, reintenta con backoff y `fights.settle_failed` en `error`; tras 60 s de fallos hay detención (RF16) hasta que confirme, así que un fallo pasajero no detiene el vivo. Si el `UPDATE` afecta 0 filas, no es un fallo: relee la fila, y si ya está `settled` (RF59) cierra la emisión y difunde lo que haya en la base, sin reintentar. Cada intento corre en su propia tarea: un pánico en la transacción o en el gancho equivale a `settle_failed` y nunca mata en silencio la única tarea del runner. → integración con un gancho que falla, con uno que entra en pánico, con un kill antes del `COMMIT` y con la fila ya liquidada por otro proceso.
- **RF19. Cancelada.** `cancelled` es un final de `Settled` que solo sale de `scheduled`, sin `seed_commit`, `revealed_seed` ni `log`, y llama al gancho en su transacción. Es un intento en su propia tarea, como RF18, y su transacción borra un secreto plantado en esa fila (`sealed::cancel_fight`, *Enmienda 2*). Nunca hay una semilla de una pelea cancelada que publicar. → integración.

### Cruces (A)
- **RF20. Elección por rondas.** `pick_matchup(roster, usados_en_ronda, recientes, roll) -> (a, b)` es pura. Una ronda usa cada par ordenado del roster una vez, sin espejos (`a == b`). Los candidatos son los pares sin usar en la ronda (`fights.matchup_round`, que sube cuando no queda ninguno). Sobre ellos se aplican dos preferencias suaves, en orden: pares cuyo cruce sin orden no salió en las últimas 4 peleas; y, de esos, los que no comparten personaje con la anterior. Si una deja la lista vacía, se ignora. `roll` elige entre lo que queda. Las preferencias van **dentro** de los candidatos: como filtro global, con 4 personajes solo quedarían 4 de los 12 pares. Una pelea cancelada no consume su par ni cuenta como reciente. Con 4 personajes, una ronda son 12 peleas (2 h), su orden cambia cada vez, el mismo cruce vuelve a una distancia mínima de 5 peleas y mediana de 6, y el 50 % de las peleas comparte personaje con la anterior. Si cambia el conjunto de slugs (`roster_sha256` de la última fila distinto al actual), al crear la fila se abre ronda nueva, así un personaje nuevo no acapara el final de la ronda. → propiedad: en 1200 peleas no canceladas, cada par ordenado sale exactamente 100 veces, con cualquier secuencia de `roll`, también intercalando canceladas; con 4 personajes y cualquier `roll`, ningún cruce se repite a menos de 5 peleas no canceladas (recorrido de todos los estados alcanzables, no un muestreo).
- **RF21. Azar.** `roll` es un `u32` de `OsRng`, independiente de toda semilla. No viene del PRNG del motor ni de una semilla de pelea, y el cruce se fija antes de que la semilla exista. El server sigue sin depender de `sim` (`check-engine.mjs`).
- **RF22. Roster.** Se carga una vez al arrancar, con `spawn_blocking`, desde `CONTENT_DIR`, con `RULESET_VERSION` (constante del server, hoy 1). Con error o menos de 2 personajes: `fights.roster_invalid` en `error` con `error_kind`, sin peleas nuevas, las ya creadas siguen y el server arranca. Cambiar `content/` pide reiniciar. → integración.

### Confidencialidad (A y B)
- **RF23. Tipos sellados.** En `apps/server/src/fights/sealed.rs`:
  - `SealedSeed`: campo privado, sin `Debug`, `Serialize`, `Clone` ni `Display`. Al leerla de `fight_secrets`, compara `sha256(secreto)` con `seed_commit`; si falta o no coincide, error y parada segura (RF16). **No da un `engine::Seed` fuera de `sealed.rs`** ni sus bytes a nadie: `SealedLog::compute` la consume, y si falla la devuelve junto con el error, para que un fallo no pierda la semilla.
  - `SealedLog`: posee el `FightLog`, con las mismas ausencias. Solo da `release_due(elapsed_ms) -> Vec<FightEvent>` (los de `t ≤ elapsed` aún no liberados) e `into_final(self) -> Result<FinalLog, NotFinished>`, que solo funciona con `end` ya liberado. **Ningún método da tiempos futuros**: ni la duración, ni el `t` del siguiente evento, ni cuántos quedan.
  - `FinalLog`: `canonical_json`, `FightFinal`, `settle_at` y **los bytes de la semilla para el reveal, que no salen de ningún otro tipo**, y este no existe hasta liberar `end`. Sin `Debug` ni `Serialize`. Solo se escribe en la base y, tras el `COMMIT`, sale por RF34.
  - La anulación de RF59 tiene una sola entrada pública, `void_fight(tx, id, by)`, que contiene la negativa completa y el reveal, sin pasar por `FinalLog`. Nunca existe algo como `reveal_for_void(seed)`: ningún otro código puede obtener los bytes.
  - `events_of_settled(log_bytes)`: decodifica el log de una fila `settled`. Es de donde saca el hub los eventos de `last` tras un reinicio (RF38).

  El módulo no contiene ninguna macro de `tracing`. → doctests `compile_fail`: esos tipos no implementan los traits, usar un `SealedSeed` tras `compute` no compila, y no hay forma de pedirle los bytes a `SealedLog`.
- **RF24. Barrera de fuente.** Un test al estilo de `only_the_ledger_module_inserts_ledger_rows` recorre `apps/server/src` y, por tokens de identificador y no por rutas completas, falla si fuera de `fights/sealed.rs` aparece `simulate`, `canonical_json`, `FightLog`, `Seed`, `fight_secrets`, las formas calificadas `.outcome` o `::outcome` (como tokens, con cualquier espacio o salto de línea en medio; una variable local `outcome` no cuenta, *Enmienda fase A: DP-5*), un renombrado (`use engine as`, `extern crate engine as`) o un glob (`engine::*`, `protocol::*`); también si fuera de `fights/` aparece `revealed_seed`. Los demás módulos solo importan de `engine` por nombre. La función que busca se prueba con un caso negativo por forma: `use engine::{simulate, Seed};`, `use engine as e;`, `use engine::*;`, `e::simulate(…)`, `log.outcome()` y `FightLog::outcome`, con el tipo inferido, y con un caso positivo: `let outcome = 1;`. Otro test falla si un tipo de `crates/protocol` que no sea el propio log contiene `FightLog` (`ws.rs`, `fights.rs`). Es la defensa frente a que `FightLog` derive `Debug` y `Serialize` en `protocol`, que no se cambia.
- **RF25. La semilla en su tabla.** `fight_secrets` guarda la semilla; `fights` no tiene esa columna, así que un `SELECT` sobre `fights` no puede traerla. `fight_secrets` rechaza con `TA006` todo `UPDATE` y `TRUNCATE`, un `INSERT` con su pelea ya `settled` o que no coincida con su `seed_commit` si ya existe *(Enmienda 2: R2)*, y un `DELETE` salvo si la transacción ve su pelea en `settled`. Otra transacción no ve ese estado hasta el `COMMIT`. Si un liquidador olvidara borrar la fila, un `DELETE` posterior solo quitaría un valor ya público en `revealed_seed`, así que no hace falta exigir la misma transacción, algo que un trigger no puede comprobar con fiabilidad (`xmin` con savepoints) *(Enmienda fase A: DP-6)*. `revealed_seed` y `log` tienen `CHECK (… IS NULL OR state = 'settled')`, y RF9 ata `revealed_seed` al commit. → integración con SQL directo.
- **RF26. Por WS nunca salen la semilla ni el log.** Ni siquiera tras `Settled`: `FightSummary` no tiene esos campos. Solo salen por `GET /api/fights/{id}` en una fila `settled`. → test de forma de `ServerMessage`.
- **RF27. Test 12 de ADR-0008.** Para cada estado del ciclo y como anónimo, con sesión y con sesión de admin (`RecentAdmin` válido), recorre **todas las entradas de `ROUTES`**, también las mutaciones, con una petición válida o la que corresponda a su ruta, y los canales de `/ws`. Mira cuerpos, respuestas de error y cabeceras. Busca la semilla en hexadecimal (minúsculas y mayúsculas), base64, base64url y como arreglo de bytes; el `canonical_json`; y cada evento aún no liberado, incluidos `ko`, `end`, su `t` y `winner`. Antes de `Settled` no aparece la semilla ni el log, y un evento no aparece antes de su hora. Como sale de `ROUTES`, una ruta nueva entra sola. Una ruta con parámetros usa el id de la pelea.
- **RF28. Logs.** Con `RUST_LOG=trace`, una corrida completa con semilla conocida: ninguna línea de ningún target contiene la semilla (con los canarios de RF27), el `canonical_json` ni fragmentos de eventos (ADR-0012 A4.8, A4.9 y A8). Se repite en B con un socket conectado. No se agregan campos a `ALLOWED_FIELDS`. Eventos nuevos: el ciclo normal en `debug` (`fights.opened`, `fights.closed`, `fights.live`, `fights.settled`) y los fallos de RF8, RF14, RF16, RF18 y RF22 en `warn` o `error`, con `error_kind` de lista cerrada.

### Contrato con TA-27 (A)
> Se prueba aquí, sin apuestas. TA-27 no rediseña el ciclo.

- **RF29. Cierre serializable.** `FightsShared::lock_open_for_bet(&self, tx, id, now) -> Result<OpenWindow, WindowClosed>` *(Enmienda fase A: DP-8, misma semántica)* da `WindowClosed` durante una detención, mientras el runner no tenga el bloqueo de RF10 y hasta terminar la evaluación inicial al arrancar (RF15), y si no hace `SELECT state, closes_at FROM fights WHERE id = $1 AND closes_at > $now FOR SHARE` y exige `state = 'betting_open'`. Sin filas da `WindowClosed` **sin tomar ni esperar ningún bloqueo**. El cierre es un `UPDATE`, que espera a toda transacción con ese `FOR SHARE`. Así: apuestas concurrentes no se bloquean entre sí, toda apuesta que vio abierto confirma antes del cierre, y ninguna transacción lo ve abierto después. Tiene que ser `FOR SHARE`, porque el `FOR KEY SHARE` de una FK no conflictúa con ese `UPDATE`. `FOR SHARE` encadenados retrasarían el `UPDATE` sin límite, pero el filtro por hora corta la cadena en `closes_at`, y el cierre usa `lock_timeout` con reintento (RF8). El log solo se calcula tras ese `COMMIT` (RF17). La función no fija un `lock_timeout` propio, así que TA-27 lo pone en su transacción antes de llamarla, y un error de la base también da `WindowClosed` *(Enmienda 2)*. → integración: T1 toma el bloqueo; el cierre queda esperando y la puerta previa al cálculo no se alcanza; T1 confirma; una T2 recibe `WindowClosed`; se calcula. Por hora: con el reloj en `closes_at` y la fila aún abierta, `WindowClosed` sin esperar a una T1 que retiene el bloqueo. Con detención activa, sin el bloqueo de RF10 o antes de terminar la evaluación inicial, y la ventana en plazo, `WindowClosed`. Con lectores encadenados tras `closes_at`, el cierre confirma en un plazo acotado.
- **RF30. Hora de cierre.** `closes_at` es inmutable y es la hora de ADR-0008 D8: la temporada de la pelea es la que la contiene. Sirve también tras una caída, porque RF29 no acepta nada después de esa hora.
- **RF31. Gancho de liquidación.** `trait SettlementHook { fn settle(&self, tx: &mut PgConnection, fight: &SettlingFight) -> BoxFuture<Result<(), SettlementError>>; }`, con `SettlingFight { id, final_result, closes_at }` (`final` es palabra reservada) y un alias propio en lugar de `BoxFuture`, porque `futures-util` es solo de desarrollo *(Enmienda fase A: menor M1)*. Por defecto, `NoBets`. Se llama en la transacción de RF18, RF19 y RF59: TA-27 implementa ahí pagos y reembolsos. Un error revierte también el reveal. **Ningún estado alcanzable por la API hace fallar el gancho, y su resultado no depende de datos del usuario que cambien tras apostar** (como `banned_at`): sus errores solo pueden ser de infraestructura. Si no, una apuesta o un ban detendrían el vivo. TA-27 lo lleva como criterio bloqueante, con pruebas de cuenta baneada, apuestas en los topes y pozo de un solo lado. **Como la liquidación va en la misma transacción, `settled` implica liquidada**: no hay estado intermedio ni columna de pendiente. → integración con los finales `decided{a}`, `decided{null, draw}`, `cancelled` y `voided`: un gancho de prueba escribe una fila y falla o se mata antes del `COMMIT`: no quedan ni la fila ni el reveal, y el reintento los deja una sola vez.
- **RF32. Pendientes.** `fights::unsettled_in(pool, desde, hasta)` lista las peleas con `closes_at` en ese rango y `state <> 'settled'`. Es la consulta de `farm-report`. Una `voided` cuenta como liquidada. → integración.
- **RF33. Cancelación con apuestas abiertas: no existe.** Ninguna pelea `betting_open` se cancela (RF16), y RF29 solo acepta en `betting_open`, así que una cancelada nunca tiene apuestas. El reembolso de ADR-0008 D8 tiene dos casos reales: el empate y `voided` (RF59), que da su camino al test 25(f). En una cancelada, el gancho no tiene nada que devolver. → integración: RF29 da `WindowClosed` en toda pelea `scheduled` y `settled`.
- **RF59. Salida `voided` (contrato; la implementa TA-27, obligatoria antes de la primera apuesta).** Es la salida de un fallo irrecuperable, distinta de `cancelled`, porque hubo commit y puede haber apuestas. 010 deja listos el enum `VoidReason`, la columna `void_reason`, las reglas de RF9 y RF25 y el texto de la vista, para que TA-27 no toque el protocolo ni el ciclo.
  - **Quién:** solo `server fight void <fight_id> --by <admin_user_id>`, en el contenedor, como `farm-report` (ADR-0008 D8 y D9): `--by` debe tener `role = admin`, y escribe una fila en `admin_audit` en la misma transacción (`action = 'fight_void'`). TA-27 reemplaza `admin_audit_action_ck` y también `admin_audit_actor_ck`, con un actor nuevo, porque `admin_audit_cli_ck` exige `admin_id` nulo con `actor = 'cli'`. `--by` es **atribución sin autenticar**: identifica, no autoriza, porque quien corre el comando ya tiene `DATABASE_URL` (preocupación 1). No hay endpoint. El runner nunca lo llama. La salida no imprime la semilla ni datos del log.
  - **Estados:** solo `betting_closed` o `live`. Nunca `betting_open` ni `scheduled`, y con una fila `settled` falla sin escribir nada.
  - **Cuándo se niega:** evalúa en este orden, y en cada negativa no escribe nada. El comando deduce la razón, nunca la elige quien lo corre.
    1. Falta la versión: se niega, la salida es volver a la versión anterior.
    2. El secreto falta o no coincide: se niega (preocupación 13).
    3. `snapshot_invalid`, comprobado **sobre los bytes guardados, sin el código del motor**: el `SHA-256` de RF2 no coincide con `snapshot_sha256`, o `slug_*` difieren de los specs (lectura como JSON genérico).
    4. El hash y los slugs coinciden, y falla decodificar o reserializar o difiere `max_hp_*`, `time_limit_ms` o `max_duration_ms` (RF5, *Enmienda fase A: DP-3*): se niega. Los bytes están intactos, así que el problema es del binario, y la salida es volver a la versión anterior. Esas son columnas derivadas para el resumen, que no alimentan al motor: esa diferencia no prueba que la pelea esté rota. Un spec que decodifica y no pasa `validate` también se niega: lo pasó antes del commit (RF5), así que el fallo viene del binario. La salida es volver a la versión anterior *(Enmienda fase A: menor M6)*.
    5. `compute` funciona: se niega, la pelea está sana.
    6. `compute` falla con un spec válido, por `EventLimit`, `encode_failed` o un pánico de `compute`, capturado como en el runner: `engine_error` *(Enmienda fase A: menor M6)*. No se distingue de un binario con fallos (preocupación 3). **Pendiente para TA-27:** `log_without_end` (`ComputeError::NoEnd`) no está en esta lista. Hoy es inalcanzable, porque el motor siempre escribe `end`, y TA-27 decide cómo tratarlo *(Enmienda 2)*.

    Así no sirve para anular una pelea sana, ni desplegando sin esa versión, ni con un release que rompa la decodificación, ni borrando el secreto.
  - **Qué publica:** siempre revela la semilla, y `revealed_seed` nulo no se admite con `voided` (RF9). Borra la fila de `fight_secrets` en la misma transacción. Nunca hay `log`.
  - **Qué paga:** el gancho reembolsa con asientos del ledger. No cuenta como apuesta liquidada ni suma `RankedProfit` (ADR-0008 D8, donde equivale a una pelea cancelada).
  - **Después:** la CLI es otro proceso y no difunde nada. El runner lo ve en su relectura de 60 s (RF15), o antes si intenta liquidar (RF18): cierra la emisión si la pelea estaba en curso, actualiza el hub, levanta la detención si no queda otra causa y difunde `schedule_update`. La difusión tarda como máximo 60 s tras el `COMMIT`.

  → en 010, integración con SQL directo de las reglas de RF9 y RF25 para `voided`, y prueba de fuente de que nada en `src` la escribe. En TA-27: cada negativa de arriba (pelea sana, solo falta la versión, secreto perdido, hash correcto con fallo al decodificar, hash y slugs correctos con `max_hp_*`, `time_limit_ms` o `max_duration_ms` distintos, hash y slugs correctos y `validate` falla, `betting_open`, `--by` sin admin) falla sin escribir; cada razón; el reembolso; y que ningún log contiene la semilla.

### API de lectura (A)
- **RF34. `GET /api/fights/{id}`.** `FightDetail`: `FightSummary`, `spec_a` y `spec_b` como cadenas con los bytes exactos, y `seed` (hexadecimal) y `log` (cadena con `canonical_json`) solo en `settled`. Un `id` que no sea un decimal canónico de 1 a `i32::MAX`, o que no exista, da 404 `fight.not_found`. Disponible desde `scheduled`. `Cache-Control: no-store`. Un fallo de la base da 503 `fight.unavailable`, también con `no-store`. Si los bytes guardados de `spec_a`, `spec_b` o `log` no son UTF-8, algo que solo ocurre con una fila corrupta, ese campo va `null` y el resto de la respuesta sale igual, incluida `seed`: una `voided` por `snapshot_invalid` puede ser justo ese caso, y tiene que publicar su semilla (ADR-0002 punto 4, RF7 y RF59). Nunca se usa una conversión con pérdida *(Enmienda fase A: DP-10)*. Límite nuevo `FIGHT_READ_IP`, 120/min con ráfaga de 30 por `IpKey`, con `rate::http::layer`.
- **RF35. `GET /api/fights/current`.** `FightsCurrent { server_time, halted, current, next, last }`, servida de memoria, sin tocar la base, sin límite propio y con `Cache-Control: no-store`. `current` es la no liquidada más antigua, `next` la siguiente y `last` la última liquidada. Es lo que usa el SSR. Si el runner aún no cargó, todo a `null` con 200. `halted` es `detenido` (RF16), sin causa, y es nulo hasta terminar la evaluación inicial o sin el bloqueo de RF10, para no decir `false` sin saberlo. No filtra nada: una liquidación solo falla tras liberar `end`, ya público, y una parada segura ocurre porque no hay log, así que su valor no depende de un resultado. RF41 lo incluye.
- **RF36.** Ninguna ruta nueva muta: `Effect::Read` en `ROUTES`. Errores con el enum nuevo `FightErrorCode` (`fight.*`) en `ErrorCode`.

### Canales y liberación (B)
- **RF37. `Channel`.** Suma `Schedule` y `Fight(FightId)`, con `FightId(u32)`. Sigue siendo `Copy` y `Hash`. `as_str` se reemplaza por un formateo a cadena (`self`, `schedule`, `fight:12`), y `parse` acepta solo esos: `fight:` más un decimal canónico de 1 a `i32::MAX`. Rechaza `fight:0`, `fight:01`, `fight:+1`, `fight: 1`, `FIGHT:1`, `fight:2147483648`, `schedule:1`. Serde y `ts-rs` van con esa forma de cadena. `just gen`, sin 64 bits. → unitario y `check` de frescura.
- **RF38. Suscribir.** `schedule` y `fight:{id}` no piden sesión. `fight:{id}` de una pelea que no está en el hub en memoria (no liquidadas y la última liquidada, que tras un reinicio sale de `events_of_settled`) da `ws.forbidden_channel`: suscribirse nunca consulta la base por el socket. Resuscribirse a un canal ya abierto responde `subscribed` y no reenvía `fight_sync` ni `schedule_update`: si no, un mensaje de 50 KB saldría por uno de 40 bytes. Se corrige de paso lo que `session.rs` asume hoy con un solo canal: suscribir cualquier canal abre `self`, y desuscribir cualquiera lo cierra. → integración, también de esos dos errores y de la resuscripción.
- **RF39. Mensajes.** Tres variantes nuevas en `ServerMessage`:
  - `schedule_update { board: FightsCurrent }`: al suscribirse, en cada transición y al empezar o terminar una detención. El cliente reemplaza lo que tenía.
  - `fight_sync { fight: FightSummary, server_time, events }`: al suscribirse a `fight:{id}`, en cada transición y a un socket atrasado. Trae **todos los eventos ya liberados**.
  - `fight_events { fight_id, from_index, events, server_time }`: cada lote liberado. `from_index` es el índice del primero, y el cliente descarta lo que ya tiene.
- **RF40. Liberación solo por tiempo.** *(Enmienda fase A: DP-7. La tarea y su prueba son de A, con un `EventSink` que por defecto descarta y que nunca registra un evento. B conecta el hub como sink.)* Los eventos salen en `live_started_at + t`, **sin adelanto**. Una tarea con **tick fijo de 50 ms**, igual para toda pelea, calcula `elapsed` y llama a `release_due`; no pregunta nunca cuándo es el siguiente evento, así que el ritmo no depende del log. Lo liberado en un tick va en un lote. **La liberación nunca espera a la base**: una base caída no detiene los eventos, solo la liquidación. `elapsed` sale del reloj de pared al empezar o reanudar y luego del monotónico, así que un salto del reloj de pared durante la pelea no la mueve. Al reanudar con el reloj adelantado, los eventos salen antes de su hora real por ese salto, y con el reloj atrasado, después. Un salto de reloj tampoco rompe el cierre: uno hacia adelante cierra antes, y uno hacia atrás retrasa el cierre, y el log sigue sin calcularse hasta su `COMMIT`. Ninguno cambia un resultado, porque las apuestas ya cerraron cuando el log existe. → integración con reloj manual: en `t − 1` no llegó, en `t` llega en el siguiente tick. Incluye `ko` y `end`. Con la base caída, siguen saliendo.
- **RF41. Nada delata antes de `Live`.** Dos peleas con los mismos specs y dos semillas fijas que den resultado o duración distintos producen, con reloj manual, las mismas secuencias de mensajes y los mismos cuerpos de las dos `GET` hasta `live_started_at`, salvo `id`, `seed_commit` y horarios. `FightSummary` no tiene duración: solo las cotas constantes `max_duration_ms` (`time_limit_ms + end_ms` del ruleset, 90 000) y `time_limit_ms`.
- **RF42. Inactividad.** `ws.idle_timeout` (RF-B13 de 001) no corre mientras el socket tenga al menos una suscripción: un espectador no manda nada durante minutos. El ping y `ws.pong_timeout` siguen igual. Un socket sin suscripciones se cierra a los 120 s, como hoy. Es una enmienda de RF-B13. → integración, junto con el test que ya existe.
- **RF43. Clientes lentos.** El hub guarda por pelea solo lo liberado, serializado una vez. Avisa con un `watch` de generación, sin cola por socket. Cada socket lleva su índice: si coincide con el último lote, envía ese lote (`Arc` compartido); si no, un `fight_sync` compartido. Así un cliente lento salta, sin acumular. Un `send` que pasa de `close_grace` (5 s) cierra el socket, como ya ocurre. El upgrade fija `write_buffer_size` en 8 KiB y `max_write_buffer_size` en 72 KiB: tungstenite exige que el segundo supere al primero, cuyo valor por defecto es de 128 KiB (fijar solo el máximo entraría en pánico), y un `fight_sync` de 418 eventos, unos 50 KB, tiene que caber. → integración: un `fight_sync` en la cota de eventos se envía sin `WriteBufferFull`; con un socket que no lee, la memoria del hub no crece, los demás siguen a tiempo, y al cerrarse reconecta y se pone al día.
- **RF44. `server_time`.** Hora del server en ms desde epoch como cadena (`TimestampMs`, sin 64 bits) en `schedule_update`, `fight_sync` y `fight_events`. Es lo único que da el ancla de sincronización (RNF).
- **RF45. `dev-fights`.** Feature del server, como `dev-auth`: `FIGHT_SCRIPT_PATH` apunta a un JSON con filas `{seed, spec_a, spec_b}`, usadas en ciclo, que reemplaza a `OsRng` y a los cruces para forzar victoria y empate con los vectores dorados de 003. Fuera de `default` y del `Dockerfile`. El server se niega a arrancar con ella fuera de loopback. `check-compose` generaliza sus reglas a las dos features, y `check-release-binary` suma una cadena marca de esta (`FIGHT_SCRIPT_PATH`) en release y en `--expect-dev`. La fuente de semillas de `Settings.fights` para las integraciones va tras el mismo `cfg`: **en release ni se compila**. Las integraciones con semilla fija corren en la pasada con la feature de `just test`. → los tests de esos scripts; un test comprueba que el binario sin la feature ignora la variable.

### Renderer (C)
- **RF46. Timeline parcial.** `validateTimeline(json, manifests, { partial: true })` aplica las mismas reglas salvo las que exigen cierre (`ending_missing`, `closing_missing`, `ko_count` y `win_count` por falta). La `timeline_version` sigue siendo 2. La v1 no admite `partial`. → Vitest.
- **RF47. API.** `RendererHandle` suma `loadLive(timeline, assets)` y `extend(timeline)`. `extend` es síncrono, no recarga hojas, no toca la reproducción ni el cursor, rechaza con `timeline_invalid` si los cues nuevos no empiezan por los anteriores, y actualiza `durationMs`. `load` no cambia. → Vitest con un `Stage` falso.
- **RF48. Duración provisional.** Mientras no llega `end`, `duration_ms = max_duration_ms`. Al llegar, `duration_ms = end.t`. El estado `finished` solo existe desde entonces. El renderer no expone progreso respecto a la duración.
- **RF49. Prefijos equivalentes.** Para los 7 dorados y todo corte `c`: `sceneAt(prepareScene(prefijo(c)), t)` es idéntico a la escena del log completo para `t ≤ c − 3500`, en todos los campos, incluido el HP. → propiedad.
- **RF50. Entrar tarde.** `play()` sobre un reloj ya avanzado deja el cursor en `cursorAt(t)`: no dispara `onCue` por lo saltado. Con el reloj negativo (antes de empezar), `t = 0`. → Vitest.
- **RF51. Adaptador.** `timeline-from-fight-log.ts` suma `liveTimelineFromEvents(header, events, fighters, maxDurationMs)`: el mismo núcleo, donde un `attack` sin resolver al final y la falta de `end` no son errores. Sigue siendo el único archivo que conoce las dos formas. → Vitest, `fight-boundary.test.ts` sin cambios.
- **RF52. Goteo sin server.** `/dev/arena?log=…&live=1` entrega el dorado por lotes según su `t`, para medir fps y probar a mano. → e2e.

### Vista (D)
- **RF53. Ruta.** `/` deja de ser "próximamente" y pasa a ser la vista. `+page.server.ts` llama a `GET /api/fights/current`. Si falla, la página responde 200 con el estado de error, nunca 500. Sin sesión. Pixi solo por `import()`, en `FightStage.svelte` (`check-renderer-split`).
- **RF54. Estados.** Cargando (hasta el primer `schedule_update`, sin saltos gracias al SSR); iniciando (sin peleas); próxima, con los personajes y su epíteto, la cuenta atrás y el commit si ya existe (truncado a 375 px, completo en un atributo); por comenzar; en vivo; resultado, con victoria (KO o decisión), empate, cancelada ("no se disputó") o anulada; detenida; y sin conexión en vivo (RF57). La vista no dice "apuestas abiertas" hasta TA-27. El resultado se muestra por el cue de cierre del renderer o, sin WebGL, por `t ≥ end.t` en texto, nunca antes por llegar `end`.
  - **`last` sigue visible junto a la próxima** hasta que la siguiente esté en vivo, también para quien entra a mitad de la espera. Si es una cancelada, dice "no se disputó".
  - **La cuenta atrás apunta a** `live_at + playback_delay_ms`, o a `live_started_at + playback_delay_ms` si ya se conoce: es lo que ve el espectador.
  - **Con la cuenta atrás en cero y sin `live`** (arranque tardío, RF13), el estado es "por comenzar", en en y es. Nunca un `0:00` congelado.
  - **`halted` nulo es desconocido:** nunca muestra "detenida".
  - **Con `halted` en `true`**, el estado es "detenida", en en y es, por encima de "por comenzar" y de "en vivo". No promete una hora de regreso y mantiene `last`. Lo activa ese campo, y no una deducción por horarios, porque así cubre también la liquidación fallida, que ocurre con el `end` ya visto: quien ya vio el final mantiene el resultado, derivado de los eventos ya liberados, junto al aviso.

  → e2e de cada estado, incluidos `last` al entrar a mitad, "por comenzar" y "detenida" (con el reloj o un mensaje simulado). También el texto "anulada", con un mensaje simulado, porque en 010 ningún flujo real llega a él.
- **RF55. Sin barra de progreso.** El server no publica la duración, así que hay una cuenta atrás de `time_limit_ms` (86 s), como un reloj de ronda de arcade, que para al llegar el `ko` o `time_up`. Un KO termina antes del cero, y eso no delata nada. → e2e y revisión.
- **RF56. Nombres y assets.** Con placeholders `dummy_a` y `dummy_b` por lado (ADR-0004). Nombre y epíteto salen de `name` y `epithet` del frontmatter de `content/characters/<slug>/lore.<locale>.md`, **leídos al construir la web, sin copiarlos a `apps/web/messages/`**: esos archivos están bajo AGPL, y `LICENSE-CONTENT.md` reserva todo lo de `content/`, nombres incluidos. Un módulo virtual de Vite extrae solo esos dos campos, y `apps/web/Dockerfile` copia `content/characters` en su etapa de build, con el ajuste de `check-compose`. Un Vitest exige ambos campos para cada personaje y locale. Un slug desconocido usa un mensaje de respaldo con el slug (los personajes de prueba del e2e). → Vitest y `check-compose`.
- **RF57. Reconexión y capacidad.** Backoff de ADR-0006 D10: 1, 2, 4… hasta 30 s, con jitter. Al reconectar se resuscribe y `fight_sync` pone al día. Con `visibilitychange` a primer plano u `online`, el cliente comprueba el socket, reconecta de inmediato si hace falta y reinicia el backoff, porque un celular en segundo plano suele perder el socket. Un navegador no puede leer el 503 de un upgrade fallido, así que `ws.capacity` y un corte de red se ven igual. Tras 3 fallos seguidos, la vista muestra un único aviso ("no pudimos conectarte en vivo; seguimos intentando"), sigue intentando, consulta `GET /api/fights/current` cada 30 s con jitter y muestra estados y resultado sin animación. Nunca ve eventos futuros, porque no recibe eventos. → e2e con el WS bloqueado y con los dos eventos del navegador.
- **RF58. Texto.** Todo en en y es (`check-i18n`), cómoda a 375 px, resultado en una región `aria-live`, y cuenta atrás sin anuncios por segundo.

## Requisitos no funcionales
- **Sincronización:** con el mismo `live_started_at`, todos reproducen `t = server_now − live_started_at − 3500`. El cliente estima el offset con el máximo de `server_time − reloj_local` entre lo recibido en esa conexión, que es la muestra de menor latencia. Tolerancia: dos clientes en la misma red difieren ≤ 500 ms en `t` y reciben los mismos eventos en el mismo orden. → e2e con dos contextos, uno con sesión y otro sin ella.
- **Memoria (1 vCPU, `mem_limit` de 256 MB):** un log sellado (≤ 418 eventos, 8 a 13 KB en los dorados, unos 50 KB en la cota) y un buffer liberado por pelea activa. Sin colas por socket. Sin serializar por socket: un mensaje se serializa una vez, y solo el buffer de escritura de cada socket lo copia.
- **CPU:** el motor corre una vez por pelea y al reanudar. Con 2000 sockets, un lote cada ~600 ms. `prepareScene` se ejecuta en cada `extend`: ≤ 4 ms con 200 cues en el dispositivo de referencia de 002. Si no, el plan C lo vuelve incremental, sin cambiar RF47.
- **Carga:** `#[ignore]` en `apps/server/tests/fights_fanout.rs` con 2000 sockets en local: RSS y p99 de entrega, reportados en el PR de B. No bloquea. La prueba en el VPS sigue siendo de ADR-0008, con TA-8.
- **Disco:** ~2 MB al día sin comprimir (TOAST lo reduce). No se borra nada en 010.
- **Sin red:** `just check` y `just e2e` no salen de loopback.
- **Imagen:** `apps/server/Dockerfile` hace `COPY content/characters /content/characters` en la etapa final y `ENV CONTENT_DIR=/content/characters`; en local, el valor por defecto es `content/characters`, relativo a la raíz. Sin `RUN`. `check-compose` y los filtros `images` y `e2e` de `ci.yml` se ajustan (el job `e2e-server` usa el filtro `e2e`, *Enmienda fase A: menor M3*). El contenido ya es público en el repo; la licencia no cambia. → `just prod-local`.

## Diseño

### Componentes afectados
| Componente | Cambio | Fase |
|---|---|---|
| `crates/protocol` | `fights.rs` (tipos de abajo), `Channel`, `ServerMessage`, `FightErrorCode`. `just gen` | A, B |
| `apps/server/src/fights/` | Nuevo: `mod`, `sealed`, `state` (`next_action`), `runner`, `store` (único SQL de `fights`), `roster`, `matchup`, `hub`, `api`, `hook`, `bets_contract`; y, sin cambiar ningún contrato, `snapshot`, `release`, `board` y `clock` *(Enmienda fase A)* | A, B |
| `apps/server` resto | `app.rs` (arranque), `config.rs` (variables), `http/routes.rs`, `ws/{channels,session,handshake}.rs`, `Cargo.toml` (`engine`, `content`) | A, B |
| `apps/server/Dockerfile`, `scripts/check-*`, `ci.yml` | Contenido en la imagen, `dev-fights` | A, B |
| `packages/renderer` | `validate.ts`, `player/`, sin cambios en `Stage` | C |
| `apps/web` | En A, solo las entradas de `FightErrorCode` en `ERROR_CODES` y `ERROR_TEXT` y sus dos mensajes, sin UI *(Enmienda fase A: DP-9)*. Después: adaptador, `FightStage`, `lib/features/fight/live/*`, `routes/+page.*`, mensajes, e2e | A, C, D |
| `crates/engine` | **Sin cambios.** El helper `max_hp` del server se ata con un test al `FighterInfo` del log | |
| `crates/content`, `crates/sim` | Sin cambios. El server no depende de `sim` | |
| Ledger | Sin cambios: ningún asiento ni tipo nuevo | |

### Datos
```sql
CREATE TABLE fights (
    id int GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    state text NOT NULL,                 -- CHECK con los cinco valores
    opens_at, closes_at, live_at timestamptz NOT NULL,  -- UNIQUE (opens_at); inmutables
    ruleset_version smallint NOT NULL,
    spec_a, spec_b bytea NOT NULL,       -- bytes de canonical_spec_json
    snapshot_sha256 bytea NOT NULL,      -- 32 bytes
    slug_a, slug_b text NOT NULL,  max_hp_a, max_hp_b int NOT NULL,  -- derivados para el resumen
    time_limit_ms, max_duration_ms int NOT NULL,  -- derivados del ruleset; Enmienda fase A, DP-3
    matchup_round int NOT NULL,          -- RF20
    roster_sha256 bytea NOT NULL,        -- SHA-256 de los slugs ordenados, RF20
    seed_commit bytea NULL,              -- NOT NULL desde betting_open; nulo en una cancelada
    opened_at, closed_at, live_started_at, settled_at timestamptz NULL,
    winner text NULL, end_reason text NULL, cancel_reason text NULL, void_reason text NULL,
    revealed_seed bytea NULL,            -- solo en settled
    log bytea NULL                       -- canonical_json, solo en settled con final decided
);
CREATE TABLE fight_secrets (
    fight_id int PRIMARY KEY REFERENCES fights,
    seed bytea NOT NULL CHECK (octet_length(seed) = 32)
);
```
`bytea`, nunca `jsonb`: `jsonb` reordena y rompería "byte a byte". Es solo una adición (ADR-0006 D10). `TA006` se suma a los códigos existentes (RF4). Las horas viajan al SQL como ms desde epoch, sin agregar una librería de fechas al server.

### Tipos (`crates/protocol/src/fights.rs`)
```rust
pub struct TimestampMs(String);          // ms desde epoch en decimal; sin 64 bits (architecture)
pub enum FightState { Scheduled, BettingOpen, BettingClosed, Live, Settled }
pub enum CancelReason { MissedWindow, FightUnrunnable }
pub enum VoidReason { SnapshotInvalid, EngineError }
pub enum FightFinal {                    // tag "kind"
    Decided { winner: Option<Side>, reason: EndReason },
    Cancelled { reason: CancelReason },
    Voided { reason: VoidReason },
}
pub struct FighterCard { slug: String, max_hp: u16 }
pub struct FightSummary {
    id: u32, state: FightState, a: FighterCard, b: FighterCard,
    ruleset_version: u16, snapshot_sha256: String, seed_commit: Option<String>,
    opens_at: TimestampMs, closes_at: TimestampMs, live_at: TimestampMs,
    live_started_at: Option<TimestampMs>,
    time_limit_ms: u32, max_duration_ms: u32, playback_delay_ms: u32,
    final_result: Option<FightFinal>,    // serde "final"
}
pub struct FightDetail { fight: FightSummary,
                         spec_a: Option<String>, spec_b: Option<String>, // null solo si no son UTF-8; Enmienda fase A, DP-10
                         seed: Option<String>, log: Option<String> }
pub struct FightsCurrent { server_time: TimestampMs, halted: Option<bool>, current: Option<FightSummary>,
                           next: Option<FightSummary>, last: Option<FightSummary> }
```
Sin texto para humanos. `FightDetail` se construye en un solo sitio, que solo llena `seed` y `log` con una fila `settled`.

### Flujo
```
 anuncio           opens_at            closes_at   live_at          fin         settle_at
    │ snapshot       │ OsRng → commit     │ COMMIT   │ eventos a su   │ +5 s       │
    │ publicado      │ RF3                │ y luego  │ hora, sin      │            │ TX: final,
    ▼                ▼                    ▼ calcula  ▼ adelanto       ▼            ▼ reveal, log,
 Scheduled ───▶ BettingOpen ─────▶ BettingClosed ─▶ Live ─────────────────────▶ Settled  gancho
    │               │ con el commit, el runner no la termina │                     │ COMMIT
    │               │ sin resultado: parada segura           │                     ▼ luego difunde
    │               └ irrecuperable: CLI → Settled + voided (RF59, desde closed o live)
    │ se perdió, o no se puede correr
    └──▶ Settled + cancelled (sin semilla ni apuestas)
  cliente:                                            t_reproducción = t_server − 3500 ms
```

### Margen de reproducción
El server no adelanta nada: `release_lead = 0`. El margen va en el cliente: `playback_delay_ms = 3500`. Sale de lo que hay que conocer antes de pintar:
- windup de un `strike`: el impacto se conoce 400 ms antes, así que hace falta `≥ 400 ms + latencia` (preocupación 2 de 002);
- **cierre:** el adaptador pone `win` o `draw` en `ko.t + 1500` o `time_up.t + 1500`, y lo deriva de `end`, que el motor fija en `+4000`. El ganador por decisión se conoce 2500 ms **después** de la hora de su cue. Hace falta `≥ 2500 ms + latencia`, y es lo que manda.

3500 ms cubre 2500 ms y 1000 ms de latencia y jitter. `settle_delay = 5000` es `3500 + 1500`: nadie ve la semilla ni el final en `schedule_update` hasta haber visto el cierre y la pose. Si la red se atrasa más que el margen, la escena salta, porque es función pura de `t`; no hay nada que deshacer.

### UI
Una columna a 375 px y dos zonas en escritorio: escenario y panel de la pelea. Los estados son los de RF54. El lienzo solo se carga cuando la pelea está por empezar (al abrir `pre_live`) o en vivo, para no pagar Pixi durante los minutos de cuenta atrás. Sin WebGL (`webgl_unavailable`), texto con el HP y el resultado a partir de los eventos. El plan D cubre `webglcontextlost` y el bloqueo de pantalla en iOS y Android: la escena se rehace por `t`, y RF57 repone el socket. Reproducir la última pelea ya terminada, comprobar en pantalla y todo lo de apuestas quedan fuera.

## Áreas de preocupación
1. **Riesgo residual del operador (intent, pregunta 7). Aceptado (ver "Decisiones de 2026-10-31").** Basta con `DATABASE_URL`: no hace falta un binario nuevo ni un superusuario, porque `db::migrate` corre con el rol de la app, que es dueño de las tablas si hay una sola credencial. Ni (a) ni (b) evitan esto:
   - **Elegir la semilla.** Con los specs públicos desde `Scheduled`, quien tenga `DATABASE_URL` simula semillas fuera del server y, justo antes de `opens_at`, hace `INSERT INTO fight_secrets` y `UPDATE fights SET state = 'betting_open', seed_commit = …`. Es legal para RF4 y RF8, y la verificación pública pasa. **Ningún tercero lo detecta, ni archivando**: una semilla elegida es indistinguible de una al azar. Lo mismo logra un binario modificado.
   - **Leer `fight_secrets` durante la ventana**, conocer el resultado y apostar. También desde un backup hecho durante esa ventana.
   - **Anular los triggers.** El dueño de la tabla puede hacer `ALTER TABLE … DISABLE TRIGGER`, así que RF4, RF9 y RF25 frenan errores y a quien solo tiene el server, no a quien tiene la credencial. Reescribir lo ya publicado solo lo detecta quien lo archivó.

   - **Bloquear una pelea con la credencial.** Quien la tiene hace `DISABLE TRIGGER` y borra el secreto: la pelea queda detenida, todo el vivo se para y los ganadores no cobran (preocupación 13). También puede corromper el snapshot para que `voided` la acepte y convertir un resultado que no le conviene en un reembolso: queda a la vista como una anulación, con su semilla, y la registra `admin_audit`, que esa misma credencial también podría alterar.

   Mitigado: una caída o un error de despliegue no anulan una pelea (RF16), el runner no anula nada, y `voided` se niega si el log se puede reconstruir (RF59). Esa negativa protege de errores y de quien solo controla el proceso del server. **No protege de quien puede correr la CLI:** lee `DATABASE_URL` del mismo entorno, así que tenerla equivale a tener la credencial, y `--by` identifica, no autoriza. Un rol de migración aparte quitaría `DISABLE TRIGGER` a la credencial de la app, pero no la elección de semilla, porque la app tiene que poder escribir el secreto. Lo que cerraría de verdad el sorteo (entropía externa o de usuarios) está descartado "al inicio" por ADR-0002 y choca con "sin servicios externos". El daño posible es sobre monedas ficticias y el ranking (ADR-0005), no sobre dinero.
2. **La inmutabilidad depende de que alguien archive.** Un tercero la detecta solo si guardó `GET /api/fights/{id}` antes. Vale igual para (b), cuyo commit también vive en la base. Se acepta. Si un día importa más, una cadena de hashes entre peleas permitiría verificar el historial con un solo valor archivado: sería una adición, sin tocar el commit.
3. **Sin final automático tras el commit. Aceptado (ver "Decisiones de 2026-10-31").** Una cancelada nunca tiene semilla ni apuestas (RF16, RF33). Una pelea detenida no tiene salida automática, y mientras dure no hay peleas nuevas, `farm-report` falla y sus stakes siguen bloqueados. Si se puede recuperar (versión ausente), la salida es un release. Si es irrecuperable y el secreto sigue disponible (`EventLimit`, `encode_failed` o un pánico de `compute` con un spec válido, snapshot corrupto), la salida es `voided` (RF59). Con el secreto perdido, ver la preocupación 13. **Riesgo residual de `engine_error`:** un fallo de `compute` con los bytes intactos no distingue un bug de ese binario de un fallo real de la pelea, así que `voided` podría anular una pelea que un rollback habría terminado. La regla es que **antes de anular se prueba el binario anterior**, y como la semilla sale publicada, cualquiera puede resimular con un motor anterior y detectarlo. **Dueño: TA-27, como criterio bloqueante.** Hasta entonces no hay apuestas que proteger, y en local se resuelve con `just db-reset`.
4. **RF6 depende de disciplina al desplegar.** La tabla vacía no protege nada hasta su primera entrada. Por eso entra en los prerrequisitos que bloquean TA-8. Un cambio del motor que pase revisión sin versión nueva rompería la verificación de peleas pasadas. RF6 y los dorados de 003 lo hacen visible, no imposible.
5. **Reinicio con un motor distinto.** No se guarda un hash del log antes de `Settled`: un despliegue que cambie los bytes de una versión a mitad de una pelea (contra RF10 de 003) terminaría con un log distinto del emitido, y `Settled` publicaría el que verifica. Lo evita RF6. Agregar ese hash daría detección, pero sin remedio mejor que la parada segura, y a cambio de otra columna sensible. Se puede revisar si ocurre.
6. **Detención frente a disponibilidad.** RF16 y RF18 prefieren dejar de avanzar a inventar un final. Con un error de liquidación que dure más de 60 s, no se abren ventanas nuevas, y una ventana abierta deja de aceptar apuestas (RF29), así no se bloquean más stakes en peleas que no corren y no sale una maratón al arreglarse. Quien ya apostó sigue a la espera. Es lo correcto para el ledger, pero un bug ahí detiene todo el vivo. RF31 lo limita: el gancho solo puede fallar por infraestructura.
7. **RF42 suaviza una defensa de 001.** La inactividad frenaba sockets ociosos. Con una suscripción ya no hay cierre, y un atacante solo gasta un mensaje más. Quedan los 50 sockets por IP y el tope global, y `ws.capacity` sigue siendo la salida. Falta la prueba de carga real de ADR-0008, con TA-8. La reserva del 20 % sigue siendo de TA-27.
8. **Memoria en el peor caso.** 2000 sockets atascados con un `fight_sync` a medias son unos 100 MB, y como máximo ~140 MB con el tope de 72 KiB, durante los 5 s antes de cerrarlos, sobre un límite de 256 MB. Es un ataque coordinado, no el caso normal. La medición de B decide si conviene partir el `fight_sync` y bajar el tope.
9. **Tiempo del e2e.** Un empate real dura 94 s, y la rejilla mínima con `ventana = 5` y `pre_live = 3` es de 113 s. El e2e ve dos peleas (≈ 4 min) y todos los proyectos miran **las mismas**, en paralelo. Si pesa demasiado en CI, se mueve a un job propio sin bajar la cobertura.
10. **WebKit en Linux puede no tener WebGL** (preocupación 6 de 002). Por eso el e2e afirma el vivo por los eventos y los estados de la vista, con `data-*` solo bajo `debug`, y no por lo pintado.
11. **Vista en `/`.** Hoy varios e2e (`a11y`, `hydration`, `i18n`, `screenshots`) visitan `/` con un contenido fijo. El contenido pasa a depender del reloj. El plan D los fija en un estado conocido.
12. **Conflicto resuelto entre intent y ADR-0002 sobre el margen.** La regla es la del punto 5 del intent, y con margen 0 en el server se cumple con holgura.
13. **Pelea con el secreto perdido. Decidido: opción 1 (ver "Decisiones de 2026-10-31"). Dueño: TA-27, como criterio bloqueante.** Esta spec no la implementa: hasta TA-27, esa pelea queda en detención, y `voided` se niega, porque siempre revela la semilla. **La salida preferente es restaurar el secreto desde un backup:** un `INSERT` en `fight_secrets` que coincida con el commit, y la pelea retoma sola (RF15). Solo sirve si el backup tiene la fila, y con backups diarios eso solo ocurre si la pelea estaba abierta en ese momento. **Hasta TA-27, si no hay backup, la única salida es SQL manual con los triggers apagados**, que rompe la regla de las transiciones (RF8 y RF9) y además escribe reembolsos fuera del módulo del ledger. Mientras tanto no hay peleas nuevas, y `farm-report` falla. Antes de TA-27 no hay apuestas que proteger, y en local se resuelve con `just db-reset`.

    El problema: ADR-0002 punto 4 dice "en `Settled` se publica la semilla", y aquí ya no existe. Un final sin reveal, además, daría a quien tiene la credencial una forma de evitar el reveal borrando el secreto (preocupación 1).

    | Opción | Consecuencia |
    |---|---|
    | 1. Enmendar ADR-0002 con un final `voided(secret_lost)` sin semilla **(la elegida)** | La pelea tiene salida con reembolso. El costo es una excepción al reveal, y la vía de la preocupación 1 |
    | 2. Decidir que es una lectura del ADR y no una enmienda | Igual que la 1, sin cambiar el texto del ADR |
    | 3. No agregar ese final | No hay excepción, y queda solo la salida por SQL manual |

    **Texto propuesto para la opción 1, aún no aplicado (va en TA-27):** "Salvo una pelea anulada porque el secreto se perdió (`voided(secret_lost)`), en la que no hay semilla que publicar y la pelea queda sin verificar". TA-27 suma esa razón a RF9, RF25 y RF59.

## Respuestas a preguntas abiertas del intent
1. **Cadencia y ventana** → RF11: 10 min, ventana de 7:30, inicio 30 s después. Espera sin animación de 8:30 como mínimo, ~9:00 lo típico y 9:15 en las más cortas. Configurables. Valida game-designer.
2. **Cruces** → RF20 y RF21: rondas con cada par ordenado una vez, en orden al azar con `OsRng`, sin repetir cruce a menos de 5 peleas con 4 personajes; una ronda dura 2 h. Se anticipa una cadencia antes de abrir (RF12). Revisa game-designer.
3. **Fijación** → snapshot publicado, opción (a), más `snapshot_sha256` (RF1, RF2 y "Alternativas"). No cambia el commit.
4. **Si no puede correr** → RF15, RF16 y RF59. Una pelea `scheduled` abre tarde si queda la ventana mínima de RF11 (60 s con los valores por defecto); si no, se cancela sin semilla. Con commit, ni una caída ni un fallo la terminan solos: corre tarde o se detiene. Por tanto, el runner no termina una pelea tras calcular el log, tampoco tras un reinicio. Un fallo irrecuperable con el secreto disponible sale por `voided`, a mano, con registro, y publica la semilla. Con el secreto perdido, primero se restaura, y si es imposible, anulada sin semilla, con reembolso (preocupación 13, en TA-27). Una cancelada no tiene semilla.
5. **Enganche con TA-27** → RF29 a RF33 y RF59: liquidación en la misma transacción que el reveal, sin estado intermedio; una caída la revierte y se reintenta; `unsettled_in` da las pendientes; la temporada usa `closes_at`. Una cancelada nunca tiene apuestas abiertas. El reembolso sin resultado va por `voided`.
6. **Qué se anuncia** → RF11 y RF41: rejilla fija con reserva de 95 s, sin duración publicada.
7. **Riesgo del operador** → preocupaciones 1 y 3. **Aceptado (ver "Decisiones de 2026-10-31").**
8. **`ws.capacity`** → RF57: un aviso único, poll y resultado sin animación. El navegador no puede distinguir capacidad de red. La "pelea con retraso" se cubrirá con la UI de repeticiones, fuera de alcance.
9. **Reproducir solo el pasado** → RF46 a RF52. Costo acotado: un modo parcial, dos métodos y el adaptador; el núcleo ya es función pura de `t`.
10. **Margen** → 0 en el server y 3500 ms en el cliente ("Margen de reproducción").

## Estrategia de pruebas
| Criterio del intent | Prueba | Nivel | Fase |
|---|---|---|---|
| Sola | Varias peleas seguidas con plazos cortos | integración | A |
| Reinicio | Tabla de RF15, con kill en cada fila; `canonical_json` idéntico | integración | A |
| Personajes fijados | RF1, RF5 | integración | A |
| Comprobable | RF7, sin importar `server` | integración | A |
| Final único e inmutable | RF9, RF4, RF25, con SQL directo por regla | integración | A |
| Cierre consultable | RF29, con puerta antes del cálculo | integración | A |
| Logs | RF28, canarios en `trace` | integración | A |
| Imagen | `just prod-local`; una pelea completa con los valores por defecto, hasta unos 20 min *(Enmienda fase A: DP-11)* | manual en el PR | A |
| Sin fugas (test 12) | RF27, RF40, RF41; RF28 otra vez en B | integración | A y B |
| Sincronizada | Dos sockets reciben los mismos bytes; RNF en e2e | integración, e2e | B, D |
| Llegar tarde | `fight_sync` solo con lo liberado; reconexión | integración, e2e | B, D |
| Vista | Próxima, vivo, entrar tarde, victoria y empate; en y es; 375 px; Chromium y WebKit; sin sesión | e2e con `dev-fights` y `FIGHT_SCRIPT_PATH` | D |
| Reembolso delegado | Comprobar TA-27 y TA-13 en Linear | manual al cerrar | D |

Unitarias y de propiedad: `next_action`, `pick_matchup`, RF2 (dorado), `Channel::parse`, RF49. El e2e compara además el `log` publicado con `log-draw.json` y un `log-ko-*.json`, byte a byte. `just gen` y `check-i18n` entran en `just check`.

## Trabajo de cierre (fase D)
No se edita antes de aprobar y solo cuando la pieza exista:
- `docs/architecture.md`: ciclo con el final cancelado, canales y sincronización, y prerrequisito 5 cerrado. Las tablas, la dependencia del server de `engine` y `content` y el prerrequisito de RF6 ya entraron en la fase A *(Enmienda 2)*.
- `docs/glossary.md`: `FightSnapshot`, `FightFinal`, `Cancelada`, `Anulada` (`voided`), `FightState`, parada segura, detención, retraso de reproducción. `SeedCommit` no cambia.
- ADR-0002: sin cambios en 010. TA-27 enmienda su punto 4 (preocupación 13).
- Spec de 001: enmienda de RF-B13 y de su frase en la línea 277 ("ningún socket vive… más de 120 s sin mensajes aceptados"), por RF42. Spec de 003: RF22 resuelta con (a).
- `CLAUDE.md`: `dev-fights`, `CONTENT_DIR` y variables `FIGHT_*`. `apps/server/.env.example`. `apps/server/README.md`: cómo verificar una pelea con datos públicos. `REVIEW.md`: regla de RF6.
- Linear: comprobar en TA-27 los criterios bloqueantes: reembolso de empate y cancelación con asientos, `voided` completo (RF59), las garantías del gancho (RF31), la regla de probar el binario anterior antes de anular (preocupación 3) y la salida de la preocupación 13. Comprobar la delegación en TA-13.

## Alternativas consideradas

### (a) Snapshot publicado o (b) commit extendido: se elige (a)
| | (a) snapshot + `SHA-256(seed)` | (b) sola: `SHA-256(seed ‖ specs ‖ ruleset)` | (a) + (b) |
|---|---|---|---|
| ¿Personajes públicos en la apertura? | **Sí** | **No**, solo un hash. Incumple el punto 4 del intent | Sí |
| Cambiar specs tras el commit, simular con otros | Se detecta: no reproduce el log publicado | Se detecta | Se detecta |
| Reescribir specs en la base con un commit ya publicado | Lo detecta quien archivó el snapshot o su hash | Lo detecta quien archivó el commit | Igual |
| Reescribir también el commit | Lo detecta quien archivó | Igual | Igual |
| Elegir la semilla antes del commit, con `DATABASE_URL` o un binario modificado | **No lo evita ni se detecta** | **No lo evita ni se detecta** | **No lo evita ni se detecta** |
| Elegir specs viendo la semilla | Con el binario sin modificar y sin escritura directa, imposible: el snapshot es anterior. Con escritura directa, solo lo detecta quien archivó el anuncio | Antes del commit nadie vio los specs: quien controla el binario o la base elige los dos y no hay qué archivar | Igual que (a) |
| Leer la semilla en la base | **No lo evita** | **No lo evita** | **No lo evita** |
| Quién verifica y con qué | Cualquiera con `seed`, specs, versión y commit, tras `Settled` | Igual, y los specs solo llegan tras `Settled` | Igual que (a) |
| Costo | Columnas, trigger, API | Lo mismo o más, y framing sin ambigüedad en `‖` | Suma de ambos |
| ¿Enmienda de ADR-0002 y glosario? | **No** (RF22 la permite tal cual) | **Sí**, con OK del owner | **Sí** |

Conclusión:
- **(b) sola no cumple el intent**, porque los personajes no son públicos al abrir.
- **(b) sobre (a) no agrega defensa:** ambos dependen de que alguien haya archivado algo publicado al abrir, y un operador que reescribe la base reescribe el commit igual que el snapshot. Elegir la semilla por fuera (preocupación 1) pasa con cualquiera, y (b) lo deja igual. Lo único que da es un solo valor de 32 bytes que archivar.
- **¿Basta (a)?** Basta para RF22 y para el ataque que RF22 describe, cambiar un personaje después del commit, con tres condiciones que esta spec convierte en requisitos: publicación ligada al ciclo (RF1), simular solo desde el snapshot (RF5) y datos archivables por cualquiera (RF34). Que el snapshot sea anterior a la semilla impide elegir specs viendo la semilla **solo con el binario sin modificar y sin escritura directa en la base**. No basta contra quien tiene `DATABASE_URL`, y (b) tampoco: es un riesgo residual, no una razón para cambiar de opción.
- **¿Añade algo `snapshot_sha256`?** Sin tocar el commit, un valor corto que viaja en cada `schedule_update` sin mandar los specs a todos los sockets, que se compara fácil entre observadores y que da lo único bueno de (b). No liga la semilla ni defiende contra una base reescrita sin archivo. Cuesta una columna, y por eso se incluye.
- **`ruleset_version` solo fija un número**, en (a) y en (b). Lo que lo ata a un comportamiento es RF10 de 003, y hoy solo por disciplina: los dorados se pueden regenerar. Por eso RF6 agrega el dorado de congelación. Hace falta con cualquiera de las dos.
- **El orden de lados** queda fijado: `spec_a` y `spec_b` y su lugar en RF2.

### Otras
- **Persistir el log al calcularlo.** Evita recalcular y detecta un motor distinto. Se descarta: suma una columna con el resultado que un `SELECT` o una ruta podrían filtrar, y no protege de quien lee la base, que ya tiene la semilla. Recalcular cuesta microsegundos.
- **Estado `Cancelled` propio.** Sin semilla que revelar ya no choca con ADR-0002, pero duplica "terminada" en `farm-report` y en el gancho.
- **Cancelar una pelea con commit publicado (por caída o `fight_unrunnable`).** Más simple, y da una salida a una pelea que no puede correr, pero permite anular un resultado ya conocido apagando el server, desplegando sin esa versión o borrando el secreto. Una versión anterior de esta spec lo admitía en `betting_open` y `betting_closed`. `voided` cubre lo irrecuperable sin ese problema: es manual, se niega con una pelea sana y queda registrado.
- **Deducir "detenida" de los horarios en el cliente, sin `halted`.** No expone nada nuevo, pero no cubre una liquidación fallida y pone la regla en cada cliente.
- **Adelantar eventos en el server.** Es lo que sugería `docs/architecture.md`, con ≥ 400 ms y más latencia. Cubrir el cierre exigiría adelantar ≥ 2500 ms, y una regla con excepción es más difícil de probar que `t ≤ elapsed`. La vista total para el usuario es la misma.
- **Sintetizar `win` en el cliente al llegar `ko`.** Reduciría el retraso en los KO, pero no en decisión y empate, y copiaría lógica del motor.
- **`broadcast` de tokio o una cola por socket.** Con 2000 sockets, la cola multiplica la memoria, y un `broadcast` retrasado obliga a tratar `Lagged` igual que RF43. El `watch` con buffer compartido es más simple y está acotado.
- **Renderer: varios `load` o una `Timeline` v3.** `load` recarga hojas y reinicia la reproducción. La v3 cambiaría el formato para algo que es solo un modo de validación.
- **Cruces sin la preferencia de las últimas 4 peleas.** Con 4 personajes daba revanchas a 2 peleas en el 66,7 % de las veces. Con esa preferencia, la distancia mínima es 5.
- **Cruces deterministas por índice (round robin puro).** Más simple, sin estado ni azar, pero el orden se repite idéntico cada 2 h y un personaje nuevo lo reordena todo. La diferencia en código es pequeña. Si game-designer lo prefiere, solo cambia RF20.
- **Barra de progreso con la cota de 90 s.** Una barra que salta al terminar se lee mal; el reloj de ronda es un patrón conocido.
- **Reusar la feature `dev-auth` para forzar peleas.** Evita tocar los scripts, pero mezcla dos permisos bajo un nombre engañoso.

## Enmienda (fase A, 2026-10-31)
Sale de la revisión de [plan-a.md](./plan-a.md), cuyos puntos conservan aquí su número. Cada cambio está también en su RF, con la marca *Enmienda fase A*. La spec sigue aprobada: esta enmienda no reabre el resto. **No toca ningún ADR.**

| Punto | Qué cambia | Dónde | Por qué | Security |
|---|---|---|---|---|
| DP-1 | La ventana mínima para abrir tarde es `min(60 s, ventana / 2)`, en ms, y RF11 exige `ventana > 0` | RF11, RF15, respuesta 4 | Con `ventana = 5`, la rejilla mínima de la preocupación 9, toda pelea se cancelaba. Con los valores por defecto sigue en 60 s | No |
| DP-2 | Fila `betting_closed`, `now < live_at`: espera | RF15 | Sin cota inferior, la pelea se saltaba `pre_live` | No |
| DP-3 | Columnas `time_limit_ms` y `max_duration_ms`, inmutables, comparadas con el ruleset. Una diferencia se trata como `max_hp_*` | RF4, RF5, RF7, RF59 paso 4, "Datos" | `FightSummary` no se podía construir con la versión ausente, un caso que RF15 y RF16 prevén | **Sí** |
| DP-4 | Sin bloqueo no empieza nada, pero una emisión ya empezada sigue hasta su `end`, sin liquidar | RF10 | La conexión del bloqueo es de la misma base que RF40 manda no esperar | **Sí** |
| DP-5 | De `outcome` solo se prohíben `.outcome` y `::outcome`, como tokens | RF24 | El nombre ya existe como variable local en `db.rs`, `admin.rs` y `ws/session.rs`, sin relación con el log. La defensa real es que no se obtiene un `FightLog` sin un token prohibido | **Sí** |
| DP-6 | El guard corre en `INSERT` y rechaza un `UPDATE` sin transición. `fight_secrets` rechaza un `INSERT` con la pelea `settled`, y su `DELETE` mira el estado que ve la transacción | RF4, RF25 | Un `INSERT` directo se saltaba RF8 y RF9, y "en esa transacción" no se comprueba con fiabilidad desde un trigger. La restauración de la preocupación 13 y la ruta de la preocupación 1 no cambian | **Sí** |
| DP-7 | La tarea de liberación pasa a A, con un `EventSink` que descarta | Fases, RF40 | RF18 solo liquida tras liberar `end`, y `SealedLog` no da otro camino | **Sí** |
| DP-8 | `lock_open_for_bet` es un método de `FightsShared`. La semántica no cambia | RF29 | Tiene que leer la detención, el bloqueo y la evaluación inicial | No |
| DP-9 | A toca `ERROR_CODES`, `ERROR_TEXT` y `messages/{en,es}.json` | "Componentes" | Son tipos mapeados sobre `ErrorCode`: sin esas entradas, `svelte-check` falla | No |
| DP-10 | 503 `fight.unavailable` si falla la base. `spec_a` y `spec_b` pasan a `Option<String>`: un campo con bytes que no son UTF-8 va `null` y el resto sale igual | RF34, "Tipos" | **Cambia lo propuesto:** un 503 por una fila corrupta ocultaría la semilla de una `voided` por `snapshot_invalid`, contra ADR-0002 punto 4 y RF7 | **Sí** |
| DP-11 | La prueba manual de `just prod-local` usa los valores por defecto | "Estrategia de pruebas" | `check-compose` cierra las variables de `server`, y `compose.local.yml` solo agrega `ports` | No |
| M1 | `final_result` en `SettlingFight`, y un alias propio en lugar de `BoxFuture` | RF31 | `final` es palabra reservada, y `futures-util` es solo de desarrollo | No |
| M2 | Un `CHECK` da `23514`; `TA006` lo da el guard | RF9 | Un `CHECK` no puede lanzar un código propio | No |
| M3 | `ci.yml`: el job `e2e-server` usa el filtro `e2e` | RNF Imagen | No existe un filtro `e2e-server` | No |
| M4 | La prueba de RF16 recorre `cli::parse` y `admin --help` | RF16 | `server --help` da `Invalid` | No |
| M5 | Forma concreta de la prueba de "`cancel` solo desde `next_action`" (la Enmienda 2 la ajusta a `sealed::cancel_fight`) | RF16 | `next_action` es pura y no llama a nada | No |
| M6 | RF5 suma `validate`. Su fallo da `fight_unrunnable` en `scheduled` y parada segura después. `SpecInvalid` sigue siendo una variante propia para el log, pero en RF59 se trata como el paso 4: se niega. `engine_error` queda para `EventLimit`, `encode_failed` o un pánico de `compute` | RF5, RF59, preocupación 3 | Evita publicar un commit de una pelea que `simulate` ya va a rechazar. `validate` es pura y pasó antes del commit, así que un fallo posterior con los bytes intactos viene del binario: si llevara a `engine_error`, un release que la endurezca permitiría anular una pelea sana. **Sí cambia el comportamiento**, aunque el plan lo listara como menor | **Sí** |

Quedan fuera de esta enmienda, porque no cambian ningún contrato: los módulos nuevos `snapshot`, `release`, `board` y `clock`. `next_action` no lleva parámetro de configuración: su firma es la de RF14, y la ventana mínima sale de las horas de la fila (Enmienda 2).

**Para security-reviewer:** DP-3, DP-4, DP-5, DP-6, DP-7, DP-10 y M6. Las demás no tocan confidencialidad, integridad del final ni la negativa de RF59.

## Enmienda 2 (fase A, 2026-10-31)
Recoge lo que `plan-a.md` registró tras su aprobación, que architect aceptó al revisar el PR #69. Cada cambio está también en su RF, con la marca *Enmienda 2*. Describe código ya escrito, y no toca ningún ADR ni reabre la spec.

| Punto | Qué cambia | Dónde | Por qué |
|---|---|---|---|
| Causas de detención | Suma: fila que no pasa RF5 desde `betting_open` (R8), paso de una fila con commit que falla 60 s, cancelación que falla 60 s y pasadas que abortan 60 s | RF15, RF16 | Sin ellas, una fila rota o una base colgada no llegaban nunca a `halted`. R8 sale de datos públicos, así que no delata el resultado |
| Una fila mala no para a las demás | Un error de la base en una fila se reintenta con backoff y las demás siguen. Sin commit nunca detiene. Al arrancar no se aceptan apuestas mientras falla una con commit | RF15, RF16 | Es la misma regla de 60 s que RF18. RF29 sigue cubriendo a la fila que falla, porque su filtro por hora no admite apuestas tras `closes_at` |
| Matiz del anuncio | Una `scheduled` vencida retiene la fila siguiente | RF12 | Su par cuenta como usado hasta su final (RF20). Con los valores por defecto, la fila siguiente aparece al menos 210 s antes de abrir |
| R2 | El secreto tiene que coincidir con el commit al insertarlo, y `betting_open` exige que exista | RF4, RF25 | No se publica un commit sin su semilla. La restauración de la preocupación 13 y la ruta de la preocupación 1 siguen siendo posibles |
| R5 y límite de pasada | `idle_session_timeout`, `tcp_*`, latido en su tarea y pasada de 30 s como máximo | RF10 | Una conexión colgada no debe retener el bloqueo, ni una base colgada ocultar que se perdió |
| `sealed::cancel_fight` | Una cancelación es un intento en su tarea, y borra un secreto plantado | RF16, RF19 | `sealed` sigue siendo el único SQL de `fight_secrets`, y un pánico del gancho no mata al runner |
| `next_action` | Sin parámetro de configuración | RF14, Enmienda fase A | Un cambio de configuración no toca una pelea ya anunciada |
| `log_without_end` | Fuera de la lista del paso 6 de RF59, pendiente para TA-27 | RF59 | Hoy es inalcanzable. Decidirlo aquí sería decidir una negativa de `voided` sin caso real |
| `lock_timeout` de TA-27 | Lo pone TA-27 en su transacción, antes de `lock_open_for_bet` | RF29 | Sin él, un bloqueo de tabla retendría la petición de una apuesta sin límite |

El resto de R1 a R8 (R1, R3, R4, R6 y R7) no cambia ningún contrato de la spec: endurece la barrera de RF24, la verificación de RF7 y la liberación de RF40. Están en `plan-a.md`.

**Para security-reviewer:** R2, R5 y R8 salen de sus recomendaciones. Le toca mirar en el PR #69 las causas nuevas de detención y que a mitad de corrida las otras ventanas sigan abiertas hasta los 60 s. La decisión sobre la preocupación 13 de abajo se revisa en TA-27, con su implementación.

## Decisiones de 2026-10-31
Las tomó quien coordina el pipeline, con la delegación que comunicó del owner ("toma tú la que consideres mejores"). Son decisiones de esa delegación, no una aprobación firmada por el owner. Cierran lo que esta spec dejaba pendiente.

| Tema | Decisión | Motivo |
|---|---|---|
| Preocupación 1 (riesgo del operador) | **Aceptado** | Hay un solo operador, las monedas son ficticias y no hay dinero real (ADR-0005). Se revisa si otra persona obtiene `DATABASE_URL` o si cambia el valor de las monedas |
| Preocupación 3 (sin final automático tras el commit) | **Aceptado** | Se prefiere detener a inventar un final. Sigue la regla de probar el binario anterior antes de anular, y sigue siendo criterio bloqueante de TA-27 |
| Preocupación 13 (secreto perdido) | **Salida con reembolso y sin semilla** | Ver abajo |
| RF6 | La primera entrada sigue como prerrequisito de TA-8 | Sin ella, la tabla no protege nada (preocupación 4) |
| Android | La medición en el dispositivo de referencia de 002 sigue obligatoria antes del primer despliegue público | La de 002 solo se hizo en un iPhone de gama alta |

**Preocupación 13, en detalle.** Primero se intenta restaurar desde un backup. Si es imposible, la pelea termina como anulada, sin revelar la semilla, sin ganador y con reembolso total por el ledger, marcada públicamente como no verificable, por el mismo comando de RF59 y con `admin_audit`.
- **Descartadas:** la detención indefinida, que para todo el vivo, y el SQL manual, que rompe RF8, RF9 y el ledger.
- **Abuso:** da como mucho un reembolso visible, y quien podría hacerlo ya tiene la credencial que cubre la preocupación 1.
- **ADR:** enmienda el punto 4 de ADR-0002 ("en `Settled` se publica la semilla"). Esa enmienda y la implementación van en TA-27, y aceptar la enmienda sigue pasando por el gate de ADRs de `docs/process.md`.
- **Hasta entonces:** RF59 se niega como hoy, y esta spec no cambia de comportamiento. `void_reason`, `fights_reveal_ck`, RF9 y RF25 siguen exigiendo la semilla en toda `voided`.
- **Lo que suma TA-27:** una razón nueva en `VoidReason`, una migración aditiva que admita esa razón sin `revealed_seed`, el marcado público, el paso 2 de RF59 y sus pruebas, y la revisión de security-reviewer.

## Revisión
### architect — 2026-10-31 — Enmienda 2 y decisiones delegadas registradas
- Escribí "Enmienda 2" contra `b9df497`, leyendo el código, sin compilar. Registré las decisiones que me pasó quien coordina, tal como las comunicó: no verifiqué la delegación con el owner. Sin cambios en ningún ADR ni en el código.

### architect — 2026-10-31 — Enmienda (revisión de plan-a), pendiente de security-reviewer
- Ver "Enmienda (fase A, 2026-10-31)". Sin cambios en ningún ADR. La revisé contra el código de `apps/server`, `crates/engine`, `crates/engine-wasm`, `crates/protocol`, `scripts/check-compose.mjs` y `ci.yml`, sin compilar.
- Resultado de security-reviewer, comunicado por quien coordina: aprueba DP-3, DP-4, DP-5, DP-6, DP-7 y DP-10, y bloqueó M6 tras el commit. Apliqué sus cambios en RF59 (pasos 4 y 6, y pruebas de TA-27), la preocupación 3 y la fila M6. También corregí el motivo de DP-5: no era cierto que el método "no se alcanza de otra forma"; la defensa real es que no se obtiene un `FightLog` sin un token prohibido.

### security-reviewer — 2026-10-31 — APPROVE (cuarta ronda)
- Verifiqué: en el código real, `FightLog` con `Debug` y `Serialize`, `Seed` redactado, la allowlist de `telemetry.rs`, `ws/session.rs` y los límites de `ws/handshake.rs`. En Postgres 17.10, `FOR KEY SHARE` no retiene el `UPDATE` del cierre, `FOR SHARE` sí, y un `FOR SHARE` con filtro de hora sin filas no espera.
- Ronda 1, cambios pedidos: riesgo residual real del operador (elegir la semilla solo requiere `DATABASE_URL`), barrera de fuente por tokens, sellado sin tiempos futuros, reglas en la base, snapshot completo con `slug_*` y `max_hp_*`, cierre sin inanición y test 12 sobre todas las rutas. Hallé con una prueba que los `FOR SHARE` encadenados retrasan el cierre sin límite.
- Ronda 2: salida `voided` para lo irrecuperable, frases falsas sobre borrar el secreto, gancho que solo falla por infraestructura, y bytes de la semilla solo desde `FinalLog`.
- Ronda 3: `snapshot_invalid` no debe confundir un binario con fallos con un snapshot corrupto, y tener la CLI equivale a tener la credencial.
- Ronda 4: aprobación condicionada a mover `max_hp_*` al paso 4 de RF59. Lo comprobó la sesión que orquesta el pipeline, sin otra ronda completa, como indicó este revisor.
- Confirmé: la opción (a) basta sin enmendar ADR-0002; una cancelada como final de `Settled` es una lectura válida; RF42 no empeora nada real; `halted` no filtra el resultado.
- La aprobación no acepta los riesgos: las preocupaciones 1, 3 y 13 son decisión del owner antes de la primera apuesta.

### product-owner — 2026-10-31 — APPROVE (tercera ronda)
- Verifiqué: la spec contra el intent, sus criterios de éxito y sus 10 preguntas abiertas, el alcance y la frontera con TA-27.
- Ronda 1, cambios pedidos: la espera real entre peleas, la experiencia entre peleas en RF54, volver a primer plano en móvil, y las respuestas 4 y 7, que prometían más de lo garantizado.
- Ronda 2: estado "detenida" y preocupación 3 pendiente del owner, con salida definida.
- Acepto para el producto que una pelea con commit no se cancele: `voided` cubre lo irrecuperable.

### game-designer — 2026-10-31 — APPROVE (segunda ronda, confirmada tras `voided`)
- Verifiqué: con `just sim`, duraciones (p10/p50/p90 de 50,6/59,4/70,6 s) y neutralidad de una ronda de pares ordenados frente a `sim`. Simulé RF20 con 4, 6 y 8 personajes, con búsqueda exhaustiva con 4. Sin medir: el sesgo de lado en cruces no espejo.
- Ronda 1, cambios pedidos: revanchas a 2 peleas en RF20, cálculo de la espera, y no abrir ventanas nuevas mientras una liquidación falla o hay parada segura.
- Una ronda del matchmaking, el empate y `voided` como reembolso no impiden un parimutuel sano en TA-27.
