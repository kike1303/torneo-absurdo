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

## Lista de peleas terminadas

`GET /api/fights` da `{ fights, next_before }`: hasta 20 peleas `settled`, de `id` mayor a menor (spec de 011, RF7). `fights` son `FightSummary`, sin semilla ni log, y `next_before` es el `id` de la última si hay más, o `null`. `?before=<id>` da la página siguiente; un valor que no sea un `id` válido da 404 `fight.not_found`. El tamaño de página es fijo. Sin sesión, con `Cache-Control: no-store` y la misma cuota por IP que `GET /api/fights/{id}` (`FIGHT_READ_IP`).

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
- **La razón se deduce, nunca se elige.** `--by` tiene que ser un `admin` sin ban (la misma regla de `server season`), pero solo atribuye: quien corre el comando ya tiene la credencial de la base.
- Corre en una transacción, con el final, el reembolso y una fila de `admin_audit` (`action = 'fight_void'`), y bloquea la fila de la pelea con `lock_timeout` de 5 s.
- Todo el comando tiene un plazo de 120 s: una base que deja de responder termina en `fight.void_failed` con `error_kind = timeout`, no en un proceso colgado. Una anulación de 20 000 apuestas tardó unos 4 s en local. Si el plazo cae durante el `COMMIT`, la anulación pudo aplicarse: sigue "Tras un `fight.void_failed`". Volver a correr el comando nunca reembolsa dos veces.

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
| `void.not_admin` | `--by` no existe, no es admin o tiene ban | Usa un admin (`server admin grant`) |
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

### Tras un `fight.void_failed`

El comando no sabe si su `COMMIT` llegó, y `void.bad_state` no lo responde: solo dice que la pelea ya está `settled`, también si la liquidó el runner. Lo que pasó está en la fila de la pelea y en su auditoría:

```
psql "$DATABASE_URL" -c "SELECT state, void_reason FROM fights WHERE id = <fight_id>" \
  -c "SELECT action, payload FROM admin_audit WHERE target = '<fight_id>' AND action = 'fight_void'"
```

- Sin fila en `admin_audit` y sin `settled`: no aplicó. Vuelve a correr el comando.
- Con fila en `admin_audit`: aplicó. Si `void_reason` es `secret_lost`, **sigue el apartado de abajo**: el aviso pudo no imprimirse nunca. Volver a correr el comando también lo repite.

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

`app::serve` construye `BetsHook`. `NoBets` queda solo para las pruebas del ciclo sin apuestas. **No vuelvas a un binario con `NoBets` si hay apuestas sin liquidar:** cerraría peleas sin sus asientos. Tampoco a una imagen de la fase A con apuestas de dos lados abiertas: esa imagen no reparte, devuelve `payout_unavailable` y detiene las peleas.

## Revisar y cerrar una temporada

Una temporada es un mes calendario UTC (`id = YYYYMM`). Al terminar, se revisa con `farm-report`, se excluye a mano a quien lo merezca y se cierra. No hay endpoint. Detalle en la [feature 020](../../docs/features/020-temporadas-leaderboard/feature.md) y en ADR-0008 D8.

```
server season farm-report <YYYYMM> --by <admin_user_id>
server season exclude <YYYYMM> <user_id> --by <admin_user_id> --reason <farm_signals|shared_ip_opposite_sides|counterpart_concentration>
server season close <YYYYMM> --by <admin_user_id> [--accept-partial]
```

- En local: `cargo run -p server -- season farm-report 202611 --by <admin_user_id>`. En la imagen: `docker exec <contenedor> /server season …`. Leen solo `DATABASE_URL` y `RUST_LOG`.
- `--by` tiene que ser un `admin` sin ban. Cada orden corre en una transacción con `lock_timeout` de 5 s y un plazo de 120 s, y deja una fila en `admin_audit` (`season_review`, `season_exclude` o `season_close`).
- **Ventana:** desde `ends_at`, y córrelo pronto. La cobertura se mide desde un día antes de `starts_at` y la retención es de 38 días, así que en un mes de 31 días una revisión `full` solo es posible hasta `ends_at + 6 d`. Después el reporte sale con `coverage: "partial"` y `close` pide `--accept-partial`.

