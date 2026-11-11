---
id: "022"
title: Vestuario 3D (accesorios, trajes y recolor)
type: feature
status: approved
risk: low
author: claude
linear: TA-73
depends_on: ["013", "016"]
created: 2026-11-10
updated: 2026-11-10
---

# Vestuario 3D (accesorios, trajes y recolor)

## Qué y por qué
El renderer 3D dibuja la misma botarga para los dos lados y solo le cambia dos tintes. Para la botarga propia (021), las prendas (TA-66, TA-67) y la tienda de skins hace falta que cada luchador pueda llevar **accesorios** por ranura, un **traje** completo y **colores** elegidos, sin hacer un modelo nuevo por cada color. Esta feature le da eso al renderer y lo deja visible en `/dev/arena`. Los cinco accesorios ya modelados entran a `content/`.

## Decisiones del owner (chat, 2026-11-10)
- El recolor es **por zonas** del mismo modelo: nunca un GLB por color.
- Por ahora el torneo va con un mismo traje para todos (aún no existe), pero cualquier luchador debe poder ponerse cualquier traje y cambiarlo de color.
- Los gorros van calados, no posados encima.

## Alcance
- Entra: el `look` de cada lado en `@torneo/renderer/three` (traje, colores y accesorios), los accesorios `propeller_beanie`, `oven_mitts`, `bunny_slippers`, `sunday_sweatpants` y `pool_floatie_vest` en `content/models/accessories/`, y los parámetros de `/dev/arena` para probarlos.
- No entra: quién lleva qué en una pelea real (protocolo, server e inventario: 021, TA-66 y TA-67), la tienda, los trajes de los personajes del torneo como contenido del repo (el renderer ya los acepta; publicarlos es otra decisión) y los tipos de cuerpo por shape keys del backlog (el encaje medido por traje los reemplaza mientras alcance).

## Diseño

**Un `look` por lado.** `BotargaModel.looks?: { a?, b? }`, cada uno con:
- `skin?`: un traje. Es otro GLB con el mismo esqueleto (huesos `mixamorig*`) y los mismos nombres de clip; trae sus propias duraciones. Sin él, la botarga base. Lleva opcionalmente `fit` (medidas por ranura del cuerpo: escala y desplazamiento respecto a la base) y `zones` (las zonas de color de su textura).
- `colors?`: color por zona del traje (`{ z1: "#…" }`). La base sigue usando `tints`.
- `accessories?`: lista de accesorios, cada uno con su GLB, su `accessory.json` y sus colores por ranura de color (`primary`, `secondary`, `accent`).

**Accesorios.** Están modelados a escala de la botarga base, en T, con el origen en el hueso del que cuelgan. `accessory.json` dice la ranura del cuerpo (`head`, `hands`, `feet`, `hips`, `chest`), los huesos (`attach`, con `mirror` para el par derecho), el ajuste fino (`fit`) y si tapa piel (`hides`).
- *Rígidos* (gorra, guantes, pantuflas): se cuelgan del hueso con la matriz que los deja donde estarían en la pose de reposo, escalados y centrados por el `fit` del traje.
- *Deformables* (short, chaleco): traen huesos de relleno con los nombres de Mixamo; el renderer los vuelve a enlazar a los huesos del luchador por nombre.
- *Piel tapada*: con `hides`, los triángulos del cuerpo que quedan dentro de la prenda en reposo, y que siguen sus mismos huesos, no se dibujan. Así el cuerpo no asoma al moverse.
- Un nodo cuyo nombre termina en `Propeller` gira en función de `t` (presentación pura, sin estado).

**Colores.** Todo sale de los `extras` de los materiales del GLB, sin literales en el código:
- `tint_default` y `tint_slot`: el material toma el color de su ranura; `tint_shade` oscurece un forro.
- `tint_pattern: "primary,accent"`: una textura de dos colores (el cuadrillé) se recalcula entre los dos colores elegidos.
- Zonas (`color-zones.json` del traje): cada texel se desplaza según cuánto pertenece a cada zona, con una tabla RGB de 64³ que se llena solo en las celdas que la textura usa; conserva sombras y detalle. `follows`, `only` e islas (`zone-map.png`) fijan partes que comparten color con una zona pero no deben cambiar (lengua, interior de las orejas).
Las texturas recoloreadas son copias por luchador: los dos lados pueden llevar el mismo traje con colores distintos.

**Dónde vive.** La matemática sin Three (zonas, patrón, prueba de "dentro de la prenda") va en `packages/renderer/src/look/`, con tests. Lo que toca Three va en `src/three/` (`wardrobe.ts`, `materials.ts`, `load.ts`, `fighter.ts`). La web arma el `look` en `lib/components/fight/botarga-model.ts` leyendo `content/models/accessories/<id>/{<id>.glb, accessory.json}` y `content/models/skins/<slug>/{<slug>.glb, skin.json, zone-map.png}` con `import.meta.glob`; los manifiestos pasan por `parseAccessoryManifest` y `parseSkinManifest`. El formato de esas carpetas está en [la guía](../../guides/flujo-personajes-3d.md).

**`/dev/arena`.** `?wear_a=propeller_beanie,oven_mitts`, `?skin_b=<slug>`, `?colors_a=primary:ff0000,z1:00aa88` (y lo mismo con `_b`). Sin parámetros, nada cambia. Los ids se validan contra lo que existe; uno desconocido se ignora.

**Fallos.** Un accesorio que no carga no se lleva y la pelea se muestra igual; un traje que no carga falla la carga (`asset_load_failed`), como la base.

**Costos.** Sin `look`, cero trabajo extra. Los GLB de accesorios se bajan solo si alguien los lleva. El recolor y la máscara se calculan una vez al cargar, no por cuadro.

## Riesgos
- *Cruces de mallas* en poses extremas (brazos contra el chaleco): se aceptan; la máscara solo quita piel bajo la prenda.
- *Costo al cargar*: medido en escritorio, vestir a un luchador con los cinco accesorios cuesta unos 15 ms (la máscara va por rebanadas en Y) y recolorear un traje de textura grande unos 400 ms, una vez por luchador. Falta medirlo en un celular real; si pesa, el recolor puede pasar al shader.
- *Trajes de otro tamaño*: la separación entre luchadores está pensada para la base; dos trajes anchos se enciman más al golpear. Se ajusta cuando haya un traje publicado.
- *Compresión*: `gltfpack` cuantiza posiciones; el re-enlace de las prendas deformables tiene en cuenta esa transformación.

## Pruebas
- [ ] `just check` en verde
- [ ] Tests de `look/`: zonas (una zona se mueve y las demás no; islas; `follows`), patrón y prueba de "dentro"
- [ ] `/dev/arena` con los cinco accesorios y colores, en móvil y escritorio, dark y light
- [ ] `/dev/arena` con un traje local con zonas recoloreadas y accesorios encima
- [ ] Sin parámetros, `/dev/arena` se ve igual que antes
