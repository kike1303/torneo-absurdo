---
id: "021"
title: Botarga propia y PvP asíncrono
type: feature
status: draft
risk: high
author: claude
linear: TA-60
depends_on: ["011", "012", "016", "020"]
created: 2026-11-09
updated: 2026-11-09
---

# Botarga propia y PvP asíncrono

## Qué y por qué
Cada usuario recibe una botarga genérica y la hace suya: sube de nivel, arma su build con lo que desbloquea y busca pelea contra la botarga de otro usuario real, esté conectado o no. El server resuelve la pelea al instante con el motor (ruleset v2), mueve el rating de los dos y guarda la semilla para que cualquiera la vea después como repetición. Es el paso 4 del [backlog](../../backlog.md) y la fase 5 que la feature [016](../016-botarga-del-jugador/feature.md) dejó pendiente. Hoy no existe nada de esto de cara al usuario: solo el motor, los catálogos y el simulador de builds.

## Decisiones del owner (chat, 2026-11-09)
- **Rendimientos decrecientes por día UTC:** las primeras 10 peleas como atacante dan recompensa completa; de la 11 a la 20 bajan por escalones (75 %, 50 %, 25 %); desde la 21 no dan XP ni monedas ni mueven el rating. Nunca hay un muro: se puede seguir peleando.
- **Rating:** Elo con 1000 de inicio, K=40 en las primeras 30 peleas y K=20 después.
- **Temporadas:** el mismo mes UTC que el torneo (020). Al cerrar, el rating se comprime hacia 1000; nivel, XP e inventario se conservan siempre.
- **Sin apuestas en el PvP** en esta versión: la pelea se resuelve al instante y no hay ventana. Queda en el backlog.
- **Sin rival:** se amplía el rango de rating y nivel por pasos hasta cualquier usuario; si no hay ningún otro usuario con botarga, se pelea contra un personaje del torneo escalado al nivel del jugador, que da XP y no mueve el rating.
- **Nombre:** la botarga usa el `display_name` del usuario. No hay un segundo nombre.
- **Defensor:** recibe el cambio de rating contrario al del atacante y la mitad de la XP; nunca gana ni pierde monedas, y sus peleas como defensor no cuentan para su límite diario.
- **Números** de XP, monedas, Elo y reinicio: los propone el `game-designer` (sección "Números") y los aprueba el owner al revisar este documento.

## Alcance
- Entra: la botarga por usuario con su alta en el onboarding, armar la build, niveles y sorteo de ataques, buscar pelea, el historial de peleas PvP con repetición verificable, el rating con su leaderboard y las temporadas PvP, y las señales anti-granjas para el admin.
- No entra: prendas e inventario (TA-66 y TA-67, sobre esta base), la tienda (TA-68), amigos y torneos privados (TA-69), apostar en peleas PvP, el chat y las notificaciones push. El torneo no cambia: sigue en ruleset v1 con su roster.

## Diseño

**Dos flujos, un motor.** El PvP vive en módulos propios del server (`botarga/`, `pvp/`) y en tablas propias. No toca `fights`, el scheduler ni el gancho de liquidación del torneo. Comparte el motor, el ledger, las sesiones, los límites y la verificación en el navegador de 011.

**Datos (una migración por fase).**
- `botargas (user_id PK → users, level, xp, stats {hp, attack, defense, speed, special}, style_id, abilities text[], accessories text[], cosmetics {idle, victory}, body_type, face_id, rating, rated_fights, created_at, updated_at)`. Una por usuario. `CHECK` de rangos: nivel 1 a 100, stats dentro del presupuesto del nivel (lo vuelve a comprobar el server con `engine::loadout::resolve`), rating de 0 a 4000.
- `botarga_unlocks (user_id, kind {ability, accessory, cosmetic}, item_id, source {starter, draw, level, torneo}, unlocked_at)`, PK `(user_id, kind, item_id)`. Lo que el jugador tiene. Es la base del inventario de TA-67.
- `botarga_draws (id, user_id, level, options text[3], chosen text NULL, created_at, chosen_at NULL)`. Un sorteo pendiente por usuario (índice único parcial con `chosen IS NULL`). Las opciones se escriben en la misma transacción que sube el nivel.
- `pvp_fights (id, attacker_id, defender_id NULL, defender_slug NULL, ruleset_version, spec_a, spec_b, seed, log, winner, end_reason, rating_delta, xp_attacker, xp_defender, coins_attacker, daily_index, season_id, created_at)`. `defender_id` nulo solo con `defender_slug` (personaje del torneo). La semilla se guarda en claro desde el inicio: aquí no hay ventana de apuestas que proteger (ADR-0002 aplica al torneo).
- `pvp_daily (user_id, day date, attacks smallint)`, PK `(user_id, day)`: el contador de rendimientos decrecientes, en la base y no en memoria.
- `pvp_seasons (id YYYYMM PK, starts_at, ends_at, closed_at NULL)` y `pvp_season_ratings (season_id, user_id, rating, rated_fights, wins, losses)` para el leaderboard histórico; la temporada en curso se lee de `botargas`.