**Procedimiento.**

1. `farm-report`: imprime una línea de JSON, solo con códigos, ids y conteos:
   - `shared_ip_opposite_sides`: las peleas donde cuentas del mismo `ip_group` apostaron en lados opuestos.
   - `counterpart_concentration.top`: el top 20 del leaderboard. Para cada usuario muestra cuánto de su `RankedProfit` salió de sus 5 contrapartes principales (`concentration_bp`) y si comparte grupo con alguna. `flagged` se marca con más de 5000 bp o con un grupo compartido.
   - `ip_group` es una etiqueta opaca de ese reporte (1, 2…), nunca el `ip_hash`.
2. **Un grupo compartido no basta para sancionar** (CGNAT móvil). Hace falta, además, lados opuestos y concentración, o una evidencia equivalente. Para sancionar: `exclude`. Se puede excluir con la temporada abierta, y la exclusión no se deshace.
3. `close`: suma las peleas liquidadas sin marca y vuelve a comprobar que no queden pendientes. Usa la `season_review` más reciente de esa temporada con `at ≥ ends_at` y de las últimas 48 h. Si excluiste a alguien, corre `farm-report` otra vez antes de cerrar: el top 20 cambió.

**Salidas.** Exit 0 imprime el reporte, `season.exclude_applied` o `season.close_applied`, con el evento `season.applied`. Exit 1 con un código imprime una negativa (`season.refused`) y no escribe nada. Exit 1 sin salida es `season.failed`, con `error_kind`: no decidió nada y se puede reintentar. Exit 2 es una línea de comandos inválida.

| Código | Órdenes | Qué pasa |
|---|---|---|
| `season.not_admin` | todas | `--by` no existe, no es admin o tiene ban |
| `season.not_found` | todas | La temporada no tiene fila (ninguna pelea suya se liquidó) |
| `season.not_ended` | `farm-report`, `close` | Todavía no llega `ends_at` |
| `season.pending_fights` | `farm-report`, `close` | Hay una pelea de la temporada sin liquidar, o liquidada sin marca en `season_tallies`. La reconciliación del job diario la suma; espera o reinicia el server |
| `season.closed` | `exclude` | La temporada ya está cerrada |
| `season.no_stats` | `exclude` | El usuario no tiene fila en la temporada |
| `season.already_excluded` | `exclude` | Ya estaba excluido: la primera exclusión se queda como está |
| `season.no_review` | `close` | No hay `season_review` desde `ends_at` y de las últimas 48 h: corre `farm-report` |
| `season.partial_review` | `close` | La revisión es `partial`: ciérrala con `--accept-partial` solo si la revisaste igual. El flag queda en `admin_audit` |
| `season.already_closed` | `close` | Ya estaba cerrada |

## Apuestas

Las apuestas (feature 012) son solo con monedas ficticias (ADR-0005). Los valores están en RF40: `DAILY_GRANT` es `B`, y `BET_MIN`, `BET_RAKE_BP` y `BET_CAP_MULTIPLE` son opcionales (ver `.env.example`). El compose de producción no pasa `BET_*`.

### Peleas PvP (feature 021, fase 2)

`POST /api/pvp/fights` con `{"client_key":"<uuidgen en minúsculas>"}` resuelve al instante una pelea contra la botarga de otro usuario (o contra un personaje del torneo de `CONTENT_DIR` si no hay nadie). La victoria del atacante contra un usuario paga `PVP_WIN_COINS` (15 por defecto, opcional, de 0 a 2000; 0 las apaga) por el escalón del día, como un asiento `pvp_reward` de `issuance`. Un usuario ataca como mucho 50 veces por día UTC (429 `pvp.daily_limit`). No pagan: una derrota, un empate, la defensa, el torneo, la cuarta pelea del día de una pareja ni una pelea con un lado en carencia PvP (menos de 7 días o de 10 ataques). El compose de producción no pasa `PVP_WIN_COINS`.

