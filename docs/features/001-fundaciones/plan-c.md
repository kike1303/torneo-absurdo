---
id: "001"
title: Fundaciones del monorepo — fase C
type: plan
status: approved
risk: high
author: devops-engineer
reviewers: [architect, security-reviewer]
approvals:
  - by: architect
    date: 2026-10-24
  - by: security-reviewer
    date: 2026-10-24
  - by: owner
    date: 2026-10-24
linear: TA-1
depends_on: []
created: 2026-10-24
updated: 2026-10-26
---

# Plan: Fundaciones del monorepo — fase C (desde spec.md 2026-10-23)

> [intent](./intent.md) · [spec](./spec.md) · [plan-a](./plan-a.md) · [plan-b](./plan-b.md) · ADRs: [0006](../../decisions/0006-despliegue-y-ci.md), [0008](../../decisions/0008-autenticacion.md) (D7)

**Alcance:** RF-C1 a RF-C19, los RNF de memoria, velocidad de CI, costo y seguridad que dependen de la infraestructura, y la confirmación del plazo de logs del aviso de privacidad. Rama base `ta-1-fundaciones-fase-c`, que sale de `main` con A y B fusionadas. No se despliega a producción.

**Un plan, dos PRs.** La spec permite partir C en C1 (RF-C1 a RF-C7) y C2 (RF-C8 a RF-C18). Los parto, porque juntos superan los 40 archivos y mezclan dos riesgos distintos: el build cruzado y la forma local por un lado, y la política de la CI por el otro. Mantengo **un solo plan** porque la fase es una: C2 llama a las recetas y a los scripts de C1 (`check-compose.mjs`, `check-dockerignore.sh`, `prod-local`), y el checklist de RF-C19 abarca los dos PRs. Así los revisores aprueban el contrato completo una vez.
- **PR C1** (`ta-1-fundaciones-fase-c1`): Dockerfiles, `.dockerignore`, compose de producción y local, `infra/local.env`, `check-compose.mjs`, `check-dockerignore.sh`, `local-networks.sh`, recetas `prod-local`. No hay CI todavía, así que la evidencia es local (Mac ARM con Colima), como en A y B.
- **PR C2** (`ta-1-fundaciones-fase-c2`, sale de `main` con C1 fusionado): `ci.yml`, `release.yml`, acciones compuestas locales, `check-workflows.mjs`, Dependabot, el hook `pre-push` y los docs. Es el primer PR que corre la CI, y sus corridas son la evidencia de C1 a C4 y de la forma Linux amd64 de C7.
- **Cierre** (RF-C19): después del merge de C2, con el OK del owner.

**Bloqueo de inicio (paso C1-0).** Las imágenes `distroless/*-debian12` están **deprecadas**: el README de distroless dice hoy que solo `-debian13` recibe actualizaciones, y que cualquier otra etiqueta "se considera deprecada y ya no se actualiza". Lo comprobé en el registro el 2026-10-24: `nodejs22-debian12:nonroot` no cambia desde el 2026-02-14 y `static-debian12:nonroot` desde el 2026-08-21. RF-C1, RF-C2 y ADR-0006 D5 nombran `-debian12`. Propongo `static-debian13:nonroot` y `nodejs22-debian13:nonroot` (decisión 2). El architect ya escribió la **enmienda menor de la spec** (RF-C1 y RF-C2) y la nota en ADR-0006 D5. Como las escribió él, las aprueban **security-reviewer y el owner**, no el architect. security-reviewer ya la dio por aprobada y preferida en su revisión de este plan, así que falta el OK del owner en el chat. Si se rechaza, el plan funciona igual con `-debian12` (los digests están en "Versiones fijadas"), con el riesgo de una base sin parches, y además **bloquea TA-8** (Riesgos).

## Decisiones de implementación

