---
id: "023"
title: Amigos, retos y torneos privados
type: feature
status: approved
risk: high
author: claude
linear: TA-69
depends_on: ["021"]
created: 2026-11-15
updated: 2026-11-15
---

# Amigos, retos y torneos privados

## Qué y por qué
Hoy un jugador solo pelea contra quien le toca el emparejamiento del PvP. Con esta feature agrega amigos con un enlace de invitación, los reta a una pelea directa y arma un torneo de eliminación directa entre amigos. Todo se resuelve al instante con el motor, como "buscar pelea", y se ve en repetición verificada. Es el paso 5 del [backlog](../../backlog.md). Nada de esto da rating, XP, niveles ni monedas: es para jugar con amigos, y así nadie se deja ganar para subir otra cuenta.

## Decisiones del owner (chat, 2026-11-15)
- **Invitar:** cada usuario tiene un código propio, que comparte como enlace. Abrirlo y aceptar con un botón los hace amigos. No hay búsqueda por nombre ni solicitudes de desconocidos.
- **Quitar:** quitar a un amigo le impide volver a aceptar mi enlace. Los demás lo siguen usando. Para volver a ser amigos, el que quitó acepta el enlace del otro.
- **Privacidad:** los retos y los combates de torneo solo los ven sus participantes. El PvP con ranking sigue público.
- **Torneos:** eliminación directa con brackets de 4 u 8 botargas. El server los resuelve al instante, sin ranking ni XP.
- **Topes:** 50 retos y 10 torneos por usuario y día UTC.
- **Empate en un torneo:** pasa el lado que indica el bit bajo del primer byte de la semilla pública de esa pelea (0 = `a`), así que se puede verificar.
- **Monedas:** ninguna en esta versión. La segunda moneda se diseña con la tienda (TA-68).

## Alcance
- Entra: código de invitación (ver, rotar), aceptar, lista de amigos, quitar con bloqueo. Retos 1 vs 1. Torneos de 4 u 8. Las pantallas `/friends`, `/friends/invite`, `/friends/tournaments/{id}` y `/friends/fights/{id}`, con la verificación de 011.
- No entra: monedas o apuestas, búsqueda o sugerencias, chat, notificaciones, otros formatos de torneo y espectadores ajenos.
- Hace falta botarga para tener amigos. La página de invitación la crea con `POST /api/botarga` si falta y vuelve.

## Diseño
**Separado del PvP con ranking.** Los retos y los combates de torneo viven en sus propias tablas, no en `pvp_fights`. Así ninguna lectura de 021 los ve: carencia, rivales recientes, tope por pareja, `pvp_daily`, `client_key`, leaderboard, cierre y `farm-report` siguen igual. Ningún flujo de esta feature llama a `store::rate`, `grant_xp` ni `ledger::pvp_reward`: son funciones propias en `apps/server/src/friends/`.

**Datos (migración de la fase 1; las filas son inmutables salvo donde se dice, con un guard `TA014`).**
- `friend_invites (user_id PK → botargas, code text UNIQUE, created_at)`. El código son 128 bits de `OsRng` en base64url (22 caracteres). Se guarda en claro para que su dueño lo pueda volver a copiar: solo sirve para hacerse amigo y se puede rotar, así que el riesgo es bajo. Nace con la botarga (alta y `POST /api/botarga`) y también con `POST rotate`, que reemplaza la fila. Ningún `GET` escribe.
- `friendships (user_lo, user_hi → botargas, created_at)`, con PK `(user_lo, user_hi)` y `CHECK (user_lo < user_hi)`. Quitar a un amigo borra la fila.
- `friend_blocks (owner_id, blocked_id → botargas, created_at)`, con PK `(owner_id, blocked_id)`. Quitar escribe uno; aceptar el enlace de `owner_id` siendo `blocked_id` da el mismo 404 que un código inválido; aceptar el enlace del bloqueado borra el bloqueo.
- `friend_fights (id, kind 'challenge'|'bracket', attacker_id, defender_id → botargas, client_key NULL, ruleset_version, spec_a, spec_b, seed, log, winner, end_reason, look_a, look_b, tournament_id NULL, round NULL, slot NULL, advances NULL, created_at)`, inmutable:
  - un reto lleva `client_key`, con `UNIQUE (attacker_id, client_key)`;
  - un combate de torneo lleva `tournament_id`, `round` y `slot`, con `UNIQUE (tournament_id, round, slot)`, y `advances IN ('a','b')`;
  - `CHECK`s: `kind = 'challenge'` exige `client_key` y deja nulos `tournament_id`, `round`, `slot` y `advances`, y `'bracket'` es al revés; `advances = COALESCE(winner, CASE get_byte(seed, 0) & 1 WHEN 0 THEN 'a' ELSE 'b' END)`, así que la base también garantiza el desempate verificable; `round` cabe en el tamaño del torneo.
