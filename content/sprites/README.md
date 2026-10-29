# Sets de sprites

Cada set propio vive en `content/sprites/<id>/`, con el nombre de la carpeta igual al `id` de su manifest:

```
<id>/
  manifest.json     # formato torneo.sprites v1 (lo escribe el importador)
  sheet-0.png …     # las hojas que declara el manifest (las escribe el importador)
  provenance.json   # procedencia, a mano, sin datos personales
```

- **Qué va:** solo sets que salieron del importador (`tools/sprite-importer`) con su `provenance.json`. Un personaje los enlaza con `"sprites": "<id>"` en su `character.json`; varios personajes pueden compartir un set.
- **Qué no va:** nada de MUGEN ni de franquicias (esos viven en `content/_local/`, ignorado por git, [ADR-0004](../../docs/decisions/0004-personajes-homenaje.md)), ni fuentes de herramientas de dibujo, ni exports, ni `.DS_Store`. Cualquier archivo de más hace fallar `just check`. Este README es el único archivo suelto que se acepta aquí.
- **Procedencia:** `provenance.json` no tiene texto libre y `author` es un identificador público con consentimiento. Los datos que identifican al autor y la prueba de cesión de derechos viven en `private/provenance/<id>.md` (nunca en el repo); el PR que agrega un set declara que ese archivo existe, sin detalles.
- **Licencia:** todo lo de esta carpeta está bajo [`LICENSE-CONTENT.md`](../../LICENSE-CONTENT.md) (todos los derechos reservados), a diferencia de los placeholders `dummy_a` y `dummy_b` del renderer y del export de referencia `ref_a`, que son AGPL.
- **Cómo se hace un set:** la [guía de arte](../../docs/guides/sprites.md) (celda, tiempos, formato de export, importador y flujo hasta el PR).
