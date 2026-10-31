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
3. Desvincular uno: la web pide primero confirmar tu identidad, porque lo que vinculaste en el paso 2 es posterior a tu autenticación. Después de confirmar, la sesión actual sigue y las demás se cierran.
4. Vincular una cuenta que ya es de otro usuario: `/login?error=auth.already_linked`, sin fusión.
5. Cancelar en la pantalla del proveedor: `/login?error=auth.provider_error`.

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
