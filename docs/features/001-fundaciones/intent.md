---
id: "001"
title: Fundaciones del monorepo
type: intent
status: in-review
risk: medium
author: product-owner
reviewers: [architect]
approvals: []
linear: TA-1
depends_on: []
created: 2026-10-20
updated: 2026-10-20
---

# Intent: Fundaciones del monorepo

## Problema
El repo solo tiene documentación y carpetas vacías. Antes de construir cualquier feature necesitamos una base estandarizada: que cada app arranque, que haya i18n, theme, componentes base, tipos compartidos, calidad automatizada y un camino de despliegue. Si no, cada feature va a reinventar su parte.

## Resultado propuesto
Cualquier persona (o agente) clona el repo, corre un comando y tiene en local:
- La web SvelteKit en `es` y `en`, con dark/light, tokens de theme, primitivas de UI y la página `/dev/components`.
- El server Rust respondiendo `/api/health` y un WebSocket de eco.
- Tipos generados desde `crates/protocol` y usados en la web.
- Postgres local con migraciones.
- `just check` que valida formato, lint, tipos, tests y llaves de i18n.
- CI mínima y frugal, más un `docker-compose` listo para Coolify (sin desplegar todavía).

## Usuarios y sistemas afectados
- Usuarios: ninguno todavía (es base interna). Owner y agentes.
- Sistemas: todo el monorepo (`apps/*`, `crates/*`, `packages/*`, `infra/`, `.github/`).

## Restricciones
- ADR-0003 (stack), ADR-0006 (CI frugal, sin compilar en el VPS), ADR-0007 (i18n), ADR-0009 (design system, pendiente de aprobación).
- Sin features de producto: solo la base.

## Criterios de éxito
- [ ] `just dev` levanta web, server y Postgres.
- [ ] `just check` pasa en limpio y falla si falta una llave de i18n en algún idioma.
- [ ] Cambiar de idioma y de tema funciona y persiste.
- [ ] Un tipo definido en `crates/protocol` aparece en TS con `just gen` y se usa en la web.
- [ ] El workflow de CI en un PR de solo docs no corre los jobs de Rust ni de web.
- [ ] `infra/docker-compose.yml` levanta todo localmente con las imágenes de producción.
- [ ] Se completan los comandos en `CLAUDE.md`.

## Fuera de alcance
Auth (feature propia, tras aprobar ADR-0008), motor, renderizado, apuestas, identidad visual final.

## Preguntas abiertas
- ¿Repo público o privado? (afecta los minutos de CI, ADR-0006)
- ¿Generador de tipos: `ts-rs`, `specta` o `typeshare`?
- ¿Hook pre-push con `lefthook` o con un script simple?
