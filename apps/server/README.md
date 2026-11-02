# apps/server

El server de Torneo Absurdo: API, WebSockets, autenticación y, más adelante, el scheduler de peleas. Cómo se corre en desarrollo está en el [`CLAUDE.md`](../../CLAUDE.md) de la raíz (`just dev`) y la arquitectura en [`docs/architecture.md`](../../docs/architecture.md). Este archivo cubre dos tareas manuales de la feature 008: el login real en local y la CLI de admin.

## Login real en local

En `just dev` el proveedor `dev` alcanza para todo. Para probar Google y Discord de verdad (RF-C10 de la [spec 008](../../docs/features/008-autenticacion/spec.md)) hace falta una app OAuth de desarrollo de cada uno.

**Las credenciales nunca se commitean ni se pegan en un issue, un PR o un chat: el repo es público.** Van solo en `apps/server/.env`, que git ignora.

### Google

1. En Google Cloud Console, crea (o elige) un proyecto y entra a "APIs y servicios" → "Credenciales".
2. Configura la pantalla de consentimiento en modo **de prueba**, con tus cuentas como usuarios de prueba y **solo** el scope `openid`. No agregues `email` ni `profile`: el server no los pide ni los lee.
3. Crea un "ID de cliente de OAuth" de tipo **Aplicación web**, con este URI de redirección autorizado, exacto:

   ```
   http://localhost:5173/api/auth/google/callback
   ```

### Discord

1. En el Discord Developer Portal, crea una aplicación y entra a "OAuth2".
2. Agrega este redirect, exacto:

   ```
   http://localhost:5173/api/auth/discord/callback
   ```

3. El server pide solo el scope `identify`; no hace falta marcar ningún otro.

### Variables

En `apps/server/.env` (copiado de `.env.example`), descomenta las cuatro líneas y pon los valores de las dos apps:

```
GOOGLE_CLIENT_ID=…
GOOGLE_CLIENT_SECRET=…
DISCORD_CLIENT_ID=…
DISCORD_CLIENT_SECRET=…
```

- Cada proveedor necesita sus **dos** variables o ninguna. Un par a medias no arranca (`config.invalid` con la variable que falta), para que un error de tipeo no apague un botón en silencio.
- `localhost` y `127.0.0.1` **no** son intercambiables: el URI registrado tiene que coincidir con `PUBLIC_ORIGIN` (`http://localhost:5173` en el `.env.example`).
- Corre `just dev` y comprueba con `curl -s http://127.0.0.1:8080/api/auth/providers` que aparecen `google` y `discord` (y `dev`).

### Prueba manual

Con dos cuentas de Google o de Discord a mano:

1. Login con Google; logout; login con Discord (es otro usuario).
2. Desde la sesión de Google, vincular Discord: vuelves con una sesión nueva, y `/api/me` lista los dos proveedores.
3. Desvincular Discord: la web pide primero confirmar tu identidad, porque lo que vinculaste en el paso 2 es posterior a tu autenticación. Después de confirmar, la sesión actual sigue y las demás se cierran.
4. Vincular una cuenta que ya es de otro usuario. El desvincular del paso 3 consumió la confirmación, así que `/account` muestra otra vez el panel en lugar de los botones de vincular: confirma tu identidad y vincula. Resultado: `/login?error=auth.already_linked`, sin fusión.
5. Volver a `/account`, empezar a vincular y cancelar en la pantalla del proveedor: `/login?error=auth.provider_error`. El intento fallido del paso 4 no consume la confirmación: los botones siguen ahí mientras la confirmación tenga menos de 10 minutos. Si ya pasaron, la web vuelve a pedirla.

## Verificar una pelea con datos públicos

Cualquiera puede comprobar una pelea terminada solo con `GET /api/fights/{id}` (ADR-0002, RF7 de la spec de 010), que da `fight`, `spec_a`, `spec_b`, `seed` y `log`:

1. **El commit:** `SHA-256(bytes de seed)` es igual a `fight.seed_commit`. Para quien archivó el commit al abrir las apuestas, eso prueba que la semilla es la de entonces.
2. **El snapshot:** `SHA-256({"ruleset_version":N,"a":<spec_a>,"b":<spec_b>})`, con esos bytes exactos y sin espacios, es igual a `fight.snapshot_sha256`.
3. **La pelea:** simular con `seed`, `spec_a`, `spec_b` y `Ruleset::for_version(fight.ruleset_version)`, con `engine::simulate` o `engine_wasm::run`, y el `canonical_json` del resultado es igual a `log`, byte a byte.

Una `cancelled` no tiene semilla ni log. Una `voided` tiene semilla y no tiene log. `apps/server/tests/fights_public_verify.rs` lo comprueba de esta misma forma, sin importar nada de `server`.

