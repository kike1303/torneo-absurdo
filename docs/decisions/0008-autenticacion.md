---
id: "0008"
title: Autenticación y sesiones
type: adr
status: in-review
risk: high
author: architect
reviewers: [security-reviewer, game-designer, owner]
approvals: []
supersedes: null
linear: TA-5
depends_on: ["0002", "0003", "0005", "0006", "0007"]
created: 2026-10-20
updated: 2026-10-20
---

# ADR-0008: Autenticación y sesiones

## Contexto
- Necesitamos un login sin costo, cómodo en el celular, que proteja el saldo de `Coins` (ADR-0005), la integridad de las apuestas y la semilla (ADR-0002).
- Un solo dominio público; Traefik (Coolify) enruta `/` a web y `/api`, `/ws` al server (ADR-0006). El server Rust es la única fuente de verdad del usuario; SvelteKit solo presenta.
- Un solo proceso `server` en un VPS de 1 vCPU: el estado en memoria (rate limiter, registro de sockets) es aceptable.
- Las monedas son gratis de obtener (bono, rescate), así que el riesgo real es **crear monedas o inflar el leaderboard**, no robar dinero.

## Decisión

### D1. Identidad y vinculación
- Solo OAuth, sin contraseñas: Google y Discord. Magic link queda fuera del MVP.
- La identidad es `(provider, provider_user_id)` (`sub` de Google, `id` de Discord), con `UNIQUE (provider, provider_user_id)` en `oauth_accounts`.
- **No se pide ni se guarda email.** Scopes: Google `openid profile`; Discord `identify`. El email nunca es llave ni criterio de vinculación (no existe en la BD).
- Datos guardados por cuenta OAuth: `provider`, `provider_user_id`, `created_at`. En `users`: `display_name` (inicializado desde el proveedor, editable), `role`, `locale`, `theme`, `created_at`, `banned_at`.
- **Nunca se vincula en automático.** Vincular un segundo proveedor solo desde una sesión iniciada (`POST /api/auth/link/{provider}`), con el flujo atado a esa sesión (D2). Si `(provider, id)` ya pertenece a otro `user`, se responde `auth.already_linked` y no se fusiona nada.
- Desvincular exige que quede al menos otro proveedor y revoca todas las demás sesiones del usuario.
- Los access/refresh tokens de Google y Discord se usan una sola vez para leer la identidad y se descartan; nunca se persisten.

### D2. Flujo OAuth (Authorization Code + PKCE)
- Crate `oauth2`. PKCE `S256`; el `code_verifier` vive solo en el servidor.
- Al iniciar (`POST /api/auth/{provider}/start` desde un formulario para login, `POST /api/auth/link/{provider}` para vincular; ambos pasan por D4 y responden 303 al proveedor) el server crea una fila en `oauth_flows`: `id_hash`, `provider`, `purpose (login|link)`, `link_user_id` (solo en link), `state` (128 bits, CSPRNG), `code_verifier`, `next`, `expires_at = now + 10 min`.
- El `id` del flujo (128 bits) va en la cookie pre-auth `__Host-oauth` (`HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=600`).
- En el callback: se busca el flujo por la cookie con `DELETE … RETURNING` (un solo uso); se rechaza con 400 `auth.invalid_state` si falta la cookie, falta `state`, no coincide, expiró o el `provider` no corresponde. Si `purpose = link`, además la sesión actual debe ser de `link_user_id`. La cookie `__Host-oauth` se expira siempre.
- La identidad se lee de los endpoints `userinfo` (Google `openidconnect.googleapis.com/v1/userinfo`, Discord `/users/@me`) con el access token, por TLS. **No se usa el `id_token`**, por eso no hace falta `nonce` ni JWKS; si en el futuro se usa, se valida firma, `iss`, `aud`, `exp` y `nonce`.
- `redirect_uri` exacta y registrada en cada proveedor: `${PUBLIC_ORIGIN}/api/auth/{provider}/callback`.
- **`next`**: solo se acepta si cumple `^/(?![/\\])[^\x00-\x1f]*$`; si no, se usa `/`. Se guarda en `oauth_flows` y se redirige a ese valor, nunca al que venga en el callback.
- Los `start` y `callback` limpian filas expiradas de `oauth_flows` de forma oportunista.

### D3. Sesión
- Token de 256 bits (`OsRng`), en base64url. En `sessions` se guarda solo `SHA-256(token)` y se busca por ese hash.
- Cookie `__Host-session` (`HttpOnly; Secure; SameSite=Lax; Path=/`, sin `Domain`), con `Max-Age` igual a la expiración absoluta.
- Columnas: `token_hash`, `user_id`, `created_at`, `authenticated_at`, `last_seen_at`, `expires_at` (absoluta), `ip_hash`, `user_agent_family`.
- **Expiración por inactividad: 7 días. Expiración absoluta: 30 días.** `last_seen_at` se actualiza como máximo una vez cada 5 min.
- Cada login emite un token nuevo; si la petición traía una `__Host-session` previa, esa fila se borra. Vincular un proveedor también rota el token.
- Logs: `tower-http::trace` redacta `Cookie`, `Set-Cookie`, `Authorization` y `X-Internal-Secret`; las rutas `/api/auth/*/callback` se registran sin query string. Ni token, ni `code`, ni `state` van a logs.