**Alta.** Al crear el usuario (mismo flujo que hoy termina en `/onboarding`), en la misma transacción se inserta su `botargas` con nivel 1, 200 puntos repartidos como la botarga genérica, el estilo `balanced`, el strike y el pesado inicial (`Catalog::starter`), una ranura abierta, rating 1000 y los desbloqueos `starter`. Un usuario existente sin fila la recibe al primer `GET /api/botarga` (idempotente con `ON CONFLICT DO NOTHING`), así la migración no recorre `users`.

**Build.** `PUT /api/botarga/build` recibe `{stats, style_id, abilities, accessories, cosmetics}`. El server comprueba en el borde que cada id está en `botarga_unlocks` y en el catálogo, arma un `engine::loadout::Loadout` y llama a `resolve` con el ruleset v2; un `LoadoutError` se devuelve como su `code()`, nunca como texto. Los stats se reparten libres dentro del presupuesto del nivel con el mínimo por stat de v2. Cambiar la build es gratis e ilimitado (límite `MUTATION_USER`).

**Niveles y sorteo.** La XP se suma en la transacción de la pelea. Si cruza un umbral, en esa misma transacción sube el nivel, se abren las ranuras de `SLOT_LEVELS`, se otorgan los puntos de presupuesto (quedan sin asignar hasta que el jugador los reparte) y, en los niveles de `DRAW_EVERY`, se crea el `botarga_draws` con 3 ids que el jugador no tiene, del tier de su nivel o inferior, elegidos con `OsRng` del server (nunca el PRNG del motor). Si quedan menos de 3, se ofrecen los que queden; si ninguno, el sorteo otorga un cosmético. `POST /api/botarga/draw/choose {draw_id, item_id}` acepta solo una opción del sorteo pendiente; repetir la llamada devuelve el mismo resultado. Varios niveles de golpe encolan sus sorteos en orden.

**Buscar pelea.** `POST /api/pvp/fights {client_key}` (`Effect::Mutate`, `Origin`, límite nuevo `PVP_USER` 10/min ráfaga 3 e IP 120/min), en una sola transacción con `SET LOCAL lock_timeout = '1s'`:
1. `client_key` repetido devuelve la pelea ya creada.
2. Bloquea la fila de `botargas` del atacante (`FOR UPDATE`) y lee `pvp_daily` del día UTC: `daily_index = attacks + 1`.
3. Elige al defensor con los pasos y filtros de la sección "Números" (ventanas de rating y nivel que se amplían, sin los últimos 3 rivales). Entre los candidatos de un paso se elige con `OsRng`. Sin ningún candidato tras el último paso, el defensor es un personaje del torneo escalado al nivel del atacante (presupuesto del nivel, mismas proporciones de stats) y `defender_id` queda nulo.
4. Semilla de 32 bytes con `OsRng`. Resuelve los dos `Loadout` a `CharacterSpec` y llama a `simulate` con el ruleset v2. El atacante es siempre el lado `a`.
5. Calcula rating, XP y monedas con el escalón del `daily_index`, el tope por pareja y el tope de XP de defensa (sección "Números"). Escribe `pvp_fights`, actualiza las dos `botargas` (rating, `rated_fights`, XP, nivel), `pvp_daily`, y las monedas del atacante **solo como asiento del ledger** (`kind = 'pvp_reward'`, de `issuance` al usuario). La fila del defensor se bloquea después de la del atacante, en orden de `user_id`, contra interbloqueos.
6. Responde `PvpFightCreated { fight: PvpFightDetail, progress: BotargaProgress }` con el log completo: es público desde que existe.

