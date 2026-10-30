---
id: "009"
title: Reautenticación para desvincular y para admin
type: plan
status: approved
risk: high
author: backend-engineer
reviewers: [architect, frontend-engineer]
approvals:
  - by: frontend-engineer
    date: 2026-10-30
  - by: architect
    date: 2026-10-30
linear: TA-29
depends_on: ["008"]
created: 2026-10-30
updated: 2026-10-30
---

# Plan: Reautenticación para desvincular y para admin (desde spec.md 2026-10-30)

> [intent](./intent.md) · [spec](./spec.md) · ADRs [0005](../../decisions/0005-economia-sin-dinero-real.md), [0007](../../decisions/0007-i18n-desde-cero.md), [0008](../../decisions/0008-autenticacion.md) (D1, D2, D3, D9 y la "Enmienda 2026-10-30: reautenticación para vincular, desvincular y admin", tests 29 a 34), [0012](../../decisions/0012-logs-y-cabeceras-de-seguridad.md) (A4, A8, A9) · Planes previos: [plan-c de 008](../008-autenticacion/plan-c.md), [plan-d de 008](../008-autenticacion/plan-d.md)

**Alcance.** RF1 a RF25 de la spec, en **un solo PR** (rama `ta-29-reauth-unlink`):
- server: la migración, las sesiones, el flujo `reauth`, vincular y desvincular, `RecentAdmin` y la CLI de admin;
- protocol, con `just gen`;
- web: el panel de `/account`, `/login` con los dos códigos nuevos, i18n y Vitest;
- e2e: `account.spec.ts`;
- docs: `docs/architecture.md`, el aviso de privacidad y el README del server.

De la estrategia de pruebas de la spec entran todas las filas, y de la enmienda de ADR-0008, los tests 29 a 34. También entra la nota de security-reviewer en su firma de la spec: un `link` empezado antes de un `unlink` exitoso, cuyo callback llega después, da 303 `auth.reauth_required` y no crea ninguna fila.

**Fuera de alcance.** El período de espera, el aviso de "vinculado hace poco", la recuperación de cuenta y las rutas de admin de TA-13 (intent, "Fuera de alcance"). No se toca `crates/engine`, `crates/engine-wasm`, `crates/sim`, `crates/content`, `packages/renderer` ni el ledger (ningún asiento nuevo). No cambia ningún workflow de CI ni el `justfile`. No se espera ningún crate nuevo. Si `Cargo.lock` cambiara, se corre `just crate-age` **antes de compilar** (ver paso 0).

**Quién implementa.** Los pasos marcados **[frontend]** los implementa frontend-engineer, y el resto, backend-engineer. Los pasos de web empiezan cuando `packages/protocol-ts` está regenerado (paso 2) y la ruta `GET /api/auth/reauth-status` existe (paso 8). Hasta entonces, el Vitest se puede escribir contra los tipos generados.

**Revisores del PR.** Además de architect (este plan):
- **security-reviewer:** `risk: high`. Firmó la spec y la enmienda. Revisa en el PR la regla en SQL (decisión 2), el orden de la transacción de `unlink` (decisión 6), el flujo `reauth` (decisión 5), el corte de `grant` (decisión 7), los canarios y las líneas del aviso de privacidad (que también firma como revisor de ese documento).
- **qa-engineer:** la verificación de los mutantes de cada paso, como en 008.
- **frontend-engineer:** revisor de este plan, acotado a los pasos **[frontend]** (respuesta 9 de architect). En el PR revisa las decisiones 13 y 14 y los pasos 2 (su parte web) y 10 a 13.

## Estado real en `main` (ed1e64b)
Lo que este plan encuentra en el código, no solo en los planes de 008:

- **`sessions`** tiene `authenticated_at timestamptz NOT NULL DEFAULT now()` y nada que diga con qué identidad se autenticó. `sessions::issue(tx, user_id, role, ip_hash, ua)` hace un `INSERT` sin `authenticated_at`, así que **toda** sesión nueva sale "recién autenticada", también la rotada por `link`. `max_age_secs(role)` da 30 días o 12 h.
- **El lookup** (`const_format_valid::lookup!`, un solo juego de condiciones para `BY_HASH`, `BY_ID` y `ANY_ID`) calcula `s.authenticated_at > now() - interval '60 minutes'` como `SessionInfo::recently_authenticated`. `SessionInfo` es `Copy`, con `Debug` sin ids. También lo construye a mano un UT de `auth/logout.rs`.
- **`oauth_accounts`** no tiene `id`. Su llave es `(provider, provider_user_id)`, con `UNIQUE (user_id, provider)` y `created_at`. El login nuevo inserta con `ON CONFLICT (provider, provider_user_id) DO NOTHING RETURNING user_id`, y el login existente lee `u.id, u.role, banned` sin la fila de la cuenta.
- **`link::attempt`**: `SELECT role FROM users … FOR UPDATE`, `revoke(One(sesión))` (tiene que devolver exactamente esa), `INSERT … ON CONFLICT DO NOTHING`, `auth_events` y `issue`. **No lee la fila de la sesión.** `LinkGate` está entre el canje y la transacción, inyectado por `Settings.link_gate`.
- **`unlink`**: `session::current()` fuera de la transacción. Dentro de ella: `users … banned_at IS NULL FOR UPDATE`, `DELETE … RETURNING` de la cuenta (404 si no hay), conteo de las habilitadas que quedan (409), `auth_events` y `revoke(OthersOfUser)`. No vuelve a leer la sesión. El éxito es **303 a `/`**. No lee el cuerpo.
- **`RecentAdmin`** (`http/admin.rs`) deja pasar `role == Admin && recently_authenticated`. No hay rutas de admin. `tests/recent_admin.rs` arma su propio `Router`.
- **CLI** (`admin.rs`): `grant` y `revoke` solo cambian `role`. No hay `--help` (`admin grant --help` hoy sale con 2 y `cli.invalid`). **Los logs van a stdout en JSON**, y `tests/admin_cli.rs` y `tests/admin_cli_logs.rs` parsean cada línea de stdout como JSON (`event_of` toma la última).
- **`users_privileged_guard`** es `BEFORE INSERT OR UPDATE OF role, banned_at`. El UT de fuente de `admin.rs` (`source_rules`) reconoce escrituras de `users` por tokens, con `is_privileged` = `role | banned_at`.
- **Rutas**: `POST /api/auth/link/{p}` y el callback llevan la capa `rate::http::redirect_layer` de `AUTH_IP` (10 por minuto, ráfaga 5). `unlink` usa `mutation_ip` y `mutation_user`. `GET /api/me` no tiene cuota de ruta. El UT `no_provider_is_called_like_a_route_word` está en `auth/link.rs`.
- **`CallbackErrorCode`** tiene seis códigos. `AuthErrorCode::ReauthRequired` ya existe (diez códigos). `Provider` vive en `crates/protocol/src/me.rs`.
- **Tests de 008 que la regla nueva rompe** (se adaptan en sus pasos; ninguno cambia lo que prueba):
  - `tests/unlink.rs`: `with_two` entra con Google y vincula Discord. Con la regla nueva, esa sesión ya no es vigente, y todos sus `unlink` darían 403.
  - `tests/link.rs` (`two_link_callbacks_of_one_session_link_once`): usa `support::app::user_with_session`, una sesión sin autenticación, y con RF8 no puede empezar un `link`.
  - `tests/recent_admin.rs`, `tests/me.rs`, `tests/sessions.rs` y `support::app::session_for` llaman a `sessions::issue` con la firma vieja.
  - `tests/admin_cli.rs` (T9: "el login siguiente da una sesión de 12 h y `RecentAdmin` pasa"): sin corte, no pasa.
- **Web.** `/account` (`routes/account/`) lee `locals.user` y `GET /api/auth/providers`. `ProviderList.svelte` pinta un `<form action="/api/auth/link/{p}">` por proveedor no vinculado y un `?/unlink` (form action) por vinculado, deshabilitado si es el último (`providerRows`, `canUnlink`). **`LocaleSwitcher` copia `page.url.search` a sus `href`**, así que un `?reauth=<lo que sea>` en `/account` terminaría en un atributo (el mismo hallazgo de `/login` en plan-d, decisión 7). `/login` con sesión muestra un `Button href` "outline" a la cuenta. No hay tests que rendericen componentes, solo Vitest en `node`.
- **e2e.** `account.spec.ts` 1 entra con `dev`, siembra Google con `seedOauthAccount` y lo desvincula sin más. Con la regla nueva, ese `unlink` da 403. Cada test tiene su `x-e2e-client` y su cubeta de `AUTH_IP` (ráfaga de 5): un login gasta 2 fichas (`start` y `callback`), y una reautenticación, otras 2.

## Para el architect: lo que tienes que decidir
_Resuelto en la revisión del 2026-10-30: architect aceptó las nueve propuestas (ver "Revisión"). Queda como registro, y las respuestas entraron al cuerpo como decisiones tomadas. Las respuestas 2 y 3 están en las decisiones 1 y 6, sin condicional. Las erratas de la spec (el "200" del diagrama y el "mismo texto" de RF25, leído como el mismo contenido) las corrige architect._

1. **El éxito de `unlink` sigue en 303 a `/`.** El diagrama "Flujo" de la spec dice "200", pero RF13 y RF14 no cambian la respuesta de éxito, y la form action de la web ya trata el 303 como éxito (`redirect: 'manual'`). Lo leo como un descuido del diagrama: no cambio el código ni sus tests.
2. **Interbloqueo entre `unlink` (o `link`) y `logout-all`.** RF14 paso 2 y RF9 bloquean la fila de la sesión en uso (`FOR UPDATE`). `logout-all` borra todas las sesiones del usuario sin bloquear `users`. Si `logout-all` corre desde otra sesión S2 mientras `unlink` tiene S1 y pide S2 (`revoke(OthersOfUser)`), cada uno espera una fila del otro. Postgres corta a uno al segundo (`deadlock_detected`): no queda nada a medias, pero uno responde `auth.try_again`. Propongo que `logout-all` tome primero `SELECT 1 FROM users WHERE id = $1 FOR UPDATE`, como `link`, `unlink`, `reauth` y la CLI. Así todas las escrituras de sesiones de un usuario quedan en serie, y la frase de RF14 ("todo `link`, `reauth` y `unlink` del mismo usuario toma el mismo bloqueo") también vale para `logout-all`. No cambia ninguna respuesta. La otra opción es aceptar el interbloqueo raro como `try_again`.
3. **Segunda barrera para `oauth_accounts.id` y `user_id` (opcional).** RF1 pide un test de fuente. Propongo además un trigger `oauth_accounts_identity_guard`, `BEFORE UPDATE OF id, user_id ON oauth_accounts`, que lanza `TA005` con un mensaje fijo y sin excepción (ni siquiera la CLI los actualiza), con `db::error_kind` → `oauth_identity_immutable`. Es el mismo criterio que `users_privileged_guard` en plan-c. No cubre `setval` (eso queda en el test de fuente). Si no lo quieres, sale de la decisión 1 y del paso 1.
4. **`POST /api/auth/reauth/{p}` con un proveedor desconocido.** RF11 dice "un proveedor habilitado del que el usuario tenga una cuenta (si no, 303 a `/login?error=auth.reauth_mismatch`)". Propongo: un texto que no es un `Provider` da **404 JSON**, igual que `start` y `link` (y el T1 de las rutas), y un proveedor conocido pero apagado o no vinculado da el **303 `auth.reauth_mismatch`**. El orden es proveedor conocido, sesión y cuenta, así un `reauth` sin sesión nunca crea un flujo.
5. **El texto de la ayuda de la CLI.** RF25 pide en `docs/architecture.md` "el mismo texto que la ayuda de la CLI". La ayuda es salida de un binario, así que la escribo en inglés (CLAUDE.md: código y texto de código en inglés), y `architecture.md` lleva lo mismo en español, con los dos casos de RF17. No hay test que compare los dos textos. Formas que acepta: `server admin --help` y `server admin <acción> --help`. Las dos imprimen la misma ayuda por stdout y salen con 0, sin leer `DATABASE_URL` ni conectar.
6. **La línea de proveedores de `grant`.** Por stdout, después del commit y **antes** del evento `admin.grant_applied`, una sola línea de texto fijo: `providers within the admin cutoff: google, dev` (o `none`). No es JSON ni pasa por `tracing`. Los tests de la CLI que parsean stdout como JSON pasan a ignorar esa línea (empieza con `providers within`), y `event_of` sigue tomando la última línea, que es el evento.
7. **`ready` de `reauth-status` es solo RF13.** No mira si P es el último proveedor habilitado: eso ya lo muestra la web (`canUnlink`, `account_last_provider_hint`), y el server lo decide en el paso 4 de RF14. Así cada campo dice una sola cosa.
8. **RF9 con la sesión ya cerrada.** Si en la transacción del callback de `link` la fila de la sesión ya no existe (un `logout-all` en el hueco), la respuesta sigue siendo `auth.invalid_state`, como hoy (plan-c, decisión 7). `auth.reauth_required` queda para una sesión viva sin la autenticación que sirve. No cambia lo que prueba `tests/link.rs`.
9. **Revisión de frontend-engineer.** Los pasos **[frontend]** (decisiones 13 y 14, pasos 10 a 12) los escribí con lo que leí en `apps/web` y en plan-d. Propongo que frontend-engineer los confirme en una nota en "Revisión" antes de tu aprobación, o que lo sumes a `reviewers`.

## Decisiones de implementación

### 1. Migración (RF1 a RF5)
`apps/server/migrations/20261030000000_reauth.sql`, nueva. No toca las anteriores (sqlx guarda su checksum). Todo constraint y trigger nuevo tiene nombre fijo, y si lanza, un SQLSTATE y un mensaje fijos sin valores (A9).

- **RF1, `oauth_accounts.id`.** `ADD COLUMN id bigint` nula. `UPDATE` con `row_number() OVER (ORDER BY created_at, provider, provider_user_id)`: el tercer criterio solo desempata dos cuentas de usuarios distintos con la misma fecha y el mismo proveedor, para que el orden no dependa del orden físico. Después `SET NOT NULL`, `ADD GENERATED ALWAYS AS IDENTITY`, `setval(pg_get_serial_sequence('oauth_accounts', 'id'), COALESCE(max(id), 0) + 1, false)` y `ADD CONSTRAINT oauth_accounts_id_uq UNIQUE (id)`. Es el único `setval` permitido (RF1). `ADD COLUMN … GENERATED ALWAYS AS IDENTITY` directo numeraría en el orden físico, y por eso no se usa.
- **`oauth_accounts_identity_guard`** (respuesta 3 de architect), una segunda barrera de RF1 que `GENERATED ALWAYS` no da: `UPDATE oauth_accounts SET id = DEFAULT` está permitido y le daría a una identidad vieja un `id` nuevo.
  - La función `oauth_accounts_identity_guard()` lanza `TA005` con el mensaje fijo `oauth account identity is immutable` (sin valores, A9) si `NEW.id IS DISTINCT FROM OLD.id` o `NEW.user_id IS DISTINCT FROM OLD.user_id`. No tiene excepción: ni la CLI las actualiza.
  - `CREATE TRIGGER oauth_accounts_identity_guard BEFORE UPDATE OF id, user_id ON oauth_accounts FOR EACH ROW`, creado **después** del `UPDATE` que numera las filas, en la misma migración (si no, la numeración misma lanzaría `TA005`).
  - `db::error_kind` mapea `TA005` a `oauth_identity_immutable`.
  - No cubre `setval` ni `ALTER SEQUENCE`: eso queda en el test de fuente (decisión 12).
- **RF2, `sessions`.**
  - `auth_account_id bigint NULL CONSTRAINT sessions_auth_account_fk REFERENCES oauth_accounts (id) ON DELETE SET NULL`.
  - `auth_link_seq bigint NULL`.
  - `CONSTRAINT sessions_auth_ck CHECK (auth_account_id IS NULL OR auth_link_seq IS NOT NULL)` (modelo de datos de la spec).
  - Índice `sessions_auth_account_id_idx (auth_account_id)`, para que el `SET NULL` de un `unlink` no recorra `sessions` entera.
  - Las filas que ya existen quedan con las dos en `NULL` (pregunta 6).
- **RF3, `users.admin_account_cutoff bigint NULL`.** `CREATE OR REPLACE FUNCTION users_privileged_guard()` con las dos reglas nuevas: en `INSERT`, `NEW.admin_account_cutoff IS NOT NULL` lanza `TA004`; en `UPDATE`, `NEW.admin_account_cutoff IS DISTINCT FROM OLD.admin_account_cutoff`, también. Después `DROP TRIGGER` y `CREATE TRIGGER users_privileged_guard BEFORE INSERT OR UPDATE OF role, banned_at, admin_account_cutoff ON users`.
- **RF4.** `oauth_flows_purpose_ck` pasa a `purpose IN ('login', 'link', 'reauth')`, y `oauth_flows_link_ck` a `(purpose IN ('link', 'reauth')) = (link_user_id IS NOT NULL)`. Se reemplazan con `DROP CONSTRAINT` y `ADD CONSTRAINT` con el mismo nombre.
- **RF5.** `auth_events_kind_ck` suma `'reauth'`.

