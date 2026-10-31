---
id: "009"
title: Reautenticación para desvincular y para admin
type: spec
status: done
risk: high
author: architect
reviewers: [product-owner, security-reviewer]
approvals:
  - by: security-reviewer
    date: 2026-10-30
  - by: product-owner
    date: 2026-10-30
linear: TA-29
depends_on: ["008"]
created: 2026-10-30
updated: 2026-10-30
---

# Spec: Reautenticación para desvincular y para admin

> Intent: [intent.md](./intent.md) · ADRs: [0005](../../decisions/0005-economia-sin-dinero-real.md), [0007](../../decisions/0007-i18n-desde-cero.md), [0008](../../decisions/0008-autenticacion.md) (con la **"Enmienda 2026-10-30: reautenticación para vincular, desvincular y admin"**, que viaja en el mismo PR que esta spec), [0012](../../decisions/0012-logs-y-cabeceras-de-seguridad.md) · Spec previa: [008](../008-autenticacion/spec.md)

## Resumen
La sesión pasa a saber **con qué identidad exacta se autenticó** (`sessions.auth_account_id`, una fila de `oauth_accounts`) y **qué vínculos existían en ese momento** (`sessions.auth_link_seq`). Con eso, tres operaciones piden una autenticación que sirva:
- **Desvincular P** exige una autenticación de hace menos de 10 min, hecha después del último vínculo de la cuenta, con una identidad que sigue vinculada y que **no es más nueva que P** (P misma o una vinculada antes).
- **Vincular** exige esa misma autenticación reciente y vigente (pregunta 7), y la sesión rotada **conserva** la autenticación anterior y su expiración, así que vincular nunca cuenta como reautenticación.
- **`RecentAdmin`** exige, además de los 60 min de D9, que la identidad de la autenticación sea anterior al `server admin grant` vigente.

Un flujo OAuth nuevo, `purpose = 'reauth'`, atado al usuario de la sesión, permite reautenticarse sin cambiar de usuario. `/account` explica por qué y lleva de ida y vuelta en un toque. Ningún criterio de éxito del intent se relaja, y no hace falta que el owner acepte un riesgo nuevo (ver "Áreas de preocupación").

Cabe en **un solo PR con un solo `plan.md`** (server, protocol, web y e2e).

## Definiciones
Estas palabras se usan con este sentido exacto en los RF:

- **Orden de vínculo.** `oauth_accounts.id` es un `bigint` de una secuencia (`GENERATED ALWAYS AS IDENTITY`). "Vinculada antes que P" significa `id < P.id`. Volver a vincular una identidad borrada crea una fila nueva con un `id` mayor. El orden nunca usa relojes.
- **Autenticación de una sesión.** El trío `authenticated_at`, `auth_account_id` (la fila de `oauth_accounts` con la que se hizo el login o la reautenticación) y `auth_link_seq` (el `max(oauth_accounts.id)` del usuario en ese momento).
- **Autenticación vigente.** `auth_account_id` no es `NULL`, su fila es del mismo usuario y `auth_link_seq ≥ max(id)` de las filas actuales del usuario. Es decir, la identidad sigue vinculada y **no se vinculó nada después** de esa autenticación. Desvincular otra identidad no la invalida por sí solo (el máximo solo baja), pero un `unlink` exitoso **consume** la confirmación de la sesión que lo hizo (RF14, paso 6). Quitar la propia identidad de la autenticación también la invalida (la FK la deja en `NULL`).
- **Autenticación reciente.** `authenticated_at > now() − 10 min`, con el reloj de Postgres (`STEP_UP_WINDOW`).
- **Confirmación** (`step_up`): autenticación vigente **y** reciente.

## Requisitos funcionales

### Esquema (una migración nueva)
- **RF1. `oauth_accounts.id`.** Columna `id bigint GENERATED ALWAYS AS IDENTITY` con `CONSTRAINT oauth_accounts_id_uq UNIQUE`. Las filas que ya existan se numeran en orden de `(created_at, provider)`, y la secuencia sigue por encima del máximo. La llave primaria `(provider, provider_user_id)` no cambia. Fuera de esa migración, ningún SQL actualiza `oauth_accounts.user_id` ni `oauth_accounts.id`, ni fija un `id` a mano: ni `INSERT … OVERRIDING SYSTEM VALUE`, ni `setval` ni `ALTER SEQUENCE … RESTART` sobre su secuencia. Lo comprueba un test de fuente, igual que el de `role`, que recorre `apps/server/src` y las migraciones posteriores a la de esta feature.
- **RF2. `sessions`.** Dos columnas nuevas:
  - `auth_account_id bigint NULL REFERENCES oauth_accounts (id) ON DELETE SET NULL`;
  - `auth_link_seq bigint NULL`.

  Las sesiones que ya existan quedan con las dos en `NULL`: no tienen una autenticación vigente (pregunta 6). Un `SET NULL` de la FK es un `UPDATE`, así que la regla de D3 (solo `sessions::revoke` borra sesiones) no cambia.
- **RF3. `users.admin_account_cutoff bigint NULL`.** Lo escribe solo `server admin grant` (RF17). El trigger `users_privileged_guard` pasa a `BEFORE INSERT OR UPDATE OF role, banned_at, admin_account_cutoff`, para que un `UPDATE` que solo toca el corte también lo dispare. Fuera de la CLI lanza `TA004` ante un `UPDATE` que cambie `admin_account_cutoff` y ante un `INSERT` con `admin_account_cutoff` distinto de `NULL`, además de lo que ya comprueba para `role` y `banned_at`. El test de fuente de `admin.rs` se extiende a esta columna.
- **RF4. `oauth_flows.purpose`** acepta `'reauth'`. `oauth_flows_link_ck` pasa a `(purpose IN ('link', 'reauth')) = (link_user_id IS NOT NULL)`: un flujo `reauth` también lleva el usuario de la sesión que lo empezó. La columna no cambia de nombre.
- **RF5. `auth_events.kind`** acepta `'reauth'`, con `provider` e `ip_hash`, como `login`.
- Todo constraint o trigger nuevo tiene nombre fijo y, si lanza, un SQLSTATE y un mensaje fijos sin valores (ADR-0012 A9).

### Sesiones
- **RF6. Login.** Toda sesión que emite un login (`purpose = 'login'`) guarda `authenticated_at = now()`, el `auth_account_id` de la identidad con la que entró (la fila recién creada si es el primer login) y `auth_link_seq = max(id)` de las filas del usuario, leído dentro de la transacción del login. El login no bloquea al usuario: si un vínculo concurrente se confirma después de esa lectura, la sesión nace sin autenticación vigente, que es el lado seguro.
- **RF7. Lookup.** La consulta única de validez de `sessions/mod.rs` (`BY_HASH`, `BY_ID`, `ANY_ID`) calcula dos booleanos con el reloj de Postgres, y `SessionInfo` los expone (sin `Debug` de valores que identifiquen):
  - `step_up`: la confirmación de las Definiciones;
  - `recent_admin`: reemplaza a `recently_authenticated`. Vale si el usuario es `admin`, la autenticación es vigente, `authenticated_at > now() − 60 min` y `auth_account_id ≤ users.admin_account_cutoff` (con `NULL` en el corte, falso).

  La condición de validez de la sesión (ban, `expires_at`, 7 días de inactividad) no cambia.

### Vincular (pregunta 7 y pregunta 3)
- **RF8. Inicio.** `POST /api/auth/link/{provider}` exige `step_up` en la sesión, después de comprobar que hay sesión y antes de crear el flujo. Si falta, responde **303 a `/login?error=auth.reauth_required`**, con `Cache-Control: private, no-store`, sin crear fila en `oauth_flows` y sin tocar ninguna cookie. El resto de RF-C2 de 008 no cambia.
- **RF9. Transacción del callback.** `link::attempt`, después de bloquear al usuario y **antes** de la rotación, lee la fila de la sesión con `FOR UPDATE` y exige una autenticación vigente con `authenticated_at > now() − 20 min` (`STEP_UP_WINDOW` más los 10 min de vida del flujo, D2: el usuario empezó con una confirmación y tardó en el proveedor). Si no la tiene, la transacción se deshace y el callback responde 303 a `/login?error=auth.reauth_required`: no se vincula nada y la sesión en uso queda intacta.
- **RF10. Rotación que conserva** (pregunta 3). La sesión nueva que emite un vínculo copia de la rotada `authenticated_at`, `expires_at`, `auth_account_id` y `auth_link_seq`. Como el vínculo crea una fila con `id` mayor que `auth_link_seq`, la sesión rotada **pierde la autenticación vigente** en el acto: no puede desvincular, ni volver a vincular, ni pasar `RecentAdmin` hasta reautenticarse. El `Max-Age` de su cookie es lo que le queda hasta `expires_at`, en segundos enteros calculados por Postgres (mínimo 1). Vincular nunca alarga los 30 días ni las 12 h de admin.