1. **Builder de Rust.** `FROM --platform=$BUILDPLATFORM rust:1.99.1-slim-bookworm@sha256:…` (índice multi-arquitectura). Es la misma versión de `rust-toolchain.toml`, así que rustup no descarga otra toolchain: solo agrega el target musl y los componentes que pide el archivo. `slim` no trae `xz-utils`, y se instala con `apt-get` para descomprimir zig. `ca-certificates`, `gcc` y `wget` ya vienen en la imagen: `gcc` compila los `build.rs` para el host. Descarto la variante `trixie`: su digest actual es del 2026-10-24 y no cumple los 3 días.
2. **Bases finales en Debian 13** (ver "Bloqueo de inicio"). `gcr.io/distroless/static-debian13:nonroot` y `gcr.io/distroless/nodejs22-debian13:nonroot`, las dos por digest de índice. Verifiqué el digest de `nodejs22-debian13`: está etiquetado `nonroot-e7bfa5b…`, y ese commit de distroless fija Node **v22.23.4**, igual que `.node-version`. Su `Entrypoint` es `/nodejs/bin/node` (la ruta del healthcheck de D3), el usuario es `65532` y el `WorkingDir`, `/home/nonroot`. Distroless publica `created: 1970-01-01`, así que la antigüedad se mide con el `timeUploadedMs` de la lista de etiquetas de `gcr.io`.
3. **Zig y cargo-zigbuild, la combinación conservadora.** Zig **0.15.2** (2025-10-11) con `cargo-zigbuild` **0.23.4** (2026-09-02). Zig 0.16 y 0.17 son más nuevos, pero `cargo-zigbuild` corrigió fallos con 0.16 (#480, #486) y 0.17 (#506) recién en la 0.24.0, publicada el 2026-10-21. Nuestro grafo no usa `bindgen`, que es el caso conocido de zig ≥ 0.15 (README de cargo-zigbuild), y `ring` y `libmimalloc-sys` compilan con `cc`. La descarga de zig elige el tarball y el checksum según `BUILDARCH`: `amd64` (Actions) y `arm64` (Mac). Cualquier otro valor falla con `no zig checksum for $BUILDARCH`. Desde 0.15 el nombre del tarball es `zig-<arch>-linux-<versión>.tar.xz`. La descarga usa `wget --https-only` (o `curl --proto '=https'`) y `sha256sum -c` antes de extraer. **Firma minisign de zig (opcional para security-reviewer): no la adopto en C.** Exigiría bajar y verificar otro binario (`minisign`) en la etapa `chef`, y el SHA-256 fijado en el repo ya cubre cualquier cambio posterior del tarball. Queda como mejora si se sube de versión de zig.
4. **`cargo-chef` 0.1.78** (2026-08-12; tiene `cook --zigbuild`, verificado en `src/main.rs` del tag) y `cargo-zigbuild` con `cargo install --locked --version`. Las dos instalaciones van en la etapa `chef`, antes de copiar el código, para que la caché `mode=max` las conserve.
5. **Etapas del server, con COPY explícitos.** `chef` (herramientas y `rust-toolchain.toml`) → `planner` (`cargo chef prepare`) → `cook` (`cargo chef cook --zigbuild --release --locked --target x86_64-unknown-linux-musl -p server`) → `build` (`ARG APP_VERSION=dev` **después** del `cook`; `cargo zigbuild --release --locked --target x86_64-unknown-linux-musl -p server`, con el mismo target, perfil y paquete que el `cook`, nota 8 de devops) → final. `planner` y `build` copian solo `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `apps/server/` y `crates/`, no `COPY . .`: así un cambio en `docs/` no invalida la compilación. `.dockerignore` sigue siendo la red de seguridad para `apps/server/.env`, que este `COPY` sí alcanzaría. Va con `ENV SQLX_OFFLINE=true` aunque hoy no hay `query!` (sin `.sqlx/`): `migrate!` lee `apps/server/migrations` al compilar. El valor por defecto `dev` de `APP_VERSION` existe porque un `ARG` vacío llega como `APP_VERSION=""`, y `build.rs` lo rechazaría (RF-B10).
6. **Final del server:** `COPY --from=build …/x86_64-unknown-linux-musl/release/server /server`, `EXPOSE 8080`, `ENTRYPOINT ["/server"]`. Sin `CMD`, porque sin argumentos es `serve` (`cli.rs`). Las etapas finales no tienen `RUN`, así que en una Mac ARM el build no emula nada: solo se emula al ejecutar.
7. **Web.** La etapa `build` (`FROM --platform=$BUILDPLATFORM node:22.23.4-bookworm-slim@sha256:…`) corre `corepack enable` y copia primero los manifiestos (`package.json`, `pnpm-lock.yaml`, `pnpm-workspace.yaml`, `.npmrc`, `apps/web/package.json` y `packages/protocol-ts/package.json`). Después corre `pnpm install --frozen-lockfile --filter web...`, con caché de `/pnpm/store` en un `--mount=type=cache`, y luego copia `apps/web/` y `packages/` y corre `pnpm --filter web build` (incluye `sync` de Paraglide). La etapa final es `WORKDIR /app`, `COPY build/`, `COPY apps/web/package.json` (por `"type": "module"`), `ENV NODE_ENV=production` y `CMD ["build/index.js"]`. Sin `node_modules`. `COREPACK_ENABLE_DOWNLOAD_PROMPT=0`; corepack verifica pnpm con el hash de `packageManager`.
8. **Sin directiva `# syntax=`.** Usarla descargaría otra imagen (`docker/dockerfile`) que también habría que fijar por digest. El frontend que trae BuildKit 0.30 (Colima) y el de Actions alcanzan, porque no uso heredocs ni opciones nuevas.
9. **`check-compose.mjs` lee el YAML con `yaml` 2.9.1** (sin dependencias, raíz `devDependencies`). Las cabeceras salen de `SECURITY_HEADERS` de `apps/web/src/lib/server/hooks/security-headers.ts`, la fuente única que sugirió devops en la enmienda menor. El script extrae el objeto con una regex sobre el texto y falla si no encuentra exactamente las tres entradas. No importa el `.ts`, para no depender de `--experimental-strip-types` ni de los tipos de Kit. El mapeo: `nosniff` → `contentTypeNosniff=true`, `referrer-policy` → `referrerPolicy=<valor>` y `max-age=N` → `stsSeconds=N`. Si el valor de HSTS trae `includeSubDomains` o `preload`, falla. Las claves de las etiquetas se comparan sin distinguir mayúsculas (`observability.accesslogs` y `accessLogs` son la misma para Traefik).
10. **Reglas de `check-compose.mjs`.** Son las de RF-C6, más las extensiones de las revisiones. Cada regla tiene un fixture que falla y un mensaje en inglés que nombra la regla.
    - **`docker-compose.yml`:**
      - servicios exactamente `web` y `server`; ni `build:` ni `ports:`;
      - `image` igual a `ghcr.io/kike1303/torneo-absurdo/<svc>:${IMAGE_TAG:?…}`;
      - las etiquetas exactas de D2 y RF-C4 (abajo);
      - `torneo-web-headers` con los cinco valores, solo en `torneo-web`, y nunca en `torneo-web-http`;
      - sin `forceSTSHeader`, `stsIncludeSubdomains=true` ni `stsPreload=true`;
      - `torneo-https-redirect`, idéntico en los dos servicios si se declara en ambos;
      - redes `torneo-edge` y `torneo-db` `external: true`; `web` solo en `torneo-edge`; `server` en `torneo-edge`, con el alias `torneo-server`, y en `torneo-db`;
      - healthchecks exactos (forma exec, 15 s / 3 s / 3 / 30 s);
      - `depends_on.server.condition: service_healthy`;
      - `mem_limit` = `memswap_limit` (256m y 384m);
      - `restart: unless-stopped`;
      - `logging` `json-file` con `max-size: "10m"` y `max-file: "3"`;
      - `stop_grace_period: 15s` en `server`;
      - `platform: linux/amd64`, `read_only: true`, `cap_drop: [ALL]` y `security_opt: ["no-new-privileges:true"]` en los dos;
      - `tmpfs` de `web` con `size=`.
    - **Variables de entorno, con lista cerrada por servicio:**
      - `web`: `PUBLIC_ORIGIN`, `ADDRESS_HEADER`, `XFF_DEPTH`, `HOST`, `PORT` y `NODE_OPTIONS`.
      - `server`: `DATABASE_URL`, `PUBLIC_ORIGIN` y `RUST_LOG`.
      - Una variable fuera de la lista falla, así que `ORIGIN`, los `*_HEADER` y `BIND_ADDR` quedan prohibidas por partida doble (lista cerrada y regla explícita, con un fixture cada una).
      - `PUBLIC_ORIGIN` es idéntico en los dos servicios.
      - `RUST_LOG` no contiene `debug` ni `trace` en ninguna directiva.
      - `ENABLE_DEV_CATALOG` y `ENABLE_WS_ECHO` no aparecen en ningún archivo de compose.
    - **`compose.local.yml`:** solo agrega `postgres` (mismo digest que `compose.dev.yml`, 512m = `memswap_limit`, solo en `torneo-db`) y los `ports` de `web` (`127.0.0.1:3000:3000`) y `server` (`127.0.0.1:8080:8080`). No toca `image`, `labels`, `networks` ni `environment` de `web` ni de `server`, y todo `ports` empieza con `127.0.0.1:`.
    - **`.dockerignore`:** las entradas de RF-C3 con su forma `**/`. Además, ningún archivo `*.dockerignore` fuera de la raíz: BuildKit le da prioridad a `<Dockerfile>.dockerignore` sobre el de la raíz (nota 1 de devops). Se busca con `git ls-files` más los archivos sin rastrear que no estén ignorados.
    - **Dockerfiles:** todo `FROM` lleva `@sha256:`; ninguno usa `latest`; la versión de `FROM rust:` es igual al `channel` de `rust-toolchain.toml`; la de `FROM node:` es igual a `.node-version`; la base final de `web` es `nodejs<major de .node-version>-`.
11. **`check-dockerignore.sh` no toca archivos del desarrollador** (nota 1 de devops a la spec, ronda 2; condición 5 de security-reviewer).
    - `set -eu`. Tres sujetos: `infra/.env`, `apps/server/.env` y `apps/web/.env`. Para cada uno, si no existe, lo crea con un canario **con `set -C`** (noclobber: si el archivo aparece entre la comprobación y la escritura, la redirección falla en vez de pisarlo) y lo anota en una lista. El `trap … EXIT` borra **solo** lo que está en esa lista. Si ya existe (el `.env` de desarrollo de `just dev`), lo usa como sujeto sin abrirlo ni modificarlo.
    - Construye `scripts/fixtures/context.Dockerfile` (`FROM scratch` + `COPY . /ctx`) con `docker buildx build -f … --output type=local,dest="$tmp" .`, con `tmp="$(mktemp -d)"`, y comprueba que **no** existen `ctx/infra/.env`, `ctx/apps/server/.env`, `ctx/apps/web/.env`, `ctx/private`, `ctx/.git`, `ctx/content/_local` ni ningún `ctx/**/node_modules` o `ctx/**/target`, y que **sí** existe `ctx/apps/server/.env.example`.
    - **Si falla, imprime solo la ruta** del archivo que se coló (`leaked into build context: apps/web/.env`). Nunca su contenido: ni `cat`, ni `diff`, ni `ls -l` del archivo, porque el `.env` real del desarrollador puede ser el sujeto.
    - El directorio temporal se borra al salir con `rm -rf "${tmp:?}"`, que aborta si `tmp` quedó vacío.
    - **Contexto mínimo.** Copiar el repo entero a `--output type=local` en local mandaría a BuildKit todo lo que `.dockerignore` no excluye (unos MB). Lo acepto: es lo que prueba la regla.
12. **`prod-local`.**
    - **Redes.** `scripts/local-networks.sh` las crea con `--label torneo.local=1`. Si `torneo-edge` existe sin la etiqueta, la usa sin tocarla y lo avisa. Si `docker network create` falla por una subred solapada, sale con 1 y un mensaje que dice qué red choca (`docker network inspect` de las existentes) y cómo resolverlo.
    - **Comprobaciones previas.** Que `127.0.0.1:3000` y `127.0.0.1:8080` estén libres: `just dev` usa el 8080 y entonces falla con un mensaje claro. Que exista el contexto de Docker (`docker context show` se imprime).
    - **Entorno.** Compose corre con `env -u DATABASE_URL -u PUBLIC_HOST`, con `IMAGE_TAG` exportado, `-p torneo-local` y `--env-file infra/local.env` (nota 5 de devops). Sin `name:` en el compose de producción, porque Coolify pone el suyo.
    - **Recetas:**
      - `prod-local [tag]`: sin `tag` construye `local`; con `tag`, hace `pull` de GHCR.
      - `prod-local-down`: `down` sin `-v` y borra solo las redes con la etiqueta.
      - `_prod-local-up` (interna): levanta con un `IMAGE_TAG` dado sin construir. La usa el job `images` (decisión 16).
    - **Postgres local:** `command: ["postgres", "-c", "shared_buffers=128MB", "-c", "max_connections=20", "-c", "work_mem=4MB"]`, como el de Coolify (D4).
13. **`infra/local.env`:** `PUBLIC_HOST=localhost` y `DATABASE_URL=postgres://torneo:torneo_local@postgres:5432/torneo`. Es solo de la red `torneo-db` local y nunca se publica. Lleva el comentario de RF-C5. Ni `.gitignore` ni `.dockerignore` lo cubren (`.env.*` no coincide con `local.env`), y está bien así.
14. **Acciones compuestas locales** en `.github/actions/`, para no repetir pasos y para que `policy` las analice (RF-C12):
    - `setup-just`: descarga `just` 1.58.0 de las releases de `casey/just` con `curl -fsSL --proto '=https' --tlsv1.2 -o "$RUNNER_TEMP/just.tar.gz"`, corre `echo "<sha256>  just.tar.gz" | sha256sum -c -` **antes** de extraer, y extrae **solo el binario** (`tar -xzf … -C "$RUNNER_TEMP/bin" just`) a `$RUNNER_TEMP/bin`. Si `sha256sum -c` falla, el paso falla sin haber extraído nada (condición 6 de security-reviewer).
    - `setup-js`: `actions/setup-node` con `node-version-file: .node-version`, `corepack enable`, caché del store de pnpm (llave: hash de `pnpm-lock.yaml`) y `pnpm install --frozen-lockfile`, con un input `filter` opcional.
    - **La caché se lee en todos lados y se escribe solo en `main`** (condición 7 de security-reviewer). `actions/cache` a secas también guarda al final de un PR, así que `setup-js` usa `actions/cache/restore` antes del install y `actions/cache/save` con `if: github.event_name == 'push' && github.ref == 'refs/heads/main' && steps.restore.outputs.cache-hit != 'true'`. Son subacciones del mismo repo y se fijan con el mismo SHA de `actions/cache` (tabla de versiones). La caché de navegadores de Playwright del job `e2e` sigue el mismo patrón. Así, la frase de "Estimación de CI" ("la caché la escriben solo `main`") es cierta para todas las cachés: rust-cache (`save-if`), pnpm, Playwright y `gha` de las imágenes.
    - **Desvío de ADR-0006 D7.** No uso `cache: pnpm` de `setup-node`, porque necesita pnpm en el `PATH` **antes** de `setup-node`, y con corepack eso solo pasa después. El resultado es el mismo: una caché del store por lockfile.
15. **Runner fijado en `ubuntu-24.04`**, no en `ubuntu-latest`, para que un cambio de imagen del runner no rompa la CI sin aviso. `check-workflows.mjs` exige `runs-on: ubuntu-*`.
16. **`images` también levanta `prod-local`** (extensión de RF-C11).
    - **Nombres y versión.** Construye las dos imágenes con `load: true` y los nombres que espera el compose de producción: `ghcr.io/kike1303/torneo-absurdo/server:ci` y `ghcr.io/kike1303/torneo-absurdo/web:ci`, con `build-args: APP_VERSION=sha-<7>` (los 7 primeros caracteres de `github.event.pull_request.head.sha` o de `github.sha`, calculados en un paso previo y pasados por `env:`). En local, `just prod-local` sin tag construye `…/server:local` y `…/web:local` (hallazgo 2 del architect). Ningún nombre `torneo/*` en recetas ni en pasos.
    - **Comprobaciones de las imágenes**, antes de levantar nada:
      - `docker inspect -f '{{.Config.User}}'` de las dos imágenes da `65532` o `nonroot`, y cualquier otro valor (vacío, `0`, `root`) falla. Así, si se cuela un digest de la variante root bajo la etiqueta `:nonroot`, el job lo detecta (condición de security-reviewer).
      - `docker run --rm ghcr.io/kike1303/torneo-absurdo/web:ci --version` (el `Entrypoint` es `/nodejs/bin/node`, así que el argumento reemplaza al `CMD`) es exactamente `v$(cat .node-version)`; si no coincide, falla (condición 4 de security-reviewer).
      - El binario del server pasa `readelf -lW` sin `INTERP` (RF-C1) y la imagen web no tiene `app/node_modules/` (RF-C2).
    - **Arranque.** Corre `IMAGE_TAG=ci just _prod-local-up` y comprueba:
      - los tres contenedores `healthy`;
      - `/api/health` 200 con `version` igual a `sha-<7>`;
      - `/healthz` 200;
      - los `curl -I` de RF-C7.
    - **RNF de memoria, que hace fallar el job** (hallazgo 5 del architect). Tras 30 s de reposo con los tres `healthy`, `docker stats --no-stream --format '{{.Name}} {{.MemUsage}}'` va al log, y el paso convierte el uso a bytes y falla si `server` llega a 64,000,000 o `web` a 200,000,000 (los MB del RNF en base 10, un poco más estrictos que los MiB que muestra Docker). Postgres solo se registra. Si en la primera corrida aparece ruido que lo vuelva inestable, no se relaja el umbral: se anota en "Desviaciones" y se discute con el architect.
    - **Siempre** hace `prod-local-down` al final (`if: always()`).
    - **`timeout-minutes: 25`**, y el architect autorizó subirlo hasta **35** si la primera corrida en frío lo pide (hallazgo 4). Si se sube, va en "Desviaciones" con la duración medida.
    - **Filtro de `images`** (RF-C8): los de D7 más `scripts/check-compose.mjs`, `scripts/check-dockerignore.sh`, `scripts/fixtures/context.Dockerfile`, `scripts/local-networks.sh` (hallazgo 3 del architect), `infra/**`, `.dockerignore`, `Cargo.lock`, `rust-toolchain.toml`, `pnpm-lock.yaml` y `justfile` (por `_prod-local-up`).

    Es la evidencia **Linux amd64** de C7 y del RNF de memoria en cada PR que toque imágenes, sin pedirle al owner una máquina Linux. Cuesta unos 60–90 s (el pull de Postgres y la espera de `--wait`).
17. **La CI de Rust.**
    - **Herramientas.** El job `rust` instala Node y pnpm con `setup-js` completo, sin filtro, porque `_check-gen` formatea con Prettier y `prettier-plugin-tailwindcss` resuelve `tailwindcss` desde `apps/web` (`tailwindStylesheet`). `musl-tools` va por `apt`. `cargo-deny` 0.20.2 es el binario de su release, con SHA-256 fijado: `cargo install` tardaría unos 3 min por corrida. La descarga sigue el patrón de `setup-just`: `curl -fsSL --proto '=https' --tlsv1.2`, `sha256sum -c` antes de extraer y extracción solo del binario (`tar -xzf … --strip-components=1 cargo-deny-0.20.2-x86_64-unknown-linux-musl/cargo-deny`).
    - **Orden de los pasos:** `just check-rust` → `just deny` (Actions ya define `CI=true`) → `cargo build -p server --locked --target x86_64-unknown-linux-musl` (perfil `dev`) → `readelf -lW … | grep -q INTERP` y, si aparece, falla.
    - **Caché.** `Swatinem/rust-cache` usa `shared-key: ci` y `save-if` solo en `main`.
18. **El job `policy`:** `setup-just` y `setup-js` con `filter: .` (solo la raíz: `yaml` y `vitest`, sin navegadores ni Rust). Después corre `node scripts/check-workflows.mjs`, `pnpm exec vitest run --config scripts/vitest.policy.config.mjs` (solo los tests de `check-workflows`) y `scripts/check-hooks.sh`, que clona en un directorio temporal, corre `just hooks` y comprueba `core.hooksPath`, el bit de ejecución y que el hook invoca `just check`. `scripts/vitest.config.mjs` excluye esos tests, así que en `web` no corren (RF-C10).
19. **El cálculo de `ci-ok`:** con `if: always()` y `needs` igual a los seis jobs, falla si algún resultado es `failure` o `cancelled`. Un job `skipped` cuenta como éxito:
    ```yaml
    env:
      RESULTS: ${{ join(needs.*.result, ' ') }}
    run: |
      for r in $RESULTS; do case "$r" in failure|cancelled) echo "a required job ended in $r" >&2; exit 1;; esac; done
    ```
20. **El release no mueve etiquetas.** Antes del push, `build` comprueba con `docker buildx imagetools inspect` que ni `vX.Y.Z` ni `sha-<7>` existen todavía en GHCR, y si existen, falla. Esto cubre "una etiqueta publicada nunca se mueve" (D5) cuando se relanza un job. Las imágenes llevan `org.opencontainers.image.source=https://github.com/kike1303/torneo-absurdo` para que el paquete quede vinculado al repo: lo necesitan los permisos del `GITHUB_TOKEN` en la retención y la visibilidad.
21. **`deploy` y `verify`.**
    - `deploy`, `verify` y `ci-ok` llevan `permissions: {}` (decisión 23): ninguno lee el repo ni toca paquetes.
    - `deploy` recibe el tag ya validado por `needs.build.outputs.tag`, vía `env:`. Con `DEPLOY_MODE == 'api'` sale con 1 y el mensaje `DEPLOY_MODE=api is not implemented: it requires a new OK from the owner`, que no nombra a la plataforma (RF-C12). Con cualquier otro valor, registra el tag aprobado.
    - `verify` lee `vars.PUBLIC_HOST` por `env:` y falla si está vacía o si no es un nombre de host (`^[a-z0-9.-]+$`). Después consulta `https://$PUBLIC_HOST/api/health` cada 10 s durante 5 min y compara `version` con el tag usando `jq`.
22. **Dependabot.**
    - Ecosistemas y directorios: `github-actions` (`directories: ["/", "/.github/actions/*"]`, porque las acciones compuestas fijan SHAs), `cargo` (`/`), `npm` (`/`, workspace de pnpm), `docker` (`directories: ["/apps/server", "/apps/web"]`) y `docker-compose` (`/infra`).
    - Todos van con `schedule.interval: weekly` y `cooldown: { default-days: 3 }`. Confirmé en la referencia de opciones de GitHub que `default-days` lo admiten los cinco, incluidos `docker-compose` y `github-actions`.
    - Sin `groups` ni automerge. `open-pull-requests-limit: 5`.
23. **Permisos de los workflows, con lista cerrada por job** (condición 1 de security-reviewer). Los dos workflows declaran `permissions: { contents: read }` en la raíz (RF-C12). Cada job declara su bloque, y `check-workflows.mjs` lo compara **exacto** contra esta tabla: un permiso de más, de menos o con otro nivel falla.

    | Workflow / job | `permissions` |
    |---|---|
    | `ci.yml` / `changes` | `contents: read`, `pull-requests: read` |
    | `ci.yml` / `policy`, `rust`, `web`, `e2e`, `images` | `contents: read` |
    | `ci.yml` / `ci-ok` | `{}` |
    | `release.yml` / `build` | `contents: read`, `packages: write` |
    | `release.yml` / `deploy`, `verify` | `{}` |

    - Un job nuevo que no esté en la tabla falla: hay que agregarlo al script en el mismo PR, a la vista del revisor.
    - Las formas abreviadas `write-all` y `read-all`, en la raíz o en un job, fallan siempre. `id-token` falla con cualquier valor.
    - `continue-on-error` está prohibido en cualquier job o paso de `ci.yml` y de `release.yml`: un paso que "falla en verde" haría pasar a `ci-ok`.
    - Fixtures nuevos, uno por regla: `id-token: write` en un job, `write-all` en la raíz, `read-all` en un job, `packages: write` en `ci.yml/images`, `deploy` con `contents: read` en vez de `{}`, un job fuera de la tabla, `continue-on-error: true` en un paso y `continue-on-error: false` en un job (también falla: la clave no debe aparecer).
24. **Hook:** `scripts/git-hooks/pre-push` en POSIX `sh`, con `exec just check`, comentado con lo de RF-C18, y con el bit de ejecución en git (`git update-index --chmod=+x`). La receta `just hooks` corre `git config core.hooksPath scripts/git-hooks`.

**Etiquetas del compose (forma exacta que exige `check-compose.mjs`).** Las de `server` son iguales a las de `web`, con estos cambios:
- el router `torneo-api` en lugar de `torneo-web`;
- la regla ``Host(`${PUBLIC_HOST:?…}`) && (PathPrefix(`/api`) || PathPrefix(`/ws`))``;
- `priority=100` y el puerto `8080`;
- sin `torneo-web-headers`, porque axum ya pone `nosniff` y HSTS (RF-B12).

```yaml
web:
  labels:
    - traefik.enable=true
    - traefik.docker.network=torneo-edge
    - traefik.http.routers.torneo-web.rule=Host(`${PUBLIC_HOST:?PUBLIC_HOST is required}`)
    - traefik.http.routers.torneo-web.priority=1
    - traefik.http.routers.torneo-web.entrypoints=https
    - traefik.http.routers.torneo-web.tls.certresolver=letsencrypt
    - traefik.http.routers.torneo-web.service=torneo-web
    - traefik.http.routers.torneo-web.middlewares=torneo-web-headers
    - traefik.http.routers.torneo-web.observability.accesslogs=false
    - traefik.http.routers.torneo-web-http.rule=Host(`${PUBLIC_HOST:?PUBLIC_HOST is required}`)
    - traefik.http.routers.torneo-web-http.priority=1
    - traefik.http.routers.torneo-web-http.entrypoints=http
    - traefik.http.routers.torneo-web-http.service=torneo-web
    - traefik.http.routers.torneo-web-http.middlewares=torneo-https-redirect
    - traefik.http.routers.torneo-web-http.observability.accesslogs=false
    - traefik.http.services.torneo-web.loadbalancer.server.port=3000
    - traefik.http.middlewares.torneo-https-redirect.redirectscheme.scheme=https
    - traefik.http.middlewares.torneo-https-redirect.redirectscheme.permanent=true
    - traefik.http.middlewares.torneo-web-headers.headers.contentTypeNosniff=true
    - traefik.http.middlewares.torneo-web-headers.headers.referrerPolicy=strict-origin-when-cross-origin
    - traefik.http.middlewares.torneo-web-headers.headers.stsSeconds=31536000
    - traefik.http.middlewares.torneo-web-headers.headers.stsIncludeSubdomains=false
    - traefik.http.middlewares.torneo-web-headers.headers.stsPreload=false
```

## Versiones fijadas

Verificadas el **2026-10-24**:
- actions: `gh api` sobre `releases`, `git/ref/tags` (derreferenciando los tags anotados) y `commits`;
- imágenes de Docker Hub: `timeout curl` a `hub.docker.com/v2/repositories/…/tags`;
- distroless: `timeout curl` a `gcr.io/v2/distroless/<img>/tags/list` (`timeUploadedMs`) y a los manifiestos;
- crates: `timeout curl` a la API de crates.io;
- zig: `ziglang.org/download/index.json`;
- `just` y `cargo-deny`: los `SHA256SUMS` y `.sha256` de sus releases.

Regla: **3 días o más** desde la publicación del tag o de la subida del digest. El paso C1-1 vuelve a comprobar toda la tabla. Si una pieza cambió (un digest movido o una versión retirada), se anota en "Desviaciones" y se usa la anterior que cumpla la regla.

**Actions** (todas `node24` salvo la que se indica):

| Action | Tag | SHA | Fecha del tag |
|---|---|---|---|
| `actions/checkout` | v7.0.1 | `3d3c42e5aac5ba805825da76410c181273ba90b1` | 2026-07-20 |
| `actions/setup-node` | v7.1.0 | `949feb2413d6458794dcd2491c4babbbce0c15c1` | 2026-10-08 |
| `actions/cache` | v6.1.0 | `55cc8345863c7cc4c66a329aec7e433d2d1c52a9` | 2026-06-26 |
| `actions/upload-artifact` | v7.0.2 | `cf430e030ddbb5b0abf93d22962f4752f3646cd9` | 2026-10-07 |
| `Swatinem/rust-cache` | v2.9.3 | `d1527c8f67ef9af357290475d4af4b0fab8f12e0` | 2026-10-20 |
| `dorny/paths-filter` | v4.0.3 | `ceb8a2b8f2d89434be7ff52d3de7ec3738c5cc9d` | 2026-08-05 |
| `docker/setup-buildx-action` | v4.4.1 | `f87e5991a6d7451dcb8d9637bfbc97413f497069` | 2026-09-16 |
| `docker/build-push-action` | v7.4.0 | `c3c9e263c25d99ce0380d002d59b67737d91b0dc` | 2026-09-15 |
| `docker/login-action` | v4.6.0 | `dbcb813823bdd20940b903addbd779551569679f` | 2026-07-29 (la v4.7.0 es del 2026-10-23 y no cumple la regla) |
| `actions/delete-package-versions` | v5.0.0 | `e5bc658cc4c965c472efe991f8beea3981499c55` | 2024-01-16. **`node20`**: ver Riesgos |

**Imágenes:**

| Pieza | Referencia | Subida | Notas |
|---|---|---|---|
| Builder de Rust | `rust:1.99.1-slim-bookworm@sha256:86234e857a413004348b471d11a4ebe1aa92d2eae2b6b143cac68fd1ac3c48be` | 2026-10-22 10:45 UTC | Índice con `amd64` y `arm64`. Cumple 3 días desde el **2026-10-25 10:45 UTC**; C1-1 no empieza antes |
| Builder de web | `node:22.23.4-bookworm-slim@sha256:de170b39439eb0e5a3444ad7d60b015614a381e10fecae2e30ec9e735122f38d` | 2026-10-21 17:38 UTC | = `.node-version` |
| Final de server | `gcr.io/distroless/static-debian13@sha256:516757106de35205efb70b91ba03f6f1298424a0678c603255cf2852a7000087` | 2026-10-20 06:25 UTC | Antes etiquetado `nonroot` (hoy `nonroot` apunta a `18f4cc26…`, del 2026-10-23). En el Dockerfile: `static-debian13:nonroot@sha256:…` |
| Final de web | `gcr.io/distroless/nodejs22-debian13@sha256:b0b1352373f14fe86376d423e6d283ea6b85b912ff55ca89bed8d2fe11c257b8` | 2026-10-21 16:35 UTC | `nonroot-e7bfa5b…`: Node v22.23.4. Cumple desde el 2026-10-24 16:36 UTC |
| Respaldo si se rechaza Debian 13 | `static-debian12:nonroot@sha256:afa5c872c891853ca7fcf1f12c3edb23f7eeef36189728842dd51042ff57f7ab` · `nodejs22-debian12:nonroot@sha256:13593b7570658e8477de39e2f4a1dd25db2f836d68a0ba771251572d23bb4f8e` | 2026-08-21 · 2026-02-14 | Deprecadas; la versión de Node de la segunda no está verificada (≥ 22.17 probable, se comprobaría con `docker run … --version`) |
| Postgres (dev y local) | `postgres:17.10-alpine@sha256:742f40ea20b9ff2ff31db5458d127452988a2164df9e17441e191f3b72252193` | 2026-07-08 | El mismo de `compose.dev.yml`. `17-alpine` (17.11, `eee6c6e1…`) cumple desde el 2026-10-25 13:08 UTC; queda para Dependabot, que mueve los dos archivos a la vez |

**Herramientas:**

| Pieza | Versión | Publicada | Verificación fijada |
|---|---|---|---|
| zig | 0.15.2 | 2025-10-11 | `x86_64-linux` `02aa270f183da276e5b5920b1dac44a63f1a49e55050ebde3aecc9eb82f93239` · `aarch64-linux` `958ed7d1e00d0ea76590d27666efbf7a932281b3d7ba0c6b01b0ff26498f667f` |
| `cargo-chef` | 0.1.78 | 2026-08-12 | `cargo install --locked --version 0.1.78` |
| `cargo-zigbuild` | 0.23.4 | 2026-09-02 | `cargo install --locked --version 0.23.4` (MSRV 1.88) |
| `cargo-deny` | 0.20.2 | 2026-07-09 | `cargo-deny-0.20.2-x86_64-unknown-linux-musl.tar.gz` `9f12ed4c49936e09b48bf862b595cde2fe64fcbd9d74dfacac6131ca824c8d5f` |
| `just` (CI) | 1.58.0 | 2026-08-03 | `just-1.58.0-x86_64-unknown-linux-musl.tar.gz` `4a5cc2f53e6f0f8c59092a6cc38291eb729d46a7dd95d3ae582008881b84931d`. Es la versión local del owner; la 1.59.0 (2026-10-15) no aporta nada aquí |
| `yaml` (npm, raíz) | 2.9.1 | 2026-09-11 | Sin dependencias; `pnpm add -Dw yaml@2.9.1` |
| Playwright | 1.64.1 | (fase A) | `playwright install --with-deps chromium webkit`; caché por versión |
| Runner | `ubuntu-24.04` | — | Trae Docker con buildx y compose, `readelf` y rustup |

Los checksums de zig, `just` y `cargo-deny` los publica el mismo origen que el binario. Fijarlos en el repo protege contra un cambio **posterior** del artefacto, no contra uno anterior al 2026-10-24. Es el mismo modelo que el `zig` de RF-C1, y el paso C1-1 los recalcula descargando cada archivo una vez.

## Entorno local: Colima en Mac ARM

Estado actual (leído el 2026-10-24 con `colima list` y `~/.colima/default/colima.yaml`): Colima 0.10.3, `vmType: vz`, 2 CPU, 2 GiB, **`rosetta: false`** y `binfmt: true`. La emulación amd64 de hoy es **QEMU** (`/proc/sys/fs/binfmt_misc/qemu-x86_64`, `interpreter /usr/bin/qemu-x86_64`). `docker buildx ls` muestra `linux/amd64` en el builder `colima` (BuildKit v0.30.0); el Docker es 29.5.2 y Compose, v5.6.0. Rosetta está instalado en el host (`oahd` corre); la Mac tiene 10 CPU y 32 GB.

- **El build no necesita emulación.** Las etapas de compilación usan `$BUILDPLATFORM` (`linux/arm64`) y las finales solo copian archivos. QEMU o Rosetta solo entran al **ejecutar** los contenedores amd64 de `prod-local`.
- **Recomiendo Rosetta** (equivalente a "Use Rosetta" de Docker Desktop que nombra RF-C7): `colima stop && colima start --vz-rosetta`, o `rosetta: true` en `colima.yaml`. Con `vz` ya activo no hace falta nada más; `binfmt` deja de usarse cuando Rosetta está activo. QEMU también funciona, pero Node bajo QEMU tarda varios segundos más en arrancar y el healthcheck de `web` (cada 15 s, con `start_period` de 30 s) queda justo. Reiniciar Colima baja el Postgres de `just dev`; se vuelve a levantar con `just db-up`.
- **Memoria.** Con 2 GiB, el `cook` y el `build` en `release` (thin LTO y `codegen-units = 1`) con 2 trabajos de rustc en paralelo pueden quedar al borde. Si el build muere con `signal: 9` o `exit code 137`, se sube Colima a 4 GiB **para la sesión de build** (`colima start --vz-rosetta --memory 4`) y se anota. `prod-local` en reposo cabe en 2 GiB (límites de 256 + 384 + 512 MB y consumo real mucho menor).
- **Docker Desktop en otra sesión de macOS** ocupa 8443 y 6550, puertos que `prod-local` no usa (3000 y 8080 en `127.0.0.1`). Su socket (`~/.docker/run/docker.sock`) no existe en esta sesión. Las recetas imprimen `docker context show` (hoy `colima`) para que no haya dudas de contra qué daemon corren.
- **Logs.** En Colima, el `LogPath` del contenedor vive dentro de la VM: se lee con `colima ssh -- sudo stat -c %s "$(docker inspect --format '{{.LogPath}}' torneo-local-server-1)"`.
- **La memoria medida bajo emulación no es representativa** (Rosetta y QEMU agregan su propia RSS). La evidencia del RNF de memoria que cuenta es la del job `images` en amd64 nativo (decisión 16). La de la Mac se pega como referencia.

## Archivos que cambian

**PR C1**

| Archivo | Cambio |
|---|---|
| `apps/server/Dockerfile` | nuevo (decisiones 1, 3–6) |
| `apps/web/Dockerfile` | nuevo (decisiones 2, 7) |
| `.dockerignore` | nuevo: `.git`, `**/target`, `**/node_modules`, `**/.env`, `**/.env.*`, `!**/.env.example`, `private/`, `content/_local/`, `**/.svelte-kit`, `apps/web/build`, `apps/web/test-results`, `apps/web/src/lib/paraglide`, `.claude/`, `docs/` |
| `infra/docker-compose.yml` | nuevo: `web` y `server` (RF-C4, etiquetas de arriba) |
| `infra/compose.local.yml` | nuevo: `postgres`, más `ports` en `127.0.0.1` (RF-C7) |
| `infra/local.env` | nuevo (decisión 13) |
| `infra/docker/.gitkeep` | se borra (el directorio queda vacío; los Dockerfiles viven en cada app, como dice la spec) |
| `scripts/check-compose.mjs` | nuevo (decisiones 9 y 10) |
| `scripts/__tests__/check-compose.test.mjs`, `scripts/__fixtures__/check-compose/**` | nuevos: un fixture válido (copia mínima de los cuatro archivos) y uno por regla |
| `scripts/check-dockerignore.sh`, `scripts/fixtures/context.Dockerfile` | nuevos (decisión 11; tres sujetos, `set -C`, `rm -rf "${tmp:?}"`) |
| `scripts/local-networks.sh` | nuevo (decisión 12) |
| `justfile` | `check-web` agrega `node scripts/check-compose.mjs` al final; recetas nuevas `prod-local`, `prod-local-down`, `_prod-local-up` y `check-dockerignore` |
| `package.json`, `pnpm-lock.yaml` | `yaml` 2.9.1 en `devDependencies` de la raíz |
| `CLAUDE.md` | "Comandos": `prod-local`, `prod-local-down` y la nota de Colima con Rosetta |
| `docs/legal/aviso-de-privacidad.md` | solo si la medición de 1 h da más de 21 días (no se espera: B calculó 18.8) |

**PR C2**

| Archivo | Cambio |
|---|---|
| `.github/workflows/ci.yml` | nuevo (RF-C8 a RF-C12; filtro y comprobaciones de `images` de la decisión 16; permisos de la decisión 23) |
| `.github/workflows/release.yml` | nuevo (RF-C13 a RF-C15) |
| `.github/workflows/.gitkeep` | se borra |
| `.github/actions/setup-just/action.yml`, `.github/actions/setup-js/action.yml` | nuevos (decisión 14) |
| `.github/dependabot.yml` | nuevo (decisión 22) |
| `scripts/check-workflows.mjs` | nuevo (RF-C12, más `runs-on: ubuntu-*`, la lista cerrada de permisos por job y la prohibición de `continue-on-error` de la decisión 23) |
| `scripts/__tests__/check-workflows.test.mjs`, `scripts/__fixtures__/check-workflows/**` | nuevos: uno válido y uno por regla, con las cuatro sintaxis de `secrets` y los de permisos y `continue-on-error` de la decisión 23 |
| `scripts/vitest.config.mjs` | `exclude` de `check-workflows.test.mjs` |
| `scripts/vitest.policy.config.mjs` | nuevo: solo `check-workflows.test.mjs` |
| `scripts/check-hooks.sh` | nuevo (decisión 18) |
| `scripts/git-hooks/pre-push` | nuevo, ejecutable (decisión 24) |
| `justfile` | receta `hooks` |
| `docs/architecture.md` | "Despliegue" reescrita (RF-C18; nota del architect): Postgres standalone y **fuera** del compose, compose con solo `web` y `server`, bases finales `static-debian13:nonroot` y `nodejs22-debian13:nonroot` nombradas explícitamente, redes, Traefik y `torneo-web-headers`, release con gate, `DEPLOY_MODE`, preparación del VPS con los comandos de ADR-0006 y las dos advertencias, y prerrequisitos del primer despliegue. Más una línea sobre el servidor hyper http1 propio (desviación 8 de `plan-b.md`) |
| `CLAUDE.md` | "Comandos": `hooks`, el `pre-push` (árbol de trabajo, Docker, `--no-verify`) y que la CI llega con C; se quita "Llegan en la fase C" |
| `docs/features/001-fundaciones/plan-c.md` | "Desviaciones" y evidencia |

## Orden de trabajo

### PR C1

0. **Bloqueo.** Enmienda de la spec y nota de ADR-0006 D5 para Debian 13 (las escribió el architect), aprobadas por **security-reviewer** (ya en su revisión) **y el owner** en el chat. El architect no las aprueba, porque es su autor. Mientras tanto se pueden hacer los pasos 2, 5 y 6, que no dependen de la base. → `grep -n debian13 docs/features/001-fundaciones/spec.md`.
1. **Versiones.** Repetir las consultas de "Versiones fijadas" (los mismos comandos `gh api` y `timeout curl`). Descargar una vez zig (los dos tarballs), `just` y `cargo-deny` a un `mktemp -d` y recalcular `sha256sum`. → la tabla coincide, o las diferencias quedan en "Desviaciones".
2. **`.dockerignore` y `check-dockerignore.sh`** (paralelizable con 5 y 6). → `bash scripts/check-dockerignore.sh` en verde con los `apps/server/.env` y `apps/web/.env` reales presentes: después, `shasum` de los dos es el mismo que antes e `infra/.env` no existe. Una corrida que falla a propósito imprime solo la ruta (se revisa que la salida no contenga el valor del canario). Prueba negativa: quitar `**/` de `**/.env` en una copia temporal hace fallar el script (se revierte).
3. **Dockerfile del server.** → `docker buildx build --platform linux/amd64 -f apps/server/Dockerfile --build-arg APP_VERSION=sha-0000000 -t ghcr.io/kike1303/torneo-absurdo/server:local --load .` termina. Después, `docker create` + `docker cp …:/server` a un temporal, y `readelf -lW server | grep INTERP` vacío; `file server` dice `x86-64` y `static`. Hay que anotar el tiempo en frío y en caliente, y la memoria máxima de Colima si hubo que subirla. Si `ring` o `mimalloc` no compilan con zig: Riesgos, "build cruzado".
4. **Dockerfile de web.** → `docker buildx build --platform linux/amd64 -f apps/web/Dockerfile -t ghcr.io/kike1303/torneo-absurdo/web:local --load .` termina. `docker export $(docker create ghcr.io/kike1303/torneo-absurdo/web:local) | tar -t | grep -c '^app/node_modules/'` da 0. Smoke test: `docker run --rm -d --read-only --cap-drop ALL --security-opt no-new-privileges -p 127.0.0.1:3000:3000 -e PUBLIC_ORIGIN=http://localhost:3000 -e HOST=0.0.0.0 -e PORT=3000 ghcr.io/kike1303/torneo-absurdo/web:local`, y `curl` a `/healthz` y a `/en/` da 200. `docker run --rm ghcr.io/kike1303/torneo-absurdo/web:local --version` da `v$(cat .node-version)` y `docker inspect -f '{{.Config.User}}'` de las dos imágenes da `65532` o `nonroot` (lo mismo que comprueba `images`, decisión 16). Sin `PUBLIC_ORIGIN`, `timeout 60 docker run --rm ghcr.io/kike1303/torneo-absurdo/web:local` sale con un código distinto de 0 y de 124, y `curl` al puerto nunca conecta.
5. **Compose de producción, local y `infra/local.env`** (paralelizable con 2 y 6). → `env -u IMAGE_TAG docker compose --env-file infra/local.env -f infra/docker-compose.yml config` sale con error y el mensaje contiene `IMAGE_TAG`. `IMAGE_TAG=x docker compose --env-file infra/local.env -f infra/docker-compose.yml -f infra/compose.local.yml config` sale con 0.
6. **`check-compose.mjs`, fixtures y tests** (paralelizable con 2 y 5). → `pnpm exec vitest run --config scripts/vitest.config.mjs` en verde, con un fixture por regla. `node scripts/check-compose.mjs` en verde sobre el repo. Cambiar `stsSeconds` o el valor de `SECURITY_HEADERS` en una copia lo hace fallar.
7. **`local-networks.sh` y recetas `prod-local`.** → `just prod-local`, después `docker compose -p torneo-local ps` (tres `healthy`), y `curl -s 127.0.0.1:8080/api/health` (200, `"version":"dev"`, o el valor de `APP_VERSION` del build local) y `curl -s -o /dev/null -w '%{http_code}' 127.0.0.1:3000/healthz` (200). Los `curl -I` de `/es/` muestran las tres cabeceras una sola vez y sin `includeSubDomains`; los de `/_app/version.json` y `/healthz/` salen sin ellas (línea base de RF-C7). `docker stats --no-stream`. Después, `just prod-local-down` deja sin redes `torneo.local=1`, y una red `torneo-edge` creada a mano **sin** la etiqueta sobrevive a `prod-local-down`. La subred ocupada se prueba creando antes una red `10.250.0.0/24` sin etiqueta: el script falla con el mensaje.
8. **Plazo de logs (confirmación de C).** `just prod-local` en reposo durante 1 h; tamaño del `LogPath` de `server` al inicio y al final (sección Colima). Se extrapola a un día y se calcula `31,457,280 / bytes_día`. → si pasa de 21 días, se ajusta el aviso en este PR; si no, el número va en "Desviaciones" y en el PR.
9. **`just check` completo** y `CLAUDE.md`. → `just check` en verde, con la salida en el PR.

### PR C2

10. **`check-workflows.mjs` con sus fixtures y tests, y la separación de configs de Vitest.** → `pnpm exec vitest run --config scripts/vitest.policy.config.mjs` en verde; `pnpm exec vitest run --config scripts/vitest.config.mjs` ya no los corre. Lo comprueba la cuenta de archivos en la salida.
11. **Acciones compuestas, `ci.yml` y `release.yml`.** → `node scripts/check-workflows.mjs` en verde sobre el repo. `actionlint` no es dependencia: si el owner lo tiene, se corre una vez en local y se pega la salida (opcional).
12. **Hook y `check-hooks.sh`.** → `bash scripts/check-hooks.sh` en verde; `git ls-files -s scripts/git-hooks/pre-push` muestra `100755`.
13. **`dependabot.yml`.** → `node -e` con `yaml` lo parsea. La validación real la hace GitHub al fusionar: la pestaña "Dependabot" no muestra errores de configuración (paso de cierre 5).
14. **Docs** (`architecture.md` y `CLAUDE.md`). → revisión. `grep -n postgres docs/architecture.md` ya no lo pone dentro del compose.
15. **Abrir el PR C2** y medir las corridas: `changes`, `policy`, `rust`, `web`, `e2e`, `images` y `ci-ok`, con su duración. Las dos corridas de `rust` e `images` (en frío y, tras un push sin cambios de dependencias, en caliente) se pegan en el PR. → todo en verde.
16. **PRs de prueba** (criterio C1 y verificaciones 6 y 7 de ADR-0006), en borrador y desde ramas del repo; se cierran sin fusionar:
    - (a) solo `docs/**`: `ci-ok` verde en ≤ 2 min, sin `rust`, `web`, `e2e` ni `images`;
    - (b) un comentario en `crates/protocol/src/lib.rs`: corren `rust` y `web`;
    - (c) un test roto a propósito: `ci-ok` rojo;
    - (d) un cambio a mano en `packages/protocol-ts/src/generated/*.ts`: `rust` falla por frescura;
    - (e) un cambio a `deny.toml`: corre `rust`.

    Los enlaces de las corridas van en el PR C2.
17. **`just check` completo.** → verde, con la salida en el PR.

## Qué se verifica dónde

| Dónde | Qué |
|---|---|
| **Local (Mac ARM, Colima)** | Build cruzado de las dos imágenes (RF-C1, RF-C2), `readelf`, `node_modules` ausente, smoke test de `web`, `check-dockerignore.sh` (RF-C3), `compose config` (RF-C4), `check-compose.mjs` y sus tests (RF-C6), `prod-local` con Rosetta: `healthy`, `curl`, `curl -I` y `docker stats` como referencia (RF-C7), plazo de logs en 1 h, `check-workflows.mjs` y sus tests, `check-hooks.sh` y `just check` |
| **Solo en CI** | Filtros y `ci-ok` con PRs reales (RF-C8), duración de `rust` e `images` con caché caliente (RNF), Postgres por `just db-up` en el runner (RF-C9), e2e en Linux (RF-C10), `cache-from` y `cache-to` de `gha` (RF-C11), `prod-local` en Linux amd64 con `docker stats` nativo (decisión 16), `policy` en un PR de docs (RF-C12) |
| **Solo en GitHub (cierre)** | Rulesets, environment `production`, aprobación para forks, alertas de Dependabot, el tag `v0.0.1` con push, `imagetools --raw`, retención y gate de `deploy` (RF-C13, RF-C14), visibilidad de GHCR, `workflow_dispatch` desde otra rama, validación de `dependabot.yml` |
| **Solo en el primer despliegue (TA-8)** | `verify` (RF-C15), `curl -I` detrás de Traefik (RF-C4 y RF-C18), `docker inspect` de `ReadonlyRootfs`, `CapDrop`, `SecurityOpt`, `LogConfig` y etiquetas, y el resto de las verificaciones 1, 2, 4, 5 y 11 a 16 de ADR-0006 |

## Checklist de cierre RF-C19 (variante GitHub "sí", VPS "aparte")

Lo ejecuta devops-engineer con `gh` y el token del owner, **después del merge de C2 y con el OK del owner en el chat**. Ninguna salida que incluya el token se pega en ningún lado. La evidencia (salidas de `gh api` en JSON) va en TA-1. El orden importa: la configuración va **antes** del tag ("Áreas de preocupación" 8).

0. **Precondición.** `ci.yml` corrió en verde en `main` después del merge (calienta las cachés de `main`, y `ci-ok` ya aparece como check para el ruleset). → `gh run list --workflow ci.yml --branch main --limit 1`.
0b. **PRs de prueba de C2-16 y medición en caliente** (desviación 19). Van después del paso 0, con `main` ya caliente, y **antes del paso 4**, para que los filtros y `ci-ok` queden probados antes de que el ruleset haga de `ci-ok` el único check requerido. Son PRs en borrador, desde ramas del repo, que se cierran sin fusionar, y cada corrida se enlaza en TA-1.
    - (a) Solo `docs/**`: corren `changes`, `policy` y `ci-ok`, y `ci-ok` sale verde en ≤ 2 min, con `rust`, `web`, `e2e` e `images` saltados. Así se prueba "job saltado → `ci-ok` verde" (RF-C8, RF-C12).
    - (b) Un comentario en `crates/protocol/src/lib.rs`: corren `rust` y `web`. Es la medida de `rust` **en caliente** frente a los 8 min de D7.
    - (c) Un test roto a propósito: `ci-ok` rojo ("test roto → `ci-ok` rojo").
    - (d) Un cambio a mano en `packages/protocol-ts/src/generated/*.ts`: `rust` falla por frescura (RF-B6).
    - (e) Un cambio en `deny.toml`: corre `rust`.
    - (f) Un cambio en `infra/compose.local.yml` (por ejemplo, un comentario): corre `images`. Es su medida **en caliente**.
1. **Linear.** TA-8 ("Primer despliegue a producción") lleva la checklist de prerrequisitos de RF-C18 y queda bloqueado por la feature de auth (límites anónimos de D6 y D7, con `TRUSTED_PROXIES=10.250.0.2/32`) y por **TA-6** (preparación del VPS, variante "aparte": runbook privado, fuera del repo). Si TA-8 no existe, se crea. En TA-6 queda la constancia del owner de que cubre "durante la feature 001" de ADR-0006 (ya está en "Decisiones del owner" de la spec; se enlaza).
2. **Actions del repo.**
   - `gh api -X PUT repos/kike1303/torneo-absurdo/actions/permissions/fork-pr-contributor-approval -f approval_policy=all_external_contributors`. Hoy vale `first_time_contributors`.
   - Se confirma que `actions/permissions/workflow` sigue en `default_workflow_permissions: read` y con `can_approve_pull_request_reviews: false`, como hoy.
   - **Propuesta adicional:** `sha_pinning_required: true` en `actions/permissions` (hoy `false`), que hace cumplir en GitHub la regla de SHAs de `policy`. Se aplica solo con el OK del owner en el chat.
3. **Environment `production`.**
   - `gh api -X PUT repos/kike1303/torneo-absurdo/environments/production` con:
     - `reviewers: [{type: User, id: 17516911}]` (`kike1303`);
     - `prevent_self_review: false`, porque el owner es quien empuja el tag y quien aprueba; con `true` no podría aprobar;
     - `can_admins_bypass: false`, para que ni un admin pueda saltarse la aprobación (condición 2 de security-reviewer);
     - `deployment_branch_policy: {protected_branches: false, custom_branch_policies: true}`.
   - Después, `POST …/environments/production/deployment-branch-policies` dos veces: `{name: main, type: branch}` y `{name: "v*", type: tag}`.
   - No se crean `vars.DEPLOY_MODE` ni `vars.PUBLIC_HOST`.
   - **Regla: ningún agente aprueba, rechaza ni se salta un deployment de `production`.** Ni por la UI, ni con `gh api …/pending_deployments`, ni con `can_admins_bypass`. Solo el owner, en la UI. El agente se limita a leer el estado de la corrida.
4. **Rulesets.**
   - **`main`:** `target: branch`, `include: ["~DEFAULT_BRANCH"]`, `enforcement: active`. Reglas:
     - `deletion` y `non_fast_forward`;
     - `pull_request` con `required_approving_review_count: 0`;
     - `required_status_checks` con `[{context: "ci-ok", integration_id: 15368}]` (la app de GitHub Actions).
     - Sin `bypass_actors`.
   - **Tags:** `target: tag`, `include: ["refs/tags/v*"]`. Reglas `creation`, `update`, `deletion` y `non_fast_forward`. `bypass_actors: [{actor_type: RepositoryRole, actor_id: 5, bypass_mode: always}]` (admin), así que solo el owner crea, mueve o borra tags `v*`.
5. **Dependabot.**
   - `gh api -X PUT repos/kike1303/torneo-absurdo/vulnerability-alerts` (hoy da 404, desactivadas) y `gh api -X PUT repos/kike1303/torneo-absurdo/automated-security-fixes` (hoy `enabled: false`).
   - Se comprueba que la pestaña de Dependabot no reporta errores de `dependabot.yml`: `gh api repos/…/dependabot/…` no expone ese estado, así que lo mira el owner en la UI, o se espera el primer PR semanal.
   - Secret scanning y push protection ya están activos.
6. **Evidencia de 2 a 5.** `gh api` GET de `environments/production` (donde se ve `can_admins_bypass: false`, `prevent_self_review: false` y el reviewer), `environments/production/deployment-branch-policies`, `rulesets` (y cada uno por id), `actions/permissions`, `actions/permissions/workflow`, `actions/permissions/fork-pr-contributor-approval`, `vulnerability-alerts` (204) y `automated-security-fixes`, pegados en TA-1.
7. **Tag de prueba.** El owner crea y empuja `v0.0.1` sobre `main` (`git tag -a v0.0.1 -m "v0.0.1" && git push origin v0.0.1`), o autoriza en el chat que lo haga el agente.
   - → `build` en verde, con dos imágenes publicadas, `vX.Y.Z` y `sha-<7>` sobre el mismo digest (`imagetools inspect`), un manifiesto de imagen y no un índice (`--raw`), y la retención en verde (sin borrar nada, porque hay una sola versión). Criterio C5.
   - `deploy` queda en "Waiting" y el **owner la rechaza en la UI** (criterio C6; el agente no la toca, cierre 3). `verify` no corre.
8. **GHCR público (owner, en la UI).** Para cada paquete: Package settings → Change visibility → Public.
   - → el agente comprueba el acceso anónimo: con `DOCKER_CONFIG` vacío, `docker buildx imagetools inspect --raw ghcr.io/kike1303/torneo-absurdo/server:v0.0.1`, y lo mismo para `web`.
   - **Opcional:** `just prod-local v0.0.1`, que prueba el camino de pull de RF-C7 y el respaldo del riesgo de Mac ARM.
9. **`workflow_dispatch` desde otra rama** (primera mitad de la verificación 17).
   - Una rama `ta-1-dispatch-probe` desde `main`, y `gh workflow run release.yml --ref ta-1-dispatch-probe -f tag=v0.0.1`.
   - → `build` pasa (el tag existe y no reconstruye), y `deploy` falla de inmediato con el error de protección del environment ("branch is not allowed to deploy"), sin pedir aprobación.
   - Se borra la rama.
   - Además, desde `main`: `gh workflow run release.yml --ref main -f 'tag=v1$(id)'` → `build` falla en la validación sin construir ni publicar nada, y `deploy` no llega a pedir aprobación (RF-C13).
10. **Cierre.** La evidencia de 0 a 9 queda en TA-1; `plan-c.md` pasa a `done` con sus desviaciones; TA-1 pasa a Done.

## Riesgos

- **Build cruzado** (`ring` o `mimalloc` con `zig cc` para musl; "Áreas de preocupación" 6). Es el paso más riesgoso, y por eso va primero (C1-3) y antes de escribir la CI. → Si falla con zig 0.15.2, se prueba `cargo-zigbuild` 0.24.0 con zig 0.16.0 (cuando la 0.24.0 cumpla 3 días), y se anota. Si tampoco, se usa el plan B de D8 en la Mac solo para diagnosticar. En CI (amd64 nativo), el respaldo es `musl-tools` sin zig dentro del Dockerfile, que exige una enmienda a RF-C1. No se pasa a C2 sin un binario que pase `readelf`.
- **Memoria de Colima (2 GiB) en el build en `release`.** → Subir a 4 GiB para la sesión de build (sección Colima). No afecta a la CI (16 GB en el runner).
- **`actions/delete-package-versions` corre en `node20`.** GitHub está retirando Node 20 de los runners. Si la corrida del tag avisa o falla por el runtime: buscar una versión `node24` que cumpla la regla de 3 días o, si no existe, reemplazarla por un paso con `gh api` que conserve las 10 más recientes. Ese cambio se aparta del texto de D5 ("con exactamente estos parámetros") y va a "Desviaciones" con revisión del architect, más una nota en ADR-0006. El respaldo, aceptado por architect (hallazgo 1) y security-reviewer (condición 3), tiene esta forma:
  - **Endpoint:** `/users/${OWNER}/packages/container/torneo-absurdo%2F${IMG}/versions` (con `/users/<owner>`, no `/user`, que es el del usuario autenticado y no aplica al `GITHUB_TOKEN`).
  - **Credencial:** el `GITHUB_TOKEN` del job `build`, con su `packages: write` (decisión 23), vía `GH_TOKEN: ${{ github.token }}`. **Nunca un PAT** ni un secret nuevo: `check-workflows.mjs` ya prohíbe `secrets.*` fuera de la lista.
  - **Datos por `env:`:** `OWNER`, `IMG` y el digest publicado (`steps.push.outputs.digest`) entran como variables de entorno; ninguna expresión `${{ }}` dentro de `run:`.
  - **Lista completa antes de borrar:** `gh api --paginate "…/versions?per_page=100"` y orden por `created_at` descendente con `jq`. `set -euo pipefail`: cualquier error de la API, una respuesta que no sea un arreglo o una **lista vacía** (acabamos de publicar, así que vacía es un error) abortan el paso sin borrar nada.
  - **Con 10 versiones o menos, no se borra nada.** Con más, se borran las que quedan después de la décima.
  - **Nunca se borra el digest recién publicado:** si su `name` aparece entre los candidatos a borrar (por ejemplo, por un `created_at` raro), el paso falla en vez de borrar. Tampoco se borra una versión con una etiqueta `v*` del release en curso.
  - El permiso del `GITHUB_TOKEN` para borrar depende de que el paquete esté vinculado al repo (decisión 20); se comprueba en la corrida del tag de prueba (cierre 7).
- **Debian 13 rechazado.** → Respaldo `-debian12` (tabla de versiones), con el riesgo de una base sin parches. Por condición de security-reviewer, usar `-debian12` **bloquea TA-8** hasta migrar a `-debian13`: se anota como bloqueo en Linear, no solo como nota. La versión de Node de `nodejs22-debian12` la verifica el mismo `docker run … --version` del job `images` (decisión 16), que falla si no es igual a `.node-version`.
- **Dependabot no encuentra `compose.dev.yml` ni `compose.local.yml`** (no verificado: no sé si el ecosistema `docker-compose` reconoce esos nombres). → Lo dice la pestaña de Dependabot tras el merge. Si no los ve, se anota y el digest de Postgres se mueve a mano hasta que Dependabot lo soporte; `check-compose.mjs` obliga a que los dos archivos tengan el mismo digest.
- **Bumps de Dependabot que fallan a propósito:** `rust:` sin `rust-toolchain.toml` y `node:` sin `.node-version` (RF-C16, decisión 10). → Se resuelven subiendo los dos en el mismo PR. Va en `CLAUDE.md`.
- **`ci-ok` como check requerido** no aparece en el selector del ruleset hasta que corre una vez. → Precondición 0 del checklist.
- **`prevent_self_review`.** Con `true`, el owner no podría aprobar sus propios despliegues. → `false` explícito (cierre 3).
- **Traefik sin probar en vivo.** Etiquetas, `torneo-web-headers` y la mayúscula de `accessLogs` ("Áreas de preocupación" 13). → Chequeo estático en `check-compose.mjs`; la prueba real es un prerrequisito de TA-8 (RF-C18), con `curl -I` por `https://`, porque Traefik solo emite HSTS sobre TLS (nit N2 de security).
- **`rust` por encima de 8 min (p50)** por la compilación musl de RF-C9. → Se mide en C2-15. `check-rust` no se recorta, porque es el mismo camino que en local. Si se pasa, la palanca es mover el build musl con `readelf` a un paso que corra solo cuando cambian `Cargo.lock`, `apps/server/**` o `rust-toolchain.toml`. Eso se propone en "Desviaciones" con revisión del architect, porque la verificación 8 de ADR-0006 lo pide en `rust`.
- **Puertos 3000 y 8080 ocupados en la Mac** (`just dev` o la otra sesión). → Comprobación previa en `prod-local` con un mensaje claro (decisión 12).
- **Logs bajo emulación** (Rosetta) con un tamaño de línea distinto. → El tamaño de línea no depende de la arquitectura; el crecimiento por hora lo da el healthcheck de Docker cada 15 s, que es el mismo.

## Alternativas descartadas

- **`plan-c1.md` y `plan-c2.md` separados:** duplicarían el contrato de los scripts y el checklist de cierre, y harían revisar C2 sin ver lo que llama de C1. Se mantienen dos PRs.
- **Traefik en `compose.local.yml`:** exige TLS local y un certresolver falso, y rompe la regla de "solo agrega" (respuesta de devops a la enmienda menor).
- **`cargo install cargo-deny` en CI:** unos 3 min por corrida; el binario con checksum fijado da el mismo resultado.
- **`taiki-e/install-action` o `extractions/setup-just`:** otra action de terceros con acceso al job; la descarga con checksum es corta y la analiza `policy`.
- **`cache: pnpm` de `setup-node`:** necesita pnpm antes que Node, y con corepack eso solo pasa después (decisión 14).
- **zig 0.17 con `cargo-zigbuild` 0.24.0:** la 0.24.0 tiene 3 días justos y zig 0.17 tiene arreglos recientes; prefiero la pareja con un año de uso.
- **Ecosistema `rust-toolchain` de Dependabot:** existe, pero abriría PRs separados de los del Dockerfile y los dos fallarían por separado en `check-compose.mjs`. Hasta que haya grupos entre ecosistemas, el bump de Rust se hace a mano en un solo PR.
- **`ubuntu-latest`:** puede cambiar de versión sin aviso (decisión 15).
- **Medir la memoria solo en la Mac:** la emulación la distorsiona (decisión 16).

## Pruebas (cómo se demuestra que funciona)

| RF | Prueba | Dónde |
|---|---|---|
| C1 | C1-3: build cruzado, `readelf -lW` sin `INTERP` y `file` estático x86-64; job `images` con la misma comprobación; `check-compose.mjs` (`rust:` = toolchain, todo `FROM` con digest); el `zig` con un `BUILDARCH` sin checksum falla (se prueba con `--build-arg BUILDARCH=riscv64` en la etapa `chef`) | local · CI |
| C2 | C1-4: sin `app/node_modules/`, smoke test 200 en `/healthz` y `/en/` con `--read-only`, y salida distinta de 0 sin `PUBLIC_ORIGIN`; en `images`, además, `--version` igual a `.node-version` y `Config.User` `65532`/`nonroot` en las dos imágenes | local · CI |
| C3 | `check-dockerignore.sh` (C1-2, con los `.env` reales intactos y tres sujetos) en local y en `images`; `check-compose.mjs` con las formas `**/` y sin `*.dockerignore` fuera de la raíz (fixtures) | local · CI |
| C4 | `check-compose.mjs` sobre el repo y sus fixtures; `compose config` sin `IMAGE_TAG` falla nombrando `IMAGE_TAG` (C1-5 e `images`) | local · CI |
| C5 | Revisión de `infra/local.env` (comentario, ningún secreto) | PR |
| C6 | Vitest de `check-compose` con un fixture por regla, incluidos `BIND_ADDR`, `forceSTSHeader`, `stsIncludeSubdomains=true`, `stsPreload=true`, falta de `torneo-web-headers` o de uno de sus valores, divergencia con `SECURITY_HEADERS`, `logging` ausente, `tmpfs` sin tamaño y `compose.local.yml` con `postgres` aceptado | local · CI (`web`) |
| C7 | C1-7 en Mac con Rosetta (healthy, `curl`, `curl -I` de `/es/`, `/_app/version.json` y `/healthz/`, `docker stats`, redes con etiqueta, subred ocupada); `images` en Linux amd64 (decisión 16); `just prod-local v0.0.1` opcional en el cierre | local · CI · cierre |
| C8 | Corridas del PR C2 y PRs de prueba (a) a (e) de C2-16 | CI |
| C9 | Job `rust`: `check-rust` con `db-up`, `just deny`, build musl y `readelf`; duración en caliente frente a 8 min | CI |
| C10 | Job `web` (`check-web`, sin los tests de `check-workflows`) y job `e2e` (dos instancias de `playwright.config.ts`, WebKit para tema y CSP, artefacto de capturas) | CI |
| C11 | Job `images`: `push: false`, `provenance` y `sbom` en `false`, `cache-from` por `scope`, `cache-to` `mode=max` solo en `push` a `main` (se ve en el log: el PR no exporta caché) | CI |
| C12 | Vitest de `check-workflows` con un fixture por regla (incluidas las cuatro sintaxis de `secrets`, `runs-on` no ubuntu, `deploy` sin `environment`, y los de permisos y `continue-on-error` de la decisión 23); `policy` en el PR de docs (a) en ≤ 2 min; `check-hooks.sh` | local · CI |
| C13 | Tag `v0.0.1`: publicación, mismo digest para las dos etiquetas, `--raw` como manifiesto de imagen, retención (sin borrar nada con una sola versión); un tag inválido (`workflow_dispatch` con `tag=v1$(id)`) falla en la validación sin construir nada (cierre 9) | cierre |
| C14 | `policy` (environment y sin la palabra prohibida); `deploy` esperando aprobación y rechazado en el cierre 7; un fixture de `check-workflows` con el mensaje de `api` que nombra a la plataforma falla | CI · cierre |
| C15 | Revisión del job `verify`; se ejercita en TA-8 | PR · TA-8 |
| C16 | Revisión de `dependabot.yml`; GitHub lo acepta sin errores tras el merge (cierre 5) | PR · cierre |
| C17 | `check-hooks.sh` en `policy`; `100755` en el índice | CI |
| C18 | Revisión de `docs/architecture.md` y `CLAUDE.md` (security-reviewer revisa los prerrequisitos) | PR |
| C19 | Checklist de arriba, con la evidencia en TA-1 | cierre |
| RNF memoria | `docker stats --no-stream` en `images` (amd64 nativo): `server` < 64 MB y `web` < 200 MB en reposo, **y el job falla si se pasan** (decisión 16); referencia de la Mac en el PR C1, solo informativa | CI · local |
| RNF velocidad | Duraciones de C2-15 y C2-16 (docs ≤ 2 min, Rust ≤ 8 min p50, `images` en caliente ≤ 8 min) y del tag de prueba (≤ 20 min sin la espera) | CI · cierre |
| Aviso (plazo) | C1-8: 1 h de `prod-local` en reposo | local |

- [ ] `just check` en verde al final de C1 y de C2, con la salida en cada PR.

## Estimación de CI

Repo público: los minutos son ilimitados (ADR-0006), así que la estimación es de tiempo de respuesta y de higiene. Tiempos de pared con caché caliente; entre paréntesis, minutos de runner sumando los jobs en paralelo.

| Corrida | Jobs | Tiempo de pared | Minutos de runner |
|---|---|---|---|
| PR solo de docs | `changes` (≈0.3), `policy` (≈1), `ci-ok` | ≈1.5 min | ≈2 |
| PR de Rust | `rust` (6–8), `web` (3–4) y `e2e` (5–7) si toca `crates/protocol`, más los de docs | ≈8 min | ≈18 |
| PR de web | `web`, `e2e`, más los de docs | ≈7 min | ≈11 |
| PR de infra o lock | `images` (6–8 en caliente; 18–22 en frío por `cargo install` y el `cook` con LTO) | ≈8 min | ≈9 |
| Push a `main` | lo mismo que el PR, más la exportación de caché (`mode=max`, +1–2 min en `images`) | ≈9 min | ≈20 |
| Release (tag) | `build` (8–12 con la caché de `main`), `deploy` (<1, sin contar la espera), `verify` (<1 en 001: no corre) | ≈12 min | ≈13 |

Con unos 30 PRs y 30 pushes a `main` al mes, en una mezcla típica, son unos 700–900 min/mes, por debajo de los ~2,000 de referencia aunque el repo pasara a privado. La caché `gha` (cuota de 10 GB) la escriben solo `main` (rust-cache, pnpm, Playwright y las dos imágenes con `scope`), y el release no escribe nada (RF-C13).

## Desviaciones durante la implementación

1. **C1-1, digest del builder de Rust (excepción del owner, 2026-10-25).** El owner autorizó empezar C1 antes de las 10:45 UTC del 2026-10-25, con esta regla: si existe un digest anterior de `rust:1.99.1-slim-bookworm` que ya cumpla los 3 días, se usa ese; si no, el del plan. **Existe, y es el que uso:** `rust:1.99.1-slim-bookworm@sha256:11d39e118fbc6aba0c89511ff6810dcbad03e7c28d78e3f9d59b714f8d4e08dc`.
   - **Fuente:** historial de `repos/rust/remote/1.99.1-slim-bookworm.md` en `docker-library/repo-info`. El commit `b1870c3` (2026-10-21 23:04 UTC) registra ese índice, así que se subió antes de esa hora y cumple los 3 días desde el 2026-10-24 23:04 UTC. La consulta fue el 2026-10-25 a las 02:29 UTC.
   - **Qué cambia frente a `86234e85…`:** el hijo `linux/amd64` es el mismo (`04f6411f…`, creado el 2026-10-21 21:51 UTC). El hijo `linux/arm64/v8` es el anterior (`65215e66…`, del 2026-10-21 01:17 UTC, en vez de `c8d02717…`). También cambia `ppc64le`, que no se usa. Los dos traen `RUST_VERSION=1.99.1`. El índice siguiente (`54e39a2f…`, registrado el 2026-10-22 a las 03:58 UTC) no se puede probar que cumpla los 3 días antes de las 03:58 de hoy, así que lo descarto.
   - **Efecto:** ninguno en CI, porque el `amd64` es idéntico. En la Mac, el `chef` corre sobre el `arm64` anterior. Dependabot moverá el digest cuando abra su primer PR.
   - El resto de la tabla de "Versiones fijadas" coincide al 2026-10-25: los digests de `node`, `postgres` y los dos `-debian13` (por `timeUploadedMs`), los diez SHAs de actions, `cargo-chef` 0.1.78, `cargo-zigbuild` 0.23.4 (ni una ni otra retiradas), `yaml` 2.9.1 (sin dependencias) y los SHA-256 de zig `x86_64` y `aarch64`, `just` 1.58.0 y `cargo-deny` 0.20.2, recalculados con una descarga de cada archivo.
2. **C1-3, `wget` no viene en `rust:1.99.1-slim-bookworm`.** La decisión 1 decía que sí. `chef` instala `wget` junto con `xz-utils` en el mismo `apt-get` (paquetes de Debian firmados). `gcc` y `ca-certificates` sí vienen.
3. **C1-3, `rustup toolchain install --no-self-update`.** Sin la opción, rustup 1.29.1 se autoactualiza a 1.29.2 durante el build, y esa descarga no está fijada. Con la opción solo agrega el target musl y `rustfmt`/`clippy` sobre la toolchain 1.99.1 de la imagen.
4. **C1-3, aviso del linker de zig.** El `build` en `release` emite `warning: linker stderr: ignoring deprecated linker optimization setting '1'` (lint `linker_messages` de rustc). Es benigno: el binario sale estático y pasa `readelf`. No afecta al job `rust`, que compila con `musl-tools` y no con zig (decisión 17). Si molesta, se resuelve al subir zig y `cargo-zigbuild`.
5. **C1-6, los fixtures de `check-compose` son mutaciones del fixture válido.** Hay un solo directorio válido (`scripts/__fixtures__/check-compose/valid/`, copia de los nueve archivos que lee el script), más `dockerignore-elsewhere/`. Los demás casos (70, uno por regla, más uno de `dockerignore-elsewhere`) son ediciones de una línea sobre el válido, declaradas en una tabla de `check-compose.test.mjs`. Cada edición comprueba que el texto que reemplaza existe, así que si el fixture válido cambia, el test falla en vez de esconder una regla. Copiar el compose entero unas 60 veces habría sumado miles de líneas al PR sin mejorar la cobertura.
6. **C1-6, reglas de más en `check-compose.mjs`.** Todas apuntan en la dirección de RF-C1, RF-C4 y RF-C7:
   - la base final de cada Dockerfile tiene que ser una etiqueta `:nonroot`;
   - el compose de producción no lleva `name:` (decisión 12);
   - `compose.local.yml` solo puede tener `services` y `volumes` en la raíz, porque un `networks:` ahí pisaría las redes externas;
   - la regla de `127.0.0.1:` también aplica a `postgres`;
   - `!**/.env.example` tiene que ir después de `**/.env.*`, porque en `.dockerignore` gana la última regla.

   El listado de `*.dockerignore` exime a `scripts/__fixtures__/check-compose/`, que son datos de prueba que BuildKit nunca lee.
7. **C1-6, `RUST_LOG` (corregida en la revisión del PR #4).** La primera versión solo rechazaba las palabras `debug` y `trace`, y dejaba pasar `warn`, `off`, un target sin nivel (`server`, `info,sqlx`), que en `EnvFilter` es TRACE, y niveles numéricos (`info,server=5`). Ahora la regla exige el valor exacto `info` que fija RF-C4, con un fixture para cada una de esas cinco formas. `warn` y `off` también quedan fuera porque quitarían la línea de `/api/health` en la que se apoya el plazo de logs (desviación 11).
8. **C1-6, `pnpm-lock.yaml`.** `pnpm add -Dw yaml@2.9.1` también reescribe los identificadores de peer de `vite`, porque `yaml` es su peer opcional (`vite@8.3.4(...)(yaml@2.9.1)`). No se mueve ninguna versión: el único paquete nuevo es `yaml@2.9.1`.
9. **C1-7, recetas y volumen.**
   - Hay una receta interna nueva, `_prod-local-preflight`, que comparten `prod-local` y `_prod-local-up`. Así la CI también la usa.
   - Los puertos se comprueban con `/dev/tcp` de bash, no con `nc`, porque `nc -G` solo existe en macOS.
   - `prod-local` valida el `tag` (`vX.Y.Z` o `sha-<7>`) antes del `pull`.
   - `compose.local.yml` le da a `postgres` un volumen con nombre (`torneo-local-pg`). Es lo que conserva `prod-local-down` sin `-v`.
10. **Medidas de C1-3 y C1-4 (Mac ARM, Colima con 2 CPU, 2 GiB y Rosetta).**
    - **Build del server:** en frío, unos 4 min (112 s de `chef`, 97 s del `cook` y 27 s del `build`); en caliente, 26 s, cambiando `APP_VERSION` y con el `cook` en caché. Con 2 GiB alcanzó: no hubo que subir Colima a 4 GiB.
    - **Build de `web`:** 42 s en frío.
    - **Binario:** 5,266,192 B, `ELF 64-bit LSB executable, x86-64, statically linked, stripped`. `readelf -lW` no muestra `INTERP` y `readelf -dW` no muestra `NEEDED`.
    - **Imágenes (`docker image ls` / comprimida):** `server` 14.5 MB / 3.4 MB; `web` 218 MB / 55.9 MB.
    - **Memoria de `prod-local`** (unos 25 s después de `healthy`, bajo Rosetta, solo como referencia): `server` 12.6 MiB, `web` 108 MiB y `postgres` 58 MiB. Tras 1 h en reposo: `server` 12.6 MiB, `web` 61.5 MiB y `postgres` 19.4 MiB.
11. **C1-8, plazo de logs: 18.8 días, así que el aviso no cambia.**
    - **Medición:** `prod-local` en reposo durante 3,606 s, del 2026-10-25 02:50 a las 03:50 UTC. El `LogPath` de `torneo-local-server-1` pasó de 465 B a 70,330 B (+69,865 B, 243 líneas, ~289 B por línea con el envoltorio de `json-file`).
    - **Cálculo:** 1,673,970 B/día, y 31,457,280 / 1,673,970 = **18.79 días**.
    - **Comparación:** coincide con los 18.8 de B y queda por debajo de 21, así que "menos de 3 semanas" sigue siendo cierto y `docs/legal/aviso-de-privacidad.md` no cambia.
    - **Condiciones:** durante la hora corrieron `just check` y `just e2e`, que no le hacen peticiones a `prod-local`. Las únicas líneas son las del healthcheck de Docker cada 15 s.
12. **Revisión del PR #4 (architect, security-reviewer y qa-engineer): `check-compose.mjs` se endurece.** Para cada hallazgo, primero un fixture que fallaba y después el arreglo.
    - **Anclas, alias y `<<`.** `yaml` (esquema core de YAML 1.2) deja `<<` como clave literal, pero Compose sí lo resuelve, así que `<<: *ancla` podía meter `ports`, `privileged` o `docker.sock` sin que el script lo viera. Ahora el parser recorre el documento y rechaza cualquier ancla, alias o clave `<<` en los tres archivos de compose.
    - **Listas cerradas de claves.**
      - Raíz de producción: solo `services` y `networks`, así que `include` y las extensiones `x-*` fallan.
      - Raíz de `compose.local.yml`: siguen siendo solo `services` y `volumes`, y `volumes` solo puede tener `torneo-local-pg`, sin opciones.
      - `server` y `web`: exactamente las claves de RF-C4. Faltar una o sobrar otra (`privileged`, `cap_add`, `user`, `env_file`, `extends`, `volumes`, `network_mode`…) falla.
      - `healthcheck`: exactamente `test`, `interval`, `timeout`, `retries` y `start_period`, así que `disable: true` falla.
      - `logging`: exactamente `driver` y `options`.
      - `depends_on`: exactamente `server: { condition: service_healthy }`.
      - Redes externas: exactamente `{ external: true }`, sin `name:`. En `torneo-edge`, `server` solo lleva el alias, sin `ipv4_address`; `torneo-db` no lleva opciones.
      - `postgres` de `compose.local.yml`: claves cerradas, el `command` de D4, las variables `POSTGRES_USER`, `POSTGRES_PASSWORD` y `POSTGRES_DB`, el único volumen `torneo-local-pg:/var/lib/postgresql/data` y un healthcheck sin `disable`.
    - **`.dockerignore`.** Rechaza cualquier línea `!` distinta de `!**/.env.example`. Es más amplio que lo pedido (reinclusiones de `.env*`), porque reincluir algo de `private/` o de `.git` es igual de grave, y `check-dockerignore.sh` solo prueba archivos llamados `.env`.
    - **Fixture válido.** Un test exige que `valid/` sea idéntico, byte a byte, a los nueve archivos reales. Así un bump de Dependabot obliga a resincronizarlo en el mismo PR.
    - **Re-revisión de security-reviewer (etiquetas YAML y mayúsculas en labels).**
      - **Etiquetas explícitas.** Compose respeta `!reset` y `!override` también en el archivo base, y borra o reemplaza la clave, mientras que `yaml` solo emite un *warning* y conserva el valor. Así, `cap_drop: !reset [ALL]` pasaba el chequeo y el contenedor arrancaba con las capabilities por defecto. Ahora cualquier nodo con etiqueta explícita (`!reset`, `!override`, `!!str`, …) falla, y también falla cualquier `warning` del documento, además de los `errors`. Hay fixtures para `!reset` en `cap_drop`, en `labels` y en el volumen de Postgres, `!override` en `compose.local.yml` y `!!str`.
      - **Labels con mayúsculas.** Antes las claves se pasaban a minúsculas, así que una pisaba a la otra en el chequeo, pero Docker recibía las dos: por ejemplo, `...headers.STSPRELOAD=true` junto a `...stsPreload=false`, o `routers.TORNEO-WEB.MIDDLEWARES=` vacío. Ahora dos claves que solo difieren en mayúsculas fallan, y cada clave tiene que estar escrita exactamente como en RF-C4.
      - **Esto cambia la decisión 9,** que comparaba las claves sin distinguir mayúsculas: el test que lo aceptaba ahora exige la grafía exacta.
    - **Nits.** El comentario de `apps/server/Dockerfile` ahora nombra `wget`. `prod-local` corre la comprobación previa una sola vez, antes del build, y después llama a la receta interna nueva `_prod-local-start` (redes y `compose up`). `_prod-local-up`, la que usará la CI, sigue haciendo la comprobación previa y luego `_prod-local-start`.
13. **C2-10, `check-workflows.mjs` va más allá de RF-C12.** Todo apunta en la misma dirección que la decisión 23:
    - **Ningún `${{ }}` dentro de `run:`**, con cualquier contexto, no solo con los datos controlables (`inputs.*`, `github.event.*`, `github.head_ref`, `github.ref_name`). Así la regla no depende de mantener una lista: todo valor llega a `run:` por `env:`, también en las acciones compuestas.
    - **`secrets`** se busca en los valores ya parseados (dentro de `${{ }}`, en todo `if:`, que es una expresión aunque no lleve `${{ }}`, y en cualquier clave `secrets:`), nunca en los comentarios. Los contextos no distinguen mayúsculas, así que `SECRETS.X` también falla.
    - **Reglas de más:**
      - el conjunto de jobs de `ci.yml` y `release.yml` es exactamente el de la tabla, así que también falla si falta uno;
      - `ci-ok` necesita todos los demás jobs y corre con `if: always()`;
      - `policy` no tiene `if:`;
      - `continue-on-error` también está prohibido en los pasos de las acciones compuestas;
      - las acciones locales tienen que ser `composite`;
      - mismo endurecimiento del YAML que `check-compose` (ni anclas, ni alias, ni `<<`, ni etiquetas explícitas, ni *warnings*).
    - Las reglas de texto crudo (la plataforma de despliegue, `/api/v1/deploy` y la fusión automática) sí leen los comentarios, así que los workflows nombran a la plataforma solo de forma genérica.
    - **Fixtures:** hay un solo conjunto válido mínimo (`scripts/__fixtures__/check-workflows/valid/`), independiente de los workflows reales para que un bump de Dependabot no obligue a resincronizarlo, y 54 mutaciones de una o dos líneas, una por regla y con las cuatro sintaxis de `secrets`, como en la desviación 5. Un test exige además que el repo real pase.
14. **C2-11, filtros de `changes` más amplios que D7 y RF-C8.**
    - **`web`** agrega `.editorconfig`, `infra/**`, `.dockerignore`, `**/Dockerfile`, `rust-toolchain.toml` y `.github/dependabot.yml`. Son las entradas de `check-web`: Prettier formatea el YAML de `infra/` y de `.github/`, y `check-compose.mjs` lee `infra/`, `.dockerignore`, los Dockerfiles y `rust-toolchain.toml`. Sin ellas, un PR que solo tocara esos archivos dejaría `check-web` en rojo en `main` con `ci-ok` en verde.
    - **`rust`** agrega:
      - `clippy.toml`, que lo lee clippy;
      - `infra/compose.dev.yml`, el Postgres de `db-up` (un bump de su digest corre los tests de Rust);
      - `scripts/gen-protocol-ts.mjs` y `scripts/compare-trees.mjs`, que `_check-gen` ejecuta (hallazgo de qa-engineer en el PR #5). Sin ellos, un cambio a esos scripts saltaba `rust`, y el PR siguiente que tocara Rust fallaba por frescura sin haber cambiado nada de eso.
    - **Test que lo cubre** (en `policy`). Para cada job con filtro, recorre las recetas de `just` y los `run:` que el job ejecuta, junta toda ruta `scripts/...` y exige que entre en el filtro de ese job. Fallaba con esos dos scripts antes del arreglo.
15. **C2-11, detalles de las acciones y de los jobs.**
    - **`setup-js`.** La llave de la caché incluye el alcance del install (`all`, o el hash del `filter`). Sin eso, `policy` (que solo instala la raíz) podría guardar en `main` un store parcial bajo la misma llave que el completo, y los demás jobs tendrían un `cache-hit` que no saltaría nunca el `save`. También fija `package-manager-cache: false`: `setup-node` v7 solo la activa para npm, pero así queda explícito.
    - **`e2e`.** Con la caché de navegadores restaurada corre `playwright install-deps` (los paquetes del sistema no se guardan en la caché); sin ella, `install --with-deps`. El artefacto sube `test-results/screenshots` y `test-results/logs` con `if: always()`, por 14 días.
    - **`images`.** Además de la decisión 16, corre el smoke test de RF-C2 con la imagen `web` sola: `--read-only`, `127.0.0.1:3001`, 200 en `/healthz` y `/en/`, y salida distinta de 0 y de 124 sin `PUBLIC_ORIGIN`. Si un paso falla, vuelca las últimas 100 líneas de `docker logs` de los tres contenedores. Los logs del server ya pasan por la allowlist de campos de B, y el `postgres` es el local, sin datos reales.
    - **`release.yml/build`.**
      - El chequeo de que ninguna de las dos etiquetas existe distingue "no existe" (`not found`, `manifest unknown` o `name unknown`, la respuesta de GHCR cuando el paquete todavía no existe) de cualquier otro error, que también hace fallar el job.
      - La igualdad de digests compara el SHA-256 del manifiesto crudo de las dos etiquetas.
      - El checkout corre solo en un tag, no en el rollback.
      - El login usa `github.token`, no `secrets.GITHUB_TOKEN`.
    - **`images`, causa del fallo sin `PUBLIC_ORIGIN`** (nit de qa-engineer en el PR #5). Además del código de salida, la salida tiene que traer el `env_invalid` de Kit y el mensaje del validador (`PUBLIC_ORIGIN is required at runtime`). Así no pasa un crash por otro motivo.
    - **`setup-js`** corre `pnpm store prune` antes del `save` en `main` (nit del architect), para que el store que se restaura con `restore-keys` no crezca sin límite.
    - **`release.yml/verify`** limita la espera a 300 s de reloj (`SECONDS`) y `curl --max-time 5` (nit de qa-engineer). En el peor caso termina en unos 315 s, con su propio mensaje y no por `timeout-minutes: 10`.
    - **`check-hooks.sh`** además exige `#!/bin/sh` como primera línea y pasa `sh -n` sobre el hook. Probado en negativo, en un clon temporal: con `100644` y con `exec just test`, falla con el mensaje de cada caso.
16. **C2-11, `actionlint` no está instalado,** y por la regla del encargo no lo instalé. En su lugar:
    - los 34 bloques `run:` de los dos workflows y las dos acciones pasan `bash -n`;
    - el parser de `yaml` lee los cuatro archivos sin errores;
    - `check-workflows.mjs` pasa sobre el repo;
    - el paso de memoria se probó con salidas de ejemplo de `docker stats` (`190.7MiB` pasa y `191MiB` falla contra 200,000,000 B).
    - El job `policy` se simuló en un clon superficial (`git clone --depth 1`): `pnpm install --frozen-lockfile --filter .` instala solo la raíz (59 paquetes), y `check-workflows.mjs`, sus 59 tests y `check-hooks.sh` pasan.

    Lo que solo valida GitHub (expresiones, contextos e inputs de cada action) se ve en la primera corrida del PR.
17. **C2-14, `architecture.md`.** El comando de `torneo-db` (`docker network create --driver bridge torneo-db`, sin subred fija) sale de RF-C7, porque ADR-0006 D1 no lo escribe. El fragmento de la configuración del proxy nombra el servicio `traefik` del compose del proxy, y advierte que se agrega `torneo-edge` sin quitar la red `coolify`.
18. **Evidencia local de C2 (Mac ARM, Colima, 2026-10-25, con load average de 280 a 410 por otra sesión).**
    - **`just check`:** verde en 73 s. Prettier sin cambios; los 1213 tests de Vitest de `web` y los 200 de `scripts/` (8 archivos, sin `check-workflows`); `check-tokens`, `check-i18n`, `check-prerender`, `check-kit-config` y `check-compose`; `cargo fmt`, clippy dos veces, `cargo test --workspace` con Postgres, `_check-gen` y `check-ts-rs-graph`.
      - La primera corrida falló en un solo test de tiempo ajeno a C2: `sanitize.test.ts`, "`a://:`" lineal en 8 KB, que tardó 109 ms contra un tope de 50 ms con la máquina a load 370. Solo pasa 72/72, y la segunda corrida completa salió verde. C2 no toca `apps/web` (`git diff --stat ccfcf1e -- apps/web` vacío).
    - **`just e2e`:** verde, 63/63 en 42 s.
      - La primera corrida falló en un test de WebKit (`theme.spec.ts`, RF-A8: una navegación inesperada a `/es/`; lo más probable es que el clic llegara antes de la hidratación, sin verificar). Una repetición aislada (`--repeat-each 3`) falló en "test 5, no flicker" de WebKit: `__firstFrameTheme` llegó `undefined`, porque la sonda de `requestAnimationFrame` no corrió antes del `evaluate`. Los dos son de tiempo y aparecen con la CPU saturada (load de unos 400).
      - El runner de Actions no tiene esa carga, pero si aparecen ahí, son candidatos a revisión de qa-engineer (esperar a la hidratación), no de C2.
    - **`policy` simulado** en un clon superficial: verde (desviación 16).
19. **C2-15 y C2-16 se mueven al cierre** (hallazgo de architect y qa-engineer en el PR #5).
    - **C2-15 no se puede cumplir dentro del PR, por diseño.** La caché solo se guarda en un `push` a `main` (condición 7 de security-reviewer: `save-if` de rust-cache, `cache-to` de `images` y el `save` de `setup-js` y de Playwright). Un PR solo restaura caché de `main`, que antes del merge no tiene ninguna, así que un segundo push al PR también corre en frío. El plan pedía algo que contradecía su propio diseño de caché.
    - **Duraciones en frío del run [40204955327](https://github.com/kike1303/torneo-absurdo/actions/runs/40204955327)** (head `e3390fa`, 7/7 en verde, 5 min 33 s de reloj):

      | Job | Duración |
      |---|---|
      | `changes` | 6 s |
      | `policy` | 19 s |
      | `web` | 34 s |
      | `e2e` | 112 s (1.9 min) |
      | `rust` | 208 s (3.5 min) |
      | `images` | 318 s (5.3 min) |
      | `ci-ok` | 3 s |

      Una corrida en caliente no tarda más que una en frío, así que estos números son la **cota superior** del RNF de velocidad: `rust` y `images` quedan por debajo de los 8 min de D7. El riesgo "`rust` por encima de 8 min (p50)" se cierra, y los `timeout-minutes` (20 y 25) no hace falta subirlos. Memoria en reposo, en el mismo run: `server` 11,890,852 B y `web` 23,949,476 B.
    - **La medición en caliente pasa al cierre:** paso 0b del checklist de RF-C19, con el PR (b) para `rust` y el (f) para `images`.
    - **C2-16, de (a) a (e), también pasa al paso 0b,** antes del paso 4. Es la evidencia de RF-C8 y del criterio C1: "job saltado → `ci-ok` verde" y "test roto → `ci-ok` rojo" nunca se observaron en este PR, porque toca `.github/workflows/**` (un filtro común) y corrieron todos los jobs.
    - **Pendiente de evidencia en TA-1 (nit del architect):** el mensaje real de GHCR cuando el paquete todavía no existe. Se registra en el paso 7 del cierre. Si GHCR responde `denied`, el job falla cerrado: es seguro, pero frena el cierre, y se ajusta la heurística con ese mensaje.
20. **Revisión del PR #5: `check-workflows.mjs` se endurece.** Para cada hallazgo, primero un test que fallaba y después el arreglo (14 casos nuevos).
    - **Acciones locales (Important de security-reviewer).** Todo `uses:` local tiene que cumplir `^\./\.github/actions/[a-z0-9-]+$` y tener su `action.yml` en el árbol, porque solo esas acciones se analizan. Fallan `./tools/evil`, `./`, `./.github/actions/../x`, `./.github/actions/../../tools/x`, una acción inexistente, una en mayúsculas y una ruta inválida usada desde una acción compuesta.
    - **`actions/checkout` sin distinguir mayúsculas** (`Actions/checkout` sin `persist-credentials: false` fallaba en abrir).
    - **`container:` y `services:`** tienen que fijar su imagen por `@sha256:`. Hoy ningún job los usa (RF-C9).
    - **`${{ }}` dentro de `with.script`** falla, por la misma inyección que en `run:` (por ejemplo, con `actions/github-script`).
    - **Las reglas de texto también leen los valores ya parseados.** Antes, un escape YAML como `"…/api/v1/deplo\x79"` o `"\x43oolify"` pasaba la regla de texto crudo.
    - **`check-workflows.mjs` corre en `just check`,** al final de `check-web` y sin sus tests (RF-C10), así que también corre en el hook `pre-push` y en el job `web`.

## Revisión

### architect — 2026-10-24 — APPROVE

**Verifiqué:**
- El plan completo, contra:
  - la spec: RF-C1 a C19, sus enmiendas y las "Áreas de preocupación" 5, 6, 8, 11 y 13;
  - ADR-0006: D1 a D8 y las verificaciones 1 a 17;
  - las desviaciones de los planes A y B.
- El código en `main`: `justfile`, `SECURITY_HEADERS`, `compose.dev.yml`, `rust-toolchain.toml`, `.node-version`, el workspace, `build.rs`, `cli.rs` y `.gitignore`.
- **La deprecación de Debian 12:** el README de distroless la confirma. Además, `nodejs22-debian12` no tiene subidas desde el 2026-02-14, y los digests `-debian13` son reales.
- Seis SHAs de actions, comprobados.
- `delete-package-versions`: es `node20`, la v5 es de 2024 y no hay versión para `node24`.

**Decisiones:**
1. **Debian 13: aceptado.** Escribí una enmienda en la spec (RF-C1 y C2) y agregué una nota en la sección "Notas" de ADR-0006, sin tocar su `status`. Como las escribí yo, las aprueban security-reviewer y el owner, no yo.
2. **Un plan para dos PRs (C1 y C2): aceptado.**
3. **Respaldo de la retención con `gh api`: aceptado,** siempre que se cumpla el hallazgo 1. Va a "Desviaciones" y se le agrega una nota en ADR-0006.
4. **Orden del checklist RF-C19:** correcto.
5. **Cada RF-C tiene su prueba.**
6. **`prod-local` en amd64 dentro del job `images`:** aceptado.
7. **"Despliegue" de `architecture.md`:** la escribe el PR C2. Tiene que nombrar `-debian13` y quitar a Postgres del compose. La reviso en ese PR.

**Hallazgos:**
1. **Respaldo con `gh api` (condición):**
   - Endpoint `/users/${owner}/packages/container/torneo-absurdo%2F<img>/versions`, no `/user/...`.
   - Paginar con `per_page=100` y ordenar por `created_at` descendente.
   - Si hay 10 versiones o menos, no borrar nada.
   - Pasar los valores por `env:` y fallar ante cualquier error.
2. **Nombres de las imágenes locales:** deben ser `ghcr.io/kike1303/torneo-absurdo/{server,web}:local` y `:ci`, no `torneo/server:local`. Además, `images` tiene que pasar `APP_VERSION=sha-<7>`.
3. **Filtro de `images`:** agregar `scripts/local-networks.sh`.
4. **`timeout-minutes` de `images`:** autorizo subirlo hasta 35 en la primera corrida en frío y anotarlo en "Desviaciones".
5. **RNF de memoria:** que el job falle si `server` pasa de 64 MB o `web` de 200 MB. Si no, que la tabla diga que es una comprobación manual.
6. **Paso C1-0:** la enmienda la aprueban security-reviewer y el owner, no el architect.

### security-reviewer — 2026-10-24 — APPROVE

**🤖 security-reviewer**

**Verifiqué:**
- El plan completo, contra:
  - RF-C1 a RF-C19;
  - ADR-0006 (D5, D6, D7 y verificaciones 9, 16 y 17);
  - ADR-0008 D7.
- Siguen en pie las condiciones que ya había aprobado:
  - `.dockerignore` con `**/`;
  - `check-workflows.mjs` completo;
  - el tag validado por `env:`;
  - la cadena de suministro con digest, SHA, `--locked` y checksums;
  - HSTS sin `includeSubDomains` ni `preload`;
  - `production` configurado antes del tag.
- Permisos de los jobs:
  - ninguno tiene `id-token`;
  - solo `release.yml/build` tiene `packages: write`;
  - solo `changes` tiene `pull-requests: read`.

**Condiciones (no bloquean; antes del PR que corresponda):**
1. **`check-workflows.mjs`:**
   - Lista cerrada de permisos por job, con fixtures que incluyan `id-token: write`, `write-all` y `read-all`.
   - `permissions: {}` en `deploy`, `verify` y `ci-ok`.
   - Prohibir `continue-on-error` en `ci.yml`.
2. **Environment `production`:**
   - `can_admins_bypass: false`, y que quede en la evidencia.
   - `prevent_self_review: false` es correcto con un solo owner.
   - Regla escrita: ningún agente aprueba, rechaza ni se salta un deployment.
3. **Respaldo de retención con `gh api`:**
   - Endpoint `/users/kike1303/packages/container/...` con `GITHUB_TOKEN` y el `packages: write` de `build`. Nunca un PAT.
   - Paginar antes de borrar y abortar si la lista viene vacía o con error.
   - Nunca borrar el digest recién publicado.
4. **Debian 13:** aprobado y preferido. Si se usa el respaldo `-debian12`, bloquea TA-8. En el job `images`, `docker run <web> --version` debe coincidir con `.node-version`.
5. **`check-dockerignore.sh`:**
   - Crear el canario con `set -C` y limpiar con `rm -rf "${tmp:?}"`.
   - Agregar `apps/web/.env` como tercer sujeto.
   - Si falla, mostrar solo la ruta, nunca el contenido.
6. **`setup-just` y `cargo-deny`:**
   - Descargar con `curl -fsSL --proto '=https'`.
   - Correr `sha256sum -c` antes de extraer y extraer solo el binario.
   - Opcional: firma minisign de zig.
7. **Caché de `setup-js`:** corregir la frase, porque `actions/cache` también guarda en los PRs, o separar restore y save con `if` en `main`.

**Sin objeción:** los rulesets (`ci-ok` con `integration_id`), `approval_policy`, `default_workflow_permissions: read`, `sha_pinning_required: true` (recomendado), el hook `pre-push` e `infra/local.env`.

### devops-engineer — 2026-10-24 — RESPUESTA

**🤖 devops-engineer**

Incorporé al cuerpo del plan todas las condiciones y hallazgos. No toqué `status` ni `approvals`, ni otros archivos.

**architect:**
1. **Respaldo de la retención con `gh api`:** reescrito en Riesgos ("`actions/delete-package-versions` corre en `node20`"). Usa `/users/${OWNER}/…`, `--paginate` con `per_page=100`, orden por `created_at` descendente, no borra nada con 10 versiones o menos, pasa los datos por `env:` y aborta ante cualquier error o lista vacía.
2. **Nombres de las imágenes:** C1-3 y C1-4 usan `ghcr.io/kike1303/torneo-absurdo/{server,web}:local`. La decisión 16 usa `:ci` y `APP_VERSION=sha-<7>` por `env:`.
3. **Filtro de `images`:** la decisión 16 lista el filtro completo, con `scripts/local-networks.sh`.
4. **`timeout-minutes`:** 25 en `images` (decisión 16). Se puede subir hasta 35 tras la primera corrida en frío, anotándolo en "Desviaciones".
5. **RNF de memoria:** el job falla si `server` llega a 64 MB o `web` a 200 MB, con base 10 (decisión 16 y tabla de Pruebas). La medición de la Mac es solo informativa.
6. **C1-0:** "Bloqueo de inicio" y el paso C1-0 dicen que la enmienda la aprueban security-reviewer (ya dada) y el owner, no el architect.
7. **`architecture.md`:** la fila de "Archivos que cambian" exige nombrar `static-debian13:nonroot` y `nodejs22-debian13:nonroot`, y Postgres fuera del compose.

**security-reviewer:**
1. **Permisos:** la decisión 23 es nueva. Trae la tabla cerrada por job, con `permissions: {}` en `deploy`, `verify` y `ci-ok`. Prohíbe `write-all`, `read-all`, `id-token` y `continue-on-error` en los dos workflows, y agrega los fixtures de cada regla. También quedan en "Archivos que cambian" y en la fila C12 de Pruebas. Renumeré el hook como decisión 24.
2. **Environment `production`:** el cierre 3 incluye `can_admins_bypass: false` y la regla escrita "ningún agente aprueba, rechaza ni se salta un deployment". El cierre 6 lo exige en la evidencia, y el cierre 7 aclara que el agente no toca el rechazo.
3. **Retención:** está en la misma viñeta de Riesgos que el hallazgo 1 del architect. Usa solo el `GITHUB_TOKEN` de `build`, nunca un PAT, y falla en vez de borrar el digest recién publicado.
4. **Debian 13 y la versión de Node:** la decisión 16 y C1-4 exigen que `docker run <web> --version` sea igual a `v$(cat .node-version)`. Si se usa el respaldo `-debian12`, TA-8 queda bloqueado en Linear (Riesgos y "Bloqueo de inicio").
5. **`check-dockerignore.sh`:** la decisión 11 y C1-2 incluyen `set -C`, `rm -rf "${tmp:?}"`, `apps/web/.env` como tercer sujeto y, si falla, solo la ruta.
6. **Descargas de `setup-just` y `cargo-deny`:** las decisiones 14 y 17 usan `curl -fsSL --proto '=https' --tlsv1.2`, corren `sha256sum -c` antes de extraer y extraen solo el binario. **Minisign de zig no lo adopto** (decisión 3): exigiría otro binario sin verificar en `chef`, y el SHA-256 fijado ya cubre los cambios posteriores del tarball.
7. **Caché de `setup-js`:** la decisión 14 separa `actions/cache/restore` y `actions/cache/save`, con el save solo en `push` a `main`. Playwright sigue el mismo patrón, así que la frase de "Estimación de CI" ahora es exacta.
8. **`Config.User` (condición enviada por el coordinador):** la decisión 16 y C1-4 exigen que `docker inspect -f '{{.Config.User}}'` dé `65532` o `nonroot` en las dos imágenes finales. Cualquier otro valor hace fallar `images`.