Un defensor nunca pierde monedas ni cuenta en su `pvp_daily`. Un atacante no puede ser elegido como defensor mientras su transacción está abierta (su fila está bloqueada; el emparejamiento usa `SKIP LOCKED`).

**Lectura.** `GET /api/botarga` (mi botarga, build, sorteo pendiente y progreso del día), `GET /api/pvp/fights?before=<id>` (mis peleas como atacante y como defensor, 20 por página, con lo que pasó mientras no estaba), `GET /api/pvp/fights/{id}` (detalle público con `spec_a`, `spec_b`, `seed`, `log` y `ruleset_version`: lo que `verifyFight` de 011 necesita; nunca `user_id`, solo `display_name`), `GET /api/pvp/leaderboard?season=YYYYMM` (top 50 por rating con al menos 10 peleas puntuadas en la temporada, sin baneados ni excluidos) y `GET /api/pvp/seasons`. Todas con `Cache-Control: no-store` y el límite `FIGHT_READ_IP`.

**Temporadas PvP.** Mes UTC con la fórmula de 020. El job diario cierra la temporada vencida: copia `rating`, `rated_fights`, `wins` y `losses` a `pvp_season_ratings`, aplica el reinicio suave a `botargas.rating` y escribe `closed_at`. Idempotente: con `closed_at` puesto no hace nada. `server pvp farm-report <season>` da al admin las señales de la sección "Números"; la sanción sigue siendo manual (ban o exclusión del leaderboard, en `admin_audit` con `pvp_exclude`).

**Protocolo** (`crates/protocol/src/pvp.rs` y `botarga.rs`, `just gen`): `BotargaView`, `BotargaBuild`, `BotargaProgress { level, xp, xp_to_next, budget_free, slots_open, daily { attacks, factor_pct } }`, `PendingDraw`, `PvpFightSummary`, `PvpFightDetail`, `PvpFightCreated`, `PvpLeaderboard`, `PvpLeaderboardEntry`, `FighterCosmetics { idle, victory }` (016, fuera del hash canónico) y los códigos `BotargaErrorCode` y `PvpErrorCode`. Sin texto para humanos.

**Web** (`lib/features/botarga/`, `lib/features/pvp/`, `lib/components/botarga/`; mobile-first, 44 px, i18n es/en, tokens, `prefers-reduced-motion`).
- `/botarga`: la botarga en 3D con `FightStage` en idle (cosméticos aplicados), nivel y barra de XP, puntos libres, build por ranuras y estilo. Cada cambio es un form action. Con sorteo pendiente, un panel modal de 3 cartas bloquea el resto hasta elegir.
- Botón **buscar pelea** en `/botarga`: tras la respuesta, el reproductor de 011 en pausa en `t = 0` con los mismos controles y la misma regla de spoilers (resultado en `aria-live` al terminar o con el botón "ver el resultado"), y debajo el cambio de rating, XP y monedas, con el factor del día si no es completo.
- `/pvp/fights`: mis peleas, con las de defensor marcadas y lo que pasó desde la última visita. `/pvp/fights/{id}`: la repetición con la verificación de 011. `/pvp/leaderboard`: tabla y selector de temporada, con la nota de quién entra.
- Enlaces desde la home junto a repeticiones y leaderboard del torneo. `/botarga` y `/pvp/fights` exigen sesión; las repeticiones y el leaderboard son públicos.

## Números
Propuesta del `game-designer` (2026-11-09), con enteros y división hacia abajo. Referencias de 012: bono diario `B = 500`, rescate `R = 250`, apuesta mínima `m = 25`.

**Escalones del día** (atacante; aplican a XP, monedas y rating): peleas 1 a 10 → 100 %; 11 a 13 → 75 %; 14 a 16 → 50 %; 17 a 20 → 25 %; 21 o más → 0 %. La pelea se resuelve y se guarda igual.

**XP por nivel:** `xp_to_next(level) = 100 + 20 · level`; acumulada `xp_total(level) = (level − 1) · (100 + 10 · level)`. Con 10 peleas al día y 50 % de victorias: nivel 5 el primer día, 15 en una semana, 30 en 18 días, 100 en unos 5,5 meses (4 si además te atacan). Lineal para que del 60 al 100 cada nivel cueste 2 o 3 días y no una semana.

