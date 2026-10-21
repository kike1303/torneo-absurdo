---
id: "0010"
title: Narrador con plantillas, sin LLM en runtime
type: adr
status: approved
risk: low
author: owner
approvals:
  - by: owner
    date: 2026-10-20
created: 2026-10-20
updated: 2026-10-20
---

# ADR-0010: Narrador con plantillas, sin LLM en runtime

## Decisión
- El narrador es un **sistema de frases con plantillas**, como en los juegos de deportes. El costo en runtime es cero.
- Cada frase tiene: `id`, `trigger` (tipo de evento: inicio, crítico, esquiva, HP bajo, remontada, KO…), `conditions` opcionales (rivalidad, racha ≥ N, sorpresa, favorito perdiendo, revancha), `weight` y texto con variables (`{attacker}`, `{streak}`, `{last_result}`).
- Selección: la frase **más específica** que cumpla las condiciones, ponderada por peso, sin repetir las últimas N.
- Las frases viven en `content/narrator/<locale>/*.yaml` y se escriben por idioma.
- **Las generamos con un LLM durante el desarrollo** (offline, en sesiones con Claude) y quedan versionadas como datos.
- El narrador corre en el cliente sobre los eventos (o en el servidor, si se quiere el mismo texto para todos; se decide en su spec), con un contexto histórico que el servidor manda al iniciar la pelea.

## Futuro
Voz con TTS local (por ejemplo Piper), pre-generada offline.