### 2. Sesiones (RF2, RF6, RF7, RF10)
**La regla vive en un solo fragmento de SQL.** `sessions/mod.rs` gana una macro `auth_state!()` que, con alias fijos `s` (la sesión) y `acc` (un `LEFT JOIN LATERAL` sobre las cuentas del usuario), produce tres columnas booleanas siempre no nulas:

```sql
LEFT JOIN LATERAL (
  SELECT max(a.id) AS seq, bool_or(a.id = s.auth_account_id) AS own
  FROM oauth_accounts a WHERE a.user_id = s.user_id
) acc ON true
-- vigente
COALESCE(s.auth_account_id IS NOT NULL AND acc.own AND s.auth_link_seq >= acc.seq, false)
-- step_up = vigente AND s.authenticated_at > now() - interval '10 minutes'
-- recent_admin = u.role = 'admin' AND vigente
--   AND s.authenticated_at > now() - interval '60 minutes'
--   AND COALESCE(s.auth_account_id <= u.admin_account_cutoff, false)
```

- **Sin `COALESCE`,** una sesión anterior a 009 (todo `NULL`) daría un booleano nulo, y `try_get::<bool>` fallaría como un error de base (un 503 en lugar de un 403). Lo prueba el caso (v) de admin.
- **Los mismos tres booleanos** los usan el lookup, la lectura de RF9 (`link`), la de RF14 (`unlink`) y `reauth-status`, todos desde la macro. No hay una segunda copia de la regla.
- **Ventanas.** `STEP_UP_WINDOW` (10 min), `LINK_CALLBACK_WINDOW` (20 min) y `RECENT_ADMIN_WINDOW` (60 min) son literales del SQL (un `concat!` no admite valores). Un UT fija que el SQL contiene exactamente esos `interval` y que las constantes de Rust (`pub const … _MINUTES`), que documentan y usan los tests, coinciden.
- **Lookup (RF7).** `lookup!` suma `step_up` y `recent_admin` y quita `recently_authenticated`. La condición de validez no cambia. `SessionInfo` gana `step_up: bool` y `recent_admin: bool`, y su `Debug` muestra solo `role` y esos dos booleanos.
- **`issue` recibe la autenticación** y devuelve el `Max-Age`:

  ```rust
  pub enum Authentication {
      /// A login or a reauthentication: now(), that account, max(id) of the user (in the INSERT).
      Fresh(OauthAccountId),
      /// A link: copies authenticated_at, expires_at, auth_account_id and auth_link_seq of this
      /// session, which the caller holds FOR UPDATE.
      KeptFrom(SessionId),
      /// No authentication that counts (tests, and the safe side: never step_up nor admin).
      None,
  }
  ```

  - `Fresh` hace `INSERT … VALUES (…, $account, (SELECT max(id) FROM oauth_accounts WHERE user_id = $user))`, con `expires_at = now() + max_age_secs(role)`.
  - `KeptFrom` hace `INSERT … SELECT … FROM sessions WHERE id = $old`, con el `expires_at`, `authenticated_at`, `auth_account_id` y `auth_link_seq` de la vieja. Así los instantes nunca salen de Postgres: el server no tiene la feature `time` de sqlx, y no la sumo por esto.
  - Las dos variantes devuelven `RETURNING id, GREATEST(1, ceil(extract(epoch FROM expires_at - now())))::bigint`, que es el `Max-Age` (RF10).
  - `None` existe para los helpers de test (`support::app::session_for`), que hoy emiten sesiones sin identidad. No da ningún permiso, así que no necesita un chequeo de fuente.
- **`OauthAccountId(i64)`**: un newtype con `Debug` redactado y sin `Display` ni `Serialize`, como `SessionId`. Nunca sale en un cuerpo ni en un log (RNF de logs).
- **Una lectura bloqueante** para RF9 y RF14: `sessions::lock_auth(tx, SessionId) -> Option<LockedAuth>`. Es `SELECT s.user_id, <auth_state>, s.authenticated_at > now() - interval '20 minutes', s.auth_account_id FROM sessions s JOIN users u … LEFT JOIN LATERAL … WHERE s.id = $1 AND <validez del lookup> FOR UPDATE OF s`. `FOR UPDATE OF s` es obligatorio: no se puede bloquear un agregado. `LockedAuth` tiene los booleanos y `account: Option<OauthAccountId>`, con `Debug` redactado. La condición de validez sale del mismo `concat!` que el lookup.
- **Consumir la confirmación (RF14, paso 6):** `sessions::consume_step_up(tx, SessionId)`, que hace `UPDATE sessions SET auth_account_id = NULL WHERE id = $1`. Vive en `sessions/mod.rs` junto a `revoke`. No es un borrado, así que el UT "un solo `DELETE FROM sessions`" no cambia.

### 3. Login (RF6)
- La identidad existente lee también `o.id`. La nueva cambia el `RETURNING user_id` del `INSERT` por `RETURNING id`.
- `issue(…, Authentication::Fresh(account))`. El `max(id)` se lee en el mismo `INSERT`, dentro de la transacción del login, sin bloquear al usuario (RF6). Si un vínculo concurrente se confirma después, la sesión nace no vigente, que es el lado seguro.
- Nada más cambia: la rotación de la cookie previa y el `grant` del primer login quedan igual.

### 4. Vincular (RF8 a RF10)
- **Inicio (RF8).** En `link::handler`, después de `current()` y antes de `begin_flow`: si `!info.step_up`, 303 a `error_location(ReauthRequired)` con `Cache-Control: private, no-store` y **ninguna** `Set-Cookie`. No se usa `error_redirect`, que expira la cookie del flujo: RF8 dice "sin tocar ninguna cookie". Es una función nueva en `callback.rs`, junto a `error_location`, para que cada `Location` al login siga teniendo un solo lugar.
- **Transacción del callback (RF9 y RF10)**, en este orden:
  1. `SELECT role FROM users … banned_at IS NULL FOR UPDATE`, como hoy.
  2. `lock_auth(sesión)`. Sin fila, `auth.invalid_state` (pregunta 8). Si la autenticación no es vigente o tiene más de 20 min, `auth.reauth_required`, y la transacción se deshace.
  3. `issue(…, KeptFrom(sesión))`, que lee la fila que el paso 2 tiene bloqueada.
  4. `revoke(One(sesión))`, que tiene que devolver exactamente esa (se conserva la comparación de plan-c como defensa: con el `FOR UPDATE` del paso 2 ya no puede fallar).
  5. El `INSERT` de la cuenta con `ON CONFLICT DO NOTHING` y la clasificación del choque, como hoy.
  6. `auth_events` de `link`.

  **Cambia el orden de plan-c** (allí la rotación iba antes de emitir): la sesión nueva se inserta antes de borrar la vieja, porque copia sus valores. La garantía de plan-c (que la comparación no quede tapada por el choque del `INSERT` de la cuenta) se conserva: los pasos 2 y 4 van antes del paso 5. Un error en cualquier paso deshace todo, también la sesión nueva.
- **La nota de security-reviewer** (spec, ronda 2): un `link` empezado con la confirmación, después un `unlink` exitoso (que la consume) y después el callback. El paso 2 ve `auth_account_id = NULL`, así que la autenticación no es vigente: 303 `auth.reauth_required`, sin fila en `oauth_accounts`, sin `auth_events` de `link` y con la sesión en uso intacta.

### 5. Reautenticarse (RF11 y RF12)
- **`Purpose::Reauth(Uuid)`** en `start.rs`, con `as_str() = "reauth"` y `link_user_id() = Some(user)`. `FlowPurpose::Reauth(Uuid)` en `callback.rs`, en el `match` exhaustivo de `purpose_of`: `("reauth", Some(user))`. Un `reauth` sin usuario o un `login` con usuario siguen dando `None` (`auth.invalid_state`).
- **`auth/reauth.rs`** (nuevo), con el handler y la transacción.
  - **Inicio:** `Result<Form<StartForm>, FormRejection>` como `link`. Primero `Provider::parse` (si no, 404; pregunta 4). Después `current()`: sin sesión, el mismo `to_login` de `link` (303 a `/login` con la cookie expirada); con la BD caída, `try_again_redirect`. Si el proveedor está apagado o el usuario no tiene una cuenta de ese proveedor (`SELECT 1 FROM oauth_accounts WHERE user_id = $1 AND provider = $2`), 303 a `error_location(ReauthMismatch)`, sin flujo ni cookies. Si todo está bien, `begin_flow(…, Purpose::Reauth(user))`.
  - **Callback**, la rama `Reauth(user)`:
    1. La sesión del callback tiene que ser válida y del usuario del flujo (si no, `auth.invalid_state`), como `link`.
    2. El canje con el proveedor y `state.auth.link_gate.0.before_link().await`. Se reutiliza el gate de `link`, y su doc comment pasa a decir "link o reauth".
    3. `reauth::reauth(state, ReauthRequest { user, session, identity, ip_hash, ua })`, en una transacción:
       - el bloqueo de `users` (`banned_at IS NULL`, si no, `auth.invalid_state`);
       - `revoke(One(sesión))`, que tiene que devolver exactamente esa (si no, `auth.invalid_state`);
       - `SELECT id, user_id FROM oauth_accounts WHERE provider = $1 AND provider_user_id = $2`. Sin fila, o con otro `user_id`: `auth.reauth_mismatch`, y la transacción se deshace (la sesión en uso queda como estaba, y nada se crea);
       - `auth_events` (`reauth`, `provider`, `ip_hash`);
       - `issue(…, Fresh(id))`, con el rol leído bajo el bloqueo (30 días o 12 h).
  - **Errores:** un error de base escribe `auth.reauth_failed` (`warn`, solo `event` y `error_kind`) y responde `auth.try_again`. Todo error pasa por `error_redirect`, que expira la cookie del flujo.
- **El `next`** sigue la regla de D2 (`next::sanitize`). `/{locale}/account?reauth=1` pasa tal cual: la regla acepta `?` y `=`.

### 6. Desvincular (RF13 a RF15)
- **`UnlinkGate`** en `auth/unlink.rs`, con el molde de `LinkGate`: un trait, `NoUnlinkGate` y `UnlinkGateConfig` en `Settings.unlink_gate`, solo por código de test. Se llama entre `current()` (y las cuotas) y `state.db.begin()`.
- **La transacción**, en el orden de RF14:
  1. `SELECT 1 FROM users WHERE id = $1 AND banned_at IS NULL FOR UPDATE` (sin fila: 401, como hoy).
  2. `lock_auth(info.id)`. Sin fila: **401 `auth.unauthenticated`** con la cookie expirada (`session::unauthenticated`), sin hacer nada más. Si la fila es de otro usuario (no debería pasar), lo mismo.
  3. `SELECT id FROM oauth_accounts WHERE user_id = $1 AND provider = $2` (sin fila: 404).
  4. `SELECT count(*) … AND provider = ANY($enabled) AND id <> $p` (0: 409 `auth.last_provider`). Va antes de la regla (RF14, paso 4).
  5. La regla, en una función pura `unlink_allowed(&LockedAuth, p: OauthAccountId) -> bool` = `step_up && account.is_some_and(|a| a <= p)`, con su UT de tabla (con `account` en `None`, igual, menor y mayor, y con `step_up` falso). Si no se cumple: **403 `auth.reauth_required`**, `private, no-store`.
  6. `DELETE FROM oauth_accounts WHERE id = $p RETURNING id` (tiene que devolver la fila), `auth_events` de `unlink`, `revoke(OthersOfUser)`, `consume_step_up(info.id)` y `COMMIT`.
- **El éxito no cambia:** 303 a `/` con `private, no-store` (pregunta 1).
- **`logout-all` toma el bloqueo de `users`** (respuesta 2 de architect). Con 009, `unlink` tiene S1 (`FOR UPDATE`) y pide S2 (`revoke(OthersOfUser)` y el `SET NULL` de la FK), y un `logout-all` desde S2 podía tener S2 y pedir S1: un interbloqueo. Ahora `logout-all` hace `SELECT 1 FROM users WHERE id = $1 FOR UPDATE` como primera sentencia de la transacción que ya tiene, **sin** `banned_at IS NULL`, porque un usuario baneado también tiene que poder cerrar todo. Así queda en serie con `link`, `reauth`, `unlink` y la CLI. `logout` de una sola sesión no lo toma: tiene una sola fila y no forma un ciclo. No cambia ninguna respuesta.
- **Un login con X que se cruza con el `unlink` de X** (sugerencia 5 de architect). Hoy esa carrera podía crear una sesión de la víctima después del `unlink`. Con la FK de `sessions.auth_account_id`, el `INSERT` de la sesión del login espera el `DELETE` de X y, cuando este se confirma, falla con `23503`. `LoginError::from(sqlx::Error)` ya lo vuelve `auth.try_again` (con `auth.login_failed` y su `error_kind`), y la transacción del login se deshace sin sesión. `db::error_kind` no tiene ese caso hoy, y lo suma: `23503` → `foreign_key`. Nunca termina en una sesión.
- **Concurrencia.** Dos `unlink` de la misma sesión (Google y Discord) quedan en serie por el bloqueo de `users`. El segundo ve, en el paso 4, que no queda nada habilitado aparte de P: 409. Es el resultado que ya espera `two_unlinks_at_once_leave_one_provider` (un 303 y un 409).

### 7. `RecentAdmin` y la CLI (RF16 y RF17)
- **`RecentAdmin`** deja pasar `info.recent_admin` (el rol ya va dentro del booleano). Se actualiza su doc comment. Todo lo demás de RF-C7 queda igual.
- **`grant`**: `UPDATE users SET role = 'admin', admin_account_cutoff = (SELECT max(id) FROM oauth_accounts WHERE user_id = $1) WHERE id = $1`. **`revoke`**: `role = 'user', admin_account_cutoff = NULL`. Los dos en `admin.rs`, la única fuente que escribe columnas privilegiadas.
- **La línea de proveedores** (pregunta 6): dentro de la transacción, `SELECT provider FROM oauth_accounts WHERE user_id = $1 AND id <= (SELECT admin_account_cutoff FROM users WHERE id = $1) ORDER BY id`. Después del commit, `execute` la escribe por stdout con `writeln!(std::io::stdout().lock(), …)` (nunca `tracing`). Un error de escritura no cambia el resultado (el commit ya se hizo). Solo en `grant`.
- **`--help`** (pregunta 5): `Command::AdminHelp` en `cli.rs` para `admin --help` y `admin <acción> --help`, que imprime `admin::HELP` por stdout y sale con 0, sin config ni conexión. `HELP` dice:
  - los cuatro subcomandos;
  - **cuándo volver a correr `grant`**: después de migrar a 009 (ningún admin tiene corte), y después de vincular un proveedor nuevo como admin (no da admin hasta otro `grant`);
  - que antes de correrlo hay que revisar los proveedores en `/account`, porque todo lo vinculado hasta ese momento pasa a dar admin;
  - y que si `grant` muestra un proveedor que no se reconoce, hay que correr `revoke`, quitarlo desde `/account` y volver a correr `grant`.
- **`apps/server/README.md`**, en la sección "Admin y ban por CLI", lleva los dos casos y el aviso de la línea de proveedores.

### 8. `GET /api/auth/reauth-status` (RF18)
- **`auth/reauth_status.rs`** (nuevo). `Effect::Read`, sin cuota de ruta, como `GET /api/me`. Sin sesión, `session::unauthenticated` (401 con la cookie expirada). Con la BD caída, `session::try_again`.
- **Una consulta:** `SELECT a.provider, a.id, <step_up>, s.auth_account_id FROM sessions s JOIN users u … LEFT JOIN LATERAL … JOIN oauth_accounts a ON a.user_id = s.user_id WHERE s.id = $1`. El orden de las filas no importa.
- **En Rust:**
  - `link_ready = step_up`;
  - por cada cuenta P, `ready = unlink_allowed(…, P.id)` (la misma función de la decisión 6);
  - `reauth_with` = los proveedores **habilitados** del usuario con `id <= P.id`, en el orden de `AuthState::providers`.
  - Los ids nunca salen de la función.
- **Respuesta:** 200 con `ReauthStatus` y `private, no-store`. Es una lectura: la regla se vuelve a evaluar en cada transacción.

### 9. Rutas y cuotas
- **El manifiesto** (`routes.rs`) suma:
  - `POST /api/auth/reauth/{provider}` (`Handler::AuthReauth`, `Mutate`, cuerpo de 4 KB y `auth_ip(keep)`, como `link`);
  - `GET /api/auth/reauth-status` (`Handler::AuthReauthStatus`, `Read`).

  `SAFE_METHOD_EXCEPTIONS` no cambia.
