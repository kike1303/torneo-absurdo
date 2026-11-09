---
id: "020"
title: Temporadas, leaderboard y revisión anti-granjas
type: feature
status: draft
risk: high
author: claude
linear: TA-28
depends_on: ["012", "019"]
created: 2026-11-08
updated: 2026-11-08
---

# Temporadas, leaderboard y revisión anti-granjas

## Qué y por qué
Las apuestas guardan un `RankedProfit` inmutable por apuesta (012 RF6), pero todavía no se puede ver en ningún lado. Falta el leaderboard de apostadores, el último pilar del torneo de espectadores. Esta feature hace tres cosas:
- Suma ese `RankedProfit` por temporada.
- Lo publica en un leaderboard.
- Le da al admin las herramientas de ADR-0008 D8 para revisar granjas y cerrar cada temporada.

## Decisiones del owner (chat, 2026-11-08)
- **Duración:** un mes calendario, del día 1 a las 00:00 UTC al día 1 siguiente a las 00:00 UTC.
- **Qué se reinicia:** solo el ranking. Los saldos se mantienen.
- **Métrica:** `RankedProfit` (D8). Para aparecer en el leaderboard hace falta estar fuera de carencia y tener `SEASON_MIN_FIGHTS = 10` peleas ganadas o perdidas en la temporada.
- **Granjas:** las señales de D8 sin cambios. La sanción la decide el admin a mano.

## Alcance
- **Entra:**
  - Temporadas y `season_stats`, que se escriben en el gancho y en una reconciliación.
  - El leaderboard público, con API y página.
  - La retención de `ip_hash`.
  - En la fase B, `farm-report`, `exclude` y `close`.
- **No entra:** premios, ver la posición propia fuera del top, el leaderboard de personajes y las revisiones a mitad de temporada.

## Diseño
**Temporada de una pelea.** Es el mes UTC de su `closes_at` (D8). En el gancho, en la reconciliación y en las pruebas se usa la misma fórmula, que no depende del `TimeZone` de la sesión:
- `starts_at = date_trunc('month', closes_at AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'`
- `ends_at = starts_at + 1 mes`
- `id = YYYYMM`

**Datos (una migración por fase).**
- `seasons (id PK, starts_at, ends_at, closed_at NULL)`. Un `CHECK` exige la forma del mes. Un trigger impide reescribir `closed_at` una vez puesto, y que se escriba sin `torneo.actor = 'cli'` (la misma idea que `users_privileged_guard`). El `INSERT` de `tally`, con la columna en NULL, pasa: lo cubre un test.
- `season_stats (season_id, user_id, ranked_profit, settled_fights, excluded_at, excluded_by, excluded_reason)`, con PK `(season_id, user_id)` e índice `(season_id, ranked_profit DESC, settled_fights DESC, user_id)`. Escribir `excluded_*` con un valor exige `torneo.actor = 'cli'` (trigger); el upsert de `tally`, que las deja en NULL, pasa.
- `season_tallies (fight_id PK, season_id)`: marca cada pelea ya sumada. Gracias a ella el gancho, la reconciliación y el backfill nunca suman dos veces ni se pisan.
- En la migración de la fase B, `admin_audit_action_ck` agrega `season_review`, `season_close` y `season_exclude`. Un trigger rechaza una fila `season_review` en cualquiera de estos casos:
  - `actor` no es `cli_attributed` o `admin_id` es NULL.
  - `target` no es una temporada existente.
  - `now() < ends_at`.