## Admin y ban por CLI

El rol `admin` y los bans solo se cambian con la CLI del binario (RF-C6). Cada comando, en una transacción, cambia el usuario, cierra todas sus sesiones (sus sockets se cierran al instante) y escribe una fila en `admin_audit`.

```
server admin grant  <user_id>
server admin revoke <user_id>
server admin ban    <user_id> --reason <texto>
server admin unban  <user_id>
```

- En local: `cargo run -p server -- admin grant <user_id>`, con el `DATABASE_URL` de `apps/server/.env` en el entorno. La CLI lee solo `DATABASE_URL` y `RUST_LOG`.
- En la imagen: `docker exec <contenedor> /server admin ban <user_id> --reason <texto>`.
- Para encontrar un `user_id`, por ejemplo por su nombre visible: `psql "$DATABASE_URL" -c "SELECT id FROM users WHERE display_name = '…'"`.
- Sale con 0 si aplicó el cambio, con 1 y `admin.user_not_found` si el usuario no existe (sin cambios), y con 2 y `cli.invalid` si el comando no se entiende.
- `server admin --help` y `server admin <acción> --help` imprimen la ayuda por stdout y salen con 0, sin leer `DATABASE_URL` ni conectar. Cualquier otra forma (`-h`, o `--help` después del `user_id`) es un comando que no se entiende.
- Los logs de la CLI llevan solo el evento. Ni el `user_id` ni el motivo aparecen en ellos: el motivo queda en `admin_audit.payload`.
- **El `--reason` queda en el historial de la shell y, con `docker exec`, en los eventos `exec_create` del daemon de Docker, junto con el `user_id`. Por eso nunca lleva datos personales:** basta con algo como `spam en el chat`.
- Un `UPDATE users SET role …`, `banned_at …` o `admin_account_cutoff …` a mano por `psql` falla con `TA004` (`users_privileged_guard`). El camino es la CLI, así el cambio queda en `admin_audit`.
- Un admin tiene sesiones de 12 h. Las rutas de admin (TA-13) piden una autenticación vigente (la cuenta sigue vinculada y no se vinculó nada después), de hace menos de 60 min y con una cuenta dentro del corte de `grant` (`admin_account_cutoff`).

### Cuándo volver a correr `grant`

`grant` guarda como corte la cuenta vinculada más reciente del usuario, y `revoke` lo borra. Hay que volver a correr `grant`:

- después de migrar a 009: ningún admin tiene corte todavía, así que ninguna sesión pasa como admin;
- después de vincular un proveedor nuevo como admin: no da admin hasta otro `grant`.

Antes de correrlo, revisa los proveedores vinculados en `/account`: todo lo vinculado hasta ese momento pasa a dar admin. Como cada comando cierra todas las sesiones del usuario, después hay que volver a entrar.

Al terminar, `grant` imprime por stdout (no es un log) los proveedores dentro del corte, en orden de vínculo y sin ids:

```
providers within the admin cutoff: google, dev
```

Si el usuario no tiene cuentas, la línea dice `none`. Si muestra un proveedor que no reconoces, corre `revoke`, quítalo desde `/account` y vuelve a correr `grant`.

## Anular una pelea por CLI

`server fight void` es la salida de una pelea que no puede terminar: reembolsa todo y la deja en `Settled` con el final `voided`. El runner nunca lo llama y no hay endpoint. Detalle en la [spec de 012](../../docs/features/012-apuestas/spec.md), RF16 a RF24.

```
server fight void <fight_id> --by <admin_user_id> [--previous-binary <vX.Y.Z|sha-abc1234>] [--backup-checked]
```

- En local: `cargo run -p server -- fight void 12 --by <admin_user_id>`. En la imagen: `docker exec <contenedor> /server fight void …`. Lee solo `DATABASE_URL` y `RUST_LOG`.
- **La razón se deduce, nunca se elige.** `--by` tiene que ser un `admin`, pero solo atribuye: quien corre el comando ya tiene la credencial de la base.
- Corre en una transacción, con el final, el reembolso y una fila de `admin_audit` (`action = 'fight_void'`), y bloquea la fila de la pelea con `lock_timeout` de 5 s.
- Todo el comando tiene un plazo de 120 s: una base que deja de responder termina en `fight.void_failed` con `error_kind = timeout`, no en un proceso colgado. Una anulación de 20 000 apuestas tardó unos 4 s en local. Si el plazo cae durante el `COMMIT`, la anulación pudo aplicarse: vuelve a correr el comando, y `void.bad_state` lo confirma sin reembolsar dos veces.

