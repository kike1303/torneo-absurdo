---
id: "0007"
title: Internacionalización desde cero (en, es)
type: adr
status: approved
risk: medium
author: owner
approvals:
  - by: owner
    date: 2026-10-20
created: 2026-10-20
updated: 2026-10-20
---

# ADR-0007: Internacionalización desde cero (en, es)

## Decisión
- **Idiomas iniciales**: `es` y `en`. Fallback: `en`. Agregar un idioma no debe requerir tocar código, solo mensajes y contenido.
- **UI**: Paraglide JS (inlang), la integración oficial de SvelteKit. Mensajes compilados, con tipos y tree-shaking. Archivos en `apps/web/messages/{es,en}.json`.
- **Rutas con prefijo de idioma** (`/es/...`, `/en/...`). Detección por cookie, luego `Accept-Language`, luego fallback. Los usuarios con sesión pueden guardar su preferencia.
- **El dominio no tiene texto**: el motor, el protocolo y el API emiten **códigos** (`event: "crit_hit"`, `error: "bet.window_closed"`). La UI los traduce.
- **Contenido localizado**: el lore de los personajes y las frases del narrador se escriben **por idioma** (no se traducen literal, para que el humor funcione), en archivos separados por locale dentro de `content/`.
- **Formato**: fechas, números y monedas siempre con `Intl`, según el locale.
- **CI**: valida que todas las llaves existan en todos los idiomas y que el contenido tenga todos los locales requeridos.

## Consecuencias
- Prohibidos los strings literales visibles en componentes (lo revisa el pase de i18n en REVIEW.md, y se agrega una regla de lint si es viable).
- Los nombres de los personajes pueden variar por idioma.
