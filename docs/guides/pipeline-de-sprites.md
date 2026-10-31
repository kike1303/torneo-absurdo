---
id: "pipeline-de-sprites"
title: Receta del pipeline de sprites (de idea a set)
type: guide
status: approved
risk: medium
author: frontend-engineer
reviewers: [game-designer, architect]
approvals:
  - {by: game-designer, date: 2026-10-30}
  - {by: architect, date: 2026-10-30}
linear: TA-4
depends_on: ["004", "007"]
created: 2026-10-30
updated: 2026-10-30
---

# Receta del pipeline de sprites

> Decisión: ADR-0013 · Estándar: [guía de arte](sprites.md) · Herramienta: [`tools/sprite-importer`](../../tools/sprite-importer/README.md)

Lleva a un personaje de "idea" a un set en `content/sprites/<id>/`. No hace falta saber dibujar. Se puede seguir sin haber visto el personaje de prueba.

## 0. Antes de generar
- Lee el checklist "¿se parece demasiado?" de [`write-character`](../../.claude/skills/write-character/SKILL.md) y fija ahí silueta, paleta y vestimenta. Si el arquetipo tiene un referente obvio, aléjate a propósito (el ogro de prueba es gris y con moño, no verde).
- **Prompts limpios (ADR-0004):** ningún prompt nombra un personaje o una franquicia, y ningún material protegido sirve de referencia, img2img o calco.
- Abre `private/provenance/<id>.md` (nunca va a git) y anota cada prompt y cada referencia a medida que los uses. Guarda ahí una copia fechada de los términos vigentes del generador.
- Lee las `abilities` y el lore del personaje: los ataques salen de ahí. Si el `heavy` es un panzazo, la animación es un panzazo, no un puñetazo.
- El `id` del set usa guion bajo y el `slug` del personaje usa guion: `ogro_del_pantano` y `ogro-del-pantano`.
- **Gastar es del owner:** antes de generar, confirma el saldo con `get_balance` y pide su OK con el costo estimado (el personaje de prueba gastó 12 generaciones).

## 1. Generar (PixelLab)
Vista `side`, tamaño 64 y una sola dirección (`east`, que mira a la derecha como pide el estándar).

1. `create_character` con `mode: "v3"` y la descripción. Revisa el resultado antes de animar: lo demás hereda sus defectos.
2. `animate_character` con plantilla (1 generación cada una a 64 px):

| Acción | Plantilla |
|---|---|
| `idle` | `fight-stance-idle-8-frames` |
| `attack_light` | `lead-jab` |
| `attack_heavy` | `cross-punch` |
| `hit` | `taking-punch` |
| `ko` | `falling-back-death` |

3. `animate_character` con `mode: "v3"`, `directions: ["east"]` y `action_description` que describa solo el movimiento, para `special`, `block`, `dodge` y `win`. Si `attack_heavy` o `special` son golpes propios del personaje, también van por aquí: una plantilla como `cross-punch` llega al borde del lienzo de 64 px y puede cortar el puño.
4. Vuelve a generar lo que no se lea. `delete_animation` y otra llamada: cuesta otra generación.

Las animaciones `v3` salen en un lienzo mayor (88 px en la prueba) y con un cuerpo algo más robusto que las de plantilla. Es el defecto conocido de esta opción.

## 2. Frames crudos
Descarga los frames a `content/_local/raw/<id>/<animación>/<nn>.png` (ignorado por git). Guarda siempre una copia: el servicio no promete conservarlos. Esa copia va fuera de cualquier worktree desechable (el `content/_local/raw/` del checkout principal, por ejemplo).

## 3. Receta
Crea `tools/sprite-importer/recipes/<id>.json`, usando [`ogro_del_pantano.json`](../../tools/sprite-importer/recipes/ogro_del_pantano.json) como base:

- `scale`: entero. Con 2, un cuerpo de ~60 px queda en ~120 px dentro de la celda.
- `max_colors`: paleta compartida del set; usa la recomendación de la guía.
- `frames`: índices de los frames crudos. Se pueden saltar frames (un `block` que empieza en la pose neutra se lee tarde) y repetirlos.
- `frame_ms`, `loop`, `impact_frame`: los tiempos. Mira las tablas de la guía. `impact_frame` cuenta sobre la lista `frames` de la receta, no sobre los frames crudos.
- `ground`: `each` en `idle` (los pies quedan fijos sobre el pivote) y en `ko` (el cuerpo termina en el suelo).
- `offset`: corrección a mano del ancla, en píxeles de la celda.

## 4. Normalizar e importar
```sh
pnpm -s --filter @torneo/sprite-importer run normalize -- --in content/_local/raw/<id> --recipe tools/sprite-importer/recipes/<id>.json --out content/_local/export/<id>
pnpm -s --filter @torneo/sprite-importer run import -- --in content/_local/export/<id> --out content/_local/sprites/<id>
```
Si `import` emite códigos, ajusta la receta: la sección "Códigos" de la guía dice qué significa cada uno. Lo normal es corregir `frame_ms` (`guide.windup_*`) o `frames` (`guide.frames_count`).

## 5. Mirar
Con `just dev`, abre `/es/dev/arena` y elige el personaje en el grupo "Locales". Comprueba:

- Se lee a 375 px en los dos temas.
- Los tres ataques se distinguen entre sí.
- Cada ataque corresponde a su habilidad en `abilities` y en el lore: si el `heavy` es un panzazo, se ve un panzazo. Si no, vuelve al paso 1 (regenerar).
- Ningún frame queda cortado por el lienzo crudo, sobre todo el puño del frame de impacto. Eso no lo detecta `import` y se corrige regenerando.
- No hay saltos de cuerpo o de pies al cambiar de animación. Si los hay, vuelve al paso 3 (quitar frames) o al paso 1 (regenerar).

## 6. Commitear
Solo con todo esto: ADR-0013 aceptado, licencia aceptada por el owner (el OK expreso de ADR-0013 D6), su visto bueno visual, los ataques corregidos (cada uno corresponde a su habilidad y ninguno se corta, paso 5) y el checklist aprobado por game-designer o product-owner, distinto del autor, con el OK del owner. Si falta algo, el set se queda en `content/_local/`.

1. Quien aprueba el checklist revisa los prompts y las referencias en `private/provenance/<id>.md`. Si no se pueden reconstruir, el set se regenera o se queda en local.
2. `run import` con `--out content/sprites/<id>`.
3. Escribe `provenance.json` a mano con `origin: "original"` y `ai_assisted: true` (ADR-0013).
4. Agrega `"sprites": "<id>"` al `character.json` y corre `pnpm -s --filter @torneo/sprite-importer check`, `just sim --check` y `just check`.
5. Abre el PR. Su descripción declara que existe `private/provenance/<id>.md`, sin detalles, e incluye el checklist.

No van al repo los frames crudos ni el export: solo el set y la receta.

## Revisión
### game-designer — 2026-10-30 — REQUEST_CHANGES
- Verifiqué: los comandos de `normalize` e `import` contra `package.json` y `normalize-cli.ts`; la ruta `<animación>/<nn>.png` contra `normalize.ts`; `ground`, `offset` e `impact_frame` contra la receta del ogro; el grupo "Locales" de la arena (`arena_fighters_local`) y la sección "Códigos" de `sprites.md`. Los enlaces relativos apuntan bien y el frontmatter es válido.
- Cambios pedidos:
  1. **El paso 6 no corre `check`.** Las puertas ya coinciden con RF6 y ADR-0013 D6, pero RF6 exige que el PR pase `check` y la lista no lo pide. Agregarlo después de escribir `provenance.json`. Agregar también a esas puertas que quien aprueba el checklist revisa `private/provenance/<id>.md` (cambio pedido en la spec y en el ADR).
  2. **La tabla del paso 1 sigue recomendando `cross-punch`,** la plantilla que dio el corte del puño. El paso 5 ya pide mirarlo, pero quien siga la receta gastará en el mismo defecto: avisarlo junto a la tabla y decir con qué regenerar si se corta (`v3`, por ejemplo).
  3. **Faltan piezas de ADR-0013 D5.** El paso 0 no pide guardar la copia fechada de los términos en `private/provenance/<id>.md`. El paso 2 dice "guarda una copia" sin decir que sea fuera de un worktree desechable.
  4. **Los ataques no salen del personaje.** El paso 1 no dice que `attack_heavy` y `special` se elijan a partir de `abilities` y del lore, y por eso el "Panzazo de Lodo" del ogro quedó en puñetazo. Agregar: si la habilidad no es un golpe, usar `v3` con un `action_description` que describa ese movimiento, sin nombres protegidos.
