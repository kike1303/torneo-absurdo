---
id: "001"
title: Fundaciones del monorepo
type: intent
status: approved
risk: high
author: product-owner
reviewers: [architect]
approvals: [{by: architect, date: 2026-10-21}]
linear: TA-1
depends_on: []
created: 2026-10-20
updated: 2026-10-21
---

# Intent: Fundaciones del monorepo

## Problema
El repo solo tiene documentación y carpetas vacías. Antes de construir cualquier feature necesitamos una base estandarizada: que cada app arranque, que haya i18n, theme, componentes base, tipos compartidos, calidad automatizada y un camino de despliegue. Si no existe, cada feature va a reinventar su parte. Los ADRs 0006, 0008 y 0009 ya están aprobados y fijan cómo debe ser esa base.

## Resultado propuesto
Cualquier persona (o agente) clona el repo, corre un comando y tiene en local:
- La web SvelteKit en `/es` y `/en`, con dark/light/system sin parpadeo, la paleta neutra provisional, las primitivas de UI y el catálogo `/dev/components`.
- El server Rust respondiendo `/api/health` y un WebSocket de eco.
- Tipos generados desde `crates/protocol` y usados en la web.
- Postgres local con migraciones.
- `just check`, que valida formato, lint, tipos, tests, llaves de i18n y tokens de diseño.
- Una CI rápida que solo corre lo que el PR toca, y las imágenes y el compose listos para Coolify, sin desplegar todavía.

Se entrega como **una sola feature (TA-1) en tres fases**, con un PR por fase y sus propios criterios de éxito.

## Alcance y fases
**Fase A — web, i18n y design system** (no depende de otras fases)
- App SvelteKit con rutas `/es` y `/en`, cambio de idioma por cookie (ADR-0007) y `/healthz` (ADR-0006 D3).
- Design system de ADR-0009: tokens con paleta neutra provisional, tema por cookie, primitivas, fronteras de ESLint, `check-tokens.mjs` y catálogo con `ENABLE_DEV_CATALOG`.
- Logs de `web` sin datos del cliente (aviso de privacidad, "Pendiente" punto 8).
- Primera versión de `just check` con lo que existe en esta fase.

**Fase B — server, protocol y Postgres** (depende de A)
- Server: `/api/health` con versión horneada en la imagen, subcomando de healthcheck, espera a la BD y migraciones al arrancar (ADR-0006 D3, D10), y WebSocket de eco.
- `crates/protocol` con generación de tipos TS (`just gen`) y un tipo usado en la web. **Depende de A** porque necesita que la web exista.
- Las dos piezas base de ADR-0008: redacción de headers sensibles en los logs del server (D3) y ausencia de capa CORS (D4).
- Forma local de levantar web, server y Postgres con un comando.
- `just check` extendido a Rust.

**Fase C — infra y CI** (depende de A y B: las imágenes, los healthchecks y los filtros por ruta necesitan que las dos apps existan)
- Imágenes de `server` (binario estático musl, sin OpenSSL) y `web`, construidas en Actions y nunca en el VPS (ADR-0006 D5, D8).
- `infra/docker-compose.yml` de producción con solo `web` y `server`, con las redes, las etiquetas Traefik `torneo-*`, los healthchecks y los límites de memoria de ADR-0006. Postgres queda fuera (recurso standalone de Coolify), así que la forma local de la fase B también debe poder levantar las imágenes de producción con Postgres; la spec decide cómo.
- `ci.yml` (filtros por ruta, `ci-ok` como único check requerido, caché, permisos mínimos, `cargo deny`, chequeo contra `pull_request_target`) y `release.yml` (publicación en GHCR y gate de despliegue con `DEPLOY_MODE=manual`), según ADR-0006 D5–D7.
- Hook `pre-push` que corre `just check`, instalable con `just hooks` (ADR-0006 D7).
- Sección "Despliegue" de `docs/architecture.md`.

En las tres fases: los comandos de `CLAUDE.md` se actualizan con lo que la fase agrega.

**No entra:** el primer despliegue a producción y las verificaciones de ADR-0006 que lo bloquean (restauración de backups, simulacro con pelea `Live`, aislamiento de red e IP en el VPS, retención de GHCR en el VPS).

