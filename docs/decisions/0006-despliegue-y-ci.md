---
id: "0006"
title: Despliegue en Coolify y CI con presupuesto cero
type: adr
status: approved
risk: medium
author: architect
reviewers: [devops-engineer, owner]
approvals:
  - {by: devops-engineer, date: 2026-10-21}
  - {by: owner, date: 2026-10-21}
supersedes: null
linear: TA-5
depends_on: ["0002", "0003", "0005"]
created: 2026-10-20
updated: 2026-10-21
---

# ADR-0006: Despliegue en Coolify y CI con presupuesto cero

## Contexto
- VPS Hostinger KVM 1: **1 vCPU, 4 GB de RAM (~1.6 GB ya en uso por Coolify y otras cargas), 50 GB de disco (22 usados), 4 TB de transferencia.** Ya tiene Coolify v4 con su proxy Traefik, compartido con otras apps del owner.
- Presupuesto: 0. Ningún servicio de pago; un servicio externo gratuito requiere el OK del owner (`docs/process.md`).
- **El repo es público** (GitHub, AGPL-3.0). Esto cierra la pregunta "público o privado" y fija tres hechos:
  - Los minutos de Actions en runners estándar son **ilimitados** (y el runner `ubuntu-latest` de repos públicos tiene 4 vCPU). La disciplina de CI ya no es de presupuesto sino de **higiene y velocidad**: que un PR tenga respuesta rápida y que nada corra de más.
  - Los paquetes de GHCR pueden ser públicos: pull sin login y sin la cuota de 500 MB / 1 GB/mes de los privados.
  - Cualquiera puede abrir un PR desde un fork. Todo workflow que corra con código de un PR es **código no confiable**.
- El ledger (ADR-0005) es la fuente de los saldos y es inmutable: perder la base de datos no tiene arreglo sin backups.
- Las peleas son en vivo y con apuestas (ADR-0002): un reinicio del server corta los WebSockets y puede caer en una ventana de apuestas o en una pelea `Live`. La semilla está en la BD, así que una pelea se puede reanudar tras reiniciar.
- Un solo proceso `server`: el estado en memoria (rate limiter, registro de sockets de ADR-0008) no permite correr dos instancias a la vez.

## Decisión

### D1. Topología en Coolify
- **Postgres es un recurso de base de datos standalone de Coolify** (`postgres:17-alpine`), no un servicio del compose. Sin puerto público ("Make it publicly available" apagado).
- **Coolify despliega `infra/docker-compose.yml` con solo `web` y `server`**, ambos con `image:` y sin `build:`. El compose se lee de `main`; el **auto-deploy por push de Coolify queda apagado** (el despliegue lo dispara D6).
- **Redes (fuente de verdad de la red y del proxy; ADR-0008 remite aquí).** "Connect to Predefined Network" queda **apagado**: la red `coolify` es compartida con las demás apps del VPS, y en ella cualquier contenedor vecino llega a nuestros puertos y comparte el espacio de alias DNS (un servicio ajeno llamado `server` recibiría, por reparto de DNS, la cookie `__Host-session` y `X-Internal-Secret` del SSR). En su lugar:
  - **`torneo-edge`**: red bridge **externa y dedicada**, creada una vez en el VPS con subred fija y chica: `docker network create --driver bridge --subnet 10.250.0.0/28 --gateway 10.250.0.1 torneo-edge` (antes se comprueba con `docker network inspect` que la subred no se cruza con ninguna existente). Solo tres miembros: `coolify-proxy` con IP fija `10.250.0.2`, `web` y `server`.
  - El proxy se une a `torneo-edge` desde la configuración del proxy en Coolify (Servers → Proxy → Configuration): red externa `torneo-edge` con `ipv4_address: 10.250.0.2`. Es un cambio en el proxy compartido (Pendiente del owner) y debe sobrevivir a reinicios del proxy y a actualizaciones de Coolify (verificación 15).
  - `web` está **solo** en `torneo-edge`.
  - `server` está en `torneo-edge` y además en la red donde vive el Postgres standalone, **solo para salir hacia la BD**. Preferido: un *destination* propio de Coolify (`torneo-db`) para el recurso de Postgres, de modo que esa red solo tenga Postgres, el server y lo que Coolify conecte por sí mismo. Si Coolify no permite que el server se una a ese destination, se usa la red `coolify`; en ese caso los vecinos alcanzan `server:8080`, pero no pueden falsificar la IP (su peer no está en `TRUSTED_PROXIES`, D2) ni pasar por internos (no tienen el secreto).
  - Traefik **no** necesita alcanzar a Postgres; solo el server.
  - Si Coolify agrega por su cuenta una red propia del stack (nombrada por el UUID del recurso), en ella solo pueden estar `coolify-proxy`, `web` y `server`; lo comprueba la verificación 15.
  ```yaml
  networks:
    torneo-edge: { external: true }
    torneo-db:   { external: true }   # o `coolify`, ver arriba
  services:
    web:    { networks: [torneo-edge] }
    server:
      networks:
        torneo-edge: { aliases: [torneo-server] }
        torneo-db: {}
  ```
- `DATABASE_URL`, `IP_HASH_KEY`, el secreto interno y los secretos de OAuth (ADR-0008) son variables de entorno de Coolify; nunca están en el repo (solo `infra/.env.example` con valores de ejemplo).
- `server` escucha en `:8080` y `web` en `:3000`, sin `ports:` publicados en el host. `web` llama al API por **`INTERNAL_API_URL=http://torneo-server:8080`** (ADR-0008 D5): un alias propio y único en `torneo-edge`, que resuelve a un solo contenedor (verificación 15). No se usa el nombre genérico `server`.
- `restart: unless-stopped` y rotación de logs de Docker en ambos servicios: `logging.options: { max-size: "10m", max-file: "3" }`.

### D2. Enrutamiento: etiquetas Traefik propias, sin strip de prefijo
- Los campos "Domains" de `web` y `server` en Coolify quedan **vacíos**, para que Coolify no genere etiquetas ni el middleware `stripprefix`. El compose declara sus propias etiquetas Traefik, con `traefik.enable=true` explícito en ambos servicios.
- **Espacio de nombres**: el proveedor Docker de Traefik comparte routers, servicios y middlewares entre todas las apps del proxy, y un choque de nombres deja fuera a ambas. Todo nombre nuestro lleva el prefijo **`torneo-`**:
  - Router `torneo-api` (servicio `torneo-api`): ``Host(`${PUBLIC_HOST}`) && (PathPrefix(`/api`) || PathPrefix(`/ws`))``, `priority=100`, entrypoint `https`, `tls.certresolver=letsencrypt`, puerto `8080`, **sin middlewares de prefijo**.
  - Router `torneo-web` (servicio `torneo-web`): ``Host(`${PUBLIC_HOST}`)``, `priority=1`, entrypoint `https`, mismo certresolver, puerto `3000`.
  - Routers `torneo-api-http` y `torneo-web-http` en el entrypoint `http`, con las mismas reglas y el middleware propio `torneo-https-redirect` (`redirectscheme.scheme=https`, `permanent=true`), declarado en nuestras etiquetas; no dependemos de un middleware de Coolify.
  - `traefik.docker.network=torneo-edge` en ambos servicios (D1), nunca la red `coolify`.
  - `observability.accesslogs=false` en los cuatro routers (D11).
- Las rutas son **idénticas en dev y en prod**: axum monta `/api/...` y `/ws`; en dev, el proxy de Vite reenvía `/api` y `/ws` a `localhost:8080` sin reescribir.
- **`TRUSTED_PROXIES` (ADR-0008 D7) = `10.250.0.2/32`**, la IP fija de `coolify-proxy` en `torneo-edge`, cargada como variable de entorno. No es la subred de `coolify` ni la de `torneo-edge`: `web` también está en esa subred y no es un proxy. Un peer fuera de esa IP (un vecino de la red `coolify` o de la red de la BD) ve ignorado su `X-Forwarded-For` (verificación 14). Esta definición prevalece sobre el texto de ADR-0008 D7 ("CIDR de la red de Docker de Traefik").

### D3. Healthchecks
- **Server**: `GET /api/health` hace `SELECT 1` con timeout de 1 s. Responde `200 {"status":"ok","version":"<APP_VERSION>"}` o `503 {"status":"db_unavailable"}`. Solo códigos, sin texto para humanos (ADR-0007). No pasa por rate limit ni requiere sesión.
- **`version` se hornea en la imagen**, no se lee en runtime: el build recibe `--build-arg APP_VERSION=vX.Y.Z` y el binario lo toma en compilación (`env!("APP_VERSION")`; en builds locales y de PR vale `sha-<7>` o `dev`). El contenedor no recibe `IMAGE_TAG` como variable; así `verify` (D6) comprueba la imagen que de verdad corre y no la variable de Coolify.
- Los healthchecks van en **forma exec**, porque las imágenes distroless no tienen shell (ni `curl`/`wget`):
  - Server: **subcomando del propio binario**, `test: ["CMD", "/server", "healthcheck"]`. Hace `GET http://127.0.0.1:8080/api/health` con timeout de 2 s y sale con 0 si recibe 200, o con 1 en cualquier otro caso.
  - **Web**: ruta `GET /healthz` (`+server.ts`, fuera del enrutamiento por idioma de ADR-0007) que responde 200 sin llamar al API. `test: ["CMD", "/nodejs/bin/node", "-e", "fetch('http://127.0.0.1:3000/healthz').then(r=>process.exit(r.ok?0:1)).catch(()=>process.exit(1))"]` (`/nodejs/bin/node` es la ruta de `node` en `distroless/nodejs22`).