| XP por pelea | Base | Nota |
|---|---|---|
| Victoria / derrota del atacante | 100 / 30 | 10 victorias = 1000 XP, fácil de leer |
| Defensa ganada / perdida | 50 / 15 | La mitad, de la base ya multiplicada por el escalón del atacante; contra un mismo atacante se paga como mucho 3 veces al día |
| Contra personaje del torneo, victoria / derrota | 70 / 20 | No debe convenir jugar sin rivales; cuenta para el límite diario |
| Bono por rating (solo victorias) | `clamp(rating_rival − rating_propio, −200, 200) / 4` | De −50 a +50; en derrota no aplica, así perder a propósito contra fuertes no paga |

**Monedas:** `PVP_WIN_COINS = 15` por victoria del atacante contra un humano (variable del server, validada al arrancar, 0 permitido); 0 por derrota, por defensa y contra el personaje del torneo (su oferta es infinita y sería la única fuente granjeable en solitario). Asiento `pvp_reward` de `issuance` al usuario, uno por pelea. No entra en `RankedProfit`. A tope (10 victorias) agrega 150 al día, el 30 % de `B`; en uso realista, 75. Frente a la masa de 012 (55 `B` por usuario activo en D90 con rake 0) la empuja a unos 60 a 65 `B`: adelanta la revisión del rake (TA-59), no la rompe.

**Elo en enteros:** rating `i32` desde 1000, K=40 hasta 30 peleas puntuadas y K=20 después. Esperanza en milésimas por tabla de 33 entradas (`d = rating_rival − rating_propio`, un escalón cada 25 puntos hasta 800: 500, 464, 429, 394, 360, 327, 297, 268, 240, 215, 192, 170, 151, 133, 118, 104, 91, 80, 70, 61, 53, 46, 40, 35, 31, 27, 23, 20, 17, 15, 13, 11, 10; simétrica para `d < 0`). `delta_win = (K · (1000 − E) + 500) / 1000`, `delta_loss = −(K · E + 500) / 1000`, multiplicados por el escalón truncando hacia cero; una pelea que cuenta mueve al menos 1. Ejemplo: 1000 contra 1300, K=40: victoria +34, derrota −6. Tabla y no `10^(d/400)` para que el server nunca persista nada que salga de `f64`.

**Emparejamiento:** pasos de `|Δrating| ≤ 100, 200, 300, 400`, cada uno con `|Δnivel| ≤ gap(level) = min(10, 3 + level / 5)` del atacante (nivel 1 a 9: 3 o 4; nivel 20: 7; 35 o más: 10, porque 016 midió que 10 niveles dan 58 % a 65 % al de arriba y abajo del 30 cada nivel abre ranuras). Sin candidato tras el paso 4, el personaje del torneo. Filtros: no a uno mismo ni a baneados, no a los últimos 3 rivales (si deja la lista vacía se ignora), preferir cuentas con sesión en los últimos 30 días. **Una misma pareja mueve rating como mucho 3 veces por día UTC**: la cuarta se resuelve y da XP, con `delta = 0`.

**Reinicio suave:** al cerrar el mes, `rating' = 1000 + (rating − 1000) / 2`. Conserva el orden y recorta la distancia a la mitad: el 1600 arranca en 1300 (65 % contra uno nuevo, no 97 %) y a K=20 esa distancia se recupera en unas 15 victorias netas. El contador para K no se reinicia. Entra al leaderboard quien tenga 10 peleas puntuadas en la temporada, como `SEASON_MIN_FIGHTS`.

**Señales de `farm-report`** (etiquetas opacas como en 020, `ip_hash` como señal y nunca prueba): pareja repetida (10 o más peleas entre las mismas cuentas en la temporada, o la pareja es la mitad de las peleas del atacante); saco de boxeo (nunca ataca, pierde el 90 % de 8 o más defensas, el 80 % contra la misma cuenta); cuenta nueva unidireccional (menos de 7 días o 30 peleas, pierde el 90 % contra una cuenta y gana el 40 % contra el resto); concentración de rating ganado a las 5 contrapartes principales, en especial del mismo grupo de `ip_hash`; hundimiento y repunte (cae más de 200 en 30 peleas y luego gana el 80 % de las 30 siguientes); monedas PvP de una cuenta que apuesta siempre contra una misma cuenta del top 20; racha contra el torneo (la mitad o más de las peleas del mes, informativa).

