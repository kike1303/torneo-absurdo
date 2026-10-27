---
title: Arquitectura general
type: architecture
status: draft
author: architect
created: 2026-10-20
updated: 2026-10-25
---

# Arquitectura general

> Vista de alto nivel. Las decisiones concretas viven en los ADRs de `docs/decisions/`, y el detalle de cada pieza en la spec de su feature.

## Componentes

```
          navegador (móvil / desktop)
  ┌──────────────────────────────────────────┐
  │ SvelteKit UI ── renderer (PixiJS)        │
  │      │              ▲                    │
  │      │ HTTP         │ FightEvent (WS)    │
  │      │              │ engine-wasm (verif)│
  └──────┼──────────────┼────────────────────┘
         ▼              │
   Traefik (Coolify) ── mismo dominio: /  → web,  /api /ws → server
         │              │
   ┌─────▼─────┐  ┌─────┴──────────────────────────────┐
   │ web (node)│  │ server (Rust/axum)                 │
   │ SSR + i18n│─▶│ auth · apuestas · ledger · rankings│
   └───────────┘  │ scheduler ─▶ engine (crate)        │
                  └─────┬──────────────────────────────┘
                        ▼
                    Postgres
```

## Principios
1. **El servidor Rust es la fuente de verdad** de usuarios, sesiones, monedas, apuestas, peleas y rankings. SvelteKit solo presenta: hace SSR llamando al API y no tiene base de datos propia.
2. **Motor puro**: `engine::simulate(seed, fighter_a, fighter_b, ruleset) -> FightLog`. Sin I/O, sin reloj y determinista en todas las plataformas (nativo y WASM).
3. **Sin video**: el cliente recibe eventos y anima. Se sincroniza con el reloj del servidor (`server_time` y offset).
4. **Sin texto en el dominio**: el motor y el protocolo emiten códigos; la UI y el narrador los traducen (ADR-0007).
5. **Contenido como datos**: personajes y frases en `content/`, versionados y validados en CI.

## Ciclo de vida de una pelea

```
Scheduled ─▶ BettingOpen ─▶ BettingClosed ─▶ Live ─▶ Settled
              │ publica       │ se calcula     │ emite    │ revela la semilla,
              │ SHA-256(seed) │ el FightLog    │ eventos  │ liquida apuestas
              │               │ (no sale aún)  │ según el │ (transacción),
              │               │                │ tiempo   │ actualiza rating
```
- La semilla se genera con un CSPRNG al abrir las apuestas y vive solo en el servidor hasta `Settled`.
- Durante `Live`, cada evento se envía cuando le toca (más un pequeño buffer). Quien entra tarde recibe los eventos ya ocurridos y sigue en vivo.
- Las repeticiones usan el log guardado, y cualquiera puede re-simularlo en el navegador con `engine-wasm` para verificarlo.

## Datos (borrador)
- `users`, `sessions`, `oauth_accounts`
- `ledger_entries` (asientos inmutables), con saldos derivados o materializados con verificación
- `fights` (personajes, ruleset_version, seed_commit, seed —oculta hasta Settled—, resultado, log)
- `bets`, `pools`
- `tournaments`, `tournament_rounds`
- `character_ratings`, `seasons`, `season_stats`

## Tipos compartidos
`crates/protocol` define los mensajes WS y los DTOs del API con serde. Es una librería pura: sin I/O, sin `tokio`, sin reloj y sin texto para humanos (ADR-0007).