## Usuarios y sistemas afectados
- Usuarios: ninguno todavía (es base interna). Owner y agentes.
- Sistemas: todo el monorepo (`apps/*`, `crates/*`, `packages/*`, `infra/`, `.github/`, `scripts/`), más la configuración del repo en GitHub.

## Restricciones
- ADR-0003 (stack), ADR-0006 (despliegue y CI), ADR-0007 (i18n), ADR-0008 (solo las piezas base citadas arriba) y ADR-0009 (design system), todos aprobados.
- Repo público: ningún secreto en el repo ni expuesto a PRs; nada de modelo de negocio en docs ni commits.
- Mobile-first (375 px), WCAG 2.2 AA y cero strings literales en la UI.
- Presupuesto cero: sin servicios de pago ni CDNs.
- Sin features de producto: solo la base.
- El WebSocket de eco no puede llegar a producción aceptando mensajes arbitrarios (ADR-0008 D6); la spec decide si es solo de dev o si ya aplica `Origin` y los límites.
- **`risk: high`** para toda la feature: la fase C toca secretos, permisos de workflows y el gate de despliegue, y las fases A y B fijan qué datos del cliente (IP, cookies, headers) quedan en los logs, lo que el aviso de privacidad promete. Por eso el security-reviewer revisa la spec y los tres PRs, y el owner da su OK explícito (gate humano de `docs/process.md`).

## Criterios de éxito
**Fase A**
- [ ] `just check` pasa en limpio y falla si falta una llave de i18n en algún idioma.
- [ ] Cambiar de idioma funciona en `/es` y `/en` y persiste por cookie.
- [ ] Pasan los tests 1–6, 8 y 9 de ADR-0009 (el 7 es del renderer y está fuera de alcance).
- [ ] `/healthz` de `web` responde 200.
- [ ] Ante una petición con `Cookie`, `Authorization` y `X-Forwarded-For`, y ante un error que pasa por `handleError`, los logs de `web` no contienen la IP del cliente (`getClientAddress()`), `X-Forwarded-For`, cookies ni los headers sensibles de ADR-0008 D3 (test en CI).

**Fase B**
- [ ] Un solo comando levanta en local web, server y Postgres, con migraciones aplicadas.
- [ ] Un tipo definido en `crates/protocol` aparece en TS con `just gen` y se usa en la web.
- [ ] `/api/health` responde 200 con `version` cuando la BD está arriba y 503 cuando está abajo; el subcomando de healthcheck sale con 0 o 1 en cada caso (test de integración en CI).
- [ ] Un mensaje enviado al WebSocket de eco vuelve igual (test de integración en CI).
- [ ] Una respuesta del server no lleva `Access-Control-Allow-Origin` (ni ante un preflight ajeno), y los logs del server no contienen `Cookie`, `Set-Cookie`, `Authorization`, `X-Internal-Secret` ni `X-Client-Ip`.
- [ ] `just check` cubre también formato, lint y tests de Rust y pasa en limpio.

**Fase C**
- [ ] Un PR de solo `docs/**` termina con `ci-ok` en verde en ≤ 2 min sin correr `rust` ni `web`; un PR que toca `crates/protocol/**` corre los dos; un job fallido deja `ci-ok` en rojo.
- [ ] El job `rust` pasa `cargo deny check bans` y comprueba que el binario es estático.
- [ ] CI falla si algún workflow usa `pull_request_target`, y ningún job de `ci.yml` referencia `secrets.*` salvo `GITHUB_TOKEN` (ADR-0006, verificación 9).
- [ ] En un PR, el job `images` construye las dos imágenes sin hacer push.
- [ ] Un tag de prueba publica `server` y `web` en GHCR con `vX.Y.Z` y `sha-<7>`, cada una como manifiesto de imagen y no como índice (ADR-0006 D5).
- [ ] Con `DEPLOY_MODE=manual`, `deploy` espera la aprobación del environment `production` y no hace ninguna llamada a Coolify (ADR-0006, verificaciones 10 y 17).
- [ ] El compose de producción sin `IMAGE_TAG` falla al levantar, y la forma local con Postgres levanta las imágenes de producción.
- [ ] `just hooks` instala el `pre-push`, que corre `just check`.
- [ ] `docs/architecture.md` ("Despliegue") y los comandos de `CLAUDE.md` reflejan lo construido.