**Pendientes del owner al revisar:** (a) `PVP_WIN_COINS = 15` o 0 al inicio, por la revisión del rake; (b) el tope de 3 peleas puntuadas por pareja y día, la única regla dura nueva.

## Fases
Una issue por fase en el proyecto de Linear "Fase 3 — Juego propio". Cada una es un PR con `just check` en verde; las de monedas y rating pasan por el `security-reviewer`.
1. **TA-61** Botarga en el server: migración, alta en el onboarding, `GET /api/botarga` y `PUT /api/botarga/build`, `FighterCosmetics` en `protocol`.
2. **TA-63** Buscar pelea: `pvp_fights`, emparejamiento, resolución, `pvp_reward` en el ledger, lecturas y `fights_no_leaks` sobre las rutas nuevas.
3. **TA-62** Niveles, XP y sorteo de ataques.
4. **TA-64** Rating, rendimientos decrecientes, leaderboard, temporadas PvP y `farm-report`.
5. **TA-65** UI: `/botarga`, buscar pelea, `/pvp/fights`, repetición y leaderboard; e2e del flujo completo.

## Riesgos
- **Monedas por volumen:** el `pvp_reward` sale de `issuance` y no de otro usuario, así que infla la masa. Lo acota el factor diario (tope duro de 10 recompensas completas por día) y el test de conservación del ledger, que suma `pvp_reward` a los grants permitidos.
- **Granjas de rating:** dos cuentas que pelean en bucle o cuentas nuevas que solo pierden contra una misma cuenta. Se mitiga con el emparejamiento (sin los últimos 3 rivales), el tope de 3 peleas puntuadas por pareja y día, K bajo tras 30 peleas y las señales de `farm-report`; la sanción es manual, como en 020.
- **Determinismo y verificación:** el log de `pvp_fights` tiene que re-simularse idéntico en el navegador. Mismo contrato que 011: `spec_a`, `spec_b`, `seed`, `ruleset_version` y `log` públicos; un test re-simula cada pelea de prueba con el WASM.
- **Entrada del usuario:** la build y la elección del sorteo llegan del cliente. El server valida contra `botarga_unlocks`, el catálogo y `resolve`; el sorteo no se repite nunca.
- **Interbloqueos:** dos `FOR UPDATE` por pelea. Orden fijo por `user_id` y `lock_timeout` de 1 s; un fallo responde `pvp.busy` y el cliente reintenta con el mismo `client_key`.
- **Arranque frío:** con un solo usuario, todas las peleas son contra el torneo y no mueven rating. El leaderboard puede quedar vacío el primer mes; la nota de elegibilidad lo explica.

## Pruebas
- [ ] `just check` en verde en cada fase.
- [ ] Integración: un usuario nuevo tiene botarga al terminar el onboarding; una build con un id no desbloqueado, fuera de presupuesto o con 3 especiales se rechaza con su código.
- [ ] Integración: buscar pelea con dos usuarios mueve los dos ratings en sentido contrario y con la misma magnitud, da monedas solo al atacante y solo por asiento, y la segunda llamada con el mismo `client_key` devuelve la misma pelea.
- [ ] Integración: la pelea 11 del día da el 75 %, la 21 da 0 y no mueve rating; una pelea como defensor no cuenta en `pvp_daily`; la cuarta pelea del día entre la misma pareja deja `delta = 0`.
- [ ] Unitario: la tabla de Elo es simétrica, `delta_win` y `delta_loss` dan +34 y −6 para 1000 contra 1300 con K=40, y una pelea que cuenta nunca da 0.
- [ ] Integración: sin otros usuarios, el rival es un personaje del torneo, da XP y no mueve rating.
- [ ] Integración: el sorteo nunca ofrece un ataque que el jugador tiene, repetir `choose` devuelve lo mismo, y dos niveles de golpe encolan dos sorteos.
- [ ] Conservación del ledger con `pvp_reward` incluido; `fights_no_leaks` cubre las rutas nuevas y ningún cuerpo lleva `user_id`.
- [ ] Vitest: `verifyFight` da `match` con un `PvpFightDetail` real; el cierre de temporada aplica el reinicio suave y es idempotente.
- [ ] e2e: onboarding → `/botarga` → buscar pelea → repetición verificada, a 375 px y escritorio, claro y oscuro.