- **Generación:** `ts-rs`, opcional detrás de la feature `ts` para que el server no lo compile. `just gen` vacía `packages/protocol-ts/src/generated/`, exporta un `.ts` por tipo (imports con `.js`), les antepone la cabecera de "generado, no editar", escribe el barril `src/index.ts` (`export type *`) y formatea con Prettier. Los archivos generados se commitean, y `just check` falla si están desactualizados. Nunca se editan a mano.
- **Paquete:** `@torneo/protocol` (`packages/protocol-ts`) exporta el TS fuente, sin paso de build. `apps/web` lo consume con `import type`.
- **Convenciones (RF-B4):**
  - Enums con datos: `#[serde(tag = "type", rename_all = "snake_case")]` (o el campo de etiqueta que corresponda, como `status` en `HealthResponse`).
  - Códigos de error: `dominio.motivo` (`ws.origin_forbidden`), como enums con `#[serde(rename = "dominio.motivo")]` en cada variante, nunca `String` libre. Cada enum de códigos es dueño de un solo prefijo (`ApiErrorCode` → `http.*`, `WsErrorCode` → `ws.*`) y expone `ALL` y `as_str()`.
  - `ErrorCode` es la unión sin etiquetar de los enums de códigos y `ApiError { error: ErrorCode }` es el cuerpo de toda respuesta HTTP de error. Un dominio nuevo (por ejemplo `auth.*`) agrega su enum y su variante en `ErrorCode`; los conjuntos de cadenas no se solapan (lo comprueba un test).
  - Nada de enteros de 64 bits o más (`u64`, `i64`, `u128`, `i128`, `usize`) en tipos exportados: `just gen` falla si aparece `bigint`. Si hace falta, se serializan como cadena.

## Despliegue

> Fuente de las decisiones: [ADR-0006](decisions/0006-despliegue-y-ci.md). Detalle de la fase C: [spec de 001](features/001-fundaciones/spec.md) (RF-C1 a RF-C19) y [plan-c](features/001-fundaciones/plan-c.md). Esta sección es el resumen operativo; si algo no coincide, mandan el ADR y la spec.

### Piezas
- **Postgres es un recurso standalone de Coolify** (`postgres:17-alpine`, 512 MB, `shared_buffers=128MB`, 20 conexiones), **fuera del compose** y sin puerto público. Sus backups son los programados de Coolify (diarios a las 09:00 UTC, 7 días en el VPS y 30 días en Cloudflare R2; ADR-0006 D9).
- **`infra/docker-compose.yml` tiene solo `web` y `server`.** Los dos usan `image: ghcr.io/kike1303/torneo-absurdo/{server,web}:${IMAGE_TAG:?}` y ninguno tiene `build:` ni `ports:`. Coolify lee el compose de `main`, con el auto-deploy apagado, y le pone `IMAGE_TAG`, `PUBLIC_HOST` y `DATABASE_URL` como variables de entorno. Esas variables viven **solo en Coolify**: no hay `infra/.env` con valores reales. `scripts/check-compose.mjs` exige la forma exacta del archivo:
  - `mem_limit` = `memswap_limit` (256 MB `server` y 384 MB `web`, con `NODE_OPTIONS=--max-old-space-size=256`);
  - healthchecks en forma exec (15 s / 3 s / 3 / 30 s), y `web` espera a que `server` esté `healthy`;
  - `read_only`, `cap_drop: [ALL]`, `no-new-privileges`, `platform: linux/amd64` y `restart: unless-stopped`;
  - logs `json-file` de 3 × 10 MB, en los que se apoya el plazo del aviso de privacidad;
  - `stop_grace_period: 15s` en `server`, con cierre `1012` de los WebSockets (ADR-0006 D10);
  - `RUST_LOG=info` exacto.
- **Imágenes.** Se construyen en GitHub Actions (amd64 nativo) y **nunca en el VPS**. Se publican en GHCR público, así que el VPS hace pull sin `docker login`.
  - `server`: binario estático musl (`cargo-chef` y `cargo-zigbuild`) sobre **`gcr.io/distroless/static-debian13:nonroot`**.
  - `web`: `build/` de adapter-node y `package.json`, sin `node_modules`, sobre **`gcr.io/distroless/nodejs22-debian13:nonroot`** (Node igual a `.node-version`).
  - Las dos bases van por digest y corren como uid `65532`. Las `-debian12` están deprecadas (enmienda del 2026-10-24 en la spec y nota en ADR-0006).
  - El server sirve HTTP/1.1 con su propio bucle de hyper `http1` (`header_read_timeout`, upgrades para WS), no con `axum::serve` (desviación 8 de [plan-b](features/001-fundaciones/plan-b.md)).