### Reautenticarse (pregunta 5)
- **RF11. Inicio.** `POST /api/auth/reauth/{provider}`: formulario (D4), mismo cuerpo que `start` (`next` y, solo para `dev`, `dev_identity`), misma cuota por IP que `start` y `link` (D7, 10/min). Exige sesión (si no hay, 303 a `/login` como `link`) y un proveedor habilitado del que el usuario tenga una cuenta (si no, 303 a `/login?error=auth.reauth_mismatch`, sin crear flujo). Crea el flujo con `purpose = 'reauth'` y `link_user_id` = el usuario de la sesión, y responde 303 al proveedor. `next` sigue la regla de D2. La palabra `reauth` se suma a la lista del test `no_provider_is_called_like_a_route_word`.
- **RF12. Callback con `purpose = 'reauth'`.**
  1. Como `link`: la cookie de sesión del callback debe ser una sesión válida del usuario del flujo (si no, `auth.invalid_state`). Luego el intercambio con el proveedor, igual que login y link.
  2. Una transacción: bloquea al usuario (`banned_at IS NULL`, si no `auth.invalid_state`), rota con `RevokeFilter::One` y exige que haya borrado exactamente esa sesión (si no, `auth.invalid_state`), y busca la identidad `(provider, provider_user_id)`.
  3. Si la identidad no existe o es de otro usuario: **`auth.reauth_mismatch`**. La transacción se deshace, así que la sesión en uso queda como estaba. Nunca se crea un usuario ni una cuenta OAuth, y nunca se cambia de usuario. El código es el mismo para "no existe" y "es de otro", para no revelar si una identidad ajena tiene cuenta.
  4. Si es del usuario: escribe `auth_events` (`reauth`) y emite una sesión nueva con `authenticated_at = now()`, `auth_account_id` = esa fila, `auth_link_seq` = `max(id)` del usuario (con el usuario bloqueado, exacto) y expiración absoluta nueva por rol (30 días o 12 h), como un login. Responde 303 al `next` del flujo con la cookie nueva.

  Todo error usa el 303 a `/login?error=<code>` de la Enmienda 2026-10-28 de ADR-0008, con la cookie del flujo expirada y sin crear ni rotar sesión.

### Desvincular (preguntas 1, 2 y 4; resultados 3 y 4)
- **RF13. Regla de desvincular.** `POST /api/auth/unlink/{provider}` quita la cuenta P del usuario solo si la sesión, **leída dentro de la transacción**, tiene:
  - autenticación reciente (10 min);
  - autenticación vigente;
  - `auth_account_id ≤ P.id`, es decir, la identidad de la autenticación es P misma o una vinculada antes que P.

  Si falta algo: **403 `auth.reauth_required`**, `private, no-store`, sin borrar nada, sin `auth_events` y sin revocar. La regla vale igual para un proveedor habilitado o deshabilitado por config: uno deshabilitado solo se puede quitar con una identidad más antigua (ver "Áreas de preocupación", 5).
- **RF14. Orden dentro de la transacción de `unlink`.**
  1. Bloquea al usuario (`banned_at IS NULL`, si no, 401 como hoy).
  2. Lee la sesión por id, con las condiciones de validez del lookup y `FOR UPDATE`. Si ya no existe (logout, `logout-all`, rotación, ban o un `unlink` de otra sesión que la revocó), responde **401 `auth.unauthenticated`** con la cookie expirada, y no hace nada más.
  3. Busca la cuenta P del usuario (si no hay, 404).
  4. Cuenta lo que quedaría entre los proveedores habilitados (si no queda nada, 409 `auth.last_provider`). Va antes de la regla, para que un usuario con un solo proveedor no reciba una invitación a reautenticarse que no le sirve.
  5. Aplica RF13 con los valores leídos en el paso 2.
  6. Borra P, escribe `auth_events` (`unlink`), revoca las demás sesiones (`RevokeFilter::OthersOfUser`), **consume la confirmación de la sesión en uso** (`UPDATE sessions SET auth_account_id = NULL WHERE id = <esa sesión>`) y confirma. Es un `UPDATE`, así que la regla de D3 sobre los borrados no cambia, y el `CHECK` de RF2 lo permite. Con eso, en una ventana de confirmación se hace **una sola** cosa: quitar un proveedor o vincular uno. Para volver a quitar o para vincular después, hay que confirmar otra vez.

  Como todo `link`, `reauth` y `unlink` del mismo usuario toma el mismo bloqueo, el paso 2 ve el resultado de cualquiera de ellos que se haya confirmado en medio. Una identidad de la autenticación borrada en medio llega como `auth_account_id = NULL` y no pasa el paso 5.
- **RF15. Pausa de prueba.** `UnlinkGate`, con el mismo molde que `LinkGate` (trait, `NoUnlinkGate` en producción, configurado solo en `Settings` desde código de test, nunca por variable de entorno), entre `session::current()` y la transacción.

### Admin
- **RF16. `RecentAdmin`** usa `SessionInfo::recent_admin` (RF7). Lo demás de RF-C7 de 008 no cambia: un no admin también recibe 403 `auth.reauth_required`, sin sesión 401, con error de base 503.
- **RF17. `server admin grant|revoke`.** `grant` guarda `admin_account_cutoff = max(id)` de las cuentas OAuth del usuario, en su misma transacción. Volver a correr `grant` sobre un admin lo actualiza: así el owner habilita para admin un proveedor que vinculó después. `revoke` lo pone en `NULL`. Los dos siguen revocando todas las sesiones (D9).
  - Al confirmar, `grant` imprime por **stdout** (nunca en un log) los nombres de los proveedores que quedan dentro del corte (`google`, `discord`, `dev`), sin `provider_user_id` ni ids. Si aparece uno que el admin no reconoce, la ayuda le indica correr `revoke`, quitarlo desde `/account` y volver a correr `grant`.
  - La ayuda de la CLI (`server admin grant --help`) explica los dos casos en que hay que volver a correr `grant`: "después de migrar a 009" (ningún admin tiene corte) y "después de vincular un proveedor nuevo como admin" (no da admin hasta otro `grant`). Pide revisar los proveedores vinculados en `/account` antes de correrlo, porque todo lo vinculado hasta ese momento pasa a dar admin.

### Lectura para `/account`
- **RF18. `GET /api/auth/reauth-status`.** Con sesión, 200 con `ReauthStatus`, `Cache-Control: private, no-store`; sin sesión, 401 como `/api/me`. El server calcula todo, y la web no repite la regla:
  - `link_ready: bool`: la sesión tiene `step_up`.
  - `unlink: Vec<UnlinkStatus>`, una entrada por cuenta vinculada, con:
    - `provider`;
    - `ready: bool`: RF13 se cumple ahora;
    - `reauth_with: Vec<Provider>`: los proveedores **habilitados** del usuario con los que una reautenticación dejaría quitar P (P misma, si está habilitada, y las vinculadas antes), en el orden de los botones de login.

  No expone fechas, ids ni el proveedor de la autenticación. Es una lectura: la regla se vuelve a evaluar en la transacción de cada acción.

### Protocol
- **RF19.** En `crates/protocol`:
  - `AuthErrorCode::ReauthMismatch` (`auth.reauth_mismatch`). El comentario de `ReauthRequired` pasa a "la acción necesita una autenticación reciente que sirva (D9, enmienda 2026-10-30)".
  - `CallbackErrorCode` suma `ReauthRequired` y `ReauthMismatch`: la lista cerrada de `/login?error=` crece de seis a ocho códigos (es la enmienda de ADR-0008).
  - `ReauthStatus` y `UnlinkStatus` (RF18).

  `just gen` regenera `packages/protocol-ts`.

### Web
- **RF20. `/account`, vincular.** El `load` lee `reauth-status` (una lectura, D5). Si `link_ready` es falso, en lugar de los botones de vincular la tarjeta de proveedores muestra el **panel de confirmación** (RF22) con todos los proveedores habilitados vinculados del usuario. Con `link_ready`, los botones de vincular son los de hoy.
- **RF21. `/account`, desvincular.** El botón "Quitar" sigue siendo una form action. Si la respuesta es `auth.reauth_required`, la fila de ese proveedor muestra el panel de confirmación con su `reauth_with`, y el foco va al título del panel. Si `reauth-status` trae `reauth_with` vacío para un proveedor (está deshabilitado y no hay uno más antiguo, Área 5), la fila **no ofrece "Quitar"** y muestra desde el principio la explicación de que ese proveedor no se puede quitar mientras esté desactivado, sin botones. Si `reauth-status` no está disponible, la fila se muestra como hoy y la acción decide.
- **RF22. Panel de confirmación.** Explica en una o dos frases por qué: "para proteger tu cuenta si alguien más está usando tu sesión, confirma que eres tú" (llaves nuevas en `en` y `es`). Muestra un botón por proveedor de la lista, de ancho completo y con al menos 44 px de alto a 375 px. Cada uno es un `<form method="post" action="/api/auth/reauth/{p}">` con `next=/{locale}/account?reauth=1`. El de `dev` (solo en builds con `dev-auth`) incluye el campo `DevIdentityField` de `/login`.
- **RF23. Regreso.** Con `?reauth=1` y `link_ready` verdadero, `/account` muestra en su `role="status"` un mensaje fijo ("Listo, ya puedes terminar"). La página solo lee la presencia de ese parámetro, nunca refleja su valor. El usuario vuelve a tocar "Quitar" o "Vincular": ningún `GET` muta (D5).
- **RF24. `/login?error=`** reconoce `auth.reauth_required` y `auth.reauth_mismatch` con texto propio en `en` y `es`. `error-text.ts` y `codes.ts` los suman. El texto de `auth.reauth_mismatch` dice qué hacer: el caso típico es que en el proveedor se eligió otra cuenta, así que pide elegir **la misma cuenta con la que se vinculó ese proveedor** (por ejemplo, "Esa no es la cuenta que vinculaste. Vuelve a intentarlo y elige la misma cuenta de ese proveedor."). Con sesión, la página ofrece un botón "Volver a mi cuenta" a `/{locale}/account` como acción principal, no solo un enlace. Sin sesión (la sesión expiró en medio), ofrece el login como hoy.
- Todo texto nuevo vive en `apps/web/messages/{en,es}.json` y pasa `check-i18n`. Ningún texto viene del server.

### Documentación
- **RF25.** El PR actualiza:
  - `docs/architecture.md`: las tablas (columnas nuevas, `reauth` en `oauth_flows` y en `auth_events`), la lista de "qué postea el navegador directo a `/api`" (`POST /api/auth/reauth/{p}`), la lectura `GET /api/auth/reauth-status` y, en la parte de admin, cuándo hay que volver a correr `server admin grant` (los dos casos de RF17), con el mismo contenido que la ayuda de la CLI: la ayuda va en inglés y este documento en español. *Errata 2026-10-30: decía "el mismo texto"; ver "Revisión".*
  - `docs/legal/aviso-de-privacidad.md`, sección 3: en "Sesiones", "el proveedor con el que iniciaste sesión o confirmaste tu identidad, y cuándo"; en "Historial de eventos de seguridad", "desvinculación de proveedor" (faltaba desde 008) y "confirmación de identidad"; en "Datos temporales de inicio de sesión", "si vinculas un proveedor **o confirmas tu identidad**, tu identificador interno". Los plazos de la sección 8 no cambian.

