---
name: write-character
description: Create or edit a Torneo Absurdo homage character (character.json with stats, abilities and personality, bilingual lore with localized ability names) in content/characters/<slug>/ in the standard format, including the "too similar?" legal checklist and the balance check with `just sim --check`. Use whenever designing or changing a fighter.
---

# Escribir un personaje

El formato lo fija la spec del motor ([003, "Esquema de personaje" y RF12 a RF17](../../../docs/features/003-motor-de-pelea/spec.md)). Si la spec cambia, este skill se actualiza en el mismo PR.

## Estructura
```
content/characters/<slug>/
  character.json     # datos de juego: solo códigos y enteros, ningún texto para humanos
  lore.es.md         # nombre, epíteto, nombres de habilidades y lore en español
  lore.en.md         # lo mismo en inglés
  sprites/           # opcional, llega con la feature 004 (el renderer usa placeholders mientras tanto)
```
- El nombre del directorio **es** el `slug`. No hay symlinks: el cargador los rechaza.
- Solo se commitea arte original. Nada de sprites de MUGEN ni de franquicias: esos van en `content/_local/` (ignorado por git, ADR-0004).
- Cualquier otro archivo o directorio dentro del personaje se ignora (el cargador solo lee `character.json` y comprueba que existan los lore).

## character.json
JSON estricto: un campo desconocido, un tipo incorrecto o un valor fuera de rango hace fallar la carga (y `just check`). Los campos van en este orden:
```json
{
  "slug": "rata-electrica",
  "archetype": "electric-rodent",
  "ruleset_version": 1,
  "stats": { "hp": 55, "attack": 60, "defense": 45, "speed": 85, "special": 55 },
  "personality": { "aggression": 70, "caution": 20, "showmanship": 90 },
  "abilities": [
    { "id": "thunder_squeak", "kind": "special", "power": 70, "cooldown": 3, "element": "electric" }
  ],
  "weaknesses": ["ground"],
  "rivals": []
}
```
| Campo | Regla (ruleset v1) |
|---|---|
| `slug` | `^[a-z][a-z0-9-]{1,31}$`, igual al nombre del directorio. **Estable**: nunca cambia, porque las peleas guardadas lo referencian. |
| `archetype` | Mismo patrón. Describe el arquetipo (`electric-rodent`), nunca el nombre del personaje original. El motor lo ignora. |
| `ruleset_version` | `1` (la versión con que se carga). |
| `stats` | `hp`, `attack`, `defense`, `speed`, `special`: enteros de 1 a 100, con la suma ≤ 300. |
| `personality` | `aggression`, `caution`, `showmanship`: enteros de 0 a 100 (ya no hay pesos 0–1). |
| `abilities` | De 1 a 4. El golpe básico (`strike`) no se declara: lo tienen todos. |
| `abilities[].id` | `^[a-z][a-z0-9_]{1,31}$` (con guion bajo), único en el personaje y distinto de `strike`. Es un código, no un nombre. |
| `abilities[].kind` | `heavy` (`power` 20–60, `cooldown` 0–5) o `special` (`power` 40–100, `cooldown` 2–8). |
| `abilities[].element` | Opcional: `fire`, `water`, `electric`, `ground`, `air` o `ice`. |
| `weaknesses` | De 0 a 2 elementos distintos de esa lista. Un golpe de ese elemento hace ×5/4. |
| `rivals` | Slugs de otros personajes del roster, sin repetir y sin el propio. Para el narrador; el motor los ignora. |

Qué mueve cada número (detalle en la tabla del ruleset de la spec): el daño del golpe y de los `heavy` escala con `attack` del atacante contra `defense` del rival, y el de los `special` con `special` contra `special`; `speed` acorta el intervalo entre acciones y sube la esquiva; `defense` y `caution` suben el bloqueo; `special` sube además la probabilidad de crítico; `aggression` pesa el golpe y los `heavy`; `showmanship` pesa los `special` (y más con poca vida). `hp` da `500 + 4·hp` de vida máxima.