- **Redes** (ADR-0006 D1). "Connect to Predefined Network" está apagado: nada nuestro vive en la red compartida `coolify`.
  - **`torneo-edge`**: bridge externa y dedicada, `10.250.0.0/28`. Solo la usan `coolify-proxy` (IP fija `10.250.0.2`), `web` y `server`, que lleva el alias `torneo-server`. Es la única subred que el server acepta en `TRUSTED_PROXIES` (`10.250.0.2/32`).
  - **`torneo-db`**: bridge externa, que solo comparten `server` y el Postgres standalone. `web` no la ve.
- **Traefik** (el proxy de Coolify; ADR-0006 D2 y D11). Usa etiquetas propias con prefijo `torneo-*` y sin strip de prefijo:
  - `torneo-api` (`/api` y `/ws`, prioridad 100) va a `server:8080`;
  - `torneo-web` (todo lo demás, prioridad 1) va a `web:3000`;
  - `torneo-*-http` redirigen a HTTPS con `torneo-https-redirect`;
  - los cuatro routers llevan `observability.accesslogs=false`.

  El router `torneo-web` lleva además el middleware `torneo-web-headers` (`nosniff`, `Referrer-Policy: strict-origin-when-cross-origin` y HSTS `max-age=31536000` **sin** `includeSubDomains` ni `preload`). Así cubre las respuestas que no pasan por el `handle` de SvelteKit: los estáticos y la 308 de `/healthz/`. axum ya pone las suyas.

### Release con gate manual (ADR-0006 D5 y D6)
1. El owner empuja un tag `vX.Y.Z` (un ruleset de tags se lo reserva). `release.yml/build` valida el tag con `^v[0-9]+\.[0-9]+\.[0-9]+$` antes de construir y comprueba que ni `vX.Y.Z` ni `sha-<7>` existen ya, porque una etiqueta publicada nunca se mueve. Después publica `server` y `web` con las dos etiquetas sobre el mismo digest, sin `latest`, y con un manifiesto de imagen por etiqueta (`provenance` y `sbom` apagados, comprobado con `imagetools inspect --raw`). Al final aplica la retención: conserva las 10 versiones más recientes de cada paquete.
2. `deploy` usa el environment `production`, que exige la **aprobación del owner**. Lo que hace depende de la variable `DEPLOY_MODE`:
   - **`manual`** (por defecto, también si la variable no existe): el owner fija `IMAGE_TAG` en Coolify, despliega a mano y aprueba el environment. El job solo registra el tag aprobado.
   - **`api`**: no está implementado y el job **falla**. Implementarlo requiere un nuevo OK del owner, un token de Coolify de alcance de equipo y un secreto del environment (riesgo descrito en ADR-0006 D6).
3. `verify` consulta `https://$PUBLIC_HOST/api/health` (variable de repositorio `PUBLIC_HOST`) cada 10 s durante 5 min, y falla si `version` no es el tag.
4. **Rollback:** `workflow_dispatch` de `release.yml` desde `main` con el tag anterior. No reconstruye (solo comprueba que el tag existe en GHCR) y pasa por la misma aprobación.

Antes de aprobar un release que traiga migraciones se hace un "Backup now" de Postgres en Coolify, y nunca se despliega durante un main event (ADR-0006 D9 y D10).

### Preparación del VPS (una vez, ADR-0006 D1, D4 y D11)
Son los cambios en el proxy compartido que el owner aprobó el 2026-10-21. Se hacen por SSH y en la UI de Coolify.

1. **Red `torneo-edge`.** Primero se comprueba que `10.250.0.0/28` no choca con ninguna red existente, y después se crea con un `--ip-range` que deja la `.2` reservada al proxy:
   ```sh
   docker network inspect $(docker network ls -q) --format '{{.Name}} {{range .IPAM.Config}}{{.Subnet}} {{end}}'
   docker network create --driver bridge --subnet 10.250.0.0/28 --gateway 10.250.0.1 \
     --ip-range 10.250.0.8/29 torneo-edge
   docker network create --driver bridge torneo-db
   ```
   El Postgres standalone se conecta a `torneo-db` desde su configuración en Coolify.
