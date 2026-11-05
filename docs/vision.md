---
title: Visión del producto
type: vision
status: approved
author: owner
created: 2026-10-20
updated: 2026-11-04
---

# Torneo Absurdo — Visión

## Concepto
Una webapp (mobile-first) donde personajes "homenaje" pelean en batallas **simuladas por IA en el momento**: nadie las controla y nadie sabe el resultado de antemano. Todos los espectadores ven la misma pelea, sincronizada y animada, y apuestan **monedas ficticias** antes de cada combate.

Referente: SaltyBet (peleas de MUGEN con apuestas falsas en Twitch). Nos diferenciamos por el **lore persistente**, por ser una web nativa (sin video) y por el humor absurdo de los cruces.

Desde 2026-11-04 las peleas se dibujan en **3D**, con los personajes como **botargas** graciosas que comparten un mismo cuerpo base y se pueden vestir con prendas combinables. Más adelante, cada usuario tendrá su propia botarga para pelear contra otros jugadores. El detalle de esas ideas está en el [backlog](backlog.md).

## Pilares
1. **Incertidumbre real**: el motor corre al momento con una semilla secreta, verificable después (commit-reveal).
2. **Algo en juego**: apuestas parimutuel con monedas gratis diarias y un rescate cuando te quedas en cero.
3. **Ver juntos**: misma pelea para todos, en vivo, con un narrador que reacciona al contexto.
4. **Historia que importa**: rivalidades, rachas, rankings y consecuencias entre peleas.
5. **Absurdo con cariño**: arquetipos reconocibles ("la rata eléctrica", "el guerrero de pelo erizado") sin copiar a nadie.

## Formato de la programación
- **Peleas rápidas** cada X minutos (a definir, entre 10 y 15), siempre hay algo pasando.
- **Main event diario** a hora fija: torneo con bracket, más premio y más hype.
- **Repeticiones** de peleas pasadas, reproducibles desde su semilla.

## Público
Hispanohablantes primero (marca en español), con la UI bilingüe en/es desde el día uno. Después, un alias de marca en inglés.

## Alcance del MVP
- 6–8 personajes homenaje con lore corto y arte simple.
- Motor determinista más renderizado sincronizado en web.
- Peleas programadas y ventana de apuestas.
- Login con OAuth, monedas diarias, rescate y apuestas parimutuel.
- Leaderboard de apostadores (por temporada) y de personajes (rating).
- Repeticiones.
- i18n (en, es), theme definido y componentes estandarizados.

## Fuera de alcance (por ahora)
- Dinero real de cualquier tipo (ADR-0005).
- Personajes creados por usuarios en el MVP (la botarga propia llega después del torneo; ver el [backlog](backlog.md)).
- LLM en runtime: el narrador usa plantillas (ADR-0010).
- Apps nativas.

## Roadmap tentativo
- **Fase 0 — Fundaciones y spikes**: monorepo, renderizado (spike), motor v0, pipeline de assets.
- **Fase 1 — MVP jugable**: peleas en vivo, apuestas, auth, leaderboards, repeticiones.
- **Fase 1.5 — 3D**: piloto con Three.js y migración del reproductor a botargas 3D con escenarios.
- **Fase 2 — Espectáculo**: torneo diario, narrador, temporadas, chat.
- **Fase 3 — Juego propio**: botarga de cada usuario, niveles, PvP asíncrono con ranking, amigos y torneos privados, skins.

## Métricas de éxito (MVP)
- % de usuarios que regresan al día siguiente (D1) y a la semana (D7).
- Espectadores simultáneos promedio en el main event.
- Apuestas por usuario activo por día.

## Riesgos principales
- **Legal**: el contenido de franquicias se mitiga con personajes homenaje (ADR-0004), y las apuestas con dinero real, con la ADR-0005.
- **Arte**: costo y tiempo de los assets; mitigado con el spike 004-pipeline-de-assets.
- **Arranque en frío**: una web "en vivo" vacía se siente muerta; el diseño debe funcionar con pocos usuarios.
- **Recursos**: VPS de 1 vCPU y 4 GB, y presupuesto cero (ADR-0006).
