---
id: "0006"
title: Despliegue en Coolify y CI con presupuesto cero
type: adr
status: changes-requested
risk: medium
author: architect
reviewers: [devops-engineer]
created: 2026-10-20
updated: 2026-10-20
---

# ADR-0006: Despliegue en Coolify y CI con presupuesto cero

## Contexto
- VPS Hostinger KVM 1: **1 vCPU, 4 GB de RAM (~40% ya en uso), 50 GB de disco (22 usados), 4 TB de transferencia.** Ya tiene Coolify.
- GitHub Actions: 2,000 min/mes gratis en repos privados. Presupuesto: 0.

## Decisión
1. **Coolify despliega `infra/docker-compose.yml`** (`web`, `server`, `postgres`). Usamos el proxy de Coolify (Traefik) con HTTPS automático, sin Caddy. Un solo dominio: `/` va a web y `/api` y `/ws` van al server.
2. **Nunca compilar en el VPS**: Rust con 1 vCPU es lentísimo y puede quedarse sin memoria. Las imágenes se construyen fuera y se suben a GHCR; Coolify solo descarga y corre.
3. **Disciplina con los minutos de CI:**
   - Solo runners `ubuntu-latest` (macOS cuenta 10x).
   - CI de tests solo en PRs, con filtros por ruta (los cambios en `docs/**` no corren Rust).
   - Caché de cargo (`Swatinem/rust-cache`) y de pnpm.
   - Build y push de imágenes **solo al crear un tag de release**, no en cada push.
   - `just check` local y un hook `pre-push` como primera línea de defensa.
4. **Estimación**: unos 15 PRs/mes × ~8 min, más ~6 releases × ~15 min, da **≈ 200–500 min/mes**.
5. **Plan B (si se acaban los minutos):**
   - Repo **público**: los minutos son ilimitados en runners estándar, y además aporta transparencia ("provably fair"). La seguridad no depende de ocultar el código.
   - **Runner self-hosted** en la Mac del owner: no consume minutos.
   - Build local con `cargo zigbuild` (cross-compile a `x86_64-unknown-linux-musl`) y una imagen mínima.

## Consecuencias
- En ejecución estimamos ~300–500 MB (server Rust ~50 MB, Postgres ~200 MB, web node ~100 MB): cabe en el VPS actual.
- Si se escala, el primer paso es subir el plan del VPS o mover Postgres fuera.

## Pendiente
- Decidir si el repo será público o privado.

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