## Requisitos no funcionales
- **Rendimiento (1 vCPU / 4 GB).** El lookup suma dos subconsultas sobre como mucho tres filas de `oauth_accounts` por usuario, con el índice `oauth_accounts_user_provider_uq`. `reauth-status` es una consulta por carga de `/account`. Nada nuevo en memoria.
- **Determinismo y reloj.** Toda comparación de tiempo usa `now()` de Postgres. El orden de vínculo es una secuencia, no un reloj. Sin aleatoriedad nueva fuera de `OsRng` (tokens y flujos, como hoy).
- **Seguridad.** Toda decisión de vincular, desvincular y admin se toma con datos leídos bajo el bloqueo del usuario (vincular y desvincular) o en la misma consulta de validez de la sesión (`RecentAdmin`). Ninguna respuesta dice por qué falló la regla (fecha, orden o identidad): siempre `auth.reauth_required`.
- **Logs (ADR-0012).** El único evento nuevo es `auth.reauth_failed`, con `error_kind` y nada más, ante un error de base. Ningún log lleva `provider_user_id`, `user_id`, ids de sesión ni de cuenta OAuth, `ip_hash`, la IP, el token, el `code`, el `state` ni el `code_verifier` (A4). `SessionInfo`, `ReauthStatus` y los tipos nuevos no muestran nada de eso en `Debug`.
- **Ledger (ADR-0005).** Vincular, desvincular y reautenticarse no escriben en `ledger_*`.
- **i18n (ADR-0007).** El server responde códigos. `check-i18n` pasa con las llaves nuevas.
- **Accesibilidad y móvil.** El panel funciona con teclado y lector de pantalla (título con foco, botones con nombre accesible que incluye el proveedor) y a 375 px sin scroll horizontal.

## Diseño

### Componentes afectados
| Área | Archivos |
|---|---|
| Migración | `apps/server/migrations/<nueva>_reauth.sql` (RF1 a RF5) |
| Sesiones | `apps/server/src/sessions/mod.rs`: `issue` recibe la autenticación (nueva o conservada) y devuelve el `Max-Age`; lookup con `step_up` y `recent_admin` |
| Login | `apps/server/src/auth/login.rs` (RF6) |
| Vincular | `apps/server/src/auth/link.rs` (RF8 a RF10) |
| Reautenticar | `apps/server/src/auth/reauth.rs` nuevo (inicio y transacción), `start.rs` (`Purpose::Reauth`), `callback.rs` (rama `reauth`), `http/routes.rs` |
| Desvincular | `apps/server/src/auth/unlink.rs` (RF13 a RF15) |
| Admin | `apps/server/src/http/admin.rs` (RF16), `apps/server/src/admin.rs` (RF17) |
| Lectura | `apps/server/src/auth/reauth_status.rs` nuevo (RF18) |
| Protocol | `crates/protocol/src/error.rs`, `crates/protocol/src/auth.rs` o el módulo donde viva `Provider` (RF19); `packages/protocol-ts` regenerado |
| Web | `apps/web/src/routes/account/+page.server.ts` y `+page.svelte`, `ProviderList.svelte`, un `ReauthPanel.svelte` nuevo, `lib/server/api/reads.ts`, `lib/features/auth/{codes,error-text}.ts`, `messages/{en,es}.json` |
| Tests | `apps/server/tests/` (ver "Estrategia de pruebas"), `apps/web/e2e/auth/account.spec.ts`, `apps/web/e2e/support/db.mjs` |
| Docs | `docs/architecture.md`, `docs/decisions/0008-autenticacion.md` (la enmienda ya va en este PR) |

No se tocan `crates/engine`, `crates/sim`, `packages/renderer` ni el ledger.

### Modelo de datos
```
oauth_accounts  + id bigint IDENTITY UNIQUE          -- orden de vínculo
sessions        + auth_account_id bigint NULL → oauth_accounts(id) ON DELETE SET NULL
                + auth_link_seq   bigint NULL        -- max(oauth_accounts.id) del usuario al autenticarse
users           + admin_account_cutoff bigint NULL   -- max(oauth_accounts.id) al hacer `admin grant`
oauth_flows     purpose ∈ {login, link, reauth}; link_user_id obligatorio en link y reauth
auth_events     kind ∈ {login, link, unlink, grant, reauth}
```
Un `CHECK` en `sessions` exige que `auth_link_seq` no sea `NULL` cuando `auth_account_id` tampoco lo es. La FK puede dejar `auth_account_id` en `NULL` con `auth_link_seq` lleno, y eso significa "sin autenticación vigente".

Las condiciones, en SQL ilustrativo (el plan fija el texto exacto):
```sql
-- vigente
s.auth_account_id IS NOT NULL
AND EXISTS (SELECT 1 FROM oauth_accounts a WHERE a.id = s.auth_account_id AND a.user_id = s.user_id)
AND s.auth_link_seq >= (SELECT max(id) FROM oauth_accounts WHERE user_id = s.user_id)
-- step_up = vigente AND s.authenticated_at > now() - interval '10 minutes'
-- recent_admin = u.role = 'admin' AND vigente
--   AND s.authenticated_at > now() - interval '60 minutes'
--   AND s.auth_account_id <= u.admin_account_cutoff
-- unlink(P) = step_up AND s.auth_account_id <= P.id
```

### API
| Ruta | Cambio |
|---|---|
| `POST /api/auth/link/{p}` | Exige `step_up`; si falta, 303 `/login?error=auth.reauth_required` sin flujo |
| `GET /api/auth/{p}/callback` | `link`: RF9 y RF10. `reauth`: RF12 |
| `POST /api/auth/reauth/{p}` | **Nueva**, formulario, 303 al proveedor (RF11) |
| `POST /api/auth/unlink/{p}` | RF13 y RF14: 403 `auth.reauth_required`, 401 si la sesión se cerró en medio |
| `GET /api/auth/reauth-status` | **Nueva**, lectura (RF18) |
| Rutas de admin (TA-13) | `RecentAdmin` con la regla nueva (RF16) |

### Flujo: el usuario quita un proveedor con una autenticación vieja
```
/account ── Quitar Google (form action) ──▶ POST /api/auth/unlink/google
                                              │ tx: lock user, sesión FOR UPDATE, regla
                                   403 auth.reauth_required
/account (panel: "confirma que eres tú") ── Confirmar con Dev ──▶ POST /api/auth/reauth/dev
                                              │ 303 al proveedor (flujo reauth, atado al usuario)
proveedor ──▶ GET /api/auth/dev/callback ── tx: lock, rota, identidad del usuario, sesión nueva
                                              │ 303 /es/account?reauth=1 (cookie nueva)
/account ("Listo") ── Quitar Google ──▶ unlink: pasa la regla ──▶ 303 a /, revoca las demás sesiones
```
*Errata 2026-10-30: decía "200"; ver "Revisión".*

### El ataque de TA-29, paso a paso, con la regla nueva
Víctima con V. El atacante tiene solo la sesión S de la víctima.
1. **S vieja (más de 10 min).** `link` responde `auth.reauth_required`: el ataque termina en el paso 1. Desvincular V también da `auth.reauth_required` (o `auth.last_provider` si V es su único proveedor).
2. **S reciente** (robada justo después de un login real). `link` de X pasa (RF8). La sesión rotada conserva la autenticación con V, pero X tiene un `id` mayor que `auth_link_seq`, así que ya no es vigente: desvincular V da `auth.reauth_required`.
3. **El atacante cierra sesión y entra con X**, o hace `logout-all` y entra con X, o espera lo que sea y entra con X. Su autenticación es con X, y `X.id > V.id`, así que no cumple `auth_account_id ≤ V.id`. El orden no depende del tiempo: esperar no cambia nada. Desvincular V da `auth.reauth_required`, para siempre.
4. **Reautenticarse con X** es lo mismo que entrar con X. Para tener una autenticación con V, el atacante necesita la cuenta del proveedor V.
5. **Admin.** X tiene un `id` mayor que `admin_account_cutoff` (el `grant` fue antes del robo), así que ninguna sesión autenticada con X pasa `RecentAdmin`, sin importar cuándo ni cómo entre.

La víctima, en cambio, se reautentica con V (`V.id < X.id`), quita X (RF13 pasa), y `unlink` revoca las sesiones del atacante (cierra sus sockets por el `NOTIFY` de siempre). Un login posterior con X no encuentra la identidad y crea un usuario nuevo y vacío: ya no entra a la cuenta de la víctima.

### UI
- **Móvil (375 px).** Las tarjetas de siempre. El panel de confirmación aparece dentro de la tarjeta de proveedores, debajo de la fila afectada (al desvincular) o en lugar de los botones de vincular. Va con un título, el texto del porqué y un botón de ancho completo por proveedor. El campo de `dev`, si aparece, va encima de su botón.
- **Escritorio.** El mismo panel, con los botones `sm:w-auto`.
- **Estados.** Cargando: el botón deshabilitado mientras la acción está pendiente, como hoy. Error de la acción: el texto del código, nunca JSON. `reauth-status` no disponible (503): la página se muestra sin el panel preventivo, y la acción decide (el 403 sigue abriendo el panel con un texto genérico y los proveedores vinculados habilitados). Vacío: no aplica, porque con sesión siempre hay al menos un proveedor.