## Fuera de alcance
- Auth y sesiones (feature propia, ADR-0008), incluidos el middleware de `Origin`/CSRF, la IP del cliente con `TRUSTED_PROXIES` y el rate limiting.
- Tema guardado en el perfil (llega con auth; en 001 solo cookie).
- Motor, renderizado (incluido el contrato `RendererTheme` de ADR-0009 D7 y su test 7), apuestas.
- Identidad visual final (feature nueva; 001 usa la paleta neutra provisional).
- Primer despliegue a producción, backups y su prueba de restauración.

## Preguntas abiertas
- **Generador de tipos** (`ts-rs`, `specta` o `typeshare`): decide architect en la spec.
- **Herramienta del hook `pre-push`** (`lefthook` o un script simple): el hook ya está decidido por ADR-0006; la herramienta la decide architect en la spec.
- **WebSocket de eco y `Origin`**: ¿el eco es solo de dev o ya valida `Origin` y aplica los límites de ADR-0008 D6? Decide architect en la spec.
- **Logs del server y plazo**: qué registra `tower-http::trace` sobre el usuario y el plazo típico de los logs (`{{PLAZO_LOGS}}` del aviso de privacidad, "Pendiente" punto 8). Decide architect en la spec.
- **Configuración de GitHub** (ruleset de `main` con `ci-ok`, environment `production`, aprobación para forks). *Recomendación del architect:* que sea criterio de cierre de la fase C, como checklist posterior al merge, porque sin el ruleset `ci-ok` no es requerido de verdad y sin `production` no se puede probar el gate de `release.yml`; se verifica en GitHub, sin el VPS. **Pendiente de confirmación del owner.** Si el owner decide que no, el criterio del gate de `deploy` de la fase C se ajusta.
- **Preparación del VPS** (red `torneo-edge` con la IP fija del proxy, swap, apagar los logs de acceso de Traefik y limpieza de Docker). *Recomendación del architect:* que no sea criterio de cierre de 001, sino una tarea aparte en Linear, asignada a devops-engineer, que bloquee el primer despliegue; no se verifica en local ni en CI y sus verificaciones (13–15) ya están en "No entra". Choca con la frase "durante la feature 001" de las decisiones del owner en ADR-0006. **Pendiente de confirmación del owner.**

## Revisión
### architect — 2026-10-21 — REQUEST_CHANGES
- Verifiqué: el frontmatter contra el esquema de `docs/process.md` (válido); el alcance contra ADR-0003 (stack), ADR-0006 (D1–D11, Consecuencias y verificaciones 1–17), ADR-0007, ADR-0008 (D3, D4 y D6) y ADR-0009 (D1–D8 y tests 1–9), todos `approved`; la viabilidad con el VPS de 1 vCPU / 4 GB y presupuesto cero (no se compila en el VPS, GHCR público y Actions sin costo en un repo público, sin CDNs ni servicios de pago: viable); que cada criterio se pueda verificar en local o en la CI sin el VPS (todos lo son); y que no haya decisiones de diseño metidas en el intent (no las hay: la forma local con Postgres y las tres preguntas técnicas quedan para la spec). No contradice ningún ADR aprobado y "No entra" es coherente con las verificaciones de ADR-0006 que bloquean el primer despliegue, no el merge.
- Cambios pedidos:
  1. **Tamaño: partir la entrega.** Tal como está, son tres frentes independientes (web + i18n + design system con 8 tests de ADR-0009; server + protocol + Postgres; infra + CI + release), y en un solo PR no se pueden revisar ni verificar bien. Hay dos opciones aceptables: (a) tres features, por ejemplo 001a web+i18n+DS, 001b server+protocol+Postgres y 001c infra+CI, cada una con sus criterios; o (b) una sola feature con fases explícitas, un PR por fase y los criterios asignados a cada fase. En los dos casos hay que declarar las dependencias: el criterio "un tipo de `crates/protocol` se usa en la web" cruza web y protocol, y CI e imágenes necesitan que las apps existan. Recomiendo la (a).
  2. **WebSocket de eco sin criterio.** El "Resultado propuesto" lo promete, pero ningún criterio lo verifica. Hay que agregar uno verificable en la CI (por ejemplo, un test de integración en el que un mensaje enviado a `/ws` vuelve igual).
  3. **`release.yml` sin criterio.** Entra en el alcance, pero ningún criterio lo cubre. Hay que agregar los que se pueden verificar sin el VPS: en un PR, el job `images` construye las dos imágenes sin hacer push; un tag de prueba publica `server` y `web` en GHCR con `vX.Y.Z` y `sha-<7>`, cada una como un manifiesto de imagen y no como índice (ADR-0006 D5); y con `DEPLOY_MODE=manual`, `deploy` espera la aprobación de `production` y no llama a Coolify (verificaciones 10 y 17).
  4. **Completar los criterios de seguridad.** En los logs, falta `X-Client-Ip` en la lista de headers redactados (ADR-0008 D3 lo incluye). En el criterio de `pull_request_target`, falta la segunda mitad de la verificación 9 de ADR-0006: ningún job de `ci.yml` referencia `secrets.*` salvo `GITHUB_TOKEN`.
  5. **Alinear los criterios del design system con ADR-0009.** Faltan la mitad del test 4 (axe también en las páginas existentes con cookie `theme=light` y `theme=dark`), la mitad del test 6 (`?theme=` no tiene efecto fuera de `/dev/`) y el test 8 (capturas también en los dos temas). Lo más simple es un criterio que diga "pasan los tests 1–6, 8 y 9 de ADR-0009" (el 7 es del renderer y está fuera de alcance).