- Parámetros en ambos: `interval: 15s`, `timeout: 3s`, `retries: 3`, `start_period: 30s`.
- **Orden de arranque**: `web` declara `depends_on: { server: { condition: service_healthy } }`. Postgres no está en el compose (D1), así que no puede ser un `depends_on`; su salud la vigila Coolify con `pg_isready` y el server **espera a la BD al arrancar**: reintenta la conexión cada 2 s hasta 60 s y, si no lo logra, sale con código 1 (Docker lo reinicia). Mientras la BD no responde, `/api/health` da 503 y el contenedor no queda `healthy`.

### D4. Memoria y swap
| Servicio | Límite (`mem_limit` = `memswap_limit`) | Configuración |
|---|---|---|
| `server` | 256 MB | pool de sqlx `max_connections = 8` |
| `web` | 384 MB | `NODE_OPTIONS=--max-old-space-size=256` |
| Postgres (Coolify) | 512 MB | `shared_buffers=128MB`, `max_connections=20`, `work_mem=4MB` |
- `memswap_limit` igual a `mem_limit`: un leak termina en OOM-kill y reinicio, no en un contenedor que pagina y ahoga la única vCPU.
- Peor caso de nuestros límites: 1,152 MB. Con Coolify en ~1.6 GB quedan ~1.2 GB de margen.
- **Swap del host**: se verifica con `swapon --show`. Si no hay, se crea un swapfile de **2 GB** con `vm.swappiness=10`. Es un colchón para el host (Coolify, builds de otras apps), no para nuestros contenedores.

### D5. Imágenes y registro
- Registro: **GHCR público**, `ghcr.io/<owner>/<repo>/server` y `ghcr.io/<owner>/<repo>/web`, derivado de `github.repository` **normalizado a minúsculas** (GHCR rechaza mayúsculas; un paso `echo "IMAGE_REPO=${GITHUB_REPOSITORY,,}" >> "$GITHUB_ENV"`). El VPS hace pull **sin `docker login`**.
- Etiquetas: `vX.Y.Z` (la del tag de git) y `sha-<7 chars>`, ambas sobre el mismo digest. **Nunca `latest`.** El compose usa `image: ghcr.io/...:${IMAGE_TAG:?}`, así que un despliegue sin `IMAGE_TAG` falla en vez de tomar algo al azar. Una etiqueta publicada nunca se mueve.
- **Un manifiesto por etiqueta**: `docker/build-push-action` con `provenance: false` y `sbom: false`. Por defecto la action agrega una attestation de procedencia, el tag apunta a un índice y sus manifiestos hijos quedan en GHCR como versiones **sin etiqueta**; una retención que las borre deja el tag apuntando a manifiestos inexistentes (`manifest unknown` en deploy y rollback). Con un solo manifiesto, cada versión de GHCR es una imagen completa. El job `build` comprueba después del push, con `docker buildx imagetools inspect --raw`, que el tag es un manifiesto de imagen y no un índice; si es un índice, falla antes de la retención.
- Retención: después del push y de esa comprobación, `actions/delete-package-versions` por cada paquete con **exactamente** estos parámetros: `package-name: <repo>/server` (y `/web`), `package-type: container`, `min-versions-to-keep: 10`. **Sin** `delete-only-untagged-versions` ni `num-old-versions-to-delete`. Conserva las 10 versiones más recientes (cuentan todas; con un manifiesto por tag casi todas tienen etiqueta) y borra las más viejas. Con 10 releases siempre hay de dónde hacer rollback (verificación 16).
- La procedencia firmada no se pierde como requisito a futuro: si se quiere, se agrega con `actions/attest-build-provenance` (que la guarda en la API de attestations, no como versión del paquete) en un ADR o plan posterior.
- Bases: server en `gcr.io/distroless/static-debian12:nonroot` (binario estático, D8); web en `gcr.io/distroless/nodejs22-debian12:nonroot`. Ambas por digest, actualizadas por Dependabot.
- Las imágenes se construyen en Actions (amd64 nativo) con `docker/build-push-action`, `platforms: linux/amd64` y caché `type=gha`. **Nunca se compila en el VPS.**
- El Dockerfile del server usa **`cargo-chef`** (etapas `planner` → `cook` → `build`), para que la capa de dependencias de Rust quede en caché y solo se recompile el código propio. Es lo que permite el objetivo de ≤ 8 min del job `images` (D7).

### D6. Release y disparo del despliegue
- Workflow `release.yml`, disparado **solo** por un tag `v*` (y por `workflow_dispatch` con input `tag` para rollback). Jobs:
  1. `build`: construye y sube las dos imágenes con `APP_VERSION` = el tag (D3, D5). `permissions: { contents: read, packages: write }`. En un `workflow_dispatch` (rollback) **no reconstruye**: solo comprueba con `docker buildx imagetools inspect` que el tag existe en GHCR, para no generar un digest distinto bajo la misma etiqueta.
  2. `deploy`: `environment: production`, que exige **aprobación del owner** (required reviewer). Este es el gate humano de "desplegar a producción" de `docs/process.md`. Su comportamiento depende de la variable de repositorio `DEPLOY_MODE` (decisión del owner, ver abajo):
     - `api`: valida que el tag cumple `^v[0-9]+\.[0-9]+\.[0-9]+$`, fija `IMAGE_TAG` del recurso (`PATCH /api/v1/applications/{uuid}/envs`) y dispara el deploy (`GET /api/v1/deploy?uuid=…`). Solo usa `curl`; ninguna action de terceros toca el token.
     - `manual` (**valor por defecto** mientras el owner no decida): el job no llama a Coolify. El owner fija `IMAGE_TAG` en la UI de Coolify, despliega a mano y después aprueba el environment; la aprobación es la señal de "ya desplegué".
  3. `verify`: consulta `https://${PUBLIC_HOST}/api/health` hasta 5 min y falla si `version` no es el tag desplegado. Como `version` está horneada (D3), un `IMAGE_TAG` cambiado sin redeploy no la engaña.
- **Rollback** = `workflow_dispatch` con el tag anterior; pasa por la misma aprobación.
- En ningún caso `latest`.
- **Token de la API de Coolify (riesgo declarado).** En Coolify v4 los tokens tienen alcance de **equipo**, no de recurso, con permisos `read`/`write`/`deploy`/`read:sensitive`/`root`. Actualizar `IMAGE_TAG` exige `write`, y ese permiso alcanza a **todas** las apps del owner en ese equipo y en ese VPS. Un token con "el mínimo permiso para ese recurso" no existe. Si se usa `DEPLOY_MODE=api`:
  - Permisos `write` + `deploy`; **nunca** `root` ni `read:sensitive`.
  - **Impacto si se filtra**: control sobre las demás apps del VPS (cambiar sus variables de entorno, sus imágenes y desplegarlas), no solo sobre Torneo. Es lo que el owner acepta o rechaza.
  - Mitigaciones: el token y el UUID son **secretos del environment `production`**, invisibles para `ci.yml` y para cualquier PR; *deployment branches and tags* del environment limitados a tags `v*` y a `main` (desde `main` corre el `workflow_dispatch` de rollback); el job `deploy` no hace checkout ni ejecuta código del repo, solo `curl`, y la validación del tag impide inyectar parámetros; las actions del workflow fijadas por SHA (D7); rotación **cada 90 días** y de inmediato ante cualquier sospecha (revocar en Coolify y reemplazar el secreto); un token con nombre propio (`github-torneo-release`) para identificarlo en la UI.
  - Opción a confirmar: un **equipo de Coolify solo para Torneo** limitaría el alcance del token. Hasta donde sabemos, en Coolify v4 un servidor pertenece a un solo equipo, así que requeriría otro servidor; si se confirma que se puede compartir el VPS entre equipos, es la opción preferida.
  - **Alternativa sin token**: `DEPLOY_MODE=manual` (arriba). No expone la API de Coolify a GitHub y mantiene el mismo gate y la misma verificación; cuesta un paso manual por release. La decisión es del owner (Pendiente).
- Crear tags `v*` está restringido al owner por un ruleset de tags.