**Sumar una pelea (`seasons::tally`, una sola función).** Corre dentro de la transacción del llamador:
1. Crea la temporada con `INSERT … ON CONFLICT DO NOTHING`.
2. Inserta el marcador en `season_tallies` con `ON CONFLICT DO NOTHING RETURNING`. Si no devuelve fila, la pelea ya estaba sumada y no hace nada más.
3. Hace un upsert que, por usuario y ordenado por `user_id` (orden fijo de locks contra interbloqueos), suma su `ranked_profit` y agrega 1 a `settled_fights`. Solo toma filas de `bet_settlements` con `outcome <> 'refunded'`. El upsert nunca toca `excluded_*`.
- **Desde el gancho:** se llama en la rama que ya escribió `pool_settlements` (012 RF8, idempotente), tanto al pagar como al reembolsar. Con un reembolso solo pone el marcador.
- **Temporada cerrada:** igual suma (nunca falla ni pierde el `RankedProfit`) y deja un log `seasons.tally_after_close`. No debería ocurrir nunca, porque `close` vuelve a comprobar las peleas pendientes con lock.
- **Reconciliación:** una pasada al arrancar y otra en el job diario suman toda pelea liquidada que no tenga marcador. La migración no suma nada: así se cubre lo liquidado antes de que existieran las temporadas, y también lo que liquide un binario viejo durante un deploy. Si falla, deja un log con su código y el server arranca igual.
- **Pelea pendiente de una temporada:** la que tiene `closes_at` en `[starts_at, ends_at)` y está sin liquidar, **o** liquidada sin fila en `season_tallies`. `farm-report` y `close` usan esta definición; `close` además corre `tally` sobre las que falten dentro de su transacción antes de volver a comprobar.

**Elegibilidad (al leer, con `T = min(now, ends_at)`).** Para salir en el leaderboard, la cuenta tiene que cumplir todo esto:
- No está baneada ni excluida.
- `settled_fights ≥ 10`.
- `bets::probation::of(T − created_at, peleas ganadas o perdidas con closes_at < T)` es `None`. Las dos cifras salen de un fragmento SQL que recibe `T`, no de `facts.rs`, que usa `now()`. Es la única función que decide la carencia.

Con `T = ends_at`, quien termina la temporada en carencia no aparece en su clasificación final, y en la temporada nueva empieza de cero (test 27). El acumulado de la carencia aparece completo al salir de ella, porque siempre se suma y el filtro se aplica solo al leer.

**API pública** (`FIGHT_READ_IP`, `no-store`, como `/api/fights`).
- `GET /api/leaderboard?season=YYYYMM` devuelve `Leaderboard { season: SeasonView, entries: Vec<LeaderboardEntry>, min_fights }`.
  - `LeaderboardEntry { rank, display_name, ranked_profit: CoinAmount, settled_fights }`, sin `user_id`.
  - `season` se valida en el borde (año de cuatro cifras, mes de 01 a 12). Si está mal formado, responde 404 `season.not_found` sin tocar la base; si no existe, el mismo 404 tras consultarla.
  - Sin `season`, usa la temporada del momento.
  - Lee candidatos por el índice en tandas de 100, filtra la elegibilidad en Rust y se detiene al llegar a 50 o cuando no quedan.
- `GET /api/seasons` devuelve `Vec<SeasonView { id, starts_at, ends_at, closed }>`, la más nueva primero.

**Web** (`routes/leaderboard/`, pública, enlazada desde la home junto a las repeticiones: el header no tiene lugar a 375 px).
- Muestra la tabla y un selector de temporada.
- Una nota estática y general explica quién entra (fuera de carencia y con 10 peleas). Nunca dice por nombre que alguien está en carencia, excluido o baneado.
- Mobile-first, `Intl`, i18n `es`/`en` y tokens del theme.

**Retención (código de `jobs.rs`).** `auth_events` pasa de 30 a 38 días, que es `max(30, 31 + 7)` de D8. Se actualiza el doc del módulo. El comentario de `auth_events` en la migración de 008 queda como historia (editarlo cambiaría su checksum de sqlx), y la nota va en la migración de esta feature. La ventana operativa de la revisión es hasta `ends_at + 7 d`.

**CLI (fase B).** Tres órdenes con las mismas reglas:
- `--by` exige `role = 'admin' AND banned_at IS NULL`. Se alinea `fight void` con la misma regla.
- Escriben con `actor = 'cli_attributed'` y `torneo.actor = 'cli'`.
- No tienen endpoint HTTP. Cada negativa sale con su código y no escribe nada.