## Áreas de preocupación
1. **Qué cede la regla (pregunta 1).** No cede la espera: no hay período de espera, y el criterio 1 aguanta cualquier espera porque el orden no depende del tiempo. Tampoco cede el camino para desvincular **mientras el usuario controle el proveedor**: todo proveedor habilitado se puede quitar reautenticándose con él mismo. Quien perdió el control de su proveedor más antiguo no lo puede quitar (Área 13). **Cede la salida que proponía la pregunta 2**: el proveedor de la autenticación sí puede ser el que se quita, pero solo si no se vinculó nada después de esa autenticación. Ese "después" lo garantiza `auth_link_seq`, y es lo que impide "vincular X con una sesión reciente y quitar V con esa misma sesión".
2. **La ventana de la pregunta 2 (navegador desatendido o sesión robada en los 10 min posteriores a un login real o a una reautenticación).** Quien tenga esa sesión puede hacer **una sola** cosa. Puede quitar **una** de las identidades de la víctima, si tiene dos, pero nunca la última, porque `auth.last_provider` lo impide y la que queda es de la víctima. O puede vincular X, y entonces la sesión se congela (punto 2 del ataque). No puede hacer las dos cosas: un `unlink` exitoso consume la confirmación de su sesión (RF14, paso 6), así que después ya no puede vincular sin reautenticarse, y un vínculo deja la autenticación sin vigencia, así que después ya no puede desvincular. Con solo esa sesión, la víctima nunca queda fuera; si el navegador además tiene el proveedor abierto, ver el Área 4. Quitar una identidad es una molestia acotada, no una toma de la cuenta, y no está en el criterio 1. Vincular X es la convivencia del Área 12. *Errata 2026-10-30; ver "Revisión", entrada "riesgos residuales".*
3. **Doble robo (parte de la convivencia).** Si X **ya está vinculado** (un robo anterior que la víctima no notó) y el atacante consigue, además, una sesión de la víctima de los 10 min posteriores a un login o reautenticación con V, esa sesión cumple la regla para quitar V, y X queda como el proveedor restante. Hace falta un segundo robo de una sesión reciente, así que no es "solo la sesión de la víctima" del criterio 1. Es un agravante del riesgo de convivencia que el owner aceptó (Área 12), y lo cierra lo mismo: que la víctima vea X en `/account` y lo quite. No hay una regla que lo cierre sin romper el camino para desvincular: "tengo Google y Discord, quiero quedarme solo con Discord" es exactamente la misma forma. Ver "Riesgo residual" en la enmienda de ADR-0008.
4. **Navegador desatendido con el proveedor abierto.** Si en ese navegador la cuenta de Google o Discord de la víctima sigue con sesión, quien lo use puede completar una reautenticación con V sin conocer ninguna contraseña: el proveedor no la pide. Ninguna reautenticación por OAuth sin contraseñas lo evita (ADR-0008 D1 descarta las contraseñas). El intent habla de "solo la sesión", y esto es más que la sesión. Queda escrito como límite. **La consecuencia es la toma de cuenta completa de TA-29, sin segundo robo:** quien use ese navegador se reautentica con V, vincula X, se reautentica con V y quita V. La víctima queda fuera para siempre, y su login con V crea un usuario nuevo y vacío, mientras que X sigue entrando a su cuenta. Con un admin, la reautenticación con V además pasa `RecentAdmin`. El intent nombra el navegador desatendido en su problema, así que esta feature **no** lo cierra si el proveedor sigue abierto. Forzar `prompt` en el proveedor solo agrega un clic. El owner lo aceptó (Área 12). *Errata 2026-10-30; ver "Revisión", entrada "riesgos residuales".*
5. **Proveedor deshabilitado por config.** Hoy `unlink` deja quitar uno deshabilitado, y la regla nueva lo sigue permitiendo con una identidad vinculada antes. Si el deshabilitado es el **más antiguo** de la cuenta, no se puede quitar hasta que vuelva a habilitarse, porque nadie puede reautenticarse con él. Relajarlo reabriría el criterio 1 cuando V está deshabilitado (el atacante con X quitaría V para siempre). Para el usuario legítimo no es un riesgo de seguridad: un proveedor deshabilitado no deja entrar a nadie, así que conservarlo no cuesta nada. Pero se aparta de la letra "siempre debe existir un camino para desvincular": el camino es esperar a que se rehabilite. product-owner aceptó esa espera en su revisión del 2026-10-30, porque la restricción del intent dice "aunque tenga que esperar". Esa espera no pide aceptación de riesgo del owner. `/account` lo explica desde el principio y no ofrece "Quitar" (RF21). Si algún día se **retira** un proveedor para siempre, la spec de ese cambio decide qué pasa con sus filas. **Matiz de seguridad:** mientras V, el más antiguo, esté deshabilitado, una víctima cuyo único otro proveedor sea X no puede iniciar sesión ni quitar X hasta que V vuelva. Es parte de la convivencia, no de esa espera: product-owner no lo evaluó, y lo aceptó el owner (Área 12). *Errata 2026-10-30; ver "Revisión", entrada "riesgos residuales".*
6. **Admin se aparta de la letra del Resultado 2.** El Resultado 2 pide para `RecentAdmin` "la misma regla" de desvincular, pero esa regla es por proveedor que se quita y no tiene un objetivo en admin. `RecentAdmin` usa la misma autenticación vigente (identidad vinculada y nada vinculado después), y, en lugar del orden respecto de P, el orden respecto del `admin grant` (`admin_account_cutoff`). Es más estricta: una identidad vinculada después del `grant` **nunca** da admin, ni siquiera a un admin legítimo, hasta que el owner vuelva a correr `server admin grant`. Con eso desaparece el peor caso de la convivencia (acceso de admin permanente con X), salvo que se corra `server admin grant` con X ya vinculado, sea el primero o uno posterior (Área 12). *Errata 2026-10-30; ver "Revisión", entrada "riesgos residuales".*
7. **Vincular pide confirmación (pregunta 7).** Un usuario que vincula su segundo proveedor más de 10 min después de entrar tiene que confirmar antes. Es un toque más, en un caso poco frecuente, y está en el mismo panel.
8. **Ventana del callback de `link`: 20 min.** RF9 tolera hasta 20 min en el callback porque la confirmación se exigió al empezar (10 min) y el flujo vive 10 min más. Desvincular, que es una sola petición, usa los 10 min exactos.
9. **Sesiones y admins que ya existen (pregunta 6).** Al migrar, ninguna sesión tiene autenticación vigente y ningún admin tiene `admin_account_cutoff`. Hoy no hay nada desplegado. Localmente, el owner vuelve a correr `server admin grant` sobre su usuario y entra de nuevo. Lo dicen la ayuda de la CLI y `docs/architecture.md` (RF17 y RF25), no solo el PR.
10. **TA-13.** Las acciones de admin que lleguen con TA-13 usan `RecentAdmin`, que se evalúa al leer la sesión. Si una acción de admin necesita que la autenticación siga sirviendo dentro de su transacción (por ejemplo, frente a un `unlink` concurrente de su identidad), TA-13 repite la lectura bajo su bloqueo, como RF14. Queda anotado para su spec.
11. **Aviso de privacidad.** `sessions` guarda ahora qué cuenta OAuth usó la sesión, y `auth_events` registra las confirmaciones de identidad. No es un dato nuevo (la identidad ya está en `oauth_accounts`), pero es un tratamiento que el aviso tiene que declarar. security-reviewer pidió las líneas, y RF25 las agrega.
12. **Riesgos para el owner.** Ninguno nuevo. El criterio 1 y el de admin quedan **sin relajar**, así que el riesgo "si la regla no aguanta la espera" del intent no aplica. El riesgo de **convivencia** lo aceptó el owner, y es como el del intent, pero más chico: con la pregunta 7 hace falta una sesión de los 10 min posteriores a un login real o a una reautenticación para vincular X. El punto 3 (doble robo) es parte de esa convivencia y se le reporta con ella. Con una sola sesión robada en la ventana de 10 min, el atacante elige entre quitar una identidad de la víctima o vincular X, nunca las dos (Área 2). El owner aceptó estos riesgos el 2026-10-30, tal como están descritos aquí (ver "Revisión", entrada del owner).
    - **Con solo X, el atacante nunca llega a admin ni deja fuera a la víctima.** Hay tres excepciones:
      - **Segundo robo, lockout (punto 3):** con X vinculado, la víctima entra con V y el atacante roba esa sesión reciente. Quita V, X queda como único proveedor y la víctima queda fuera.
      - **Segundo robo, admin:** esa misma sesión reciente de V también pasa `RecentAdmin` durante 60 min.
      - **`grant` con X vinculado:** si se corre `server admin grant` con X ya vinculado, sea el primero o uno posterior, X entra en el corte y da admin hasta que se corra `revoke`.
    - **Mientras conviva,** X puede gastar las monedas de la víctima, cambiar el alias y cerrar sus sesiones con `logout-all`. Hoy ninguna ruta gasta monedas, así que lo primero vale desde que existan las apuestas.
    - **Para que el owner lo lea junto con la convivencia:** el proveedor más antiguo es la raíz de la cuenta (Área 13). Quien lo pierde no lo puede quitar, y quien lo controla puede quitar los demás. No es un riesgo nuevo (antes de 009, cualquiera de los dos podía quitar al otro), y no hay otra regla que lo resuelva. Aquí también entran el matiz del Área 5 y el navegador con el proveedor abierto del Área 4, que lleva a la toma de cuenta completa.
    - *Errata 2026-10-30; ver "Revisión", entrada "riesgos residuales".*
