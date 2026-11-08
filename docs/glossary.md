---
title: Glosario
type: glossary
status: approved
author: owner
created: 2026-10-20
updated: 2026-11-01
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
| Snapshot de la pelea | `FightSnapshot` | Los dos `canonical_spec_json` y la `ruleset_version` de una pelea, publicados al anunciarla, antes de que exista la semilla. Es lo único que alimenta al motor. |
| Estado de la pelea | `FightState` | `scheduled`, `betting_open`, `betting_closed`, `live` o `settled`. |
| Final de la pelea | `FightFinal` | Cómo terminó una pelea `settled`: exactamente uno entre `decided`, `cancelled` y `voided`. |
| Cancelada | `cancelled` | Final de una pelea que no llegó a abrir: sin semilla, sin log y sin apuestas. |
| Anulada | `voided` | Final manual de una pelea con commit que no se puede terminar: reembolsa y no tiene log. Revela la semilla, salvo con `secret_lost`, que no la tiene (012). |
| Anulada no verificable | `voided` con `secret_lost` | Anulada sin la semilla, que se perdió: reembolsa, y nadie puede comprobar el commit. La vista lo dice, y el contador público la cuenta (012 RF21 a RF23). |
| Parada segura | `fights.safe_stop` | Una pelea en `betting_closed` o `live` cuyo log no se puede reconstruir queda detenida, sin inventar un final. |
| Detención | `halted` | Mientras dura, no se abren ventanas ni se crean peleas nuevas, y las ventanas abiertas no aceptan apuestas. El server lo publica sin causa. |
| Retraso de reproducción | `playback_delay_ms` | Lo que el cliente reproduce por detrás del server (3500 ms): el margen para conocer el impacto y el cierre antes de mostrarlos. |
| Log de eventos | `FightLog` | Secuencia ordenada de `FightEvent` con offset de tiempo; el renderizador la reproduce. |
| Ventana de apuestas | `BettingWindow` | Periodo en que se aceptan apuestas, antes de que empiece la pelea. |
| Pozo (parimutuel) | `Pool` | Suma de lo apostado por lado; las ganancias se reparten proporcionalmente entre los ganadores. |
| Comisión | `Rake` | % del pozo, con tope en el lado perdedor, que se destruye como sumidero de monedas (puede ser 0). El residuo del redondeo también va al sumidero (012 RF10). |
| Pozo reembolsado | `refund_*` | Un pozo que devuelve a cada uno lo apostado: empate, un lado vacío al cierre, anulada o cancelada. No da `RankedProfit` ni cuenta para salir de carencia (012 RF9). |
| Monedas | `Coins` | Moneda ficticia para apostar. No se compra ni se retira. |
| Bono diario | `DailyGrant` | Monedas gratis por entrar cada día. |
| Rescate | `Bailout` | Monedas gratis cuando el saldo está en (casi) cero. |
| Ledger | `Ledger` | Registro inmutable de movimientos de monedas; los saldos se derivan de él. |
| Temporada | `Season` | Periodo del ranking de apostadores; al terminar se reinicia el ranking (no las monedas). |
| Ganancia neta | `NetProfit` | Lo ganado en apuestas menos lo apostado; no incluye bonos ni rescates. |
| Ganancia clasificable | `RankedProfit` | La parte de `NetProfit` que cuenta para el leaderboard: las pérdidas cuentan completas y las ganancias solo acreditan la parte que sale de stakes perdedores de **apuestas hechas** fuera de carencia (cuenta la foto al apostar, no el estado actual de la cuenta). Nunca es mayor que `NetProfit` (ADR-0008 D8). No cambia el ledger ni los pagos. |
| Carencia | `Probation` | Periodo inicial de una cuenta (menos de 7 días o de 10 peleas con una apuesta ganada o perdida; los reembolsos no cuentan): tope por pelea reducido, no sale en el leaderboard y sus stakes perdidos no dan `RankedProfit` a nadie (ADR-0008 D8). |
| Rating | `Rating` | Fuerza estimada de un personaje (Glicko-2). |
| Narrador | `Narrator` | Sistema de frases con plantillas que comenta la pelea según el contexto. |
| Repetición | `Replay` | Reproducir una pelea pasada a partir de su log o su semilla. |
| Timeline del renderer | `Timeline` | Lista de *cues* que reproduce `@torneo/renderer` (formato propio y versionado, `timeline_version`). No es el `FightLog`: un adaptador del lado de `apps/web` traduce uno a la otra (spec de 002). |
| Cue | `Cue` | Momento de la `Timeline` (`intro`, `strike`, `knockdown`, `ko`, `win` y, en la `timeline_version` 2, `time_up` y `draw`) con su `at_ms`; en un `strike`, `at_ms` es el instante del impacto. `time_up` marca el fin del tiempo, y lo cierra un `win` (decisión) o un `draw` (empate). |
| Set de sprites | `SpriteSet` | Carpeta `content/sprites/<id>/` con `manifest.json`, sus hojas y `provenance.json`; un personaje la enlaza con `"sprites": "<id>"` (feature 007). |
| Export | `SpriteExport` | Lo que entrega quien dibuja: `export.json` y un PNG por frame en una celda fija (`torneo.sprites.export`). El importador lo convierte en un set. |
| Procedencia | `Provenance` | `provenance.json` de un set: autor (identificador público), origen, fecha, si intervino IA y licencia, sin texto libre. Los datos personales van en `private/`. |
| Capa de guía | `validateGuide` | Reglas de arte (celda, tiempos, píxeles) sobre un manifest ya válido, con códigos `guide.*` (`@torneo/renderer/guide`). |
| Manifest de sprites | `SpriteManifest` | `manifest.json` de un personaje: hojas PNG, frames con pivote y las animaciones estándar (`format: "torneo.sprites"`, `format_version`). |