- **Solapamientos con `matchit`:**
  - `/api/auth/reauth/start` cae en `reauth` con el proveedor `start` y da 404;
  - `reauth-status` es un segmento estático, al lado de `providers`.

  `no_provider_is_called_like_a_route_word` suma `reauth` y `reauth-status`. `tests/routes.rs` (T1) suma las dos rutas, el `POST` sin `Origin` (403) y `POST /api/auth/reauth/start` (404).

### 10. Protocol (RF19)
- **`AuthErrorCode::ReauthMismatch`** (`auth.reauth_mismatch`), con su índice en el test de conjuntos (`AUTH_COUNT` = 11). El comentario de `ReauthRequired` pasa al texto de RF19.
- **`CallbackErrorCode`** suma `ReauthRequired` y `ReauthMismatch` (`CALLBACK_COUNT` = 8). Se extienden `ALL`, `from_callback` y el test de que cada código de callback es un código de auth con la misma cadena.
- **`ReauthStatus { link_ready: bool, unlink: Vec<UnlinkStatus> }`** y **`UnlinkStatus { provider: Provider, ready: bool, reauth_with: Vec<Provider> }`**, en `me.rs` (donde vive `Provider`), con `ts(export)`, `Serialize` y `deny_unknown_fields` en `Deserialize`. Un test fija la forma exacta del JSON y el round-trip.
- **`just gen`** y se commitea `packages/protocol-ts`. `codes.ts` y `error-text.ts` dejan de compilar hasta que la web suma sus entradas (es lo que quiere el tipo mapeado), así que el commit del paso 2 suma también las tres entradas en `codes.ts`, `error-text.ts` y `messages/{en,es}.json`. Son de **[frontend]**, pero viajan en ese commit para que `svelte-check` siga en verde (ver el paso 2).

### 11. Logs y canarios (ADR-0012)
- **Un solo evento nuevo:** `auth.reauth_failed` (`warn`, `event` y `error_kind`). Ningún campo nuevo en `ALLOWED_FIELDS`.
- **`tests/logs_account.rs`** suma al flujo, en los tres niveles de `RUST_LOG`:
  - un `link` rechazado por RF8;
  - un `reauth` exitoso y uno con `auth.reauth_mismatch` (la identidad de otro usuario);
  - un `unlink` rechazado (403) y uno exitoso;
  - `reauth-status`.

  Los canarios son los de la spec: `provider_user_id`, `user_id`, `code`, `state`, `code_verifier`, la cookie del flujo y la de sesión, `ip_hash` (hex y base64) y la IP (`127.0.0.1` no es canario, como en la desviación 14 de plan-c).
- **Los ids de `oauth_accounts`** son enteros chicos (`1`, `2`) y no sirven de canario: aparecerían en cualquier línea. Los cubre el UT de `Debug` de `OauthAccountId`, `LockedAuth`, `SessionInfo`, `Issued` y `ReauthStatus`, que comprueba que ninguno imprime el valor.
- **Un mutante por camino nuevo**, comprobado y no commiteado:
  - `auth.reauth_failed` con el `sub` en un campo de texto;
  - un evento con el `user_id` en la rama `reauth_mismatch`;
  - el `Debug` de `LockedAuth` sin redactar en un `error_kind` de `unlink`.

  Cada uno tiene que hacer fallar el test (el camino que filtra es `record_str` en `error_kind`, desviación 13 de plan-c).
- **`tests/admin_cli_logs.rs`** suma `grant` con dos proveedores: ni el `user_id` ni los `provider_user_id` aparecen en stdout ni en stderr, y la línea de proveedores solo tiene nombres.

### 12. Tests de fuente (RF1 y RF3)
- **El normalizador de `admin.rs` (`source_rules`) se generaliza** a `writes(tokens, table, columns)`. Hoy fija `users` y `role | banned_at`. No se mueve de archivo, para no tocar más de lo necesario.
- **RF3.** `is_privileged` suma `admin_account_cutoff`, con su forma en el UT del normalizador. "`admin.rs` todavía las escribe" exige también esta columna.
- **RF1** (`only_migrations_up_to_009_renumber_oauth_accounts`), sobre cada `.rs` de `src/` y cada `.sql` de `migrations/` con versión mayor que `20261030000000`. Falla si encuentra:
  - un `update` de `oauth_accounts` que escribe `id` o `user_id` (también con alias, esquema y comillas, como el de `users`), o un `on conflict … do update set` con esas columnas;
  - el token `overriding` seguido de `system`;
  - `setval`;
  - `alter sequence` (cualquier secuencia: el server no tiene por qué tocar una);
  - `alter table [only] [<esquema>.]oauth_accounts alter [column] id`, que cubre `RESTART`, `SET GENERATED BY DEFAULT` y `DROP IDENTITY`.

  Además falla si la migración de 009 deja de existir o deja de contener el `setval`, para que el test no pase en vacío. **Mutantes**, uno por forma, cada uno tiene que hacerlo fallar:
  - `UPDATE oauth_accounts SET user_id = $1` en `auth/link.rs`;
  - `INSERT … OVERRIDING SYSTEM VALUE` en `auth/login.rs`;
  - `SELECT setval('oauth_accounts_id_seq', 1)` en `auth/unlink.rs`;
  - una migración de prueba `20991231000000_x.sql` con `ALTER TABLE oauth_accounts ALTER COLUMN id RESTART WITH 1` (se borra después: sqlx la aplicaría).

### 13. Web **[frontend]** (RF20 a RF24)
- **`lib/server/api/reads.ts`:** `getReauthStatus(api)`, `GET /api/auth/reauth-status`.
- **`routes/account/+page.server.ts`:**
  - **Canonicaliza la query, solo en un `GET`.** Con `request.method === 'GET'`, si `url.search` no es `''` ni exactamente `?reauth=1`, 303 a `/{locale}/account` (más `?reauth=1` si `reauth` estaba presente). Así ningún valor de la URL llega al `LocaleSwitcher` ni a la página (RF23: solo se lee la presencia). Es el mismo patrón que `/login` (plan-d, decisión 7).
    - **En un `POST` no se canonicaliza** (cambio pedido 1 de architect y punto 1 de frontend-engineer). Sin JS, SvelteKit corre la action y después el `load` con la misma URL (`?/unlink` o `?/rename`). Un 303 ahí tiraría el `fail()`, y sin JS nunca aparecerían el panel ni el error del renombre. Con `use:enhance` no pasa (`invalidateAll` pide `__data.json` con la URL de la página), así que el e2e no lo vería: lo fija el Vitest del paso 10.
    - En ese `POST`, `reauthDone` es falso (la URL no tiene `reauth`), y la página nunca lee nada más de la query.
  - El `load` suma `reauth: ReauthStatus | null` (`null` si la lectura falla o la forma no valida) y `reauthDone: url.searchParams.has('reauth') && reauth?.link_ready === true`.
  - La action `unlink` no cambia: su 403 llega como `fail(403, { code: 'auth.reauth_required', provider })` por `apiFailure`.
- **`lib/features/account/providers.ts`.** `providerRows(linked, enabled, reauth = null)` suma a cada fila:
  - `reauthWith: Provider[]`;
  - `blocked: boolean` (vinculado, con `reauth` disponible y `reauth_with` vacío: Área 5).

  **`ready` no entra en `ProviderRow`** (punto 10 de frontend-engineer, aceptado por architect): ninguna vista lo usa, porque RF21 solo abre el panel con el 403. RF18 exige que el server lo exponga, no que la web lo use, y la regla sigue en el server. Cambia la firma de `providerRows`, así que se actualizan sus cinco tests y las dos llamadas de `AuthSection.svelte`.

  **`linkPanel(reauth, linked, enabled)`** devuelve los proveedores del panel de vincular (los vinculados y habilitados) solo si se cumplen las dos condiciones: `link_ready` es falso **y** queda al menos un proveedor habilitado sin vincular (punto 2 de frontend-engineer). Si no queda nada que vincular, el panel no aparece. Si `reauth` es `null`, todo se comporta como hoy y la acción decide (UI, "Estados" de la spec). Es lógica de presentación sobre datos del server, sin repetir la regla.
- **`lib/components/account/ReauthPanel.svelte`** (nuevo). Recibe `providers`, `locale`, `idPrefix`, `testid` y `autofocus`.
  - Un título (`<h3 tabindex="-1">`) que recibe el foco cuando el panel aparece por un 403 (RF21). Con `use:enhance`, el foco llega desde un `$effect` después de `update()`. Sin JS, con `autofocus` en el `h3`: el aviso `a11y_autofocus` de Svelte se ignora solo en esa línea, con un comentario que explica por qué.
  - El texto del porqué (`account_reauth_text`). El "texto genérico" del panel sin `reauth-status` es el mismo, sin una llave nueva.
  - Un `<form method="post" action="/api/auth/reauth/{p}">` por proveedor, con `next=/{locale}/account?reauth=1` (texto fijo, nunca de la URL) y un `Button` `w-full sm:w-auto min-h-11` con `account_reauth_with({ provider })`. `provider` es el nombre traducido (`provider_name_*`), nunca el código, y el nombre accesible lo incluye.
  - Para `dev`, el `DevIdentityField` de `/login` (que acepta `id`, con el `idPrefix`), encima de su botón.
  - El contenedor lleva `data-testid={testid}`: `reauth-panel-<proveedor>` bajo una fila, o `reauth-panel-link` en el de vincular.
- **`ProviderList.svelte`** recibe un `idPrefix` (el catálogo monta dos `ProviderList`, y sin el prefijo axe da `duplicate-id-aria`) y el `code` del `fail` (hoy solo recibe la llave):
  - una fila `blocked` no tiene "Quitar" y muestra `account_unlink_disabled_hint` desde el principio, sin botones;
  - una fila con el `fail` de `auth.reauth_required` muestra el `ReauthPanel` debajo de la fila, con su `reauthWith` (o, sin `reauth`, con los vinculados habilitados). En esa fila, el panel **reemplaza** al `provider-error`, para no repetir el mensaje;
  - **un solo panel a la vez** (punto 3 de frontend-engineer, decidido por architect): mientras está abierto el panel de una fila, el de vincular no se muestra, porque es la acción que el usuario acaba de pedir, y coincide con "una sola cosa por ventana" (RF14, paso 6). Si no hay panel de fila y `linkPanel` no está vacío, en lugar de los botones "Vincular" va **un** `ReauthPanel` dentro de la tarjeta.
- **`+page.svelte`:** el `status` de la action (`account_unlinked`, `account_renamed`) tiene prioridad sobre `reauthDone`. Si no, el caso 1 seguiría mostrando "Listo" después de quitar. Sin status de action, con `reauthDone`, el `role="status"` muestra `account_reauth_done`.
- **`/login` (RF24).** `codes.ts` y `error-text.ts` suman `auth.reauth_mismatch` en `ERROR_CODES` y `ERROR_TEXT`, y los dos códigos en `CALLBACK_ERROR_CODES` y `CALLBACK_ERROR_TEXT` (en el paso 2, decisión 10). Con sesión y con `error`, el enlace a la cuenta pasa a ser la **acción principal**: `login_back_to_account`, `variant="default"`, ancho completo a 375 px. Conserva `data-testid="login-account-link"`, que usa `errors.spec.ts` 4. Con sesión y sin `error`, se queda en `outline` con `account_link`. Sin sesión, la página queda como hoy.
- **Llaves nuevas, en `en` y `es`:**
  - `account_reauth_title`, `account_reauth_text`, `account_reauth_with`, `account_reauth_done` y `account_unlink_disabled_hint`;
  - `error_auth_reauth_mismatch`, con el texto de RF24 ("Esa no es la cuenta que vinculaste. Vuelve a intentarlo y elige la misma cuenta de ese proveedor.");
  - `login_back_to_account`;
  - `error_auth_reauth_required` cambia su texto a "Confirma que eres tú para hacer esto." (hoy dice "Vuelve a entrar…", que ya no describe la acción).

  `check-i18n` pasa.
- **`/dev/components`:** `AuthSection.svelte` suma el `ReauthPanel` (con y sin `dev`) y una fila `blocked`, en los dos temas.

### 14. E2E **[frontend]**
- **`e2e/support/db.mjs`:**
  - `ageSession(identity, minutes)`: `UPDATE sessions SET authenticated_at = authenticated_at - make_interval(mins => $2)` sobre las sesiones del usuario de esa identidad `dev`, con `assertE2eTarget` antes de conectar;
  - `seedOauthAccount` no cambia: el `id` lo pone la secuencia.

  Su UT en `tests/e2e-db.test.ts` suma la negativa de la guardia para `ageSession`.
- **`e2e/auth/fixtures.ts`:** `reauth(page, identity, panel)`.
  - Busca **dentro** del `data-testid` del panel que recibe (`reauth-panel-<fila>` o `reauth-panel-link`), llena su `DevIdentityField` y toca su "Confirmar con Dev".
  - Espera `/{locale}/account?reauth=1`.
  - Llama a `declareSessionToken`, porque la cookie rotada es un canario nuevo.
- **El presupuesto de `AUTH_IP`:** la ráfaga es de 5 fichas, con 1 ficha nueva cada 6 s. Un login gasta 2 (`start` y `callback`), y una reautenticación, otras 2. **Cada test gasta como mucho 4 fichas por contexto**: un login y una reautenticación, o dos logins. Si un escenario necesita un tercer ida y vuelta, lo hace en un `newContext(browser, info, '<tag>')`, que tiene su propio `x-e2e-client` y su propia cubeta, y sigue en ese contexto hasta el final.
- **`account.spec.ts`**, en los **tres** proyectos que corren `auth/` (`chromium`, `chromium-mobile` y `webkit-mobile`; `webkit` de escritorio tiene su propio `testMatch`, sin `auth/`, y no se cambia `playwright.config.ts`). En `es` salvo donde se indica:
  1. **(Se adapta el 1 de hoy.)** Login con `dev` y Google sembrado: con eso, la autenticación ya no es vigente. Antes de tocar nada está el panel de vincular (`reauth-panel-link`), porque Discord sigue sin vincular. "Quitar Google" abre `reauth-panel-google` bajo la fila de Google, con su título enfocado y el texto en `es`, y el de vincular desaparece: hay un solo panel y un solo `DevIdentityField` en la página. "Confirmar con Dev" (`reauth(page, identity, 'reauth-panel-google')`) hace el ida y vuelta y termina en `/es/account?reauth=1`, con `account_reauth_done`. "Quitar Google" pasa, y el status muestra `account_unlinked`, no "Listo". En el lugar de "Vincular Google" está `reauth-panel-link`, porque la confirmación se consumió. "Quitar Dev" está deshabilitado con su pista, y un `POST` forzado da el texto de `auth.last_provider`, sin JSON.
  2. **Sesión envejecida:** login con `dev` (2 fichas) y siembra de Google. El segundo login con `dev` (ahora vigente) va en un `newContext(browser, info, 'aged')`, y el resto del caso sigue en ese contexto: `ageSession(identity, 11)`, "Quitar Google" abre el panel, reautenticación (2 fichas más en la cubeta de `aged`) y "Quitar Google" pasa. Ningún contexto pasa de 4 fichas. En `en`.
  3. **Sin panel:** siembra de Google y login otra vez con `dev`. "Quitar Google" pasa sin que aparezca el panel (la letra del criterio 2: "entra con uno y quita el otro").
  4. **Vincular:** login con `dev` y `ageSession(identity, 11)`. El panel ocupa el lugar de "Vincular Google" y no hay `link-google`. Tras la reautenticación con `dev`, aparece `link-google` (nunca se pulsa).
  5. **`auth.reauth_mismatch`**, como **dos tests** (un `for` sobre `es` y `en`, cada uno con un login y una reautenticación: 4 fichas). Login con `dev` y `ageSession(identity, 11)`. "Confirmar con Dev" en `reauth-panel-link` con otra `dev_identity`, declarada con `addDynamicCanary`, termina en `/{locale}/login?error=auth.reauth_mismatch`, con el texto de RF24 y el botón "Volver a mi cuenta" (`login-account-link`). Al tocarlo, se vuelve a `/{locale}/account` con la misma sesión (el alias de antes).
  6. **`?reauth=<canario>`:** `/es/account?reauth=<canario>&x=1` da 303 a `/es/account?reauth=1`, y el canario no está en el HTML, en ningún `href` ni en los logs (canario dinámico en `dynamic-canaries.txt`).
- **`screenshots.spec.ts`** suma `/account` con el panel abierto (375 y 1280 px, en y es, dark y light). **`a11y.spec.ts`** suma la misma vista con axe (AA).

### 15. Documentación (RF25)
- **`docs/architecture.md`:**
  - las tablas (`oauth_accounts.id`, las dos columnas de `sessions`, `users.admin_account_cutoff`, `reauth` en `oauth_flows` y en `auth_events`);
  - `POST /api/auth/reauth/{p}` en "Qué postea el navegador directo a `/api`". Esa lista dice hoy "salvo el 404 de un proveedor desconocido o apagado". Pasa a distinguir `reauth`: ahí solo el desconocido da 404, y uno apagado o no vinculado da 303 `auth.reauth_mismatch` (respuesta 4 de architect);
  - el bloqueo de `users` en `logout-all` (decisión 6);
  - la lectura `GET /api/auth/reauth-status`;
  - en admin, en español, el mismo contenido que la ayuda de la CLI (respuesta 5 de architect): los dos casos de RF17, el aviso de revisar `/account` antes y qué hacer con un proveedor que no se reconoce. Lo revisa architect.