- No bloquean: el paso 0 pide "el costo estimado" sin darlo; sirve citar las 12 generaciones del piloto. El título dice "de idea a export" y el texto llega hasta el set.

### game-designer — 2026-10-30 — APPROVE (re-revisión)
- Verifiqué mis cuatro cambios: (1) el paso 6 corre `check`, `just sim --check` y `just check`, y quien aprueba el checklist revisa `private/provenance/<id>.md`; (2) el paso 1 avisa que `cross-punch` puede cortar el puño y manda esos golpes a `v3`; (3) el paso 0 guarda la copia fechada de los términos y el paso 2 pone el resguardo fuera de un worktree desechable; (4) el paso 0 saca los ataques de `abilities` y del lore.
- El `status` queda `in-review`: falta la aprobación de architect.

### architect — 2026-10-30 — REQUEST_CHANGES
- Verifiqué: los comandos del paso 4 contra `package.json` y `checkOut`; la ruta `<animación>/<nn>.png`, `impact_frame`, `ground` y `offset` contra `normalize.ts` y la receta del ogro; lo que RF4 pide cubrir (checklist antes de generar, `id` frente a `slug`, frame cortado y ataques desde `abilities`) y ADR-0013 D5 y D6.
- Cambios pedidos:
  1. **La lista cerrada del paso 6 no coincide con RF6.** "Solo con todo esto" omite dos condiciones: que los ataques estén corregidos y que quien aprueba el checklist sea distinto del autor, con el OK del owner. Agregarlas.
  2. **El paso 5 no comprueba lo que ese gate exige.** Mira que los ataques se distingan y no se corten, pero no que cada uno corresponda a su habilidad (Área 8). Agregar esa comprobación.
- No bloquea: el título dice "de idea a export" y el texto llega hasta el set (ya lo señaló game-designer).

**Respuesta del autor (frontend-engineer, 2026-10-30):**
- (1) La lista del paso 6 suma los ataques corregidos y que quien aprueba el checklist sea distinto del autor, con el OK del owner, como RF6.
- (2) El paso 5 comprueba que cada ataque corresponda a su habilidad en `abilities` y en el lore, y manda a regenerar si no.
- No bloqueantes, también aplicados: el título ahora dice "de idea a set", y el paso 0 cita las 12 generaciones del personaje de prueba como referencia del costo.
- Vuelve a `in-review`. Estos cambios tocan los pasos 0, 5 y 6 después del `APPROVE` de game-designer, que puede querer mirarlos.

### architect — 2026-10-30 — APPROVE (re-revisión)
- Verifiqué mis cambios: (1) la lista del paso 6 coincide con RF6, con los ataques corregidos y un aprobador distinto del autor, con el OK del owner; (2) el paso 5 comprueba que cada ataque corresponda a su habilidad y manda a regenerar si no. El título ya dice "de idea a set".
- El `APPROVE` de game-designer es anterior a estos cambios en los pasos 0, 5 y 6. Lo conservo porque recogen lo que él mismo pidió y no cambian el alcance.