13. **El proveedor más antiguo es la raíz de la cuenta.** Pongamos a una usuaria que entró con Google y después vinculó Discord. Si borra o pierde su cuenta de Google, **nunca** podrá quitar Google de Torneo Absurdo, aunque Google esté habilitado: `reauth_with` de Google es `[google]`, y nadie más que esa cuenta de Google puede reautenticarse con ella. Sigue entrando y jugando con Discord. Si otra persona se queda con esa cuenta de Google, esa persona entra a la cuenta y además puede quitar Discord, porque Google es más antiguo. Con lo que ve el server, "perdí mi Google" no se distingue de "alguien quiere quitar mi Google", así que no hay regla que lo resuelva sin reabrir el criterio 1. Antes de 009 era peor: cualquiera de los dos proveedores podía quitar al otro. Se aparta de la letra "siempre debe existir un camino para desvincular" para este usuario legítimo, y queda escrito. El remedio queda fuera de alcance: recuperación de cuenta o soporte, como dice el intent.

## Respuestas a preguntas abiertas del intent
1. **¿Qué es "ya estaba vinculado antes"?** → Una regla del tipo (c) ampliada con (b). Quitar P exige una autenticación reciente (10 min) y vigente (la identidad sigue vinculada y no se vinculó nada después) con una identidad **no más nueva que P**: P misma o una vinculada antes (`auth_account_id ≤ P.id`). Vale igual para Google, Discord y `dev`. Para `RecentAdmin`, la misma autenticación vigente con el corte del `admin grant` en lugar de P (Área 6). Qué cede: la salida de la pregunta 2 (Área 1) y, para un proveedor deshabilitado que sea el más antiguo, el camino inmediato (Área 5).
2. **La ventana después de un login real** → La identidad de la autenticación **sí** puede ser la que se quita, con la condición "nada vinculado después". En esa ventana se hace una sola cosa: quitar una identidad de la víctima (con solo esa sesión, sin dejarla fuera) o vincular X, porque un `unlink` exitoso consume la confirmación. Riesgo restante en las Áreas 2 a 4 y 12.
3. **Qué conserva la sesión rotada por vincular** → `authenticated_at`, `expires_at`, `auth_account_id` y `auth_link_seq`, con el `Max-Age` de lo que le queda. Vincular no alarga los 30 días ni las 12 h (RF10).
4. **La ventana exacta** → 10 min (`STEP_UP_WINDOW`) para vincular, desvincular y `reauth-status`, aparte de los 60 min de admin, que no cambian. El callback de `link` tolera 20 min (Área 8).
5. **Código y flujo de vuelta** → Desvincular y vincular reusan `auth.reauth_required`. La reautenticación usa un flujo propio, `purpose = 'reauth'`, atado al usuario de la sesión (`link_user_id`, como D2). Nunca crea ni cambia de usuario, responde `auth.reauth_mismatch` (código nuevo) si la identidad no es del usuario y deja la sesión como estaba. Vuelve a `next = /{locale}/account?reauth=1`, con la regla de D2.
6. **Sesiones que ya existen** → Cuentan como sin autenticación vigente (`NULL`), y los admins necesitan otro `server admin grant` (Área 9).
7. **¿Vincular pide autenticación reciente?** → Sí: `step_up` al empezar y autenticación vigente en el callback (RF8 y RF9). Para admin, además, una identidad vinculada después del `grant` nunca da `RecentAdmin`.

## Estrategia de pruebas
Integración en `apps/server/tests/`, con los proveedores simulados de `tests/support/oauth.rs` (Google y Discord) y `support::auth` (`dev`), sin red. El "paso del tiempo" se simula moviendo hacia atrás `sessions.authenticated_at` (y `oauth_accounts.created_at`, para mostrar que no influye) con SQL de test, nunca con un reloj en la aplicación.

| Criterio del intent | Prueba |
|---|---|
| 1. El ataque falla | `tests/reauth_attack.rs`. Víctima con Google; sesión robada **reciente** (el peor caso). El atacante vincula Discord y luego intenta desvincular Google: (a) enseguida; (b) tras logout y login con Discord; (c) tras `logout-all` y login con Discord; (d) tras "esperar" 400 días (las fechas movidas), entrando con Discord y reautenticándose con Discord en medio. En todos: 403 `auth.reauth_required`, la fila de Google sigue, ningún `auth_events` `unlink`, y la víctima entra con Google. Variante con la sesión robada **vieja**: el `link` responde 303 `auth.reauth_required` y no crea flujo ni fila. |
| 2. El usuario legítimo desvincula | Integración: con dos proveedores, entra con el más antiguo y quita el otro (pasa). Entra con el más nuevo, intenta quitar el más antiguo (403), se reautentica con el más antiguo y lo quita (pasa). Con la autenticación movida 11 min atrás, 403, y tras reautenticarse, pasa. **e2e** (en los tres proyectos que corren `auth/`: `chromium`, de escritorio, y `chromium-mobile` y `webkit-mobile`, a 375 px; en `es` y una corrida en `en`; *errata 2026-10-30, ver "Revisión"*): login con `dev`, se siembra Google (con eso la autenticación ya no es vigente) y también una variante con la sesión envejecida por SQL (`ageSession` nuevo en `e2e/support/db.mjs`). Quitar Google abre el panel con su texto, "Confirmar con Dev" hace el ida y vuelta, `/account?reauth=1` muestra el mensaje, y Quitar Google pasa. Otra variante entra de nuevo con `dev` después de sembrar Google y lo quita sin ver el panel (la letra del criterio: "entra con uno y quita el otro"). El test 1 actual de `account.spec.ts` se adapta a este flujo. |
| 3. La víctima echa al atacante | Integración: tras el paso 1 (sesión reciente, Discord vinculado), la víctima se reautentica con Google y quita Discord. Las sesiones del atacante quedan borradas, su socket recibe el cierre (como el test 10 de ADR-0008), y un login con Discord crea un usuario distinto, sin las monedas ni el alias de la víctima. |
| 4. Admin | `tests/recent_admin.rs` ampliado. Admin con Google, `grant`, login. (i) Sesión de más de 60 min: `link` rechazado, y `RecentAdmin` da 403. (ii) Sesión reciente que vincula Discord (pasa por RF8): enseguida, tras logout y login con Discord, tras `logout-all` y login con Discord, y tras "esperar" con reautenticación con Discord, `RecentAdmin` da 403 en todos. (iii) Una reautenticación con Google pasa. (iv) Un segundo `grant` hace que Discord pase. (v) Una sesión de antes de la migración (columnas en `NULL`) da 403. |
| Carrera de `unlink` | `tests/unlink.rs` con `UnlinkGate`. En la pausa: (a) `logout` de esa sesión; (b) `logout-all`; (c) desde otra sesión del mismo usuario, que se desvincula la identidad de la autenticación; (d) un `link` que rota la sesión → 401; (e) **otra** sesión del mismo usuario vincula un proveedor: la sesión en pausa sigue viva, pero su `auth_link_seq` quedó viejo → 403 `auth.reauth_required` (es el caso que prueba que la vigencia se calcula en la transacción y no se toma del `SessionInfo` de antes de la pausa); (f) un `reauth` que rota esta sesión → 401. En todos, la petición en pausa responde 401 o 403, no borra la cuenta pedida, no escribe `auth_events` `unlink` y no revoca ninguna sesión (se cuentan los `NOTIFY` y las filas de `sessions` que borró esa petición). |
| Ledger | En el escenario completo (login, vincular, reautenticarse, desvincular), `ledger_entries` y `ledger_postings` solo tienen el `grant` del primer login. |
| Canarios (ADR-0012) | `tests/logs_account.rs` ampliado: un `link` rechazado, un `reauth` exitoso y uno con `auth.reauth_mismatch`, un `unlink` rechazado y uno exitoso, y `reauth-status`. Canarios: el `provider_user_id`, el `user_id`, el `code`, el `state`, el `code_verifier`, la cookie del flujo y la de sesión, el `ip_hash` y la IP. Ninguno aparece en el subscriber de tracing ni en el log de Postgres del test (`pg_log`). |
| `unlink` consume la confirmación | Tras un `unlink` exitoso, la sesión tiene `auth_account_id = NULL`: un `link` da 303 `auth.reauth_required` sin crear flujo, un segundo `unlink` da 403 y `reauth-status` dice `link_ready = false`. El escenario del Important 1 de security-reviewer: víctima con Google V1 (más antiguo) y Discord V2, sesión reciente con V1; quitar V2 pasa y vincular X después falla. Tras reautenticarse, se puede volver a quitar o vincular. |
| Área 5 y Área 13 | Integración: el proveedor más antiguo deshabilitado por config tiene `reauth_with` vacío en `reauth-status`, y su `unlink` da 403. Si está habilitado, su `reauth_with` es solo él mismo. Vitest del componente de la fila: con `reauth_with` vacío no hay botón "Quitar" y se ve la explicación. |
| Rotación conserva (pregunta 3) | `link` conserva `authenticated_at`, `expires_at`, `auth_account_id` y `auth_link_seq`, y el `Max-Age` de la cookie es lo que le queda (con `expires_at` movido, por ejemplo, a 1 h). En admin, nunca más de 12 h desde el login. |
| `reauth` atado | Con un flujo `reauth`, el proveedor devuelve una identidad de otro usuario, o una que no existe: 303 `auth.reauth_mismatch`, la misma sesión sigue sirviendo, ningún usuario ni cuenta nueva y ningún `auth_events`. Un flujo `reauth` de otra sesión, o sin sesión: `auth.invalid_state`. Ataque CSRF de login: un callback `reauth` con la cookie de otro flujo no cambia de usuario. |
| Esquema | Migración sobre una base con datos de 008: ids en orden de `created_at`, sesiones con `NULL`. `users_privileged_guard` rechaza fuera de la CLI (`TA004`) un `UPDATE` que solo toca `admin_account_cutoff` y un `INSERT` con el corte no `NULL`. Tests de fuente: solo `admin.rs` escribe `admin_account_cutoff`, y nada actualiza `oauth_accounts.id` ni `user_id` ni usa `OVERRIDING SYSTEM VALUE`, `setval` o `ALTER SEQUENCE … RESTART` sobre `oauth_accounts`. CLI: `grant` imprime por stdout los nombres de los proveedores dentro del corte, sin ids, y nada de eso llega a un log (`admin_cli_logs`). |
| Protocol | Los tests de `ALL` y de cadenas de `error.rs` con los códigos nuevos; los ocho de `CallbackErrorCode`; `just gen` al día. |
| Web | Vitest: `codes.ts` y `error-text.ts` con los códigos nuevos; `/login?error=auth.reauth_mismatch` con sesión muestra el texto de "elige la misma cuenta" y el botón a `/account` (también en el e2e del mismo caso, con otra identidad de `dev`); `/login` no refleja un `error` ajeno (T4 sigue); el panel arma `next` sin reflejar nada de la URL. `check-i18n`. |