- **`docs/legal/aviso-de-privacidad.md`, sección 3**, con las tres líneas de RF25:
  - en "Sesiones", el proveedor con el que se inició sesión o se confirmó la identidad, y cuándo;
  - en "Historial de eventos de seguridad", "desvinculación de proveedor" y "confirmación de identidad";
  - en "Datos temporales de inicio de sesión", "o confirmas tu identidad".

  La sección 8 no cambia. `updated` del frontmatter. Lo firma security-reviewer en el PR.
- **`apps/server/README.md`:** la decisión 7, en español y con el mismo contenido que la ayuda de la CLI.
- **No los escribe este plan:** la nota en D1, D3 y D9 de ADR-0008 que apunta a la enmienda, y su "Estado: en vigor". Le tocan a architect, como autor del ADR, en el mismo PR.

## Archivos que cambian
| Archivo | Cambio |
|---|---|
| `apps/server/migrations/20261030000000_reauth.sql` | nuevo: RF1 a RF5, el índice de `sessions.auth_account_id` y `oauth_accounts_identity_guard` (`TA005`, `IS DISTINCT FROM`, creado después del `UPDATE` que numera, mensaje fijo; decisión 1) |
| `apps/server/src/sessions/mod.rs`, `sessions/tests.rs` | modificado: `auth_state!`, `step_up` y `recent_admin` en el lookup, `Authentication`, `OauthAccountId`, `issue` con el `Max-Age` de Postgres, `lock_auth`, `LockedAuth` y `consume_step_up` (decisión 2) |
| `apps/server/src/auth/login.rs` | modificado: `o.id`, `RETURNING id` y `Fresh` (decisión 3) |
| `apps/server/src/auth/link.rs` | modificado: RF8, RF9 y RF10, el orden nuevo y las palabras de ruta (decisiones 4 y 9) |
| `apps/server/src/auth/reauth.rs` | nuevo: el inicio y la transacción de `reauth` (decisión 5) |
| `apps/server/src/auth/reauth_status.rs` | nuevo: RF18 (decisión 8) |
| `apps/server/src/auth/{start,callback,mod}.rs` | modificado: `Purpose::Reauth`, la rama `reauth` del callback, el 303 sin cookies de RF8 y `pub mod` (decisiones 4 y 5) |
| `apps/server/src/auth/unlink.rs` | modificado: `UnlinkGate`, el orden de RF14, `unlink_allowed` y el consumo (decisión 6) |
| `apps/server/src/auth/logout.rs` | modificado: el UT que construye `SessionInfo` y el bloqueo de `users` (sin `banned_at IS NULL`) como primera sentencia de `logout-all` (decisión 6) |
| `apps/server/src/app.rs` | modificado: `Settings.unlink_gate` (decisión 6) |
| `apps/server/src/http/admin.rs` | modificado: `recent_admin` (decisión 7) |
| `apps/server/src/http/routes.rs` | modificado: las dos rutas (decisión 9) |
| `apps/server/src/admin.rs`, `apps/server/src/cli.rs` | modificado: el corte, la línea de proveedores, `HELP`, `Command::AdminHelp` y los tests de fuente de RF1 y RF3 (decisiones 7 y 12) |
| `apps/server/src/db.rs` | modificado: `TA005` → `oauth_identity_immutable` (decisión 1) y `23503` → `foreign_key` (decisión 6) |
| `apps/server/README.md` | modificado: cuándo volver a correr `grant` (decisión 7) |
| `crates/protocol/src/{error,me}.rs` | modificado: los códigos, `ReauthStatus` y `UnlinkStatus` (decisión 10) |
| `packages/protocol-ts/src/**` | regenerado con `just gen` |
| `apps/server/tests/support/{oauth,app}.rs` | modificado: `oauth::reauth` (empezar y terminar una reautenticación), `oauth::reauth_status` y un `session_for` con `Authentication::None` |
| `apps/server/tests/{reauth_attack,reauth,reauth_status,migration_reauth}.rs` | nuevos: el ataque (29 y 30), el flujo `reauth` (33), `reauth-status` y la migración sobre datos de 008 |
| `apps/server/tests/{link,unlink,recent_admin,admin_cli,admin_cli_logs,admin_audit,logs_account,routes,me,sessions,logout_db}.rs` | modificado: los casos nuevos, la adaptación de los de 008 a la regla, `TA005` con `SET id = DEFAULT` y `SET user_id` (`admin_audit.rs`), el bloqueo de `logout-all` y el login que se cruza con el `unlink` (paso 6) |
| `apps/web/src/lib/server/api/reads.ts` | modificado **[frontend]**: `getReauthStatus` |
| `apps/web/src/routes/account/{+page.server.ts,+page.svelte}` | modificado **[frontend]**: la canonicalización, `reauth` y `reauthDone` |
| `apps/web/src/routes/login/+page.svelte` | modificado **[frontend]**: "Volver a mi cuenta" como acción principal, con el mismo `data-testid` |
| `apps/web/src/lib/features/account/providers.ts`, `providers.test.ts` | modificado **[frontend]**: `reauthWith`, `blocked` y `linkPanel` (sin `ready`), y la firma nueva en sus cinco tests |
| `apps/web/src/lib/components/account/{ProviderList,ReauthPanel}.svelte` | modificado y nuevo **[frontend]**: `idPrefix`, el `code` del `fail`, un solo panel a la vez y `data-testid` por panel |
| `apps/web/src/lib/features/auth/{codes,error-text}.ts`, `error-text.test.ts` | modificado **[frontend]**: los códigos nuevos (en el commit del paso 2) |
| `apps/web/messages/{en,es}.json` | modificado **[frontend]**: las llaves de la decisión 13 |
| `apps/web/src/routes/dev/components/_sections/AuthSection.svelte` | modificado **[frontend]**: el panel, la fila `blocked`, las dos llamadas de `providerRows` con la firma nueva y un `idPrefix` distinto en cada `ProviderList` |
| `apps/web/tests/routes/account.test.ts` | nuevo **[frontend]**: el `load` (canonicalización solo en `GET`, `reauth` y `reauthDone`) con `fakeEvent({ fetch })` de `tests/support/event.ts`, como `welcome.test.ts` |
| `apps/web/tests/components/{provider-list,login-page}.test.ts` | nuevos **[frontend]**: render SSR con `svelte/server` de la lista y de `/login` con `auth.reauth_mismatch` (paso 11) |
| `apps/web/e2e/support/db.mjs`, `apps/web/tests/e2e-db.test.ts` | modificado **[frontend]**: `ageSession` y su guardia |
| `apps/web/e2e/auth/{account.spec.ts,fixtures.ts}` | modificado **[frontend]**: los casos de la decisión 14 (el 5 en dos tests) y `reauth(page, identity, panel)` |
| `apps/web/e2e/{screenshots,a11y}.spec.ts` | modificado **[frontend]**: `/account` con el panel |
| `docs/architecture.md`, `docs/legal/aviso-de-privacidad.md` | modificado: RF25 (decisión 15) |
| `docs/features/009-reautenticacion-desvincular/plan.md` | este plan |
| `docs/decisions/0008-autenticacion.md` | notas en D1, D3 y D9 y el "Estado" de la enmienda. **No las escribe este plan** (architect) |

No cambian `Cargo.toml`, `Cargo.lock`, `package.json`, `pnpm-lock.yaml`, el `justfile`, `.github/workflows/**`, `infra/**`, `crates/engine*`, `crates/sim`, `crates/content` ni `packages/renderer`.

## Orden de trabajo
Cada paso empieza con sus tests en rojo, y el commit lleva el test y el arreglo juntos. Con un bug encontrado en el camino, primero el test que lo reproduce. Los mutantes se comprueban a mano, se anotan en el PR con la salida en rojo y no se commitean. Commits `feat(<scope>): … (TA-29)`.

0. **Base.** `just check` en verde sobre `ed1e64b` antes de tocar nada, para saber qué falla por nosotros. Si algún paso llegara a cambiar `Cargo.lock` (no está previsto), se para, se corre **`just crate-age` antes de compilar**, se baja lo marcado y se corre `just deny`.

1. **Migración** (decisión 1). En rojo, `tests/migration_reauth.rs`:
   - con `sqlx::migrate!("./migrations").run_to(20261029100000, …)` se siembran datos con la forma de 008: tres cuentas con `created_at` desordenado respecto del orden de inserción, dos con la misma fecha y una sesión. Después se corre `run` completo y se comprueba:
     - los `id` siguen el orden `(created_at, provider, provider_user_id)`;
     - la cuenta siguiente recibe `max + 1`;
     - la sesión queda con las dos columnas en `NULL`;
   - un `INSERT INTO oauth_accounts (id, …)` da error (`GENERATED ALWAYS`);
   - un `UPDATE` de `sessions` que deja `auth_account_id` con `auth_link_seq` nulo viola `sessions_auth_ck`;
   - borrar la cuenta pone `auth_account_id` en `NULL` y deja `auth_link_seq`;
   - `oauth_flows` acepta `reauth` con usuario y lo rechaza sin él (`oauth_flows_link_ck`);
   - `auth_events` acepta `reauth`.

   `tests/admin_audit.rs` suma: un `UPDATE` que **solo** toca `admin_account_cutoff`, sin el helper, da `TA004`; con `privileged`, pasa; un `INSERT` con el corte no nulo, sin el helper, da `TA004`. `UPDATE oauth_accounts SET id = DEFAULT` y `UPDATE oauth_accounts SET user_id = <otro usuario>` dan `TA005`, también dentro de una transacción con el actor `cli` (no hay excepción), y el error no lleva valores. `db::error_kind` da `oauth_identity_immutable`. Un `UPDATE` que no toca esas columnas (por ejemplo, de `created_at`, solo en el test) pasa. `migration_reauth.rs` ya prueba que la numeración de la propia migración no choca con el trigger.

   **Mutantes:**
   - el trigger sin `admin_account_cutoff` en `UPDATE OF` (el caso "solo el corte" pasa en silencio);
   - la numeración con `ADD COLUMN … GENERATED ALWAYS` directo (falla el orden);
   - `oauth_flows_link_ck` sin `reauth`;
   - el trigger `TA005` sin `user_id` en `UPDATE OF` (falla el caso de `SET user_id`);
   - el trigger creado antes del `UPDATE` que numera (la migración falla con `TA005`).

   Después la migración. Commit `feat(server): reauth columns and constraints (TA-29)`.

2. **Protocol** (decisión 10). En rojo: los conjuntos (11 y 8), las cadenas, `from_callback`, la forma de `ReauthStatus` y `UnlinkStatus` y el round-trip. Después los tipos, `just gen` y, **[frontend]** en el mismo commit, las entradas de `codes.ts` y `error-text.ts` con sus llaves en `en` y `es`, más `error-text.test.ts` (los ocho de `/login?error=` y `auth.reauth_mismatch` en `resolveApiError`). `svelte-check` y `check-i18n` en verde. **Mutante:** quitar `ReauthMismatch` de `CallbackErrorCode::ALL` (falla el test de conjuntos). Commit `feat(protocol): reauth codes and status (TA-29)`. **Paralelizable** con el paso 1.

3. **Sesiones** (decisión 2), el paso más riesgoso. En rojo, en `sessions/tests.rs` y `tests/sessions.rs`:
   - `step_up` con `authenticated_at` movido a 9 y a 11 min;
   - `step_up` falso con `auth_account_id` nulo, con una cuenta de otro usuario (sembrada por SQL de test) y con `auth_link_seq` menor que el `max(id)` (una cuenta sembrada después);
   - `step_up` verdadero cuando se borra **otra** cuenta del usuario (el máximo baja);
   - `recent_admin` a 59 y 61 min, con el corte nulo, con `auth_account_id` igual al corte y con uno mayor, y siempre falso para un `user`;
   - una sesión con las dos columnas en `NULL` da los dos booleanos en falso **sin error**;
   - `issue(Fresh)`: `auth_link_seq` es el `max(id)`;
   - `issue(KeptFrom)` copia los cuatro valores y su `Max-Age` es lo que le queda (con `expires_at` movido a 1 h, entre 3598 y 3600), con mínimo 1 (con `expires_at` en 300 ms);
   - `lock_auth` de una sesión vencida o revocada da `None`;
   - el UT de las ventanas y el de `Debug`.

   **Mutantes**, cada uno tiene que hacer fallar su caso:
   - sin `COALESCE` en la vigencia (el caso de `NULL` da error);
   - `>` en lugar de `>=` en `auth_link_seq` (falla el login recién hecho);
   - sin `acc.own` (falla la cuenta de otro usuario);
   - el corte con `<` (falla el caso igual);
   - `KeptFrom` con `now()` en `authenticated_at` (falla la copia).

   Después `sessions/mod.rs`, `login.rs` (decisión 3), las llamadas de `link.rs` (todavía sin la regla), `http/admin.rs` con `recent_admin`, el UT de `logout.rs` y los helpers de test con la firma nueva. La suite completa corre antes del commit. Los tests de 008 que dependían de que `link` renovara la autenticación se adaptan aquí:
   - `tests/recent_admin.rs`: el admin sale de un login real con el Google falso, `grant` en proceso (`server::admin::run`) y otro login;
   - `tests/admin_cli.rs`, T9: lo mismo con el binario.

   Commit `feat(server): sessions know their authentication (TA-29)`.

4. **Vincular** (decisión 4). En rojo, en `tests/link.rs`:
   - **RF8:** con la autenticación movida 11 min atrás, `POST /api/auth/link/discord` da 303 `/login?error=auth.reauth_required`, `private, no-store`, **ninguna `Set-Cookie`** y ninguna fila nueva en `oauth_flows`. Lo mismo con una sesión de `Authentication::None`. **Mutante:** responder con `error_redirect` (pone `Set-Cookie`).
   - **RF9:**
     - un flujo empezado con la confirmación cuyo callback llega con la autenticación movida 21 min atrás: 303 `auth.reauth_required`, sin fila en `oauth_accounts` ni `auth_events`, y la sesión en uso sigue sirviendo;
     - a 19 min, pasa;
     - con una cuenta sembrada por SQL entre el inicio y el callback (la autenticación deja de ser vigente), `auth.reauth_required`.

     **Mutante:** el paso 2 solo con la ventana de 20 min, sin la vigencia.
   - **RF10:** la sesión rotada conserva `authenticated_at`, `expires_at`, `auth_account_id` y `auth_link_seq` (comparados por SQL con la vieja, leída antes); el `Max-Age` de la cookie es lo que le queda (con `expires_at` movido a 1 h); la rotada tiene `step_up = false` enseguida (un segundo `link` da `auth.reauth_required`). En admin, la sesión rotada nunca pasa de 12 h desde el login. **Mutante:** `Fresh` en lugar de `KeptFrom` en la rotación.
   - **Se adapta** `two_link_callbacks_of_one_session_link_once`: el usuario se crea por SQL con una sola fila `dev` en `oauth_accounts` (el `CHECK` la acepta en todos los entornos) y una sesión con `Fresh` de esa fila, y vincula Google y Discord. El resto del test no cambia. Los demás tests de `link.rs` entran con un login real, así que ya tienen la confirmación.
   - El mutante de plan-c (ignorar el resultado de `revoke(One(…))`) ya no hace fallar `a_logout_all_during_the_exchange_wins_over_the_link`, porque ahora lo para el `FOR UPDATE` del paso 2 (que da `auth.invalid_state` sin fila). **Mutante nuevo:** el paso 2 trata "sin fila" como una autenticación que sirve. Tiene que hacer fallar ese mismo test (la respuesta deja de ser `auth.invalid_state`).

   Después `link.rs` y el 303 sin cookies en `callback.rs`. Commit `feat(server): link needs a recent authentication and keeps it (TA-29)`.