- Opinión sobre las preguntas del owner (no bloquean):
  - **Configuración de GitHub: sí debería ser criterio de cierre**, como checklist posterior al merge. Sin el ruleset, `ci-ok` no es requerido de verdad, y sin el environment `production` el gate de `release.yml` (cambio 3) no se puede probar. Además se verifica en GitHub, sin el VPS.
  - **Preparación del VPS: no debería ser criterio de cierre.** No se verifica en local ni en la CI, y sus verificaciones (13–15) ya están en "No entra". Propongo una tarea aparte en Linear que bloquee el primer despliegue, asignada a devops-engineer. Choca con la frase "durante la feature 001" de las decisiones del owner en ADR-0006, así que el owner tiene que confirmarlo.
- Notas para la spec (no bloquean el intent; las tomo como áreas de preocupación):
  - Un eco que acepta mensajes arbitrarios choca con ADR-0008 D6 ("el cliente solo manda suscripción/desuscripción", 4 KB por mensaje y límites de frecuencia). La spec tiene que decidir si el eco solo existe en dev o si ya aplica `Origin` y los límites, y no puede llegar así a producción.
  - El compose de producción usa las redes externas `torneo-edge` y `torneo-db`, y las imágenes son `linux/amd64`. La forma local tiene que resolver las dos cosas (crear o sustituir las redes, y la emulación en una Mac ARM) para que el criterio "levanta las imágenes de producción" se cumpla.
  - Recomiendo que la parte de infra y CI (o 001c, si se parte) lleve `risk: high`: toca secretos, permisos de workflows y redacción de cookies en logs, y así el security-reviewer revisa la spec y el PR.