- `friend_tournaments (id, owner_id, size 4|8, client_key, created_at)`, con `UNIQUE (owner_id, client_key)`. `friend_tournament_slots (tournament_id, slot 0..size−1, user_id)`, con `UNIQUE (tournament_id, user_id)`. El dueño ocupa un slot.
- `friend_daily (user_id, day, challenges ≤ 50, tournaments ≤ 10)`, con sus `CHECK`.

**Locks.** El reto y el torneo bloquean solo al actor: `lock_user` (que exige `banned_at IS NULL`) y su botarga, con `lock_timeout` de 1 s. Las botargas de los amigos se leen sin lock, porque no se escriben. Se conserva así el invariante de 021: el actor es la única fila bloqueada con espera, y no hay ciclos con las peleas ni con el cierre de temporada. Aceptar y quitar bloquean las dos filas de `users` en orden de `user_id`. Aceptar cuenta el tope de 200 amigos de los dos bajo ese lock. Si A y B se quitan a la vez, el segundo ya no ve la amistad y no escribe su bloqueo. En el reto y el torneo, el `client_key` se busca antes de mirar el tope de `friend_daily`, como en 021, así que un reintento después del tope devuelve su pelea.

**Rutas** (con sesión; las escrituras con `Effect::Mutate`, `Origin` y respuestas `private, no-store`; nunca un id de usuario; los amigos se nombran por `display_name` y se buscan por `display_name_key` solo entre mis amigos):
- `GET /api/friends`: mis amigos (`display_name`, nivel y `ready`) y mi código (o `null`). `ready` es un booleano sin motivo: un baneado y una build que ya no resuelve se ven igual.
- `POST /api/friends/invite/preview {code}`, solo con sesión: el `display_name` del dueño. Tiene la cuota de `FRIENDS_ACCEPT` y el mismo 404 único que `accept`.
- `POST /api/friends/invite/rotate`.
- `POST /api/friends/accept {code}`:
  - 404 `friends.invite_not_found` con un código mal formado, inexistente, propio, de un baneado o del que me bloqueó (un solo código, sin oráculo);
  - 409 `friends.already_friends`;
  - 422 `friends.limit` con 200 amigos.
- `POST /api/friends/remove {display_name}`: 404 si no es amigo.
- `POST /api/friends/challenge {display_name, client_key}`: una pelea `challenge` contra la botarga del amigo.
  - 404 `friends.not_friend` si no es amigo o está baneado.
  - 409 `friends.build_invalid` si su build o la mía ya no resuelve.
  - 429 `friends.daily_limit`.
  - Repetir el `client_key` devuelve la misma pelea.
- `POST /api/friends/tournaments {size, display_names[], client_key}`: el dueño y `size − 1` amigos distintos.
  - El orden del bracket se sortea con `OsRng`.
  - En cada combate, el lado `a` es el del slot menor.
  - Las rondas se resuelven en una transacción.
  - Errores: 422 `friends.tournament_invalid` (tamaño, repetidos, no amigos o baneados, con un solo código), 409 `friends.build_invalid` y 429.
  - Repetir el `client_key` devuelve el mismo torneo.
- `GET /api/friends/tournaments`: los torneos donde participé, paginados.
- `GET /api/friends/tournaments/{id}`: el bracket, con cada combate (ronda, slot, nombres, id de la pelea y quién pasó) y el campeón. 404 si no participé.
- `GET /api/friends/fights?before=` y `GET /api/friends/fights/{id}`: mis retos y combates, y su detalle con la forma de `PvpFightDetail` (lo que necesita `verifyFight`). 404 si no participé.
- `GET /api/friends/fights` y `GET /api/friends/tournaments` responden `FriendFightsPage` y `FriendTournamentsPage`, de 20 en 20.
- Cuotas:
  - `FRIENDS_USER` (10/min, ráfaga 3): retos y torneos.
  - `FRIENDS_ACCEPT` (5/min por usuario y 20/min por IP): `preview` y `accept`.
  - `MUTATION_USER`: `rotate` y `remove`.
  - La cuota de `GET /api/botarga`: las lecturas.

**Auditoría:** un `auth_events` por reto (`friend_challenge`, con `friend_fight_id`), por torneo (`friend_tournament`, con `friend_tournament_id`) y por aceptación (`friend_accept`), siempre a nombre del actor y con su `ip_hash`. Cada referencia lleva un `CHECK` como `pvp_fight_id`. Nunca es un `pvp_attack`.