5. **Reautenticarse** (decisión 5). En rojo, `tests/reauth.rs`:
   - un `reauth` con Google desde una sesión de Google movida 30 min atrás: 303 a `next` con la cookie nueva, la vieja da 401, `step_up` verdadero, `auth_events` de `reauth` con `provider` e `ip_hash`, y la expiración es nueva (30 días, o 12 h para un admin);
   - **33:** con un flujo `reauth`, el proveedor devuelve la identidad de otro usuario: 303 `auth.reauth_mismatch`, la misma sesión sigue sirviendo, ningún usuario ni cuenta nuevos (conteos antes y después), ningún `auth_events` y la cookie del flujo expirada. Lo mismo con una identidad que no existe, con el mismo código;
   - un flujo `reauth` de A con la cookie de sesión de B, y sin cookie de sesión: `auth.invalid_state`, sin rotar ninguna;
   - el inicio:
     - sin sesión, 303 a `/login` sin flujo;
     - un proveedor que el usuario no tiene, 303 `auth.reauth_mismatch` sin flujo;
     - un proveedor apagado (Discord sin credenciales en loopback), lo mismo;
     - un texto desconocido, 404;
     - un formulario de 4097 bytes, 303 `auth.invalid_state` sin el texto de axum;
   - **login CSRF en el ida y vuelta:** un callback `reauth` con la cookie de otro flujo no cambia de usuario;
   - **la sesión cerrada en el hueco**, con el gate: `logout-all` desde otra sesión durante el canje da `auth.invalid_state`, cero sesiones vivas y ningún `auth_events` de `reauth`.

   **Mutantes**, cada uno tiene que hacer fallar su caso:
   - insertar la identidad si no existe (falla "no existe");
   - aceptar una cuenta de otro usuario (falla 33);
   - ignorar el resultado de `revoke(One)` (falla el del gate);
   - `purpose_of` que acepta `("reauth", None)` (el `CHECK` lo impide en la base, así que este mutante se prueba con un UT de `purpose_of`).

   Después `reauth.rs`, `start.rs`, `callback.rs` y las rutas (decisión 9, con `tests/routes.rs`). Commit `feat(server): reauthenticate without changing user (TA-29)`.

6. **Desvincular** (decisión 6). En rojo, en `tests/unlink.rs` y en el UT de `unlink_allowed`:
   - **Se adapta `with_two`:** login con Google, vincular Discord y reautenticarse con Google (`oauth::reauth`). Con eso, los tests de 008 vuelven a probar lo mismo.
   - **La regla:**
     - autenticado con Google (más antiguo), quitar Discord pasa;
     - autenticado con Discord (`reauth` con Discord), quitar Google da 403 `auth.reauth_required` y quitar Discord pasa;
     - con la autenticación movida 11 min atrás, 403, y tras reautenticarse, pasa;
     - con una sesión `None`, 403;
     - los 403 llevan `private, no-store` y no borran, no escriben `auth_events` ni revocan.
   - **El orden:** con un solo proveedor, 409 aunque la autenticación sea vieja (el paso 4 va antes del 5).
   - **34 y el consumo:** tras un `unlink` exitoso, `auth_account_id` es `NULL`, un `link` da 303 `auth.reauth_required` sin flujo, un segundo `unlink` da 403 y `reauth-status` dice `link_ready = false`. Tras reautenticarse, se puede volver a quitar o a vincular.
   - **El escenario V1, V2 y X** de security-reviewer: con una sesión reciente con V1, quitar V2 pasa y vincular X después falla.
   - **La nota de security-reviewer para el plan:** con Google V1 y Discord V2, una sesión reautenticada con V1 empieza `link/discord` (pasa RF8), después quita Discord (303) y después llega el callback con un Discord X: 303 `auth.reauth_required`, ninguna fila de X en `oauth_accounts`, ningún `auth_events` de `link`, la sesión en uso sigue viva y la cookie del flujo queda expirada. **Mutantes:** quitar `consume_step_up` (el callback vincula X) y el paso 2 de RF9 sin la vigencia (lo mismo). Los dos tienen que hacer fallar el test.
   - **Área 5:** con Google y Discord vinculados y Google apagado por config, autenticado con Discord, quitar Google da 403 (Discord es más nuevo), y `reauth_with` de Google está vacío (en el paso 8).
   - **La carrera** (RF15, test 32), con `UnlinkGate`. La sesión S1 está autenticada con V1 y quiere quitar V2. En la pausa:
     - (a) `logout` de S1: 401;
     - (b) `logout-all` desde S2: 401;
     - (c1) S2 quita V1 por HTTP, lo que revoca S1: 401;
     - (c2) solo con `dev-auth`: con V1, V2 y una cuenta `dev`, el test borra V1 por SQL (la FK deja `auth_account_id` en `NULL` y S1 sigue viva) y S1 quiere quitar la cuenta `dev`: 403;
     - (d) un `link` de S1 que la rota (solo con `dev-auth`, vinculando `dev`): 401;
     - (e) por HTTP, solo con `dev-auth`: S2 vincula `dev`. En las dos pasadas: el test inserta por SQL una cuenta `dev` del usuario (sube el `max(id)`; en la pasada sin `dev-auth`, `dev` no cuenta como habilitada en el paso 4). S1 sigue viva: 403;
     - (f) un `reauth` de S1: 401.

     En todos, la petición en pausa no borra V2, no escribe `auth_events` de `unlink`, no revoca ninguna sesión (se cuentan las filas de `sessions` y los `NOTIFY` con un `LISTEN` propio del test) y no consume la confirmación de S1 si S1 sigue viva.

     **Mutantes**, cada uno tiene que hacer fallar su caso:
     - usar `info` de `current()` en el paso 5 en lugar de `lock_auth` (fallan (c2) y (e));
     - sin el paso 2 (fallan (a), (b), (d) y (f) con 303 o 403);
     - el paso 4 después del 5 (falla el de un solo proveedor).

     Sin `FOR UPDATE OF s`, ningún caso con la pausa **antes** de la transacción falla, porque el cierre ya se confirmó cuando la transacción lee. El bloqueo protege contra un `logout` que llega **durante** la transacción (un `logout` de una sola sesión no toma `users`). Ese caso necesitaría una segunda pausa dentro de la transacción, que no agrego. Lo fija un UT del SQL de `lock_auth` (contiene `FOR UPDATE OF s`), y su mutante es quitarlo.
   - **Se adapta `two_unlinks_at_once_leave_one_provider`:** la misma sesión de `with_two`, con el mismo resultado (un 303 y un 409). El mutante sin el `FOR UPDATE` de `users` tiene que seguir haciéndolo fallar, con las filas de `oauth_accounts` retenidas por el test como hoy. Ahora la petición que entra primero espera en el `DELETE` del paso 6.

   - **`logout-all` en serie** (decisión 6), en `tests/logout_db.rs`: mientras otra transacción del test retiene `SELECT … FROM users … FOR UPDATE`, un `logout-all` espera ese bloqueo (`pg_stat_activity.wait_event_type = 'Lock'`), y al soltarlo responde su 303 y borra todas las sesiones. Con el usuario baneado, también espera y también cierra todo (no hay filtro `banned_at`). **Mutantes:** quitar el bloqueo (el `logout-all` no espera) y agregarle `banned_at IS NULL` (el baneado no cierra sus sesiones).
   - **El login con X que se cruza con el `unlink` de X** (decisión 6, sugerencia 5 de architect), en `tests/unlink.rs`, con el patrón de `tests/banned_writes.rs`. El test abre una transacción que borra la cuenta X y no la confirma. Un login con X lee la identidad (el borrado no está confirmado), y el `INSERT` de su sesión espera el bloqueo de la fila de X. El test espera ese `Lock` en `pg_stat_activity` y confirma. El login termina en 303 `/login?error=auth.try_again`, sin `Set-Cookie` de sesión, sin ninguna sesión nueva del usuario y con `auth.login_failed` y `error_kind = foreign_key`. No hay mutante de código: lo garantiza la FK, y el IT fija la respuesta.

   Después `unlink.rs`, `app.rs` y `logout.rs`. Commit `feat(server): unlink needs a recent authentication not newer than the provider (TA-29)`.

7. **El ataque, la víctima y el admin** (tests 29, 30 y 31). Solo tests: si fallan, el arreglo va en el paso que corresponde.
   - **`tests/reauth_attack.rs`**:
     - **29.** Una víctima con Google y una sesión robada **reciente**. El atacante vincula Discord y quita Google: (a) enseguida; (b) tras logout y login con Discord; (c) tras `logout-all` y login con Discord; (d) tras "esperar 400 días" (`authenticated_at` y `oauth_accounts.created_at` movidos), entrando con Discord y reautenticándose con Discord en medio. En todos: 403, la fila de Google sigue, ningún `auth_events` de `unlink` y un login de la víctima con Google entra a la misma cuenta. **Con la sesión robada vieja**, `link` da 303 `auth.reauth_required` sin flujo ni fila. Entre pasos, `app.refill()` para la cuota.
     - **30.** Tras el paso 1 del ataque, la víctima se reautentica con Google y quita Discord: las sesiones del atacante dan 401, su socket se cierra con `1008 auth.unauthenticated` antes de 5 s, y un login con Discord crea un usuario distinto, sin las monedas ni el alias de la víctima.
     - **Ledger:** en todo el escenario, `ledger_entries` y `ledger_postings` solo tienen el `grant` de los primeros logins (la víctima y, en 30, el usuario nuevo del atacante).
   - **`tests/recent_admin.rs`** (31): admin con Google, `grant`, login.
     - (i) A 61 min, `link` da `auth.reauth_required` y `RecentAdmin` da 403.
     - (ii) Una sesión reciente que vincula Discord: enseguida, tras logout y login con Discord, tras `logout-all` y login con Discord, y tras "esperar" con reautenticación con Discord, `RecentAdmin` da 403 en todos.
     - (iii) Una reautenticación con Google pasa.
     - (iv) Un segundo `grant` hace que Discord pase.
     - (v) Una sesión con las columnas en `NULL` da 403 (no 503).

     El `Router` de prueba se monta sobre el `AppState` de la app con los proveedores falsos (si `support::oauth::App` no lo expone, se le suma un accesor; ver Riesgos). **Mutante:** `RecentAdmin` con `step_up || recent_admin` (falla (ii)).

   Commit `test(server): the TA-29 takeover fails and the victim can remove the attacker (TA-29)`. **Paralelizable** con el paso 8.

8. **`reauth-status`, la CLI y las fuentes** (decisiones 7, 8 y 12). En rojo:
   - **`tests/reauth_status.rs`:**
     - sin sesión, 401 con la cookie expirada;
     - con Google V1 y Discord V2 y autenticado con V1: `link_ready` verdadero, los dos `ready` y `reauth_with` de V2 igual a `["google", "discord"]` (en el orden de los botones);
     - autenticado con V2: `ready` de V1 falso y su `reauth_with` igual a `["google"]`;
     - el Área 5: V1 apagado, `reauth_with` de V1 vacío;
     - tras vincular, `link_ready` falso;
     - el cuerpo no tiene números ni fechas (ninguna clave fuera de la forma);
     - `private, no-store`.
   - **`tests/admin_cli.rs`:**
     - `grant` guarda el `max(id)` como corte e imprime `providers within the admin cutoff: google, discord`, y nada de esa línea llega al JSON de los eventos;
     - un segundo `grant` actualiza el corte;
     - `revoke` lo pone en `NULL`;
     - un usuario sin cuentas da `none`;
     - `admin --help` y `admin grant --help` salen con 0, con los dos casos de RF17 en el texto y sin conectar (un `DATABASE_URL` inválido no cambia nada).
   - **`tests/admin_cli_logs.rs`:** `grant` con dos proveedores, sin `user_id` ni `provider_user_id` en stdout ni en stderr.
   - **Los UT de fuente de RF1 y RF3** con los mutantes de la decisión 12.

   Después `reauth_status.rs`, `admin.rs` y `cli.rs`. Commit `feat(server): reauth status, admin cutoff and source rules (TA-29)`.

9. **Logs** (decisión 11). `tests/logs_account.rs` con el flujo ampliado en los tres niveles, cada camino nuevo con su mutante comprobado, y el UT de `Debug`. Commit `test(server): log canaries for link, reauth and unlink (TA-29)`.

10. **[frontend] Lógica y `load` de `/account`** (decisión 13). En rojo:
    - **`providers.test.ts`**, con la firma nueva en sus cinco tests:
      - `reauthWith` y `blocked` con `reauth` y sin `reauth` (`null` por defecto);
      - `linkPanel` vacío con `link_ready`, con `reauth` en `null` y **cuando no queda ningún proveedor habilitado sin vincular**, aunque `link_ready` sea falso;
      - con `link_ready` falso y algo que vincular, los vinculados habilitados en el orden de la pantalla.
    - **`tests/routes/account.test.ts`**, con `fakeEvent({ fetch })` de `tests/support/event.ts`:
      - sin sesión, al login;
      - en `GET`, `?reauth=1` se conserva;
      - en `GET`, `?reauth=<canario>`, `?reauth=1&x=1`, `?x=1` y `?reauth` dan 303 a la URL canónica, y ninguna `Location` contiene el canario;
      - **en un `POST` a `?/unlink`** (el `load` que corre después de una action sin JS), no hay redirección: el `load` devuelve sus datos, y la action con un 403 del server devuelve su `fail(403, { code: 'auth.reauth_required', provider })`. Lo mismo para `?/rename` con un 409;
      - `reauthDone` solo con `link_ready`, y falso en el `POST`;
      - `reauth` en `null` si la lectura da 503 o una forma inválida.

    **Mutantes**, cada uno tiene que hacer fallar su caso:
    - canonicalizar también en `POST` (falla el caso de `?/unlink`);
    - `reauthDone` sin `link_ready`;
    - conservar la query cruda;
    - `linkPanel` sin la condición de "algo que vincular".

    Después `reads.ts`, `providers.ts` y `+page.server.ts`. Commit `feat(web): reauth status in the account page (TA-29)`.

11. **[frontend] Panel, lista y `/login`** (decisión 13). En rojo, con `render` de `svelte/server` (sin dependencias nuevas):
    - **`tests/components/provider-list.test.ts`:**
      - con `reauth_with` vacío, no hay botón "Quitar" y está `account_unlink_disabled_hint`;
      - con el `fail` de `auth.reauth_required`, `reauth-panel-<fila>` está bajo esa fila, con un `form` por proveedor a `/api/auth/reauth/{p}`, `next=/es/account?reauth=1` y el `DevIdentityField` solo en el de `dev`. En esa fila no está el `provider-error`;
      - **un solo panel:** con ese `fail` y `linkPanel` no vacío, no está `reauth-panel-link`, y hay un solo `DevIdentityField`;
      - con `link_ready` falso y algo que vincular, no hay `link-*` y está `reauth-panel-link`;
      - dos `ProviderList` con `idPrefix` distintos no repiten ningún `id`.
    - **`tests/components/login-page.test.ts`** (cambio pedido 2 de architect): `/login` con sesión y `errorKey` de `auth.reauth_mismatch` muestra el texto de RF24 y `login-account-link` con `login_back_to_account`, `href="/es/account"` y la variante principal. Con sesión y sin error, `account_link` en `outline`. Sin sesión, no está el botón.

    **Mutantes:** mostrar el panel de vincular junto al de la fila, y el botón de `/login` con `account_link` en el caso de error.

    **El respaldo es el mismo para los dos archivos.** Si el render SSR de un componente que importa `$app/forms` (o el `getLocale()` de Paraglide fuera de una petición) no funciona en Vitest, la parte visual queda en el e2e: la lista en los pasos 12 y 13, y el botón de `/login` en el caso 5. El Vitest cubre `providers.ts`, y se anota como desviación. Después `ReauthPanel.svelte`, `ProviderList.svelte`, `+page.svelte`, `/login` y `AuthSection.svelte`. Commit `feat(web): confirm your identity panel in the account (TA-29)`.

12. **[frontend] E2E** (decisión 14). En rojo los casos de `account.spec.ts` (el 5 son dos tests) y `ageSession` (con el UT de su guardia), en los **tres** proyectos que corren `auth/` (`chromium`, `chromium-mobile` y `webkit-mobile`). Ningún contexto de un test gasta más de 4 fichas de `AUTH_IP`. Después, lo que haga falta en web para que pasen. `global-teardown.ts` sin canarios, también la segunda `dev_identity` del caso 5 y la cookie rotada por `reauth`. Commit `test(web): e2e of the reauth round trip (TA-29)`.

13. **[frontend] Capturas y axe.** `screenshots.spec.ts` y `a11y.spec.ts` con el panel. Se revisan contra la UI de la spec: el panel dentro de la tarjeta, botones de ancho completo de 44 px a 375 px, sin scroll horizontal, foco visible y `sm:w-auto` en escritorio. Commit `test(web): screenshots and axe for the reauth panel (TA-29)`.

14. **Documentación** (decisión 15). Commit `docs(auth): reauth in the architecture, the privacy notice and the CLI (TA-29)`. **Paralelizable** desde el paso 8.

15. `just fmt`, `just check` y `just e2e` completos, con la salida pegada en el PR. La lista de mutantes con su resultado en la descripción del PR. `status: done` de este plan al merge.

