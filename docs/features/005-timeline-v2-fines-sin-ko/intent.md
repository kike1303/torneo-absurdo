---
id: "005"
title: "Timeline v2: fines sin KO"
type: intent
status: done
risk: medium
author: product-owner
reviewers: [architect]
approvals: [{by: architect, date: 2026-10-27}]
linear: TA-19
depends_on: ["002", "003"]
created: 2026-10-27
updated: 2026-10-27
---

# Intent: Timeline v2, fines sin KO

## Problema
La `Timeline` v1 del renderer (spec de 002, "Timeline v1" y RF-C4) solo sabe cerrar una pelea con un KO: exige exactamente un `ko` seguido de un `win`. El motor de 003, en cambio, también termina peleas **por tiempo**: emite `time_up` y un `end` con `reason: decision` (con ganador) o `reason: draw` (sin ganador). Ese final es parte del registro honesto y verificable de la pelea (preocupación 7 de la spec de 003), así que no se puede "forzar" un KO.

Hoy el adaptador `FightLog → Timeline` rechaza esos logs con `adapter.unsupported_ending` (enmienda técnica del 2026-10-27 a RF-C4 de 002). Esas peleas son raras (RF17 de 003: como mucho un 10 % de `time_up` y menos de un 1 % de empates), pero que alguna no se pueda reproducir basta para que no se pueda publicar ninguna. Por eso `docs/architecture.md` (trabajo derivado 6) deja esto como puerta: **ninguna feature publica peleas del motor a espectadores sin la v2.**

Para el espectador, una pelea que se queda sin final, o que termina en un error, rompe la confianza en todo el producto. Si además apostó, ni siquiera sabe si ganó.

## Resultado propuesto
Existe una `timeline_version` 2 que reproduce los tres finales posibles del motor y que cualquier espectador entiende sin leer números:
- **KO**, igual que hoy.
- **Decisión**: suena la campana del tiempo (cue de fin por tiempo), una pausa y la pose de victoria del ganador, sin que nadie caiga.
- **Empate**: la campana del tiempo y un cierre sin ganador, sin pose de victoria para ninguno.

La coreografía de los dos finales nuevos la diseña game-designer. Los textos que acompañan el cierre (por ejemplo "¡Tiempo!", "Decisión", "Empate") salen de Paraglide en es y en en. El validador acepta la v2 y sigue aceptando la v1, así que el guion de prueba y los fixtures existentes no cambian. Con eso, el adaptador traduce cualquier log válido del motor, y la primera feature que publique peleas deja de estar bloqueada.

## Usuarios y sistemas afectados
- Usuarios: el espectador y el apostador, cuando exista la página pública de pelea. Hoy, el owner y los agentes en la ruta de prueba `/[lang]/dev/arena`.
- Sistemas: `packages/renderer` (formato, validador, coreografía y estado final), el adaptador de `apps/web/src/lib/features/fight/` y los mensajes de i18n de `apps/web`.
- No toca: `crates/protocol`, `packages/protocol-ts` ni `crates/engine`. El formato del log es de 003 y se consume tal cual.

## Restricciones
- **Formato del motor congelado:** nada de cambios en `crates/protocol`, `packages/protocol-ts` ni `crates/engine`. Si la v2 necesita algo que el log no trae, va a "Preguntas abiertas", no a un cambio del motor.
- **ADR-0007:** el renderer y el adaptador manejan solo códigos. Todo texto visible sale de Paraglide (es y en) y pasa `check-i18n`.
- **ADR-0012:** sin ampliar la CSP.
- **ADR-0001, ADR-0002 y CLAUDE.md:** el renderer sigue sin `Math.random`, `Date` ni `performance`, y su azar cosmético sigue saliendo del PRNG local. Los dos finales nuevos se ven igual para todos los espectadores y en cada repetición.
- **ADR-0004:** sin assets de MUGEN ni de franquicias. Los placeholders del repo tienen que alcanzar para los finales nuevos.
- **Formato de assets:** sin animaciones obligatorias nuevas. Si la coreografía pide una (por ejemplo, una pose de empate), entra como opcional con fallback, para no encarecer cada personaje de 004 ni romper los placeholders.
- **Compatibilidad:** se mantiene la cota de `cueWindow` ya probada, y las reglas de pose de la v1 (≥ 1,5 s de KO a victoria y ≥ 2 s de pose) siguen valiendo para la v2.
- **Mobile-first:** el cierre se lee en 375 px y respeta `prefers-reduced-motion` como el resto del renderer.

## Criterios de éxito
- [ ] Los dorados `crates/engine/tests/golden/log-decision-a.json`, `log-decision-b.json` y `log-draw.json` se traducen con el adaptador sin errores y su `Timeline` pasa `validateTimeline`.
- [ ] La ruta de prueba `/[lang]/dev/arena` reproduce esos tres logs hasta el estado `finished`, en Chromium y en WebKit.
- [ ] En la decisión se ve la pose de victoria del ganador correcto. En el empate no hay pose de victoria y el estado final no tiene ganador.
- [ ] El validador sigue aceptando todas las `Timeline` v1 existentes (guion de RF-A8 y logs con KO) sin cambios en ellas.
- [ ] `adapter.unsupported_ending` deja de producirse para logs válidos del motor, y los logs incoherentes siguen rechazados con su código.
- [ ] Los textos del cierre existen en es y en, y ningún string literal visible queda fuera de Paraglide.
- [ ] game-designer aprueba la coreografía de decisión y empate en la spec.

