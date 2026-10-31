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
updated: 2026-10-31
---

# Receta del pipeline de sprites

> Decisión: [ADR-0013](../decisions/0013-pipeline-de-sprites.md) (`approved`) · Estándar: [guía de arte](sprites.md) · Herramienta: [`tools/sprite-importer`](../../tools/sprite-importer/README.md)

Lleva a un personaje de "idea" a un set en `content/sprites/<id>/`, o en `content/_local/` si no se publica ([Quedarse en local](#quedarse-en-local)). No hace falta saber dibujar. Se puede seguir sin haber visto el personaje de prueba.

## 0. Antes de generar
- Lee el checklist "¿se parece demasiado?" de [`write-character`](../../.claude/skills/write-character/SKILL.md) y fija ahí silueta, paleta y vestimenta. Si el arquetipo tiene un referente obvio, aléjate a propósito (el ogro de prueba es gris y con moño, no verde).
- **Prompts limpios (ADR-0004):** ningún prompt nombra un personaje o una franquicia, y ningún material protegido sirve de referencia, img2img o calco.
- Abre `private/provenance/<id>.md` (nunca va a git) y anota cada prompt y cada referencia a medida que los uses. Antes de generar, guarda junto a él una copia fechada de los términos vigentes del generador: una copia posterior no prueba qué términos regían.
- Lee las `abilities` y el lore del personaje: los ataques salen de ahí. Si el `heavy` es un panzazo, la animación es un panzazo, no un puñetazo.
- El `id` del set usa guion bajo y el `slug` del personaje usa guion: `ogro_del_pantano` y `ogro-del-pantano`.
- **Gastar es del owner:** antes de generar, confirma el saldo con `get_balance` y pide su OK con el costo. ESTIMADO de una corrida limpia: 11 generaciones (2 del personaje y 9 animaciones a 1), más las repeticiones. Sale del costo medido por llamada, porque ningún personaje se hizo así desde cero. El personaje de prueba consumió 19 en total, contando las animaciones de plantilla abandonadas y las repeticiones.

## 1. Generar (PixelLab)
Vista `side`, tamaño 64 y una sola dirección (`east`, que mira a la derecha como pide el estándar).

1. `create_character` con `mode: "v3"` y la descripción (2 generaciones). Revisa el resultado antes de animar: lo demás hereda sus defectos.
2. `animate_character` con `mode: "v3"`, `directions: ["east"]` y un `action_description` para cada una de las 9 acciones: `idle`, `attack_light`, `attack_heavy`, `special`, `hit`, `block`, `dodge`, `ko` y `win` (1 generación cada una).
   - Pide más frames de los que usará la receta, para poder saltar los neutros, sin perder de vista la columna "Frames" de la [guía de arte](sprites.md): la lista final de la receta tiene que caber en ese rango. En la prueba se pidieron de 4 a 8 por acción, y cada carpeta `v3` trae uno más, la pose de referencia.
   - El parámetro es `frame_count`: en `v3`, un número par de 4 a 16. `keep_first_frame` vale `true` por defecto y por eso sale la pose de referencia de más; con `false` se guardarían exactamente `frame_count` frames. La prueba usó el valor por defecto.
   - Describe solo el movimiento. Los tres ataques salen de `abilities` y del lore: si la habilidad no es un golpe, describe ese movimiento.
   - Se recomienda cerrar cada descripción con una cláusula negativa que excluya objetos: cuesta una frase. En la prueba, dos ataques salieron con un saco delante de la barriga. Sus repeticiones sumaron esa cláusula y también reescribieron el movimiento, así que no se sabe cuál de los dos cambios lo quitó. Adáptala si el personaje lleva un objeto propio (eso no se probó).
3. Lanza una animación a la vez: el trial solo admite un trabajo en curso. En la prueba, 8 llamadas tomaron unos 15 min en serie.
4. Vuelve a generar lo que no se lea. Cuesta otra generación. No hace falta borrar la animación anterior: guarda la nueva en otra carpeta y elígela con `source` (paso 3).

**Por qué `v3` en todas.** Medido contra `idle` en el personaje de prueba, que es el único caso:

| Animaciones | Diferencia de histograma |
|---|---|
| Plantillas mezcladas con `v3` | 19–25 % |
| Todas `v3` | 4.8–9.4 % |

Las dos opciones cuestan 1 generación por animación a 64 px, y mezclarlas se nota en el cuerpo al cambiar de animación. Además, la plantilla `cross-punch` cortó el puño en su lienzo de 64 px, y ningún frame que la receta de prueba elige toca el borde del lienzo `v3`. Por eso las plantillas de `animate_character` quedan descartadas.

**Qué esperar de la salida `v3`.** El lienzo sale de 88 px con tamaño 64. El frame 0 es la pose de referencia de pie, y a menudo le siguen algunos frames neutros: la receta los salta (paso 3).

## 2. Frames crudos
`get_character` lista cada animación con un patrón de URL que termina en `east/{i}.png?t=...` y el rango de `i` (`i=0..8`, por ejemplo). Descarga cada animación a su propia carpeta, `content/_local/raw/<id>/<animación>/` (ignorada por git), y guarda cada frame como `<nn>.png`. El `i` del servicio no lleva ceros y `<nn>` sí: `i=0` es `00.png`. `normalize` solo busca esa ruta y, si no la encuentra, emite `normalize.source_missing`. Guarda siempre una copia: el servicio no promete conservarlos. Esa copia va fuera de cualquier worktree desechable (el `content/_local/raw/` del checkout principal, por ejemplo).

## 3. Receta
Crea `tools/sprite-importer/recipes/<id>.json`, usando [`ogro_del_pantano.json`](../../tools/sprite-importer/recipes/ogro_del_pantano.json) como base:

- `scale`: entero. Con 2, un cuerpo de ~60 px queda en ~120 px dentro de la celda.
- `max_colors`: paleta compartida del set; usa la recomendación de la guía.
- `source`: carpeta de los crudos, si no se llama como la animación. Sirve para elegir una repetición (`attack_heavy_v2`, por ejemplo) sin borrar las anteriores. El nombre va en minúsculas, números y guion bajo, y empieza con letra.
- `frames`: índices de los frames crudos. Se pueden saltar frames y repetirlos. Con `v3`, empieza después de la pose de referencia y de los frames neutros: la receta de prueba empieza entre el 1 y el 4 según la animación.
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
- No aparecen objetos que el personaje no lleva. `import` tampoco lo detecta: se corrige regenerando (paso 1).
- No hay saltos de cuerpo o de pies al cambiar de animación. Si los hay, vuelve al paso 3 (quitar frames) o al paso 1 (regenerar).

## 6. Commitear
Solo con todo esto: ADR-0013 aceptado, licencia aceptada por el owner (el OK expreso de ADR-0013 D6), su visto bueno visual, los ataques corregidos (cada uno corresponde a su habilidad y ninguno se corta, paso 5) y el checklist aprobado por game-designer o product-owner, distinto del autor, con el OK del owner. Si falta algo, el set se queda en `content/_local/`.

1. Quien aprueba el checklist revisa los prompts y las referencias en `private/provenance/<id>.md`. Si no se pueden reconstruir, el set se regenera o se queda en local.
2. `run import` con `--out content/sprites/<id>`.
3. Escribe `provenance.json` a mano con `origin: "original"` y `ai_assisted: true` (ADR-0013).
4. Agrega `"sprites": "<id>"` al `character.json` y corre `pnpm -s --filter @torneo/sprite-importer check`, `just sim --check` y `just check`.
5. Abre el PR. Su descripción declara que existe `private/provenance/<id>.md`, sin detalles, e incluye el checklist.

No van al repo los frames crudos ni el export: solo el set y la receta.

## Quedarse en local
Un set puede quedarse en `content/_local/` aunque se vea bien, porque publicarlo es decisión del owner. El personaje de prueba se queda así por decisión suya: no es arte final y la licencia no queda aceptada para publicarlo.

- No se crean `content/sprites/<id>/` ni `provenance.json`, y `character.json` no lleva `sprites` (`check` fallaría con `link.unknown_set`).
- La receta sí puede ir al repo, porque solo tiene números, modos e identificadores cortos.
- El set se sigue viendo en la arena, en el grupo "Locales".
- `private/provenance/<id>.md` y el resguardo de los crudos y del export se conservan. Si después se publica, se cumple completo el paso 6, con un OK nuevo del owner sobre la licencia y los términos vigentes al generar todo lo que se publique. En el personaje de prueba, la copia guardada no cubre lo generado antes de ella: habría que regenerar esa parte o tener confirmación escrita del generador.

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

**Respuesta del autor (frontend-engineer, 2026-10-31):**
- Cambios por la regeneración del personaje de prueba: el paso 1 recomienda `v3` para las 9 animaciones, con los números medidos, y descarta las plantillas. También suma la cláusula negativa, el frame 0 y el trabajo único del trial. El paso 0 actualiza el costo (11 estimadas y 19 reales), el paso 3 explica `source` y el paso 5 busca objetos de más. La sección "Quedarse en local" es nueva. El encabezado cita ADR-0013 como `approved`. Las puertas del paso 6 no cambian.
- El `status` sigue en `approved` y las aprobaciones se conservan: `docs/process.md` y `review-doc` no piden volver a `in-review` cuando cambia un documento ya aprobado. Aun así, **estos cambios no están revisados** y las aprobaciones de arriba son anteriores a ellos: se pide re-revisión a game-designer y architect.

### game-designer — 2026-10-31 — REQUEST_CHANGES (re-revisión)
- Verifiqué con `git diff HEAD`:
  - **Cifras:** coinciden con la spec, ADR-0013 y mi checklist del 2026-10-31: 19–25 % y 4.8–9.4 %; 19 generaciones; 11 estimadas (2 + 9); 8 llamadas y unos 15 min (878 s). La receta empieza entre el frame 1 y el 4.
  - **Código:** `source` y su patrón contra `normalize.ts` y `limits.ts`; `<nn>` desde `00`; `link.unknown_set`; `character.json` sin `sprites`; `check` da `{"sets":0,"characters":4,"links":0}`.
  - **Prompts:** comparé la guía con `private/provenance/ogro_del_pantano.md` y no lleva texto de prompts del personaje de prueba. La cláusula negativa es una instrucción genérica.
  - **Puertas:** el paso 6 no cambia. "Quedarse en local" coincide con RF6, RF7 y ADR-0013 D5 y D6, y no agrega ninguna salida.
- Cambios pedidos (la guía todavía no se puede seguir sin haber visto la prueba):
  1. **Falta cuántos frames pedir (paso 1).** La guía dice qué acciones generar, pero no cuántos frames pedir en cada una. En la prueba se pidieron de 4 a 8, y cada carpeta `v3` trae uno más, la pose de referencia. Decirlo, remitir a la columna "Frames" de `sprites.md` y avisar que hay que pedir de más para poder saltar los frames neutros.
  2. **Falta cómo se obtienen los frames (paso 2).** "Descarga los frames" no dice con qué llamada ni cómo llegan a `<animación>/<nn>.png`. Agregar el paso, con el renombrado si hace falta.
  3. **"Agregarla los corrigió" no está demostrado (paso 1; también lo repite el paso 5).** Las dos repeticiones sumaron la cláusula y además reescribieron el movimiento, así que no se sabe cuál de los dos cambios quitó el saco. La cláusula probada habla de objetos, no de accesorios ni de manos, y las otras siete animaciones vigentes no la llevan. Dejarla como recomendación barata, sin afirmar la causa. La spec ya lo dice bien: "se corrigió con el prompt".
- Límites: no probé la receta generando un personaje nuevo. El alcance y el histograma vienen de mi checklist del 2026-10-31 y no los volví a medir. Tampoco comprobé el saldo de PixelLab.

### architect — 2026-10-31 — APPROVE (re-revisión de los cambios posteriores a la aprobación)
- Verifiqué, con `git diff` contra HEAD y sin compilar ni correr `just check`: `source` y su patrón contra `normalize.ts` y `LIMITS.idPattern`; `<nn>` desde `00` contra `pad`; en la receta, que los frames empiezan entre el 1 y el 4 y que suman 51. Leí los PNG crudos que elige la receta: todos existen, miden 88×88 y ninguno tiene píxeles opacos en el borde. "Quedarse en local" coincide con RF6, RF7 y ADR-0013 D5 y D6, y las puertas del paso 6 no cambiaron. Costos (11 estimadas, 19 reales), 19–25 % frente a 4.8–9.4 % y los 15 min coinciden con la fila 5 de la spec; no los volví a medir. No comprobé los parámetros de PixelLab ni el comportamiento del frame 0: vienen de la prueba.
- La decisión de dejar el set en local y los hechos del owner de 2026-10-31 no contradicen la guía.
- No bloquea, para su autor: (a) el paso 0 dice que la copia de los términos se guarda "ahí", en `private/provenance/<id>.md`, pero la real es un archivo aparte en `private/provenance/`; D5 dice "junto con", así que conviene escribir "junto a él". (b) "Quedarse en local" pide "la copia fechada" sin más, y para el personaje de prueba esa copia no cubre todo lo generado (ver la revisión del plan). El paso 0 ya da la regla correcta: copia antes de generar.

**Respuesta del autor (frontend-engineer, 2026-10-31, a game-designer y architect):**
- (1) El paso 1 dice cuántos frames pedir: de más, según la columna "Frames" de `sprites.md`, y cita los 4 a 8 de la prueba más la pose de referencia.
- (2) El paso 2 dice que los frames salen de `get_character` y cómo llegan a `<animación>/<nn>.png`, con el renombrado. Comprobé la ruta contra `normalize.ts` y los crudos. El coordinador verificó contra el servicio, el 2026-10-31, el patrón de URL de `get_character`, `frame_count` y `keep_first_frame`, que los pasos 1 y 2 ya citan; yo no los comprobé por mi cuenta.
- (3) La cláusula negativa queda como recomendación, sin afirmar la causa, en los pasos 1 y 5.
- No bloqueantes de architect: el paso 0 dice "junto a él" y "antes de generar", y "Quedarse en local" aclara qué no cubre la copia de la prueba.
- El `status` sigue en `approved`. Falta la re-revisión de game-designer sobre estos cambios.

### game-designer — 2026-10-31 — APPROVE (re-revisión de mis tres cambios)
- Verifiqué solo mis tres cambios, sin compilar ni correr nada:
  1. **Frames (paso 1):** los 4 a 8 por acción coinciden con el registro privado, y las carpetas `v3` que abrí (`block`, `dodge`, `win`, `special_v3b`, `hit_v3` y `attack_light_v3`) traen uno más. Las listas de la receta se miden contra la columna "Frames" de `sprites.md`.
  2. **Descarga (paso 2):** la ruta `<animación>/<nn>.png` coincide con los crudos y con `normalize.ts`, y `normalize.source_missing` existe en `codes.ts` y en `sprites.md`.
  3. **Cláusula negativa (pasos 1 y 5):** queda como recomendación y ya no afirma la causa. Tampoco agrega texto de prompts.
- Límites: que `get_character` dé una URL por frame viene del coordinador; ni el autor ni yo lo comprobamos contra el servicio, y nada más en el repo o en el registro privado lo confirma. La guía tampoco nombra el parámetro de cantidad de frames, así que quien la siga debe mirarlo en la herramienta. Si la llamada resulta distinta, hay que corregir el paso 2.