## Riesgos
- **La regla queda copiada y una copia diverge** (el lookup, `link`, `unlink` y `reauth-status`). → Un solo fragmento de SQL (`auth_state!`) y una sola función de Rust (`unlink_allowed`). Los tests de cada consumidor usan los mismos casos de vigencia.
- **Un booleano nulo se lee como un error de base** (las sesiones de antes de 009) y responde 503 en lugar de 403. → `COALESCE` en cada booleano, con su caso y su mutante en el paso 3 y el caso (v) de admin.
- **La regla se evalúa con datos de antes del bloqueo.** → `unlink` y el callback de `link` leen la sesión con `lock_auth`, bajo el bloqueo de `users`. El caso (e) de la carrera falla si se usa el `SessionInfo` de `current()`.
- **Interbloqueo con `logout-all`** (respuesta 2 de architect). → `logout-all` toma el bloqueo de `users` primero, sin filtro de ban, así que queda en serie con `link`, `reauth`, `unlink` y la CLI. Tiene su IT y dos mutantes en el paso 6.
- **Residuo aceptado: la limpieza de sesiones vencidas** (`RevokeFilter::Expired`, el job de retención, que no bloquea `users`). Solo podría cruzarse con la sesión que `unlink` o `link` tienen bloqueada en el instante en que esa sesión vence, y formar un ciclo con el `revoke(OthersOfUser)` o el `SET NULL` de la FK. → Postgres corta a uno (`deadlock_detected`). Si cae la petición, responde `auth.try_again`; si cae el job, la sesión vencida se borra en su próxima corrida. No queda nada a medias en ningún caso. No se bloquea `users` en el job, porque recorre sesiones de muchos usuarios.
- **Un login con X que se cruza con el `unlink` de X** (sugerencia 5 de architect). Antes de 009 podía crear una sesión de la víctima después de quitarle X. → Lo cierra la FK de `sessions.auth_account_id`: el `INSERT` de la sesión espera el `DELETE` de X y falla con `23503` al confirmarse. El login responde `auth.try_again`, nunca una sesión, y lo fija el IT del paso 6.
- **Un `link` empezado antes de un `unlink` exitoso** usa una sola ventana para quitar y vincular. → RF9 exige la vigencia en la transacción del callback, y `unlink` consume la confirmación. Lo prueba el test de la nota de security-reviewer, con dos mutantes.
- **La rotación al vincular deja de conservar** (o vuelve a dar `now()`). → `KeptFrom` copia en SQL, con su caso y su mutante en el paso 4. La sesión rotada no tiene `step_up` enseguida.
- **Cambia el orden de la transacción de `link` respecto de plan-c.** → La sesión nueva se emite antes de borrar la vieja, con la fila bloqueada. La comparación de `revoke(One)` se conserva, y el mutante nuevo del paso 4 sustituye al de plan-c, que ahora sobrevive porque lo ataja el `FOR UPDATE`.
- **El orden de vínculo se altera a mano** (un `setval`, un `id` explícito o un `UPDATE` de `user_id`). → `GENERATED ALWAYS`, el test de fuente de RF1 con un mutante por forma y el trigger `TA005`, que además cubre `SET id = DEFAULT`.
- **`grant` habilita una identidad del atacante.** → La línea de proveedores por stdout y la ayuda que pide revisar `/account` antes. Es lo que pide la spec, y la convivencia sigue siendo un riesgo del owner.
- **La línea de stdout de `grant` rompe los tests que parsean stdout como JSON.** → Se imprime antes del evento, y los tests la filtran por su prefijo fijo (pregunta 6).
- **Los tests de 008 dejan de probar lo que probaban** al adaptarlos (`with_two`, la sesión de `two_link_callbacks…`, `recent_admin.rs` y T9). → Cada adaptación solo agrega la reautenticación o la fila que la regla exige, sin tocar sus aserciones, y se nombra en su paso.
- **La carrera necesita un tercer proveedor**, que solo existe con `dev-auth` ((c2), (d) y la variante HTTP de (e)). → `just check` corre `cargo test` con y sin la feature, y la variante por SQL de (e), que es el caso clave, corre en las dos pasadas.
- **`recent_admin.rs` no tiene acceso al `AppState` de la app con los proveedores falsos.** → Se suma un accesor a `support::oauth::App`. Si no se puede, se arma la app de los falsos con el mismo `Settings` y se monta el `Router` de prueba encima.
- **La cuota `AUTH_IP` del e2e** (ráfaga de 5 por cubeta, con 1 ficha nueva cada 6 s) corta un test con tres ida y vuelta. → 4 fichas como máximo por contexto (decisión 14). El caso 2 hace su segundo login en un `newContext` propio, y el caso 5 son dos tests.
- **Dos paneles de confirmación a la vez** (dos `DevIdentityField` con la misma etiqueta, y un test que llena el equivocado). → Un solo panel a la vez en `ProviderList`, un `data-testid` por panel que la fixture usa como alcance, y un `idPrefix` para el catálogo, con su caso de Vitest y axe.
- **Una action sin JS pierde su `fail`** si el `load` de `/account` canonicaliza en el `POST`. → Canonicalizar solo en `GET`, con el caso de `?/unlink` en Vitest y su mutante. El e2e no lo vería, porque usa `use:enhance`.
- **Un valor de la URL de `/account` reflejado** por el `LocaleSwitcher` (`?reauth=<canario>`). → La canonicalización con 303, con su Vitest y el caso 6 del e2e.
- **El render SSR de componentes en Vitest** no está probado en el repo. → El respaldo está escrito en el paso 11: la lógica en `providers.ts` y lo visual en el e2e.
- **El owner pierde admin al migrar.** Ningún admin tiene corte después de 009. → Hoy no hay nada desplegado. La ayuda de la CLI, el README y `architecture.md` dicen que hay que volver a correr `grant` (Área 9).
- **Un texto nuevo de `/login` o del panel sin traducir.** → `check-i18n`, los tipos mapeados de `codes.ts` y `error-text.ts`, y el Vitest de llaves en `en` y `es`.

## Alternativas descartadas
- **Calcular la regla en Rust con fechas.** Necesita la feature `time` (o `chrono`) de sqlx y deja el reloj de la app en juego. La spec pide el reloj de Postgres.
- **Copiar `authenticated_at` y `expires_at` como texto** al rotar. Depende de `DateStyle` y del huso de la conexión. `INSERT … SELECT` sobre la fila bloqueada no saca nada de Postgres.
- **Una CTE `DELETE … RETURNING` + `INSERT` para rotar.** Sería un segundo `DELETE FROM sessions` fuera de `revoke`, sin su `NOTIFY` (D3, y el UT de un solo borrado).
- **Un `UnlinkGate` por variable de entorno.** Las pausas de test van solo por `Settings` (plan-c, decisión 7).
- **Que `reauth-status` diga por qué no se puede quitar.** La spec lo prohíbe: ninguna respuesta dice si falló la fecha, el orden o la identidad.
- **Repetir la regla en la web** con `Me.providers` ordenado. Es lógica de dominio en `apps/web` (spec, "Alternativas").
- **Un flujo `reauth` con su propia columna en `oauth_flows`.** RF4 reutiliza `link_user_id`. Una columna nueva duplicaría el `CHECK` sin ganar nada.
- **Mover el normalizador de fuentes a un módulo propio.** Es más ordenado, pero mueve un archivo con sus tres mutantes de plan-c. Se generaliza en el lugar.
- **Imprimir la línea de `grant` como JSON.** Parecería un evento de log, que es justo lo que RF17 prohíbe.

## Pruebas (cómo se demuestra que funciona)
- [ ] `just check` en verde, con la salida pegada en el PR (incluye `cargo test` con y sin `dev-auth`, clippy de las dos variantes, `_check-pg-log`, `svelte-check`, Vitest, `check-i18n` y `_check-gen`).
- [ ] `just e2e` en verde (Chromium, WebKit y los dos proyectos móviles), con la salida pegada en el PR.
- [ ] `Cargo.lock` sin cambios (o `just crate-age` y `just deny` en verde antes de compilar, si cambió).
- [ ] **Criterio 1 y test 29:** `tests/reauth_attack.rs`, con las cuatro variantes y la sesión vieja.
- [ ] **Criterio 2:** `tests/unlink.rs` (la regla, el orden y el Área 5) y los casos 1 a 4 de `account.spec.ts`, en `es` y en `en`, en los tres proyectos que corren `auth/` (`chromium`, `chromium-mobile` y `webkit-mobile`).
- [ ] **Criterio 3 y test 30:** la víctima echa al atacante (sesiones, socket y login con X).
- [ ] **Criterio 4 y test 31:** `tests/recent_admin.rs`, del (i) al (v), y el `grant` con su línea de proveedores.
- [ ] **`logout-all` en serie y el login que se cruza con el `unlink`:** los dos IT del paso 6, con los mutantes de `logout-all`.
- [ ] **Carrera y test 32:** `tests/unlink.rs` con `UnlinkGate`, (a) a (f), con los cuatro mutantes.
- [ ] **Test 33:** `tests/reauth.rs` con `auth.reauth_mismatch` sin cambios en la base.
- [ ] **Test 34 y la nota de security-reviewer:** el consumo, y el `link` empezado antes de un `unlink` cuyo callback da 303 `auth.reauth_required` sin fila, con los dos mutantes.
- [ ] **RF8 a RF10:** `tests/link.rs` (303 sin cookies, la ventana de 20 min y la vigencia, la rotación que conserva y el `Max-Age`), con sus mutantes.
- [ ] **Ledger:** ningún asiento fuera del `grant` del primer login, en el escenario completo.
- [ ] **Canarios:** `tests/logs_account.rs` y `tests/admin_cli_logs.rs`, cada camino nuevo con su mutante, y el UT de `Debug`.
- [ ] **Esquema:** `tests/migration_reauth.rs` sobre datos de 008, `tests/admin_audit.rs` con el corte y `TA005` (`SET id = DEFAULT` y `SET user_id`), y los UT de fuente de RF1 y RF3 con sus mutantes.
- [ ] **Rutas (T1):** `tests/routes.rs` con las dos rutas, el 403 sin `Origin` y el 404 de `reauth/start`.
- [ ] **Protocol:** conjuntos, cadenas, la forma de `ReauthStatus` y `just gen` al día.
- [ ] **Web:** Vitest de `codes.ts`, `error-text.ts`, `providers.ts` (con `linkPanel` vacío si no queda nada que vincular), el `load` de `/account` (canonicalización solo en `GET`, con el `POST` a `?/unlink` que conserva su `fail` y su mutante), la lista (un solo panel, `idPrefix`) y `/login` con `auth.reauth_mismatch` y "Volver a mi cuenta"; casos 5 y 6 del e2e (el texto de `auth.reauth_mismatch` con "Volver a mi cuenta" y `?reauth=` sin reflejar); capturas a 375 y 1280 px, en y es, dark y light, y axe sin violaciones AA.
- [ ] **Docs:** `architecture.md`, el aviso de privacidad (firmado por security-reviewer) y el README, revisados en el PR.

## Estimación de CI
Sin cambios en los workflows.
- **Job `rust`:** suma de 1 a 2 min por los IT nuevos en las dos pasadas (con y sin `dev-auth`). El ataque y la carrera hacen varios logins y vínculos con los proveedores falsos (segundos cada uno), y la CLI se lanza unas 8 veces más.
- **Job `e2e`:** siete tests de `account.spec.ts` (los casos 1 a 6, con el 5 en dos) en los tres proyectos de `auth/`: unos 21 tests de unos 5 s, de los que 3 reemplazan al test 1 de hoy, más las capturas y axe: de 1 a 2 min, que se suman a la ruta crítica `e2e-server` más `e2e` de plan-d. Si el p50 medido supera lo que dejó plan-d, se anota aquí con los números y se aplica la palanca (b) de plan-d (las capturas de 1280 px solo en `chromium`).

## Desviaciones durante la implementación
1. **Paso 1: `tests/startup.rs`.** Fija la lista de versiones de `_sqlx_migrations`, y suma `20261030000000`. No estaba en "Archivos que cambian". No cambia lo que prueba.
2. **Paso 1: la sonda de `oauth_flows_purpose_ck` va sin usuario.** En `tests/migration_reauth.rs`, un flujo con `purpose = 'other'` y un usuario viola primero `oauth_flows_link_ck`. La sonda del `CHECK` de `purpose` va con `link_user_id` nulo, para que el constraint que lo rechaza sea el que se prueba.
3. **Paso 2: el texto nuevo de `error_auth_reauth_required` viaja en el commit del paso 2.** La decisión 13 lo pone con las llaves de **[frontend]**. Lo adelanto porque desde este commit `/login?error=auth.reauth_required` es un código de la lista cerrada, y el texto de 008 ("Vuelve a entrar…") ya no describe la acción. Los textos son los de la decisión 13 (`es`) y su traducción (`en`).
4. **Paso 3: el `UPDATE` de `grant` y `revoke` con el corte se adelanta del paso 8 al 3.** Con `RecentAdmin` sobre `recent_admin`, el T9 de `tests/admin_cli.rs` y el `tests/recent_admin.rs` adaptado (login real, `grant` y otro login) solo pasan si `grant` guarda `admin_account_cutoff`. En el paso 3 entra solo la sentencia de la decisión 7. La línea de proveedores, `--help`, sus tests y los UT de fuente de RF3 quedan en el paso 8.
5. **Paso 3: dónde da `NULL` la regla, y un mutante más.** El plan dice que, sin `COALESCE`, una sesión anterior a 009 (las dos columnas en `NULL`) da un booleano nulo. En SQL no pasa: `s.auth_account_id IS NOT NULL` es falso, y `false AND NULL` es falso. Por eso el mutante "sin `COALESCE` en la vigencia" sobrevivía al caso de `NULL`. La regla da `NULL` en dos casos reales, y los dos están ahora en `a_session_without_an_authentication_is_false_without_an_error`:
   - en `step_up`, un `auth_account_id` que apunta a una cuenta de otro usuario cuando el usuario no tiene ninguna (`bool_or` y `max` sobre nada);
   - en `recent_admin`, la autenticación vigente de un admin con el corte en `NULL` (`<= NULL`).

   Con esos casos, el mutante de `step_up` muere. Sumo el mutante "sin `COALESCE` en `recent_admin`", que también muere. El caso (v) de admin (paso 7) sigue siendo una sesión con todo en `NULL`.
6. **Paso 4: dos tests de 008 más se adaptan.** Solo suman lo que pide la regla, sin tocar sus aserciones:
   - `tests/link.rs`, `a_second_account_of_the_same_provider_is_provider_already_linked`, vinculaba dos veces desde la sesión rotada, que con RF10 no tiene confirmación. Antes del segundo `link` vuelve a entrar con la misma identidad de Google.
   - `tests/oauth_providers.rs`, `the_redirect_to_a_real_provider_is_a_303_to_its_authorize_constant`, empezaba `link` con una sesión sin autenticación. Usa `support::app::user_with_dev_account`, el mismo helper del test de los dos callbacks.

   El 303 sin cookies de RF8 es `callback::code_redirect(code, cache_control)`, y `try_again_redirect` pasa a usarlo. El mutante de plan-c (ignorar el resultado de `revoke(One)`) sobrevive, como prevé el paso 4, y lo reemplaza el mutante "sin fila es una autenticación que sirve".
7. **Paso 5: la ruta `reauth-status` entra en el paso 8, no en el 5.** El paso 5 suma al manifiesto, a `no_provider_is_called_like_a_route_word` y a `tests/routes.rs` solo `POST /api/auth/reauth/{provider}`. `GET /api/auth/reauth-status` y la palabra `reauth-status` van con su handler en el paso 8. El mutante "insertar la identidad si no existe" muere con el caso "no existe" de 33, pero también lo frena `oauth_accounts_user_provider_uq`: el inicio exige una cuenta de ese proveedor, así que el `INSERT` choca con ella.
8. **Paso 6: el caso del usuario baneado en `tests/logout_db.rs` (precisión de architect, ronda 2).** Un usuario ya baneado no llega al bloqueo: `current()` no encuentra su sesión y `decide` cae en `RevokeFilter::Token`. El caso es el ban que se confirma **mientras** `logout-all` espera: el test retiene `users … FOR UPDATE` en una transacción con el actor de la CLI, espera el `Lock` en `pg_stat_activity`, banea en esa transacción y la confirma. `logout-all` responde su 303 y deja cero sesiones. Para que el mutante `banned_at IS NULL` muera, el bloqueo usa `fetch_one`: sin fila, la transacción falla y el `logout-all` responde `auth.try_again`, sin revocar.
9. **Paso 6: `unlink_allowed(step_up, account, target)` en lugar de `unlink_allowed(&LockedAuth, p)`.** `reauth-status` usa la misma función y lee sin bloqueo, así que no tiene un `LockedAuth`. Sigue siendo una sola función, con el mismo UT de tabla.
10. **Paso 6: `two_unlinks_at_once_leave_one_provider` suma una pasada con dos sesiones.** El plan esperaba que la pasada de una sesión siguiera matando al mutante sin el `FOR UPDATE` de `users`, y sobrevivía: el `FOR UPDATE OF s` de `lock_auth` ya pone en serie dos `unlink` de la misma sesión. Con dos sesiones solo los pone en serie el bloqueo de `users`, y ahí el mutante muere (dos 303). La respuesta esperada en esa pasada es `[303, 401]`: el primero revoca la otra sesión (RF14, paso 2).
11. **Paso 6: los casos de la carrera.**
    - (c2) corre en las dos pasadas, no solo con `dev-auth`: la cuenta `dev` se siembra por SQL, y lo que queda en el paso 4 es Discord, habilitado en las dos.
    - (e) tiene las dos variantes del plan, y su variante por HTTP (`dev-auth`) la hace S2.
    - El mutante "usar `info` de `current()` en el paso 5" solo puede cambiar `step_up`, porque `SessionInfo` no tiene la cuenta. Lo matan las dos variantes de (e), no (c2), donde la cuenta `NULL` viene de `lock_auth`. Sigue fijando lo mismo: la vigencia se lee bajo el bloqueo.
