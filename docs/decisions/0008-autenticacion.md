---
id: "0008"
title: Autenticación y sesiones
type: adr
status: approved
risk: high
author: architect
reviewers: [security-reviewer, game-designer, owner]
approvals:
  - by: security-reviewer
    date: 2026-10-21
  - by: game-designer
    date: 2026-10-21
  - by: owner
    date: 2026-10-21
  - by: owner
    date: 2026-10-22
  - by: security-reviewer
    date: 2026-10-22
  - by: security-reviewer
    date: 2026-10-28
  - by: owner
    date: 2026-10-28
  - by: security-reviewer
    date: 2026-10-30
supersedes: null
linear: TA-5
depends_on: ["0002", "0003", "0005", "0006", "0007"]
created: 2026-10-20
updated: 2026-10-30
---

# ADR-0008: Autenticación y sesiones

## Contexto
- Necesitamos un login sin costo, cómodo en el celular, que proteja el saldo de `Coins` (ADR-0005), la integridad de las apuestas y la semilla (ADR-0002).
- Un solo dominio público; Traefik (Coolify) enruta `/` a web y `/api`, `/ws` al server (ADR-0006). El server Rust es la única fuente de verdad del usuario; SvelteKit solo presenta.
- Un solo proceso `server` en un VPS de 1 vCPU: el estado en memoria (rate limiter, registro de sockets) es aceptable.
- Las monedas son gratis de obtener (bono, rescate), así que el riesgo real es **crear monedas o inflar el leaderboard**, no robar dinero.
- El público es mayormente mexicano y móvil: muchas personas comparten IP detrás del CGNAT de los operadores, así que la IP sirve como señal, nunca como prueba.

## Decisión