2. **`coolify-proxy` en `torneo-edge` con IP fija.** En Coolify, Servers → Proxy → Configuration, se agrega la red externa a la lista de redes del servicio del proxy, **sin quitar `coolify`**, y se reinicia el proxy:
   ```yaml
   networks:
     torneo-edge:
       external: true
   services:
     traefik:
       networks:
         torneo-edge:
           ipv4_address: 10.250.0.2
   ```
   → `docker inspect coolify-proxy --format '{{json .NetworkSettings.Networks}}'` muestra `torneo-edge` con `10.250.0.2`.
3. **Swap de 2 GB con `swappiness` 10**, solo si `swapon --show` no muestra ninguno:
   ```sh
   sudo fallocate -l 2G /swapfile && sudo chmod 600 /swapfile
   sudo mkswap /swapfile && sudo swapon /swapfile
   echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab
   echo 'vm.swappiness=10' | sudo tee /etc/sysctl.d/99-swappiness.conf && sudo sysctl --system
   ```
4. **Logs de acceso de Traefik apagados** (D11). El comando del proxy no lleva `--accesslog`, y Traefik es ≥ 3.1 (lo necesita `observability.accesslogs`):
   ```sh
   docker inspect coolify-proxy --format '{{json .Config.Cmd}}' | tr ',' '\n' | grep -i accesslog
   docker exec coolify-proxy traefik version
   ```
   El primero no tiene que imprimir nada.
5. **Limpieza de Docker de Coolify** (Servers → Settings): diaria y además al pasar el 80 % de disco, con **"Delete Unused Volumes" apagado**.

> **Advertencias.**
> - **Nunca borrar `torneo-edge`.** Si la red no existe, el compose del proxy falla y se caen **todas** las apps del VPS. `just prod-local-down` solo borra redes con la etiqueta `torneo.local=1`, así que no la toca.
> - **"Reset configuration to default" del proxy en Coolify quita `torneo-edge` y la IP `10.250.0.2`.** Después de un reset, de reiniciar el proxy o de actualizar Coolify, se repite el paso 2 y la verificación 15 de ADR-0006.

### Prerrequisitos del primer despliegue a producción (bloquean; RF-C18)
Ninguno se cumple en la feature 001. Se siguen en Linear (TA-8, bloqueado por la feature de auth y por TA-6).

1. **Límites anónimos de ADR-0008 D6 y D7 en `/ws`, implementados y probados:** como máximo 50 sockets simultáneos por `ip_key`, y upgrade a 30/min con ráfaga de 10 por IP, con `TRUSTED_PROXIES=10.250.0.2/32` (ADR-0006 D2).
2. **Las verificaciones de ADR-0006 que no se hicieron en el cierre de 001** (RF-C19), más dos comprobaciones con el stack desplegado:
   - `docker inspect` de `web` y `server`: `ReadonlyRootfs`, `CapDrop`, `SecurityOpt` y `LogConfig`, para confirmar que Coolify respeta el endurecimiento y los logs al reescribir el compose.
   - `curl -I` a `https://${PUBLIC_HOST}/es/`, `/_app/version.json` y `/healthz/` (la 308). Las tres respuestas traen `X-Content-Type-Options: nosniff`, `Referrer-Policy: strict-origin-when-cross-origin` y `Strict-Transport-Security: max-age=31536000`, sin `includeSubDomains` ni `preload` y cada una una sola vez (Traefik solo emite HSTS sobre TLS).
3. **La configuración de GitHub de RF-C19**, si no se hizo en el cierre de 001: rulesets de `main` (con `ci-ok` requerido) y de tags `v*`, el environment `production` con el owner como reviewer y `can_admins_bypass: false`, aprobación de workflows para todos los contribuidores externos y las alertas de Dependabot.
4. **Preparación del VPS** (sección anterior), con la verificación 15 de ADR-0006 en verde, y la **prueba de restauración** de backups de ADR-0006 D9.