## lore.<locale>.md
Uno por idioma (`en` y `es`, ADR-0007). El motor y el log solo llevan códigos: los nombres que ve el usuario viven aquí.
```markdown
---
name: Chispa
epithet: La Rata que Muerde Rayos
locale: es
abilities:
  thunder_squeak: Chillido Trueno
---
<Lore de 80–150 palabras: origen, motivación, un rasgo ridículo y un rival.>

## Frases
- intro: "..."
- victoria: "..."
```
- `abilities` tiene **exactamente** los `id` de `character.json`, ni uno más ni uno menos. En 003 nadie lo valida automáticamente (lo hará la feature que muestre los nombres): lo comprueba el revisor en el PR.
- Se escribe **por idioma**, adaptando el humor; no es una traducción literal.
- El nombre y el epíteto no repiten el nombre original ni una variación obvia (ver el checklist).

## Checklist "¿se parece demasiado?" (ADR-0004), obligatorio
Lo **completa el autor** del personaje, uno por personaje, en la descripción del PR. Lo **aprueba un agente distinto del autor**: el product-owner por defecto (o el game-designer si el autor es otro), con un comentario firmado en el PR, por personaje, que incluye la comprobación de que los `abilities` de cada lore coinciden con los `id` del JSON. Como es un tema legal, además hace falta el OK del owner, que llega con el OK del merge.

- [ ] El nombre no es el original ni una variación obvia (nada de "Pikachú" ni "Gokú"), en ningún idioma.
- [ ] `slug` y `archetype` describen el arquetipo, no el personaje concreto.
- [ ] Sin frases icónicas literales ni ataques con su nombre original (tampoco en los `id`).
- [ ] Diseño visual distinto: silueta, paleta y vestimenta no son calcos (cuando haya sprites).
- [ ] Sin logos, símbolos ni marcas registradas.
- [ ] Se reconoce el **arquetipo**, no al personaje concreto.
- [ ] Los `abilities` de `lore.es.md` y `lore.en.md` tienen exactamente los `id` de `character.json`.

## Balance
`crates/sim` corre el motor real sobre `content/characters/` y mide el balance (RF15, RF17). Tras crear o editar un personaje:
```bash
just sim --check                     # 10 000 peleas, semilla fija; sale con 1 si falla un criterio
just sim --check --seed <64 hex>     # otra muestra, para no sobreajustar a una semilla
just sim --json                      # el mismo informe en JSON
just sim --content <dir> --fights N  # otro roster o tamaño de muestra
```
La semilla de `--seed` es la del simulador: **nunca** se le pasa la semilla de una pelea real.

Criterios (RF17), todos sobre los cruces entre personajes distintos:
- win rate global de cada personaje entre 40 % y 60 % (un empate cuenta medio);
- al menos 15 % de peleas con algún `moment`, y cada `kind` (`comeback`, `decisive_crit`, `clutch_ko`) en al menos 3 %;
- al menos 90 % de peleas con `end.t` entre 45 000 y 90 000 ms;
- ritmo (006 RF11), sobre las separaciones entre resoluciones consecutivas: media entre 1100 y 1300 ms (pausas incluidas); al menos 25 % de separaciones en ráfaga (≤ 900 ms); mediana de al menos 2 pausas (≥ 2500 ms) por pelea; al menos 90 % de peleas con alguna pausa; y separación mínima ≥ 600 ms (`TIMELINE_CONTRACT.strike_gap_ms`);
- `time_up` como mucho en el 10 % y `draw` en menos del 1 %;
- el lado `a` gana entre el 47 % y el 53 % de los espejos.

`speed` mueve la media de ritmo: un roster más rápido acorta las separaciones y uno más lento las alarga, así que un personaje nuevo, aunque esté equilibrado, tiene que pasar también los criterios de ritmo de `just sim --check`.

Un cruce fuera de 30–70 % es solo un **aviso**. El mismo test corre dentro de `just check` (`crates/sim/tests/balance.rs`), así que un personaje que rompe el balance no pasa CI. Los umbrales viven en `Criteria::RF17` (`crates/sim/src/report.rs`); qué se puede ajustar y con qué OK está en RF17 y en la decisión 16 de `plan-b.md`. Primero se ajusta el contenido; solo si un criterio global no se alcanza con contenido se proponen valores de `Ruleset` (decisión 14 de `plan-b.md`).
