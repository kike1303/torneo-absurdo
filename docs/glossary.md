---
title: Glosario
type: glossary
status: approved
author: owner
created: 2026-10-20
updated: 2026-10-27
---

# Glosario

| Término | Código (en) | Definición |
|---|---|---|
| Pelea | `Fight` | Combate entre dos personajes, calculado por el motor a partir de una semilla. |
| Pelea rápida | `QuickFight` | Pelea programada cada X minutos. |
| Main event | `MainEvent` | Torneo diario a hora fija, con bracket. |
| Torneo | `Tournament` | Conjunto de peleas en bracket con un campeón. |
| Personaje homenaje | `Character` | Personaje original inspirado en un arquetipo reconocible, sin copiarlo. |
| Arquetipo | `Archetype` | La idea reconocible detrás de un personaje ("rata eléctrica"). |
| Semilla | `Seed` | 256 bits aleatorios que, junto con los personajes, determinan la pelea por completo. |
| Commit | `SeedCommit` | `SHA-256(seed)` publicado al abrir las apuestas. |
| Reveal | `SeedReveal` | Publicación de la semilla al terminar la pelea, para verificarla. |
| Log de eventos | `FightLog` | Secuencia ordenada de `FightEvent` con offset de tiempo; el renderizador la reproduce. |
| Ventana de apuestas | `BettingWindow` | Periodo en que se aceptan apuestas, antes de que empiece la pelea. |
| Pozo (parimutuel) | `Pool` | Suma de lo apostado por lado; las ganancias se reparten proporcionalmente entre los ganadores. |
| Comisión | `Rake` | % del pozo que se destruye como sumidero de monedas (puede ser 0). |
| Monedas | `Coins` | Moneda ficticia para apostar. No se compra ni se retira. |
| Bono diario | `DailyGrant` | Monedas gratis por entrar cada día. |
| Rescate | `Bailout` | Monedas gratis cuando el saldo está en (casi) cero. |
| Ledger | `Ledger` | Registro inmutable de movimientos de monedas; los saldos se derivan de él. |
| Temporada | `Season` | Periodo del ranking de apostadores; al terminar se reinicia el ranking (no las monedas). |
| Ganancia neta | `NetProfit` | Lo ganado en apuestas menos lo apostado; no incluye bonos ni rescates. |
| Ganancia clasificable | `RankedProfit` | La parte de `NetProfit` que cuenta para el leaderboard: las pérdidas cuentan completas y las ganancias solo acreditan la parte que sale de stakes perdedores de **apuestas hechas** fuera de carencia (cuenta la foto al apostar, no el estado actual de la cuenta). Nunca es mayor que `NetProfit` (ADR-0008 D8). No cambia el ledger ni los pagos. |
| Carencia | `Probation` | Periodo inicial de una cuenta (menos de 7 días o de 10 apuestas liquidadas, es decir, ganadas o perdidas; los reembolsos no cuentan): tope por pelea reducido, no sale en el leaderboard y sus stakes perdidos no dan `RankedProfit` a nadie (ADR-0008 D8). |
| Rating | `Rating` | Fuerza estimada de un personaje (Glicko-2). |
| Narrador | `Narrator` | Sistema de frases con plantillas que comenta la pelea según el contexto. |
| Repetición | `Replay` | Reproducir una pelea pasada a partir de su log o su semilla. |
| Timeline del renderer | `Timeline` | Lista de *cues* que reproduce `@torneo/renderer` (formato propio y versionado, `timeline_version`). No es el `FightLog`: un adaptador del lado de `apps/web` traduce uno a la otra (spec de 002). |
| Cue | `Cue` | Momento de la `Timeline` (`intro`, `strike`, `knockdown`, `ko`, `win` y, en la `timeline_version` 2, `time_up` y `draw`) con su `at_ms`; en un `strike`, `at_ms` es el instante del impacto. `time_up` marca el fin del tiempo, y lo cierra un `win` (decisión) o un `draw` (empate). |
| Manifest de sprites | `SpriteManifest` | `manifest.json` de un personaje: hojas PNG, frames con pivote y las animaciones estándar (`format: "torneo.sprites"`, `format_version`). |