### D4. CSRF
- Ninguna mutación acepta `GET`/`HEAD`/`OPTIONS`.
- Un middleware de axum, aplicado a todo el router, revisa toda petición no segura: `Origin` debe ser exactamente `PUBLIC_ORIGIN`. Si `Origin` falta, se acepta solo con `Sec-Fetch-Site: same-origin`. Si no, 403 `auth.csrf_origin`.
- Única excepción: peticiones internas desde SvelteKit (D5).
- SvelteKit mantiene `csrf.checkOrigin` activado.

### D5. SSR (SvelteKit → server)
- SvelteKit llama al API por `INTERNAL_API_URL` (p. ej. `http://server:8080`, variable de entorno), no por Traefik.
- `handleFetch` reenvía **solo** `__Host-session`, y **solo** si el origen de la URL es exactamente `INTERNAL_API_URL`. Cualquier otro `fetch` sale sin cookie.
- Toda llamada interna lleva `X-Internal-Secret` (32 bytes, variable de entorno, nunca expuesto al cliente) y `X-Client-Ip` (de `getClientAddress()`). El server solo trata una petición como interna si el secreto coincide (comparación en tiempo constante); entonces omite el chequeo de `Origin` (ya lo hizo `checkOrigin` en las form actions) y usa `X-Client-Ip` para el rate limit.
- Toda respuesta del API que lea una sesión y toda página SSR con `locals.user` llevan `Cache-Control: private, no-store`.

### D6. WebSocket (`/ws`)
- En el handshake, `Origin` debe ser exactamente `PUBLIC_ORIGIN`; si falta o es ajeno, 403.
- La autenticación es por la cookie `__Host-session` en el upgrade; **nunca** por token en el query string. Sin sesión se permite el WS anónimo con solo canales públicos.
- En el MVP, el cliente solo manda mensajes de suscripción/desuscripción. **Ninguna acción que cambie estado viaja por WS**; apostar y reclamar van por `POST` HTTP.
- El server mantiene en memoria `session_id → sockets`. Al revocar una sesión (logout, ban, desvincular, cerrar todas) se cierran sus sockets en el acto. Como defensa extra, cada socket autenticado revalida su sesión cada 5 min y se cierra si ya no es válida.
- Límites: 5 sockets simultáneos por usuario y 20 por IP.
- Los canales públicos de pelea cumplen ADR-0002 sin importar la auth.

### D7. IP del cliente y rate limiting
- IP del cliente: se toma de `X-Forwarded-For`, usando la entrada de más a la derecha que **no** pertenezca a `TRUSTED_PROXIES` (CIDR de la red de Docker de Traefik, por variable de entorno). Si el peer no está en `TRUSTED_PROXIES`, se ignora `X-Forwarded-For` y se usa la IP del peer. Para peticiones internas, `X-Client-Ip` (D5).
- `tower-governor` con esa llave. Límites (en memoria, un solo proceso):

| Ruta | Llave | Límite |
|---|---|---|
| `POST /api/auth/*/start`, `/link`, `GET /callback` | IP | 10/min, ráfaga 5 |
| `POST /api/bets` | `user_id` + IP | 30/min, ráfaga 10 |
| `POST /api/bonus/claim`, `/api/bailout/claim` | `user_id` + IP | 5/min |
| Otras mutaciones | `user_id` + IP | 60/min |
| Upgrade `/ws` | IP | 10/min |

- El rate limiter es defensa adicional. **Los invariantes de la economía los garantiza la BD** (D8).

### D8. Economía: controles mínimos del MVP
- **"Día" = día calendario UTC.** Es fijo, sin horario de verano y fácil de probar; la UI muestra cuándo se reinicia en la hora local.
- **Bono diario**: índice único parcial `UNIQUE (user_id, grant_day) WHERE kind = 'grant'` en `ledger_entries`, con `INSERT … ON CONFLICT DO NOTHING`. Si no se insertó nada, 409 `bonus.already_claimed`.
- **Rescate**: solo si `saldo libre + stakes en apuestas abiertas < BAILOUT_THRESHOLD` (por defecto, la apuesta mínima). Máximo uno por día UTC: `UNIQUE (user_id, grant_day) WHERE kind = 'bailout'`.
- Toda operación que lea o reduzca saldo (apostar, rescate) toma `SELECT … FOR UPDATE` sobre la fila del usuario dentro de la misma transacción, para serializarse por usuario.
- **Cuentas nuevas** (menos de 7 días o menos de 10 apuestas liquidadas): no aparecen en el leaderboard de `NetProfit` y su tope por apuesta es 1× el bono diario.
- **Tope por apuesta**: 5× el bono diario por usuario y pelea (suma de sus apuestas en esa pelea).
- **Señales anti-granjas**: `ip_hash = HMAC-SHA256(ip, IP_HASH_KEY)` en `sessions` y en `auth_events` (login, link, bono, rescate). Retención: 30 días, luego se borra por un job diario. La IP en claro nunca se persiste ni se registra en logs del server. Es **dato personal** y se declara en el aviso de privacidad. En el MVP solo se usa para revisión manual del admin (p. ej. "N cuentas con el mismo `ip_hash` que reclaman bono").
- Los números (`7 días`, `10 apuestas`, `1×`, `5×`, umbral de rescate) son valores de configuración que valida game-designer contra ADR-0005.