`just check` y `just e2e` pasan sin red. El e2e solo usa `dev`. Google está habilitado con credenciales de relleno, así que su botón de vincular se ve, pero nunca se pulsa. El e2e comprueba que, sin confirmación, el panel ocupa el lugar de "Vincular Google" y que, tras reautenticarse con `dev`, aparece el botón.

## Alternativas consideradas
- **(a) Antigüedad mínima del vínculo (período de espera).** Sin aviso, el atacante solo espera. Rompe el criterio 1 y necesitaría la aceptación del owner. Descartada.
- **(b) sola: solo una identidad más antigua que P.** El proveedor más antiguo nunca se podría quitar. Descartada sola, pero se conserva como parte de la regla (`≤` en lugar de `=`), y con eso se cumple la letra del criterio 2 ("entra con uno y quita el otro") cuando el usuario entra con el más antiguo.
- **(c) sola: solo P misma, sin "nada vinculado después".** Con una sesión robada en la ventana de la pregunta 2, el atacante vincula X y quita V con la misma sesión. Descartada; `auth_link_seq` la cierra.
- **La salida de la pregunta 2 tal cual (la identidad de la autenticación nunca es la que se quita).** Junto con (b) o (c), deja algún proveedor sin forma de quitarse. Descartada a favor de "nada vinculado después".
- **Orden por `created_at` en lugar de una secuencia.** Depende del reloj de Postgres. Un ajuste del reloj podría invertir el orden de dos vínculos. Descartada.
- **Reautenticarse con un login normal (`purpose = 'login'`).** Si en el proveedor se elige otra cuenta, el usuario termina en otro usuario o en uno nuevo, y un login CSRF en el ida y vuelta lo cambia de cuenta. Descartada (Minor 4 de security-reviewer).
- **Para admin: exigir `RecentAdmin` para vincular.** Corta el paso 1 con una sesión de admin vieja, pero con una reciente X seguiría dando admin después. El corte del `grant` lo cierra siempre. Descartada a favor del corte.
- **Que la web derive la regla de `Me.providers` ordenado.** Pondría lógica de dominio en `apps/web`. Descartada a favor de `reauth-status` (RF18).
- **Reanudar el `unlink` al volver de la reautenticación.** Haría que un `GET` mutara (D5). Descartada: el usuario vuelve a tocar "Quitar".

## Revisión

### product-owner — 2026-10-30 — REQUEST_CHANGES
- Verifiqué:
  - **Resuelve el problema del intent.** Los seis resultados tienen su RF: la identidad exacta en la sesión (RF2, RF6), vincular que no cuenta como reautenticación (RF10), la comprobación dentro de la transacción (RF14), cómo echa la víctima al atacante (el ataque, paso a paso) y el ida y vuelta en `/account` (RF20 a RF24). Los ocho criterios de éxito tienen su prueba, y ni el criterio 1 ni el de admin se relajan. Las siete preguntas abiertas tienen respuesta y no queda ninguna arrastrada. La pregunta 7 sí entra y deja más chica la convivencia, que sigue pendiente del owner.
  - **ADRs.** 0005: ningún asiento, con su prueba. 0007: solo códigos, llaves nuevas en `en` y `es` y `check-i18n`. 0004 y el MVP: nada que dependa de tener muchos usuarios conectados, nada de dinero y nada fuera del alcance (el período de espera y el aviso siguen fuera). La enmienda de ADR-0008 va en el mismo PR.
  - **UX del ida y vuelta en `/account` (en/es, 375 px).** El camino corto queda bien: "Quitar", un panel con el porqué en una o dos frases, un botón por proveedor de ancho completo y al menos 44 px, el foco en el título, el regreso a `/{locale}/account?reauth=1` con un mensaje fijo y un toque más en "Quitar". Es un toque más que lo ideal, pero tiene sentido: ningún `GET` muta (D5). El caso "me quedo solo con Discord" (entrar con Google, vincular Discord, reautenticarse con Google y quitar Google) son tres viajes a un proveedor. Lo acepto porque es una acción poco frecuente. El e2e cubre Chromium y WebKit a 375 px, en `es` y en `en`.
  - **Área 5 (el proveedor más antiguo, deshabilitado por config): aceptable para el producto.** La restricción del intent dice "siempre debe existir un camino para desvincular, **aunque tenga que esperar** o volver a entrar", así que esperar a que el proveedor vuelva a estar habilitado entra en la letra. Un proveedor deshabilitado no deja entrar a nadie, el usuario sigue entrando con el otro, y una fila que no sirve no le cuesta nada. Relajarlo reabriría el criterio 1. Si algún día se **retira** un proveedor para siempre, la spec de ese cambio decide qué pasa con sus filas. Lo pido abajo como una línea.
  - **Pregunta 6: aceptable.** No hay nada desplegado. Que las sesiones viejas pierdan la autenticación vigente solo obliga a volver a entrar o a confirmar, y el owner corre `server admin grant` otra vez en local. Que un admin tenga que volver a correr `grant` después de vincular un proveedor también lo acepto: hoy el único admin es el owner. Lo único que pido es que esté donde el owner lo va a encontrar (Minor 3).
- Cambios pedidos:
  1. **[Important] Falta el caso "perdí el acceso a mi proveedor más antiguo".** El Área 1 dice que "todo proveedor habilitado se puede quitar reautenticándose con él mismo", y eso supone que el usuario todavía controla esa cuenta. Pongamos a una usuaria que entró con Google y después vinculó Discord. Si borra su cuenta de Google o la pierde, **nunca** va a poder quitar Google de Torneo Absurdo, aunque Google esté habilitado: `reauth_with` de Google es `[Google]`. Si alguien más se queda con esa cuenta de Google, esa persona entra y además puede quitar Discord, porque el proveedor más antiguo manda en la cuenta. No es un riesgo nuevo: antes de 009, cualquiera de los dos proveedores podía quitar al otro, que era peor. Pero contradice la frase del Área 1 y lo que se le reporta al owner en el Área 12. Además, es justo el usuario legítimo del que habla la restricción del intent ("solo quiere ordenar sus proveedores"). *Cambio:* corregir el Área 1 y sumar este caso al Área 5 (o a una nueva) con lo que significa: el proveedor más antiguo es la raíz de la cuenta, quien lo pierde no lo puede quitar (pero sigue entrando con el otro) y quien lo controla gana. Además, una línea en el Área 12 para que el owner lo lea junto con la convivencia. No pido otra regla: con lo que ve el server, esto no se distingue del ataque.
  2. **[Minor] RF21: que no haya un "Quitar" que nunca puede funcionar.** `reauth-status` ya trae `ready` y `reauth_with` por proveedor, pero el panel de desvincular solo aparece después de un 403. Si `reauth_with` está vacío (el Área 5), el usuario toca "Quitar", recibe un 403 y recién ahí se entera de que no se puede. *Cambio:* si `reauth_with` está vacío, que la fila muestre de entrada la explicación del Área 5 y no ofrezca "Quitar". Y que eso tenga su prueba: integración (`reauth-status` del proveedor deshabilitado más antiguo, con `reauth_with` vacío, y el `unlink` que da 403) y Vitest o e2e del panel sin botones. Hoy "Estrategia de pruebas" no cubre el Área 5.
  3. **[Minor] Que el owner sepa que tiene que volver a correr `grant`.** El Área 9 lo deja como una nota en el PR, y el Área 6 dice que hay que correr `grant` otra vez después de vincular un proveedor. *Cambio:* que RF25 también actualice la ayuda de la CLI (`server admin grant --help`) o la sección de admin de `docs/architecture.md`, con dos casos: "después de migrar a 009" y "después de vincular un proveedor nuevo como admin".
  4. **[Minor] RF24: decirle al usuario qué hacer con `auth.reauth_mismatch`.** El caso típico es que en el proveedor eligió otra cuenta. *Cambio:* que el texto en `en` y `es` lo diga ("elige la misma cuenta con la que vinculaste ese proveedor") y que ofrezca volver a `/account`, no solo un enlace.
- Lo que no es de este MVP (anotado, no lo pido): el aviso de "vinculado hace poco" y retomar el `unlink` solo al volver. Los dos siguen fuera, como en el intent.

### security-reviewer — 2026-10-30 — REQUEST_CHANGES
**🤖 security-reviewer**

