---
id: "0002"
title: Semilla con commit-reveal
type: adr
status: approved
risk: high
author: owner
approvals:
  - by: owner
    date: 2026-10-20
created: 2026-10-20
updated: 2026-10-20
---

# ADR-0002: Semilla con commit-reveal

## Contexto
Con apuestas de por medio, el resultado no puede conocerse antes de que cierren las apuestas, y los usuarios deben poder comprobar que no hubo trampa.

## Decisión
1. Al abrir las apuestas, el servidor genera `seed` (32 bytes) con un CSPRNG del sistema operativo (`OsRng`/`getrandom`) y publica `commit = SHA-256(seed)`.
2. La semilla vive solo en el servidor, nunca en logs, respuestas del API ni mensajes WS, hasta `Settled`.
3. Al cerrar las apuestas se calcula el `FightLog`. Los eventos se emiten **solo cuando les toca** según el tiempo de la pelea; nunca se envía el log completo ni eventos futuros.
4. En `Settled` se publica la semilla. Cualquiera puede verificar `SHA-256(seed) == commit` y re-simular con `engine-wasm`.

## Alternativas descartadas
- Mezclar entropía aportada por los usuarios: es más complejo y no aporta mucho al inicio. Se puede revisar después.
- Enviar el log completo al cliente al empezar: cualquiera vería el final en DevTools.

## Consecuencias
- Hay tests que garantizan que ningún endpoint ni mensaje expone `seed` antes de `Settled`.
- La semilla se guarda en la base de datos (necesaria si el servidor se reinicia a mitad de una pelea), en una columna que el API nunca serializa mientras la pelea no esté liquidada.
