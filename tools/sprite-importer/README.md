# @torneo/sprite-importer

Importador y chequeo de los sets de sprites propios ([007](../../docs/features/007-estandar-de-assets/spec.md), fase B). Es una herramienta de desarrollo: no es dependencia de `apps/web` y no entra en ninguna imagen (`tools/` está en `.dockerignore`).

Las reglas (celda, tiempos, formato de export, set y procedencia) y el diccionario de códigos están en la guía de arte, [`docs/guides/sprites.md`](../../docs/guides/sprites.md). Este README no las repite.

## Comandos
Desde la raíz del repo. `-s` evita que pnpm imprima la línea del comando, que lleva tus rutas.

```sh
# Export → set (solo content/_local/sprites/<id> o content/sprites/<id>, con <id> igual al del export).
pnpm -s --filter @torneo/sprite-importer import -- --in <carpeta-del-export> --out content/_local/sprites/<id>

# Chequeo de todos los sets de content/sprites/, su procedencia y los vínculos de los personajes (corre en just check).
pnpm -s --filter @torneo/sprite-importer check

# Regenera las tablas de la guía y el export de referencia (fixtures/export-ref, ref_a).
pnpm --filter @torneo/sprite-importer gen:guide
pnpm --filter @torneo/sprite-importer gen:fixture
```

- La salida es una línea JSON por problema, `{ "code", "file", "path", "data" }`, sin rutas absolutas ni texto. Sale con 0 si pasa (y una línea de resumen), con 1 si el contenido no cumple y con 2 ante un error de uso o de E/S.
- Las rutas relativas se resuelven contra el directorio desde el que llamas a `pnpm` (`INIT_CWD`).