- Verifiqué la spec y la "Enmienda 2026-10-30" de ADR-0008 contra el intent aprobado (criterios, Riesgos aceptados y mis notas de la ronda 2), contra ADR-0005, ADR-0007 y ADR-0012 A4 y A9, y contra el código de `69c2e8a`: `callback.rs` (el flujo se consume con `DELETE … RETURNING` antes del intercambio y la sesión del callback se filtra por `link_user_id`), `link.rs` (bloqueo de `users`, `RevokeFilter::One` antes del `INSERT … ON CONFLICT DO NOTHING`), `unlink.rs`, `20261028000000_auth.sql` y `20261029100000_admin_audit.sql`.
  - **Variantes del criterio 1, cerradas sin relajar.** Con sesión vieja, `link` corta en RF8 y desvincular el único proveedor da 409. Con sesión reciente, la rotación conserva `auth_link_seq`, y la fila de X tiene un `id` mayor, así que la autenticación deja de ser vigente en el acto. Logout y login con X, `logout-all` y login con X, o cualquier espera, dan una autenticación con X, y `X.id > V.id` no cumple `auth_account_id ≤ P.id`. El orden no usa relojes, así que esperar no cambia nada. La reautenticación con X equivale a entrar con X.
  - **Admin (Área 6), cerrado.** Toda identidad que planta el atacante tiene un `id` mayor que `admin_account_cutoff`, así que nunca da `RecentAdmin`, sin importar cuándo ni cómo entre. El corte solo puede quedar bajo: `grant` lee `max(id)` y un `link` concurrente que confirma en medio queda fuera, que es el lado seguro. La regla es más estricta que la letra del Resultado 2, y lo acepto porque elimina el peor caso de la convivencia.
  - **Carrera, cerrada en el diseño.** `link`, `reauth` y `unlink` toman el mismo bloqueo de `users`. En READ COMMITTED, la lectura de la sesión con `FOR UPDATE` y el `max(id)` del paso 2 de RF14 corren después del bloqueo y ven lo que se confirmó en medio. Una identidad borrada en medio llega como `NULL` por la FK.
  - **Orden por `oauth_accounts.id`, correcto.** Todo `INSERT` en las cuentas de un usuario existente corre bajo su bloqueo (`link::attempt`), así que dentro de un usuario el orden de los `id` es el orden de confirmación. `GENERATED ALWAYS` rechaza un `id` explícito salvo con `OVERRIDING SYSTEM VALUE` (ver Minor 2). Volver a vincular da un `id` nuevo y más alto, y la regla de UNIQUE `(user_id, provider)` impide plantar un segundo Google o Discord.
  - **Flujo `reauth` atado al usuario, correcto.** Exige una sesión válida del usuario del flujo en el callback, rota con `RevokeFilter::One` y exige que se haya borrado exactamente esa sesión. Ante `auth.reauth_mismatch` se deshace la transacción, sin crear usuario ni cuenta, y usa un solo código para "no existe" y "es de otro". Un login CSRF en el ida y vuelta choca con la cookie del flujo y con `link_user_id`, y da `auth.invalid_state`. `next` sigue D2, y `?reauth=1` solo se lee por presencia.
  - **Áreas 3 y 4, aceptables como riesgo residual.** El Área 3 es correcta. Un atacante que solo tiene X nunca puede quitar una identidad de la víctima más antigua que X, y la original siempre lo es. Para dejarla fuera necesita una segunda sesión reciente, y eso es parte de la convivencia que sigue pendiente del owner. El Área 4 es un límite de cualquier reautenticación por OAuth sin contraseñas. Como D1 lee la identidad de `userinfo` y no del `id_token`, tampoco hay un `auth_time` que comprobar. Las dos están bien escritas y no relajan el criterio 1.
  - **Logs, ledger e i18n.** `auth.reauth_failed` lleva solo `error_kind`, los canarios cubren el `reauth` exitoso y el de `auth.reauth_mismatch`, y la lista sale de A4. Ningún camino nuevo escribe en `ledger_*`.
- Cambios pedidos:
  1. **[Important] El Área 2 afirma algo falso: en la ventana de 10 min, quien tiene la sesión sí puede quitar una identidad de la víctima y además vincular X.** El Área 2 dice: "No puede hacer las dos cosas: si quita la identidad de la autenticación, la sesión pierde su autenticación vigente". Eso solo vale si quita **la identidad de la autenticación**. *Escenario:* la víctima tiene Google V1 (más antiguo) y Discord V2, y entra con V1. El atacante roba esa sesión en los 10 min siguientes. Quita V2 (`V1.id ≤ V2.id`, y el `max(id)` baja, así que la sesión sigue vigente y reciente), y `unlink` revoca las demás sesiones de la víctima. Después vincula su Discord X (RF8 y RF9 pasan, porque el hueco de Discord quedó libre). Resultado: la víctima pierde V2 y el atacante convive con X, todo con una sola sesión robada. No la deja fuera, porque V1 es más antiguo que X, y el criterio 1 se sostiene. Pero el Área 2, el Área 12 y el "Riesgo residual" de la enmienda describen ante el owner un riesgo menor del real. *Cambio, una de dos:*
     - (recomendada) **Un `unlink` exitoso consume la confirmación de su sesión.** En la misma transacción, `UPDATE sessions SET auth_account_id = NULL` sobre la sesión en uso: es un `UPDATE`, así que D3 no cambia, y el `CHECK` lo permite. Con eso la frase del Área 2 pasa a ser cierta: en la ventana se hace una sola cosa, quitar o vincular. Al usuario legítimo casi no le cuesta nada, porque con dos proveedores reales solo puede quitar uno, y si después quiere vincular otra cuenta confirma, como en el Área 7. Hay que sumarlo a RF14 paso 6, a la enmienda (Desvincular, D1) y a las pruebas: tras un `unlink` exitoso, `link` da `auth.reauth_required` y un segundo `unlink` da 403.
     - o bien corregir el Área 2, el Área 12 y el "Riesgo residual" de la enmienda para que digan este escenario tal cual, y reportárselo al owner junto con la convivencia.
  2. **[Minor] Que el test de fuente de RF1 también prohíba fijar el `id` a mano.** RF1 prueba que ningún SQL hace `UPDATE` de `oauth_accounts.id` ni de `user_id`, pero un `INSERT … OVERRIDING SYSTEM VALUE`, o un `setval` o `ALTER SEQUENCE … RESTART` sobre su secuencia, fuera de la migración, puede hacer que X parezca más antiguo que V. *Escenario:* un cambio futuro "repara" filas con un `id` explícito, y con eso una identidad recién vinculada queda por debajo de `admin_account_cutoff` o de `V.id`. *Cambio:* sumar esas tres formas al test de fuente.
  3. **[Minor] RF3, explícito en el trigger.** Hoy `users_privileged_guard` es `BEFORE INSERT OR UPDATE OF role, banned_at` y en el `INSERT` solo mira `role` y `banned_at`. *Cambio:* que RF3 diga `UPDATE OF role, banned_at, admin_account_cutoff` y que un `INSERT` con `admin_account_cutoff` no `NULL` también lance `TA004`. Si no, un `UPDATE` que solo toca el corte no dispara el trigger.
  4. **[Minor] Que `server admin grant` muestre qué proveedores quedan dentro del corte.** *Escenario:* el atacante plantó X en la cuenta del owner (convivencia) y el owner vuelve a correr `grant`, después de un `revoke` o para habilitar un proveedor (RF17, y el Minor 3 de product-owner). X queda por debajo del corte nuevo y pasa a dar admin de forma permanente, el peor caso que el Área 6 había cerrado. *Cambio:* que `grant` imprima por stdout (no en un log) los nombres de los proveedores, sin `provider_user_id` ni ids, que quedan dentro del corte, y que la ayuda de la CLI pida revisarlos en `/account` antes de confirmar.
  5. **[Minor] Más casos en la prueba de la carrera de `unlink`.** Faltan dos: (e) en la pausa, otra sesión del mismo usuario vincula un proveedor. Esta sesión sigue viva, pero su `auth_link_seq` quedó viejo, así que tiene que dar 403. (f) Un `reauth` que rota esta sesión tiene que dar 401. (e) es el único caso que prueba que la vigencia se calcula en la transacción y no se toma del `SessionInfo` de antes de la pausa.
  6. **[Minor] Área 11: sí, el aviso de privacidad necesita líneas.** En `docs/legal/aviso-de-privacidad.md`, sección 3:
     - "Sesiones": sumar "el proveedor con el que iniciaste sesión o confirmaste tu identidad, y cuándo".
     - "Historial de eventos de seguridad": sumar "desvinculación de proveedor" (ya faltaba desde 008) y "confirmación de identidad" (`auth_events` `reauth`, con seudónimo de IP).
     - "Datos temporales de inicio de sesión": "si vinculas un proveedor **o confirmas tu identidad**, tu identificador interno".

     Que RF25 lo incluya. No es un dato nuevo, pero es un tratamiento que el aviso tiene que declarar.
- Sobre el Important 1 de product-owner (perder el proveedor más antiguo): coincido en que es una consecuencia de la regla y no un riesgo nuevo, y en que no hace falta otra regla. Con la opción recomendada de mi punto 1, ese análisis no cambia.
- Enmienda de ADR-0008: **no la firmo todavía.** La firmo cuando refleje el punto 1, en cualquiera de sus dos opciones. Los Minors 2, 3 y 5 bastan en la spec y el plan.

### architect — 2026-10-30 — RESPUESTA
- **security-reviewer, Important 1:** tomé la opción recomendada. Un `unlink` exitoso consume la confirmación de su sesión (RF14, paso 6, y la Definición de "vigente"), con pruebas: `link` y un segundo `unlink` fallan después. Corregí el Área 2, el Área 12, la respuesta a la pregunta 2 y el "Riesgo residual" y la parte de desvincular de la enmienda de ADR-0008.
- **security-reviewer, Minors 2 a 6:** el test de fuente de RF1 prohíbe `OVERRIDING SYSTEM VALUE`, `setval` y `ALTER SEQUENCE … RESTART`. RF3 fija el trigger `UPDATE OF role, banned_at, admin_account_cutoff` y la regla del `INSERT`. `grant` imprime por stdout los proveedores dentro del corte (RF17). La carrera suma los casos (e) y (f). RF25 agrega las líneas del aviso de privacidad, y el Área 11 lo refleja.
- **product-owner, Important 1:** corregí el Área 1 y agregué el Área 13 (el proveedor más antiguo es la raíz de la cuenta), con una línea en el Área 12 para el owner. No hay regla nueva.
- **product-owner, Minors 2 a 4:** RF21 no ofrece "Quitar" con `reauth_with` vacío, con pruebas de integración y de Vitest. RF17 y RF25 ponen la instrucción de volver a correr `grant` en la ayuda de la CLI y en `docs/architecture.md`, con los dos casos. RF24 fija el texto de `auth.reauth_mismatch` y el botón a `/account`. El Área 5 registra la aceptación de product-owner.