**Protocolo:** `FriendsView`, `FriendEntry`, `FriendInvitePreview`, `FriendTournament`, `BracketMatch`, `FriendFightSummary`, `FriendFightsPage`, `FriendTournamentsPage` y `FriendsErrorCode` (las variantes de arriba). El detalle reutiliza `PvpFightDetail`, con `rating_delta` siempre en 0. Cada fase de server corre `just gen`.

**Web:** en `/friends`:
- mi enlace (copiar y rotar) y la lista, con "retar" y "quitar";
- "armar torneo" eligiendo 3 o 7 amigos listos;
- mis retos y mis torneos.

El enlace es `/friends/invite#<code>`: el fragmento nunca llega al servidor ni a sus logs. La página guarda el código en `sessionStorage` y, sin sesión, muestra un texto genérico ("te invitaron a ser amigos"). Con sesión y botarga, pide `preview` y muestra el nombre del dueño con un botón "aceptar" (`POST`). Nunca acepta sola. `/friends/fights/{id}` es la repetición privada, con la pantalla de `/pvp/fights/{id}` y otra lectura. `/friends/tournaments/{id}` muestra el bracket, con los resultados ocultos hasta pulsar "ver resultados" y cada combate enlazado a su repetición privada.

## Fases
1. Server A: invitaciones, amistades, bloqueos, retos y lecturas, con los tests de seguridad.
2. Server B: torneos y su bracket.
3. Web A: `/friends`, la invitación, los retos y su repetición privada, con el e2e.
4. Web B: torneos y bracket, con el e2e.

## Riesgos
- **Grafo de amistades:** no sale en ninguna lectura pública. Las peleas entre amigos y los torneos son privados (404 para los ajenos, sin sesión incluidos), y sus ids no se cruzan con los de `pvp_fights`. Dentro de un torneo, cada participante ve a los demás: es inherente al torneo, y lo decide el dueño al armarlo.
- **Códigos:**
  - el formato se valida en el borde;
  - nunca van a `tracing`;
  - aceptar tiene cuota por usuario y por IP;
  - todos los rechazos dan el mismo 404, para que no sirvan de oráculo de baneos ni de bloqueos;
  - el código no viaja en `next` del login: la página lo guarda en `sessionStorage` y lo lee al volver.
- **Granjas:** ningún flujo de amigos toca rating, XP, monedas, `pvp_daily` ni sorteos. Lo prueban los tests comparando esas tablas antes y después.
- **Volumen:** cada pelea se guarda para siempre. Lo acotan `friend_daily` y las cuotas.
- **Interbloqueos:** el actor es la única fila bloqueada con espera. Lo prueban los tests de concurrencia.
- **Borrado de cuenta:** cuando exista un flujo para borrar una cuenta, tiene que incluir `friendships`, `friend_blocks`, `friend_invites`, `friend_fights`, `friend_tournaments` y sus slots.
- **Volumen por destinatario:** un amigo puede retarme hasta 50 veces al día, pero esas peleas son privadas y solo pesan en almacenamiento. No hay tope por pareja.

## Pruebas
- [ ] `just check` en verde.
- [ ] Integración: aceptar hace amigos una sola vez. Un código rotado, propio, inventado, de un baneado o del que me bloqueó da el mismo 404. Quitar bloquea que vuelva a aceptar. Dos aceptaciones concurrentes respetan el tope de 200.
- [ ] Integración: un reto guarda una `friend_fights` y deja idénticos `botargas`, `botarga_draws`, `botarga_unlocks`, `pvp_daily`, `pvp_fights` y `ledger_entries`. Repetir el `client_key` devuelve la misma pelea. Retar a quien no es amigo da 404. El reto 51 del día da 429. Diez retos no sacan de la carencia PvP.
- [ ] Integración: un torneo de 4 y uno de 8 tienen 3 y 7 combates, ganadores encadenados y un campeón. Un empate pasa por el bit de la semilla. Repetir el `client_key` devuelve el mismo torneo. El torneo 11 del día da 429. Un ajeno o un usuario sin sesión ve 404 en el torneo y en sus peleas.
- [ ] Concurrencia: dos torneos cruzados y dos retos cruzados terminan sin `40P01`.
- [ ] `banned_writes.rs` y `fights_no_leaks.rs` cubren las rutas nuevas, `preview` incluida. Los `GET` no escriben. `preview` responde el mismo 404 en los mismos casos que `accept`.
- [ ] e2e: dos usuarios se hacen amigos por el enlace, uno reta al otro y ve la repetición verificada, y arman un torneo de 4. A 375 px y escritorio, claro y oscuro.