Las órdenes:
- `server season farm-report <id> --by <admin>`:
  - Se niega antes de `ends_at` y si queda alguna pelea pendiente de la temporada (definición de arriba).
  - Imprime JSON solo con códigos: las peleas donde cuentas del mismo grupo de IP apostaron a lados opuestos y, para el top 20, la concentración de contrapartes (umbral 50 %) y si comparten grupo.
  - Los grupos son etiquetas opacas por reporte (`ip_group: 1, 2…`). Nunca salen los valores de `ip_hash`, ni en stdout, ni en `payload`, ni por `tracing`.
  - Compara el `auth_events` más viejo con `starts_at`. Si no cubre toda la temporada, marca `coverage: "partial"`.
  - Escribe **una fila `season_review` por ejecución**, con conteos, `user_id` marcados y `coverage` en `payload`. No hay `UNIQUE` por temporada.
- `server season exclude <id> <user> --by <admin> --reason <code>`:
  - `reason` sale de un enum cerrado (`farm_signals`, `shared_ip_opposite_sides`, `counterpart_concentration`).
  - Se niega si la temporada ya está cerrada (`season.closed`) o si el usuario no tiene fila en ella (`season.no_stats`).
- `server season close <id> --by <admin> [--accept-partial]`:
  - Toma la fila de `seasons` con `FOR UPDATE`, suma las peleas liquidadas sin marcador y vuelve a comprobar que no queden peleas pendientes.
  - Usa la `season_review` más reciente que cumpla `at ≥ ends_at` y `at ≥ now − 48 h`. Si es `partial`, la acepta solo con `--accept-partial`, y el flag queda en `admin_audit`.
  - Hace `UPDATE … WHERE closed_at IS NULL`. Si no cambia ninguna fila, responde `season.already_closed`.

## Fases
- **A (PR 1):** migración, `tally` (gancho y reconciliación), retención, `/api/leaderboard`, `/api/seasons` y la página.
- **B (PR 2):** `farm-report`, `exclude` y `close`, con el test 28 y los chequeos de fuente.

## Riesgos
- **Doble suma o suma perdida:** el marcador `season_tallies` y la reconciliación. Invariante con test: `season_stats` es igual a la suma de `bet_settlements` no reembolsados agrupada por temporada.
- **El gancho falla (RF31):** fórmula fija en UTC, locks en orden de `user_id` y ninguna rama que falle por una temporada cerrada.
- **Revisión falsa o incompleta:** dos barreras para `season_review` (chequeo de fuente y trigger), `coverage` y re-chequeo con lock al cerrar.
- **Datos personales:** el leaderboard muestra `display_name`, que ya es público (ADR-0008 D1), y nunca `user_id` ni `ip_hash`. `farm-report` usa etiquetas opacas.
- **Leaderboard casi vacío** con poca población (aviso de 012): la nota estática explica quién entra.
- **Con `r = 0`** el riesgo residual de una granja sube a `N · (B + R)`, y `farm-report` lo revisa al cierre.

## Pruebas
- [ ] `just check` en verde.
- [ ] Test 25 sobre el agregado: la invariante con `bet_settlements`. Reliquidar, reconciliar y volver a correr la reconciliación no suman dos veces.
- [ ] Test 27:
  - La pelea que cierra antes de `ends_at` y se liquida después cuenta en su temporada.
  - El acumulado de la carencia aparece al salir de ella.
  - La carencia que cruza el cambio de temporada no sale en la final vieja y en la nueva empieza de cero.
  - Con `SET TIME ZONE 'America/Mexico_City'`, una pelea que cierra a las 00:30 UTC del día 1 cae en el mes nuevo.
- [ ] Dos liquidaciones concurrentes con usuarios en común no se interbloquean.
- [ ] Una temporada cerrada no hace fallar el gancho.
- [ ] Leaderboard:
  - Elegibilidad por carencia, mínimo de peleas, baneo y exclusión.
  - Orden y desempate.
  - Sin `user_id` en la respuesta.
  - 404 con un `season` mal formado o inexistente.
- [ ] Fase B, test 28:
  - Cada negativa de `close` y de `farm-report`, incluida una pelea liquidada sin marcador en `season_tallies`.
  - Una fila de `season_review` por ejecución, y ninguna en las negativas.
  - El trigger y el chequeo de fuente encuentran el token `season_review`.
  - Ningún byte de un `ip_hash` sembrado aparece en la salida ni en `payload`.
  - `coverage: partial` y `--accept-partial`.
- [ ] Capturas a 375 px y en escritorio, claro y oscuro.