## Fuera de alcance
- La página pública de pelea, en vivo o en repetición, y el WebSocket. Llegan con la primera feature que publique peleas.
- Cómo se liquidan las apuestas en un empate (reembolso con asientos del ledger, preocupación 9 de 003). Lo decide la spec de apuestas.
- El narrador (ADR-0010) y las frases para estos finales.
- El cue `taunt` y cualquier otro cue nuevo que no sea de fin de pelea.
- Cambios de balance o de `ruleset` para que haya más o menos peleas por tiempo.
- El go/no-go visual del owner (TA-20).

## Preguntas abiertas
- **¿Se explica la decisión?** ¿El cierre muestra por qué ganó (por ejemplo, el % de vida que le quedaba a cada uno) o basta con la pose del ganador y el texto "Decisión"? Las barras de vida ya lo sugieren; la spec, con game-designer, decide si alcanza.
- **¿Qué hacen los dos luchadores en un empate?** ¿Los dos en `idle`, una pose opcional nueva con fallback o alguna reacción compartida? Lo decide game-designer.
- **¿El adaptador emite siempre v2, o v1 para los KO y v2 solo para los fines por tiempo?** Es una decisión técnica de la spec. Desde producto solo importa que el KO se vea igual que hoy.
- **Duración en pantalla:** con `time_limit_ms = 86 000` y `end_ms = 4000`, ¿la pausa entre la campana y la victoria, más los ≥ 2 s de pose, caben dentro de `end.t`, o la reproducción se pasa de los 90 s del intent de 002? La spec lo comprueba con los dorados.
- **Logs en el arena:** hoy el arena empaqueta una sola copia (`log-ko-comeback.json`) porque el contexto de Docker de `apps/web` no incluye `crates/` (enmienda a RF-C2 y RF-C3 de 002). ¿Cómo llegan los tres dorados nuevos a la ruta de prueba sin romper esa regla?
- **¿Dónde se dibuja el texto del cierre?** ¿En el HUD de la página (Svelte, con Paraglide) o en el canvas a partir de un código que el host traduce? La spec lo decide sin meter texto en el renderer.

## Revisión
### architect — 2026-10-27 — APPROVE
- Verifiqué: frontmatter completo según `docs/process.md`; los tres dorados existen en `crates/engine/tests/golden/` y cierran con `end` en `t = 90000` (`reason: decision` con `winner` a/b y `reason: draw` con `winner: null`); `adapter.unsupported_ending` vive en `apps/web/src/lib/features/fight/timeline-from-fight-log.ts`; el trabajo derivado 6 de `docs/architecture.md` describe esta misma puerta (TA-19). El alcance queda en `packages/renderer` y `apps/web`, sin tocar `crates/protocol`, `packages/protocol-ts` ni `crates/engine`, así que respeta ADR-0001/0002 (el motor no cambia; el renderer sigue sin `Math.random`, `Date` ni `performance`), ADR-0007 (solo códigos; texto por Paraglide), ADR-0012 (sin ampliar la CSP) y ADR-0004 (placeholders del repo). Es viable con el stack y sin coste: todo es cliente, sin carga nueva en el VPS. Los criterios de éxito se pueden verificar con tests y e2e.
- Notas para la spec (no bloquean):
  1. Duración: `end.t = 90000` en los tres dorados (86 000 + 4000), así que el log ya cabe en los 90 s de 002. La spec tiene que fijar dónde caen la campana, la pausa y la pose dentro de esos 4000 ms y comprobarlo con los dorados.
  2. Reglas de pose: "≥ 1,5 s de KO a victoria" no tiene `ko` en la decisión. La spec define la regla equivalente (de `time_up` a la victoria) y cómo el validador la comprueba por versión.
  3. Versionado: recomiendo que el validador despache por `timeline_version` con reglas explícitas por versión, y que el adaptador emita siempre v2 si la v2 es un superconjunto estricto de la v1. Así hay un solo camino en producción y la v1 queda solo para los fixtures. Hay que decidirlo y justificarlo.
  4. `cueWindow`: los cues nuevos de cierre tienen que entrar en la cota ya probada (`MAX_ACTION_MS + RETURN_MS`), o la spec explica por qué no la cambian.
  5. Logs en el arena: los dorados se copian a `apps/web` igual que `log-ko-comeback.json`, con una prueba de frescura contra `crates/engine/tests/golden/`. Así no se rompe el contexto de Docker.
  6. Texto del cierre: prefiero el HUD de Svelte, a partir de un estado final con códigos que expone el player, antes que texto en el canvas. Así no se le pasan strings al renderer.
  7. Agregar `game-designer` a `reviewers` de la spec, porque un criterio de éxito depende de su aprobación.