### product-owner — 2026-10-30 — APPROVE (ronda 2)
- Verifiqué solo lo que cambió, contra mis cuatro pedidos y contra el cambio de producto que trajo el Important 1 de security-reviewer:
  1. **Important (el proveedor más antiguo que se pierde): cerrado.** El Área 1 ahora dice "mientras el usuario controle el proveedor". El Área 13 explica que el proveedor más antiguo es la raíz de la cuenta: quien lo pierde no lo puede quitar pero sigue entrando con el otro, y quien lo controla puede quitar los demás. También deja claro que no es peor que antes de 009 y que el remedio (recuperación o soporte) sigue fuera de alcance, como en el intent. El Área 12 lo pone delante del owner junto con la convivencia. Coincide con lo que pedí, y no hay regla nueva.
  2. **Minor RF21: cerrado.** Con `reauth_with` vacío, la fila no ofrece "Quitar" y muestra la explicación desde el principio. Si `reauth-status` no responde, la acción decide. Hay pruebas de integración (`reauth_with` vacío y el 403) y de Vitest (sin botón, con el texto).
  3. **Minor `grant`: cerrado.** La ayuda de la CLI y `docs/architecture.md` cubren los dos casos con el mismo texto (RF17 y RF25), y el Área 9 apunta ahí. Que `grant` imprima los proveedores dentro del corte también le sirve al owner: ve qué está habilitando antes de confiar en él.
  4. **Minor `auth.reauth_mismatch`: cerrado.** RF24 dice qué hacer ("elige la misma cuenta") y pone "Volver a mi cuenta" como acción principal cuando hay sesión. Sin sesión, ofrece el login. Hay Vitest y un e2e con otra identidad de `dev`.
- **El cambio nuevo, desde el producto: aceptable.** Que un `unlink` exitoso consuma la confirmación (RF14, paso 6) le cuesta poco al usuario legítimo. Con dos proveedores reales, solo puede quitar uno, y si después quiere vincular otra cuenta, confirma en el mismo panel (el Área 7). En `/account` queda coherente: después de quitar, `link_ready` es falso, así que en lugar de los botones de vincular aparece el panel. El mensaje "Listo" de RF23 no reaparece aunque `?reauth=1` siga en la URL, porque exige `link_ready`. Con este cambio, lo que el Área 2 le dice al owner es cierto.
- Nada nuevo fuera del MVP. El aviso de "vinculado hace poco" y la recuperación de cuenta siguen fuera.

### security-reviewer — 2026-10-30 — APPROVE (ronda 2)
**🤖 security-reviewer**

- Verifiqué solo lo que cambió, contra mis seis pedidos de la ronda 1:
  1. **Important 1: cerrado, con la opción recomendada.** RF14 paso 6 consume la confirmación (`UPDATE sessions SET auth_account_id = NULL` sobre la sesión en uso) en la misma transacción. La Definición de "vigente" lo refleja, y lo prueba la fila "`unlink` consume la confirmación", con mi escenario V1/V2/X. El Área 2, el Área 12, la respuesta a la pregunta 2 y el "Riesgo residual" de la enmienda dicen ahora "una sola cosa por ventana", y eso es cierto. Revisé la vía que podría saltarse el consumo: empezar un `link` con la confirmación, desvincular V2 y completar el callback. RF9 exige una autenticación vigente dentro de la transacción del callback, y con `auth_account_id = NULL` ya no lo es, así que queda cerrada. El consumo también le quita a la sesión `RecentAdmin` y `link_ready`, que es lo que corresponde.
  2. **Minor 2: cerrado.** RF1 prohíbe `OVERRIDING SYSTEM VALUE`, `setval` y `ALTER SEQUENCE … RESTART`, y su test de fuente recorre `src` y las migraciones posteriores.
  3. **Minor 3: cerrado.** RF3 fija `UPDATE OF role, banned_at, admin_account_cutoff` y que un `INSERT` con el corte no `NULL` lanza `TA004`, con su prueba.
  4. **Minor 4: cerrado.** `grant` imprime por stdout los nombres de los proveedores dentro del corte, sin ids. Un test de canarios (`admin_cli_logs`) comprueba que no llegan a un log, y la ayuda indica qué hacer con uno que no se reconoce. También está en la enmienda.
  5. **Minor 5: cerrado.** La carrera suma (e), que es la que prueba la vigencia en la transacción, y (f).
  6. **Minor 6: cerrado.** RF25 agrega las tres líneas a la sección 3 del aviso de privacidad, y el Área 11 lo refleja.
- **Áreas 1 y 13 (pedido de product-owner):** desde seguridad son correctas. Que el proveedor más antiguo sea la raíz de la cuenta es consecuencia de la regla, y no se puede relajar sin reabrir el criterio 1.
- No encontré problemas de seguridad nuevos. El criterio 1 y el de admin siguen sin relajar. La convivencia sigue pendiente del owner, más chica que en el intent.
- Nota para el plan (no bloquea): sumar a la prueba del consumo la variante "`link` empezado con la confirmación, después un `unlink` exitoso y después el callback": el callback da 303 `auth.reauth_required` y no crea ninguna fila. Es la única forma de usar una sola ventana para quitar una identidad y vincular otra.
- Firmo la Enmienda 2026-10-30 de ADR-0008.

### architect — 2026-10-30 — ERRATA
Tres correcciones editoriales que registran lo que ya aprobó el plan (`plan.md`, "Revisión"). No cambian ningún requisito. Las revisiones firmadas de arriba quedan como estaban.
1. **"Flujo": el éxito de `unlink` es 303, no 200.** El diagrama pasa a "303 a /". Es el contrato de 008, y RF13 y RF14 nunca cambiaron la respuesta de éxito (respuesta 1 de architect).
2. **RF25: "el mismo texto" pasa a "el mismo contenido".** La ayuda de la CLI va en inglés y `docs/architecture.md` en español (respuesta 5 de architect).
3. **"Estrategia de pruebas", criterio 2: los proyectos del e2e.** Decía "Chromium y WebKit, escritorio y 375 px". Pasa a `chromium`, `chromium-mobile` y `webkit-mobile`, porque `webkit` de escritorio no corre `auth/` y el plan no cambia `playwright.config.ts` (punto 5 de frontend-engineer, aceptado por architect). Lo que pide el intent, Chromium y WebKit a 375 px, no cambia.

### architect — 2026-10-30 — ERRATA (riesgos residuales)
Corrige cómo se describen los riesgos, a pedido de security-reviewer en el PR 57 (https://github.com/kike1303/torneo-absurdo/pull/57#issuecomment-6537072388). No cambia ningún requisito ni agrega reglas: no se toca ningún RF, ninguna prueba ni ningún criterio. Las revisiones firmadas de arriba quedan como estaban. **Ninguno de estos riesgos queda aceptado:** el owner no los aceptó, y nadie lo hizo en su nombre. Yo escribí esta errata, así que no la valido: security-reviewer la relee. Los dos encadenamientos que describe los corrió security-reviewer con proveedores simulados, y yo no los volví a correr.
1. **Área 4: la consecuencia.** Con el proveedor abierto, el navegador desatendido lleva a la toma de cuenta completa de TA-29, sin segundo robo. Con un admin, también pasa `RecentAdmin`. Forzar `prompt` solo agrega un clic.
2. **Área 12: la convivencia.** Decía que con el punto 6 X "ya no da admin". Pasa a "con solo X, nunca", con tres excepciones: el segundo robo que deja fuera a la víctima, esa misma sesión pasando `RecentAdmin` y un `grant` posterior con X vinculado. Suma lo que X puede hacer mientras conviva (gastar monedas vale desde que existan las apuestas, porque hoy ninguna ruta las gasta) y que la ventana también corre tras una reautenticación. Que el atacante puede quitar otra identidad en lugar de vincular X ya estaba en las Áreas 2 y 12.
3. **Área 2, Área 6 y respuesta 2.** Tenían la misma afirmación absoluta. "La víctima nunca queda fuera" pasa a valer "con solo esa sesión", y "desaparece el peor caso" suma la excepción del `grant`.
4. **Área 5: el matiz.** "No es un riesgo de seguridad" pasa a valer para el usuario legítimo. Mientras V, el más antiguo, esté deshabilitado, una víctima cuyo único otro proveedor sea X no puede iniciar sesión ni quitar X hasta que V vuelva. La espera del usuario legítimo sigue sin pedir aceptación del owner. El matiz va con la convivencia, pendiente del owner, y la aceptación de product-owner no lo cubre.
5. **Área 13: sin cambios.** Ya dice las dos mitades: quien pierde el proveedor más antiguo no lo puede quitar, y quien lo controla entra a la cuenta y puede quitar los demás.
6. **ADR-0008, en el mismo commit.** Nota en D2, "D1, D2, D3 y D9" en el "Estado" de la Enmienda 2026-10-30, puntero en la lista de seis códigos de la Enmienda 2026-10-28 y dos frases en el "Riesgo residual": la toma de cuenta con el proveedor abierto, y que "nunca de admin" vale con solo X. Esas dos frases son posteriores a la firma de security-reviewer del 2026-10-30.

### owner — 2026-10-30 — riesgos residuales aceptados

La sesión coordinadora le preguntó "¿Aceptas estos riesgos residuales para cuando haya deploy?", después de listarlos, y el owner contestó en el chat: "Los acepto". Cubre las Áreas 3, 4, 5 (el matiz con X), 12 y 13 tal como están descritas en esta spec y en el PR #57. No relaja ningún criterio y no autoriza ningún tag `v*` ni deploy. Lo registra la sesión coordinadora.