Para ver lo que pagó el PvP, de solo lectura:

```sh
psql "$DATABASE_URL" -c "SELECT date_trunc('day', e.created_at) AS day, count(*), sum(p.amount) \
  FROM ledger_entries e JOIN ledger_postings p ON p.entry_id = e.id \
  WHERE e.kind = 'pvp_reward' AND p.account_id <> '00000000-0000-0000-0000-000000000001' \
  GROUP BY 1 ORDER BY 1 DESC LIMIT 14"
```

### Comprobar la custodia

Una consulta de solo lectura, en **una sola sentencia** para que una liquidación a medias no dé un falso positivo (RF13). Sana, `escrow` y `open_stakes` coinciden, y las otras tres dan 0. La consulta está en [`sql/custody.sql`](sql/custody.sql), y un test la ejecuta tal cual (`tests/bets_custody.rs`).

```sh
psql "$DATABASE_URL" -f apps/server/sql/custody.sql
```

**Si no da limpia:** no la corrijas con SQL. Detén el server, guarda la salida y la hora, y abre un issue en Linear con lo que diga cada columna. `escrow` distinto de `open_stakes` con las otras en 0 apunta a un pago de más o de menos en una liquidación; `bets_without_stake` o `wrong_refunds` distintos de 0, a una escritura que no pasó por la API ni por el gancho. Quien tiene `DATABASE_URL` también podría haberlo hecho (preocupación 21 de la spec).

### Atasco de una liquidación (RF15)

El gancho corre dentro de la transacción del reveal, con un `attempt_timeout` de 30 s. Si no cabe, reintenta, y tras 60 s de fallos el runner detiene todo, mientras `fight void` responde `fight_healthy`. En una detención la ventana no acepta apuestas, así que el pozo ya no crece. Salida:

1. Quita la contención (otros procesos sobre la base) y reinicia el server.
2. Si no basta, despliega un binario con un `attempt_timeout` mayor.

### Métrica del rake (RF40)

El rake empieza en 0. Su objetivo es 500 pb, con esta métrica por debajo del 10 % en 7 días móviles, y se revisa a los 90 días de la primera apuesta en producción aunque no se cumpla:

```sql
SELECT count(*) FILTER (WHERE outcome = 'refund_one_sided')::float
       / NULLIF(count(*) FILTER (WHERE outcome IN ('paid', 'refund_one_sided')), 0) AS one_sided
FROM pool_settlements WHERE settled_at > now() - interval '7 days';
```

**Activar el rake** es decisión del owner, con un PR que agrega `BET_RAKE_BP` al compose y a `check-compose`. Rige para las peleas que se anuncien después: cada pelea guarda el suyo al anunciarse.

### Probar en local con dos clientes HTTP

Con `just dev` y una rejilla corta en `apps/server/.env` (RF11 de 010 la acepta):

```
FIGHT_CADENCE_SECS=180
FIGHT_BETTING_WINDOW_SECS=60
FIGHT_PRE_LIVE_SECS=10
```

Dos cuentas con el proveedor `dev`, cada una con su archivo de cookies, por el proxy de Vite:

```sh
BASE=http://localhost:5173
for who in alice bob; do
  curl -sS -L -o /dev/null -c $who.jar -b $who.jar -H "Origin: $BASE" \
    -d "dev_identity=$who&next=/es/" "$BASE/api/auth/dev/start"
done
```

Después, con `-b <quien>.jar -H "Origin: $BASE"`:

1. `GET /api/wallet`: saldo `B` y el bono ya reclamado por el login.
2. `GET /api/fights/current`: el `id` de la pelea en `betting_open`.
3. `POST /api/bets` con `{"fight_id":…,"side":"a","amount":100,"client_key":"<uuidgen en minúsculas>"}` para alice y `"side":"b"` para bob.
4. `GET /api/fights/{id}/quote?side=a&amount=100` y `GET /api/fights/{id}/pool`.
5. Esperar `settled` y leer `GET /api/bets/mine?fight_id={id}` y `/api/wallet` en las dos cuentas: pago y pérdida, o reembolso si fue empate.