### product-owner — 2026-10-21 — RESPUESTA
1. **Tamaño** → opción (b): una sola feature (TA-1) con tres fases y un PR por fase, en "Alcance y fases" y con los criterios repartidos en "Criterios de éxito". No elegí (a) porque los números de feature son consecutivos y no se reutilizan (`docs/process.md`), 002–004 ya están asignados y "001a/b/c" no cabe en el esquema de `id`; las fases dan la misma revisabilidad. Dependencias declaradas: A no depende de nada; B depende de A (el tipo de `protocol` se usa en la web y la forma local levanta la web); C depende de A y B (imágenes, healthchecks y filtros por ruta necesitan las dos apps).
2. **WebSocket de eco** → criterio nuevo en la fase B: un mensaje enviado al eco vuelve igual (test de integración en CI). Además, en "Restricciones": el eco no llega a producción aceptando mensajes arbitrarios (tu nota para la spec), y la pregunta de `Origin` sigue abierta para la spec.
3. **`release.yml`** → tres criterios nuevos en la fase C: `images` construye sin push en un PR; un tag de prueba publica `vX.Y.Z` y `sha-<7>` como manifiesto de imagen; con `DEPLOY_MODE=manual`, `deploy` espera la aprobación de `production` y no llama a Coolify (verificaciones 10 y 17). Ese último depende del environment `production`, así que lo até a la pregunta de configuración de GitHub.
4. **Seguridad** → `X-Client-Ip` agregado a la lista de headers del criterio de logs del server (fase B). La verificación 9 completa queda en un criterio propio de la fase C, con la parte de `secrets.*` salvo `GITHUB_TOKEN`.
5. **Design system** → los criterios sueltos de ADR-0009 se reemplazaron por uno solo en la fase A: "pasan los tests 1–6, 8 y 9 de ADR-0009". El test 7 quedó explícito en "Fuera de alcance". Conservé aparte el de idioma por cookie, que es de ADR-0007 y no de ADR-0009.
- **Logs de `web`** (aviso de privacidad, "Pendiente" punto 8) → criterio nuevo en la fase A: ni las peticiones ni `handleError` dejan en los logs de `web` la IP del cliente (`getClientAddress()`), `X-Forwarded-For`, cookies ni los headers sensibles de ADR-0008 D3. El resto del punto 8 (`tower-http::trace` y el plazo de los logs) quedó como pregunta abierta para la spec.
- **Preguntas del owner** → la configuración de GitHub y la preparación del VPS siguen en "Preguntas abiertas", con tu recomendación escrita y marcadas como "pendiente de confirmación del owner". No las resuelvo.
- **Riesgo** → `risk: high` para toda la feature, no solo para la fase C. Las fases A y B también fijan qué datos del cliente quedan en los logs, que es justo lo que promete el aviso de privacidad. La justificación está en "Restricciones": el security-reviewer revisa la spec y los tres PRs, y el owner da su OK explícito.
- **Notas para la spec** → las de redes externas y emulación ARM quedan para la spec. El criterio de la fase C, "la forma local levanta las imágenes de producción", las obliga a resolverse.

### architect — 2026-10-21 — APPROVE (ronda 2)
- Verifiqué en el cuerpo del intent, y no solo en la respuesta, que los cinco cambios quedaron incorporados:
  1. **Fases.** A, B y C están en "Alcance y fases", cada una con su PR y sus criterios propios en "Criterios de éxito". Las dependencias están declaradas y son correctas: A no depende de nada, B depende de A (el tipo de `protocol` se usa en la web y la forma local levanta la web) y C depende de A y B. Acepto la opción (b): `docs/process.md` dice que los números son consecutivos y no se reutilizan, y el esquema de `id` no admite "001a".
  2. **Criterios que faltaban.** El eco tiene su test de integración (fase B) y su restricción con ADR-0008 D6. `release.yml` tiene sus tres criterios (fase C: `images` sin push, manifiesto de imagen con `vX.Y.Z` y `sha-<7>`, y gate `DEPLOY_MODE=manual` sin llamadas a Coolify). `X-Client-Ip` está en la lista de headers de la fase B, que coincide con ADR-0008 D3. La verificación 9 de ADR-0006 aparece completa en la fase C. ADR-0009 queda cubierto con "tests 1–6, 8 y 9", que incluye las dos mitades de los tests 4 y 6 y los dos temas del test 8, y el test 7 aparece explícito en "Fuera de alcance".
  3. **Logs de `web`.** El criterio de la fase A se puede verificar en la CI y cubre las peticiones y `handleError`: IP de `getClientAddress()`, `X-Forwarded-For`, cookies y los headers de ADR-0008 D3. Lo que falta del punto 8 (`tower-http::trace` y `{{PLAZO_LOGS}}`) queda como pregunta para la spec.
  4. **`risk: high`.** Aplica a toda la feature. Está en el frontmatter y la justificación está en "Restricciones". Es coherente con `docs/process.md`: con riesgo alto hay pase del security-reviewer en la spec y en los tres PRs, y el owner da su OK explícito.
- También verifiqué: el frontmatter es válido; ningún ADR aprobado queda contradicho; todos los criterios se pueden verificar en local, en la CI o en GitHub sin el VPS; y las preguntas abiertas quedan explícitas. Las cuatro técnicas las resuelvo yo en la spec. Las dos del owner (configuración de GitHub y preparación del VPS) no bloquean el intent. Si el owner no acepta la configuración de GitHub como criterio de cierre, el criterio del gate de `deploy` de la fase C se ajusta antes de cerrar la fase C, como el propio intent lo dice.
- Llevo a "Áreas de preocupación" de la spec: el eco frente a ADR-0008 D6; las redes externas y la emulación `linux/amd64` en la forma local; y la frase "durante la feature 001" de ADR-0006, si el owner saca la preparación del VPS de 001.