### D9. Rol `admin`
- `users.role ∈ {user, admin}`. Se asigna **solo** con el subcomando `server admin grant|revoke <user_id>` ejecutado en el contenedor. Ningún endpoint escribe `role`, y ningún atributo de OAuth lo influye.
- Sesiones de admin: expiración absoluta de 12 h. Toda acción de admin exige `authenticated_at` de hace menos de 60 min; si no, 403 `auth.reauth_required`.
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
- **Día en `America/Mexico_City`**: más natural para el público mexicano, pero ata la lógica a una zona; UTC es más simple de razonar y probar.
- **Acciones por WS**: duplicaría la autorización y el CSRF; HTTP ya tiene ambos.

## Consecuencias
- Más fácil: no hay contraseñas, emails ni tokens de terceros que proteger; revocar es borrar filas.
- Más difícil: un usuario que pierde acceso a su único proveedor pierde la cuenta (no hay recuperación por email). Se mitiga invitando a vincular un segundo proveedor.
- Vigilar: el rate limiter y el registro de sockets en memoria dejan de servir si algún día hay más de un proceso `server`; eso requeriría un ADR nuevo.

**Números fijados:** `state`/flujo 128 bits, TTL 10 min, un solo uso · token de sesión 256 bits · inactividad 7 días · absoluta 30 días (admin 12 h, reauth 60 min) · `last_seen_at` cada 5 min · revalidación WS cada 5 min · tabla de límites de D7 · día UTC · retención de `ip_hash` 30 días.

**Tests mínimos (bloquean el merge de la feature de auth):**
1. Ningún `GET`/`HEAD` muta: un test recorre el manifiesto de rutas y falla si una ruta de mutación se registra con método seguro. Única excepción declarada: `GET /callback` (exigido por OAuth), que solo consume un flujo atado a la cookie pre-auth.
2. `Origin` ausente (sin `Sec-Fetch-Site: same-origin`) o ajeno da 403 en `POST` HTTP y en el handshake de `/ws`.
3. Una petición con `X-Internal-Secret` incorrecto se trata como externa (sin `Origin` → 403).
4. Callback sin `state`, con `state` reusado, expirado o de otra cookie → 400; un flujo `link` de otra sesión no vincula.
5. `next` externo (`https://…`, `//…`, `/\…`) redirige a `/`.
6. `(provider, id)` ya vinculado a otro usuario → `auth.already_linked`, sin fusión.
7. Bono: 20 `POST /bonus/claim` concurrentes del mismo usuario → exactamente 1 asiento `grant`. Igual para el rescate.
8. Rescate con una apuesta abierta que deja saldo libre en 0 → rechazado.
9. Ningún endpoint escribe `users.role` (test que intenta cada ruta de perfil/admin con `role` en el body, más un chequeo en CI de que el único SQL que escribe `role` vive en el módulo de la CLI).
10. Logout: tras `POST /logout`, el mismo token da 401 y su WS se cierra; `logout-all` invalida todas.
11. Sesión con `last_seen_at` de hace más de 7 días o `expires_at` vencido → 401.
12. Ningún endpoint ni mensaje WS expone `seed` antes de `Settled` con sesión de admin (extiende el test de ADR-0002).
13. Los logs de una corrida de login completa no contienen el token, el `code` ni el `state` (test de captura del subscriber de tracing).
14. `X-Forwarded-For` falsificado por el cliente no cambia la llave del rate limit.

## Pendiente (owner)
- **OK del owner** para aceptar el ADR (`risk: high`).
- **Registrar las apps OAuth** en Google Cloud Console y en el Discord Developer Portal (gratis, pero son servicios externos con cuenta del owner), con las `redirect_uri` de D2, y cargar los secretos en Coolify.
- **Aviso de privacidad** (LFPDPPP): declarar `display_name`, identificadores de proveedor e `ip_hash` con retención de 30 días.
- **Logs de acceso de Traefik**: decidir con devops-engineer si se desactivan o se limitan a ≤7 días, porque guardan IPs en claro y query strings (afecta a ADR-0006).

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