### D1. Identidad y vinculación
- Solo OAuth, sin contraseñas: Google y Discord. Magic link queda fuera del MVP.
- La identidad es `(provider, provider_user_id)` (`sub` de Google, `id` de Discord), con `UNIQUE (provider, provider_user_id)` en `oauth_accounts`.
- **Scopes mínimos:** Google `openid` y nada más; Discord `identify`. **No se pide ni se guarda email.** El email nunca es llave ni criterio de vinculación (no existe en la BD).
- Datos guardados por cuenta OAuth: `provider`, `provider_user_id`, `created_at`. En `users`: `display_name`, `role`, `locale`, `theme`, `created_at`, `banned_at`.
- **`display_name` nunca se copia del proveedor** (ni el `name` de Google ni el `username`/`global_name` de Discord). Al crear el usuario, el server genera un alias al azar con `OsRng` a partir de listas de palabras en `content/` (p. ej. adjetivo + sustantivo + número), y el onboarding invita a cambiarlo. El alias es dato del usuario, no texto de la UI, así que no pasa por i18n (ADR-0007).
- **Validación de `display_name`** en el borde, al crearlo y al editarlo:
  - Se normaliza a NFC y se recortan los espacios de los extremos.
  - Largo de 3 a 24 caracteres (grafemas).
  - Se rechazan caracteres de control (`Cc`), de formato bidi (U+061C, U+200E–U+200F, U+202A–U+202E, U+2066–U+2069) y de ancho cero (U+200B–U+200D, U+2060, U+FEFF).
  - **Llave de comparación `display_name_key`** = skeleton de confusables de UTS #39 (p. ej. crate `unicode-security`) aplicado sobre NFKC + casefold. Así `Tоrneo_King` con `о` cirílica y `Torneo_King` con `o` latina dan la misma llave.
  - Lista de reservados (`admin`, `moderador`, `system`, la marca y sus variantes), comparada por `display_name_key`.
  - *Ver la "Enmienda 2026-10-29: caracteres invisibles y comparación de reservados" (security-reviewer firmó el 2026-10-29 (https://github.com/kike1303/torneo-absurdo/pull/54#issuecomment-6502944943); architect firmó el 2026-10-29 (https://github.com/kike1303/torneo-absurdo/pull/54#issuecomment-6504044430)): se rechazan todos los caracteres invisibles, y los reservados se comparan por dos formas construidas con las letras y los dígitos, que solo agregan rechazos a la comparación por `display_name_key`.*
  - **Único por `display_name_key`**, garantizado por la BD: columna `users.display_name_key` con `UNIQUE`. La aplicación no hace un "¿existe?" previo; si el `INSERT`/`UPDATE` viola el índice, responde `profile.display_name_taken`. Dos renombres concurrentes al mismo nombre: gana exactamente uno.
  - **Alias generado:** pasa por la misma validación. Si choca con un reservado o con el índice único, se genera otro, hasta 10 intentos; si se agotan, el login responde 503 `auth.try_again` y no crea el usuario.
  - Errores: `profile.display_name_invalid`, `profile.display_name_reserved`, `profile.display_name_taken`.
- **Nunca se vincula en automático.** Vincular un segundo proveedor solo desde una sesión iniciada (`POST /api/auth/link/{provider}`), con el flujo atado a esa sesión (D2). Si `(provider, id)` ya pertenece a otro `user`, se responde `auth.already_linked` y no se fusiona nada.
- Desvincular exige que quede al menos otro proveedor y revoca todas las demás sesiones del usuario.
  - *Ver la "Enmienda 2026-10-30: reautenticación para vincular, desvincular y admin" (security-reviewer firmó el 2026-10-30, ver "Revisión"): vincular exige una autenticación reciente y vigente, y desvincular P exige además que su identidad no sea más nueva que P. Reemplaza la frase anterior sobre desvincular.*
- Los access/refresh tokens de Google y Discord se usan una sola vez para leer la identidad y se descartan; nunca se persisten.

### D2. Flujo OAuth (Authorization Code + PKCE)
- Crate `oauth2`. PKCE `S256`; el `code_verifier` vive solo en el servidor.
- Al iniciar (`POST /api/auth/{provider}/start` desde un formulario para login, `POST /api/auth/link/{provider}` para vincular; ambos pasan por D4 y responden 303 al proveedor) el server crea una fila en `oauth_flows`: `id_hash`, `provider`, `purpose (login|link)`, `link_user_id` (solo en link), `state` (128 bits, CSPRNG), `code_verifier`, `next`, `expires_at = now + 10 min`.
  - *Ver la "Enmienda 2026-10-30: reautenticación para vincular, desvincular y admin" (security-reviewer firmó el 2026-10-30, ver "Revisión"): `purpose` también vale `reauth`, con `POST /api/auth/reauth/{provider}` y `link_user_id`, y la lista cerrada de `/login?error=` suma dos códigos.*
- El `id` del flujo (128 bits) va en la cookie pre-auth `__Host-oauth` (`HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=600`).
- En el callback: se busca el flujo por la cookie con `DELETE … RETURNING` (un solo uso); se rechaza con 400 `auth.invalid_state` si falta la cookie, falta `state`, no coincide, expiró o el `provider` no corresponde. Si `purpose = link`, además la sesión actual debe ser de `link_user_id`. La cookie `__Host-oauth` se expira siempre. La respuesta del callback lleva `Cache-Control: no-store`.
- La identidad se lee de los endpoints `userinfo` (Google `openidconnect.googleapis.com/v1/userinfo`, que con scope `openid` devuelve solo `sub`; Discord `/users/@me`) con el access token, por TLS. **No se usa el `id_token`**, por eso no hace falta `nonce` ni JWKS; si en el futuro se usa, se valida firma, `iss`, `aud`, `exp` y `nonce`.
- `redirect_uri` exacta y registrada en cada proveedor: `${PUBLIC_ORIGIN}/api/auth/{provider}/callback`.
- **`next`**: solo se acepta si cumple `^/(?![/\\])[^\x00-\x1f]*$`; si no, se usa `/`. Se guarda en `oauth_flows` y se redirige a ese valor, nunca al que venga en el callback.
- Los `start` y `callback` limpian filas expiradas de `oauth_flows` de forma oportunista.

### D3. Sesión
- Token de 256 bits (`OsRng`), en base64url. En `sessions` se guarda solo `SHA-256(token)` y se busca por ese hash.
- Cookie `__Host-session` (`HttpOnly; Secure; SameSite=Lax; Path=/`, sin `Domain`), con `Max-Age` igual a la expiración absoluta.
- Columnas: `token_hash`, `user_id`, `created_at`, `authenticated_at`, `last_seen_at`, `expires_at` (absoluta), `ip_hash`, `user_agent_family`.
- **Expiración por inactividad: 7 días. Expiración absoluta: 30 días.** `last_seen_at` se actualiza como máximo una vez cada 5 min.
- Cada login emite un token nuevo; si la petición traía una `__Host-session` previa, esa fila se borra. Vincular un proveedor también rota el token.
  - *Ver la "Enmienda 2026-10-30: reautenticación para vincular, desvincular y admin" (security-reviewer firmó el 2026-10-30, ver "Revisión"): la sesión también guarda `auth_account_id` y `auth_link_seq`, y la rotación al vincular conserva esos dos valores, `authenticated_at` y `expires_at`, así que vincular nunca cuenta como reautenticación ni alarga la expiración.*
- **Borrar una fila de `sessions`, por cualquier motivo, cierra sus sockets** (D6): logout, rotación en login o vinculación, ban, desvincular, cerrar todas, revocación por admin y la limpieza de sesiones expiradas. Todo borrado pasa por una sola función del server que borra la fila y cierra los sockets; ningún otro código hace `DELETE FROM sessions`.
- Logs: `tower-http::trace` redacta `Cookie`, `Set-Cookie`, `Authorization`, `X-Internal-Secret` y `X-Client-Ip`; las rutas `/api/auth/*/callback` se registran sin query string. Ni token, ni `code`, ni `state` van a logs.

### D4. CSRF y CORS
- Ninguna mutación acepta `GET`/`HEAD`/`OPTIONS`.
- Un middleware de axum, aplicado a todo el router, revisa toda petición no segura:
  - **Petición del navegador** (sin secreto interno válido): `Origin` debe ser exactamente `PUBLIC_ORIGIN`. Si `Origin` falta, se acepta solo con `Sec-Fetch-Site: same-origin`. Si no, 403 `auth.csrf_origin`.
  - **Petición interna de SvelteKit** (secreto válido, D5): se aplica la misma regla a los headers originales del navegador que reenvía SvelteKit, con una diferencia: `X-Client-Origin` es **obligatorio** y debe ser exactamente `PUBLIC_ORIGIN`; si viene `X-Client-Fetch-Site`, debe ser `same-origin`. Sin `X-Client-Origin` (p. ej. una mutación disparada desde un `load` en una navegación `GET`), 403 `auth.csrf_origin`. No hay excepción interna que omita el chequeo.
- SvelteKit mantiene `csrf.checkOrigin` activado.
- **Sin capa CORS.** Todo vive en el mismo origen: el server no monta `CorsLayer` y ninguna respuesta lleva `Access-Control-Allow-*`. Un preflight `OPTIONS` ajeno recibe la respuesta por defecto sin esos headers.

### D5. SSR (SvelteKit → server)
- SvelteKit llama al API por `INTERNAL_API_URL` (variable de entorno), no por Traefik. Su valor lo fija ADR-0006 D1: `http://torneo-server:8080`, un alias único en la red dedicada `torneo-edge` que resuelve a un solo contenedor. Nunca un nombre genérico como `server`, que en una red compartida podría resolver a un contenedor ajeno.
- `handleFetch` agrega `__Host-session`, `X-Internal-Secret`, `X-Client-Ip`, `X-Client-Origin` y `X-Client-Fetch-Site` **solo** si el origen de la URL es exactamente el de `INTERNAL_API_URL`. Cualquier otro `fetch` sale sin cookie ni headers internos.
  - `X-Internal-Secret`: 32 bytes, variable de entorno, nunca expuesto al cliente.
  - `X-Client-Origin` y `X-Client-Fetch-Site`: copia literal de `Origin` y `Sec-Fetch-Site` de la petición del navegador que está atendiendo SvelteKit; si no venían, no se envían.
  - `X-Client-Ip`: `getClientAddress()`.
- El server trata una petición como interna solo si el secreto coincide (comparación en tiempo constante). Solo entonces lee `X-Client-*`; sin secreto válido los ignora y aplica D4 y D7 como a cualquier petición del navegador.
- **Los `load` y los `GET` de SvelteKit (`+page`, `+layout`, `+server` con `GET`) nunca llaman a rutas de mutación del API.** Las mutaciones solo salen de form actions o de `fetch` del cliente. D4 lo hace cumplir en el server (una mutación interna sin `X-Client-Origin` da 403), y el code review lo vigila.
- adapter-node con `ADDRESS_HEADER=X-Forwarded-For` y `XFF_DEPTH=1`: toma la entrada que agregó el único Traefik, no las que mande el cliente.
- Toda respuesta del API que lea una sesión y toda página SSR con `locals.user` llevan `Cache-Control: private, no-store`.

### D6. WebSocket (`/ws`)
- En el handshake, `Origin` debe ser exactamente `PUBLIC_ORIGIN`; si falta o es ajeno, 403.
- La autenticación es por la cookie `__Host-session` en el upgrade; **nunca** por token en el query string. Sin sesión se permite el WS anónimo con solo canales públicos.
- En el MVP, el cliente solo manda mensajes de suscripción/desuscripción. **Ninguna acción que cambie estado viaja por WS**; apostar y reclamar van por `POST` HTTP.
- **Canales.** Los nombres de canal son un enum de `crates/protocol`, no texto libre:
  - Públicos: los canales de peleas y de programación (p. ej. `fight:{fight_id}`, `schedule`), iguales para todos y sujetos a ADR-0002 sin importar la auth.
  - Privado: un único canal `self`, **sin identificador del lado del cliente**. El server lo resuelve al `user_id` de la sesión del socket. Un socket anónimo no puede suscribirse a `self`.
  - Cualquier otro canal (no público, con id de usuario, desconocido o mal formado) se rechaza con `ws.forbidden_channel` y no abre la suscripción.
- El server mantiene en memoria `session_id → sockets`. Cuando se borra una fila de `sessions`, por cualquier motivo (D3, incluida la rotación del token en un login o una vinculación), sus sockets se cierran en el acto. Como defensa extra, cada socket autenticado revalida su sesión cada 5 min y se cierra si ya no es válida.
- **Límites por socket:** mensaje del cliente de 4 KB como máximo (si lo pasa, cierre `1009`); 10 mensajes por cada 10 s (si lo pasa, cierre `1008`); 8 suscripciones simultáneas (la siguiente se rechaza con `ws.too_many_subscriptions`).
- **Límites de conexión** (pensados para el CGNAT móvil en el main event):
  - Con sesión: 5 sockets simultáneos por usuario y, además, un techo alto de 200 sockets con sesión por IP (`/64` en IPv6), con la misma regla de dos buckets de D7. No cuentan contra el límite de anónimos.
  - Anónimos: 50 sockets simultáneos por IP (`/64` en IPv6).
  - Tope global `WS_MAX_CONNECTIONS` (por defecto 2000, a validar con una prueba de carga en el VPS de 1 vCPU). Al llegar al tope, el upgrade responde 503 `ws.capacity`.
  - **Reserva de capacidad:** el último 20 % de `WS_MAX_CONNECTIONS` (400 por defecto) solo admite sockets de usuarios **fuera de carencia** (D8). Pasado el 80 %, un upgrade anónimo o de una cuenta en carencia responde 503 `ws.capacity`. Así, llenar el tope con cuentas OAuth nuevas o con muchas `/64` no deja fuera a los usuarios con antigüedad.

### D7. IP del cliente y rate limiting
- **IP del cliente:** se toma de `X-Forwarded-For`, usando la entrada de más a la derecha que **no** pertenezca a `TRUSTED_PROXIES`. Si el peer no está en `TRUSTED_PROXIES`, se ignora `X-Forwarded-For` y se usa la IP del peer. Para peticiones internas con secreto válido, `X-Client-Ip` (D5).
- **`TRUSTED_PROXIES`**: la fuente de verdad es ADR-0006 D2. Hoy vale `10.250.0.2/32`, la IP fija de `coolify-proxy` en la red dedicada `torneo-edge`. Regla de este ADR: solo la IP (`/32` o `/128`) de un proxy real, **nunca** un CIDR que incluya contenedores que no son proxy: ni la red `coolify` (compartida con otras apps del VPS) ni la subred de `torneo-edge` (ahí también vive `web`; un RCE o SSRF en `web` podría elegir su llave de rate limit y su `ip_hash`).
- **Llave de IP (`ip_key`):** la dirección completa en IPv4 y el prefijo `/64` en IPv6. La usan los buckets de esta sección, los límites por IP de D6 y el `ip_hash` de D8.
- `tower-governor` (en memoria, un solo proceso). En las rutas autenticadas hay **dos buckets independientes**, uno por `user_id` y otro por IP. La petición pasa solo si pasa los dos. Cambiar de IP no le da un bucket nuevo al usuario.

| Ruta | Buckets | Límite (por bucket) |
|---|---|---|
| `POST /api/auth/*/start`, `/link`, `GET /callback` | IP | 10/min, ráfaga 5 |
| `POST /api/bets` | usuario · IP | usuario 30/min, ráfaga 10 · IP 120/min |
| `POST /api/bonus/claim`, `/api/bailout/claim` | usuario · IP | usuario 5/min · IP 60/min |
| Otras mutaciones | usuario · IP | usuario 60/min · IP 240/min |
| Upgrade `/ws` anónimo | IP | 30/min, ráfaga 10 |
| Upgrade `/ws` con sesión | usuario · IP | usuario 10/min · IP 120/min |

- El bucket por IP es más alto que el por usuario para no bloquear a usuarios legítimos detrás del mismo CGNAT; el control fino es el bucket por usuario.
- El rate limiter es defensa adicional. **Los invariantes de la economía los garantiza la BD** (D8).

### D8. Economía: controles del MVP

**Parámetros.** D8 se escribe en función de:
- `B`: bono diario (`DailyGrant`).
- `R`: rescate (`Bailout`). Por defecto `R = 0.5 B`, y siempre `R ≤ B`, para que el rescate nunca convenga más que el bono.
- `m`: apuesta mínima. El umbral de rescate es `m`.
- `r`: rake (ADR-0005).

Los valores absolutos de `B`, `m` y `r` los fija la spec de apuestas. Todos son configuración que valida game-designer contra ADR-0005.

**Día y programación.**
- "Día" = día calendario UTC. Es fijo, sin horario de verano y fácil de probar; la UI muestra cuándo se reinicia en la hora local.
- `grant_day` se calcula en la BD (`(now() AT TIME ZONE 'UTC')::date`), nunca con datos del cliente.
- Las temporadas empiezan y terminan a las 00:00 UTC. El main event empieza después del reinicio y no cruza las 00:00 UTC (p. ej. 20:00 CDMX = 02:00 UTC), para que todos lleguen con el bono del día.

**Bono y rescate.**
- **Bono diario**: índice único parcial `UNIQUE (user_id, grant_day) WHERE kind = 'grant'` en `ledger_entries`, con `INSERT … ON CONFLICT DO NOTHING`. Si no se insertó nada, 409 `bonus.already_claimed`.
- **Rescate** de `R` monedas. Se concede solo si se cumplen, en la misma transacción:
  1. El usuario ya tiene el asiento `grant` del día UTC actual. Si no, 409 `bailout.grant_available` y la UI ofrece el bono.
  2. `saldo libre + stakes en apuestas abiertas < m`. Si no, 409 `bailout.not_eligible`.
  3. No hay otro `bailout` ese día: `UNIQUE (user_id, grant_day) WHERE kind = 'bailout'`. Si no, 409 `bailout.already_claimed`.
- Toda operación que lea o reduzca saldo (apostar, rescate) toma `SELECT … FOR UPDATE` sobre la fila del usuario dentro de la misma transacción, para serializarse por usuario.

**Carencia (`Probation`).**
- Una cuenta está en carencia mientras tenga menos de 7 días desde `users.created_at` **o** menos de 10 apuestas liquidadas. Al salir, no vuelve a entrar.
- "Apuesta liquidada" = apuesta **ganada o perdida**. Un reembolso (pelea cancelada, empate, pozo reembolsado) **no cuenta**, para que nadie madure una alt con pozos cancelados.
- Efectos:
  - Su tope por pelea es `1 B`.
  - No aparece en el leaderboard.
  - Sus stakes perdidos no dan ganancia clasificable a nadie.
- Lo demás no cambia: bono, rescate, pagos en monedas y ledger son los de cualquier usuario.
- **La ganancia clasificable de la carencia se acumula:** la cuenta suma su `RankedProfit` desde la primera apuesta y aparece en el leaderboard con ese acumulado al salir de la carencia. La UI muestra el progreso con la llave `ranking.probation_progress` (`days_left`, `bets_left`).
- **Temporada del acumulado.** Cada liquidación suma en el `season_stats` de la temporada de la pelea (ver "Temporada de una pelea" en "Tope por pelea"), no en la del momento de salir de la carencia. Si la carencia cruza un cambio de temporada, lo acumulado en la temporada que terminó se queda en ella: la cuenta no sale en su clasificación final, porque al cerrarla seguía en carencia, y ese acumulado no se arrastra. En la temporada nueva acumula desde cero, como todos (el ranking se reinicia por temporada, glosario `Season`).

**Tope por pelea.**
- La suma de los stakes de un usuario en una misma pelea (todas sus apuestas) no puede pasar de `5 B`, o de `1 B` si está en carencia.
- Se comprueba dentro de la transacción de la apuesta, después del `FOR UPDATE` del usuario, sumando sus `bet_stake` en esa pelea más el nuevo. Si se pasa, 409 `bet.fight_cap_exceeded`.
- El múltiplo es configurable por tipo de pelea (`QuickFight`, `MainEvent`). Por defecto es 5× en ambos, y la spec de apuestas puede subirlo para el main event. El tope de carencia es 1× siempre.
- **Temporada de una pelea:** la temporada `[starts_at, ends_at)` que contiene el cierre de su ventana de apuestas. Su liquidación va a esa temporada aunque ocurra después de `ends_at`. Ninguna apuesta que cuente para una temporada se acepta después de su `ends_at`.

**Ganancia clasificable (`RankedProfit`).**
- El leaderboard de temporada ordena por `RankedProfit`. Es un refinamiento de `NetProfit` (ADR-0005): tampoco incluye bonos ni rescates, y además no acredita ganancias que salgan de apuestas hechas en carencia. Las pérdidas cuentan completas.
- Al crear una apuesta se guarda `bets.ranked_stake = true` si el usuario está **fuera** de carencia en ese momento. Es una foto: no cambia si la cuenta madura antes de la liquidación.
- Al liquidar, en la misma transacción idempotente, se actualiza `season_stats`. Sea `d = payout − stake` de la apuesta, `L` la suma de stakes del lado perdedor y `L_c` la parte de `L` con `ranked_stake = true`:
  - Apuesta perdedora: `RankedProfit −= stake`.
  - Apuesta ganadora: `RankedProfit += min(d, ⌊d · L_c / L⌋)`; si `L = 0`, `RankedProfit += d`. En la práctica: si `d ≤ 0` (el rake dejó al ganador con pérdida neta), cuenta `d` completo; si `d > 0`, se escala por `L_c / L`.
  - Reembolso: 0. **Una pelea cancelada y un empate del motor se liquidan como reembolso** (asiento `refund`, ADR-0005); la spec de apuestas lo implementa así.
- **Por qué `min` y no "rake solo del lado perdedor".** ADR-0005 y el glosario (`Rake`) definen el rake como "% del pozo". Cobrarlo solo del lado perdedor y obligar a reembolsar los pozos de un solo lado se apartaría de ese texto, y esa definición no le toca a este ADR. Con `min`, la regla vale para cualquier base de rake: las pérdidas cuentan enteras, las ganancias se escalan, y `RankedProfit ≤ NetProfit` en cada apuesta, como dice el glosario. Un pozo de un solo lado que no se reembolse resta el rake del ganador en vez de borrarlo.
- **Aritmética entera y determinista.** `Coins` son enteros. El escalado se calcula en enteros de 128 bits como `(d · L_c) div L`, con división entera hacia abajo (solo se aplica con `d > 0`, así que no hay negativos que redondear). Reliquidar la misma pelea da el mismo resultado y no vuelve a sumar (transacción idempotente de ADR-0005).
- El ledger, los pagos en monedas y `NetProfit` no cambian. El usuario nuevo legítimo no pierde ninguna moneda.

**Riesgo residual (granjas de cuentas maduras).**
- Una alt en carencia aporta 0 al ranking. Una alt **madura** (7 días y 10 apuestas liquidadas, que puede hacer con `m`) inyecta hasta `B + R = 1.5 B` al día.
- El tope por pelea limita lo que la principal **apuesta**, no lo que **gana**: su parte del lado perdedor es `S / (S + L_X)`, donde `S` es su apuesta y `L_X` lo que apostaron otros usuarios en su mismo lado. En arranque en frío o en horas valle, `L_X ≈ 0` y se lleva casi todo.
- Cuando la principal pierde, las alts conservan las monedas y vuelven a apostar, así que a lo largo del día la principal absorbe toda la inyección con varianza baja. Con `N` alts maduras: `RankedProfit` de la principal ≈ `(1 − r) · N · (B + R)` al día.
- Ilustrativo, con `B = 100`, `R = 50`, `r = 5 %`:
  - 10 alts maduras: ≈ 1,425/día (≈ 14 B); ≈ 43k en una temporada de 30 días.
  - 50 alts maduras: ≈ 7,125/día (≈ 71 B); ≈ 214k por temporada.
  - Un jugador honesto excelente (20 peleas/día al tope de `5 B`, con 10 % de ventaja) hace ≈ 10 B/día.
- **Los números del MVP no cierran este riesgo.** La carencia lo encarece (cada alt cuesta 7 días y 10 apuestas, y queda visible en `auth_events`) y la revisión manual lo detecta. **El owner lo acepta de forma explícita** (ver Pendiente).

**Señales anti-granjas y revisión.**
- `ip_hash = HMAC-SHA256(ip_key, IP_HASH_KEY)`, sobre la misma `ip_key` de D7 (la dirección IPv4 completa o el `/64` de IPv6, para que las direcciones temporales de privacidad de IPv6 no rompan la correlación), en `sessions` y en `auth_events` (login, link, bono, rescate y apuesta, con `bet_id`). La IP en claro nunca se persiste ni se registra en logs del server. Es **dato personal** y se declara en el aviso de privacidad.
- **Retención de `ip_hash`**: `max(30 días, duración de la temporada + 7 días)`, para que la revisión de cierre cubra toda la temporada. Un job diario borra lo que pasa de ese plazo.
- **`farm_report`**: subcomando de la CLI, `server season farm-report <season_id> --by <admin_user_id>`, ejecutado en el contenedor como `server admin grant` (D9). No hay endpoint HTTP. `--by` debe ser un usuario con `role = admin`. Lee los datos sin modificarlos, imprime el reporte y escribe **una sola** fila en `admin_audit` con `action = 'season_review'`, `target = season_id`, `admin_id = --by` y un resumen del reporte en `payload` (conteos y usuarios marcados). Es la **única** forma de crear una fila `season_review`. Falla sin escribir nada si `now < season.ends_at` o si queda alguna pelea de la temporada sin liquidar ni reembolsar. Lista:
  1. Las peleas de la temporada donde dos o más cuentas con el mismo `ip_hash` apostaron en lados opuestos.
  2. Para cada usuario del top 20: la **concentración de contrapartes**, es decir, qué porcentaje de su `RankedProfit` salió de los stakes de sus 5 contrapartes principales. Se atribuye a cada perdedor `j` en proporción a `stake_j / L_c`. Se marca si pasa del 50 % (configurable) o si comparte `ip_hash` con alguna de esas contrapartes.
- **Cierre de temporada**: `server season close <season_id>` falla salvo que exista en `admin_audit` una fila `season_review` de esa temporada con `at ≥ season.ends_at` **y** `at ≥ now − 48 h`. Así la revisión siempre ve la temporada completa, incluidos sus últimos días, y no está vieja al cerrar. En esa revisión, el admin lee el reporte de `farm-report` y revisa el top 20. La retención de `ip_hash` (temporada + 7 días) cubre esa ventana.
- **Un `ip_hash` compartido no basta para sancionar.** En México, el CGNAT móvil pone a cientos de usuarios legítimos detrás de la misma IP. Una sanción exige, además, apuestas en lados opuestos y una concentración de contrapartes sobre el umbral, o una evidencia equivalente. La decide el admin y queda en `admin_audit` con su motivo.
- Sanciones posibles: excluir de la clasificación de la temporada (`season_stats.excluded_at` y motivo) y/o banear (D10). Ningún saldo se edita; un ajuste es un asiento `admin_adjustment` (D9).

### D9. Rol `admin`
- `users.role ∈ {user, admin}`. Se asigna **solo** con el subcomando `server admin grant|revoke <user_id>` ejecutado en el contenedor. Ningún endpoint escribe `role`, y ningún atributo de OAuth lo influye.
- `server admin grant` y `revoke` revocan todas las sesiones vigentes del usuario, para que la regla de 12 h aplique desde la primera sesión de admin.
- Sesiones de admin: expiración absoluta de 12 h. Toda acción de admin exige `authenticated_at` de hace menos de 60 min; si no, 403 `auth.reauth_required`.
  - *Ver la "Enmienda 2026-10-30: reautenticación para vincular, desvincular y admin" (security-reviewer firmó el 2026-10-30, ver "Revisión"): la autenticación, además de tener menos de 60 min, tiene que ser vigente y con una identidad vinculada antes del `server admin grant` vigente (`users.admin_account_cutoff`). Reemplaza el requisito de la frase anterior.*
- Las acciones de admin pasan por el mismo middleware de `Origin` (D4) y se registran en `admin_audit` (`admin_id`, `action`, `target`, `payload`, `at`), solo de inserción.
- **Ningún rol** puede leer `seed` antes de `Settled` (ADR-0002) ni editar saldos. Un ajuste es un asiento `admin_adjustment` del ledger con motivo obligatorio y su fila en `admin_audit` (ADR-0005).

### D10. Logout, revocación y ban
- `POST /api/auth/logout`: borra la fila de `sessions`, expira `__Host-session` y cierra los sockets de esa sesión.
- `POST /api/auth/logout-all`: borra todas las sesiones del usuario y cierra todos sus sockets.
- Banear (`users.banned_at`), desvincular un proveedor o una acción de admin por abuso revocan todas las sesiones del usuario. Un usuario baneado no puede iniciar sesión (`auth.banned`).

## Alternativas descartadas
- **Better Auth en SvelteKit**: habría dos dueños del usuario; rompe "una sola fuente de verdad".
- **Auth0, Clerk, Supabase Auth**: cuestan al escalar y agregan una dependencia externa.
- **JWT**: revocar es difícil y no aporta nada con un solo backend.
- **Vincular por email verificado**: más cómodo, pero un error de un proveedor en `verified` regala cuentas. Sin email no hay nada que vincular por error.
- **`id_token` de Google con validación OIDC**: necesita JWKS y `nonce`; `userinfo` por TLS da la misma identidad con menos superficie.
- **Inicializar `display_name` con el nombre del proveedor**: publica el nombre real en el ranking sin que el usuario lo decida, y obliga a pedir `profile`.
- **Omitir el chequeo de `Origin` en peticiones internas**: `checkOrigin` de SvelteKit no cubre los `load` ni los `GET`, así que una navegación desde otro sitio podría disparar una mutación con el secreto.
- **Canal privado `user:{id}` con chequeo de dueño**: funciona, pero un olvido en el chequeo es un IDOR; con `self` no hay id que manipular.
- **Día en `America/Mexico_City`**: más natural para el público mexicano, pero ata la lógica a una zona; UTC es más simple de razonar y probar.
- **Acciones por WS**: duplicaría la autorización y el CSRF; HTTP ya tiene ambos.
- **Reducir el bono durante la carencia**: castiga al usuario nuevo legítimo y no frena la granja; la ganancia clasificable ataca el problema sin quitarle monedas a nadie.
- **Excluir en automático del ranking los pagos de cuentas con el mismo `ip_hash`**: con el CGNAT móvil castigaría a usuarios legítimos de la misma red; el `ip_hash` queda como señal para la revisión manual.

## Consecuencias
- Más fácil: no hay contraseñas, emails, nombres reales ni tokens de terceros que proteger; revocar es borrar filas.
- Más difícil:
  - Un usuario que pierde acceso a su único proveedor pierde la cuenta (no hay recuperación por email). Se mitiga invitando a vincular un segundo proveedor.
  - El leaderboard ya no es una suma directa del ledger: `RankedProfit` vive en `season_stats` y se calcula en la liquidación.
  - Cerrar una temporada requiere una revisión manual del admin.
- Vigilar:
  - El rate limiter y el registro de sockets en memoria dejan de servir si algún día hay más de un proceso `server`; eso requeriría un ADR nuevo.
  - **Riesgo residual aceptado:** una granja de `N` alts maduras puede mover ≈ `(1 − r) · N · (B + R)` al día hacia el `RankedProfit` de una cuenta principal (D8). Si la revisión de cierre lo encuentra con frecuencia, se abre un ADR con controles más fuertes.
  - Los límites por IP de D6 y D7 frente al CGNAT en el main event: medirlos y ajustarlos. La prueba de carga del VPS incluye el caso de agotar `WS_MAX_CONNECTIONS` con muchas cuentas nuevas o muchas `/64`, y comprueba que la reserva de D6 deja entrar a cuentas fuera de carencia.

**Números fijados:** `state`/flujo 128 bits, TTL 10 min, un solo uso · token de sesión 256 bits · inactividad 7 días · absoluta 30 días (admin 12 h, reauth 60 min) · `last_seen_at` cada 5 min · revalidación WS cada 5 min · WS: mensaje ≤ 4 KB, 10 mensajes/10 s, 8 suscripciones por socket, 5 sockets por usuario, 200 con sesión por IP, 50 anónimos por IP, `WS_MAX_CONNECTIONS` 2000 con el último 20 % reservado a cuentas fuera de carencia · tabla de límites de D7 · IPv6 por `/64` · `display_name` de 3 a 24, único por skeleton UTS #39, hasta 10 intentos de alias · día UTC · carencia 7 días / 10 apuestas liquidadas (los reembolsos no cuentan) · tope por pelea `1 B` en carencia y `5 B` después (configurable por tipo de pelea) · umbral de rescate = `m` · `R = 0.5 B` · 1 rescate por día UTC, solo con el bono del día reclamado · retención de `ip_hash` `max(30 días, temporada + 7 días)` · `RankedProfit` en enteros con división hacia abajo · `season_review` con `at ≥ ends_at` y de las últimas 48 h al cerrar · umbral de concentración de contrapartes 50 %.

**Tests mínimos (bloquean el merge de la feature de auth):**
1. Ningún `GET`/`HEAD` muta: un test recorre el manifiesto de rutas y falla si una ruta de mutación se registra con método seguro. Única excepción declarada: `GET /callback` (exigido por OAuth), que solo consume un flujo atado a la cookie pre-auth.
2. `Origin` ausente (sin `Sec-Fetch-Site: same-origin`) o ajeno da 403 en `POST` HTTP y en el handshake de `/ws`.
3. Una petición con `X-Internal-Secret` incorrecto se trata como externa: sin `Origin` da 403, y sus `X-Client-Ip`/`X-Client-Origin` se ignoran.
4. Callback sin `state`, con `state` reusado, expirado o de otra cookie → 400; un flujo `link` de otra sesión no vincula.
5. `next` externo (`https://…`, `//…`, `/\…`) redirige a `/`.
6. `(provider, id)` ya vinculado a otro usuario → `auth.already_linked`, sin fusión.
7. Bono: 20 `POST /bonus/claim` concurrentes del mismo usuario → exactamente 1 asiento `grant`. Igual para el rescate.
8. Rescate con una apuesta abierta que deja saldo libre en 0 → rechazado.
9. Ningún endpoint escribe `users.role` (test que intenta cada ruta de perfil/admin con `role` en el body, más un chequeo en CI de que el único SQL que escribe `role` vive en el módulo de la CLI). `server admin grant` revoca las sesiones vigentes.
10. Logout: tras `POST /logout`, el mismo token da 401 y su WS se cierra; `logout-all` invalida todas. Rotación: un login nuevo en un navegador con la sesión de A (y una vinculación) borra la fila anterior y cierra en el acto el socket de A, sin esperar la revalidación de 5 min. Un chequeo en CI verifica que el único `DELETE FROM sessions` vive en la función de D3.
11. Sesión con `last_seen_at` de hace más de 7 días o `expires_at` vencido → 401.
12. Ningún endpoint ni mensaje WS expone `seed` antes de `Settled` con sesión de admin (extiende el test de ADR-0002).
13. Los logs de una corrida de login completa no contienen el token, el `code` ni el `state` (test de captura del subscriber de tracing).
14. `X-Forwarded-For` falsificado por el cliente no cambia la llave del rate limit, ni en el API ni en el `X-Client-Ip` que SvelteKit calcula para una página SSR.
15. Mutación interna con secreto válido y sin `X-Client-Origin`, o con `X-Client-Origin` ajeno → 403. Con `X-Client-Origin = PUBLIC_ORIGIN` y `X-Client-Fetch-Site` igual a `cross-site`, `same-site` o `none` → 403. Con `X-Client-Origin = PUBLIC_ORIGIN` y `X-Client-Fetch-Site` ausente o `same-origin` → pasa.
16. `handleFetch` (test unitario en web): un `fetch` a una URL que no es `INTERNAL_API_URL` sale sin cookie, sin `X-Internal-Secret` y sin `X-Client-*`.
17. IDOR por WS: un socket del usuario A no recibe eventos del usuario B con ningún mensaje de suscripción (`self`, `user:{id_B}`, nombres desconocidos o mal formados); los no públicos responden `ws.forbidden_channel`. Un socket anónimo no puede suscribirse a `self`.
18. Límites del WS: un mensaje de más de 4 KB cierra el socket; la suscripción número 9 se rechaza. El socket con sesión número 201 desde la misma `/64` → 503. Con el 80 % de `WS_MAX_CONNECTIONS` ocupado, un upgrade anónimo o de una cuenta en carencia → 503 `ws.capacity` y uno de una cuenta fuera de carencia → pasa.
19. Primer login con Google: la URL de autorización pide solo `openid`, y el `display_name` creado no coincide con ningún campo devuelto por el proveedor. Igual con Discord.
20. Validación de `display_name`: tabla de casos (largo, control, bidi, ancho cero, reservados, duplicado con otra capitalización, homoglifo con `о` cirílica frente a un nombre con `o` latina, reservado escrito con homoglifos) → código de error correcto. Concurrencia: dos renombres simultáneos al mismo nombre → exactamente uno pasa y el otro da `profile.display_name_taken`. Alias: con el generador forzado a chocar, se reintenta y crea un alias libre; si choca 10 veces, 503 `auth.try_again` sin crear el usuario.
21. Ninguna respuesta lleva `Access-Control-Allow-Origin`, incluido un preflight `OPTIONS` con `Origin` ajeno.
22. Rate limit: el mismo usuario desde dos IPs comparte su bucket de usuario; dos direcciones IPv6 del mismo `/64` comparten bucket de IP.
23. Tope por pelea: dos `POST /api/bets` concurrentes del mismo usuario en la misma pelea, cada uno bajo `5 B` pero que juntos lo superan → se acepta exactamente uno.
24. Una cuenta en carencia no pasa de `1 B` por pelea, ni con varias apuestas. Una cuenta con 7 días, 9 apuestas ganadas o perdidas y varios reembolsos sigue en carencia.
25. `RankedProfit` en la liquidación (en todos los casos, el ledger y el `bet_payout` son los mismos que sin la regla, y reliquidar no cambia nada):
    - (a) Lado perdedor solo con `ranked_stake = false` → el ganador con `d > 0` suma 0.
    - (b) Pozo mixto con `L_c = L / 2` → el ganador suma `⌊d / 2⌋`; con `d` impar se redondea hacia abajo.
    - (c) Foto: una cuenta que sale de la carencia entre su apuesta y la liquidación sigue con `ranked_stake = false` y no acredita a nadie.
    - (d) Ganador con pérdida neta por el rake (p. ej. 500 en A contra 10 de una cuenta en carencia en B): su `RankedProfit` baja exactamente lo mismo que su `NetProfit`.
    - (e) Pozo de un solo lado (`L = 0`) que no se reembolsa → el ganador suma `d`.
    - (f) Empate del motor y pelea cancelada → reembolso; `RankedProfit` no cambia y no cuenta como apuesta liquidada.
    - (g) Propiedad: en ningún caso el `RankedProfit` de una apuesta es mayor que su `NetProfit`.
26. Rescate sin el bono del día reclamado → 409 `bailout.grant_available`.
27. El `RankedProfit` acumulado durante la carencia aparece completo en el leaderboard al salir de ella. Una pelea cuya ventana cierra antes de `ends_at` y se liquida después suma en esa temporada. Si la carencia cruza el cambio de temporada, la cuenta no sale en la clasificación final de la temporada vieja y en la nueva acumula desde cero.
28. Cierre de temporada:
    - `server season close` falla sin fila `season_review`, con una `season_review` anterior a `ends_at` y con una de hace más de 48 h. Con una de `at ≥ ends_at` y de las últimas 48 h, pasa.
    - `server season farm-report` falla sin escribir nada antes de `ends_at`, con peleas de la temporada pendientes o con un `--by` que no es admin.
    - No hay otra forma de insertar una fila `season_review` (chequeo en CI de que el único SQL que la escribe vive en el subcomando).
    - Dos direcciones IPv6 del mismo `/64` producen el mismo `ip_hash`.

## Decisiones del owner (2026-10-21)
- **ADR aceptado** (`risk: high`).
- **Riesgo residual de D8 aceptado.** Una granja de `N` cuentas maduras puede mover ≈ `(1 − r) · N · (B + R)` al día hacia el `RankedProfit` de una principal. En el MVP lo frenan la carencia, el tope por pelea y la revisión obligatoria con `farm-report` al cerrar cada temporada. Si las señales lo piden, se agregan revisiones intermedias con el mismo subcomando.
- **Apps OAuth registradas** en Google Cloud Console y en el Discord Developer Portal. Falta confirmar que sus scopes sean exactamente `openid` e `identify` y que las `redirect_uri` coincidan con D2 cuando exista el dominio.
- **Trazabilidad en ADR-0005**: el owner autorizó agregar la nota en sus Consecuencias. Ya está.

## Pendiente
- **Aviso de privacidad** (LFPDPPP): borrador en [`docs/legal/aviso-de-privacidad.md`](../legal/aviso-de-privacidad.md). Requiere revisión de security-reviewer y, antes de publicarse, una revisión legal y los datos del responsable.
- **Logs de acceso de Traefik**: resuelto en ADR-0006 D11; se aplica en el VPS durante la feature 001.

## Enmienda 2026-10-22: SvelteKit 3
El owner aprobó el 2026-10-22 usar **SvelteKit 3** desde la feature 001. Kit 3 eliminó la opción `csrf.checkOrigin` que cita D4 ("SvelteKit mantiene `csrf.checkOrigin` activado"). Esa línea queda reemplazada por lo siguiente. El resto de D4 y todo D5 no cambian.

- **Chequeo de Kit.** En Kit 3, el chequeo de `Origin` de los formularios (`POST`/`PUT`/`PATCH`/`DELETE` con `Content-Type` de formulario o sin él) está siempre activo, y solo se apaga con `csrf.trustedOrigins: ['*']`. `csrf.trustedOrigins` queda vacío y nunca contiene `'*'`.
- **Origen de confianza.** Kit 3 compara contra `paths.origin`, que es de build, o contra el origen de la petición. adapter-node 6 ya no lee la variable `ORIGIN`. `web` no define `paths.origin` (la imagen es la misma para cualquier host), así que el origen de la petición es `https://` + `Host`, sin `PROTOCOL_HEADER`, `HOST_HEADER` ni `PORT_HEADER`.
- **Guard de `Origin` propio en `web`.** Como el chequeo de Kit no cubre JSON ni otros tipos y compara contra el `Host`, `web` aplica en `handle` la misma regla que el middleware de axum para peticiones del navegador: en todo método distinto de `GET`, `HEAD` y `OPTIONS`, `Origin` debe ser exactamente `PUBLIC_ORIGIN` (variable de runtime de `web`, con el mismo valor que la del server). Si `Origin` falta, se acepta solo con `Sec-Fetch-Site: same-origin`. Si no, 403 `auth.csrf_origin` sin ejecutar la ruta. El requisito verificable es RF-A23 de la spec 001.
- **D5 no cambia.** `X-Client-Origin` y `X-Client-Fetch-Site` siguen siendo copia literal de los headers del navegador, y el middleware de axum sigue siendo la defensa de las mutaciones del API. `ADDRESS_HEADER=X-Forwarded-For` y `XFF_DEPTH=1` siguen existiendo en adapter-node 6 con el mismo comportamiento (lo verifiqué en su código).

## Nota (2026-10-28): reparto de tests por feature
Ninguna decisión de D1 a D10 cambia, y ningún número fijado tampoco. Esta nota solo precisa **qué feature tiene que pasar cada test mínimo antes de mergear**. Donde "Tests mínimos" dice "bloquean el merge de la feature de auth", ahora vale este reparto. Cada test sigue bloqueando el merge de la feature que tiene el código que prueba, y está como criterio bloqueante en el issue de Linear de esa feature. El owner aceptó el reparto el 2026-10-28 desde la revisión del intent de la feature 008. El owner confirmó la nota en chat el 2026-10-28.

| Feature | Tests |
|---|---|
| 008, autenticación (TA-26) | 1–6, 9–11, 13–17, 19–22. De 18: mensaje de más de 4 KB, suscripción número 9 y socket con sesión número 201 desde la misma `/64`. De 28: "dos direcciones IPv6 del mismo `/64` producen el mismo `ip_hash`". |
| Apuestas (TA-27) | 7, 8, 23, 24, 26. De 18: la reserva de capacidad de D6 (con el 80 % ocupado, un anónimo o una cuenta en carencia recibe 503 y una cuenta fuera de carencia pasa). Criterio bloqueante adicional: `bets.ranked_stake` se registra al crear cada apuesta (D8), con un test que lo verifica para una cuenta en carencia y otra fuera de ella. |
| Peleas en vivo y commit-reveal (TA-13) | 12. |
| Temporadas (TA-28) | De 27: la parte de temporada (la pelea que cierra antes de `ends_at` y se liquida después, y la carencia que cruza el cambio de temporada). 28, salvo el caso del `/64`. El término `temporada + 7 días` de la retención de `ip_hash` (D8), que se aplica antes de abrir la primera temporada. |

- **Tests 25 y la parte de carencia de 27** (el `RankedProfit` acumulado durante la carencia aparece completo al salir de ella) bloquean a **la primera feature que escriba `season_stats`**, sea TA-27 o TA-28. Sin `ranked_stake` desde la primera apuesta, las apuestas anteriores a TA-28 no se podrían clasificar después, y una granja en carencia contaría para el primer ranking.
- **Reserva de capacidad de D6:** se implementa con la feature de apuestas. Sin apuestas liquidadas, toda cuenta sigue en carencia, y la reserva solo bajaría el tope útil al 80 % de `WS_MAX_CONNECTIONS`. Hasta entonces el tope es `WS_MAX_CONNECTIONS` para todos.
- **Retención de `ip_hash`:** hasta la feature de temporadas se aplica el piso de 30 días de D8.

## Enmienda 2026-10-28: errores del callback OAuth
Aprobada por el owner el 2026-10-28 (transmitida por la sesión coordinadora). Revisada y aprobada por security-reviewer el 2026-10-28. Solo reemplaza **cómo responde** `GET /api/auth/{provider}/callback` cuando falla. Las condiciones de rechazo de D2 y el resto de D1 a D10 no cambian.

- **Por qué.** El callback es una navegación top-level que vuelve del proveedor. Un 400 con cuerpo JSON deja al usuario del celular ante un texto crudo, sin salida. La web no puede intervenir, porque `/api` va directo al server.
- **Respuesta.** En vez de 400 `auth.invalid_state` (y de los otros errores del callback), el server responde **303** con `Location: /login?error=<code>`. Vale para los flujos `login` y `link` por igual: con sesión, `/login` muestra el error y enlaza a la cuenta. Además:
  - no crea ni rota ninguna sesión;
  - expira siempre la cookie del flujo y consume la fila de `oauth_flows` si existía;
  - lleva `Cache-Control: no-store`;
  - nunca usa `next`.
- **Lista cerrada de códigos.** `error` solo puede valer `auth.invalid_state`, `auth.banned`, `auth.provider_error`, `auth.try_again`, `auth.already_linked` o `auth.provider_already_linked`. Es un enum de `crates/protocol`, y el server nunca copia al `Location` nada que venga de la petición ni del proveedor.
  - *Ver la "Enmienda 2026-10-30: reautenticación para vincular, desvincular y admin" (security-reviewer firmó el 2026-10-30, ver "Revisión"): la lista suma `auth.reauth_required` y `auth.reauth_mismatch`, y son ocho códigos.*
- **En la web.** `/login` resuelve `error` a una llave i18n solo si está en la lista. Un valor desconocido se muestra con un texto genérico, y **nunca se refleja** en la página, en un atributo ni en un log.
- **T4 adaptado.** Un callback sin `state`, con `state` reusado, expirado o de otra cookie responde 303 a `/login?error=auth.invalid_state`, sin `Set-Cookie` de sesión y con la cookie del flujo expirada. Un flujo `link` de otra sesión no vincula. T4 suma un caso nuevo: con un `error` ajeno a la lista en la query de `/login`, la página no contiene ese valor.

## Enmienda 2026-10-29: caracteres invisibles y comparación de reservados
**Firmas:** security-reviewer firmó el 2026-10-29 (https://github.com/kike1303/torneo-absurdo/pull/54#issuecomment-6502944943). architect firmó el 2026-10-29 (https://github.com/kike1303/torneo-absurdo/pull/54#issuecomment-6504044430). security-reviewer revalidó el texto con el invariante de dos formas el 2026-10-29 (https://github.com/kike1303/torneo-absurdo/pull/54#issuecomment-6504090300).

Viene del hallazgo de security-reviewer en esa revisión: con la letra de D1, un nombre podía ser invisible o hacerse pasar por la marca. Cambia dos viñetas de la validación de `display_name` de D1; el resto no cambia.

- **Caracteres rechazados.** Además de `Cc`, se rechazan los separadores de línea y de párrafo (U+2028, U+2029), todo `Default_Ignorable_Code_Point` de Unicode (la tabla completa de `DerivedCoreProperties.txt`, que incluye los de formato bidi y de ancho cero que nombraba D1, además de U+00AD, U+034F, U+115F–U+1160, U+17B4–U+17B5, U+180B–U+180F, U+2060–U+206F, U+3164, U+FE00–U+FE0F, U+FFA0, U+FFF0–U+FFF8, U+1BCA0–U+1BCA3, U+1D173–U+1D17A y U+E0000–U+E0FFF) y otros blancos que no están en esa propiedad: U+2800 (el blanco del braille), U+13441 y U+13442 (los blancos de los jeroglíficos egipcios) y U+16FE4 (el relleno del khitan). Ninguno se ve, así que permitían un nombre en blanco o uno que se lee como otro.
- **Comparación con los reservados: dos formas.** Sea `letras(x)` = solo las letras y los dígitos de `x`, sin acentos (NFD y `char::is_alphanumeric`). Un nombre es reservado si cualquiera de sus dos formas coincide con cualquiera de las dos formas de algún reservado (es más estricto que comparar forma con forma):
  1. `display_name_key(letras(nombre))`: ningún signo, espacio o acento entre las letras esquiva la lista (`Torneo Absurdo`, `Torneo·Absurdo`, `Torneo—Absurdo`, `Anónimo`).
  2. `letras(display_name_key(nombre))`, llaveado otra vez: un símbolo que el skeleton lee como letra sigue contando como esa letra (`Syst℮m`, `Sta₽f`, `S₞stem`). La forma 1 sola lo descartaba.
- **Invariante.** Esta regla solo agrega rechazos respecto de comparar `display_name_key` tal cual. Si un nombre tiene la misma `display_name_key` que un reservado, también tiene su forma 2. Lo prueba un test de monotonía sobre todas las variantes de cada reservado que la regla original rechazaba.
- La llave del `UNIQUE` no cambia: "Ana Bel" y "AnaBel" siguen siendo dos nombres.

## Enmienda 2026-10-30: reautenticación para vincular, desvincular y admin
**Firmas:** escrita por architect, como autor de este ADR, desde la spec de la feature 009 (`docs/features/009-reautenticacion-desvincular/spec.md`, TA-29), y viaja en el mismo PR. Necesita la firma de security-reviewer. security-reviewer firmó el 2026-10-30 (ADR-0008, "Revisión", y la spec de 009, "Revisión"). El gate del owner de `docs/process.md` ("aceptar o reemplazar un ADR") lo cubre su delegación del 2026-10-29, citada textual en el intent de 009 ("Decisiones → Delegación"): "todo lo que falte" la extiende a 009 y a esta enmienda mientras no haya nada desplegado. No cubre un tag `v*` ni un deploy, ni aceptar un riesgo residual, que sigue siendo del owner (ver "Riesgo residual" abajo).

**Estado:** en vigor. security-reviewer firmó el 2026-10-30 y la aplica el PR de 009. D1, D2, D3 y D9 llevan una nota que apunta aquí, como la Enmienda 2026-10-29.

Viene del hallazgo de security-reviewer en la revisión de 008 (TA-29). Con la letra de D1, D3 y D9, una sesión robada podía vincular un proveedor X (la rotación salía con `authenticated_at = now()`), desvincular el proveedor de la víctima y dejarla fuera para siempre, y con una sesión de admin, pasar el chequeo de 60 min sin volver a probar quién es. Cambia D1 (vincular y desvincular), D2 (un `purpose` nuevo y la lista cerrada de la Enmienda 2026-10-28), D3 (qué guarda la sesión y qué conserva la rotación) y D9 (qué cuenta como reautenticación de admin). El resto no cambia, incluidos los 60 min de admin, las 12 h y los 30 días.

- **Orden de vínculo (D1).** `oauth_accounts` gana `id`, de una secuencia. "Vinculada antes que P" es `id < P.id`. Volver a vincular una identidad crea una fila nueva con un `id` mayor. El orden no usa relojes.
- **Qué guarda la sesión (D3).** Además de `authenticated_at`, la sesión guarda la fila exacta de `oauth_accounts` con la que se autenticó (`auth_account_id`, `ON DELETE SET NULL`) y el mayor `id` de las cuentas del usuario en ese momento (`auth_link_seq`). Una autenticación es **vigente** si su identidad sigue vinculada al mismo usuario y no se vinculó nada después (`auth_link_seq ≥ max(id)`). Es **reciente** si `authenticated_at` tiene menos de **10 min**. Una sesión sin esos datos, por ejemplo una anterior a esta enmienda, no tiene autenticación vigente.
- **Vincular (D1).** Vincular exige una autenticación reciente y vigente al empezar (`POST /api/auth/link/{provider}`). En la transacción del callback, con el usuario bloqueado, exige una autenticación vigente de menos de 20 min (los 10 min más la vida del flujo). Si falta, 303 a `/login?error=auth.reauth_required`, sin vincular nada.
- **Rotación al vincular (D3).** La sesión nueva **conserva** `authenticated_at`, `expires_at`, `auth_account_id` y `auth_link_seq` de la rotada, y el `Max-Age` de su cookie es lo que le queda. Vincular nunca cuenta como reautenticación ni alarga la expiración. Como la fila nueva tiene un `id` mayor que `auth_link_seq`, la sesión rotada pierde su autenticación vigente en el acto.
- **Desvincular (D1).** Reemplaza "Desvincular exige que quede al menos otro proveedor y revoca todas las demás sesiones del usuario": quitar P exige que quede otro proveedor habilitado y una autenticación **reciente y vigente** con una identidad **no más nueva que P** (`auth_account_id ≤ P.id`: P misma o una vinculada antes). Si falta, 403 `auth.reauth_required`. Todo se evalúa dentro de la transacción de `unlink`, con el usuario bloqueado y la fila de la sesión leída otra vez con `FOR UPDATE`: si la cerraron en medio, 401 y nada cambia. Sigue revocando las demás sesiones. Un `unlink` exitoso **consume la confirmación** de la sesión que lo hizo (`auth_account_id = NULL`, un `UPDATE`, no un borrado): en una misma ventana de confirmación se quita un proveedor o se vincula uno, nunca las dos cosas.
- **Reautenticarse (D2).** `POST /api/auth/reauth/{provider}` empieza un flujo con `purpose = 'reauth'`, atado al usuario de la sesión (`link_user_id`) y con la cuota por IP de `start` y `link` (D7). En el callback, la identidad tiene que ser una cuenta de ese usuario: si no existe o es de otro, 303 a `/login?error=auth.reauth_mismatch`, sin crear un usuario ni una cuenta, sin cambiar de usuario y con la sesión intacta. Si es suya, rota la sesión a una con autenticación nueva (`now()`, esa identidad, el `max(id)` actual) y la expiración absoluta de un login, y registra `auth_events` con `kind = 'reauth'`. `next` sigue D2.
- **Lista cerrada de `/login?error=` (Enmienda 2026-10-28).** Suma `auth.reauth_required` y `auth.reauth_mismatch`: son ocho códigos. Las demás reglas de esa enmienda no cambian.
- **Admin (D9).** Reemplaza "Toda acción de admin exige `authenticated_at` de hace menos de 60 min": exige una autenticación **vigente** de menos de 60 min, con una identidad vinculada **antes del `server admin grant` vigente**. `grant` guarda en `users.admin_account_cutoff` el mayor `id` de las cuentas del usuario, y `revoke` lo borra. Es una columna privilegiada, como `role`: solo la escribe la CLI. Una identidad vinculada después del `grant` nunca da admin hasta otro `grant`. `grant` imprime por stdout (nunca en un log) los nombres de los proveedores que quedan dentro del corte, sin ids, para que el admin detecte uno que no reconoce. Si falta algo, 403 `auth.reauth_required`, como hoy.
- **Números fijados que se suman:** autenticación reciente para vincular y desvincular, 10 min · callback de vincular, 20 min. Los demás no cambian.
- **Tests mínimos que se suman** (bloquean el merge de 009, en el reparto de la "Nota (2026-10-28)"):
  29. Con solo la sesión de la víctima (vieja o reciente), vincular X y desvincular el proveedor de la víctima falla enseguida, tras logout y login con X, tras `logout-all` y tras esperar sin importar cuánto. Con una sesión vieja, vincular ya falla.
  30. La víctima se reautentica con su proveedor y quita X: las sesiones del atacante se revocan, sus sockets se cierran y un login con X no entra a la cuenta.
  31. Admin: en las mismas variantes de 29, una sesión autenticada con X nunca pasa `RecentAdmin`. Una reautenticación con una identidad anterior al `grant` sí pasa.
  32. Carrera: si en medio de un `unlink` se cierra la sesión, se rota o se desvincula la identidad de su autenticación, esa petición no borra nada, no registra nada y no revoca nada.
  33. Un flujo `reauth` con una identidad que no es del usuario responde `auth.reauth_mismatch`, no crea nada y deja la sesión como estaba.
  34. Tras un `unlink` exitoso, la misma sesión no puede vincular ni volver a desvincular sin reautenticarse.
- **Riesgo residual (pendiente del owner, no lo crea esta enmienda).** Un atacante que vinculó X sigue entrando con X, con acceso de usuario y nunca de admin, hasta que la víctima lo vea en `/account` y lo quite. Con esta enmienda, vincular X exige una sesión de los 10 min posteriores a un login real. Con esa sola sesión hace una de dos cosas: quitar una identidad de la víctima (nunca la última) o vincular X, porque un `unlink` exitoso consume la confirmación. Si además consigue otra sesión reciente de la víctima (un segundo robo), puede quitar el proveedor de la víctima: es el mismo riesgo de convivencia, y lo cierra lo mismo. Aparte, el proveedor más antiguo es la raíz de la cuenta: quien pierde el control de él no lo puede quitar, y quien lo controla puede quitar los demás. Antes de esta enmienda era peor, y no lo resuelve ninguna regla. Esta feature tampoco cierra el navegador desatendido con el proveedor V abierto: quien lo use consigue, sin segundo robo, la toma de cuenta completa de TA-29 (reautenticarse con V, vincular X, reautenticarse con V y quitar V), y la víctima queda fuera para siempre. "Nunca de admin" vale con solo X: las excepciones están en el Área 12 de la spec de 009. Lo detalla la spec de 009 ("Áreas de preocupación", 2 a 4, 12 y 13).

## Revisión
### security-reviewer — 2026-10-20 — REQUEST_CHANGES
**🤖 security-reviewer**

- Verifiqué: el ADR completo contra ADR-0002 (la semilla nunca sale antes de `Settled`), ADR-0005 (ledger inmutable, bono y rescate, leaderboard por `NetProfit`), ADR-0006 y `architecture.md` (un solo dominio, Traefik enruta `/api` y `/ws` al server), el glosario y el esquema de frontmatter de `docs/process.md`. La dirección general es correcta: solo OAuth, sesión opaca con hash en la BD, cookie `HttpOnly; Secure; SameSite=Lax`, un solo dueño del usuario y nada de JWT. El ADR no contradice ningún ADR `approved`. Pero en su forma actual deja sin decidir varios puntos que permiten secuestrar cuentas o crear monedas, y la mayoría de sus requisitos no se pueden verificar ("detección básica a futuro", "verificación de `Origin` en las mutaciones" sin definir qué pasa si falta). Todos los hallazgos de abajo son **Important** y bloquean.

- Cambios pedidos:

  1. **Vinculación de cuentas Google ↔ Discord por email (toma de cuenta).** *Escenario:* la víctima entra con Google (`victima@gmail.com`). El atacante crea una cuenta de Discord con ese email sin verificarlo (Discord devuelve `verified: false`) y entra con Discord. Si el servidor junta las cuentas por email, el atacante cae en el `user` de la víctima y se queda con su saldo, sus apuestas y su sesión. *Cambio:* la identidad es `(provider, provider_user_id)`, es decir `sub` de Google e `id` de Discord, con `UNIQUE` en `oauth_accounts`. **Nunca** se vinculan cuentas en automático por email. Solo se vincula de forma explícita desde una sesión ya iniciada (pantalla "vincular Discord"), con un `state` atado a esa sesión. Si se guarda el email, solo cuando `email_verified`/`verified` sea `true`, y nunca como llave.

  2. **`state`, PKCE y `nonce` sin especificar (login CSRF y CSRF de vinculación).** *Escenario A:* el atacante empieza un flujo de vinculación con su propio Discord, se queda con la URL de callback (`code` + `state`) y se la hace abrir a la víctima ya logueada. Si el `state` no está atado al navegador o a la sesión, el Discord del atacante queda vinculado a la cuenta de la víctima y el atacante entra cuando quiera. *Escenario B:* el atacante loguea a la víctima en una cuenta del atacante (login CSRF). *Cambio:* hay que dejar escrito: `state` de ≥128 bits, de un solo uso y con TTL de ~10 min, guardado del lado del servidor y atado a una cookie pre-auth `__Host-oauth` (`HttpOnly; Secure; SameSite=Lax`). PKCE con `S256` y el `code_verifier` solo en el servidor. Para Google (OIDC), `nonce` y validación de `iss`, `aud`, `exp` y la firma si se usa el `id_token`; si no, usar `userinfo` con el access token. `redirect_uri` exacta y registrada. Rechazar un callback que llegue sin `state` válido.

  3. **Cookie de sesión: prefijo, fijación y expiración absoluta.** *Escenario:* otra app desplegada en un subdominio del mismo dominio registrable (es normal con Coolify) pone una cookie `session` con `Domain=` del dominio padre. Eso permite fijar la sesión, o sombrear la cookie legítima. Y con una expiración solo deslizante, un token robado vive para siempre si se usa seguido. *Cambio:* usar el nombre `__Host-session` (obliga `Secure`, `Path=/` y que no lleve `Domain`). Emitir un token nuevo en cada login e ignorar cualquier token previo. Definir los números: expiración por inactividad (p. ej. 7 días) y **expiración absoluta** (p. ej. 30 días). El hash es SHA-256 del token y se busca por hash. Ni el token, ni la cookie, ni el `code` ni el `state` van a los logs: hay que filtrar los headers `Cookie`/`Set-Cookie`/`Authorization` en `tower-http::trace`.

  4. **CSRF: `SameSite=Lax` + `Origin` está incompleto.** *Escenario A:* `Lax` manda la cookie en navegaciones top-level por `GET`. Si cualquier endpoint que cambia estado acepta `GET` (p. ej. `GET /api/bonus/claim` o un logout por GET), basta con un `<a>` o un `window.location` desde otro sitio. *Escenario B:* "same-site" no es "same-origin": un subdominio hermano comprometido es same-site y la cookie `Lax` viaja también en sus `POST`. *Cambio:* toda mutación es no-`GET`/`HEAD`, y así se verifica en un test. En toda petición no segura, `Origin` debe ser exactamente el origen público configurado. Si `Origin` falta, se rechaza (o se acepta solo con `Sec-Fetch-Site: same-origin`). El chequeo se hace en un middleware de axum, no endpoint por endpoint. Mantener `csrf.checkOrigin` de SvelteKit activado para las form actions.

  5. **Reenvío de la cookie en el SSR (SvelteKit → Rust): fuga de token y de datos por caché.** *Escenario A:* un `handleFetch` que reenvía la cookie a cualquier URL la filtra al primer `fetch` hacia un tercero (o a una URL controlable por el usuario). *Escenario B:* el SSR de una página con saldo o datos de usuario sale sin `Cache-Control: private` y algún proxy o caché la sirve a otro usuario. *Escenario C:* las form actions de SvelteKit llaman al API desde el servidor sin `Origin` del navegador, así que el chequeo del punto 4 en Rust las rechaza, o se "arregla" desactivándolo. *Cambio:* reenviar solo la cookie de sesión y solo a la URL interna del API (`http://server:PORT` en la red de Docker, fija por variable de entorno). Las respuestas autenticadas van con `Cache-Control: private, no-store`. Definir cómo confía el API en SvelteKit: que reenvíe el `Origin` original, ya validado por `checkOrigin`, o que use un secreto interno que nunca sale al cliente. Que el API no confíe en `X-Forwarded-*` salvo cuando vienen de Traefik o de la red interna.

  6. **Auth del WebSocket (`/ws`) no está cubierta.** *Escenario A (CSWSH):* el upgrade a WS no pasa por CORS. Una página en un subdominio hermano (same-site, así que manda la cookie `Lax`) abre `wss://.../ws` con la sesión de la víctima y lee su canal privado, o manda mensajes autenticados si el WS acepta acciones. *Escenario B:* el usuario cierra sesión o el admin lo banea, pero su WS abierto sigue vivo durante horas. *Cambio:* validar `Origin` en el handshake contra el origen público. Autenticar con la cookie en el upgrade, **nunca** con token en el query string (termina en los logs de Traefik). Si se revoca la sesión, cerrar el socket, o al menos revalidarlo cada N minutos. Si algún mensaje WS cambia estado (p. ej. apostar), autorizarlo por mensaje, y que pase por el mismo endpoint transaccional que el HTTP. Los canales WS públicos de peleas cumplen ADR-0002 sin importar la auth.

  7. **Rate limiting detrás de Traefik, y bono/rescate sin garantía transaccional.** *Escenario A:* `tower-governor` usa por defecto la IP del peer, que siempre es Traefik. O todos comparten un solo bucket (cualquiera bloquea el login de todos), o si se toma `X-Forwarded-For` tal cual, el atacante lo falsifica y tiene límites infinitos. *Escenario B:* 20 `POST /bonus/claim` en paralelo pasan todos el chequeo "¿ya reclamó hoy?" antes de que alguno haga commit, y se acreditan 20 bonos. *Cambio:* tomar la IP del cliente solo desde el `X-Forwarded-For` que escribe Traefik, usando la entrada de más a la derecha que no sea de confianza. En endpoints autenticados, el límite va por `user_id` además de por IP. El "un bono por cuenta y día" lo garantiza la BD, no el rate limiter: `UNIQUE (user_id, grant_day)` en el asiento `grant` del ledger (o un `INSERT ... ON CONFLICT DO NOTHING`). Hay que definir la zona horaria del "día", UTC o `America/Mexico_City`, fija. El rate limiter es defensa adicional, no el control.

  8. **Abuso del bono diario y del rescate con multi-cuentas.** *Escenario A (inflar el leaderboard):* el atacante crea N cuentas de Google/Discord gratis. Cada una cobra bono y rescate. En cada pelea, su cuenta principal apuesta al lado A y las alts al lado B. Como el pozo es parimutuel, cuando gana A la principal se queda con las monedas de las alts como `bet_payout`, y eso cuenta como `NetProfit` (ADR-0005 solo excluye bonos y rescates *propios*). Así la cuenta principal escala el ranking con monedas creadas de la nada. *Escenario B (rescate con apuesta abierta):* el usuario apuesta todo su saldo, queda en 0 con la apuesta abierta, cobra el rescate, y luego la apuesta gana. *Cambio:* no se puede dejar todo "a futuro". El MVP necesita al menos: (a) el rescate exige saldo ≈ 0 **contando las apuestas abiertas**, y tiene límite de frecuencia (p. ej. 1 cada 24 h) con constraint en la BD; (b) las cuentas nuevas tienen un periodo de carencia antes de cobrar el bono completo o de aparecer en el leaderboard (p. ej. 7 días y N apuestas liquidadas); (c) un tope por apuesta relativo al pozo o al saldo; (d) señales mínimas para detectar granjas (p. ej. un hash de IP con retención corta, nunca la IP en claro en los logs), documentadas como dato personal. Pedirle a game-designer que valide los números contra ADR-0005.

  9. **Rol `admin` sin modelo de protección.** *Escenario A:* si `admin` se asigna por email (p. ej. "el email del owner es admin"), el punto 1 hace que cualquiera con un Discord de email no verificado sea admin. *Escenario B:* un admin con la sesión robada puede ver la semilla de una pelea abierta o editar saldos. *Cambio:* el rol se asigna solo por CLI o migración contra la BD, nunca por un endpoint ni por un atributo de OAuth. No hay endpoint para auto-promoverse, y los tests lo verifican. Las acciones de admin pasan por el mismo middleware de `Origin`, exigen una sesión reciente (p. ej. auth en la última hora, o expiración corta para sesiones admin) y quedan en un log de auditoría (quién, qué y cuándo). **Ningún rol**, admin incluido, puede leer `seed` antes de `Settled` (ADR-0002) ni editar saldos: los ajustes son asientos del ledger con su tipo propio y auditados (ADR-0005).

  10. **Logout y revocación sin definir.** *Escenario:* el usuario pierde el celular, o cierra sesión en una compu pública, pero el logout solo borra la cookie del navegador. Quien copió el token sigue entrando. *Cambio:* el logout es `POST` con chequeo de `Origin`, borra la fila de `sessions` y expira la cookie. Agregar "cerrar todas mis sesiones". Al banear, desvincular un proveedor o detectar abuso, revocar todas las sesiones del usuario y cerrar sus WS (punto 6). No guardar access ni refresh tokens de Google/Discord: no se necesitan después del login, y guardarlos solo agrega riesgo. Guardar el mínimo de datos personales.

  11. **Redirección abierta después del login.** *Escenario:* `/api/auth/google?next=https://evil.example/login-falso`. Después de un login legítimo, la víctima termina en un phishing con apariencia de "sesión expirada". *Cambio:* `next` acepta solo rutas relativas que empiecen con `/` y no con `//` ni `/\`, guardado junto al `state` en el servidor (no reflejado del callback).

- Para volver a pedir revisión, el ADR debe incluir: las reglas de los puntos 1–11 escritas como decisiones verificables, una sección de **Consecuencias** con los tests mínimos (ningún `GET` muta; `Origin` ausente o ajeno da 403 en HTTP y WS; constraints de bono y rescate bajo concurrencia; ningún endpoint asigna `admin`; el logout invalida el token en la BD) y los números concretos (TTLs, límites de tasa, zona horaria del día).

### architect — 2026-10-20 — RESPUESTA
- Forma: frontmatter completo (`approvals: []`, `supersedes`, `linear: null`, `depends_on`), secciones estándar, `status: in-review`. Agregué a game-designer como revisor por los números de D8.
1. **Vinculación por email** → D1. Identidad `(provider, provider_user_id)` con `UNIQUE`. Fui más lejos que lo pedido: **no se pide ni se guarda email** (scopes `openid profile` / `identify`), así que no hay vinculación por email posible. Vincular solo desde sesión iniciada, con flujo atado a la sesión; si ya pertenece a otro usuario, `auth.already_linked`.
2. **`state`, PKCE, `nonce`** → D2. `state` de 128 bits, un solo uso (`DELETE … RETURNING`), TTL 10 min, en servidor y atado a `__Host-oauth`; PKCE `S256`; `redirect_uri` exacta; callback sin `state` válido → 400. Elegí `userinfo` en vez de `id_token`, así que no hay `nonce` ni JWKS (queda la regla de qué validar si algún día se usa). Además, el inicio de login es `POST` con `Origin`, lo que cierra el login CSRF desde el arranque.
3. **Cookie de sesión** → D3. `__Host-session`, token nuevo en cada login (y al vincular), la sesión previa se borra, 7 días de inactividad y 30 de absoluta, SHA-256 y búsqueda por hash, redacción de headers y de la query del callback en los logs.
4. **CSRF** → D4. Ninguna mutación por métodos seguros; middleware global con `Origin == PUBLIC_ORIGIN`; sin `Origin`, solo con `Sec-Fetch-Site: same-origin`; `checkOrigin` activado. Excepción única y declarada: `GET /callback` (test 1).
5. **SSR** → D5. Cookie reenviada solo a `INTERNAL_API_URL`; confianza por `X-Internal-Secret` (elegí el secreto en lugar de reenviar `Origin`, porque un `Origin` reenviado es falsificable por cualquiera que llegue a la red interna); `X-Client-Ip` solo se acepta con el secreto; `Cache-Control: private, no-store`.
6. **WebSocket** → D6. `Origin` en el handshake, auth por cookie, nunca por query; cierre inmediato al revocar más revalidación cada 5 min. Decidí que **ninguna acción que cambie estado viaje por WS** en el MVP, así la autorización por mensaje no hace falta.
7. **Rate limiting y concurrencia** → D7 y D8. IP real desde la entrada de más a la derecha de `X-Forwarded-For` fuera de `TRUSTED_PROXIES`; límites por `user_id` + IP con números en la tabla; bono y rescate garantizados por índices únicos parciales y `ON CONFLICT DO NOTHING`; `FOR UPDATE` por usuario; día = UTC (razón en Alternativas).
8. **Multi-cuentas** → D8. (a) rescate cuenta las apuestas abiertas, 1 por día UTC con constraint; (b) carencia de 7 días y 10 apuestas liquidadas para el leaderboard, con tope reducido; (c) tope de 5× el bono por pelea; (d) `ip_hash` HMAC con retención de 30 días, declarado como dato personal. La carencia no reduce el bono: con el tope por apuesta, el bono de una alt nueva no alcanza para inflar a la principal, y así la primera semana no castiga a usuarios legítimos. game-designer valida los números.
9. **Admin** → D9. Solo por CLI; ningún endpoint ni atributo OAuth; absoluta de 12 h y reauth de 60 min; `Origin`; `admin_audit`; nadie lee `seed` antes de `Settled` ni edita saldos (`admin_adjustment` en el ledger).
10. **Logout y revocación** → D10. `POST` con `Origin`, borra la fila, expira la cookie y cierra los sockets; `logout-all`; revocación total al banear o desvincular; los tokens del proveedor no se persisten (D1).
11. **Redirección abierta** → D2. `next` con regex de ruta relativa (sin `//`, `/\` ni caracteres de control), guardado en `oauth_flows`, con `/` como respaldo; test 5.
- **Tests y números**: en Consecuencias (14 tests mínimos y la línea de números fijados). Lo que depende de servicios externos, del aviso de privacidad y de los logs de Traefik quedó en "Pendiente (owner)".

### security-reviewer — 2026-10-21 — REQUEST_CHANGES
**🤖 security-reviewer**

- Verifiqué: el cuerpo del ADR reescrito (D1–D10, Alternativas, Consecuencias con 14 tests y la línea de números fijados), no solo la respuesta del architect. Lo comparé con ADR-0002, 0003, 0005 y 0007 (`approved`), con ADR-0006 (en corrección), `docs/architecture.md`, el glosario y el esquema de frontmatter de `docs/process.md`.
  - **Frontmatter**: es válido. `linear: TA-5` es correcto, aunque la respuesta del architect dice `linear: null`; manda el cuerpo. `depends_on` y `reviewers` también están bien.
  - **Glosario**: el ADR usa `Coins`, `NetProfit`, `Ledger`, `Settled`, `grant`/`bailout`, y los usa bien.
  - **ADRs aprobados**: no contradice ninguno.
    - D9 y el test 12 refuerzan ADR-0002.
    - D8 y `admin_adjustment` respetan ADR-0005: es un asiento nuevo del ledger, no un cambio a la regla de no usar dinero real.
    - Los errores son códigos (`auth.*`, `bonus.*`), como pide ADR-0007.
  - **Mis puntos del 2026-10-20**, uno por uno:
    1. Vinculación por email: **cerrado** (D1 y test 6). No guardar email es mejor que lo que pedí.
    2. `state`, PKCE y `nonce`: **cerrado** (D2 y test 4). Acepto usar `userinfo` en vez de `id_token`. El access token lo obtiene el server en el intercambio con el proveedor, server a server, con PKCE y `client_secret`. Ese token nunca pasa por el navegador, así que no se puede inyectar un token ajeno y el `nonce` no protege nada extra. Iniciar el login por `POST` con `Origin` cierra el login CSRF desde el arranque, y `__Host-oauth` con `SameSite=Lax` sí viaja en el redirect top-level del proveedor.
    3. Cookie de sesión: **cerrado** (D3 y tests 10, 11, 13).
    4. CSRF: **cerrado** para peticiones del navegador (D4 y tests 1–2). La excepción de `GET /callback` es aceptable: solo consume un flujo atado a `__Host-oauth`, es de un solo uso y tiene rate limit. **Queda abierta** la excepción interna (ver punto 1).
    5. SSR: **parcial** (punto 1).
    6. WebSocket: **cerrado** en `Origin`, cookie, revocación y "nada que cambie estado por WS", que me parece la decisión correcta. Falta autorizar las suscripciones (punto 2).
    7. Rate limiting y bono/rescate bajo concurrencia: **cerrado** (D7, D8 y tests 7 y 14). El día UTC es aceptable y se puede probar.
    8. Multi-cuentas: (a) el rescate contando las apuestas abiertas está **cerrado** (test 8). (b) Sacar del leaderboard a las cuentas en carencia era una de las dos opciones que propuse, así que lo acepto, pero el riesgo de fondo sigue (punto 4). (c) y (d) están, con huecos de redacción y de test (punto 4).
    9. Admin: **cerrado** (D9 y tests 9 y 12).
    10. Logout y revocación: **cerrado** (D10 y test 10).
    11. `next`: **cerrado** (D2 y test 5). Revisé la regex contra `//`, `/\`, `\t`, `\n` y `https:`.
  - **ADR-0006**: no bloqueo por ADR-0006. Los `code`/`state` que quedan en los logs de Traefik no sirven a un atacante: el `code` necesita el `code_verifier` y el `state` necesita la cookie pre-auth. Aun así, falta cerrar la retención de IPs en claro.
  - **Hallazgos nuevos**: la reescritura deja cuatro huecos que bloquean. Los puntos 1–4 son **Important**; los 5–9 son **Minor** y no bloquean.

- Cambios pedidos:

  1. **[Important] D5: el bypass de `Origin` para peticiones internas se apoya en una suposición falsa.** D5 dice que el server omite el chequeo de `Origin` porque "ya lo hizo `checkOrigin` en las form actions". Pero `checkOrigin` de SvelteKit solo revisa `POST`/`PUT`/`PATCH`/`DELETE` con content-type de formulario. **No revisa** los `load`, que corren en navegaciones `GET`, ni los `+server.ts` con `GET`. Y el `X-Internal-Secret` lo agrega SvelteKit a **toda** llamada interna, sin importar qué petición del navegador la originó.
     - *Escenario A:* un `load` de SvelteKit, por ejemplo el de `/[lang]/bonus`, `/logout` o una página que "reclama al entrar", llama a `POST /api/bonus/claim` o `POST /api/auth/logout` con la cookie y el secreto. El atacante pone `<a href="https://torneo/es/logout">` o `window.location` en su sitio. Es una navegación top-level `GET`, así que `SameSite=Lax` manda la cookie, SvelteKit hace la mutación interna con el secreto y el middleware de D4 la deja pasar. El test 1 no lo detecta, porque solo recorre las rutas del API.
     - *Escenario B:* `getClientAddress()` en adapter-node devuelve la IP de Traefik si no se configura `ADDRESS_HEADER`. Si se configura con un `XFF_DEPTH` incorrecto, el cliente la puede falsificar. Como D5 manda esa IP en `X-Client-Ip` y D7 la usa como llave, todo el SSR cae en un solo bucket (cualquiera bloquea a todos) o el atacante elige su llave y envenena el `ip_hash` de D8.
     - *Cambio:*
       - (a) Con secreto válido, el server **no omite** el chequeo: SvelteKit reenvía el `Origin` y el `Sec-Fetch-Site` originales del navegador en `X-Client-Origin` / `X-Client-Fetch-Site`, y el mismo middleware de D4 los valida. Su objeción ("un `Origin` reenviado es falsificable") no aplica, porque esos headers solo se aceptan junto con el secreto. Si no hay `Origin` original, como en un `load` por `GET`, toda mutación interna se rechaza con 403.
       - (b) Regla escrita: los `load` y los `GET` de SvelteKit nunca llaman a rutas de mutación del API.
       - (c) `handleFetch` agrega `X-Internal-Secret` **solo** cuando el origen de la URL es exactamente `INTERNAL_API_URL`, igual que la cookie.
       - (d) Fijar `ADDRESS_HEADER=X-Forwarded-For` y `XFF_DEPTH=1`, el valor que corresponde a un solo Traefik.
       - Tests: una mutación interna sin `X-Client-Origin` válido da 403, y `X-Forwarded-For` falsificado contra una página SSR no cambia `X-Client-Ip` (extiende el test 14).

  2. **[Important] D6: las suscripciones a canales privados no están autorizadas.** D6 dice que el cliente manda mensajes de suscripción y que un anónimo solo ve canales públicos, pero no dice cómo se autoriza un canal privado de un usuario con sesión.
     - *Escenario:* un usuario con sesión manda `{"subscribe": "user:4821"}` (o el equivalente que defina el protocolo) y recibe en vivo el saldo, las apuestas abiertas y los avisos de otro usuario. Es un IDOR por WS.
     - *Cambio:*
       - El canal privado no lleva identificador del lado del cliente. Hay un único canal `self`, y el server lo resuelve al `user_id` de la sesión del socket.
       - Cualquier otro nombre de canal que no sea público se rechaza con `ws.forbidden_channel` y no abre la suscripción.
       - Test: un socket del usuario A no puede recibir eventos del usuario B con ningún mensaje de suscripción.

  3. **[Important] D1: el `display_name` copiado de Google publica el nombre real.** Con el scope `profile`, Google devuelve el `name` de la persona (nombre y apellidos reales). D1 inicializa `display_name` con ese valor, y ese campo se muestra en el leaderboard y en cualquier vista pública. Eso contradice "datos personales mínimos", y el scope `profile` no hace falta para la identidad, que es solo el `sub`.
     - *Escenario:* alguien entra con su Google del trabajo o de la escuela y, sin haberlo decidido, aparece como "María Pérez López" en el ranking público de apuestas.
     - *Cambio:*
       - Google con scope `openid` y nada más.
       - El `display_name` inicial es un alias generado al azar (por ejemplo, de `content/`) o lo elige el usuario en el onboarding. Nunca se copia en automático del proveedor, tampoco de Discord.
       - Validar `display_name` en el borde: largo de 3 a 24, NFC, sin caracteres de control, sin caracteres bidi ni de ancho cero, y lista de reservados (`admin`, la marca).
       - Test: después del primer login con Google, el `display_name` no coincide con ningún campo del proveedor.

  4. **[Important] D8: el control anti-granjas no limita la transferencia hacia la cuenta principal, y el tope es ambiguo.**
     - *Escenario A:* el atacante tiene N alts con más de 7 días y 10 apuestas cada una. Un alt solo necesita 10 apuestas mínimas, sin ganar nada. En cada pelea con poca gente, la principal apuesta 1 moneda al lado A y los alts apuestan su bono al lado B. Cuando gana A, la principal cobra todo el pozo de los alts como `bet_payout`, y eso cuenta como `NetProfit`. Cuando gana B, pierde 1 moneda. La esperanza para la principal es de unos N·B/2 por pelea, con monedas creadas por los bonos de los alts.
       - El tope de 5× sobre la apuesta de la principal no limita lo que **cobra**.
       - El tope de 1× de los alts no reduce el flujo diario, porque un alt solo recibe B al día de todos modos.
       - La carencia solo saca del ranking a los alts, que no la necesitan.
       - Lo dicho en la respuesta ("el bono de una alt nueva no alcanza para inflar a la principal") no se cumple una vez que el alt sale de la carencia.
     - *Escenario B:* D8 dice "tope **por apuesta** 1× el bono" para las cuentas nuevas, pero "por usuario y pelea (suma)" para el tope general. Si se implementa literal, una cuenta nueva hace varias apuestas de 1× en la misma pelea.
     - *Cambio:*
       - (a) Escribir en Consecuencias el **riesgo residual aceptado**: una granja de N cuentas puede mover del orden de N·B al día hacia el `NetProfit` de una principal. Que el owner lo acepte de forma explícita en su OK.
       - (b) Hacer verificable la detección: una consulta de admin definida que liste las peleas donde cuentas con el mismo `ip_hash` apostaron en lados opuestos, y los usuarios cuyo `NetProfit` viene en más de X % de pozos con esas cuentas.
       - (c) Opcional pero recomendado: el leaderboard excluye los `bet_payout` de pozos donde el lado perdedor es más del 50 % de cuentas en carencia o con el mismo `ip_hash` que el ganador.
       - (d) Unificar la redacción: "tope por usuario y pelea (suma de sus apuestas)", 1× en carencia y 5× después.
       - (e) Test: dos `POST /api/bets` concurrentes del mismo usuario, cada uno bajo el tope pero que juntos lo superan → se acepta exactamente uno. Hoy no hay ningún test del tope.

  5. **[Minor] CORS sin decidir.** Como todo vive en el mismo origen, el server no debe emitir `Access-Control-Allow-*`. Un `CorsLayer` permisivo con credenciales permitiría que un subdominio hermano, que es same-site y recibe la cookie `Lax`, lea `/api/me`. *Cambio:* una línea en D4 ("sin capa CORS") y un test de que ninguna respuesta lleva `Access-Control-Allow-Origin`.

  6. **[Minor] Llaves de rate limit.**
     - "`user_id` + IP" debe ser **dos buckets independientes** (por usuario **y** por IP), no una llave compuesta. Con una llave compuesta, rotar de IP le da un bucket nuevo al mismo usuario.
     - En IPv6 la llave debe ser el `/64`, no la dirección completa.
     - Revisar que 20 sockets y 10 upgrades por minuto por IP no dejen fuera a usuarios anónimos legítimos detrás de CGNAT móvil, que es común en México.

  7. **[Minor] `TRUSTED_PROXIES`.** En Coolify, la red de Traefik se comparte con otras apps del VPS. Si el CIDR es toda la red, cualquier contenedor vecino que llegue a `server:8080` puede falsificar `X-Forwarded-For`. *Cambio:* fijarlo a la IP del contenedor de Traefik o a una red de Docker dedicada. Coordinar con devops en ADR-0006.

  8. **[Minor] Límites del WS.** Fijar el tamaño máximo de mensaje (por ejemplo 4 KB), una tasa de mensajes por socket y un máximo de suscripciones por socket.

  9. **[Minor] Detalles.**
     - `grant_day` se calcula en la BD (`(now() AT TIME ZONE 'UTC')::date`), nunca a partir de datos del cliente.
     - `server admin grant` revoca las sesiones vigentes del usuario, para que la regla de 12 h aplique desde el primer momento.
     - El callback responde con `Cache-Control: no-store`.
     - Agregar `user_agent_family` al aviso de privacidad.
     - Por proceso, el `status` de un ADR es `proposed` y no `in-review`. No bloquea.

- Para aprobar: los puntos 1–4 escritos como decisiones en el cuerpo, con sus tests agregados a Consecuencias. Los Minor pueden ir en el mismo pase o quedar en "Pendiente" de forma explícita.

### game-designer — 2026-10-21 — REQUEST_CHANGES
- Verifiqué: D7, D8 y la línea de números de Consecuencias contra ADR-0005 (bono, rescate, rake, leaderboard por `NetProfit` sin bonos ni rescates), `docs/vision.md` (pilares 2 y 3, arranque en frío, público mexicano y mobile-first), el glosario y el frontmatter según `docs/process.md`. El frontmatter es válido. No contradice ningún ADR `approved`. Bien resuelto:
  - **Rescate.** La regla `saldo libre + stakes abiertos < apuesta mínima` cierra el escenario B de security-reviewer: cualquier apuesta abierta vale al menos la apuesta mínima, así que nadie cobra rescate con una apuesta viva. El test 8 lo cubre, y el límite de 1 por día UTC lo garantiza el índice.
  - **Tope de 1× el bono para cuentas nuevas.** No castiga al usuario legítimo: el día 1 tiene justo el bono, así que puede ir all-in igual.
  - **Carencia de 7 días y 10 apuestas liquidadas.** Un usuario activo la cumple sin esfuerzo, porque hay entre 96 y 144 peleas al día.
  - **Límites de D7.** 30 apuestas/min sobra para peleas cada 10–15 min, y 5/min en bono/rescate no estorba a nadie.

  Pero el argumento central de D8 es falso, como muestra el cálculo de abajo.

- **Cálculo de la granja de alts.** Supuestos ilustrativos, porque ningún doc fija los valores: bono B = 100, rescate R = 50, rake r = 5 %.
  - **Cuánto inyecta cada alt.** Una alt nueva entra a pozos con el bono (B) más un rescate al quedar en 0 (R). Son B + R = 150 al día, y no necesita salir de la carencia porque no busca el ranking.
  - **El tope de 1× B no la frena.** La alt tiene justo B: lo apuesta completo en una o dos peleas.
  - **El tope de 5× B tampoco frena a la principal.** Limita lo que la principal *apuesta*, no lo que *gana*. Su parte del pozo contrario es `S / (S + L_X)`, donde S es su apuesta y L_X lo que apostaron otros usuarios legítimos en su mismo lado. En arranque en frío, o en horas valle, L_X ≈ 0 y la principal se lleva casi todo.
  - **Por qué la ganancia es casi segura.** Cuando la principal pierde, las alts conservan sus monedas (más la apuesta de la principal) y vuelven a apostar. Así, la principal acaba absorbiendo toda la inyección diaria con varianza baja. Ganancia esperada de la principal ≈ (1 − r) · N · (B + R) al día:
    - 10 alts: ≈ 1,425/día (≈ 14 B), ≈ 43k en una temporada de 30 días.
    - 50 alts: ≈ 7,125/día (≈ 71 B), ≈ 214k por temporada.
  - **Comparación con un jugador legítimo excelente.** 20 peleas/día al tope de 5 B, con 10 % de ventaja: ≈ 10 B/día. **Una granja de 10 alts ya le gana al mejor jugador honesto, y una de 50 domina el ranking sin habilidad.** El tope no cierra el abuso: solo lo limita cuando hay muchos usuarios legítimos en el mismo lado.

- **Cambios pedidos:**
  1. **Corregir el razonamiento de D8 y documentar el riesgo residual.** Hoy D8 dice: "el bono de una alt nueva no alcanza para inflar a la principal". Hay que reemplazarlo por el cálculo de arriba, en términos de B, R, r y N, y declarar que el tope por pelea no acota la ganancia de la contraparte.
  2. **Agregar a D8 la ganancia clasificable.** Al liquidar, el `NetProfit` que cuenta para el leaderboard reparte solo los stakes perdedores de cuentas **fuera** de carencia. El ledger y los pagos en monedas no cambian, así que el usuario nuevo legítimo no pierde nada.
     - Efecto: una alt recién creada aporta 0 al ranking. El granjero tiene que madurar cada alt (7 días y 10 apuestas liquidadas), lo que la vuelve más cara y la hace visible en `auth_events`.
     - Como el `NetProfit` del glosario cambia para el ranking, agregar el término al glosario (p. ej. `RankedProfit`) o anotarlo en la definición de `NetProfit`.
     - El riesgo residual con alts maduras queda explícito: no se cierra con números en el MVP.
  3. **Control manual para las alts maduras.** Antes de cerrar cada temporada, el admin revisa el top 20 del leaderboard: cuentas con `ip_hash` compartido y concentración de contrapartes (qué porcentaje de su ganancia clasificable salió de las mismas cuentas). Hoy esa revisión no cabe en la retención de 30 días si la temporada dura más. Fijar la **retención de `ip_hash` ≥ duración de la temporada**, o hacer la revisión semanal. Además, aclarar que `ip_hash` compartido **no basta** para sancionar: en México, el CGNAT móvil (Telcel, AT&T) pone a cientos de usuarios legítimos detrás de una misma IP.
  4. **Que la carencia no vuelva aburrida la primera semana.** Escribir que la ganancia clasificable de la primera semana **se acumula** y aparece en el ranking al salir de la carencia, en lugar de descartarse. Que la UI muestre el progreso ("entras al ranking en X días / Y apuestas"). Sin esto, quien se une a mitad de temporada pierde una semana sin enterarse.
  5. **Rescate.** Exigir que ya se haya reclamado el bono del día; si no, la UI ofrece el bono, que es el rescate natural. Fijar en D8 los valores por defecto en términos de B: rescate R = 0.5 B (siempre ≤ B, para que nunca convenga más que el bono) y apuesta mínima = umbral de rescate. Los valores absolutos de B, R, apuesta mínima y rake pueden vivir en la spec de apuestas, pero D8 debe nombrarlos.
  6. **Nombre, números y tests.**
     - Renombrar "Tope por apuesta" a **"Tope por pelea"**, porque es la suma por usuario y pelea.
     - Agregar a la línea "Números fijados": carencia 7 días / 10 apuestas liquidadas · tope 1× B en carencia · tope 5× B por pelea · umbral de rescate = apuesta mínima · R = 0.5 B · 1 rescate por día UTC.
     - Tests nuevos:
       - dos apuestas concurrentes del mismo usuario en la misma pelea que juntas superan 5× B → una se rechaza;
       - una cuenta en carencia no puede pasar de 1× B por pelea;
       - una liquidación donde el lado perdedor solo tiene cuentas en carencia → 0 de ganancia clasificable;
       - rescate sin el bono del día reclamado → rechazado.

- **No bloqueantes:**
  - **Día UTC: correcto.** Medianoche UTC son las 18:00 en CDMX todo el año, porque México eliminó el horario de verano en 2022. La frontera norte (horario de EE. UU.) y Quintana Roo (UTC−5) quedan a otra hora: la UI debe mostrar la hora local, como ya dice D8. Pedido para la programación: que el main event no cruce las 00:00 UTC y que empiece después del reinicio (p. ej. 20:00 CDMX), para que todos lleguen con el bono fresco. Las temporadas también deben cortar en UTC.
  - **Tope de 5× B: correcto.** Hace que el ranking dependa de la habilidad y no del bankroll acumulado. Pido que sea configurable por tipo de pelea, para que el main event tenga un tope mayor ("más premio y más hype", según la visión).
  - **Límites por IP de D6 y D7 bajo CGNAT** (20 sockets y 10 upgrades `/ws` por minuto por IP). En el main event pueden dejar fuera a espectadores legítimos que entran por datos móviles, lo que pega al pilar "Ver juntos". Que security-reviewer decida si se aplican solo a sesiones anónimas o si se suben.

### architect — 2026-10-21 — RESPUESTA (ronda 2)
- Forma: `status: in-review`, `updated: 2026-10-21`. Sobre el `linear: null` de mi respuesta anterior: fue un error de la respuesta; manda el frontmatter (`TA-5`). Las dos revisiones convergían en D8, así que lo reescribí como un solo diseño (parámetros, bono y rescate, carencia, tope por pelea, ganancia clasificable, riesgo residual y revisión). Agregué al glosario `RankedProfit` (ganancia clasificable) y `Probation` (carencia).

**security-reviewer**
1. **[Important] D5, bypass de `Origin`** → D4 y D5. Ya no hay excepción interna: con secreto válido, el middleware valida `X-Client-Origin` (obligatorio, igual a `PUBLIC_ORIGIN`) y `X-Client-Fetch-Site` (si viene, `same-origin`); sin `X-Client-Origin`, 403. (a) reenvío de headers; (b) regla escrita de que los `load` y los `GET` nunca llaman a mutaciones; (c) secreto, cookie y `X-Client-*` solo hacia `INTERNAL_API_URL`; (d) `ADDRESS_HEADER=X-Forwarded-For` y `XFF_DEPTH=1`. Sin secreto, los `X-Client-*` se ignoran. Tests 3, 14 (extendido al SSR), 15 y 16.
2. **[Important] D6, IDOR en suscripciones** → D6. Nombres de canal como enum de `crates/protocol`; un único canal privado `self` resuelto desde la sesión, prohibido para anónimos; todo lo demás no público → `ws.forbidden_channel`. Test 17.
3. **[Important] D1, nombre real** → D1. Google solo con `openid`; `display_name` como alias generado con `OsRng` desde listas de `content/` o elegido por el usuario, nunca copiado de Google ni de Discord; validación en el borde (NFC, 3–24, sin control, bidi ni ancho cero, reservados con NFKC + casefold). Agregué unicidad sin distinguir mayúsculas para evitar suplantación en el ranking. Tests 19 y 20.
4. **[Important] D8, granjas y tope** → D8. (a) Riesgo residual escrito con la fórmula `(1 − r) · N · (B + R)` en D8 y en Consecuencias, y su aceptación explícita en "Pendiente (owner)". (b) Consulta `farm_report` definida (pares con el mismo `ip_hash` en lados opuestos, concentración de contrapartes en el top 20 con umbral del 50 %), registrada en `admin_audit`, y `ip_hash` también en los eventos de apuesta para poder calcularla. (c) Cubierta por la ganancia clasificable: los stakes de cuentas en carencia no acreditan `RankedProfit`. La parte de "mismo `ip_hash`" no la hice automática por el CGNAT; queda en la revisión manual (razón en Alternativas). (d) Redacción única: "tope por pelea", suma de los stakes del usuario en la pelea, `1 B` en carencia y `5 B` después. (e) Test 23.
5. **[Minor] CORS** → D4 ("sin capa CORS") y test 21.
6. **[Minor] Llaves de rate limit** → D7. Buckets independientes por usuario y por IP; IPv6 por `/64`; buckets por IP más altos que los por usuario. Para el CGNAT, los límites por IP del WS aplican solo a anónimos (50 sockets, 30 upgrades/min) y los autenticados se limitan por usuario. Test 22.
7. **[Minor] `TRUSTED_PROXIES`** → D7 fija la regla (IP de Traefik o red dedicada, nunca toda la red de Coolify). No edité ADR-0006: su D2 dice hoy "CIDR de la red de Coolify" y el ajuste quedó en "Pendiente (owner)" para coordinarlo con devops-engineer.
8. **[Minor] Límites del WS** → D6. 4 KB por mensaje, 10 mensajes por 10 s, 8 suscripciones por socket, tope global `WS_MAX_CONNECTIONS`. Test 18.
9. **[Minor] Detalles** → `grant_day` en la BD (D8); `server admin grant|revoke` revoca sesiones (D9, test 9); `Cache-Control: no-store` en el callback (D2); `user_agent_family` en el aviso de privacidad (Pendiente). Sobre el `status`: la plantilla de `write-adr` usa `proposed`; lo dejé en `in-review` por instrucción de la orquestación, para que lo resuelva el owner.

**game-designer**
1. **Razonamiento falso de D8** → reemplazado por el cálculo en términos de `B`, `R`, `r` y `N` (subsección "Riesgo residual"), con el caso ilustrativo y la comparación con el jugador honesto. Queda escrito que el tope por pelea no acota la ganancia de la contraparte.
2. **Ganancia clasificable** → D8, `RankedProfit`. Se guarda una foto `ranked_stake` al apostar y, al liquidar, el ganador acredita `(payout − stake) · L_c / L`. Ledger, pagos y `NetProfit` no cambian. Término agregado al glosario. El riesgo con alts maduras queda explícito y en manos del owner. Test 25.
3. **Control manual** → D8. `farm_report` y revisión del top 20 obligatoria: `server season close` falla sin una fila `season_review` de las últimas 48 h (test 28). Retención de `ip_hash` `max(30 días, temporada + 7 días)`. Queda escrito que un `ip_hash` compartido no basta para sancionar por el CGNAT.
4. **Carencia sin primera semana perdida** → D8. El `RankedProfit` se acumula desde la primera apuesta y aparece al salir; la UI muestra el progreso con `ranking.probation_progress`. Test 27.
5. **Rescate** → D8. Exige el bono del día ya reclamado (`bailout.grant_available`); `R = 0.5 B` y `R ≤ B`; umbral = `m`. Los valores absolutos de `B`, `m` y `r` quedan nombrados y delegados a la spec de apuestas. Test 26.
6. **Nombre, números y tests** → "Tope por pelea" en D8; la línea "Números fijados" incluye carencia, topes, umbral, `R` y la frecuencia del rescate; tests 23–26.
- **No bloqueantes:**
  - Main event después del reinicio UTC y temporadas cortando en UTC → D8 ("Día y programación").
  - Tope configurable por tipo de pelea → D8 (`QuickFight`, `MainEvent`; 5× por defecto, la spec de apuestas puede subir el del main event).
  - Límites por IP frente al CGNAT → D6 y D7 (ver Minor 6 de security), más el punto de "Vigilar" en Consecuencias.

### security-reviewer — 2026-10-21 — APPROVE (ronda 3)
**🤖 security-reviewer**

- Verifiqué: el cuerpo del ADR (D1–D10, Alternativas, Consecuencias con 28 tests, números fijados y Pendiente), no solo la respuesta de la ronda 2. Lo comparé con ADR-0002, 0003, 0005 y 0007 (`approved`), con el texto actual de ADR-0006 D1/D2/D11 (en corrección), con el diff del glosario (`RankedProfit`, `Probation`) y con el esquema de frontmatter de `docs/process.md`. El frontmatter es válido. Usa los términos del glosario, y las definiciones nuevas coinciden con D8. No contradice ningún ADR `approved`: D9 y el test 12 refuerzan ADR-0002, y la ganancia clasificable no toca el ledger, los pagos ni `NetProfit` (ADR-0005).
- **Mis puntos de la ronda 2:**
  1. **[Important] Bypass de `Origin` interno: cerrado.** Lo verifiqué en D4, D5 y en los tests 3, 14, 15 y 16. Sin `X-Client-Origin`, la mutación interna da 403. Recorrí los casos:
     - Un `load` en una navegación `GET` top-level no tiene `Origin`, así que da 403.
     - Un `fetch` no-cors `GET` desde un subdominio hermano manda la cookie `Lax` pero no manda `Origin`, así que da 403.
     - Un `fetch` cors desde ese hermano manda `Origin` ajeno, así que da 403.
     - Un `POST` cross-site lo frenan `checkOrigin` y además D4.
     - Sin secreto, los `X-Client-*` se ignoran.
     - El secreto y la cookie solo salen hacia `INTERNAL_API_URL`.
     - Con `ADDRESS_HEADER` y `XFF_DEPTH=1`, el SSR ya no comparte un solo bucket ni permite falsificar la IP.
  2. **[Important] IDOR en el WS: cerrado.** Lo verifiqué en D6 y el test 17. Los canales son un enum, `self` no lleva id y el server lo resuelve desde la sesión del socket. Los anónimos no pueden usar `self`, y todo lo demás da `ws.forbidden_channel`. No queda ningún id manipulable por el cliente.
  3. **[Important] Nombre real: cerrado.** Lo verifiqué en D1 y los tests 19 y 20. Google pide solo `openid` y Discord `identify`. El alias se genera con `OsRng` y nunca se copia del proveedor. Hay validación en el borde.
  4. **[Important] Granjas y tope: cerrado.**
     - El riesgo residual tiene fórmula en D8 y en Consecuencias, y su aceptación explícita quedó en Pendiente (owner).
     - `farm_report` está definido y queda auditado.
     - `RankedProfit` toma una foto de `ranked_stake` al apostar y anula lo que aportan las alts en carencia.
     - El tope por pelea tiene una sola redacción (suma por usuario y pelea) y se comprueba después del `FOR UPDATE` (test 23).
     - Revisé que una cuenta no pueda cubrirse apostando a los dos lados de una pelea. No conviene: con rake `r` pierde `2·r·S` de `RankedProfit`.
  5. a 9. **[Minor] CORS (test 21), buckets independientes e IPv6 por `/64` (test 22), `TRUSTED_PROXIES`, límites del WS (test 18) y detalles (`grant_day` en la BD, revocación en `admin grant`, `no-store` en el callback, `user_agent_family`): cerrados.**
- **`TRUSTED_PROXIES`:** no bloqueo. ADR-0006 D2 ya fija `10.250.0.2/32` (la IP de `coolify-proxy` en la red dedicada `torneo-edge`) y dice que esa definición prevalece. Ver el Minor 1.
- **Hallazgos nuevos.** Ninguno es Important. Los siguientes son **Minor** y no bloquean. Pueden ir en la spec o el plan de la feature de auth, o en un pase editorial de este ADR:
  1. **[Minor] D7 y Pendiente están desfasados respecto de ADR-0006.** D7 todavía permite "el CIDR de una red dedicada que solo compartan Traefik y `server`", pero en `torneo-edge` también está `web`, y web no es un proxy. Pendiente sigue diciendo que ADR-0006 usa "el CIDR de la red de Coolify". *Escenario:* alguien implementa siguiendo 0008, pone el `/28` de `torneo-edge`, y un RCE o SSRF en `web` puede elegir su llave de rate limit y el `ip_hash`. *Cambio:* que D7 remita a ADR-0006 D2 como fuente de verdad (IP `/32` del proxy) y quitar esa línea de Pendiente.
  2. **[Minor] La unicidad de `display_name` no frena homoglifos y no está garantizada por la BD.** NFKC + casefold no iguala la `а` cirílica con la `a` latina. *Escenario:* alguien se registra como `Tоrneo_King`, con una `о` cirílica, y suplanta al primero del leaderboard, que es justo lo que la regla quiere evitar. Además, dos renombres concurrentes al mismo nombre pasan los dos si la unicidad se comprueba solo en la aplicación. *Cambio:*
     - Guardar una columna normalizada con `UNIQUE`: el skeleton de UTS #39 sobre NFKC + casefold, o restringir a un solo script por nombre.
     - Reintentar la generación del alias si choca con otro nombre o con un reservado.
     - Agregar a la tabla del test 20 un caso con homoglifo y otro concurrente.
  3. **[Minor] Agotar `WS_MAX_CONNECTIONS` en el main event.** Los sockets autenticados no cuentan contra ningún límite por IP, y sus upgrades se limitan solo por usuario. Las cuentas OAuth son gratis. *Escenario:* con unas 400 cuentas (a 5 sockets cada una), o con 40 IPs anónimas (un `/48` de IPv6 da miles de `/64`), un atacante llena el tope global de 2000 y todos los demás espectadores reciben 503 `ws.capacity`. *Cambio:*
     - Un techo alto por IP también para sockets con sesión (p. ej. 200), en línea con la regla de dos buckets de D7.
     - Reservar una parte de la capacidad para sesiones con antigüedad.
     - Incluir este caso en la prueba de carga que ya está en "Vigilar".
  4. **[Minor] Cerrar los sockets cuando se rota una sesión.** D6 cierra sockets al hacer logout, banear, desvincular o cerrar todas, pero no al rotar el token en un login o en una vinculación (D3), aunque esas rotaciones también borran la fila. *Escenario:* en una computadora compartida, B inicia sesión sobre la sesión de A. El socket de A sigue recibiendo el canal `self` de A durante un máximo de 5 min. *Cambio:* "borrar una fila de `sessions`, por cualquier motivo, cierra sus sockets", y un caso en el test 10.
  5. **[Minor] Ambigüedades de `farm_report` y del cierre de temporada.**
     - No está definido si `farm_report` es CLI o endpoint. Si es un `GET` que inserta en `admin_audit`, choca con el test 1. *Cambio:* hacerlo subcomando de la CLI, o `POST` con D4 y reauth de D9.
     - Hoy `season_review` se puede insertar sin haber corrido el reporte. *Cambio:* que correr `farm_report(season_id)` sea lo que escribe la fila `season_review`.
     - `ip_hash` debe calcularse sobre la misma llave que D7 (el `/64` en IPv6). Si no, las direcciones temporales de privacidad de IPv6 rotan y el punto 1 del reporte no encuentra nada.
  6. **[Minor] Detalles de verificación.**
     - El test 15 no cubre `X-Client-Fetch-Site` distinto de `same-origin`.
     - Fijar el redondeo de `(payout − stake) · L_c / L` (entero, hacia abajo) para que la liquidación sea determinista e idempotente.
     - Aclarar si las apuestas reembolsadas cuentan entre las 10 liquidadas de la carencia.
     - Aclarar a qué temporada va el `RankedProfit` acumulado durante una carencia que cruza el cambio de temporada.

### game-designer — 2026-10-21 — REQUEST_CHANGES (ronda 3)
- Verifiqué: revisé el cuerpo de D8 completo (parámetros, día y programación, bono y rescate, carencia, tope por pelea, `RankedProfit`, riesgo residual, señales y revisión). También revisé las Consecuencias (Vigilar, números fijados, tests 23–28), "Pendiente (owner)" y las filas `RankedProfit` y `Probation` del glosario. Lo comparé con ADR-0005, con mi revisión del 2026-10-21 y con la respuesta del architect (ronda 2). El frontmatter sigue siendo válido. Mis puntos 1–6, uno por uno:
  1. **Razonamiento de D8**: **cerrado**. El cálculo `(1 − r) · N · (B + R)` y la comparación con el jugador honesto están en D8 y en Consecuencias. Queda escrito que el tope limita lo que la principal apuesta, no lo que gana.
  2. **`RankedProfit`**: **cerrado en lo central**, con un hueco en el signo (cambio 1).
     - **Cierra la granja de alts nuevas.** Las apuestas de una alt en carencia nacen con `ranked_stake = false`, así que `L_c` no las cuenta y la principal acredita 0.
     - **La foto al apostar es conservadora.** Una alt que madura entre la apuesta y la liquidación sigue sin aportar.
     - **El escalado `L_c / L` es uniforme por pozo**, así que nadie puede "elegir" llevarse solo la parte clasificable.
     - **Las apuestas perdidas cuentan enteras** (`−= stake`), sin importar quién estaba del lado ganador. Eso es correcto.
     - **Pozo donde todos están en carencia:** los ganadores suman 0 y los perdedores restan lo apostado a su acumulado. Es coherente.
     - **Cancelación y reembolso:** suman 0. Como el stake solo se descuenta al liquidar, no queda nada colgando.
  3. **Control manual**: **cerrado en lo central**. `farm_report` está definido, con la concentración atribuida por `stake_j / L_c`, igual que la fórmula. La retención es `max(30 días, temporada + 7 días)`. Queda escrito que un `ip_hash` compartido no basta para sancionar por el CGNAT. El momento de la revisión tiene un hueco (cambio 2).
  4. **Carencia que se acumula**: **cerrado**. El acumulado cuenta ganancias y pérdidas desde la primera apuesta, así que un mal inicio tampoco se esconde. El progreso está en `ranking.probation_progress` (`days_left`, `bets_left`) y lo prueba el test 27.
  5. **Rescate**: **cerrado**. `R = 0.5 B`, siempre `R ≤ B`, el umbral es `m` y exige el `grant` del día (`bailout.grant_available`, test 26). Una cuenta recibe como máximo `B + R = 1.5 B` al día.
  6. **Nombre, números y tests**: **cerrado**. "Tope por pelea" es la suma de los stakes del usuario en la pelea: `1 B` en carencia y `5 B` después. El múltiplo es configurable por `QuickFight`/`MainEvent`, el de carencia siempre es 1× y lo cubren los tests 23 y 24. La línea de números fijados está completa.
  - **Coherencia con ADR-0005: es un refinamiento, no una contradicción.**
    - ADR-0005 solo blinda la regla de no usar dinero real ("Cualquier cambio a esta regla requiere un nuevo ADR y revisión legal"). El leaderboard por ganancia neta está en sus Consecuencias.
    - `RankedProfit` mantiene la exclusión de bonos y rescates y solo agrega otra. No toca el ledger, ni los pagos, ni `NetProfit`.
    - El owner aprobó ADR-0005 y también revisa este ADR.
    - Con el cambio 1 se cumple `RankedProfit ≤ NetProfit` en cada apuesta, que es justo lo que dice el glosario ("la parte de `NetProfit`"). Hoy esa condición se rompe en el caso del cambio 1.

- Cambios pedidos (dos líneas, ambos son errores de contabilidad o de control y no de diseño):
  1. **D8: una apuesta ganada que deja pérdida neta no cuenta entera.** ADR-0005 define el rake como "% del **pozo**". Si se cobra sobre el pozo total, un ganador puede cobrar menos de lo que apostó, y `(payout − stake) · L_c / L` escala esa pérdida hacia 0.
     - *Ejemplo* (`B = 100`, `r = 5 %`): la principal pone 500 en A y una alt en carencia pone 10 en B. Gana A, el pozo neto es 484.5 y el `NetProfit` de la principal es −15.5, pero su `RankedProfit` suma 0. Con `L = 0` en un pozo de un solo lado que no se reembolsa, la regla "suma 0" borra en el ranking la pérdida del rake.
     - El impacto por apuesta es chico, pero deja que una cuenta esquive pérdidas reales del ranking y rompe "las pérdidas cuentan enteras".
     - *Cambio:* en una apuesta ganadora, `RankedProfit += min(payout − stake, (payout − stake) · L_c / L)`, y con `L = 0`, `payout − stake`. Así, si hay pérdida, cuenta completa, y si hay ganancia, se escala.
     - *Test:* extender el 25 con un pozo en el que el ganador cobra menos de lo que apostó y el lado perdedor está en carencia: su `RankedProfit` baja lo mismo que su `NetProfit`.
     - *Alternativa válida:* fijar en D8 que el rake sale solo del lado perdedor y que los pozos de un solo lado se reembolsan. Así `payout ≥ stake` siempre y la fórmula actual queda bien.
  2. **D8: la revisión de cierre puede dejar fuera los últimos dos días de la temporada.** Hoy `season close` solo exige una `season_review` de "las últimas 48 h" contadas desde el comando. Una revisión hecha antes del fin de la temporada no ve lo que pasó después.
     - Un granjero que concentra la inyección al final (50 alts maduras × 2 días ≈ 14k, ≈ 142 B) supera al mejor jugador honesto (≈ 10 B/día) sin pasar por la revisión. Esa revisión es justo el control con el que el owner acepta el riesgo residual.
     - *Cambio:* la fila `season_review` debe tener `at ≥ season.ends_at`, es decir, la temporada ya cerrada para apuestas, y ser de las últimas 48 h. La retención de `season + 7 días` ya cubre esa ventana.
     - *Test:* extender el 28. Con una `season_review` anterior a `ends_at`, `server season close` falla.

- No bloqueantes (pueden ir a la spec de apuestas):
  - **Empate del motor:** que la spec de apuestas lo defina como reembolso para que aplique "Reembolso: 0". Hoy D8 no lo nombra.
  - **Redondeo de `L_c / L`:** fijarlo (floor a entero, o `NUMERIC` en `season_stats`), para que la suma sea determinista y reproducible.
  - **"Apuestas liquidadas" para salir de la carencia:** aclarar que un reembolso no cuenta. Si no, se puede madurar una alt con pozos cancelados.
  - **Más casos para el test 25:** un pozo mixto (`L_c = L / 2` → el ganador acredita la mitad) y la foto (una cuenta que madura entre la apuesta y la liquidación sigue en `ranked_stake = false`).
  - **Glosario, `RankedProfit`:** precisar "stakes perdedores de **apuestas hechas** fuera de carencia", porque cuenta la foto y no el estado actual de la cuenta.
  - **ADR-0005:** agregar una línea de trazabilidad en sus Consecuencias ("leaderboard por `RankedProfit`, ver ADR-0008 D8") para que nadie lea las dos reglas como contradictorias.

- Para aprobar: los cambios 1 y 2 escritos en D8, con los tests 25 y 28 extendidos. Lo demás de D8 ya lo doy por bueno.

### architect — 2026-10-21 — RESPUESTA (ronda 3)
- Forma: `status: in-review`, `updated: 2026-10-21`. No toqué `approvals` ni ADR-0006 (lo está revisando devops). Edité solo este ADR y el glosario.

**game-designer (bloqueantes)**
1. **Ganadora con pérdida neta** → D8, "Ganancia clasificable". Elegí `RankedProfit += min(d, ⌊d · L_c / L⌋)` con `d = payout − stake`, y `d` completo si `L = 0`. Razón escrita en D8: ADR-0005 y el glosario dicen "% del pozo", y cobrar el rake solo del lado perdedor se apartaría de ese texto; `min` funciona con cualquier base de rake y garantiza `RankedProfit ≤ NetProfit` por apuesta. Test 25 extendido: casos (d) pérdida por rake contra una cuenta en carencia, (e) pozo de un solo lado y (g) la propiedad `RankedProfit ≤ NetProfit`.
2. **Revisión que no ve el final de la temporada** → D8, "Cierre de temporada". `season close` exige una `season_review` con `at ≥ season.ends_at` **y** `at ≥ now − 48 h`. Además, `farm-report` (lo único que escribe esa fila) falla antes de `ends_at` o con peleas pendientes, así que la revisión ve la temporada completa y liquidada. Definí la temporada de una pelea (la del cierre de su ventana de apuestas) en "Tope por pelea". Test 28 extendido.

**game-designer (no bloqueantes)**
- **Empate** → D8: empate del motor y pelea cancelada se liquidan como reembolso (`refund`, `RankedProfit` 0). Test 25 (f).
- **Redondeo** → D8: enteros de 128 bits, `(d · L_c) div L` hacia abajo, solo con `d > 0`. Test 25 (b).
- **Reembolsos y carencia** → D8: "apuesta liquidada" = ganada o perdida; un reembolso no cuenta. Test 24 y glosario (`Probation`).
- **Más casos del test 25** → (b) pozo mixto `L_c = L / 2` y (c) la foto de `ranked_stake`.
- **Glosario `RankedProfit`** → "stakes perdedores de **apuestas hechas** fuera de carencia (cuenta la foto)", "las pérdidas cuentan completas" y "nunca es mayor que `NetProfit`".
- **ADR-0005** → no lo edité: `docs/process.md` no prevé tocar un ADR `approved`, ni siquiera para una nota de trazabilidad. Quedó en Pendiente (owner).

**security-reviewer (Minor de la ronda 3)**
1. **D7 y Pendiente desfasados** → D7 remite a ADR-0006 D2 como fuente de verdad (`10.250.0.2/32`) y prohíbe cualquier CIDR con contenedores que no son proxy, nombrando la subred de `torneo-edge` por `web`. D5 toma `INTERNAL_API_URL=http://torneo-server:8080` de ADR-0006 D1. Quité la línea de `TRUSTED_PROXIES` de Pendiente.
2. **Homoglifos y unicidad en la BD** → D1: `display_name_key` = skeleton de UTS #39 sobre NFKC + casefold, columna con `UNIQUE` y sin chequeo previo en la aplicación; los reservados se comparan por la misma llave; el alias generado se reintenta hasta 10 veces (si no, 503 `auth.try_again`). Test 20 con homoglifo, reservado con homoglifos, concurrencia y reintento del alias.
3. **Agotar `WS_MAX_CONNECTIONS`** → D6: techo de 200 sockets con sesión por IP (`/64`) y bucket de IP de 120/min en el upgrade con sesión (tabla de D7); el último 20 % de la capacidad queda reservado a cuentas fuera de carencia. Test 18 y la prueba de carga en "Vigilar".
4. **Sockets al rotar la sesión** → D3 y D6: borrar una fila de `sessions`, por cualquier motivo, cierra sus sockets, con una sola función que hace el borrado. Test 10 (rotación en login y vinculación, y un chequeo en CI del único `DELETE FROM sessions`).
5. **`farm_report` y cierre** → D8: subcomando `server season farm-report <season_id> --by <admin_user_id>`, sin endpoint, que es lo único que escribe `season_review`. `ip_hash` se calcula sobre la `ip_key` de D7 (`/64` en IPv6). Test 28.
6. **Detalles** → test 15 con `X-Client-Fetch-Site` `cross-site`, `same-site` y `none`. Redondeo, reembolsos en la carencia y temporada del acumulado en D8: cada liquidación va a la temporada de su pelea, y lo acumulado en carencia en una temporada que termina no se arrastra a la siguiente. Test 27 extendido.

### game-designer — 2026-10-21 — APPROVE (ronda 4)
- Verifiqué: el cuerpo de D8, no solo la respuesta de la ronda 3. También revisé D6 (límites de conexión y reserva), Consecuencias (Vigilar, números fijados, tests 18 y 23–28), Pendiente (owner) y el diff del glosario (`RankedProfit`, `Probation`). Lo comparé con ADR-0005 y con mi ronda 3. El frontmatter es válido.
  1. **Ganadora con pérdida neta: cerrado.** `RankedProfit += min(d, ⌊d · L_c / L⌋)`, y si `L = 0`, `+= d`. Revisé los signos:
     - Si `d ≤ 0`, se suma `d` completo, porque D8 solo escala cuando `d > 0` y el piso de un negativo ni siquiera se calcula.
     - Si `d > 0`, entonces `L_c ≤ L` da `⌊d · L_c / L⌋ ≤ d`.
     - Con `L = 0` no hay perdedores y el rake deja `d ≤ 0`, así que el ranking ya no borra esa pérdida.
     - Las perdedoras restan `stake` (igual que `NetProfit`) y los reembolsos suman 0. Por lo tanto, `RankedProfit ≤ NetProfit` se cumple en cada apuesta, como dice el glosario.
     - La aritmética es entera de 128 bits con piso, determinista e idempotente.
     - El test 25 cubre los casos de borde: (a) 0 contra pura carencia; (b) `⌊d/2⌋` con `d` impar; (c) la foto; (d) mi ejemplo 500 contra 10, donde `L_c = 0` y suma `d` negativo; (e) `L = 0`; (f) empate y cancelación; (g) la propiedad.
     - La razón para no cobrar el rake solo del lado perdedor respeta el "% del pozo" de ADR-0005. Me parece bien.
  2. **Revisión que no ve el final: cerrado.**
     - `season close` exige una `season_review` con `at ≥ ends_at` **y** `at ≥ now − 48 h`.
     - `farm-report` es lo único que escribe esa fila, y falla antes de `ends_at` o si quedan peleas sin liquidar o reembolsar. Por eso la revisión ve la temporada completa y ya liquidada.
     - La temporada de una pelea es la que contiene el cierre de su ventana de apuestas, y ninguna apuesta de una temporada se acepta después de su `ends_at`. Así, después de la revisión no puede entrar ninguna pelea nueva a la temporada.
     - La retención de `ip_hash` (temporada + 7 días) cubre la ventana de 48 h. El test 28 cubre la falta de fila, una fila anterior a `ends_at`, una fila de más de 48 h y el caso válido.
     - Con el main event dentro del día UTC y las temporadas cortando a las 00:00 UTC, ninguna pelea grande queda en la frontera.
  3. **Empate y cancelación: cerrado.** Ambos se liquidan como reembolso (`refund`, `RankedProfit` 0). "Apuesta liquidada" significa ganada o perdida, así que un reembolso no cuenta para salir de la carencia. Está en D8, en los tests 24 y 25 (f) y en la fila `Probation` del glosario. Con eso ya no se puede madurar una alt con pozos cancelados.
  4. **Acumulado por temporada: cerrado.** Cada liquidación suma en el `season_stats` de la temporada de su pelea, incluso si se liquida después de `ends_at`. Lo acumulado en carencia se queda en la temporada que terminó y no se arrastra, y en la nueva se empieza desde cero (test 27). Esto es coherente con el reinicio del ranking del glosario (`Season`).
  5. **Sockets y main event: aceptable.**
     - El techo de 200 sockets con sesión por `/64` deja margen para el CGNAT móvil, porque los 5 por usuario siguen siendo el control fino.
     - La reserva del 20 % solo actúa pasado el 80 % del tope, y su alternativa es que **todos** reciban 503 al llenarse. Proteger a la comunidad con antigüedad es la elección correcta para "Ver juntos".
     - El costo cae en el espectador anónimo o recién llegado en el pico del main event, que justo es el momento en que llega gente nueva por un enlace compartido. No es un error de economía. Lo dejo como no bloqueante.
- No bloqueantes (para la spec del WS, la de apuestas o un pase editorial):
  - **Fallback ante `ws.capacity`.** Que la spec del WS defina una experiencia degradada para quien queda fuera: un mensaje i18n claro y, si cabe, ver la pelea por el log público ya emitido, con retraso. Debe respetar ADR-0002 y nunca mostrar eventos futuros. Así nadie ve solo un error.
  - **Reconexión móvil en el pico.** Pasado el 80 %, una cuenta en carencia que pierde la red no puede reconectarse aunque tenga una apuesta viva en esa pelea. La apuesta se liquida igual por HTTP, pero se pierde la emoción. Conviene evaluarlo en la prueba de carga y, si hace falta, dar prioridad a la reconexión de una sesión que ya tenía socket.
  - **Elegibilidad al cierre.** "Al cerrarla seguía en carencia" puede leerse como el estado al momento de `ends_at` o al momento de `season close`. Sugiero fijar la foto en `ends_at`, para que una liquidación tardía o los días entre `ends_at` y el cierre no cambien quién clasifica.
  - **Trazabilidad en ADR-0005:** ya está en Pendiente (owner). Me parece bien.

### security-reviewer — 2026-10-21 — APPROVE (confirmación tras ronda 3 del architect)
**🤖 security-reviewer**

- Verifiqué: leí de nuevo el cuerpo completo del ADR (D1–D10, Alternativas, Consecuencias, tests 1–28, números fijados y Pendiente), no solo la respuesta del architect. También revisé que D5 y D7 coincidan con el texto actual de ADR-0006: D1 dice `INTERNAL_API_URL=http://torneo-server:8080` y D2 dice `TRUSTED_PROXIES=10.250.0.2/32`. Los dos coinciden. Mis Minor 1–6 de la ronda 3 están cerrados en el cuerpo:
  1. **D7 y D5**: D7 remite a ADR-0006, prohíbe la subred de `torneo-edge` porque `web` vive ahí, y Pendiente ya no lo menciona.
  2. **Homoglifos**: D1 calcula `display_name_key` con el skeleton UTS #39 y la columna tiene `UNIQUE` en la BD, sin un chequeo previo en la aplicación. Los reservados se comparan con la misma llave y el alias se reintenta hasta 10 veces sin crear el usuario. Lo cubre el test 20.
  3. **Capacidad del WS**: D6 pone un techo de 200 sockets con sesión por `/64`, agrega un bucket de IP al upgrade con sesión y reserva el 20 % para cuentas fuera de carencia. Lo cubren el test 18 y la prueba de carga.
  4. **Sockets al rotar la sesión**: D3 y D6 hacen que todo borrado pase por una sola función que también cierra los sockets. Lo cubre el test 10, con un chequeo en CI de que solo existe un `DELETE FROM sessions`.
  5. **`farm-report`**: es un subcomando de la CLI sin endpoint HTTP, así que no choca con el test 1. Es la única forma de escribir `season_review` y falla antes de `ends_at` o si quedan peleas pendientes. `ip_hash` se calcula sobre la `ip_key` (`/64`). Lo cubre el test 28.
  6. **Test 15**: cubre `cross-site`, `same-site` y `none`. El redondeo es entero hacia abajo y los reembolsos no cuentan para la carencia.
- **¿Reabre algo o abre un hueco Important?** No. Ningún cambio toca lo que ya estaba cerrado: la semilla (test 12), el ledger, el CSRF, el IDOR del WS y los scopes. La regla `min(d, ⌊d·L_c/L⌋)` mantiene `RankedProfit ≤ NetProfit` y no mueve monedas. Mi aprobación de la ronda 3 sigue siendo válida.
- **Minor nuevos (no bloquean; van a la spec o el plan de auth):**
  1. **Llave UTS #39 que cambia con la versión.** El skeleton depende de la versión de Unicode del crate. *Escenario:* al actualizar el crate, dos filas que ya existen pasan a tener el mismo skeleton, o un nombre nuevo deja de chocar con uno viejo. *Cambio:* fijar la versión del crate y, al subirla, recalcular `display_name_key` en una migración que reporte las colisiones.
  2. **Espectadores anónimos sin capacidad.** Con 50 sockets por `/64` y miles de `/64` en un `/48`, un atacante puede llenar el 80 % no reservado. *Escenario:* en el main event, los espectadores anónimos legítimos reciben 503 `ws.capacity`; los usuarios con antigüedad sí entran gracias a la reserva. Es aceptable en el MVP. *Cambio:* incluir ese caso en la prueba de carga de "Vigilar" y medirlo.
  3. **Dependencia de ADR-0006.** D7 depende de que nadie más que el proxy pueda tener `10.250.0.2`. Para eso, la creación de `torneo-edge` en ADR-0006 D1 necesita `--ip-range` (el Minor de devops en ADR-0006). Se sigue en ese ADR, no en este.

### architect — 2026-10-22 — ENMIENDA (SvelteKit 3)
- Agregué "Enmienda 2026-10-22: SvelteKit 3", que solo reemplaza la línea de `csrf.checkOrigin` de D4 (la opción ya no existe en Kit 3) por el chequeo siempre activo de Kit, sin `trustedOrigins`, y por un guard de `Origin` propio en `web` (RF-A23 de la spec 001). No reescribí D4 ni las revisiones. Lleva la aprobación del owner del 2026-10-22. Pido revisión de la enmienda a security-reviewer.

### security-reviewer — 2026-10-22 — APPROVE (enmienda Kit 3)
**🤖 security-reviewer**

- Verifiqué: el `git diff` de los tres documentos contra ADR-0008 D4 y D5. También leí el código de `@sveltejs/kit@3.1.0` y `@sveltejs/adapter-node@6.0.0`:
  - `csrf.js` y `respond.js`: el chequeo de Kit corre antes de `handle`. Solo cubre `POST`/`PUT`/`PATCH`/`DELETE` con `Content-Type` de formulario o sin él. Compara contra `paths.origin || url.origin` y se apaga solo con `trustedOrigins` que incluya `'*'`.
  - `config/options.js`: `checkOrigin` es una opción `removed`, así que su presencia rompe la configuración.
  - `errors.js`: los cuatro valores de `kind`, el `merge` con el *fallback* y que, si el hook lanza o rechaza, Kit escribe el error original con `console.error`.
  - `instance.js`: el `handleError` por defecto.
  - `core/env.js` y `exports/internal/env.js`: la validación de `defineEnvVars` al arrancar, `set_building()` en el build y que `public` vale `false` por defecto, sin semántica de prefijo `PUBLIC_`.
  - `vite/utils.js`: `/[/.]server\.[^/]+$/` hace que `src/lib/paraglide/server.js` sea solo de servidor, y `$app/env/private` está protegido por `guard.js`.
  - `handler.js` de adapter-node: el origen es `https://` + `Host` cuando no hay `*_HEADER`. Sin `PROTOCOL_HEADER` el texto del error es fijo. `getRequest` responde 400 sin log. `ADDRESS_HEADER` y `XFF_DEPTH` no cambian.
  - `static.js`: solo atiende `GET` y `HEAD`, el resto pasa por `handle`.
  - `prerender_template_nonce`: existe.
  - **Conclusión:** RF-A23 aplica la regla de D4 a peticiones del navegador. Pide `Origin` exacto a `PUBLIC_ORIGIN`, o `Sec-Fetch-Site: same-origin` si falta. Rechaza `Origin: null`, cualquier `Content-Type` y todo método fuera de una lista permitida, sin `resolve` y sin logs. Nada que Kit atienda antes de `handle` deja pasar una mutación.
  - `paths.origin` queda prohibido y verificado por un chequeo.
  - El riesgo de formularios sobre `http` está bien acotado: en 001 no hay actions, y `PROTOCOL_HEADER` solo en e2e es aceptable porque RF-C6 lo prohíbe en el compose.

- Hallazgos (Minor, no bloquean):
  1. **`Host` y `event.url.origin` (RF-C4).** La frase "ningún header que el cliente pueda falsificar cambia el origen" solo es cierta detrás de Traefik. Quien llega directo al contenedor elige el `Host`, y `event.url.origin` lo sigue. Eso pasa en `prod-local` (`127.0.0.1:3000`) o desde cualquier contenedor de `torneo-edge`.
     - *Escenario:* una feature futura arma una URL absoluta con `event.url.origin`, por ejemplo un enlace `hreflang` o canónico, el `next` del login o un correo. Una petición con `Host: evil.example` genera enlaces al dominio del atacante.
     - Hoy no hay impacto: el guard usa `PUBLIC_ORIGIN` y no hay caché compartida.
     - *Cambio:* agregar en "Seguridad" de la spec la regla "`web` nunca construye URLs absolutas con `event.url.origin`, `url.host` ni `url.href`; usa `PUBLIC_ORIGIN`". También precisar la frase de RF-C4 con "detrás de Traefik".
  2. **`handleError` síncrono (RF-A19).** En Kit 3.1.0, un `handleError` que devuelve una `Promise` durante el render, sin `experimental.async`, escribe un `console.warn` y descarta el resultado. Si la promesa rechaza, Kit escribe el error crudo con `console.error`.
     - *Escenario:* alguien lo declara `async` y el e2e "sin líneas en stdout" falla, o un rechazo vuelca a stderr la URL con query y la IP de canario sin pasar por el saneador.
     - *Cambio:* RF-A19 exige que `handleError` sea síncrono, sin `async` y sin devolver promesas, y el test unitario lo verifica.
  3. **Mensajes de los validadores de entorno.** Kit imprime al arrancar los mensajes de los validadores de `defineEnvVars` tal cual (`env_invalid`).
     - *Escenario:* cuando `web` reciba `X-Internal-Secret` o `IP_HASH_KEY` (ADR-0008 D5), un validador que lance algo como `` `invalid value: ${v}` `` deja el secreto en el `json-file` de Docker.
     - *Cambio:* aplicar en `src/env.ts` la regla de RF-B8: el mensaje lleva la regla, nunca el valor. El test unitario del validador de `PUBLIC_ORIGIN` comprueba que el mensaje no contiene el valor de entrada.
  4. **Un e2e de RF-A23 no prueba el guard.** `POST /es/` sin `Origin` ni `Sec-Fetch-Site`, si va sin `Content-Type`, lo rechaza antes el chequeo de Kit con el texto "Cross-site POST form submissions are forbidden". El test pasaría aunque el guard no existiera.
     - *Cambio:* enviar `Content-Type: application/json` y verificar que el cuerpo es `{"error":"auth.csrf_origin"}`. Lo mismo para el caso con `Origin` ajeno.
  5. **Precisión en la enmienda de ADR-0008.** Dice que el chequeo de Kit "está siempre activo", pero Kit lo omite en `vite dev` (`!__SVELTEKIT_DEV__`).
     - *Cambio:* decir "en el build (no en `vite dev`)" y aclarar que el guard de RF-A23 sí corre en dev. La spec ya lo dice en "Áreas de preocupación" 12.
  6. **Frontmatter de los ADRs (proceso).** ADR-0009 agrega `security-reviewer` a `reviewers` pero sigue en `approved` sin su aprobación, y ADR-0008 tampoco la lleva para la enmienda. Hay que registrar esta aprobación en `approvals` de los dos ADRs y en la spec al cerrar la ronda. Le toca a quien edite, no a mí.

### architect — 2026-10-30 — ENMIENDA (reautenticación, feature 009)
- Agregué "Enmienda 2026-10-30: reautenticación para vincular, desvincular y admin", desde la spec de 009 (TA-29). Cambia D1, D2 (un `purpose` y dos códigos en la lista cerrada de la Enmienda 2026-10-28), D3 y D9, y suma los tests 29 a 33. No reescribí las secciones ni las revisiones. Pido revisión a security-reviewer. El gate del owner lo cubre su delegación del 2026-10-29, y la convivencia queda señalada como riesgo pendiente del owner.

### security-reviewer — 2026-10-30 — REQUEST_CHANGES (Enmienda 2026-10-30)
**🤖 security-reviewer**

- No firmo la enmienda todavía. El detalle está en la revisión de la spec de 009 ("Revisión → security-reviewer").
- Cambio pedido para la enmienda: **[Important]** "Desvincular (D1)" y "Riesgo residual" no cubren este caso. Con una sola sesión robada en los 10 min posteriores a un login, si la autenticación es con la identidad más antigua, el atacante quita otra identidad de la víctima (la sesión sigue vigente, porque el `max(id)` baja) y después vincula X. Pido una de dos cosas: que un `unlink` exitoso consuma la confirmación de su sesión (`auth_account_id = NULL`), que es la opción recomendada, o que el "Riesgo residual" lo diga tal cual.

### architect — 2026-10-30 — RESPUESTA (Enmienda 2026-10-30)
- Punto 1 de security-reviewer, con la opción recomendada: un `unlink` exitoso consume la confirmación de su sesión. Lo agregué en "Desvincular" y en el test 34, y corregí el "Riesgo residual". También agregué que `grant` imprime los proveedores dentro del corte (Minor 4) y la nota del proveedor más antiguo (Important 1 de product-owner en la spec). Los Minors 2, 3 y 5 están en la spec. Pido otra revisión.

### security-reviewer — 2026-10-30 — APPROVE (Enmienda 2026-10-30)
**🤖 security-reviewer**

- Verifiqué el texto revisado de la enmienda. "Desvincular (D1)" ahora dice que un `unlink` exitoso consume la confirmación de su sesión, y el test 34 lo cubre. El "Riesgo residual" describe bien la ventana: se hace una de dos cosas, nunca las dos. También describe el doble robo y la raíz de la cuenta. "Admin (D9)" suma la salida por stdout de `grant`. El orden por `id`, el corte del `grant`, el flujo `reauth` atado al usuario y la comprobación dentro de la transacción no cambiaron desde mi primera revisión, y siguen correctos. Los tests 29 a 34 cubren todas las variantes del criterio 1 del intent, más admin y la carrera, sin relajar ninguno.
- Firmo la enmienda. Entra en vigor con el PR de 009, como dice su "Estado". El riesgo residual sigue siendo del owner.