12. **Paso 6: `tests/logs_account.rs` se adapta aquí, no en el paso 9.** Su `unlink` final daba 403 con la sesión rotada: suma una reautenticación con Google después de vincular. El resto del flujo ampliado queda en el paso 9.
13. **Paso 6: `error_kind = foreign_key` no se lee del log en el IT del login que se cruza.** `tests/unlink.rs` tiene varios tests, y `test_layout.rs` exige una captura de logs por binario. El IT fija la respuesta (`auth.try_again`, sin sesión) y, con una violación directa de la FK, que `db::error_kind` da `foreign_key`. `LoginError::from` ya escribe ese `error_kind`.
14. **Paso 7: el `Router` de `RecentAdmin` va sobre un `AppState` propio.** `tests/recent_admin.rs` no usa el `AppState` de la app con los proveedores falsos (Riesgos): arma otro con `state_on`, sobre la misma base y la misma política de cookies, que es todo lo que el extractor lee. No hizo falta un accesor en `support::oauth::App`. En el test 30, la víctima tiene una segunda sesión propia y se reautentica desde ella, porque la robada ya la rotó el `link` del atacante. Sumo dos mutantes de control a los del plan, para que el ataque no pase en vacío: `unlink_allowed` sin el orden y `recent_admin` sin comparar el corte. Los dos mueren.
15. **Paso 8: la lectura de `reauth-status` vive en `sessions::auth_view`.** La decisión 8 la pone en `auth/reauth_status.rs`, pero `auth_state!` es privada de `sessions/mod.rs`, y sacarla abriría una segunda copia posible de la regla. `AuthView` tiene `Debug` redactado. La consulta usa `LEFT JOIN oauth_accounts` y las condiciones de validez del lookup: un usuario sin cuentas recibe `unlink: []`, no un 401. `is_help` vive en `admin.rs`. La regla de RF1 no marca el `INSERT` con `user_id` de una cuenta nueva (`writes(tokens, table, inserts)`, `inserts = false`), y el UT de "no en vacío" exige que la migración de 009 conserve su `setval`. Sumo dos mutantes de control: `grant` sin el corte y `reauth_with` sin el orden.
16. **Paso 9: el camino de `auth.reauth_failed` se fuerza con un trigger de test.** Para que el evento corra sin cortar la base, `tests/logs_account.rs` crea un trigger que falla en el `INSERT` de `auth_events` con `kind = 'reauth'` y lo borra después. El evento es además un control positivo del test. El UT de `Debug` de `Issued` queda en `sessions/tests.rs`, y el de `ReauthStatus`, con `AuthView`, en `auth/reauth_status.rs`. Sumo un mutante al UT: el `Debug` de `LockedAuth` sin redactar.
17. **Paso 10: `tests/routes/account.test.ts` usa un `vi.mock` de `$app/env/private`.** La revisión prefería `fakeEvent({ fetch })` sin `vi.mock`, porque `welcome.test.ts` no lo necesita. Pero `/account` importa `internalApi`, y ese módulo valida `src/env.ts` al importarse y no ve `process.env` en Vitest. Solo se reemplaza el env (`INTERNAL_API_URL`); la API sigue respondiendo por `fakeEvent({ fetch })`. También sumo `parseReauthStatus` en `providers.ts`, que la decisión 13 pide sin nombrar ("`null` si […] la forma no valida"), y `+page.server.ts` canonicaliza antes de mirar la sesión, para que tampoco llegue un valor a la página con la sesión `unavailable`.
18. **Paso 11: el render SSR funciona, así que no hace falta el respaldo.** `svelte/server` renderiza `ProviderList` y `/login` en Vitest, con `overwriteGetLocale` para el idioma. Tres precisiones de la decisión 13:
    - `provider-names.ts` (nuevo) tiene los nombres traducidos, que antes vivían en `ProviderList` y ahora usa también `ReauthPanel`;
    - mientras `linkPanel` no esté vacío, los botones "Vincular" no se muestran, tampoco con el panel de una fila abierto: el server los rechazaría, porque la sesión no tiene confirmación;
    - si llega `auth.reauth_required` a una fila con `reauthWith` vacío (solo con un `POST` forzado, porque esa fila no tiene "Quitar"), no se abre un panel sin botones: queda el `provider-error`.

    El test de ids ignora los `bits-*`: bits-ui numera por `render()`, y una página real es un solo render. Sumo tres mutantes a los del plan: el botón de `/login` en `outline` con error, el panel sin `autofocus` y el panel que deja el `provider-error`. Los tres mueren.
19. **Paso 12: una premisa del plan no es cierta en Kit 3.1, y lo que cambia.** La decisión 13 dice que, con `use:enhance`, `invalidateAll` pide `__data.json` con la URL de la página. En Kit 3.1, `enhance` navega a la URL de la action sin su llave (`get_action_location`, `apply_action_navigation`), así que toda action enviada desde `/es/account?reauth=1`, también una que falla, termina en `/es/account` (lo vi en el tráfico de un `rename`). Consecuencias:
    - **La prioridad del `status` de la action sobre `reauthDone` no se puede distinguir.** Cuando llega `form`, `reauthDone` ya es falso: con JS porque la URL perdió `?reauth=1`, y sin JS porque la URL es `?/rename`. Tampoco el caso 1 la prueba: tras quitar, `link_ready` es falso. El mutante "`reauthDone` antes que la action" **sobrevive y es equivalente**. Dejo el orden que pide la decisión 13, que no cuesta nada.
    - **Sumo el caso 4b:** una confirmación que vence estando en `?reauth=1`. Ese 403 llega por la navegación de Kit, y el panel tiene que abrirse igual, con el foco en su título. Pasa en los tres proyectos.
    - El caso 4 suma un renombre y fija que la URL queda en `/es/account`.
20. **Paso 12: el foco con JS.** El mutante "sin el `$effect`" **sobrevivía** al caso 1. Al tocar "Quitar", el botón se deshabilita, el foco cae en `body`, y ahí alcanza el `autofocus` que Svelte aplica en el cliente. El `$effect` solo importa si el foco está en otro elemento, así que el caso 2 envía el formulario con el foco en el campo del nombre, y con eso el mutante muere. Corrijo el comentario de `ReauthPanel`, que decía otra cosa.
21. **Paso 12: otras precisiones de la decisión 14.**
    - **El caso 5 fija la cookie `locale`.** Sin ella, el caso en `es` termina en `/en/login?error=auth.reauth_mismatch` (comprobado). El callback vuelve al `/login` sin prefijo, que elige por perfil, cookie y `Accept-Language`, y entrar por `/es/` no escribe la cookie. Es el comportamiento de 008 para todo código de callback, no algo nuevo: se reporta aparte, no se cambia aquí.
    - **El caso 3 entra otra vez en un `newContext`.** Con sesión, `/login` no muestra los formularios, así que "login otra vez" no se puede hacer en el mismo contexto.
    - **`reflected` pasa de `errors.spec.ts` a `fixtures.ts`,** porque también la usa el caso 6. `errors.spec.ts` no cambia lo que prueba.
    - **Los dos tests de sesión de hoy pasan a ser el 7 y el 8.**
    - **Los casos no se vieron en rojo contra una UI ausente,** porque la UI entró en el paso 11. Para comprobar que pueden fallar corrí mutantes de web: `next` sin `?reauth=1` y el panel de la fila que no abre con el 403. Los dos mueren.

## Revisión

### architect — 2026-10-30 — REQUEST_CHANGES
- Verifiqué:
  - **Cumplimiento con la spec aprobada.** RF1 a RF25 tienen su decisión y su paso, y cada fila de "Estrategia de pruebas" tiene su test. Los tests 29 a 34 de la Enmienda 2026-10-30 de ADR-0008 están en los pasos 7 (29, 30 y 31), 6 (32 y 34) y 5 (33). La nota de security-reviewer de su firma (un `link` empezado antes de un `unlink` exitoso) está en el paso 6, con dos mutantes. Contrasté el "Estado real en `main`" con el código: `unlink.rs` (sesión fuera de la transacción, `DELETE` antes del conteo, éxito 303 a `/`), `sessions::revoke` (un solo lugar para `DELETE FROM sessions`), `logout.rs` (transacción sin bloqueo de `users`), `cli.rs` (sin `--help`), `db.rs` (`TA001` a `TA004`) y `/account` (form actions con `use:enhance`). Coincide.
  - **Límites y ADRs.** La regla vive en un solo fragmento de SQL (`auth_state!`) y en una sola función de Rust (`unlink_allowed`), la usan el lookup, `link`, `unlink` y `reauth-status`, y la web solo pinta lo que trae `reauth-status` (spec, "Alternativas"). Todo instante sale de Postgres (`KeptFrom` es un `INSERT … SELECT`, y el `Max-Age` lo calcula Postgres), sin la feature `time` de sqlx. D3 se respeta: `consume_step_up` y el `SET NULL` de la FK son `UPDATE`, y la CTE `DELETE … RETURNING` está descartada con la razón correcta. ADR-0005: ningún asiento, con su prueba. ADR-0007: solo códigos, con `check-i18n`. ADR-0012: un solo evento nuevo, sin campos nuevos, con canarios y mutantes. No se tocan el motor, el renderer ni el ledger. Sin crates nuevos, con `just crate-age` previsto si cambiara el lock.
  - **Lo implementaría alguien que no vio la conversación.** Nombra cada archivo, el orden con dependencias y paralelismo, los tests en rojo y los mutantes de cada paso, los tests de 008 que se adaptan sin cambiar lo que prueban, y los riesgos con su defensa.
  - **SQL de la decisión 2.** Con `auth_account_id` nulo, `bool_or` da nulo y el `COALESCE` lo vuelve falso. Con un `auth_link_seq` viejo, falso. Con una cuenta de otro usuario, `acc.own` es falso. `FOR UPDATE OF s` con el agregado dentro del `LATERAL` es válido (el agregado está en otro nivel de la consulta), y el caso de `lock_auth` del paso 3 lo confirma al correr.
  - **Una carrera que la FK nueva cierra, y que el plan no nombra.** Hoy un login con X concurrente con el `unlink` de X puede crear una sesión de la víctima después del `unlink`. Con la FK de `sessions.auth_account_id`, el `INSERT` de esa sesión espera el `DELETE` de X y falla al confirmarse. Ver la sugerencia 5.
- **Respuestas a "Para el architect":**
  1. **Éxito de `unlink`: 303 a `/`.** Es el contrato de 008 (`architecture.md`, tabla de rutas) y lo que espera la form action. El "200" del diagrama "Flujo" de la spec es un error mío. RF13 y RF14 no cambian la respuesta de éxito, así que **no hace falta enmienda**: corrijo el diagrama como errata editorial en el mismo PR, anotada en la Revisión de la spec, sin pedir otra firma.
  2. **Interbloqueo con `logout-all`: acepto la propuesta.** El ciclo es real: con 009, `unlink` tiene S1 (`FOR UPDATE`) y pide S2 (`revoke(OthersOfUser)` y el `SET NULL` de la FK), y `logout-all` desde S2 puede tener S2 y pedir S1. `logout-all` toma `SELECT 1 FROM users WHERE id = $1 FOR UPDATE` primero, **sin** el filtro `banned_at IS NULL` (un usuario baneado también tiene que poder cerrar todo), dentro de la transacción que ya tiene. `logout` de una sola sesión no lo necesita: toma una sola fila y no forma ciclo. Queda como residuo aceptado la limpieza de sesiones vencidas (`RevokeFilter::Expired`, sin bloqueo de `users`), que solo podría cruzarse con S1 en el instante en que vence: el peor caso es un `auth.try_again` sin nada a medias. Anótalo en "Riesgos". No cambia la spec: RF14 dice "todo `link`, `reauth` y `unlink`", y esto lo amplía sin contradecirlo.
  3. **Trigger `TA005`: lo acepto.** Ningún código hace hoy `UPDATE oauth_accounts` (lo comprobé en `src`, `tests` y `migrations`), así que no rompe nada, y agrega algo que `GENERATED ALWAYS` no da: `UPDATE oauth_accounts SET id = DEFAULT` está permitido y le da a una identidad vieja un `id` nuevo, con lo que una identidad plantada pasaría a ser "más antigua" que la de la víctima. Condiciones: la función compara con `IS DISTINCT FROM` (como `users_privileged_guard`), el trigger se crea **después** del `UPDATE` que numera las filas en la misma migración, mensaje fijo sin valores (A9), `db::error_kind` → `oauth_identity_immutable`, y `tests/admin_audit.rs` suma `SET id = DEFAULT` además de `SET user_id`. No cambia la spec: es una barrera extra de RF1.
  4. **Proveedor desconocido en `reauth`: de acuerdo.** Un texto que no es un `Provider` no es un proveedor de RF11, y el 404 es el de T1 y de `start` y `link`. Conocido pero apagado o no vinculado: 303 `auth.reauth_mismatch`, como dice RF11. El orden (proveedor, sesión, cuenta) es correcto. No cambia la spec. Ver el cambio pedido 3 para `architecture.md`.
  5. **Ayuda de la CLI en inglés, `architecture.md` en español: de acuerdo.** La ayuda es salida de un binario para el operador, no UI (ADR-0007 no aplica) y va en inglés por CLAUDE.md. El "mismo texto" de RF25 lo leo como **el mismo contenido**: los dos casos de RF17, el aviso de revisar `/account` antes y qué hacer con un proveedor que no se reconoce, en español en `architecture.md` y en el README. Es una aclaración de la letra, no una enmienda: la anoto junto con la errata del punto 1. No pido un test que compare textos en dos idiomas.
  6. **La línea de proveedores de `grant`: de acuerdo.** Texto fijo, fuera de `tracing`, después del commit y antes del evento. La CLI corre por `docker exec`, así que su stdout no entra al flujo de logs del contenedor. Que los tests la filtren por su prefijo fijo está bien.
  7. **`ready` es solo RF13: de acuerdo.** Cada campo dice una sola cosa, y `canUnlink` ya cubre el último proveedor en la web. Que la web combine los dos sin derivar la regla.
  8. **RF9 sin fila de sesión: `auth.invalid_state`.** De acuerdo: mantiene el contrato de 008 (`a_logout_all_during_the_exchange_wins_over_the_link`) y deja `auth.reauth_required` para una sesión viva sin la autenticación que sirve, igual que RF14 separa el 401 del 403. No cambia la spec.
  9. **frontend-engineer: lo sumo a `reviewers`,** acotado a los pasos **[frontend]** (decisiones 13 y 14, pasos 2 en su parte web y 10 a 13). No escribió el plan, así que puede firmarlo, y el plan pasa a `approved` solo cuando firmen los dos.
- Cambios pedidos:
  1. **[Important] La canonicalización de `/account` rompe las form actions sin JS** (decisión 13 y paso 10). Las actions de `/account` se postean a `?/unlink` y `?/rename`. En SvelteKit 3.1, después de una action sin `use:enhance`, el `load` corre con la misma URL, y `url.search` vale `?/unlink`. Con la regla "si `url.search` no es `''` ni `?reauth=1`, 303", el `load` redirige y se pierde el `fail(403, { code: 'auth.reauth_required', provider })`: sin JS el panel nunca aparece, y tampoco el error de un renombre. Con `use:enhance` no pasa (la action responde sin `load`, y `invalidateAll` pide `__data.json` con la URL de la página), así que el e2e no lo vería. *Cambio:* canonicalizar solo en un `GET` que no sea una action (por ejemplo, `request.method === 'GET'`), y sumar al Vitest del paso 10 un `POST` a `?/unlink` con su `fail` que no redirige, con el mutante "canonicalizar también en `POST`".
  2. **[Minor] Vitest del botón de `/login`.** La spec ("Estrategia de pruebas", fila Web) pide en Vitest que `/login?error=auth.reauth_mismatch` con sesión muestre el texto y el botón a `/{locale}/account`. El paso 11 cambia `/login`, pero su lista solo prueba la lista de proveedores, y el botón queda solo en el e2e (caso 5). *Cambio:* sumar ese caso al paso 11, con el mismo respaldo escrito (si el render SSR no funciona, se anota como desviación y queda en el e2e).
  3. **[Minor] `architecture.md`, la excepción del 404.** La lista de "Qué postea el navegador directo a `/api`" dice hoy "salvo el 404 de un proveedor desconocido o apagado". Con la respuesta 4, en `reauth` un proveedor apagado da 303 `auth.reauth_mismatch`. *Cambio:* que la decisión 15 diga que esa línea distingue `reauth` (solo el desconocido da 404).
  4. **[Minor] Las respuestas 2 y 3 en el cuerpo.** Pasar a la decisión 1, a la decisión 6 y a "Archivos que cambian" las condiciones del trigger `TA005` y del bloqueo de `logout-all`, sin el "si se acepta", y anotar el residuo de `RevokeFilter::Expired` en "Riesgos".