**Salidas.** Una línea de texto fijo en stdout, sin semilla ni datos del log, y el evento con su código.

| Exit | Salida | Evento | Qué significa |
|---|---|---|---|
| 0 | `void.applied <razón>` | `fight.void_applied` | Aplicó. Tras un `secret_lost`, además `void.reopen_acceptance …` |
| 1 | El código de abajo | `fight.void_refused` | Decidió que no. No escribió nada |
| 1 | Ninguna | `fight.void_failed`, con `error_kind` | No decidió nada: la base no respondió o se pasó el plazo (`timeout`), otra transacción retiene la fila (`lock_timeout`), o falló el reembolso. Se puede reintentar |
| 2 | Ninguna | `cli.invalid` | La línea de comandos no se entiende |

**Negativas, en este orden:**

| Código | Qué pasa | Salida |
|---|---|---|
| `void.not_admin` | `--by` no existe o no es admin | Usa un admin (`server admin grant`) |
| `void.not_found` | La pelea no existe | |
| `void.bad_state` | No está en `betting_closed` ni en `live` | |
| `void.version_missing` | Este binario no tiene su versión de reglas | Vuelve a la versión anterior |
| `void.secret_mismatch` | La fila del secreto no coincide con el commit. **Nunca es `secret_lost`** | Investigar: alguien escribió esa fila |
| `void.emission_possible` | No hay secreto, y aún puede haber una emisión en curso | Espera el plazo: `live_started_at` (o `live_at`) + duración máxima + 5 s + 120 s, 215 s con los valores por defecto |
| `void.backup_not_checked` | No hay secreto, ya pasó el plazo y falta `--backup-checked` | Intenta restaurar desde un backup |
| `void.binary_problem` | Hash y slugs bien, pero no decodifica, no pasa `validate` o difieren las columnas derivadas | Vuelve a la versión anterior |
| `void.fight_healthy` | El motor reconstruye el log | No se anula. Una pelea que no puede liquidar tampoco sale por aquí |
| `void.no_end` | El log no tiene `end` | Se niega por defecto: es un binario defectuoso |
| `void.previous_binary_not_tried` | El motor falla, y falta `--previous-binary` | Pruébalo con la versión anterior |

**Razones:** `snapshot_invalid` (hash o slugs distintos, sobre los bytes), `engine_error` (`EventLimit`, `encode_failed` o un pánico, con `--previous-binary`) y `secret_lost`. Las dos primeras publican la semilla.

**Declaraciones (RF24).** Nada comprueba que sean ciertas: quedan en `admin_audit`, y con un solo operador es lo que se puede exigir.

- `--previous-binary <tag>`: declara que el binario anterior también falla. Lo hace refutable que la semilla sale publicada y cualquiera puede resimular.
- `--backup-checked`: declara que se intentó restaurar el secreto desde un backup. **No lo restaures mientras corres el comando:** un `INSERT` en `fight_secrets` en paralelo podría confirmar después, y dejar un secreto en una pelea anulada.

### Tras un `secret_lost`

Esa anulación no se puede refutar nunca. El comando imprime un aviso, pero no puede comprobar que lo sigas:

1. Abre un issue en Linear con el id de la pelea.
2. Pídele al owner que vuelva a decidir sobre la preocupación 1 de la spec de 010.

La pelea sale en el contador público `GET /api/fights/void-stats`, y la vista la muestra como "no se puede verificar".

### Antes de volver a una imagen anterior

La migración es compatible hacia atrás, pero **los datos no** (ADR-0006 D10, "El esquema no es todo"). Una imagen anterior a esta fase no entiende una fila con `void_reason = 'secret_lost'`: esa pelea deja de tener resumen, así que `GET /api/fights/{id}` responde 503, y si es la última liquidada, `last` del tablero queda en `null`, con `/api/health` y `verify` en verde. Antes de un rollback, compruébalo:

```
psql "$DATABASE_URL" -c "SELECT id FROM fights WHERE void_reason = 'secret_lost'"
```

Con alguna fila, no vuelvas a esa imagen sin decidir antes qué hacer con esas peleas: no se reescriben, porque una fila `settled` no admite `UPDATE`.

### Nunca volver a un binario sin liquidación de apuestas

`app::serve` construye `BetsHook`. `NoBets` queda solo para las pruebas del ciclo sin apuestas. **No vuelvas a un binario con `NoBets` si hay apuestas sin liquidar:** cerraría peleas sin sus asientos. Mientras no exista el reparto (fase B), un `decided` con apuestas en los dos lados devuelve un error y lleva a la detención, nunca a una pelea `settled` sin asientos. Ese caso no tiene camino sin la API, y solo se daría volviendo de B a A.