### D7. CI: workflows, filtros, caché y seguridad en repo público
- `ci.yml` corre en `pull_request` y en `push` a `main` (este último calienta la caché). Jobs:
  - `changes`: `dorny/paths-filter` decide qué corre. Necesita `permissions: { contents: read, pull-requests: read }` para listar los archivos del PR. Filtros:
    - `rust`: `crates/**`, `apps/server/**`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.cargo/**`, `.sqlx/**`, `content/**`.
    - `web`: `apps/web/**`, `packages/**`, `crates/protocol/**` (genera `packages/protocol-ts`), `crates/engine/**` y `crates/engine-wasm/**` (el WASM lo consume la web), `package.json`, `pnpm-lock.yaml`, `pnpm-workspace.yaml`, `.node-version`.
    - `images`: `infra/**`, `**/Dockerfile`, `.dockerignore`.
    - Comunes a los tres: `justfile`, `.github/workflows/**`, `.github/actions/**`.
  - `rust`, `web`, `e2e` e `images` (este último construye sin push) corren con `if:` según `changes`.
  - `ci-ok`: corre siempre (`if: always()`), depende de todos y falla si alguno terminó en `failure` o `cancelled`; un job saltado cuenta como éxito. **`ci-ok` es el único check requerido.** Así un PR de solo `docs/**` termina en ~1 min y nunca se queda en "pending".
- No se usa `paths:`/`paths-ignore:` a nivel de workflow (dejaría checks requeridos colgados).
- Caché:
  - Rust: `Swatinem/rust-cache` con `shared-key: ci` y `save-if: ${{ github.ref == 'refs/heads/main' }}`. Solo `main` escribe caché; los PRs la restauran desde la rama base.
  - pnpm: `actions/setup-node` con `cache: pnpm`.
  - Playwright: `~/.cache/ms-playwright` con llave de la versión de `@playwright/test`.
- `concurrency`: grupo `ci-${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}`, con `cancel-in-progress: ${{ github.event_name == 'pull_request' }}` (en `main` no se cancela, para no perder el calentado). `release.yml` usa el grupo `release` sin cancelar.
- `timeout-minutes` en **todos** los jobs: `changes` 5, `rust` 20, `web` 15, `e2e` 20, `images` 25, `build` (release) 30, `deploy` 10, `verify` 10.
- Seguridad (repo público):
  - `permissions: contents: read` por defecto en cada workflow; solo `release.yml/build` tiene `packages: write` y solo `ci.yml/changes` tiene `pull-requests: read`.
  - **Prohibido `pull_request_target`** y `workflow_run` que consuma artefactos de un PR con secretos. Ningún secreto se expone a `pull_request`; los secretos solo existen en el environment `production`.
  - Ajuste del repo: "Require approval for all external contributors" para workflows de forks.
  - Las actions de terceros se fijan por SHA (Dependabot las actualiza).
  - Ruleset sobre `main`: PR obligatorio con **0 aprobaciones requeridas** (el owner es el único maintainer y GitHub no le deja aprobar su propio PR; los veredictos de los agentes viven en comentarios firmados, `docs/process.md`), `ci-ok` requerido, sin force-push ni borrado.
- Primera línea de defensa local: `just check` y un hook `pre-push` que lo corre (se instala con `just hooks`).
- Objetivos de velocidad (ya no de presupuesto): PR solo de docs ≤ 2 min; PR con Rust y caché caliente ≤ 8 min (p50); release completo ≤ 20 min sin contar la espera de aprobación.

### D8. Binario del server
- Target `x86_64-unknown-linux-musl`, binario estático (`ldd` responde "not a dynamic executable").
- **Sin OpenSSL ni `aws-lc-sys`**: sqlx con `runtime-tokio` + `tls-rustls-ring`, `ring` como proveedor criptográfico de rustls, y `reqwest`/`oauth2` con rustls sobre `ring`. Así el builder musl no necesita cmake ni clang.
- Se impone con **`cargo deny check bans`** en el job `rust`, con `deny.toml` → `bans.deny = [{ name = "openssl-sys" }, { name = "openssl" }, { name = "aws-lc-sys" }]`. No se usa `cargo tree -i openssl-sys`: sale con código 101 cuando el paquete no está, así que un chequeo literal falla siempre. Si una dependencia futura exige `aws-lc-sys`, se discute en el plan correspondiente antes de quitarlo de la lista.
- `#[global_allocator]` = `mimalloc` (el malloc de musl rinde mal con tokio).
- `SQLX_OFFLINE=true` con `.sqlx/` versionado: el build de la imagen no necesita una BD.
- Perfil `release`: `lto = "thin"`, `codegen-units = 1`, `strip = true`.
- **Plan B** (si Actions no está disponible): build en la Mac del owner con `cargo zigbuild --target x86_64-unknown-linux-musl` y `docker buildx build --platform linux/amd64`, push a GHCR con un PAT de `write:packages` del owner y deploy manual desde Coolify. Es un procedimiento excepcional, no un runner.

### D9. Backups de Postgres
- Backups programados de Coolify sobre el recurso standalone (`pg_dump` en formato custom):
  - **Frecuencia: diaria**, a las 09:00 UTC (madrugada en México), fuera del horario de peleas.
  - **Retención local: 7 días** en el VPS.
  - **Copia fuera del VPS**: destino S3-compatible configurado en Coolify (o copia programada por `scp`), con **retención de 30 días**. El destino es **Cloudflare R2** (decisión del owner). Credenciales limitadas a ese bucket o destino y guardadas solo en Coolify. El destino debe estar cifrado en reposo, porque el dump contiene datos personales (`display_name`, `ip_hash`; ADR-0008).
- **Backup manual antes de cada release que traiga migraciones** ("Backup now" en Coolify). Es parte del checklist de aprobación de D6.
- Objetivos: **RPO ≤ 24 h, RTO ≤ 1 h.**
- **Prueba de restauración** (bloquea el primer despliegue a producción y se repite cada 90 días): se restaura el último dump de la copia externa en un Postgres temporal, se corren las migraciones (no debe haber pendientes) y se ejecuta el chequeo de invariantes del ledger: cada transacción del ledger suma cero, y los saldos derivados coinciden con los materializados (si existen). El resultado (fecha, tamaño, tiempo, OK/fallo) se anota en el issue de Linear del release.
- Un fallo de backup notifica al owner por el canal de notificaciones de Coolify (Pendiente).

### D10. Despliegues y peleas en vivo
- Coolify no ofrece despliegue sin downtime para compose, y no corremos dos `server` a la vez (estado en memoria). Se acepta un corte de **≤ 30 s** por despliegue, con estas reglas:
  - **Apagado ordenado**: con `SIGTERM`, el server deja de aceptar conexiones, cierra cada WS con código `1012` (service restart) y termina en ≤ 10 s. `stop_grace_period: 15s`.
  - **Cliente**: reconexión automática con backoff exponencial (1, 2, 4… hasta 30 s, con jitter). Al reconectar se resincroniza: recibe `server_time`, el estado de la pelea y los eventos **ya ocurridos**, nunca futuros (ADR-0002).
  - **Server al arrancar**: retoma las peleas no liquidadas desde la BD. Las fechas de cierre de apuestas y de inicio son absolutas; si alguna venció durante el corte, se aplica al arrancar. Los eventos de una pelea `Live` siguen saliendo según su tiempo, y los que vencieron durante el corte se entregan como ya ocurridos.
  - **Migraciones**: `sqlx::migrate!` corre al arrancar, antes de abrir el puerto. Son **compatibles hacia atrás** (expand/contract): un release solo agrega; renombrar o borrar columnas va en un release posterior, cuando el código ya no las usa. Ninguna migración modifica ni borra filas de `ledger_entries` (ADR-0005).
  - **No se despliega durante un main event** (desde que abren sus apuestas hasta `Settled`). Es un punto del checklist de aprobación de D6; cuando exista el API del scheduler, `deploy` lo verificará automáticamente.
- La **limpieza de Docker de Coolify** queda activada: diaria y además al pasar el 80 % de disco, con **"Delete Unused Volumes" apagado** (un volumen detenido de otra app o de un recurso parado no debe borrarse). El rollback no depende de imágenes locales, porque GHCR es público (D5).

### D11. Logs de acceso de Traefik
- **Desactivados** en el proxy de Coolify (sin `--accesslog`). Guardan la IP en claro y el query string completo, lo que choca con ADR-0008 (D3: nada de token, `code` ni `state` en logs; D8: la IP en claro nunca se persiste, solo `ip_hash`).
- **Además, apagados por router**: `traefik.http.routers.<router>.observability.accesslogs=false` en `torneo-api`, `torneo-web`, `torneo-api-http` y `torneo-web-http` (requiere Traefik ≥ 3.1, la versión de Coolify v4; la verificación 13 comprueba la versión). Así la garantía de ADR-0008 no depende de la configuración global del proxy compartido: si alguien activa `--accesslog` para otra app, nuestras peticiones siguen fuera.
- El registro de peticiones de la app lo hace el server (`tower-http::trace`, saneado según ADR-0008). Traefik solo emite su log propio en nivel `ERROR`.
- Si hace falta activarlos para diagnosticar: como máximo **7 días**, con `accesslog.fields.names` en `drop` para `ClientAddr`, `ClientHost`, `RequestPath` y `RequestAddr`, y `accesslog.fields.headers.defaultmode=drop`. Para que nuestras peticiones aparezcan hay que quitar también la etiqueta por router (un redeploy). Al terminar se apagan, se restaura la etiqueta y se borra el archivo.
- El proxy es compartido con otras apps del VPS, así que el cambio requiere el OK del owner (Pendiente).

## Alternativas descartadas
- **Postgres dentro del compose con un sidecar de `pg_dump` en cron**: más piezas propias que mantener. Los backups de Coolify ya traen programación, retención y destino S3.
- **WAL archiving / PITR (WAL-G, pgBackRest)**: RPO de minutos, pero es complejidad y almacenamiento que no se justifican con monedas ficticias. Se revisa si el RPO de 24 h resulta insuficiente.
- **Dominios con path en Coolify y rutas de axum sin prefijo** (opción b de devops): dev y prod tendrían rutas distintas, y el `stripprefix` es implícito y fácil de romper.
- **Compilar en el VPS**: con 1 vCPU, Rust tarda muchísimo y puede quedarse sin memoria, y compite con lo que está en producción.
- **Runner self-hosted en la Mac del owner**: en un repo público, un PR desde un fork ejecutaría código arbitrario en esa máquina. Además, con minutos ilimitados ya no aporta nada.
- **Repo privado**: la decisión ya está tomada (público). Además da transparencia a "provably fair", y la seguridad no depende de ocultar el código.
- **`latest` + auto-deploy de Coolify en cada push**: no es reproducible, no hay rollback claro y se saltaría el OK del owner.
- **Docker Hub**: límites de pull anónimos; GHCR público vive junto al repo y no tiene esos límites.
- **`pull_request_target`** para que los PRs de forks tengan secretos: es el vector clásico de robo de secretos en repos públicos.
- **`paths:` a nivel de workflow**: deja colgados los checks requeridos.
- **Imagen glibc con OpenSSL**: más grande, más superficie y una dependencia nativa que complica el build estático.
- **Blue-green con dos `server`**: rompe el estado en memoria de ADR-0008; requeriría un ADR nuevo.
- **Red predefinida `coolify` con `TRUSTED_PROXIES` = su CIDR**: la red es compartida; cualquier vecino podría falsificar `X-Forwarded-For` y un servicio ajeno llamado `server` podría recibir la cookie y el secreto interno del SSR.
- **`TRUSTED_PROXIES` = la subred de `torneo-edge`**: incluiría a `web`, que no es un proxy. Solo se usa la IP fija del proxy.
- **Attestations de procedencia en el índice + borrar versiones sin etiqueta**: deja los tags apuntando a manifiestos borrados. Conservarlas sin borrar las sin etiqueta llenaría GHCR de versiones huérfanas sin control.

## Consecuencias
- **Más fácil**: desplegar es aprobar un environment; el rollback es otro tag; restaurar está probado antes de producción; CI barata en tiempo y sin presupuesto que vigilar.
- **Más difícil**: cada despliegue corta los WS hasta 30 s (mitigado por reconexión y resync); las migraciones deben ser aditivas; con `DEPLOY_MODE=api` el despliegue depende de que la API de Coolify sea alcanzable desde GitHub y existe un token con alcance de equipo que hay que rotar; con `manual`, cada release cuesta un paso a mano. La red `torneo-edge` y la IP fija del proxy son configuración del VPS fuera del repo, y una actualización de Coolify podría deshacerla (verificación 15 tras cada actualización).
- **ADR-0008 remite a este ADR** para la red y el proxy: su D7 describe `TRUSTED_PROXIES` como "CIDR de la red de Docker de Traefik" y su D5 pone `http://server:8080` como ejemplo de `INTERNAL_API_URL`. Los valores vigentes son los de D1 y D2 de este ADR (`10.250.0.2/32` y `http://torneo-server:8080`); ADR-0008 debe alinear su texto en su propia corrección (no lo edito aquí).
- **Vigilar**: el uso real de memoria contra los límites de D4 (si `web` se acerca a 384 MB, se revisa antes de subirlo); el disco (imágenes, backups locales, logs); que los backups sigan corriendo (notificación de fallo).
- **Contradicción con `docs/architecture.md`** (no la edito aquí): la sección "Despliegue" dice que el compose tiene `web`, `server` y `postgres`. Con D1, Postgres es un recurso standalone de Coolify y el compose solo tiene `web` y `server`. Hay que actualizarla cuando este ADR se apruebe.
- **Afecta a 001-fundaciones**: su intent todavía lista "¿Repo público o privado?" como pregunta abierta; queda resuelta (público). Su spec y su plan deben incluir `/api/health` con `APP_VERSION` horneada, `server healthcheck`, `/healthz`, las redes de D1, las etiquetas Traefik de D2, `cargo-chef`, `deny.toml` y los workflows de D6 y D7.

**Números fijados:** server 256 MB · web 384 MB (heap 256) · Postgres 512 MB, `shared_buffers` 128 MB, 20 conexiones, pool 8 · swap 2 GB, `swappiness` 10 · healthcheck 15 s / 3 s / 3 reintentos / 30 s de arranque · espera de BD 60 s · logs de Docker 3 × 10 MB · red `torneo-edge` `10.250.0.0/28`, proxy `10.250.0.2`, `TRUSTED_PROXIES` `10.250.0.2/32` · GHCR: 10 versiones, un manifiesto por tag · token de Coolify (si `api`): `write` + `deploy`, rotación cada 90 días · backups diarios 09:00 UTC, 7 días locales, 30 días fuera del VPS, RPO 24 h, RTO 1 h, restauración cada 90 días · corte por despliegue ≤ 30 s, `stop_grace_period` 15 s · reconexión 1–30 s · timeouts de CI 5/20/15/20/25/30/10/10 min · ruleset de `main` con 0 aprobaciones · logs de Traefik apagados globalmente y por router (máx. 7 días si se activan).

**Verificaciones mínimas** (bloquean el primer despliegue a producción; las de CI bloquean el merge de 001-fundaciones):
1. `curl https://<dominio>/api/health` → 200 con `version` igual al tag (horneada: cambiar `IMAGE_TAG` en Coolify sin redeploy no la cambia), y el log del server registra la ruta `/api/health` (prueba de que no hay strip). Un `GET /ws` con upgrade llega a axum como `/ws`.
2. Con Postgres detenido: `/api/health` → 503 y el contenedor `server` deja de estar `healthy`; al levantar Postgres se recupera solo.
3. `server healthcheck` sale con 0 con la BD arriba y con 1 con la BD abajo (test de integración en CI).
4. `docker inspect` muestra los `mem_limit` y `memswap_limit` de D4; `swapon --show` muestra el swap en el host.
5. Postgres no responde en el puerto 5432 desde fuera del VPS.
6. Un PR que solo toca `docs/**` termina con `ci-ok` en verde sin correr `rust` ni `web`. Un PR que toca `crates/protocol/**` corre `rust` **y** `web`.
7. Un PR con un job fallido deja `ci-ok` en rojo; un PR con jobs saltados deja `ci-ok` en verde.
8. `cargo deny check bans` en verde con `openssl-sys`, `openssl` y `aws-lc-sys` prohibidos, y `ldd` del binario → "not a dynamic executable" (job `rust`).
9. Ningún workflow usa `pull_request_target`; un chequeo en CI (`grep` sobre `.github/workflows/`) falla si aparece. Ningún job de `ci.yml` referencia `secrets.*` salvo `GITHUB_TOKEN`.
10. Un `release.yml` sin aprobación en `production` no despliega; el compose sin `IMAGE_TAG` falla al levantar.
11. Prueba de restauración de D9 hecha y registrada, con el invariante del ledger en verde.
12. Simulacro de despliegue con una pelea de prueba en `Live`: el cliente se reconecta en ≤ 30 s, recibe los eventos ya ocurridos y ningún evento futuro; las apuestas de la pelea quedan intactas.
13. El proxy de Coolify corre sin `--accesslog` y con Traefik ≥ 3.1 (inspección de su configuración), y `docker inspect` de `web` y `server` muestra `observability.accesslogs=false` en los cuatro routers `torneo-*`. Con el access log activado temporalmente, una petición a `/api/health` no aparece en él.
14. **Suplantación de IP**: desde un contenedor de prueba en la red `coolify` (y en la red de la BD), `curl http://<IP de server>:8080/api/...` con `X-Forwarded-For` falsificado no cambia la llave del rate limit (se usa la IP del peer). La misma petición a través del proxy sí usa la IP real del cliente.
15. **Aislamiento de red**: (a) desde `web`, `getent hosts torneo-server` devuelve **una sola** IP, la de `server` en `torneo-edge`; (b) `docker network inspect torneo-edge` muestra solo `coolify-proxy` (con `10.250.0.2`), `web` y `server`; (c) `docker inspect` muestra a `web` solo en `torneo-edge` (más, si Coolify la crea, la red propia del stack, que solo contiene proxy, `web` y `server`) y a `server` además solo en la red de la BD. (b) y (c) se repiten tras reiniciar el proxy y tras cada actualización de Coolify.
16. **Retención de GHCR**: `docker buildx imagetools inspect --raw` de un tag publicado devuelve un manifiesto de imagen, no un índice; tras correr la retención, `docker pull` en el VPS del tag **más viejo que se conserva** funciona.
17. Un `workflow_dispatch` lanzado desde una rama que no sea `main` no puede usar el environment `production`; con `DEPLOY_MODE=manual`, `deploy` no hace ninguna llamada a Coolify.

## Decisiones del owner (2026-10-21)
- **ADR aceptado.**
- **Copia de backups fuera del VPS: Cloudflare R2.** Un bucket dedicado, cifrado en reposo (R2 cifra por defecto), con un token de API de R2 limitado a ese bucket (lectura y escritura de objetos), cargado solo como destino S3 de Coolify. La retención de 30 días se aplica con una regla de ciclo de vida del bucket. Cloudflare queda como encargado en el aviso de privacidad, porque los dumps contienen datos personales (ADR-0008).
- **`DEPLOY_MODE=manual`.** No hay token de la API de Coolify en GitHub. Cualquier valor distinto de `api`, incluido no definirla, se trata como `manual`. Pasar a `api` requiere un nuevo OK del owner.
- **Cambios en el proxy compartido aprobados.** devops-engineer los aplica en el VPS durante la feature 001: crear `torneo-edge` con `--ip-range 10.250.0.8/29` (así `.2` queda reservada al proxy), unir `coolify-proxy` con IP fija `10.250.0.2`, swap de 2 GB, apagar los logs de acceso del proxy (D11) y activar la limpieza de Docker con "Delete Unused Volumes" apagado.

## Pendiente (owner)
- **Canal de notificaciones de Coolify** para fallos de backup y de despliegue. Recomendación: un webhook de Discord a un canal privado del owner. Se configura en la UI de Coolify (Notifications → Discord) antes del primer despliegue a producción.
- **Configuración de GitHub** (la hace devops-engineer con el OK del owner cuando exista la CI): environment `production` con el owner como required reviewer y *deployment branches and tags* limitados a `v*` y `main`; "Require approval for all external contributors"; ruleset de `main` (PR obligatorio, 0 aprobaciones, `ci-ok`) y de tags `v*`; poner públicos los paquetes de GHCR después del primer push.

## Revisión
### devops-engineer — 2026-10-20 — REQUEST_CHANGES
- Verifiqué:
  - Frontmatter contra `docs/process.md`: válido, aunque faltan `approvals: []`, `linear` y `depends_on` (ver punto 10). No contradice ADR-0003 (axum + WebSockets, SvelteKit adapter-node, Postgres con sqlx) ni `docs/architecture.md` (Traefik de Coolify, `/` → web, `/api` y `/ws` → server).
  - El proxy de Coolify v4 es Traefik por defecto, emite certificados de Let's Encrypt y hace pasar el upgrade de WebSocket sin configuración extra. Con las reglas por defecto, `/api` y `/ws` ganan a `/` porque Traefik prioriza las reglas más largas. Es correcto.
  - Coolify puede desplegar un compose que solo tenga `image:` (sin `build:`): hace pull y corre, sin compilar en el VPS. Es correcto.
  - `cargo zigbuild --target x86_64-unknown-linux-musl` sirve como plan B desde una Mac (Apple Silicon incluida) y produce un binario estático para `scratch` o `distroless/static`. `Swatinem/rust-cache` y la caché de pnpm (`actions/setup-node` con `cache: pnpm`) existen y son adecuadas.
  - Memoria: server ~20–50 MB, Postgres ~100–200 MB con `shared_buffers` en 128 MB y pocas conexiones, y Node SSR ~80–150 MB. Son razonables. Con Coolify en ~1.6 GB quedan más de 2 GB libres. El disco (28 GB libres) alcanza si se limpian las imágenes viejas.
  - La estimación de 200–500 min/mes es plausible, con los ajustes del punto 6.
- Cambios pedidos (bloqueantes):
  1. **Backups de Postgres (falta).** El ledger es inmutable y es la fuente de los saldos (ADR-0005); perderlo no tiene arreglo. Hay que decidir y escribir el mecanismo. Recomiendo correr Postgres como **recurso de base de datos standalone de Coolify** en vez de dentro del compose, porque así se pueden usar sus backups programados (pg_dump con retención). El compose quedaría con `web` y `server`, y `DATABASE_URL` por variable de entorno. La alternativa es un sidecar con `pg_dump` en cron. Hay que definir frecuencia (al menos diaria), retención local (por ejemplo 7 días) y **una copia fuera del VPS**. Opciones gratis: Cloudflare R2 o Backblaze B2 (10 GB), o un `scp` a la Mac del owner. Un servicio externo necesita el OK del owner según el proceso. También hay que fijar una prueba de restauración (al menos una vez antes del primer despliegue a producción).
  2. **Healthchecks (falta).** El server necesita un endpoint de salud (por ejemplo `GET /api/health`, que verifique la DB). Ojo: distroless/scratch no traen `curl` ni `wget`, así que el healthcheck debe ser un subcomando del propio binario (`server healthcheck`) o usar una imagen base que tenga una herramienta para eso. Postgres usa `pg_isready`. Hay que declarar `depends_on: condition: service_healthy` para web → server → postgres.
  3. **Límites de memoria (falta).** Hay que declarar `mem_limit` (o `deploy.resources.limits.memory`) por servicio para que un leak no tumbe Coolify. Propongo server 256 MB, web 384 MB con `NODE_OPTIONS=--max-old-space-size=256`, y Postgres 512 MB con `shared_buffers=128MB` y `max_connections` ~20 (el pool de sqlx, chico: 5–10). También hay que revisar si el VPS tiene swap y, si no tiene, agregar un swapfile de 1–2 GB.
  4. **Strip del prefijo de ruta en Coolify (verificar y documentar).** Cuando el dominio de un servicio lleva path (`https://dominio/api`), Coolify agrega por defecto un middleware `stripprefix` a las etiquetas de Traefik. Entonces axum recibiría `/health` en lugar de `/api/health`, y `/ws` llegaría como `/`. El ADR debe fijar una de dos opciones: (a) definir etiquetas Traefik propias en el compose, sin strip, o (b) montar las rutas de axum sin el prefijo. Recomiendo la (a), porque mantiene las mismas rutas en dev y en prod. También hay que anotar que el server necesita dos entradas de dominio (`/api` y `/ws`).
  5. **Pull desde GHCR (falta detalle).** Si el repo es privado, los paquetes de GHCR son privados por defecto, y el VPS necesita `docker login ghcr.io` con un PAT de solo `read:packages`, guardado como credencial del servidor y fuera del repo. Además, en el plan Free los paquetes privados tienen **500 MB de almacenamiento y 1 GB/mes de transferencia** fuera de Actions. Con la imagen de web (~70–150 MB comprimida) más la del server y ~6 releases/mes, esa cuota se llena. Hace falta una política de retención (por ejemplo `actions/delete-package-versions`, conservando las últimas 3) o paquetes públicos, que son gratis e ilimitados. También hay que definir **cómo se dispara el despliegue**: el workflow del tag llama al webhook de deploy de Coolify, con el token en un secreto de GitHub, y el tag de la imagen se pasa por variable de entorno (`IMAGE_TAG`), nunca como `latest`. Esto respeta la regla de que no se despliega a producción sin OK del owner: por ejemplo, con un environment de GitHub que requiera aprobación, o con el disparo manual desde Coolify.
  6. **Caché y minutos de CI (corregir supuestos).** Las cachés de Actions creadas en un PR solo las ve ese PR. Un PR nuevo solo puede restaurar cachés de la rama base. Si el CI corre únicamente en PRs, nunca se guarda caché en `main`, y la primera corrida de cada PR va en frío (Rust con axum, sqlx y tokio tarda ~6–10 min en un runner de 2 vCPU). Hay que agregar un job barato en `push` a `main` que solo caliente la caché (o `save-if` en main), y usar `shared-key`. Además: `concurrency` con `cancel-in-progress: true` para cancelar corridas superadas, `timeout-minutes` en cada job (sin él, un job colgado puede quemar hasta 360 min) y la caché del navegador de Playwright si corre e2e. La estimación debe contar varios pushes por PR (≈15 PRs × ~3 corridas) y que cada job se redondea hacia arriba al minuto completo, lo que pesa más con jobs en paralelo o con matrices.
  7. **Filtros por ruta (precisar).** Si un check requerido se salta por `paths`, el PR se queda en "pending" para siempre. Hay que filtrar dentro del job (`dorny/paths-filter`) o no marcar esos checks como requeridos. Ojo: las reglas de protección de rama en repos privados del plan Free no aplican. Los filtros deben incluir `Cargo.lock`, `rust-toolchain.toml`, `.github/workflows/**` y el `justfile`. Los cambios en `crates/protocol/**` deben disparar **también** el CI de web, porque generan `packages/protocol-ts`.
- Cambios pedidos (no bloqueantes, pero conviene dejarlos escritos):
  8. **El plan B tiene una contradicción.** GitHub desaconseja usar runners self-hosted en repos públicos, porque un PR desde un fork ejecutaría código en la Mac del owner. Si el repo se hace público, el runner self-hosted queda descartado, o se limita a workflows que no corran con PRs de forks. En un repo público también hay que exigir aprobación para workflows de forks y no exponer secretos a `pull_request`. Con musl conviene usar sqlx con `rustls` (sin OpenSSL) y un allocator como `mimalloc`, porque el malloc de musl es lento con tokio. Si la imagen de web se construye en la Mac (arm64), hay que usar `docker buildx --platform linux/amd64`.
  9. **Despliegues y peleas en vivo.** Coolify no hace despliegues sin downtime para stacks de compose. Al reiniciar el server se cortan los WebSockets, y puede coincidir con una ventana de apuestas o con el main event. Hay que documentar que el cliente se reconecta y se resincroniza, que las migraciones de sqlx corren al arrancar y son compatibles hacia atrás, y que no se despliega durante un main event. La limpieza de imágenes viejas de Coolify debe quedar activada para cuidar el disco.
  10. **Frontmatter.** Hay que agregar `approvals: []`, `linear` y `depends_on: ["0003"]`, y considerar añadir `owner` a `reviewers`, porque aceptar un ADR requiere su OK (igual que en ADR-0008).
- Pendiente arrastrado: la decisión de si el repo será **público o privado** sigue abierta y **no bloquea**, pero cambia los puntos 5 (cuota de GHCR), 6 (minutos ilimitados y runners de 4 vCPU si es público) y 8. El ADR debe decir qué se hace en cada caso.

### architect — 2026-10-21 — RESPUESTA
- Forma: frontmatter completo y secciones estándar (Contexto, Decisión D1–D11, Alternativas descartadas, Consecuencias con números y verificaciones, Pendiente (owner)), `status: in-review`.
- **Público o privado (pendiente arrastrado)** → cerrado: el repo es **público**. Quité del Contexto los 2,000 min/mes como restricción y la estimación de minutos; D7 pasa de presupuesto a objetivos de velocidad. Consecuencias en los puntos 5, 6 y 8.
1. **Backups** → D1 y D9. Postgres como recurso standalone de Coolify; backups diarios a las 09:00 UTC, 7 días locales y 30 fuera del VPS; backup manual antes de releases con migraciones; RPO 24 h y RTO 1 h; prueba de restauración con el invariante del ledger antes del primer despliegue y cada 90 días (verificación 11). El destino externo y el canal de notificaciones quedaron en "Pendiente (owner)".
2. **Healthchecks** → D3. `GET /api/health` con `SELECT 1` y respuesta en códigos; subcomando `server healthcheck` por la imagen distroless; `/healthz` en web con el `node` de la imagen; `web` depende de `server: service_healthy`. Me aparté en un punto: como Postgres ya no está en el compose (punto 1), no puede ser un `depends_on`. Lo cubre `pg_isready` de Coolify más la espera de 60 s del server al arrancar (verificaciones 2 y 3).
3. **Memoria** → D4. Tus números (256 / 384 con heap 256 / 512 con `shared_buffers` 128 MB y 20 conexiones, pool 8), con `memswap_limit = mem_limit` para que un leak termine en reinicio y no pagine. Swap del host de 2 GB con `swappiness` 10 si no existe.
4. **Strip de prefijo** → D2, opción (a). Campos de dominio de Coolify vacíos y etiquetas Traefik propias, sin `stripprefix`; un solo router del server para `/api` y `/ws` con prioridad explícita; rutas iguales en dev y prod. La verificación 1 comprueba que axum ve `/api/health` y `/ws`.
5. **GHCR y despliegue** → D5 y D6. GHCR público, pull sin login; retención de 10 versiones con `actions/delete-package-versions`; `vX.Y.Z` + `sha-…`, nunca `latest`, `${IMAGE_TAG:?}` en el compose; `deploy` en el environment `production` con aprobación del owner, que fija `IMAGE_TAG` y dispara el deploy por la API de Coolify con secretos del environment; `verify` contra `/api/health`; rollback por `workflow_dispatch`; respaldo manual desde Coolify.
6. **Caché, concurrency y timeouts** → D7. `push` a `main` calienta la caché; `rust-cache` con `shared-key` y `save-if` solo en `main`; caché de pnpm y de Playwright; `concurrency` que cancela solo en PRs; `timeout-minutes` en todos los jobs. Con minutos ilimitados, la estimación de minutos se reemplazó por objetivos de duración.
7. **Filtros por ruta** → D7. `dorny/paths-filter` dentro de `changes`, con tu lista (`Cargo.lock`, `rust-toolchain.toml`, `.github/workflows/**`, `justfile`) más `.sqlx/**`, `content/**` y los lockfiles de pnpm; `crates/protocol/**` (y también `engine`/`engine-wasm`) dispara `web`. Un agregador `ci-ok` es el único check requerido, así nada queda en "pending" (verificaciones 6 y 7). Como el repo es público, la protección de rama sí aplica: ruleset sobre `main`.
8. **Plan B, musl y repo público** → D7 y D8. Runner self-hosted **descartado** (Alternativas). Aprobación para workflows de externos, nada de `pull_request_target`, secretos solo en `production`, permisos mínimos y actions fijadas por SHA (verificación 9). musl con rustls (sin OpenSSL, verificado en CI) y `mimalloc`; `SQLX_OFFLINE`; plan B con `cargo zigbuild` y `buildx --platform linux/amd64`.
9. **Despliegues y peleas en vivo** → D10. Apagado ordenado con cierre `1012`; reconexión con backoff y resync sin eventos futuros; el server retoma las peleas desde la BD; migraciones al arrancar y expand/contract, sin tocar `ledger_entries`; no se despliega durante un main event (checklist de aprobación); limpieza de Docker activada. Simulacro en la verificación 12.
10. **Frontmatter** → `approvals: []`, `supersedes: null`, `linear: TA-5`, `depends_on: ["0002", "0003", "0005"]` (0002 por la reanudación de peleas, 0005 por el ledger y los backups), `reviewers: [devops-engineer, owner]`. No agregué 0008 para no crear una dependencia circular (0008 ya depende de 0006); lo cito en el texto.
- **Logs de acceso de Traefik** (pendiente de ADR-0008) → D11. Desactivados; si hace falta diagnosticar, máximo 7 días y sin IP, path con query ni headers. Coherente con ADR-0008 D3 y D8. Aplicarlo en el proxy compartido queda en "Pendiente (owner)".
- **Fuera del ADR**: `docs/architecture.md` (Postgres en el compose) y el intent de 001-fundaciones (pregunta público/privado) quedan desactualizados. Lo anoté en Consecuencias en vez de editarlos.

### devops-engineer — 2026-10-21 — REQUEST_CHANGES
- Verifiqué: el cuerpo reescrito (D1–D11, Alternativas, Consecuencias, 13 verificaciones), no solo la respuesta. Lo comparé con ADR-0002, 0003, 0005 y 0007 (`approved`), con ADR-0008 (D5 y D7, en corrección), con el glosario y con el esquema de frontmatter de `docs/process.md`. El frontmatter es válido y no contradice ningún ADR `approved`. La contradicción con `docs/architecture.md` y con el intent de 001 queda registrada en Consecuencias, lo cual es aceptable.
  - **Puntos 1 a 10 de mi revisión anterior: resueltos en el cuerpo**, con las excepciones que detallo abajo (3 y 5 en parte). Repaso las desviaciones:
    - **Postgres standalone sin `depends_on`**: aceptable. La espera de 60 s más `exit 1` y `restart: unless-stopped` da reintentos con backoff de Docker. `start_period` 30 s + 3 × 15 s (~75 s) cubre la espera, y las verificaciones 2 y 3 lo prueban. Ganamos los backups nativos de Coolify, y eso pesa más que el `depends_on`.
    - **`ci-ok` como único check requerido**: correcto. `if: always()`, `needs` incluye a `changes` y falla con `failure` o `cancelled`. Es el patrón estándar para que los checks no queden en "pending" con filtros dentro del job. Las verificaciones 6 y 7 lo cubren.
    - **Deploy por la API de Coolify desde `production`**: el diseño es correcto (secretos solo en el environment, required reviewer, `IMAGE_TAG` y después `deploy`, `verify` contra `version`, rollback por `workflow_dispatch`). La API de Coolify v4 sirve: `PATCH /api/v1/applications/{uuid}/envs` y `GET /api/v1/deploy?uuid=…`, y el deploy de un compose hace pull de las imágenes. Ver el punto 3 sobre el alcance del token.
    - **Etiquetas Traefik propias con los dominios de Coolify vacíos**: tiene sentido. Con "Domains" vacío Coolify no genera etiquetas `traefik.*` ni `stripprefix` y respeta las del compose. Los entrypoints `http`/`https` y el certresolver `letsencrypt` son los nombres reales del proxy de Coolify v4. `${PUBLIC_HOST}` y `${IMAGE_TAG:?}` se interpolan, y Coolify marca `:?` como variable requerida. La prioridad explícita 100/1 es correcta.
    - **D11, logs de acceso de Traefik apagados**: correcto y coherente con ADR-0008 D3 y D8. El proxy de Coolify por defecto no trae `--accesslog` y su nivel de log es `ERROR`, así que en la práctica es verificar (verificación 13) más que cambiar. Que el OK del owner esté en Pendiente es lo correcto, porque el proxy es compartido.
  - D4 (1,152 MB de peor caso, ~1.2 GB de margen, `memswap_limit = mem_limit`, swap del host de 2 GB), D8 (musl + rustls + mimalloc, `SQLX_OFFLINE`, plan B con zigbuild), D9 (09:00 UTC = 03:00 en CDMX, RPO 24 h y RTO 1 h alcanzables, invariante del ledger según ADR-0005) y D10 (cierre `1012`, `stop_grace_period` mayor que el apagado, expand/contract, resync sin eventos futuros según ADR-0002): correctos y verificables.
- Cambios pedidos (bloqueantes):
  1. **Aislamiento de red en el proxy compartido: `TRUSTED_PROXIES` y la resolución de `server`.** Lo pidió security-reviewer en ADR-0008 (punto 7) y **no está resuelto**: D2 fija `TRUSTED_PROXIES` al CIDR de la red de Coolify, justo lo que hay que evitar. Esa red es compartida, así que cualquier contenedor vecino que llegue a `server:8080` puede falsificar `X-Forwarded-For` (ADR-0008 D7 y test 14). Hay un problema más con "Connect to Predefined Network" (D1): `web` y `server` quedan en la red `coolify`, compartida, con el alias DNS `server`. Si otra app del VPS tiene un servicio llamado `server`, Docker reparte la resolución entre los dos, y el SSR mandaría `__Host-session` y `X-Internal-Secret` (ADR-0008 D5) a un contenedor ajeno. *Cambio:*
     - Una red de Docker **dedicada** (p. ej. `torneo-edge`, `external`, creada una vez en el VPS con subred fija y chica, p. ej. `/28`) donde estén solo `coolify-proxy`, `web` y `server`. El proxy se conecta a ella desde la configuración del proxy en Coolify.
     - `traefik.docker.network=torneo-edge` (no la red de Coolify).
     - `TRUSTED_PROXIES` = la IP fija de `coolify-proxy` en esa red, o su subred si solo contiene esos tres.
     - `web` solo en `torneo-edge`. `server` en `torneo-edge` y, aparte, en la red donde vive el Postgres standalone, solo para salir hacia la BD.
     - `INTERNAL_API_URL` debe resolver a un único contenedor: el alias `server` en `torneo-edge`, o un alias propio y único (p. ej. `torneo-server`).
     - Corregir D1: Traefik no necesita alcanzar a Postgres; solo el server.
     - *Verificaciones nuevas:* (a) desde un contenedor en la red `coolify`, un `curl server:8080` con `X-Forwarded-For` falsificado no cambia la llave del rate limit; (b) desde `web`, `getent hosts <alias>` devuelve una sola IP; (c) `docker network inspect torneo-edge` muestra solo esos tres contenedores, también después de reiniciar el proxy o actualizar Coolify.
  2. **La retención de GHCR rompe las imágenes etiquetadas.** `docker/build-push-action` agrega por defecto una attestation de procedencia (`provenance: mode=min`). Entonces cada tag apunta a un índice cuyos manifiestos hijos (la imagen y la attestation) quedan en GHCR como versiones **sin etiqueta**. Si D5 borra las versiones sin etiqueta, el tag `vX.Y.Z` apunta a manifiestos inexistentes y el pull falla con `manifest unknown`, tanto en el deploy como en el rollback. *Cambio:* fijar `provenance: false` y `sbom: false` (un solo manifiesto por tag), o no borrar las versiones sin etiqueta y conservar las últimas 10 por número total. Hay que fijar los parámetros exactos de `actions/delete-package-versions`: `min-versions-to-keep` y `delete-only-untagged-versions` no se combinan como dice el texto. *Verificación nueva:* después de correr la retención, `docker pull` del tag más viejo que se conserva funciona en el VPS.
  3. **El token de la API de Coolify no se puede limitar a un recurso.** En Coolify v4 los tokens tienen alcance de **equipo**, con permisos `read`/`write`/`deploy`/`read:sensitive`/`root`. Actualizar `IMAGE_TAG` exige `write`, y ese permiso alcanza a **todas** las apps del owner en ese equipo. "Token con el mínimo permiso para … ese recurso" (Pendiente) no es alcanzable tal como está escrito, y el owner tiene que aceptar el riesgo real. *Cambio:* escribir en D6 que el token es de equipo con `write` + `deploy` (sin `root` ni `read:sensitive`), que su filtración da control sobre las demás apps del VPS, y las mitigaciones: secreto solo en el environment `production`; deployment refs del environment limitados a tags `v*` y a `main` (para el rollback por `workflow_dispatch`); rotación del token cada 90 días o ante cualquier sospecha; y la alternativa ya escrita (deploy manual desde la UI) si el owner no acepta exponer la API. Si Coolify permite poner los recursos de Torneo en un equipo propio sobre el mismo servidor, mencionarlo como opción.
- Cambios pedidos (no bloqueantes; pueden ir al plan de 001-fundaciones si el architect prefiere):
  4. Traefik compartido: nombres de routers, servicios y middlewares con prefijo único (`torneo-…`), porque el proveedor Docker de Traefik comparte su espacio de nombres entre todas las apps y un choque deja fuera a ambas. `traefik.enable=true` explícito, y el middleware de redirección `http`→`https` declarado en nuestras etiquetas, sin depender de uno de Coolify.
  5. D11: además del ajuste global, poner `traefik.http.routers.<router>.observability.accesslogs=false` en nuestros routers (Traefik ≥ 3.1, la versión que trae Coolify v4). Así la garantía de ADR-0008 no depende de la configuración del proxy compartido. Agregarlo a la verificación 13.
  6. Healthchecks en forma exec (no hay shell en distroless): `["CMD", "/server", "healthcheck"]` y `["CMD", "/nodejs/bin/node", "-e", "…"]`. Ahí está `node` en `distroless/nodejs22`.
  7. `cargo tree -i openssl-sys` sale con código 101 cuando el paquete **no** está, así que un "falla si encuentra algo" literal falla siempre. Usar `cargo deny` con `bans.deny = ["openssl-sys"]` y ajustar la verificación 8. En musl, preferir sqlx `tls-rustls-ring` (y `ring` como proveedor de rustls) para no arrastrar `aws-lc-sys`, que necesita cmake y clang en el builder.
  8. El job `changes` necesita `permissions: pull-requests: read` para que `dorny/paths-filter` liste los archivos del PR. Los nombres de GHCR van en minúsculas: normalizar `github.repository`.
  9. Que la `version` de `/api/health` se hornee en la imagen (`--build-arg` con el tag), no que se lea de `IMAGE_TAG` en runtime. Así `verify` comprueba la imagen que está corriendo, no la variable.
  10. Ruleset de `main`: PR obligatorio con **0** aprobaciones requeridas (el owner es el único maintainer y no puede aprobar su propio PR). Limpieza de Docker de Coolify con "Delete Unused Volumes" **apagado**. Job `images`: usar `cargo-chef` o equivalente para que la caché de capas cubra las dependencias de Rust y se cumpla el objetivo de ≤ 8 min.

### architect — 2026-10-21 — RESPUESTA (ronda 2)
- Forma: `status: in-review`, `updated: 2026-10-21`. Revisiones anteriores intactas. Verificaciones nuevas 14 a 17. Este ADR queda como **fuente de verdad de la red y del proxy**; ADR-0008 (en corrección aparte, no lo toqué) debe remitir a D1 y D2. Lo anoté en Consecuencias.
1. **Aislamiento de red y `TRUSTED_PROXIES`** → D1 y D2. "Connect to Predefined Network" apagado. Red externa dedicada `torneo-edge` (`10.250.0.0/28`) con solo `coolify-proxy` (IP fija `10.250.0.2`, puesta desde la configuración del proxy en Coolify), `web` y `server`. `traefik.docker.network=torneo-edge`. `TRUSTED_PROXIES = 10.250.0.2/32`: elegí la IP del proxy y no la subred, porque `web` también está en ella y no es un proxy. `web` solo en `torneo-edge`; `server` además en la red de la BD (preferido un destination `torneo-db`, y si no, `coolify`), solo para salir hacia Postgres. `INTERNAL_API_URL=http://torneo-server:8080` con un alias único. D1 corregido: Traefik no alcanza a Postgres. Tus verificaciones: (a) es la 14, (b) y (c) son la 15, que se repite tras reiniciar el proxy y tras actualizar Coolify. Agregué el caso de una red propia del stack que cree Coolify.
2. **Retención de GHCR** → D5. `provenance: false` y `sbom: false`: un manifiesto por tag. El job `build` comprueba con `imagetools inspect --raw` que no es un índice antes de la retención. Parámetros exactos: `package-type: container`, `min-versions-to-keep: 10`, sin `delete-only-untagged-versions` ni `num-old-versions-to-delete`. El rollback por `workflow_dispatch` ya no reconstruye (no se genera otro digest bajo el mismo tag). Verificación 16: pull del tag más viejo conservado. La procedencia firmada queda como opción futura con `actions/attest-build-provenance`, que no crea versiones en el paquete.
3. **Token de Coolify** → D6 y Pendiente. Declarado como token de **equipo** con `write` + `deploy` (sin `root` ni `read:sensitive`), y que su filtración da control sobre las demás apps del VPS. Mitigaciones: secreto solo en `production`; environment limitado a `v*` y `main`; job `deploy` solo con `curl`, sin checkout y con el tag validado por regex; actions por SHA; rotación cada 90 días o ante sospecha. El equipo propio de Coolify queda como opción a confirmar. La decisión es la variable `DEPLOY_MODE` (`api` | `manual`). Mientras el owner no decida, vale **`manual`**: deploy desde la UI y aprobación del environment, después de lo cual corre `verify`. Verificación 17.
4. **Prefijo único en Traefik** → D2. Routers `torneo-api`, `torneo-web`, `torneo-api-http` y `torneo-web-http`; servicios `torneo-api` y `torneo-web`; middleware propio `torneo-https-redirect`; `traefik.enable=true` explícito.
5. **Logs de acceso por router** → D11 y D2. `observability.accesslogs=false` en los cuatro routers. La verificación 13 comprueba Traefik ≥ 3.1, las etiquetas y que `/api/health` no aparece con el access log encendido.
6. **Healthchecks exec** → D3. `["CMD", "/server", "healthcheck"]` y `["CMD", "/nodejs/bin/node", "-e", "…"]`.
7. **`cargo deny`** → D8 y verificación 8. `bans.deny` con `openssl-sys`, `openssl` y `aws-lc-sys`; sqlx `tls-rustls-ring` y `ring` como proveedor de rustls. Quité `cargo tree -i`.
8. **`pull-requests: read` y minúsculas** → D7 (`changes`, y en la lista de permisos) y D5 (`${GITHUB_REPOSITORY,,}`).
9. **`version` horneada** → D3, D6 y verificación 1. `--build-arg APP_VERSION` y `env!` en compilación; el contenedor no recibe `IMAGE_TAG`; `verify` lo comprueba contra la imagen que corre.
10. **Ruleset con 0 aprobaciones, volúmenes y `cargo-chef`** → D7 (0 aprobaciones, con la razón de `docs/process.md`), D10 ("Delete Unused Volumes" apagado) y D5 (`cargo-chef` en el Dockerfile del server para el objetivo de ≤ 8 min).

### devops-engineer — 2026-10-21 — APPROVE (ronda 3)
- Verifiqué: el cuerpo del ADR (D1–D11, Alternativas, Consecuencias, verificaciones 1–17 y Pendiente), no solo la respuesta de la ronda 2. Los 3 bloqueantes y los 7 no bloqueantes de mi revisión anterior están resueltos en el cuerpo, y la solución es correcta en Coolify v4. El frontmatter es válido, `TRUSTED_PROXIES = 10.250.0.2/32` cumple ADR-0008 D7 ("IP del contenedor de Traefik, /32") y el ADR no contradice ningún ADR `approved`. Punto por punto:
  - **`torneo-edge` con `coolify-proxy` en `10.250.0.2`**: es viable. La configuración del proxy en Coolify v4 es un compose editable que Coolify guarda y reutiliza al reiniciar el proxy. Una red externa creada con `--subnet` admite `ipv4_address`, y la IP se conserva en cada `compose up` del proxy. Las actualizaciones de Coolify no reescriben esa configuración. Dos casos sí la borran: "Reset configuration to default" y una migración futura que la regenere. Para eso está la verificación 15, que se repite tras reiniciar el proxy y tras actualizar Coolify. Las redes que Coolify conecta por su cuenta (`docker network connect` de destinations y de la red del stack) se suman a la nuestra y no la quitan.
  - **"Connect to Predefined Network" apagado**: correcto. Coolify crea igual una red con el UUID del recurso, pone ahí `web`, `server` y el proxy, y además respeta los `networks:` externos del compose. D1 y la verificación 15(c) ya cubren esa red. `traefik.docker.network=torneo-edge` fija por cuál red entra Traefik, así que el peer que ve `server` es `10.250.0.2`.
  - **Alias `torneo-server`**: correcto. Solo existe en `torneo-edge`, cuyos únicos miembros son nuestros, y la verificación 15(a) comprueba que resuelve a una sola IP.
  - **Red de Postgres**: viable. Coolify v4 permite crear un destination (red Docker) extra por servidor y elegirlo al crear la base de datos, y el compose puede declarar esa red como externa. El fallback a `coolify` está acotado por la verificación 14.
  - **GHCR**: con `provenance: false` y `sbom: false`, cada tag es un manifiesto de imagen y una sola versión de GHCR (`vX.Y.Z` y `sha-…` comparten digest). `min-versions-to-keep: 10` sin otros parámetros borra solo lo que pasa de 10, y el chequeo con `imagetools inspect --raw` antes de la retención cierra el caso de `manifest unknown`. El rollback sin rebuild es correcto.
  - **`DEPLOY_MODE`**: `manual` por defecto conserva el gate del owner y no expone la API. `api` usa endpoints reales de v4 (`PATCH /api/v1/applications/{uuid}/envs`, `GET /api/v1/deploy`). El riesgo del token de equipo está bien declarado, con permisos `write` + `deploy`, impacto, mitigaciones, rotación cada 90 días y la alternativa de un equipo propio.
  - **Logs por router**: `observability.accesslogs=false` existe desde Traefik 3.1, y la verificación 13 lo prueba de forma empírica.
  - **`cargo deny check bans`** sustituye bien a `cargo tree -i`, y **`cargo-chef`** con `planner → cook → build` es el patrón adecuado para el objetivo de ≤ 8 min.
- No bloqueantes (para el runbook del VPS y el plan de 001-fundaciones):
  1. **Reservar la IP del proxy en IPAM.** Sin `--ip-range`, Docker asigna IPs dinámicas desde `.2`. Si el proxy está caído mientras se redespliega `web` o `server`, uno de ellos puede quedarse con `10.250.0.2`. Entonces el proxy no arranca (afectaría a todas las apps del VPS) y ese contenedor pasaría a estar en `TRUSTED_PROXIES`. Crear la red con `docker network create --driver bridge --subnet 10.250.0.0/28 --gateway 10.250.0.1 --ip-range 10.250.0.8/29 torneo-edge`, así lo dinámico sale de `.8`–`.15` y `.2` queda solo para la IP fija. Lo aplico al crear la red. Conviene corregir el comando en D1.
  2. **`torneo-edge` no se puede borrar.** Si la red no existe, el compose del proxy falla y se caen todas las apps del VPS. Hay que anotarlo en el runbook, junto con que "Reset configuration to default" del proxy quita la red y la IP.
  3. **Caché de imágenes**: `cache-to: type=gha,mode=max` (con `mode=min` no se guarda la etapa `cook` de `cargo-chef`) y un `scope` por imagen (`scope=server`, `scope=web`). Si no, las dos imágenes se pisan la caché.
  4. **`DEPLOY_MODE` a prueba de fallos**: cualquier valor distinto de `api`, incluido no definirla, se trata como `manual`. Con `api`, la lista "Allowed IPs" de la API de Coolify no puede limitarse a los runners de GitHub, porque sus IPs cambian. Ese es un argumento más para `manual`.
  5. **`deny.toml`**: usar la sintaxis actual `{ crate = "openssl-sys" }` y `[graph] targets = ["x86_64-unknown-linux-musl"]`, para evaluar las dependencias del target real. Con un destination `torneo-db`, Coolify también conecta el proxy a esa red. Es inofensivo, porque Traefik entra por `torneo-edge`, pero la verificación 15 no debe tomarlo como un fallo.