- Sugerencia (no bloquea): 5. Nombrar en "Riesgos" la carrera que cierra la FK (login con X concurrente con el `unlink` de X) y cómo responde el login cuando la FK falla (`23503`, que tiene que terminar en `auth.try_again` y nunca en una sesión). Si un IT con el `LinkGate` o un gate de login cuesta poco, súmalo al paso 6; si no, basta la línea.
- Nota mía para el PR: me tocan, como autor de ADR-0008 y de la spec, las notas en D1, D3 y D9, el "Estado: en vigor" de la enmienda, la errata del diagrama (respuesta 1) y la aclaración de RF25 (respuesta 5).

### frontend-engineer — 2026-10-30 — nota (no es aprobación)
Revisé los pasos **[frontend]** (2 en su parte web, 10 a 13, decisiones 13 y 14) contra la spec y contra `apps/web` en `ed1e64b`. Son implementables, con estos cambios:

**Hay que corregir**
1. **La canonicalización rompe las form actions sin JS.** En un `POST` no mejorado a `?/unlink` o `?/rename`, SvelteKit 3.1 corre la action y después el `load` con el mismo `url` (`?/unlink`, ver `runtime/server/page/index.js`). Con la regla de la decisión 13, ese `load` respondería 303 y se perdería el `fail()`, así que no saldría ni el panel ni el error. Hay dos formas de resolverlo: canonicalizar solo con `request.method === 'GET'`, o ignorar las llaves que empiezan con `/`. El Vitest de `account.test.ts` suma un caso: un `POST` a `?/unlink` no redirige. Con `use:enhance` no pasa nada, porque `invalidateAll` pide `__data.json` con la URL de la página.
2. **`linkPanel` tiene que salir vacío si no queda nada que vincular.** Hoy devuelve los vinculados habilitados siempre que `link_ready` sea falso. En el e2e están habilitados Google, Discord y `dev` (`playwright.config.ts`). Hay que sumar la condición "existe al menos un proveedor habilitado no vinculado", con su caso en `providers.test.ts`.
3. **Dos paneles a la vez.** En el caso 1, Discord no está vinculado y la sesión no es vigente, así que el panel de vincular ya está en pantalla cuando el 403 abre el de la fila de Google. Quedan dos `DevIdentityField` con la misma etiqueta y dos "Confirmar con Dev". Propongo dos cambios: `reauth(page, identity, panel)` busca dentro de un `data-testid` del panel (`reauth-panel-<fila>` o `reauth-panel-link`), y `ProviderList` recibe un `idPrefix`, para que el catálogo, que monta dos `ProviderList`, no repita ids (axe `duplicate-id-aria` en "catalog with the auth section"). Otra opción es ocultar el panel de vincular mientras está abierto el de una fila. Lo decido en el paso 11 y lo anoto.
4. **El caso 2 del e2e gasta 6 fichas, no 4.** Son dos logins y una reautenticación, de 2 fichas cada uno, contra una ráfaga de 5 con 1 ficha cada 6 s: el test sería inestable. El segundo login va en un `newContext(browser, info, 'aged')`, que tiene su propia cubeta, y todo el caso sigue en ese contexto. El caso 5 "en `en` y en `es`" son dos tests (un `for` sobre el locale), no dos ida y vuelta en uno. Con eso, ningún test pasa de 4 fichas.
5. **Son tres proyectos de `auth/`, no cuatro.** `webkit` de escritorio tiene `testMatch` sin `auth/*.spec.ts`. Corren `chromium`, `chromium-mobile` y `webkit-mobile`. Hay que corregirlo en la decisión 14, en el paso 12, en "Pruebas" (criterio 2) y en la estimación de CI (unos 18 tests). No propongo sumar WebKit de escritorio: sería tocar `playwright.config.ts`, que no está en la tabla.

**Hay que precisar**
6. **La fixture `reauth`** espera `/{locale}/account?reauth=1` y llama a `declareSessionToken`, porque la cookie rotada es un canario nuevo. En el caso 5, la segunda `dev_identity` se declara con `addDynamicCanary`.
7. **`/login`** conserva `data-testid="login-account-link"`, que usa `errors.spec.ts` 4, y cambia solo el texto (`login_back_to_account`) y la variante. Sin `error`, se queda en `outline` con `account_link`.
8. **`+page.svelte`:** el `status` de la action (`account_unlinked`) tiene prioridad sobre `reauthDone`. Si no, el caso 1 seguiría mostrando "Listo" después de quitar. En la fila con el panel abierto, el panel reemplaza al `provider-error` de `auth.reauth_required`, para no repetir el mensaje. `ProviderList` necesita el `code` del `fail`, o comparar la llave, porque hoy solo recibe `key`.
9. **El "texto genérico"** del panel sin `reauth-status` (spec, "Estados") es el mismo `account_reauth_text`. No hace falta una llave nueva. `account_reauth_with({ provider })` recibe el nombre traducido (`provider_name_*`), nunca el código.
10. **`ready` de cada fila** no lo usa ninguna vista: RF21 solo abre el panel con el 403. Lo dejo fuera de `ProviderRow`, salvo que architect quiera otra cosa, para no dejar un campo muerto.
11. **`providerRows`** cambia de firma. Hay que actualizar sus cinco tests y las dos llamadas de `AuthSection.svelte`, con `reauth` opcional y `null` por defecto.
12. **El Vitest de `account.test.ts`** puede usar `fakeEvent({ fetch })` de `tests/support/event.ts` en lugar de `vi.mock`. `welcome.test.ts` no usa `vi.mock`.
13. **El render SSR de componentes con `svelte/server` en Vitest** no lo probé: `$app/forms` y el `getLocale()` de Paraglide fuera de una petición pueden fallar. El respaldo del paso 11 alcanza.
14. **Foco.** Con `use:enhance`, el título del panel recibe el foco desde un `$effect` después de `update()`. Sin JS, lo recibe con `autofocus` en el `h3` con `tabindex="-1"`, y Svelte avisa (`a11y_autofocus`). Lo ignoro solo en esa línea y explico por qué en un comentario.

Lo demás encaja como está escrito: la CSP `form-action` ya cubre el 303 de `reauth`, `DevIdentityField` acepta `id`, `ageSession` sigue el molde de `seedOauthAccount`, y las capturas y axe del panel caben en el login único de sus tests.

### architect — 2026-10-30 — sobre la nota de frontend-engineer
- Su punto 1 es mi cambio pedido 1: vale cualquiera de las dos formas, con el caso de Vitest y el mutante.
- Sus puntos 2, 4 y 5 son correcciones del plan y entran en esta ronda junto con mis cambios pedidos.
- **Punto 3: un solo panel a la vez.** Mientras está abierto el panel de una fila, el de vincular no se muestra: es la acción que el usuario acaba de pedir, y coincide con "una sola cosa por ventana" (RF14, paso 6). `idPrefix` en `ProviderList` sigue haciendo falta para el catálogo. El `data-testid` por panel también vale.
- **Punto 10: `ready` puede quedar fuera de `ProviderRow`.** RF18 exige que el server lo exponga, no que la web lo use. RF21 solo abre el panel con el 403, y la regla sigue en el server.
- Puntos 6 a 9 y 11 a 14: de acuerdo, como precisiones de los pasos 10 a 12.

### Respuesta del autor — 2026-10-30
- **architect, cambio 1, y frontend-engineer, punto 1 (canonicalización):** la decisión 13 canonicaliza solo con `request.method === 'GET'`. El paso 10 suma un `POST` a `?/unlink` (y a `?/rename`) que conserva su `fail` sin redirigir, con el mutante "canonicalizar también en `POST`". Hay una línea nueva en Riesgos.
- **architect, cambio 2 (Vitest de `/login`):** `tests/components/login-page.test.ts` en el paso 11, con el mismo respaldo escrito (si el render SSR no funciona, queda en el e2e, caso 5, y se anota como desviación).
- **architect, cambio 3 (`architecture.md`):** la decisión 15 dice que la línea de "Qué postea el navegador directo a `/api`" distingue `reauth`: solo el proveedor desconocido da 404, y uno apagado o no vinculado da 303 `auth.reauth_mismatch`.
- **architect, cambio 4 (respuestas 2 y 3 en el cuerpo):**
  - `TA005` está en la decisión 1 sin condicional, con `IS DISTINCT FROM`, creado después del `UPDATE` que numera, con mensaje fijo y `oauth_identity_immutable`. El paso 1 suma `SET id = DEFAULT` y `SET user_id`, y dos mutantes.
  - El bloqueo de `users` en `logout-all`, sin filtro de ban, está en la decisión 6 y en el paso 6, con su IT y dos mutantes.
  - Están en "Archivos que cambian" (`logout.rs`, `db.rs`, la migración y `tests/logout_db.rs`), y Riesgos suma el residuo de `RevokeFilter::Expired`.
- **architect, sugerencia 5:** la decisión 6, Riesgos y un IT barato en el paso 6, con el patrón de `banned_writes.rs` (el borrado de X sin confirmar retiene la fila). El login con X termina en 303 `auth.try_again`, sin sesión, con `error_kind = foreign_key`. `db::error_kind` suma `23503`.
- **frontend-engineer, punto 2:** `linkPanel` sale vacío si no queda ningún proveedor habilitado sin vincular, con su caso y su mutante en el paso 10.
- **frontend-engineer, punto 3, con la decisión de architect:** un solo panel a la vez (el de una fila oculta el de vincular), un `data-testid` por panel como alcance de `reauth(page, identity, panel)` y un `idPrefix` en `ProviderList` para el catálogo, con sus casos en el paso 11.
- **frontend-engineer, punto 4:** el caso 2 hace el segundo login en `newContext(browser, info, 'aged')` y sigue en ese contexto, y el caso 5 son dos tests (un `for` sobre el locale). Ningún contexto pasa de 4 fichas.
- **frontend-engineer, punto 5:** son tres proyectos (`chromium`, `chromium-mobile` y `webkit-mobile`). Corregido en la decisión 14, el paso 12, Pruebas (criterio 2) y la estimación de CI (unos 21 tests, por el caso 5 doble).
- **frontend-engineer, "hay que precisar" (6 a 14):** entraron en las decisiones 13 y 14 y en los pasos 10 a 12:
  - `ready` fuera de `ProviderRow`, como aceptó architect;
  - la fixture espera `?reauth=1` y llama a `declareSessionToken`, y la segunda `dev_identity` va con `addDynamicCanary`;
  - `login-account-link` se conserva;
  - el status de la action tiene prioridad sobre `reauthDone`, y el panel reemplaza al `provider-error` de su fila;
  - el texto genérico es `account_reauth_text`, y `account_reauth_with` recibe el nombre traducido;
  - la firma nueva de `providerRows` en sus cinco tests y en `AuthSection.svelte`;
  - `fakeEvent({ fetch })` en `account.test.ts`;
  - el foco por `$effect` con JS y por `autofocus` sin JS, ignorando `a11y_autofocus` solo en esa línea.

### frontend-engineer — 2026-10-30 — APPROVE (pasos [frontend])
Alcance: decisiones 13 y 14, pasos 10 a 13 y la parte web del paso 2.
- **Verifiqué en el cuerpo del plan, no solo en la respuesta del autor:**
  - **Punto 1:** la decisión 13 canonicaliza solo en `GET`. El paso 10 prueba `POST ?/unlink` y `?/rename` sin redirección y conserva el `fail`, con el mutante "canonicalizar también en `POST`" y una línea en Riesgos. Esto coincide con lo que hace SvelteKit 3.1 (`runtime/server/page/index.js`: action y después `load` con la misma URL).
  - **Punto 2:** `linkPanel` exige que quede algo que vincular, con su caso y su mutante.
  - **Punto 3:** un solo panel a la vez, `data-testid` por panel como alcance de la fixture e `idPrefix` en `ProviderList`. El paso 11 prueba que no se repiten ids y que hay un solo `DevIdentityField`. El caso 1 del e2e lo comprueba con Discord habilitado.
  - **Punto 4:** el caso 2 hace el segundo login en `newContext(…, 'aged')`, y el caso 5 son dos tests. Recontado: ningún contexto pasa de 4 fichas (1: login y reauth; 2: 2 y 4; 3: 4; 4: 4; 5: 4 cada uno; 6: 2).
  - **Punto 5:** tres proyectos en la decisión 14, el paso 12, Pruebas y la estimación de CI (21 tests). No queda "cuatro proyectos" en el archivo.
  - **Puntos 6 a 14:** todos están en las decisiones 13 y 14 y en los pasos 10 a 12: `ready` fuera de `ProviderRow`, los canarios de la fixture, `login-account-link`, la prioridad del status, el panel que reemplaza al `provider-error`, `account_reauth_text`, el nombre traducido, la firma de `providerRows`, `fakeEvent({ fetch })` y el foco.
  - **El Vitest nuevo de `/login`** (cambio 2 de architect) tiene el mismo respaldo escrito que el de la lista.
- **Checklist de frontend:** cero strings literales (todas las llaves nuevas en `en` y `es`, con `check-i18n`), solo tokens, objetivos de 44 px y `sm:w-auto`, foco al título, capturas a 375 y 1280 px en los dos temas y en los dos idiomas, y axe AA. Ningún valor de la URL llega a la página.
- **Cambios pedidos:** ninguno.

### architect — 2026-10-30 — APPROVE (ronda 2)
- Verifiqué solo lo que cambió, en el cuerpo del plan y no solo en la respuesta del autor, contra mis cuatro pedidos, mi sugerencia y los puntos de frontend-engineer que acepté:
  1. **Important (canonicalización): cerrado.** La decisión 13 canonicaliza solo con `request.method === 'GET'`. El paso 10 suma los `POST` a `?/unlink` y `?/rename` que conservan su `fail`, con el mutante "canonicalizar también en `POST`", y Riesgos lo nombra.
  2. **Minor (Vitest de `/login`): cerrado.** `tests/components/login-page.test.ts` en el paso 11, con el respaldo escrito.
  3. **Minor (`architecture.md`): cerrado.** La decisión 15 distingue `reauth`: solo el proveedor desconocido da 404.
  4. **Minor (respuestas 2 y 3 en el cuerpo): cerrado.** `TA005` está en la decisión 1 sin condicional, con `IS DISTINCT FROM`, creado después de la numeración, con mensaje fijo, `oauth_identity_immutable`, los casos `SET id = DEFAULT` y `SET user_id` y dos mutantes. El bloqueo de `logout-all` está en la decisión 6, en el paso 6 y en "Archivos que cambian". Riesgos tiene el residuo de `RevokeFilter::Expired`. La pregunta 3 de "Para el architect" conserva su "opcional", pero es el registro de la pregunta, y la respuesta está en la Revisión.
  5. **Sugerencia (login contra `unlink`): incorporada.** El IT con una transacción sin confirmar y la espera en `pg_stat_activity` fija la respuesta (`auth.try_again`, sin sesión), y `db::error_kind` suma `23503` → `foreign_key`. Que no tenga mutante de código es correcto, porque lo garantiza la FK.
  - Los puntos 2 a 5 de frontend-engineer y sus precisiones 6 a 14 están en las decisiones 13 y 14 y en los pasos 10 a 12, con un solo panel a la vez y `ready` fuera de `ProviderRow`, como decidí. frontend-engineer firmó esos pasos.
- **Precisión para el paso 6, no bloquea** (se aplica al implementar y se anota en "Desviaciones"): el caso de `tests/logout_db.rs` "con el usuario baneado, también espera y también cierra todo" no se puede armar así. Con el usuario ya baneado, `session::current()` no encuentra la sesión, y `decide` (en `logout.rs`) cae en `RevokeFilter::Token`, que no pasa por el bloqueo. Además, el ban ya revocó todas las sesiones (D10). El bloqueo va solo en la rama `AllOfUser`. El caso que distingue el mutante `banned_at IS NULL` es un ban que se confirma **mientras** `logout-all` espera el bloqueo: el test retiene `users … FOR UPDATE`, `logout-all` ya pasó el lookup y espera, el test banea con el helper privilegiado y confirma, y `logout-all` tiene que responder su 303 y dejar cero sesiones. Con el mutante, la fila no aparece, y la respuesta cambia o quedan sesiones.
- Ninguna respuesta de esta ronda cambia la spec. La errata del diagrama ("200" en lugar de 303) y la aclaración de RF25 (el mismo contenido en dos idiomas) las hago yo en el PR, junto con las notas de ADR-0008.
- Con las dos firmas, el plan pasa a `approved`.
